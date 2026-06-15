#!/usr/bin/env python3
"""
AnyRouter Playwright 签到子脚本。

由 Rust 主程序通过子进程调用，约定：
- 入参：从 stdin 读 JSON {"accounts": [...], "headless": bool, "timeout_ms": int}
- 账号字段：name, provider, domain, login_path, sign_in_path, user_info_path,
            api_user_key, api_user, cookies(dict|str|null), username?, password?
- 出参：向 stdout 写一行 JSON {"results": [...]}，每个账号一项
        每项含 success, before, after, user_info, error, used_login(bool), cookie_report?

设计原则：所有网络请求都走浏览器上下文（page.evaluate fetch），
让 Chromium 自身完成 TLS 握手与 WAF 通过，规避非浏览器 client 的指纹拒绝。
"""

from __future__ import annotations

import asyncio    # 异步运行时，Playwright async API 依赖
import json       # JSON 解析与序列化
import os
import sys        # 系统标准输入输出
import tempfile   # 临时目录（浏览器 user_data_dir 用完即删）
import traceback  # 异常堆栈追踪
from datetime import datetime, timedelta, timezone
from dataclasses import dataclass, field  # 数据类装饰器
from email.utils import parsedate_to_datetime
from http.cookies import CookieError, SimpleCookie
from typing import Any  # 类型提示

# 尝试导入 Playwright 异步 API
# 如果未安装，直接输出错误 JSON 并退出（Rust 主进程会捕获并报告）
try:
    from playwright.async_api import async_playwright, BrowserContext, Page, Response
    from playwright._impl._errors import Error as PlaywrightError
except ImportError:
    print(json.dumps({
        "error": "playwright_not_installed",
        "message": "请先执行: pip3 install playwright && python3 -m playwright install chromium"
    }))
    sys.exit(2)


# 伪装的 Chrome User-Agent 字符串
# 用于让 WAF 检测识别为正常 Chrome 浏览器
CHROME_UA = (
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36"
)

TRANSIENT_PAGE_ERROR_MARKERS = (
    "Execution context was destroyed",
    "Cannot find context with specified id",
    "Frame was detached",
)


def log(msg: str) -> None:
    """日志写到 stderr，避免污染 stdout 的 JSON 结果。

    Rust 主进程设置子进程 stderr 为 inherit，
    因此这里的日志会直接输出到终端。
    """
    print(f"[playwright] {msg}", file=sys.stderr, flush=True)


def is_transient_page_error(error: Exception | str) -> bool:
    """判断是否为页面导航导致的短暂上下文错误。"""
    text = str(error)
    return any(marker in text for marker in TRANSIENT_PAGE_ERROR_MARKERS)


def default_browser_channel() -> str | None:
    """优先选择系统 Chrome，避免部分站点拒绝 bundled Chromium 的 TLS 指纹。"""
    if sys.platform == "darwin" and os.path.exists("/Applications/Google Chrome.app"):
        return "chrome"
    return None


def build_browser_launch_options(payload: dict, tmp_dir: str) -> dict:
    """构建浏览器启动参数。"""
    browser_channel = payload.get("browser_channel")
    if not isinstance(browser_channel, str) or not browser_channel.strip():
        browser_channel = default_browser_channel()
    else:
        browser_channel = browser_channel.strip()

    browser_executable_path = payload.get("browser_executable_path")
    if not isinstance(browser_executable_path, str) or not browser_executable_path.strip():
        browser_executable_path = None
    else:
        browser_executable_path = browser_executable_path.strip()

    options = {
        "user_data_dir": tmp_dir,
        "headless": bool(payload.get("headless", True)),
        "user_agent": CHROME_UA,
        "viewport": {"width": 1280, "height": 800},
        "args": [
            "--disable-blink-features=AutomationControlled",
            "--disable-dev-shm-usage",
            "--no-sandbox",
        ],
    }

    if browser_executable_path:
        options["executable_path"] = browser_executable_path
    elif browser_channel:
        options["channel"] = browser_channel

    return options


async def launch_browser_context(playwright, payload: dict, tmp_dir: str) -> BrowserContext:
    """启动浏览器上下文；优先真实 Chrome，失败时回退 bundled Chromium。"""
    options = build_browser_launch_options(payload, tmp_dir)
    channel = options.get("channel")
    executable_path = options.get("executable_path")

    if channel:
        log(f"launching browser via channel={channel}")
    elif executable_path:
        log(f"launching browser via executable_path={executable_path}")
    else:
        log("launching browser via bundled chromium")

    try:
        return await playwright.chromium.launch_persistent_context(**options)
    except Exception as err:
        if "channel" not in options and "executable_path" not in options:
            raise

        log(
            f"browser launch with system Chrome failed: {err}; falling back to bundled chromium"
        )
        fallback_options = dict(options)
        fallback_options.pop("channel", None)
        fallback_options.pop("executable_path", None)
        return await playwright.chromium.launch_persistent_context(**fallback_options)


