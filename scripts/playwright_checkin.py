#!/usr/bin/env python3
"""
AnyRouter Playwright 签到子脚本。

由 Rust 主程序通过子进程调用，约定：
- 入参：从 stdin 读 JSON {"accounts": [...], "headless": bool, "timeout_ms": int}
- 账号字段：name, provider, domain, login_path, sign_in_path, user_info_path,
            api_user_key, api_user, cookies(dict|str|null), username?, password?
- 出参：向 stdout 写一行 JSON {"results": [...]}，每个账号一项
        每项含 success, before, after, user_info, error, used_login(bool)

设计原则：所有网络请求都走浏览器上下文（page.evaluate fetch），
让 Chromium 自身完成 TLS 握手与 WAF 通过，规避非浏览器 client 的指纹拒绝。
"""

from __future__ import annotations

import asyncio
import json
import os
import sys
import tempfile
import traceback
from dataclasses import dataclass, field
from typing import Any

# 确保 Windows 下 stdin/stdout/stderr 使用 UTF-8 编码
# stdin 同样需要重配，否则 Rust 端写入的 UTF-8 JSON 被按 GBK 解码，
# 中文字符变成代理对（如 \udcae），后续打印时报 UnicodeEncodeError。
if sys.platform == "win32":
    sys.stdin.reconfigure(encoding="utf-8", errors="replace")
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    os.environ.setdefault("PYTHONIOENCODING", "utf-8")

try:
    from playwright.async_api import async_playwright, BrowserContext, Page
except ImportError:
    print(json.dumps({
        "error": "playwright_not_installed",
        "message": "请先执行: pip3 install playwright && python3 -m playwright install chromium"
    }))
    sys.exit(2)


CHROME_UA = (
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36"
)


def log(msg: str) -> None:
    """日志写到 stderr，避免污染 stdout 的 JSON 结果。"""
    print(f"[playwright] {msg}", file=sys.stderr, flush=True)


@dataclass
class AccountInput:
    name: str
    provider: str
    domain: str
    login_path: str
    sign_in_path: str | None
    user_info_path: str
    api_user_key: str
    api_user: str
    cookies: Any = None
    username: str | None = None
    password: str | None = None
    # 详情页接口路径（fetch_detail 任务用，checkin 任务可忽略）
    tokens_path: str | None = None
    logs_path: str | None = None
    chart_path: str | None = None


@dataclass
class AccountResult:
    name: str
    success: bool = False
    before: dict | None = None
    after: dict | None = None
    user_info: dict | None = None
    error: str | None = None
    used_login: bool = False
    raw_sign_in: dict | None = field(default=None)


def parse_cookies(raw: Any) -> dict[str, str]:
    """支持 dict / "k1=v1; k2=v2" 字符串。"""
    if not raw:
        return {}
    if isinstance(raw, dict):
        return {str(k): str(v) for k, v in raw.items()}
    if isinstance(raw, str):
        out: dict[str, str] = {}
        for part in raw.split(";"):
            part = part.strip()
            if "=" in part:
                k, v = part.split("=", 1)
                out[k.strip()] = v.strip()
        return out
    return {}


def cookies_to_playwright(cookies: dict[str, str], domain_host: str) -> list[dict]:
    return [
        {
            "name": k,
            "value": v,
            "domain": domain_host,
            "path": "/",
            "httpOnly": False,
            "secure": True,
            "sameSite": "Lax",
        }
        for k, v in cookies.items()
    ]


def normalize_quota(raw_quota: float | int | None, raw_used: float | int | None) -> dict:
    quota = round((raw_quota or 0) / 500000.0, 2)
    used = round((raw_used or 0) / 500000.0, 2)
    return {"quota": quota, "used_quota": used, "raw_quota": raw_quota, "raw_used_quota": raw_used}


SENSITIVE_USER_INFO_KEY_PARTS = (
    "token",
    "secret",
    "password",
    "passwd",
    "cookie",
    "session",
    "authorization",
    "auth",
    "api_key",
)


def is_sensitive_user_info_key(key: str) -> bool:
    lowered = key.lower()
    return any(part in lowered for part in SENSITIVE_USER_INFO_KEY_PARTS)


def sanitize_user_info(raw: Any) -> dict | None:
    """保留用户信息里的可展示字段，避免把凭证类字段带到通知链路。"""
    if not isinstance(raw, dict):
        return None

    sanitized: dict[str, Any] = {}
    for key, value in raw.items():
        key_text = str(key)
        if is_sensitive_user_info_key(key_text):
            continue

        if value is None or isinstance(value, (str, int, float, bool)):
            sanitized[key_text] = value
        elif isinstance(value, dict):
            nested = {
                str(nested_key): nested_value
                for nested_key, nested_value in value.items()
                if not is_sensitive_user_info_key(str(nested_key))
                and (nested_value is None or isinstance(nested_value, (str, int, float, bool)))
            }
            if nested:
                sanitized[key_text] = nested

    return sanitized or None


