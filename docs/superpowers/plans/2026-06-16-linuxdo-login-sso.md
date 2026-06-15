# LinuxDo 论坛登录 + SSO 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 `sso_provider=linuxdo` 的账号先用 Playwright 浏览器登录 linux.do 论坛建立会话，再完成 connect.linux.do OAuth 授权到 anyrouter/agentrouter 并签到。

**Architecture:** 在 `scripts/playwright_checkin.py` 新增「论坛预登录」步骤与两个纯函数辅助（选择器表、Cloudflare 检测），并把 `perform_sso_login` 的 linuxdo 分支改为「先预登录、再走现有 OAuth authorize/同意/回跳」。复用现有 `fill_first_available`/`click_first_available`/`maybe_click_authorize`/`wait_for_return_to_service`/`wait_for_session_cookie`。

**Tech Stack:** Python 3 + Playwright（async）；Rust 主程序无需改动；测试用 unittest（含 IsolatedAsyncioTestCase 与 unittest.mock）。

参考设计：`docs/superpowers/specs/2026-06-16-linuxdo-login-sso-design.md`

---

## 文件结构

- 修改：`scripts/playwright_checkin.py` — 新增辅助函数与预登录函数，改 `perform_sso_login` linuxdo 分支，删除 `login_linuxdo_sso`，泛化 `session_cookie_present`/`wait_for_session_cookie`。
- 修改：`tests/test_playwright_checkin_helpers.py` — 新增单元测试。
- 修改：`conf.json`（已被 .gitignore 忽略）— 新增第三个账号。

## 前置：隔离上一轮的 bug 修复提交

当前 `scripts/playwright_checkin.py`、`src/playwright.rs`、`tests/test_playwright_checkin_helpers.py` 含上一轮调试任务的修复且未提交。先单独提交它们，避免与本功能混在同一 commit。

- [ ] **Step 0: 提交上一轮修复（独立 commit）**

```bash
git add src/playwright.rs scripts/playwright_checkin.py tests/test_playwright_checkin_helpers.py
git commit -m "fix: Windows 下自动探测 Python 解释器并统一子进程 UTF-8 编码"
```

Expected: 仅该批修复入库；之后本计划的提交只含 linuxdo 功能增量。

---

## Task 1: 新增 `linuxdo_login_selectors()` 纯函数

**Files:**
- Modify: `scripts/playwright_checkin.py`（在 `sso_button_selectors` 之后，约第 916 行）
- Test: `tests/test_playwright_checkin_helpers.py`

- [ ] **Step 1: 写失败测试**

在 `tests/test_playwright_checkin_helpers.py` 的 `PlaywrightCheckinHelpersTest` 类内新增：

```python
    def test_linuxdo_login_selectors_cover_username_password_submit(self):
        sel = playwright_checkin.linuxdo_login_selectors()
        self.assertTrue(sel["username"] and sel["password"] and sel["submit"])
        self.assertTrue(any("login-account-name" in s for s in sel["username"]))
        self.assertTrue(any("password" in s for s in sel["password"]))
        self.assertTrue(any("login-button" in s or "submit" in s for s in sel["submit"]))
```

- [ ] **Step 2: 运行测试确认失败**

Run: `python -m unittest tests.test_playwright_checkin_helpers.PlaywrightCheckinHelpersTest.test_linuxdo_login_selectors_cover_username_password_submit -v`
Expected: FAIL/ERROR —`module 'playwright_checkin' has no attribute 'linuxdo_login_selectors'`

- [ ] **Step 3: 写最小实现**

在 `scripts/playwright_checkin.py` 中 `sso_button_selectors` 函数定义之后新增：

```python
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
```

- [ ] **Step 4: 运行测试确认通过**