async def wait_for_page_stability(page: Page, timeout_ms: int = 5000) -> None:
    """等待页面进入可执行 JS 的稳定状态。"""
    try:
        await page.wait_for_load_state("domcontentloaded", timeout=timeout_ms)
    except Exception:
        pass

    try:
        await page.wait_for_function(
            "() => document.readyState === 'interactive' || document.readyState === 'complete'",
            timeout=timeout_ms,
        )
    except Exception:
        pass


# ============================================================================
# 数据类定义
# ============================================================================

@dataclass
class AccountInput:
    """从 Rust 主进程传入的单个账号数据。

    与 Rust 端的 PlaywrightAccount 结构一一对应。
    """
    name: str                             # 账号显示名称
    provider: str                         # 所属 Provider 名称
    domain: str                           # 站点域名（含 https://）
    login_path: str                       # 登录页路径
    sign_in_path: str | None              # 签到 API 路径，None 表示自动签到
    user_info_path: str                   # 用户信息 API 路径
    api_user_key: str                     # 用户标识请求头键名
    api_user: str                         # 用户标识值
    cookies: Any = None                   # Cookie 数据（dict / 字符串 / None）
    username: str | None = None           # 登录用户名（可选，Cookie 失效时回退）
    password: str | None = None           # 登录密码（可选，Cookie 失效时回退）


@dataclass
class AccountResult:
    """单个账号的签到结果，最终序列化为 JSON 返回给 Rust 主进程。

    与 Rust 端的 PlaywrightResult 结构一一对应。
    """
    name: str                             # 账号显示名称
    success: bool = False                 # 签到是否成功
    before: dict | None = None            # 签到前余额信息 {quota, used_quota}
    after: dict | None = None             # 签到后余额信息 {quota, used_quota}
    user_info: dict | None = None         # 用户信息（脱敏后）
    error: str | None = None              # 错误信息
    used_login: bool = False              # 是否使用了账密登录
    cookie_report: dict | None = None     # 登录后的 Cookie 信息与过期分析
    raw_sign_in: dict | None = field(default=None)  # 签到 API 原始返回数据


# ============================================================================
# Cookie 解析与转换
# ============================================================================

def parse_cookies(raw: Any) -> dict[str, str]:
    """解析 Cookie 数据，支持多种格式。

    支持的输入格式：
    - dict: {"session": "xxx", "other": "yyy"} → 直接转为 dict[str, str]
    - str: "session=xxx; other=yyy" → 解析分号分隔的键值对
    - None/其他: 返回空 dict

    Args:
        raw: 原始 Cookie 数据，支持 dict、字符串或 None

    Returns:
        解析后的 Cookie 字典
    """
    if not raw:
        return {}
    if isinstance(raw, dict):
        # 直接将 dict 的 key/value 转为字符串
        parsed: dict[str, str] = {}
        for key, value in raw.items():
            cookie_name = str(key).strip()
            if not cookie_name or cookie_name.startswith("_"):
                continue
            cookie_value = str(value).strip()
            if not cookie_value:
                continue
            parsed[cookie_name] = cookie_value
        return parsed
    if isinstance(raw, str):
        # 解析 "key1=val1; key2=val2" 格式的 Cookie 字符串
        out: dict[str, str] = {}
        for part in raw.split(";"):
            part = part.strip()
            if "=" in part:
                k, v = part.split("=", 1)
                out[k.strip()] = v.strip()
        return out
    return {}


def cookies_to_playwright(cookies: dict[str, str], domain_host: str) -> list[dict]:
    """将 Cookie 字典转换为 Playwright 浏览器可接受的格式。

    Playwright 的 add_cookies 方法需要特定格式的 Cookie 对象列表。

    Args:
        cookies: 解析后的 Cookie 字典
        domain_host: 目标域名（不含协议和路径）

    Returns:
        Playwright 格式的 Cookie 列表
    """
    return [
        {
            "name": k,
            "value": v,
            "domain": domain_host,  # Cookie 所属域名
            "path": "/",            # Cookie 路径
            "httpOnly": False,       # 允许 JS 访问
            "secure": True,          # 仅 HTTPS 传输
            "sameSite": "Lax",       # SameSite 策略
        }
        for k, v in cookies.items()
    ]