async def fetch_in_page(page: Page, url: str, method: str, headers: dict[str, str], body: str | None = None) -> dict:
    """在页面 JS 上下文里执行 fetch，复用浏览器的 TLS 与 cookies。"""
    js = """
    async ({ url, method, headers, body }) => {
        try {
            const init = {
                method,
                headers,
                credentials: 'include',
                cache: 'no-store',
            };
            if (body !== null) init.body = body;
            const resp = await fetch(url, init);
            const text = await resp.text();
            return { ok: true, status: resp.status, body: text };
        } catch (err) {
            return { ok: false, error: String(err) };
        }
    }
    """
    return await page.evaluate(js, {"url": url, "method": method, "headers": headers, "body": body})


def build_api_headers(account: AccountInput) -> dict[str, str]:
    # 防御 api_user_key 为空的情况（用户编辑站点时可能误清空）
    api_user_key = account.api_user_key.strip() if account.api_user_key else ""
    if not api_user_key:
        api_user_key = "new-api-user"
    headers = {
        "Accept": "application/json, text/plain, */*",
        "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",
        "Content-Type": "application/json",
        "X-Requested-With": "XMLHttpRequest",
        api_user_key: account.api_user,
    }
    return headers


async def call_user_info(page: Page, account: AccountInput) -> tuple[bool, dict | None, dict | None, str | None]:
    url = f"{account.domain}{account.user_info_path}"
    headers = build_api_headers(account)
    log(f"[{account.name}] GET {url} ({account.api_user_key}={account.api_user})")
    resp = await fetch_in_page(page, url, "GET", headers, None)
    if not resp.get("ok"):
        return False, None, None, f"fetch failed: {resp.get('error')}"
    if resp.get("status") != 200:
        return False, None, None, f"HTTP {resp.get('status')}: {resp.get('body', '')[:200]}"
    try:
        data = json.loads(resp["body"])
    except json.JSONDecodeError:
        return False, None, None, f"invalid JSON: {resp['body'][:200]}"
    if not data.get("success"):
        return False, None, None, f"server returned success=false: {data.get('message') or data}"
    user_data = data.get("data") or {}
    return (
        True,
        normalize_quota(user_data.get("quota"), user_data.get("used_quota")),
        sanitize_user_info(user_data),
        None,
    )


async def call_sign_in(page: Page, account: AccountInput) -> tuple[bool, dict | None, str | None]:
    if not account.sign_in_path:
        return True, None, None  # auto check-in providers
    url = f"{account.domain}{account.sign_in_path}"
    headers = build_api_headers(account)
    log(f"[{account.name}] POST {url}")
    resp = await fetch_in_page(page, url, "POST", headers, "")
    if not resp.get("ok"):
        return False, None, f"fetch failed: {resp.get('error')}"
    if resp.get("status") != 200:
        return False, None, f"HTTP {resp.get('status')}: {resp.get('body', '')[:200]}"
    raw_body = resp["body"]
    try:
        data = json.loads(raw_body)
    except json.JSONDecodeError:
        if "success" in raw_body.lower():
            return True, {"raw": raw_body[:200]}, None
        return False, None, f"invalid JSON: {raw_body[:200]}"
    is_success = (
        data.get("ret") == 1
        or data.get("code") == 0
        or data.get("success") is True
    )
    if is_success:
        return True, data, None
    msg = (data.get("msg") or data.get("message") or "").lower()
    already_keywords = ["已经签到", "已签到", "重复签到", "already checked", "already signed"]
    if any(kw in msg for kw in already_keywords):
        return True, data, None
    return False, data, f"sign_in failed: {data.get('msg') or data.get('message')}"


async def perform_login(page: Page, account: AccountInput) -> tuple[bool, str | None]:
    """用账号密码 POST /api/user/login 取得 session cookie。登录成功后自动更新 account.api_user。"""
    if not (account.username and account.password):
        return False, "missing username/password"
    url = f"{account.domain}/api/user/login?turnstile="
    headers = {
        "Accept": "application/json, text/plain, */*",
        "Content-Type": "application/json",
        "X-Requested-With": "XMLHttpRequest",
    }
    body = json.dumps({"username": account.username, "password": account.password})
    log(f"[{account.name}] login as {account.username}")
    resp = await fetch_in_page(page, url, "POST", headers, body)
    if not resp.get("ok"):
        return False, f"login fetch failed: {resp.get('error')}"
    if resp.get("status") != 200:
        return False, f"login HTTP {resp.get('status')}: {resp.get('body', '')[:200]}"
    try:
        data = json.loads(resp["body"])
    except json.JSONDecodeError:
        return False, f"login invalid JSON: {resp['body'][:200]}"
    if not data.get("success"):
        return False, f"login rejected: {data.get('message') or data}"
    # 从登录响应中提取用户 ID，更新 api_user
    login_data = data.get("data") or {}
    if isinstance(login_data, dict):
        user_id = login_data.get("id") or login_data.get("user_id") or login_data.get("userId")
        if user_id:
            account.api_user = str(user_id)
            log(f"[{account.name}] updated api_user from login response: {account.api_user}")
    return True, None