Run: `python -m unittest tests.test_playwright_checkin_helpers.PlaywrightCheckinHelpersTest.test_linuxdo_login_selectors_cover_username_password_submit -v`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add scripts/playwright_checkin.py tests/test_playwright_checkin_helpers.py
git commit -m "feat: 新增 linux.do 登录页选择器表"
```

---

## Task 2: 新增 `detect_cloudflare_challenge()` 纯函数

**Files:**
- Modify: `scripts/playwright_checkin.py`（紧接 `linuxdo_login_selectors` 之后）
- Test: `tests/test_playwright_checkin_helpers.py`

- [ ] **Step 1: 写失败测试**

在 `PlaywrightCheckinHelpersTest` 类内新增：

```python
    def test_detect_cloudflare_challenge_flags_known_markers(self):
        self.assertTrue(playwright_checkin.detect_cloudflare_challenge("Just a moment...", ""))
        self.assertTrue(
            playwright_checkin.detect_cloudflare_challenge("", "Checking your browser before accessing")
        )
        self.assertTrue(
            playwright_checkin.detect_cloudflare_challenge("Attention Required! | Cloudflare", "")
        )

    def test_detect_cloudflare_challenge_ignores_normal_login_page(self):
        self.assertFalse(
            playwright_checkin.detect_cloudflare_challenge("登录 - LINUX DO", "用户名 密码 登录")
        )
```

- [ ] **Step 2: 运行测试确认失败**

Run: `python -m unittest tests.test_playwright_checkin_helpers.PlaywrightCheckinHelpersTest.test_detect_cloudflare_challenge_flags_known_markers -v`
Expected: FAIL/ERROR — `has no attribute 'detect_cloudflare_challenge'`

- [ ] **Step 3: 写最小实现**

在 `scripts/playwright_checkin.py` 中 `linuxdo_login_selectors` 之后新增：

```python
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
```

- [ ] **Step 4: 运行测试确认通过**

Run: `python -m unittest tests.test_playwright_checkin_helpers.PlaywrightCheckinHelpersTest -k cloudflare -v`
Expected: 两个 cloudflare 测试 PASS

- [ ] **Step 5: 提交**

```bash
git add scripts/playwright_checkin.py tests/test_playwright_checkin_helpers.py
git commit -m "feat: 新增 Cloudflare 拦截页检测"
```

---

## Task 3: 泛化 session cookie 检测以支持自定义 cookie 名

**Files:**
- Modify: `scripts/playwright_checkin.py`（`session_cookie_present` 约第 1309 行、`wait_for_session_cookie` 约第 1318 行）
- Test: `tests/test_playwright_checkin_helpers.py`

- [ ] **Step 1: 写失败测试**

在 `PlaywrightCheckinAsyncTest` 类内新增：

```python
    async def test_session_cookie_present_supports_custom_cookie_names(self):
        class FakeContext:
            async def cookies(self, _urls):
                return [{"name": "_t", "value": "abc"}]

        ctx = FakeContext()
        self.assertTrue(
            await playwright_checkin.session_cookie_present(
                ctx, "https://linux.do", ("_t", "_forum_session")
            )
        )
        self.assertFalse(
            await playwright_checkin.session_cookie_present(ctx, "https://linux.do", ("session",))
        )
```

- [ ] **Step 2: 运行测试确认失败**

Run: `python -m unittest tests.test_playwright_checkin_helpers.PlaywrightCheckinAsyncTest.test_session_cookie_present_supports_custom_cookie_names -v`
Expected: FAIL —`session_cookie_present() takes 2 positional arguments but 3 were given`

- [ ] **Step 3: 写最小实现（替换现有两个函数体）**

把 `scripts/playwright_checkin.py` 现有 `session_cookie_present` 替换为：

```python
async def session_cookie_present(
    context: BrowserContext,
    domain: str,
    cookie_names: tuple[str, ...] = ("session",),
) -> bool:
    """检查浏览器上下文里是否已存在指定名称的非空会话 cookie。"""
    cookies = await context.cookies([domain])
    return any(
        cookie.get("name") in cookie_names and cookie.get("value")
        for cookie in cookies
    )
```

把现有 `wait_for_session_cookie` 替换为：

```python
async def wait_for_session_cookie(
    context: BrowserContext,
    domain: str,
    timeout_ms: int,
    poll_interval_ms: int = 500,
    cookie_names: tuple[str, ...] = ("session",),
) -> bool:
    """轮询等待目标域出现指定名称的会话 cookie。"""
    loop = asyncio.get_event_loop()
    deadline = loop.time() + max(timeout_ms, 0) / 1000.0
    while True:
        if await session_cookie_present(context, domain, cookie_names):
            return True
        if loop.time() >= deadline:
            return False
        await asyncio.sleep(poll_interval_ms / 1000.0)