def parse_cookie_header_names(header_value: str | None) -> list[str]:
    """从请求头里的 Cookie 字符串提取 Cookie 名称列表。"""
    if not header_value:
        return []

    names: list[str] = []
    for part in header_value.split(";"):
        item = part.strip()
        if "=" not in item:
            continue
        name, _value = item.split("=", 1)
        cookie_name = name.strip()
        if cookie_name and cookie_name not in names:
            names.append(cookie_name)
    return names


def format_duration(delta_seconds: float) -> str:
    """将秒数格式化为相对时长文本。"""
    absolute = int(abs(delta_seconds))
    days, rem = divmod(absolute, 86400)
    hours, rem = divmod(rem, 3600)
    minutes, seconds = divmod(rem, 60)

    parts: list[str] = []
    if days:
        parts.append(f"{days}d")
    if hours:
        parts.append(f"{hours}h")
    if minutes:
        parts.append(f"{minutes}m")
    if not parts:
        parts.append(f"{seconds}s")

    text = " ".join(parts[:3])
    return f"in {text}" if delta_seconds >= 0 else f"expired {text} ago"


def isoformat_utc(moment: datetime) -> str:
    """输出 UTC ISO-8601 时间字符串。"""
    return moment.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def analyze_expiry(
    *,
    expires_dt: datetime | None = None,
    max_age_seconds: int | None = None,
    now: datetime | None = None,
) -> tuple[str | None, str | None, bool]:
    """分析 Cookie 的过期时间。"""
    now = now or datetime.now(timezone.utc)

    effective_expiry = expires_dt
    if effective_expiry is None and max_age_seconds is not None:
        effective_expiry = now + timedelta(seconds=max_age_seconds)

    if effective_expiry is None:
        return None, None, True

    if effective_expiry.tzinfo is None:
        effective_expiry = effective_expiry.replace(tzinfo=timezone.utc)

    expires_in = format_duration((effective_expiry - now).total_seconds())
    return isoformat_utc(effective_expiry), expires_in, False


def parse_set_cookie_headers(header_values: list[str]) -> dict[str, dict]:
    """解析响应头中的 Set-Cookie，并提取过期信息。"""
    parsed: dict[str, dict] = {}
    now = datetime.now(timezone.utc)

    for raw_header in header_values:
        cookie = SimpleCookie()
        try:
            cookie.load(raw_header)
        except CookieError:
            cookie = SimpleCookie()

        if not cookie:
            first_part = raw_header.split(";", 1)[0].strip()
            if "=" not in first_part:
                continue
            name, _value = first_part.split("=", 1)
            cookie_name = name.strip()
            if cookie_name:
                parsed.setdefault(
                    cookie_name,
                    {
                        "name": cookie_name,
                        "source": "set-cookie",
                        "is_session": True,
                    },
                )
            continue

        for morsel in cookie.values():
            max_age_raw = morsel["max-age"].strip() if morsel["max-age"] else ""
            max_age_seconds = None
            if max_age_raw:
                try:
                    max_age_seconds = int(max_age_raw)
                except ValueError:
                    max_age_seconds = None

            expires_dt = None
            expires_raw = morsel["expires"].strip() if morsel["expires"] else ""
            if expires_raw:
                try:
                    expires_dt = parsedate_to_datetime(expires_raw)
                except (TypeError, ValueError, IndexError):
                    expires_dt = None

            expires_at, expires_in, is_session = analyze_expiry(
                expires_dt=expires_dt,
                max_age_seconds=max_age_seconds,
                now=now,
            )

            parsed[morsel.key] = {
                "name": morsel.key,
                "domain": morsel["domain"] or None,
                "path": morsel["path"] or None,
                "source": "set-cookie",
                "expires_at": expires_at,
                "expires_in": expires_in,
                "max_age_seconds": max_age_seconds,
                "same_site": morsel["samesite"] or None,
                "secure": bool(morsel["secure"]),
                "http_only": bool(morsel["httponly"]),
                "is_session": is_session,
            }

    return parsed


def merge_cookie_details(primary: dict, secondary: dict) -> dict:
    """合并两份 Cookie 元数据，优先保留 primary 的非空字段。"""
    merged = dict(secondary)

    for key, value in primary.items():
        if value in (None, "", []):
            continue
        if key == "source" and merged.get("source") and merged["source"] != value:
            merged["source"] = f"{value}+{merged['source']}"
            continue
        merged[key] = value

    return merged