async def process_account(context: BrowserContext, account: AccountInput, timeout_ms: int) -> AccountResult:
    result = AccountResult(name=account.name)
    page: Page | None = None
    try:
        from urllib.parse import urlparse
        host = urlparse(account.domain).hostname or ""

        cookies = parse_cookies(account.cookies)
        if cookies:
            await context.add_cookies(cookies_to_playwright(cookies, host))
            log(f"[{account.name}] injected {len(cookies)} cookie(s)")

        page = await context.new_page()
        page.set_default_timeout(timeout_ms)

        # 先访问首页让浏览器拿到 WAF cookies (acw_tc / acw_sc__v2 / cdn_sec_tc)
        nav_url = f"{account.domain}{account.login_path}"
        log(f"[{account.name}] goto {nav_url}")
        try:
            await page.goto(nav_url, wait_until="domcontentloaded")
        except Exception as e:
            log(f"[{account.name}] goto warning: {e}")
        # 等待页面完全加载（含 WAF JS challenge 完成）
        try:
            await page.wait_for_load_state("networkidle")
        except Exception:
            pass
        # 额外等待 WAF JS 验证完成并设置 cookie
        import time as _time
        await asyncio.sleep(3)

        # 如果没有 session cookie 但提供了账密，则尝试登录
        ctx_cookies = await context.cookies()
        has_session = any(c["name"] == "session" for c in ctx_cookies)
        if not has_session and account.username and account.password:
            ok, err = await perform_login(page, account)
            if ok:
                result.used_login = True
            else:
                log(f"[{account.name}] login failed: {err}")

        # 签到前余额
        ok, before, before_user_info, err = await call_user_info(page, account)
        if ok:
            result.before = before
            result.user_info = before_user_info
        else:
            log(f"[{account.name}] user_info(before) failed: {err}")

        # 签到（可选 manual）
        sign_ok, raw_sign, sign_err = await call_sign_in(page, account)
        result.raw_sign_in = raw_sign

        # 签到后余额
        ok2, after, after_user_info, err2 = await call_user_info(page, account)
        if ok2:
            result.after = after
            result.user_info = after_user_info or result.user_info

        if account.sign_in_path:
            result.success = sign_ok
            if not sign_ok:
                result.error = sign_err or err2 or "sign-in failed"
        else:
            # auto check-in: 只要 after 拿到就算成功
            result.success = ok2
            if not ok2:
                result.error = err2 or "auto check-in failed"
    except Exception as e:
        result.error = f"exception: {e.__class__.__name__}: {e}"
        log(f"[{account.name}] exception: {traceback.format_exc()}")
    finally:
        if page is not None:
            try:
                await page.close()
            except Exception:
                pass
    return result


async def run(payload: dict) -> dict:
    headless = bool(payload.get("headless", True))
    timeout_ms = int(payload.get("timeout_ms", 30000))
    raw_accounts = payload.get("accounts") or []
    accounts = [AccountInput(**a) for a in raw_accounts]

    log(f"starting playwright headless={headless} accounts={len(accounts)}")
    results: list[AccountResult] = []
    async with async_playwright() as p:
        with tempfile.TemporaryDirectory() as tmp_dir:
            context = await p.chromium.launch_persistent_context(
                user_data_dir=tmp_dir,
                headless=headless,
                user_agent=CHROME_UA,
                viewport={"width": 1280, "height": 800},
                ignore_https_errors=True,
                args=[
                    "--disable-blink-features=AutomationControlled",
                    "--disable-dev-shm-usage",
                    "--no-sandbox",
                    "--ignore-certificate-errors",
                ],
            )
            try:
                for acc in accounts:
                    log(f"--- processing {acc.name} ---")
                    res = await process_account(context, acc, timeout_ms)
                    results.append(res)
            finally:
                await context.close()
    return {"results": [r.__dict__ for r in results]}


def main() -> int:
    raw = sys.stdin.read()
    if not raw.strip():
        print(json.dumps({"error": "empty_stdin"}))
        return 2
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError as e:
        print(json.dumps({"error": "invalid_json", "message": str(e)}))
        return 2
    try:
        out = asyncio.run(run(payload))
    except Exception as e:
        log(traceback.format_exc())
        print(json.dumps({"error": "runtime", "message": str(e)}))
        return 1
    print(json.dumps(out, ensure_ascii=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