```

- [ ] **Step 4: 运行全部测试确认通过（含原有 session cookie 测试不回归）**

Run: `python -m unittest discover -s tests -p "test_*.py"`
Expected: OK（原 `test_wait_for_session_cookie_*` 与 `test_session_cookie_present_ignores_empty_value` 仍通过）

- [ ] **Step 5: 提交**

```bash
git add scripts/playwright_checkin.py tests/test_playwright_checkin_helpers.py
git commit -m "refactor: session cookie 检测支持自定义 cookie 名"
```

---

## Task 4: 新增 `login_linuxdo_forum()` 论坛预登录

**Files:**
- Modify: `scripts/playwright_checkin.py`（在 `login_linuxdo_sso` 附近，约第 1270 行之后新增）
- Test: `tests/test_playwright_checkin_helpers.py`

- [ ] **Step 1: 在测试文件顶部补充 mock 导入**

把 `tests/test_playwright_checkin_helpers.py` 顶部导入：

```python
import importlib.util
import json
import pathlib
import sys
import unittest
```

改为（新增一行 `from unittest import mock`）：

```python
import importlib.util
import json
import pathlib
import sys
import unittest
from unittest import mock
```

- [ ] **Step 2: 写失败测试（含 fake page/locator/context）**

在 `PlaywrightCheckinAsyncTest` 类内新增：

```python
    async def test_login_linuxdo_forum_fills_and_confirms_session(self):
        rec = []

        class FakeLocator:
            def __init__(self, selector):
                self._selector = selector

            @property
            def first(self):
                return self

            async def wait_for(self, state, timeout):
                pass

            async def fill(self, value, timeout=0):
                rec.append(("fill", self._selector, value))

            async def click(self, timeout=0, force=False):
                rec.append(("click", self._selector))

        class FakeContext:
            async def cookies(self, _urls):
                return [{"name": "_t", "value": "sess"}]

        class FakePage:
            def __init__(self, title):
                self._title = title
                self.context = FakeContext()
                self.url = "https://linux.do/login"

            async def goto(self, url, wait_until=None):
                rec.append(("goto", url))

            async def title(self):
                return self._title

            async def inner_text(self, selector, timeout=0):
                return ""

            def locator(self, selector):
                return FakeLocator(selector)

        account = playwright_checkin.AccountInput(
            name="L", provider="anyrouter", domain="https://anyrouter.top",
            login_path="/login", sign_in_path="/api/user/sign_in",
            user_info_path="/api/user/self", api_user_key="new-api-user", api_user="190030",
            sso_provider="linuxdo", sso_username="nianliu.wjj", sso_password="pw",
        )

        async def _noop(*args, **kwargs):
            return None

        with mock.patch.object(playwright_checkin, "wait_for_page_stability", _noop):
            await playwright_checkin.login_linuxdo_forum(FakePage("登录 - LINUX DO"), account, timeout_ms=5000)

        filled_values = [r[2] for r in rec if r[0] == "fill"]
        self.assertIn("nianliu.wjj", filled_values)
        self.assertIn("pw", filled_values)
        self.assertTrue(any(r[0] == "click" for r in rec))
        self.assertTrue(any(r == ("goto", "https://linux.do/login") for r in rec))

    async def test_login_linuxdo_forum_raises_on_cloudflare(self):
        rec = []

        class FakeLocator:
            def __init__(self, selector):
                self._selector = selector

            @property
            def first(self):
                return self

            async def wait_for(self, state, timeout):
                pass

            async def fill(self, value, timeout=0):
                rec.append(("fill", self._selector, value))

            async def click(self, timeout=0, force=False):
                rec.append(("click", self._selector))

        class FakeContext:
            async def cookies(self, _urls):
                return []

        class FakePage:
            def __init__(self, title):
                self._title = title
                self.context = FakeContext()
                self.url = "https://linux.do/login"

            async def goto(self, url, wait_until=None):
                rec.append(("goto", url))

            async def title(self):
                return self._title

            async def inner_text(self, selector, timeout=0):
                return ""

            def locator(self, selector):
                return FakeLocator(selector)

        account = playwright_checkin.AccountInput(
            name="L", provider="anyrouter", domain="https://anyrouter.top",
            login_path="/login", sign_in_path="/api/user/sign_in",
            user_info_path="/api/user/self", api_user_key="new-api-user", api_user="190030",
            sso_provider="linuxdo", sso_username="nianliu.wjj", sso_password="pw",
        )

        async def _noop(*args, **kwargs):
            return None

        with mock.patch.object(playwright_checkin, "wait_for_page_stability", _noop):
            with self.assertRaises(RuntimeError) as caught:
                await playwright_checkin.login_linuxdo_forum(FakePage("Just a moment..."), account, timeout_ms=5000)

        self.assertIn("Cloudflare", str(caught.exception))
        self.assertFalse(any(r[0] == "fill" for r in rec))