def cookie_from_browser_context(raw_cookie: dict) -> dict:
    """将 Playwright context.cookies() 的结果转为可序列化分析信息。"""
    expires_value = raw_cookie.get("expires")
    expires_at = None
    expires_in = None
    is_session = True

    if expires_value not in (None, -1, 0):
        try:
            expires_dt = datetime.fromtimestamp(float(expires_value), tz=timezone.utc)
            expires_at, expires_in, is_session = analyze_expiry(expires_dt=expires_dt)
        except (TypeError, ValueError, OSError):
            expires_at = None
            expires_in = None
            is_session = True

    return {
        "name": raw_cookie.get("name"),
        "domain": raw_cookie.get("domain"),
        "path": raw_cookie.get("path"),
        "source": "browser-context",
        "expires_at": expires_at,
        "expires_in": expires_in,
        "same_site": raw_cookie.get("sameSite"),
        "secure": bool(raw_cookie.get("secure")),
        "http_only": bool(raw_cookie.get("httpOnly")),
        "is_session": is_session,
    }


async def build_cookie_report(
    context: BrowserContext,
    account: AccountInput,
    request_cookie_header: str | None,
    set_cookie_headers: list[str],
) -> dict | None:
    """组合登录请求/响应与浏览器上下文中的 Cookie 信息。"""
    request_cookie_names = parse_cookie_header_names(request_cookie_header)
    response_cookie_details = parse_set_cookie_headers(set_cookie_headers)
    response_cookie_names = sorted(response_cookie_details.keys())

    context_cookies = await context.cookies([account.domain])
    interesting_names = set(response_cookie_names)
    if "session" not in interesting_names:
        interesting_names.add("session")

    merged_cookies: dict[str, dict] = dict(response_cookie_details)
    for raw_cookie in context_cookies:
        name = raw_cookie.get("name")
        if not name:
            continue
        if interesting_names and name not in interesting_names:
            continue

        browser_details = cookie_from_browser_context(raw_cookie)
        if name in merged_cookies:
            merged_cookies[name] = merge_cookie_details(browser_details, merged_cookies[name])
        else:
            merged_cookies[name] = browser_details

    if not request_cookie_names and not response_cookie_names and not merged_cookies:
        return None

    note = None
    if response_cookie_names and not merged_cookies:
        note = "Set-Cookie was present in the login response, but the browser context did not retain matching cookies."
    elif merged_cookies and not response_cookie_names:
        note = "No Set-Cookie header was captured; expiry was inferred from the browser context cookies."

    return {
        "request_cookie_names": request_cookie_names,
        "response_cookie_names": response_cookie_names,
        "cookies": [merged_cookies[name] for name in sorted(merged_cookies.keys())],
        "note": note,
    }


# ============================================================================
# 余额与用户信息处理
# ============================================================================

def normalize_quota(raw_quota: float | int | None, raw_used: float | int | None) -> dict:
    """将 API 返回的原始额度值转换为美元单位。

    原始值 / 500000 = 美元金额（站点内部单位转换规则）

    Args:
        raw_quota: 原始余额值（API 返回的整数值）
        raw_used: 原始消耗值（API 返回的整数值）

    Returns:
        包含转换后和原始值的字典
        {
            "quota": 5.10,         # 余额（美元）
            "used_quota": 1.20,    # 累计消耗（美元）
            "raw_quota": 2550000,  # 原始余额值
            "raw_used_quota": 600000  # 原始消耗值
        }
    """
    quota = round((raw_quota or 0) / 500000.0, 2)
    used = round((raw_used or 0) / 500000.0, 2)
    return {"quota": quota, "used_quota": used, "raw_quota": raw_quota, "raw_used_quota": raw_used}


# 敏感字段的子串匹配列表
# 包含这些子串的 key 将被从用户信息中过滤掉
SENSITIVE_USER_INFO_KEY_PARTS = (
    "token",         # 认证令牌
    "secret",        # 密钥
    "password",      # 密码
    "passwd",        # 密码（缩写）
    "cookie",        # Cookie
    "session",       # 会话标识
    "authorization", # 授权头
    "auth",          # 认证相关
    "api_key",       # API 密钥
)


def is_sensitive_user_info_key(key: str) -> bool:
    """判断用户信息的某个 key 是否为敏感字段。

    通过检查 key 中是否包含敏感子串来判断。

    Args:
        key: 用户信息字段名

    Returns:
        True 表示是敏感字段，应从展示中过滤
    """
    lowered = key.lower()
    return any(part in lowered for part in SENSITIVE_USER_INFO_KEY_PARTS)


