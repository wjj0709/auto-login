#!/usr/bin/env python3
"""
AnyRouter Playwright 签到子脚本。

由 Rust 主程序通过子进程调用，约定：
- 入参：从 stdin 读 JSON {"accounts": [...], "headless": bool, "timeout_ms": int}
- 账号字段：name, provider, domain, login_path, sign_in_path, user_info_path,
            api_user_key, api_user, cookies(dict|str|null), username?, password?,
            sso_provider?, sso_username?, sso_password?
- 出参：向 stdout 写一行 JSON {"results": [...]}，每个账号一项
        每项含 success, before, after, user_info, error, used_login(bool), cookie_report?

设计原则：所有网络请求都走浏览器上下文（page.evaluate fetch），
让 Chromium 自身完成 TLS 握手与 WAF 通过，规避非浏览器 client 的指纹拒绝。
"""

from __future__ import annotations

import asyncio    # 异步运行时，Playwright async API 依赖
import imaplib    # 读取邮箱以获取 GitHub 设备验证码
import json       # JSON 解析与序列化
import os
import re         # 验证码正则提取
import sys        # 系统标准输入输出
import tempfile   # 临时目录（浏览器 user_data_dir 用完即删）
import time       # 记录验证起始时间，过滤旧邮件
import traceback  # 异常堆栈追踪
from datetime import datetime, timedelta, timezone
from dataclasses import dataclass, field  # 数据类装饰器
from email import message_from_bytes
from email.header import decode_header, make_header
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
    sso_provider: str | None = None        # SSO 平台（github / linuxdo）
    sso_username: str | None = None        # SSO 平台用户名或邮箱
    sso_password: str | None = None        # SSO 平台密码
    sso_email: Any = None                  # 读取设备验证码的邮箱 IMAP 配置（dict）


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
            result = await page.evaluate(
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
            continue

        # 导航进行中发起的 fetch 会被浏览器中止，返回 "Failed to fetch"。
        # 这是短暂错误，等页面稳定后重试。
        if (
            not result.get("ok")
            and "Failed to fetch" in str(result.get("error", ""))
            and attempt < 2
        ):
            last_error = result.get("error")
            log(
                f"transient '{result.get('error')}' during {method} {url}; waiting and retrying ({attempt + 1}/3)"
            )
            await page.wait_for_timeout(400 * (attempt + 1))
            continue

        return result

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


def determine_result_status(
    account: AccountInput,
    *,
    sign_ok: bool,
    sign_err: str | None,
    after_ok: bool,
    after_err: str | None,
) -> tuple[bool, str | None]:
    """根据签到方式和签到后用户信息查询结果判断最终状态。

    手动签到 Provider 必须同时满足：
    1. 签到接口成功
    2. 签到后仍能用当前 api_user 查询到用户信息

    第二个条件能捕获“浏览器里是 A 用户 session，但当前账号 api_user 是 B”
    这种跨账号 Cookie 污染问题，避免把错误账号误判为成功。
    """
    if account.sign_in_path:
        if not sign_ok:
            return False, sign_err or after_err or "sign-in failed"
        if not after_ok:
            return False, after_err or "user_info(after) failed after sign-in"
        return True, None

    if after_ok:
        return True, None
    return False, after_err or "auto check-in failed"


SUPPORTED_SSO_PROVIDERS = {
    "github": "github",
    "linuxdo": "linuxdo",
    "linux-do": "linuxdo",
    "linux.do": "linuxdo",
    "linux_do": "linuxdo",
}


def normalize_sso_provider(provider: str | None) -> str | None:
    """标准化 SSO Provider 名称。"""
    if provider is None:
        return None

    normalized = provider.strip().lower()
    if not normalized:
        return None
    if normalized not in SUPPORTED_SSO_PROVIDERS:
        raise ValueError(f"unsupported SSO provider: {provider}")
    return SUPPORTED_SSO_PROVIDERS[normalized]


def has_sso_credentials(account: AccountInput) -> bool:
    """判断账号是否配置完整 SSO 凭据。"""
    return bool(
        normalize_sso_provider(account.sso_provider)
        and account.sso_username
        and account.sso_username.strip()
        and account.sso_password
        and account.sso_password.strip()
    )


def sso_button_selectors(provider: str) -> list[str]:
    """返回目标站点登录页上 SSO 入口按钮的候选选择器。"""
    normalized = normalize_sso_provider(provider)
    if normalized == "github":
        return [
            "a[href*='github']",
            "button:has-text('GitHub')",
            "a:has-text('GitHub')",
            "text=/GitHub/i",
            "a[href*='/oauth/github']",
            "a[href*='/auth/github']",
            "a[href*='provider=github']",
        ]
    if normalized == "linuxdo":
        return [
            "a[href*='linux.do']",
            "a[href*='linuxdo']",
            "button:has-text('LinuxDo')",
            "button:has-text('Linux DO')",
            "button:has-text('Linux.do')",
            "a:has-text('LinuxDo')",
            "a:has-text('Linux DO')",
            "a:has-text('Linux.do')",
            "text=/Linux\\s*\\.?\\s*Do/i",
            "a[href*='/oauth/linux']",
            "a[href*='/auth/linux']",
            "a[href*='provider=linux']",
        ]
    raise ValueError(f"unsupported SSO provider: {provider}")


def linuxdo_login_selectors() -> dict[str, list[str]]:
    """linux.do 登录页候选选择器（用户名 / 密码 / 提交按钮）。

    以 Discourse 常见结构为主，附通用回退；实现/调试时可对照实时 DOM 调整。
    """
    return {
        "username": [
            "#login-account-name",
            "input[name='login']",
            "input[name='username']",
            "input[type='email']",
            "input[type='text']",
        ],
        "password": [
            "#login-account-password",
            "input[name='password']",
            "input[type='password']",
        ],
        "submit": [
            "#login-button",
            "button[type='submit']",
            "input[type='submit']",
            "button:has-text('登录')",
            "button:has-text('Log In')",
            "button:has-text('Sign In')",
        ],
    }


# Cloudflare 人机校验页常见标记（小写匹配）。
CLOUDFLARE_CHALLENGE_MARKERS = (
    "just a moment",
    "checking your browser",
    "attention required",
    "cf-challenge",
    "cf-browser-verification",
    "请稍候",
    "正在验证",
)


def detect_cloudflare_challenge(title: str, body: str) -> bool:
    """根据页面标题/正文判断是否落在 Cloudflare 人机校验页。"""
    haystack = f"{title}\n{body}".lower()
    return any(marker in haystack for marker in CLOUDFLARE_CHALLENGE_MARKERS)


async def click_first_available(page: Page, selectors: list[str], timeout_ms: int = 5000) -> str:
    """依次尝试点击候选选择器，返回命中的选择器。"""
    errors: list[str] = []
    for selector in selectors:
        try:
            locator = page.locator(selector).first
            await locator.wait_for(state="visible", timeout=timeout_ms)
            try:
                await locator.click(timeout=timeout_ms)
            except Exception:
                await locator.click(timeout=timeout_ms, force=True)
            return selector
        except Exception as err:
            errors.append(f"{selector}: {err}")
    raise RuntimeError("no matching SSO entry found; tried " + "; ".join(errors[:3]))


async def fill_first_available(
    page: Page,
    selectors: list[str],
    value: str,
    timeout_ms: int = 5000,
) -> str:
    """依次尝试填写候选输入框，返回命中的选择器。"""
    errors: list[str] = []
    for selector in selectors:
        try:
            locator = page.locator(selector).first
            await locator.wait_for(state="visible", timeout=timeout_ms)
            await locator.fill(value, timeout=timeout_ms)
            return selector
        except Exception as err:
            errors.append(f"{selector}: {err}")
    raise RuntimeError("no matching input found; tried " + "; ".join(errors[:3]))


async def maybe_click_authorize(page: Page, timeout_ms: int = 3000) -> None:
    """OAuth 授权页可能要求确认授权；没有按钮时静默跳过。"""
    selectors = [
        "button:has-text('Authorize')",
        "button:has-text('授权')",
        "button:has-text('同意')",
        "button:has-text('Allow')",
        "input[type='submit'][value*='Authorize']",
        "input[type='submit'][value*='授权']",
        "button[type='submit']",
    ]
    for selector in selectors:
        try:
            locator = page.locator(selector).first
            await locator.wait_for(state="visible", timeout=timeout_ms)
            await locator.click(timeout=timeout_ms)
            return
        except Exception:
            continue


async def wait_for_return_to_service(page: Page, account: AccountInput, timeout_ms: int) -> None:
    """等待 OAuth 完成后回到 AnyRouter/AgentRouter 域名。"""
    try:
        await page.wait_for_url(
            lambda url: str(url).startswith(account.domain),
            timeout=timeout_ms,
        )
    except Exception:
        # 有些流程会停在中间页再用 JS 跳转；额外等待页面稳定即可。
        await wait_for_page_stability(page, min(timeout_ms, 5000))


def parse_sso_email_config(raw: Any) -> dict | None:
    """解析读取设备验证码所需的邮箱 IMAP 配置。

    必填：imap_host、username、password（QQ/163 等需用授权码而非登录密码）。
    可选：imap_port（默认 993）、mailbox（默认 INBOX）。
    """
    if not isinstance(raw, dict):
        return None

    host = str(raw.get("imap_host") or "").strip()
    username = str(raw.get("username") or "").strip()
    password_raw = raw.get("password")
    password = str(password_raw).strip() if password_raw is not None else ""
    if not (host and username and password):
        return None

    try:
        port = int(raw.get("imap_port") or 993)
    except (TypeError, ValueError):
        port = 993

    mailbox = str(raw.get("mailbox") or "INBOX").strip() or "INBOX"
    return {
        "imap_host": host,
        "imap_port": port,
        "username": username,
        "password": password,
        "mailbox": mailbox,
    }


def extract_github_otp(text: str | None) -> str | None:
    """从邮件文本中提取 GitHub 设备验证码（6 位数字）。"""
    if not text:
        return None
    labeled = re.search(
        r"(?:verification|authentication|device)[^0-9]{0,40}(\d{6})",
        text,
        re.IGNORECASE,
    )
    if labeled:
        return labeled.group(1)
    standalone = re.search(r"(?<!\d)(\d{6})(?!\d)", text)
    return standalone.group(1) if standalone else None


def _decode_subject(raw_subject: str | None) -> str:
    """解码可能经过 RFC2047 编码的邮件主题。"""
    if not raw_subject:
        return ""
    try:
        return str(make_header(decode_header(raw_subject)))
    except Exception:
        return raw_subject


def _message_plain_text(msg) -> str:
    """提取邮件正文文本（合并 text/plain 与 text/html）。"""
    chunks: list[str] = []
    parts = msg.walk() if msg.is_multipart() else [msg]
    for part in parts:
        if part.get_content_type() not in ("text/plain", "text/html"):
            continue
        try:
            payload = part.get_payload(decode=True)
        except Exception:
            payload = None
        if not payload:
            continue
        charset = part.get_content_charset() or "utf-8"
        try:
            chunks.append(payload.decode(charset, "ignore"))
        except (LookupError, UnicodeDecodeError):
            chunks.append(payload.decode("utf-8", "ignore"))
    return "\n".join(chunks)


def _imap_send_id(conn: imaplib.IMAP4) -> None:
    """部分服务商（QQ/163）要求客户端先发送 IMAP ID，否则 SELECT 报 Unsafe Login。"""
    try:
        args = '("name" "anyrouter-checkin" "version" "1.0")'
        typ, data = conn._simple_command("ID", args)  # type: ignore[attr-defined]
        conn._untagged_response(typ, data, "ID")  # type: ignore[attr-defined]
    except Exception:
        pass


def read_github_otp_once(config: dict, not_before_epoch: float) -> str | None:
    """连接一次 IMAP，扫描最近来自 GitHub 的邮件并提取验证码。

    只接受时间戳不早于 not_before_epoch - 180s 的邮件，避免读到历史验证码。
    在独立线程中调用（imaplib 为阻塞 API）。
    """
    conn: imaplib.IMAP4 | None = None
    try:
        conn = imaplib.IMAP4_SSL(config["imap_host"], config["imap_port"])
        conn.login(config["username"], config["password"])
        _imap_send_id(conn)
        conn.select(config["mailbox"])

        typ, data = conn.search(None, "ALL")
        if typ != "OK" or not data or not data[0]:
            return None

        ids = data[0].split()
        for num in reversed(ids[-15:]):  # 最近 15 封，从新到旧
            typ, msg_data = conn.fetch(num, "(RFC822)")
            if typ != "OK" or not msg_data:
                continue
            raw_bytes = next(
                (part[1] for part in msg_data if isinstance(part, tuple) and part[1]),
                None,
            )
            if not raw_bytes:
                continue

            msg = message_from_bytes(raw_bytes)
            sender = (msg.get("From") or "").lower()
            subject = _decode_subject(msg.get("Subject"))
            if "github" not in sender and "github" not in subject.lower():
                continue

            date_header = msg.get("Date")
            if date_header:
                try:
                    sent = parsedate_to_datetime(date_header)
                    if sent.tzinfo is None:
                        sent = sent.replace(tzinfo=timezone.utc)
                    if sent.timestamp() < not_before_epoch - 180:
                        continue
                except (TypeError, ValueError, IndexError):
                    pass

            code = extract_github_otp(subject + "\n" + _message_plain_text(msg))
            if code:
                return code
        return None
    finally:
        if conn is not None:
            try:
                conn.logout()
            except Exception:
                pass


async def fetch_github_otp(config: dict, not_before_epoch: float, timeout_ms: int) -> str | None:
    """轮询邮箱直到拿到验证码或超时（邮件投递有延迟）。"""
    loop = asyncio.get_event_loop()
    deadline = loop.time() + max(timeout_ms, 0) / 1000.0
    while True:
        try:
            code = await asyncio.to_thread(read_github_otp_once, config, not_before_epoch)
        except Exception as err:
            log(f"IMAP read failed: {err}")
            code = None
        if code:
            return code
        if loop.time() >= deadline:
            return None
        await asyncio.sleep(5)


async def complete_github_device_verification(
    page: Page,
    account: AccountInput,
    email_config: dict,
    not_before_epoch: float,
    timeout_ms: int,
) -> bool:
    """读取邮箱验证码，填入 GitHub 设备验证页并提交。"""
    log(
        f"[{account.name}] GitHub device verification triggered; "
        f"reading code from mailbox {email_config['username']}"
    )
    code = await fetch_github_otp(email_config, not_before_epoch, max(timeout_ms, 60000))
    if not code:
        return False

    log(f"[{account.name}] fetched device verification code from email")
    await fill_first_available(
        page,
        [
            "input[name='otp']",
            "#otp",
            "input[autocomplete='one-time-code']",
            "input[inputmode='numeric']",
            "input[placeholder='XXXXXX']",
        ],
        code,
    )
    try:
        await click_first_available(
            page,
            [
                "button[type='submit']:has-text('Verify')",
                "button:has-text('Verify')",
                "button[type='submit']",
            ],
        )
    except Exception:
        # 部分页面输入满 6 位会自动提交，没有可点按钮属正常。
        pass

    await wait_for_page_stability(page)
    return "/sessions/verified-device" not in page.url


async def login_github_sso(page: Page, account: AccountInput, timeout_ms: int) -> None:
    """完成 GitHub OAuth 登录表单。"""

    await fill_first_available(
        page,
        [
            "input[name='login']",
            "input#login_field",
            "input[type='email']",
            "input[name='username']",
        ],
        account.sso_username,
    )
    await fill_first_available(
        page,
        [
            "input[name='password']",
            "input#password",
            "input[type='password']",
        ],
        account.sso_password,
    )
    verification_started = time.time()
    await click_first_available(
        page,
        [
            "input[name='commit']",
            "input[type='submit']",
            "button[type='submit']",
            "button:has-text('Sign in')",
        ],
    )
    await wait_for_page_stability(page)

    # GitHub 可能要求设备验证（向账号邮箱发送 6 位验证码）。
    # 若配置了可读取的邮箱，则自动取码并填入；否则给出可读错误。
    if "/sessions/verified-device" in page.url:
        email_config = parse_sso_email_config(account.sso_email)
        if email_config:
            verified = await complete_github_device_verification(
                page, account, email_config, verification_started, timeout_ms
            )
            if not verified:
                raise RuntimeError(
                    "GitHub 设备验证失败：未能从邮箱读取到验证码或验证码无效；"
                    "请检查 sso_email 的 IMAP 配置（QQ/163 需使用授权码）"
                )
        # 未配置邮箱时由 detect_github_login_block 抛出指引性错误。

    detect_github_login_block(page)
    await maybe_click_authorize(page)
    await wait_for_return_to_service(page, account, timeout_ms)
    detect_github_login_block(page)


def detect_github_login_block(page: Page) -> None:
    """识别 GitHub 登录被设备验证 / 2FA / 凭据错误拦截的情况。

    这些都是 GitHub 侧的安全控制，脚本无法自动完成（需要邮箱验证码、
    TOTP 等人工输入），因此直接抛出可读错误，避免误判为“无 session”。
    """
    current_url = page.url
    if "/sessions/verified-device" in current_url:
        raise RuntimeError(
            "GitHub 要求设备验证（已向账号邮箱发送验证码），自动化无法完成；"
            "请在受信任设备上手动登录一次，或改用 cookie/账密登录"
        )
    if "/sessions/two-factor" in current_url or "/two-factor" in current_url:
        raise RuntimeError(
            "GitHub 启用了两步验证（2FA），自动化无法输入动态验证码；"
            "请改用 cookie 登录或为该账号准备可自动化的登录方式"
        )
    if current_url.startswith("https://github.com/session") and "return_to" in current_url:
        raise RuntimeError("GitHub 凭据可能不正确，登录未通过（停留在 GitHub 登录页）")


async def login_linuxdo_sso(page: Page, account: AccountInput, timeout_ms: int) -> None:
    """完成 LinuxDo OAuth 登录表单。"""
    assert account.sso_username is not None
    assert account.sso_password is not None

    await fill_first_available(
        page,
        [
            "input[name='login']",
            "input[name='username']",
            "input[name='email']",
            "input[type='email']",
            "input[type='text']",
        ],
        account.sso_username,
    )
    await fill_first_available(
        page,
        [
            "input[name='password']",
            "input[type='password']",
        ],
        account.sso_password,
    )
    await click_first_available(
        page,
        [
            "button[type='submit']",
            "input[type='submit']",
            "button:has-text('登录')",
            "button:has-text('Log in')",
            "button:has-text('Sign in')",
        ],
    )
    await wait_for_page_stability(page)
    await maybe_click_authorize(page)
    await wait_for_return_to_service(page, account, timeout_ms)


async def session_cookie_present(context: BrowserContext, domain: str) -> bool:
    """检查浏览器上下文里是否已存在非空的 session cookie。"""
    cookies = await context.cookies([domain])
    return any(
        cookie.get("name") == "session" and cookie.get("value")
        for cookie in cookies
    )


async def wait_for_session_cookie(
    context: BrowserContext,
    domain: str,
    timeout_ms: int,
    poll_interval_ms: int = 500,
) -> bool:
    """轮询等待 OAuth 回调异步写入 session cookie。

    AnyRouter（基于 new-api）的 GitHub OAuth 回调会先跳回
    `/oauth/github?code=...`，再由前端发起 XHR，服务端通过 Set-Cookie
    写入 session。该 cookie 在跳回域名之后才异步出现，因此必须轮询，
    而不能在导航稳定后立即一次性判断。
    """
    loop = asyncio.get_event_loop()
    deadline = loop.time() + max(timeout_ms, 0) / 1000.0
    while True:
        if await session_cookie_present(context, domain):
            return True
        if loop.time() >= deadline:
            return False
        await asyncio.sleep(poll_interval_ms / 1000.0)


# new-api 状态接口里各 OAuth Provider 的 client_id 字段名。
OAUTH_CLIENT_ID_KEYS = {
    "github": "github_client_id",
    "linuxdo": "linuxdo_client_id",
}


async def fetch_service_json(page: Page, account: AccountInput, path: str) -> dict | None:
    """在浏览器上下文里 GET 目标站点的 JSON 接口并解析 data 字段。"""
    url = f"{account.domain}{path}"
    resp = await fetch_in_page(page, url, "GET", build_api_headers(account), None)
    if not resp.get("ok") or resp.get("status") != 200:
        return None
    try:
        payload = json.loads(resp["body"])
    except json.JSONDecodeError:
        return None
    if not payload.get("success"):
        return None
    data = payload.get("data")
    return data if isinstance(data, dict) else {"data": data}


async def fetch_oauth_state(page: Page, account: AccountInput) -> str | None:
    """获取 new-api 颁发的一次性 OAuth state（GET /api/oauth/state）。"""
    data = await fetch_service_json(page, account, "/api/oauth/state")
    if data is None:
        return None
    state = data.get("data") if "data" in data else None
    return state if isinstance(state, str) and state else None


async def fetch_oauth_client_id(page: Page, account: AccountInput, provider: str) -> str | None:
    """从 /api/status 读取指定 Provider 的 client_id。"""
    key = OAUTH_CLIENT_ID_KEYS.get(provider)
    if not key:
        return None
    data = await fetch_service_json(page, account, "/api/status")
    if not data:
        return None
    client_id = data.get(key)
    return client_id if isinstance(client_id, str) and client_id else None


def build_oauth_authorize_url(provider: str, client_id: str, state: str) -> str:
    """构建第三方 OAuth 授权入口 URL（与 new-api 前端逻辑一致）。"""
    if provider == "github":
        return (
            "https://github.com/login/oauth/authorize"
            f"?client_id={client_id}&scope=user:email&state={state}"
        )
    if provider == "linuxdo":
        return (
            "https://connect.linux.do/oauth2/authorize"
            f"?response_type=code&client_id={client_id}&state={state}"
        )
    raise ValueError(f"unsupported SSO provider: {provider}")


async def start_oauth_authorization(
    page: Page,
    account: AccountInput,
    provider: str,
) -> str:
    """启动 OAuth 授权流程，返回所采用入口方式的描述。

    优先直接构造授权 URL 跳转（new-api 部分部署的登录页根本不渲染
    第三方登录按钮，按钮点击方式会失败）；拿不到 state/client_id 时
    回退到点击登录页上的 SSO 入口按钮。
    """
    state = await fetch_oauth_state(page, account)
    client_id = await fetch_oauth_client_id(page, account, provider)
    if state and client_id:
        authorize_url = build_oauth_authorize_url(provider, client_id, state)
        log(f"[{account.name}] navigating to {provider} OAuth authorize endpoint")
        await page.goto(authorize_url, wait_until="domcontentloaded")
        await wait_for_page_stability(page)
        return "authorize-url"

    selector = await click_first_available(page, sso_button_selectors(provider))
    await wait_for_page_stability(page)
    return f"button:{selector}"


async def perform_sso_login(
    page: Page,
    account: AccountInput,
    timeout_ms: int,
) -> tuple[bool, str | None, dict | None]:
    """通过 GitHub 或 LinuxDo SSO 登录目标站点并提取 Cookie 信息。"""
    try:
        provider = normalize_sso_provider(account.sso_provider)
    except ValueError as err:
        return False, str(err), None

    if not provider:
        return False, "missing sso_provider", None
    if not (account.sso_username and account.sso_password):
        return False, "missing SSO username/password", None

    log(f"[{account.name}] SSO login via {provider} as {account.sso_username}")
    before_cookie_names = {
        cookie.get("name")
        for cookie in await page.context.cookies([account.domain])
        if cookie.get("name")
    }

    try:
        entry = await start_oauth_authorization(page, account, provider)
        log(f"[{account.name}] SSO entry via {entry}")
        if provider == "github":
            await login_github_sso(page, account, timeout_ms)
        elif provider == "linuxdo":
            await login_linuxdo_sso(page, account, timeout_ms)
        else:
            return False, f"unsupported SSO provider: {provider}", None
    except Exception as err:
        return False, f"SSO login failed: {err}", None

    await wait_for_page_stability(page)
    # OAuth 回调跳回域名后，session cookie 由前端 XHR 触发服务端 Set-Cookie
    # 异步写入，需轮询等待而非立即判断。
    try:
        await page.wait_for_load_state("networkidle", timeout=min(timeout_ms, 8000))
    except Exception:
        pass

    has_session = await wait_for_session_cookie(page.context, account.domain, timeout_ms)
    if not has_session:
        current_url = page.url
        return (
            False,
            f"SSO login finished but session cookie was not found (current_url={current_url}); "
            "可能 OAuth 表单未提交成功、停在 GitHub 二次验证页，或回调未写入 session",
            None,
        )

    # OAuth 落地页仍可能在向 /console 跳转，此时直接发 API 请求会与导航竞争
    # 触发 "Failed to fetch"。先把页面停到一个稳定的同源页面再继续。
    try:
        await page.goto(f"{account.domain}/console", wait_until="domcontentloaded")
        await wait_for_page_stability(page)
    except Exception as err:
        log(f"[{account.name}] settle navigation warning: {err}")

    after_cookies = await page.context.cookies([account.domain])

    after_cookie_names = {
        cookie.get("name")
        for cookie in after_cookies
        if cookie.get("name")
    }
    new_cookie_names = sorted(after_cookie_names - before_cookie_names)
    cookie_report = await build_cookie_report(page.context, account, None, [])
    if cookie_report is None:
        cookie_report = {
            "request_cookie_names": [],
            "response_cookie_names": [],
            "cookies": [],
            "note": None,
        }
    cookie_report["note"] = (
        f"Logged in via {provider} SSO. "
        f"New cookie(s): {', '.join(new_cookie_names) if new_cookie_names else 'none detected'}."
    )

    return True, None, cookie_report


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
    3. 检查 session cookie，如缺失则按 SSO / 账密配置尝试登录
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
        login_error: str | None = None

        # 如果没有 session cookie 但提供了 SSO，优先尝试 SSO 登录
        if not has_session and has_sso_credentials(account):
            ok, err, cookie_report = await perform_sso_login(page, account, timeout_ms)
            if ok:
                result.used_login = True
                result.cookie_report = cookie_report
                ctx_cookies = await context.cookies()
                has_session = any(c["name"] == "session" for c in ctx_cookies)
            else:
                login_error = err
                log(f"[{account.name}] SSO login failed: {err}")

        # 如果仍没有 session cookie 但提供了账密，尝试普通登录
        if not has_session and account.username and account.password:
            ok, err, cookie_report = await perform_login(page, account)
            if ok:
                result.used_login = True
                result.cookie_report = cookie_report
                login_error = None
            else:
                login_error = err
                log(f"[{account.name}] login failed: {err}")

        # 登录失败后页面可能停在第三方域名（如 github.com），此时直接发
        # API 请求会因跨域得到误导性的 "Failed to fetch"。先回到目标站点，
        # 让后续请求拿到真实的 401，并保留登录失败的真实原因。
        if not has_session and login_error and not page.url.startswith(account.domain):
            try:
                await page.goto(f"{account.domain}{account.login_path}", wait_until="domcontentloaded")
                await wait_for_page_stability(page)
            except Exception as nav_err:
                log(f"[{account.name}] re-settle navigation warning: {nav_err}")

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

        # 根据签到类型和签到后用户信息查询结果判断最终结果。
        result.success, result.error = determine_result_status(
            account,
            sign_ok=sign_ok,
            sign_err=sign_err,
            after_ok=ok2,
            after_err=err2,
        )

        # 登录从未成功时，登录失败原因比下游的 401/网络错误更能说明问题。
        if not result.success and not has_session and login_error:
            result.error = login_error
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
        results = await process_all_accounts(
            playwright=p,
            payload=payload,
            accounts=accounts,
            timeout_ms=timeout_ms,
        )

    # 将结果转为字典列表，方便 JSON 序列化
    return {"results": [r.__dict__ for r in results]}


async def process_all_accounts(
    *,
    playwright,
    payload: dict,
    accounts: list[AccountInput],
    timeout_ms: int,
    context_factory=launch_browser_context,
    account_processor=process_account,
) -> list[AccountResult]:
    """逐个处理账号，每个账号使用独立浏览器上下文。

    这里不能复用同一个 BrowserContext：站点 session cookie 名相同，
    复用上下文会让第二个账号看到第一个账号的登录态，进而触发
    New-Api-User 与登录用户不匹配。
    """
    results: list[AccountResult] = []

    for acc in accounts:
        log(f"--- processing {acc.name} ---")
        with tempfile.TemporaryDirectory() as tmp_dir:
            context = await context_factory(playwright, payload, tmp_dir)
            try:
                res = await account_processor(context, acc, timeout_ms)
                results.append(res)
            finally:
                await context.close()

    return results


def _recover_text(text: str) -> str:
    """将含孤立代理项的字符串尽力恢复为可读文本，并保证可安全编码为 UTF-8。

    线上场景：服务器以 GBK 返回中文消息，其字节经 surrogateescape 解码成
    \\udcXX 孤立代理项。这里先取回原始字节，再依次尝试 utf-8 / gbk 解码；
    若都失败则用 replace 兜底，确保返回值不含孤立代理项。
    """
    if not any("\ud800" <= ch <= "\udfff" for ch in text):
        return text
    raw = text.encode("utf-8", "surrogateescape")
    for enc in ("utf-8", "gbk", "gb18030"):
        try:
            return raw.decode(enc)
        except UnicodeDecodeError:
            continue
    return raw.decode("utf-8", "replace")


def sanitize_for_json(value):
    """递归清洗对象，确保所有字符串均为合法 Unicode（无孤立代理项），
    使其能被 json.dumps 序列化为合法 UTF-8，供 Rust 主进程严格按 UTF-8 解析。
    """
    if isinstance(value, str):
        return _recover_text(value)
    if isinstance(value, dict):
        return {key: sanitize_for_json(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [sanitize_for_json(item) for item in value]
    return value


def _configure_stdio() -> None:
    """将标准输入/输出/错误流统一为 UTF-8。

    Rust 主进程通过 stdin 传入 UTF-8 的 JSON，并按 UTF-8 解析 stdout。
    但 Windows 上 Python 默认按 locale 编码（常为 GBK）读写这些流，会导致：
    1. stdin 中的中文（如账号名）被按 GBK 误解码而损坏，结果名无法与账号匹配；
    2. stdout 的 JSON 不是合法 UTF-8，Rust 端解析失败；
    3. 终端日志中文乱码。
    显式重配置为 UTF-8 可一并解决这些问题。
    """
    for stream in (sys.stdin, sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except (AttributeError, ValueError):
            # 个别环境的流对象可能不支持 reconfigure，忽略即可
            pass


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
    # 统一标准流编码为 UTF-8，避免 Windows locale（GBK）导致 stdout 非法 UTF-8
    _configure_stdio()

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
    # 先清洗，确保非 UTF-8 来源（如服务器 GBK 消息）不会产出非法 UTF-8
    print(json.dumps(sanitize_for_json(out), ensure_ascii=False))
    return 0


# 脚本直接运行时的入口
if __name__ == "__main__":
    sys.exit(main())