```

> 注：上面第一个测试里的 `account_page(...) if False else ...` 是笔误占位，**实现时直接写**：
> `await playwright_checkin.login_linuxdo_forum(FakePage("登录 - LINUX DO"), account, timeout_ms=5000)`

- [ ] **Step 3: 运行测试确认失败**

Run: `python -m unittest tests.test_playwright_checkin_helpers.PlaywrightCheckinAsyncTest -k linuxdo_forum -v`
Expected: ERROR —`has no attribute 'login_linuxdo_forum'`

- [ ] **Step 4: 写最小实现**

在 `scripts/playwright_checkin.py` 中 `login_linuxdo_sso` 函数定义之后新增：

```python
async def login_linuxdo_forum(page: Page, account: AccountInput, timeout_ms: int) -> None:
    """先登录 linux.do 论坛，建立会话，供后续 connect.linux.do OAuth 授权复用。"""
    assert account.sso_username is not None
    assert account.sso_password is not None

    await page.goto("https://linux.do/login", wait_until="domcontentloaded")
    await wait_for_page_stability(page, min(timeout_ms, 8000))

    title = await page.title()
    try:
        body = await page.inner_text("body", timeout=2000)
    except Exception:
        body = ""
    if detect_cloudflare_challenge(title, body):
        raise RuntimeError(
            "linux.do 被 Cloudflare 拦截（人机校验）；建议设置 PLAYWRIGHT_HEADLESS=0 手动通过，"
            "或在浏览器上下文复用有效的 cf_clearance。"
        )

    selectors = linuxdo_login_selectors()
    await fill_first_available(page, selectors["username"], account.sso_username)
    await fill_first_available(page, selectors["password"], account.sso_password)
    await click_first_available(page, selectors["submit"])
    await wait_for_page_stability(page, min(timeout_ms, 8000))

    if not await wait_for_session_cookie(
        page.context, "https://linux.do", timeout_ms, cookie_names=("_t", "_forum_session")
    ):
        raise RuntimeError(
            f"linuxdo forum login failed: 登录后未检测到 linux.do 会话 cookie (current_url={page.url})"
        )
    log(f"[{account.name}] linux.do forum login OK")
```

- [ ] **Step 5: 运行测试确认通过**

Run: `python -m unittest tests.test_playwright_checkin_helpers.PlaywrightCheckinAsyncTest -k linuxdo_forum -v`
Expected: 两个测试 PASS

- [ ] **Step 6: 提交**

```bash
git add scripts/playwright_checkin.py tests/test_playwright_checkin_helpers.py
git commit -m "feat: 新增 linux.do 论坛预登录（含 Cloudflare 检测与会话校验）"
```

---

## Task 5: 在 `perform_sso_login` 中接线（先预登录，再授权），并删除旧 `login_linuxdo_sso`

**Files:**
- Modify: `scripts/playwright_checkin.py`（`perform_sso_login` 约第 1448-1456 行；删除 `login_linuxdo_sso` 约第 1270-1306 行）

- [ ] **Step 1: 改写 `perform_sso_login` 的 SSO 入口/分支块**

把现有：

```python
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
```

替换为：

```python
    try:
        # LinuxDo：先登录论坛建立会话，再发起 OAuth 授权
        if provider == "linuxdo":
            await login_linuxdo_forum(page, account, timeout_ms)

        entry = await start_oauth_authorization(page, account, provider)
        log(f"[{account.name}] SSO entry via {entry}")
        if provider == "github":
            await login_github_sso(page, account, timeout_ms)
        elif provider == "linuxdo":
            # 已预登录 linux.do；授权页若需确认则点击，然后等待回跳目标站
            await maybe_click_authorize(page)
            await wait_for_return_to_service(page, account, timeout_ms)
        else:
            return False, f"unsupported SSO provider: {provider}", None
    except Exception as err:
        return False, f"SSO login failed: {err}", None
