# LinuxDo 论坛登录 + SSO 到 anyrouter/agentrouter — 设计

日期：2026-06-16
状态：已与用户确认，待转入实现计划

## 背景与目标

`anyrouter` / `agentrouter` 基于 new-api，支持通过 **LinuxDo Connect**（`connect.linux.do`）做 OAuth2 登录。
现有代码（`scripts/playwright_checkin.py` 的 `perform_sso_login → start_oauth_authorization → login_linuxdo_sso`）直接跳转
`connect.linux.do/oauth2/authorize`，并**假设跳转后会出现一个可直接填写的登录表单**。

但实际授权前需要先在 **linux.do 论坛本身**登录建立会话（参考文档 `docs/refs/LinuxDo登录接口信息.md` 描述的 `POST https://linux.do/login`）。
linux.do 受 **Cloudflare** 保护（`cf_clearance`），登录是 **Discourse** 的专有页面/表单，现有"跳转中填表"很可能就卡在这一步。

**目标**：用 Playwright 真实浏览器先登录 linux.do、建立会话，再完成 OAuth 授权到目标站点，最终完成签到。

## 已确认的关键决策

| 项 | 决策 |
|---|---|
| 登录机制 | **浏览器表单驱动**（Playwright 打开 `linux.do/login` 填表提交），而非复刻原始 POST。理由：自动处理 CSRF / Cloudflare / Cookie，契合现有"所有请求走浏览器上下文"的架构。 |
| 流程嵌入 | **先登录 linux.do，再走现有 OAuth authorize**（不是只在跳转中填表）。 |
| 配置 | conf.json **新增第三个账号**（保留原 `linuxdo_148714` 的 github 配置）。 |
| 验证 | 用真实凭据跑端到端（登录 → SSO → session → 签到）。 |

## 流程（数据流）

针对 `sso_provider == "linuxdo"` 的账号：

```
① 预登录 linux.do（新增 login_linuxdo_forum）
   page.goto https://linux.do/login (wait_until=domcontentloaded)
   → detect_cloudflare_challenge(): 命中拦截页 → 抛可操作错误（建议关 headless）
   → 填用户名(sso_username) / 密码(sso_password)，点登录
   → 轮询确认会话：linux.do 域出现 _t / _forum_session cookie，或已跳回论坛首页
   → 失败（仍停在登录页 / 报错）→ 抛 "linuxdo forum login failed: …"

② OAuth 授权（复用 start_oauth_authorization）
   GET 目标站 /api/oauth/state + /api/status(linuxdo_client_id)
   → 无 linuxdo_client_id → 抛 "目标站点未启用 LinuxDo OAuth"
   → goto connect.linux.do/oauth2/authorize?response_type=code&client_id=…&state=…
   → 已有 linux.do 会话 → 若出现授权/同意按钮则点击 (maybe_click_authorize)

③ 回调 + 写 session（复用 wait_for_session_cookie）
   → 轮询目标域 session cookie → 之后照常 user_info / sign_in
```

## 组件与接口（`scripts/playwright_checkin.py`）

**新增**
- `login_linuxdo_forum(page, account, timeout_ms) -> None`
  导航 `linux.do/login`，填表、提交、校验会话；失败抛异常（带可读原因）。
- `linuxdo_login_selectors() -> dict[str, list[str]]`
  返回 `{"username": [...], "password": [...], "submit": [...]}` 候选选择器。**纯函数，可单测。**
  候选（实现时对照实时 DOM 校准）：
  - username：`#login-account-name`、`input[name='login']`、`input[name='username']`、`input[type='email']`
  - password：`#login-account-password`、`input[name='password']`、`input[type='password']`
  - submit：`#login-button`、`button[type='submit']`、`button:has-text('登录')`、`button:has-text('Log In')`
- `detect_cloudflare_challenge(title: str, body: str) -> bool`
  根据标记（如 `Just a moment`、`cf-challenge`、`Checking your browser`、`Attention Required`）判断是否被 Cloudflare 拦。**纯函数，可单测。**

**修改**
- `perform_sso_login`：当 `provider == "linuxdo"` 时，在 `start_oauth_authorization` **之前**调用 `login_linuxdo_forum`；
  原 `login_linuxdo_sso` 的"跳转中填表"逻辑收敛——授权阶段只保留同意页处理（`maybe_click_authorize`）。

**复用（不改）**
`fetch_oauth_state`、`fetch_oauth_client_id`、`build_oauth_authorize_url`、`maybe_click_authorize`、
`wait_for_session_cookie`、`fetch_in_page`、`fill_first_available`、`click_first_available`。

## 配置改动（`conf.json`，已被 .gitignore 忽略）

`accounts` 数组**新增第三个条目**（凭据只存于本地 conf.json，不写入仓库 / spec）：

```json
{
  "name": "linuxdo_190030",
  "provider": "anyrouter",
  "api_user": "190030",
  "sso_provider": "linuxdo",
  "sso_username": "nianliu.wjj",
  "sso_password": "<见 docs/refs，写入本地 conf.json>"
}
```

原 `教育邮箱`、`linuxdo_148714`（github）两条保持不变；运行后账号总数为 3。

## 错误处理

| 场景 | 行为 |
|---|---|
| Cloudflare 拦截 linux.do | 抛错并提示：设 `PLAYWRIGHT_HEADLESS=0` 手动过验证，或复用已有 `cf_clearance`。 |
| 论坛登录失败（账密错 / 表单缺失） | `linuxdo forum login failed: <reason>`。 |
| 目标站无 `linuxdo_client_id`（如 agentrouter 未启用） | `目标站点未启用 LinuxDo OAuth`（运行时探测，明确告知）。 |
| 授权后无 session cookie | 复用现有错误（含 current_url 与可能原因）。 |

所有面向用户的文案为可读中文 + 可操作建议（沿用上一轮修复确立的 UTF-8 输出与 `sanitize_for_json`）。

## 测试

**单元测试（离线，沿用 `tests/test_playwright_checkin_helpers.py` 风格）**
- `linuxdo_login_selectors()` 覆盖用户名/密码/提交三类字段。
- `detect_cloudflare_challenge()` 对拦截页/正常页样本返回 真/假。
- 用 fake page/context 验证 `perform_sso_login` 对 `linuxdo` 会**先 `login_linuxdo_forum` 后 authorize**（仿照现有 `process_all_accounts` 的注入式异步测试）。

**端到端**
用 conf.json（api_user=190030 / nianliu.wjj）跑真实二进制，验证 登录 → SSO → session → sign_in 全通；
失败时依据可读错误定位（Cloudflare / 选择器 / client_id）。

## 风险与未决

- **Cloudflare（主要风险）**：headless 下导航 linux.do 可能触发人机校验。先实现检测+可操作报错；若实测被拦，回退非 headless 或复用 cf_clearance。
- **Discourse 选择器**：以 linux.do 实时 DOM 为准，候选选择器实现时校准。
- **agentrouter 支持**：取决于其 `/api/status` 是否返回 `linuxdo_client_id`；不支持则按错误处理明确报出。

## 不做（YAGNI）

- 不实现原始 HTTP POST 登录路径。
- 不实现"POST 失败回退浏览器"的混合路径。
- 不改动 GitHub SSO 与 username/password 既有流程。