def sanitize_user_info(raw: Any) -> dict | None:
    """脱敏用户信息，移除凭证类字段。

    保留可展示的字段（如 id、username、email），
    移除敏感字段（如 token、secret、password 等）。

    对于嵌套对象，递归过滤敏感子字段，只保留基本类型的值。

    Args:
        raw: /api/user/self 返回的 data 字段

    Returns:
        脱敏后的用户信息字典，如果无有效字段则返回 None
    """
    if not isinstance(raw, dict):
        return None

    sanitized: dict[str, Any] = {}
    for key, value in raw.items():
        key_text = str(key)
        # 跳过敏感字段
        if is_sensitive_user_info_key(key_text):
            continue

        if value is None or isinstance(value, (str, int, float, bool)):
            # 基本类型：直接保留
            sanitized[key_text] = value
        elif isinstance(value, dict):
            # 嵌套对象：过滤敏感子字段，只保留基本类型的值
            nested = {
                str(nested_key): nested_value
                for nested_key, nested_value in value.items()
                if not is_sensitive_user_info_key(str(nested_key))
                and (nested_value is None or isinstance(nested_value, (str, int, float, bool)))
            }
            if nested:
                sanitized[key_text] = nested

    return sanitized or None


# ============================================================================
# 浏览器内网络请求
# ============================================================================