```

- [ ] **Step 2: 删除不再使用的 `login_linuxdo_sso`**

删除 `scripts/playwright_checkin.py` 中整个 `async def login_linuxdo_sso(...)` 函数（约第 1270-1306 行，定义 + docstring + 函数体）。

- [ ] **Step 3: 确认无残留引用**

Run: `grep -n "login_linuxdo_sso" scripts/playwright_checkin.py`
Expected: 无输出（已全部移除）

- [ ] **Step 4: 运行全部单元测试确认不回归**

Run: `python -m unittest discover -s tests -p "test_*.py"`
Expected: OK（全部通过）

- [ ] **Step 5: 提交**

```bash
git add scripts/playwright_checkin.py
git commit -m "feat: linuxdo SSO 改为先论坛登录再授权，移除旧的跳转中填表逻辑"
```

---

## Task 6: 配置第三个账号（本地 conf.json，gitignore，不提交）

**Files:**
- Modify: `conf.json`（`accounts` 数组追加一项）

- [ ] **Step 1: 在 `conf.json` 的 `accounts` 数组末尾追加**

在 `linuxdo_148714` 条目之后追加（真实密码见 `docs/refs/LinuxDo登录接口信息.md`）：

```json
    ,
    {
      "name": "linuxdo_190030",
      "provider": "anyrouter",
      "api_user": "190030",
      "sso_provider": "linuxdo",
      "sso_username": "nianliu.wjj",
      "sso_password": "<填入文档中的真实密码>"
    }
```

- [ ] **Step 2: 校验 JSON 合法**

Run: `python -c "import json; json.load(open('conf.json', encoding='utf-8')); print('conf.json OK')"`
Expected: `conf.json OK`

- [ ] **Step 3: 确认 conf.json 不会被提交**

Run: `git check-ignore conf.json`
Expected: `conf.json`（已忽略；本任务不产生提交）

---

## Task 7: 端到端验证

**Files:** 无（运行验证）

- [ ] **Step 1: 编译 Rust 主程序**

Run: `cargo build`
Expected: Finished（无错误）

- [ ] **Step 2: 运行签到并捕获日志**

Run: `./target/debug/anyrouter-checkin.exe > /tmp/run_linuxdo.txt 2>&1; echo "exit=$?"`
Expected: 进程结束（退出码记录）

> 若实测在 linux.do 遇 Cloudflare 拦截（日志出现「被 Cloudflare 拦截」），按错误提示设 `PLAYWRIGHT_HEADLESS=0` 后重试：
> `PLAYWRIGHT_HEADLESS=0 ./target/debug/anyrouter-checkin.exe > /tmp/run_linuxdo.txt 2>&1`

- [ ] **Step 3: 核对关键日志**

Run: `cat /tmp/run_linuxdo.txt | sed 's/\x1b\[[0-9;]*m//g' | grep -E "linux.do forum login OK|SSO entry|linuxdo_190030|SUCCEEDED|FAILED|session cookie|Cloudflare"`
Expected（成功路径）：出现 `linux.do forum login OK`、`linuxdo_190030 ... SUCCEEDED`，无解析错误。

- [ ] **Step 4: 失败定位**

依据可读中文错误归因：
- 含「Cloudflare」→ 用非 headless 重试（见 Step 2 备注）。
- 含 `linuxdo forum login failed` → 核对选择器（对照实时 DOM 调整 `linuxdo_login_selectors`）或凭据。
- 含「未启用 LinuxDo OAuth」→ 目标站未配置 linuxdo（换 anyrouter，或确认 agentrouter 是否支持）。

---

## 自审记录

- **Spec 覆盖**：浏览器表单登录(Task 4)、先登录后授权(Task 5)、Cloudflare 处理(Task 2/4)、目标站无 client_id 报错(复用现有 `start_oauth_authorization` + Task 7 归因)、新增第三账号(Task 6)、端到端验证(Task 7)、单元测试(Task 1-4)。均有对应任务。
- **占位扫描**：所有步骤均含完整代码/命令，无 TBD/TODO。
- **类型一致**：`login_linuxdo_forum(page, account, timeout_ms)`、`linuxdo_login_selectors() -> dict[str,list[str]]`、`detect_cloudflare_challenge(title, body)`、`session_cookie_present(context, domain, cookie_names=...)`、`wait_for_session_cookie(context, domain, timeout_ms, poll_interval_ms=500, cookie_names=...)` 在各任务间一致。