async def fetch_in_page(page: Page, url: str, method: str, headers: dict[str, str], body: str | None = None) -> dict:
    """在浏览器 JS 上下文里执行 fetch 请求。

    这是整个脚本的核心设计：所有 API 请求都不走 Python HTTP 客户端，
    而是通过 page.evaluate 在 Chromium 的 JS 环境中执行 fetch。
    这样做的目的是：
    1. 复用浏览器的 TLS 握手，避免指纹被识别为非浏览器客户端
    2. 自动携带浏览器 Cookie（通过 credentials: 'include'）
    3. 通过 WAF 检测（如 acw_tc / acw_sc__v2 等反爬机制）

    Args:
        page: Playwright Page 对象
        url: 请求 URL
        method: HTTP 方法（GET / POST）
        headers: 请求头字典
        body: 请求体（POST 时使用）

    Returns:
        请求结果字典：
        - 成功: {"ok": True, "status": 200, "body": "..."}
        - 失败: {"ok": False, "error": "..."}
    """
    # 在浏览器 JS 上下文中执行的 fetch 代码
    js = """
    async ({ url, method, headers, body }) => {
        try {
            const init = {
                method,
                headers,
                credentials: 'include',  // 自动携带浏览器 Cookie
                cache: 'no-store',        // 禁用缓存，确保获取最新数据
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

    last_error = None
    for attempt in range(3):
        try:
            await wait_for_page_stability(page)
            return await page.evaluate(
                js,
                {"url": url, "method": method, "headers": headers, "body": body},
            )
        except PlaywrightError as err:
            last_error = err
            if not is_transient_page_error(err) or attempt == 2:
                break

            log(
                f"page context reset during {method} {url}; waiting for navigation to settle and retrying ({attempt + 1}/3)"
            )
            await page.wait_for_timeout(250 * (attempt + 1))

    return {"ok": False, "error": str(last_error) if last_error else "unknown page evaluate error"}


def build_api_headers(account: AccountInput) -> dict[str, str]:
    """构建 API 请求头。

    所有 API 请求共用此请求头，包含用户标识和浏览器伪装头。

    Args:
        account: 账号输入数据

    Returns:
        请求头字典
    """
    return {
        "Accept": "application/json, text/plain, */*",       # 接受 JSON 响应
        "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",       # 中文浏览器语言
        "Content-Type": "application/json",                   # 请求体为 JSON
        "X-Requested-With": "XMLHttpRequest",                 # 标识为 AJAX 请求
        account.api_user_key: account.api_user,               # 用户标识（如 new-api-user: xxx）
    }


# ============================================================================
# API 调用函数
# ============================================================================

async def call_user_info(page: Page, account: AccountInput) -> tuple[bool, dict | None, dict | None, str | None]:
    """调用 /api/user/self 接口获取用户信息和余额。

    用于签到前后分别查询，以计算签到奖励和余额变化。

    Args:
        page: Playwright Page 对象
        account: 账号输入数据

    Returns:
        (成功标志, 余额信息, 脱敏用户信息, 错误信息)
        - 成功标志: True/False
        - 余额信息: {"quota": float, "used_quota": float, ...}
        - 脱敏用户信息: 过滤敏感字段后的用户数据
        - 错误信息: 失败时的错误描述
    """
    url = f"{account.domain}{account.user_info_path}"
    headers = build_api_headers(account)
    log(f"[{account.name}] GET {url}")

    # 在浏览器上下文中发起 GET 请求
    resp = await fetch_in_page(page, url, "GET", headers, None)

    # 请求失败（网络错误等）
    if not resp.get("ok"):
        return False, None, None, f"fetch failed: {resp.get('error')}"
    # HTTP 状态码非 200
    if resp.get("status") != 200:
        return False, None, None, f"HTTP {resp.get('status')}: {resp.get('body', '')[:200]}"
    # 解析 JSON 响应
    try:
        data = json.loads(resp["body"])
    except json.JSONDecodeError:
        return False, None, None, f"invalid JSON: {resp['body'][:200]}"
    # 检查服务端返回的 success 字段
    if not data.get("success"):
        return False, None, None, f"server returned success=false: {data.get('message') or data}"

    # 提取用户数据
    user_data = data.get("data") or {}
    return (
        True,
        normalize_quota(user_data.get("quota"), user_data.get("used_quota")),  # 余额信息
        sanitize_user_info(user_data),  # 脱敏用户信息
        None,  # 无错误
    )


async def call_sign_in(page: Page, account: AccountInput) -> tuple[bool, dict | None, str | None]:
    """调用签到 API。

    对于 sign_in_path 为 None 的 Provider（如 agentrouter），访问即自动签到，
    无需调用此接口，直接返回成功。

    对于需要手动签到的 Provider（如 anyrouter），POST 调用签到接口。
    签到成功的判断条件（满足任一）：
    1. ret == 1
    2. code == 0
    3. success == true
    4. 消息中包含"已经签到"/"已签到"等关键词（重复签到也算成功）

    Args:
        page: Playwright Page 对象
        account: 账号输入数据

    Returns:
        (成功标志, 原始返回数据, 错误信息)
    """
    # 自动签到类型的 Provider，无需手动调用
    if not account.sign_in_path:
        return True, None, None  # auto check-in providers

    url = f"{account.domain}{account.sign_in_path}"
    headers = build_api_headers(account)
    log(f"[{account.name}] POST {url}")

    # 在浏览器上下文中发起 POST 请求（请求体为空字符串）
    resp = await fetch_in_page(page, url, "POST", headers, "")

    # 请求失败
    if not resp.get("ok"):
        return False, None, f"fetch failed: {resp.get('error')}"
    # HTTP 状态码非 200
    if resp.get("status") != 200:
        return False, None, f"HTTP {resp.get('status')}: {resp.get('body', '')[:200]}"

    # 解析响应
    raw_body = resp["body"]
    try:
        data = json.loads(raw_body)
    except json.JSONDecodeError:
        # JSON 解析失败，但响应中包含 "success" 关键词，可能是非标准响应
        if "success" in raw_body.lower():
            return True, {"raw": raw_body[:200]}, None
        return False, None, f"invalid JSON: {raw_body[:200]}"

    # 判断签到是否成功（多种成功标识兼容）
    is_success = (
        data.get("ret") == 1
        or data.get("code") == 0
        or data.get("success") is True
    )
    if is_success:
        return True, data, None

    # 检查是否"已经签到"（重复签到也算成功）
    msg = (data.get("msg") or data.get("message") or "").lower()
    already_keywords = ["已经签到", "已签到", "重复签到", "already checked", "already signed"]
    if any(kw in msg for kw in already_keywords):
        return True, data, None

    # 签到失败
    return False, data, f"sign_in failed: {data.get('msg') or data.get('message')}"


async def perform_login(page: Page, account: AccountInput) -> tuple[bool, str | None, dict | None]:
    """使用账号密码登录，获取 session cookie。

    当浏览器中没有有效的 session cookie 且提供了 username/password 时，
    通过 POST /api/user/login 接口登录以获取新的 session。

    Args:
        page: Playwright Page 对象
        account: 账号输入数据（需包含 username 和 password）

    Returns:
        (成功标志, 错误信息, Cookie 分析结果)
    """
    if not (account.username and account.password):
        return False, "missing username/password", None

    # 登录接口 URL（带 turnstile 参数，留空表示无验证码）
    url = f"{account.domain}/api/user/login?turnstile="
    headers = {
        "Accept": "application/json, text/plain, */*",
        "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",
        "Content-Type": "application/json",
        "X-Requested-With": "XMLHttpRequest",
        account.api_user_key: account.api_user,
        "Origin": account.domain,
        "Referer": f"{account.domain}{account.login_path}",
    }
    # 请求体包含用户名和密码
    body = json.dumps({"username": account.username, "password": account.password})
    log(f"[{account.name}] login as {account.username}")

    matched_responses: list[Response] = []

    def on_response(response: Response) -> None:
        try:
            if response.url == url and response.request.method == "POST":
                matched_responses.append(response)
        except Exception:
            return

    page.on("response", on_response)
    try:
        # 在浏览器上下文中发起 POST 请求
        resp = await fetch_in_page(page, url, "POST", headers, body)
        # 给 Playwright 事件循环一点时间，确保响应事件已入队
        await page.wait_for_timeout(200)
    finally:
        page.remove_listener("response", on_response)

    # 请求失败
    if not resp.get("ok"):
        return False, f"login fetch failed: {resp.get('error')}", None
    # HTTP 状态码非 200
    if resp.get("status") != 200:
        return False, f"login HTTP {resp.get('status')}: {resp.get('body', '')[:200]}", None
    # 解析 JSON 响应
    try:
        data = json.loads(resp["body"])
    except json.JSONDecodeError:
        return False, f"login invalid JSON: {resp['body'][:200]}", None
    # 检查登录是否成功
    if not data.get("success"):
        return False, f"login rejected: {data.get('message') or data}", None

    request_cookie_header = None
    set_cookie_headers: list[str] = []
    if matched_responses:
        login_response = matched_responses[-1]
        try:
            request_cookie_header = await login_response.request.header_value("cookie")
        except Exception:
            request_cookie_header = None
        try:
            set_cookie_headers = await login_response.header_values("set-cookie")
        except Exception:
            set_cookie_headers = []
    else:
        log(f"[{account.name}] warning: login response was not captured by Playwright; cookie analysis will rely on browser context only")

    cookie_report = await build_cookie_report(
        page.context,
        account,
        request_cookie_header,
        set_cookie_headers,
    )

    return True, None, cookie_report


# ============================================================================
# 单账号签到处理
# ============================================================================

async def process_account(context: BrowserContext, account: AccountInput, timeout_ms: int) -> AccountResult:
    """处理单个账号的签到全流程。

    流程步骤：
    1. 解析并注入 Cookie 到浏览器上下文
    2. 创建新页面，访问登录页（获取 WAF cookie）
    3. 检查 session cookie，如缺失且提供账密则尝试登录
    4. 调用用户信息 API（签到前余额）
    5. 调用签到 API
    6. 调用用户信息 API（签到后余额）
    7. 关闭页面

    Args:
        context: Playwright BrowserContext 对象（所有账号共用）
        account: 账号输入数据
        timeout_ms: 页面操作超时时间（毫秒）

    Returns:
        AccountResult 签到结果
    """
    result = AccountResult(name=account.name)
    page: Page | None = None
    try:
        from urllib.parse import urlparse
        # 从 domain 提取主机名，用于设置 Cookie 的 domain 属性
        host = urlparse(account.domain).hostname or ""

        # 解析 Cookie 并注入浏览器上下文
        cookies = parse_cookies(account.cookies)
        if cookies:
            await context.add_cookies(cookies_to_playwright(cookies, host))
            log(f"[{account.name}] injected {len(cookies)} cookie(s)")

        # 创建新页面
        page = await context.new_page()
        page.set_default_timeout(timeout_ms)

        # 先访问登录页，让浏览器获取 WAF cookies
        # WAF（Web Application Firewall）会在首次访问时设置验证 cookie
        # 如 acw_tc、acw_sc__v2、cdn_sec 等
        nav_url = f"{account.domain}{account.login_path}"
        log(f"[{account.name}] goto {nav_url}")
        try:
            await page.goto(nav_url, wait_until="domcontentloaded")
            await wait_for_page_stability(page)
        except Exception as e:
            # 导航失败不中断流程，可能 WAF cookie 已通过其他方式获取
            log(f"[{account.name}] goto warning: {e}")

        # 检查是否有 session cookie
        ctx_cookies = await context.cookies()
        has_session = any(c["name"] == "session" for c in ctx_cookies)

        # 如果没有 session cookie 但提供了账密，尝试登录
        if not has_session and account.username and account.password:
            ok, err, cookie_report = await perform_login(page, account)
            if ok:
                result.used_login = True
                result.cookie_report = cookie_report
            else:
                log(f"[{account.name}] login failed: {err}")

        # ===== 签到前：查询用户信息和余额 =====
        ok, before, before_user_info, err = await call_user_info(page, account)
        if ok:
            result.before = before            # 签到前余额
            result.user_info = before_user_info  # 用户信息
        else:
            log(f"[{account.name}] user_info(before) failed: {err}")

        # ===== 签到 =====
        sign_ok, raw_sign, sign_err = await call_sign_in(page, account)
        result.raw_sign_in = raw_sign  # 保存签到 API 的原始返回数据

        # ===== 签到后：再次查询用户信息和余额 =====
        ok2, after, after_user_info, err2 = await call_user_info(page, account)
        if ok2:
            result.after = after  # 签到后余额
            # 优先使用签到后的用户信息，如果没有则保留签到前的
            result.user_info = after_user_info or result.user_info

        # 根据签到类型判断最终结果
        if account.sign_in_path:
            # 手动签到类型：以签到 API 的返回为准
            result.success = sign_ok
            if not sign_ok:
                result.error = sign_err or err2 or "sign-in failed"
        else:
            # 自动签到类型：只要签到后能获取到余额就算成功
            result.success = ok2
            if not ok2:
                result.error = err2 or "auto check-in failed"
    except Exception as e:
        # 捕获未预期的异常，避免影响其他账号
        result.error = f"exception: {e.__class__.__name__}: {e}"
        log(f"[{account.name}] exception: {traceback.format_exc()}")
    finally:
        # 确保页面关闭，释放浏览器资源
        if page is not None:
            try:
                await page.close()
            except Exception:
                pass
    return result


# ============================================================================
# 主运行函数
# ============================================================================

async def run(payload: dict) -> dict:
    """异步运行所有账号的签到流程。

    1. 启动 Chromium 浏览器（使用临时目录作为 user_data_dir）
    2. 逐个处理每个账号
    3. 收集结果并返回

    使用 temporary directory 作为浏览器 user_data_dir，
    程序结束后自动清理，避免留下浏览器数据。

    Args:
        payload: 从 stdin 解析的 JSON 数据
            - headless: 是否无头模式
            - timeout_ms: 页面操作超时时间
            - accounts: 账号列表

    Returns:
        {"results": [AccountResult.__dict__, ...]}
    """
    headless = bool(payload.get("headless", True))     # 无头模式，默认 True
    timeout_ms = int(payload.get("timeout_ms", 30000))  # 超时时间，默认 30 秒
    raw_accounts = payload.get("accounts") or []        # 账号列表
    accounts = [AccountInput(**a) for a in raw_accounts]  # 解析为 AccountInput 对象

    log(f"starting playwright headless={headless} accounts={len(accounts)}")
    results: list[AccountResult] = []

    # 启动 Playwright 并创建浏览器上下文
    async with async_playwright() as p:
        # 使用临时目录作为 user_data_dir，退出后自动清理
        with tempfile.TemporaryDirectory() as tmp_dir:
            # 创建持久化浏览器上下文
            # 持久化上下文可以在多个页面间共享 Cookie 和会话状态
            context = await launch_browser_context(p, payload, tmp_dir)
            try:
                # 逐个处理每个账号（串行处理，避免并发问题）
                for acc in accounts:
                    log(f"--- processing {acc.name} ---")
                    res = await process_account(context, acc, timeout_ms)
                    results.append(res)
            finally:
                # 确保浏览器上下文关闭
                await context.close()

    # 将结果转为字典列表，方便 JSON 序列化
    return {"results": [r.__dict__ for r in results]}


# ============================================================================
# 入口函数
# ============================================================================

def main() -> int:
    """脚本入口函数。

    从 stdin 读取 JSON 输入，运行签到流程，将结果写入 stdout。

    流程：
    1. 从 stdin 读取 JSON 数据
    2. 解析 JSON 为 payload
    3. 调用 run() 执行签到
    4. 将结果 JSON 写入 stdout

    退出码：
    - 0: 成功
    - 1: 运行时错误
    - 2: 输入错误（空 stdin 或无效 JSON）

    Returns:
        退出码
    """
    # 从 stdin 读取全部内容
    raw = sys.stdin.read()
    if not raw.strip():
        # stdin 为空，输出错误 JSON
        print(json.dumps({"error": "empty_stdin"}))
        return 2

    # 解析 JSON
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError as e:
        # JSON 格式错误
        print(json.dumps({"error": "invalid_json", "message": str(e)}))
        return 2

    # 运行签到流程
    try:
        out = asyncio.run(run(payload))
    except Exception as e:
        # 运行时异常
        log(traceback.format_exc())
        print(json.dumps({"error": "runtime", "message": str(e)}))
        return 1

    # 将结果 JSON 写入 stdout（Rust 主进程从此读取）
    print(json.dumps(out, ensure_ascii=False))
    return 0


# 脚本直接运行时的入口
if __name__ == "__main__":
    sys.exit(main())
