# AnyRouter 自动签到工具

基于 Rust + Tokio 的多账号自动签到工具。业务编排由 Rust 负责，站点访问和登录由 Python + Playwright 负责。项目当前默认使用根目录 `conf.json` 作为主配置文件。

## 功能概览

- 支持多账号
- 支持多 Provider
- 支持 `cookies` 登录
- 支持 `username + password` 登录
- 支持 GitHub / LinuxDo SSO 登录获取 Cookie
- 支持登录后分析 Cookie 过期时间
- 支持余额变化检测
- 支持异步日志、按日期目录分组、按文件大小轮转
- 支持邮件通知

## 运行依赖

### Rust

```bash
cargo build --release
```

### Python 与 Playwright

```bash
pip3 install playwright
python3 -m playwright install chromium
```

如果系统 Python 受限，建议使用虚拟环境：

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install playwright
playwright install chromium
```

## 快速开始

1. 复制样例配置：`cp conf.example.json conf.json`
2. 编辑 `conf.json`，按需填写 `accounts`、`providers`、`logging`、`email`、`runtime`
3. 运行：

```bash
cargo run --release
```

> 仓库提供完整样例 [`conf.example.json`](conf.example.json)，覆盖 Cookie、账密、GitHub/LinuxDo SSO、邮箱自动取设备验证码、自定义 Provider 等全部用法。`conf.json` 含真实凭据，已在 `.gitignore` 中忽略，不会被提交。

## 配置文件发现顺序

程序按以下顺序查找配置文件：

1. 环境变量 `ANYROUTER_CONFIG_FILE` 指定的文件
2. 当前目录 `conf.json`
3. 当前目录 `anyrouter-config.json`
4. 当前目录 `anyrouter-config.toml`

建议只使用 `conf.json`。这是当前项目的默认主配置文件。

## 完整示例

下面是覆盖全部能力的完整配置样例，与仓库根目录的 [`conf.example.json`](conf.example.json) 一致：

```json
{
  "runtime": {
    "python_bin": "python3",
    "playwright_script": "scripts/playwright_checkin.py",
    "playwright_headless": true
  },
  "logging": {
    "path": "logs",
    "max_file_size_mb": 10
  },
  "email": {
    "user": "your_email@qq.com",
    "pass": "your_smtp_authorization_code",
    "to": "receiver@example.com",
    "sender": "your_email@qq.com",
    "smtp_server": "smtp.qq.com"
  },
  "providers": {
    "custom": {
      "name": "custom",
      "domain": "https://example.com",
      "login_path": "/login",
      "sign_in_path": "/api/user/sign_in",
      "user_info_path": "/api/user/self",
      "api_user_key": "new-api-user"
    },
    "auto-checkin-site": {
      "domain": "https://auto.example.com",
      "login_path": "/login",
      "sign_in_path": null,
      "user_info_path": "/api/user/self",
      "api_user_key": "new-api-user"
    }
  },
  "accounts": [
    {
      "name": "账号A：仅 Cookie（对象格式）",
      "provider": "anyrouter",
      "api_user": "148714",
      "cookies": { "session": "your_session_cookie_value" }
    },
    {
      "name": "账号B：Cookie 字符串格式",
      "provider": "anyrouter",
      "api_user": "148715",
      "cookies": "session=your_session_cookie_value; other_cookie=value"
    },
    {
      "name": "账号C：Cookie + 账密兜底",
      "provider": "anyrouter",
      "api_user": "148716",
      "cookies": { "session": "maybe_expired_session" },
      "username": "your_login_name",
      "password": "your_login_password"
    },
    {
      "name": "账号D：仅账号密码",
      "provider": "custom",
      "api_user": "9527",
      "username": "alice",
      "password": "secret"
    },
    {
      "name": "账号E：GitHub SSO + 邮箱自动取设备验证码",
      "provider": "anyrouter",
      "api_user": "10086",
      "sso_provider": "github",
      "sso_username": "your_github_username",
      "sso_password": "your_github_password",
      "sso_email": {
        "imap_host": "imap.qq.com",
        "imap_port": 993,
        "username": "your_mailbox@qq.com",
        "password": "your_imap_authorization_code",
        "mailbox": "INBOX"
      }
    },
    {
      "name": "账号F：LinuxDo SSO",
      "provider": "anyrouter",
      "api_user": "10087",
      "sso_provider": "linuxdo",
      "sso_username": "your_linuxdo_username",
      "sso_password": "your_linuxdo_password"
    },
    {
      "name": "账号G：自动签到站点",
      "provider": "auto-checkin-site",
      "api_user": "20001",
      "cookies": { "session": "your_session_cookie_value" }
    }
  ]
}
```

各账号示例对应的登录方式：

| 示例账号 | 登录方式 | 说明 |
|----------|----------|------|
| 账号A | Cookie（对象格式） | 最常见，`session` 失效后需手动更新 |
| 账号B | Cookie（字符串格式） | `"k1=v1; k2=v2"` 写法 |
| 账号C | Cookie + 账密兜底 | Cookie 失效时浏览器自动用账密重新登录 |
| 账号D | 仅账号密码 | 首次运行即由浏览器登录获取 Cookie |
| 账号E | GitHub SSO + `sso_email` | 触发设备验证时自动从邮箱读取验证码 |
| 账号F | LinuxDo SSO | 通过 LinuxDo 授权登录 |
| 账号G | 自动签到站点 | Provider 的 `sign_in_path = null`，访问即签到 |

## 顶层配置项

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `runtime` | object | 否 | 空 | Playwright 和 Python 运行时参数 |
| `logging` | object | 否 | 空 | 日志输出配置 |
| `email` | object | 否 | 空 | 邮件通知配置 |
| `providers` | object | 否 | `{}` | 自定义 Provider 配置；会叠加到内置 Provider 上 |
| `accounts` | array | 是 | 无 | 账号列表；若缺失且没有环境变量回退，程序无法运行 |
| `log` | object | 否 | 空 | `logging` 的兼容别名 |
| `log_dir` | string | 否 | 无 | 日志目录兼容别名；效果等同于 `logging.log_dir` |

## `runtime` 配置项

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `runtime.python_bin` | string | 否 | `"python3"` | Python 解释器路径 |
| `runtime.playwright_script` | string | 否 | 自动发现 | Playwright 子脚本路径 |
| `runtime.playwright_headless` | boolean | 否 | `true` | 是否启用无头模式 |

### `runtime` 详细说明

- `python_bin`
  - 示例：`"python3"`、`".venv/bin/python3"`
  - 用于启动 `scripts/playwright_checkin.py`

- `playwright_script`
  - 示例：`"scripts/playwright_checkin.py"`
  - 若未设置，程序会继续按内置规则自动查找

- `playwright_headless`
  - `true`：无头模式，不显示浏览器窗口
  - `false`：显示浏览器窗口，便于调试

### 额外行为说明

- macOS 下如果检测到系统已安装 `Google Chrome.app`，Python 子脚本会优先使用系统 Chrome 启动，以尽量贴近真实浏览器 TLS 指纹
- 如果系统 Chrome 启动失败，会自动回退到 Playwright 自带 Chromium

## `logging` 配置项

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `logging.path` | string | 否 | `"logs"` | 日志根目录，推荐字段 |
| `logging.log_dir` | string | 否 | 无 | `logging.path` 的兼容别名；若同时存在，优先使用此字段 |
| `logging.max_file_size` | integer | 否 | `10485760` | 单个日志文件最大字节数 |
| `logging.max_file_size_mb` | integer | 否 | `10` | 单个日志文件最大 MB 数；若同时存在，优先覆盖 `max_file_size` |

### `logging` 详细说明

- 日志目录结构：

```text
logs/
└── 2026-06-15/
    ├── all.log
    ├── info.log
    ├── warn.log
    ├── error.log
    └── ...
```

- 规则：
  - 每天一个目录
  - 每个日志级别单独一个文件
  - 额外有一个 `all.log` 汇总所有级别
  - 超过大小后自动轮转为 `info-1.log`、`info-2.log` 之类的文件

### 日志级别枚举

这不是 `conf.json` 的配置项，但会直接出现在控制台和日志文件中，建议了解：

| 枚举值 | 含义 |
|--------|------|
| `TRACE` | 最细粒度调试信息 |
| `DEBUG` | 调试信息 |
| `INFO` | 常规运行信息 |
| `SUCCESS` | 成功状态信息 |
| `WARN` | 警告信息 |
| `ERROR` | 错误信息 |
| `SYSTEM` | 系统级流程信息 |
| `NETWORK` | 网络相关信息 |
| `PROCESSING` | 处理中状态信息 |
| `FATAL` | 严重错误信息 |

## `email` 配置项

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `email.user` | string | 条件必填 | `""` | SMTP 登录邮箱；启用邮件通知时必填 |
| `email.pass` | string | 条件必填 | `""` | SMTP 密码或授权码；启用邮件通知时必填 |
| `email.to` | string | 条件必填 | `""` | 收件人邮箱；启用邮件通知时必填 |
| `email.sender` | string | 否 | `email.user` | 发件人邮箱；为空时自动使用 `user` |
| `email.smtp_server` | string | 否 | 自动推断 | SMTP 服务器地址；为空时按邮箱域名自动推断 |

### `email` 详细说明

- 只有当 `user`、`pass`、`to` 三个字段都填写时，邮件通知才会生效
- `sender` 为空时，自动使用 `user`
- `smtp_server` 为空时，程序会按 `user` 的邮箱域名自动推断，例如 `qq.com -> smtp.qq.com`
- 如果配置文件和环境变量都未提供邮件配置，程序会尝试读取 `docs/reference/邮件信息.txt` 作为兜底配置
- 每次签到流程完成后都会尝试发送邮件。邮件正文按账号区分，包含签到状态、失败信息、签到前余额和签到后余额

## `providers` 配置项

`providers` 是一个对象，键名就是 Provider 名称，例如：

```json
{
  "providers": {
    "custom": {
      "domain": "https://example.com"
    }
  }
}
```

每个 Provider 支持以下字段：

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `providers.<name>.name` | string | 否 | 自动使用对象键名 | Provider 名称 |
| `providers.<name>.domain` | string | 是 | 无 | 站点域名，必须包含协议头 |
| `providers.<name>.login_path` | string | 否 | `"/login"` | 登录页路径 |
| `providers.<name>.sign_in_path` | string 或 `null` | 否 | `"/api/user/sign_in"` | 签到接口路径；为 `null` 时表示自动签到模式 |
| `providers.<name>.user_info_path` | string | 否 | `"/api/user/self"` | 用户信息接口路径 |
| `providers.<name>.api_user_key` | string | 否 | `"new-api-user"` | 用户标识请求头名称 |

### `providers` 特殊语义

- `sign_in_path`
  - 字符串：表示需要手动调用签到接口
  - `null`：表示自动签到 Provider，访问站点后由站点自身完成签到

### 内置 Provider

| Provider 名称 | 域名 | 签到方式 |
|---------------|------|----------|
| `anyrouter` | `https://anyrouter.top` | 手动签到，默认调用 `/api/user/sign_in` |
| `agentrouter` | `https://agentrouter.org` | 自动签到，`sign_in_path = null` |

## `accounts` 配置项

`accounts` 是数组。每个元素代表一个账号。

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `accounts[].api_user` | string | 是 | 无 | 请求头用户标识值 |
| `accounts[].cookies` | object 或 string 或 `null` | 条件必填 | `{}` | Cookie 登录信息 |
| `accounts[].provider` | string | 否 | `"anyrouter"` | 所属 Provider 名称 |
| `accounts[].name` | string | 否 | `Account N` | 展示名称 |
| `accounts[].username` | string | 条件必填 | 无 | 登录用户名 |
| `accounts[].password` | string | 条件必填 | 无 | 登录密码 |
| `accounts[].sso_provider` | string | 条件必填 | 无 | SSO 平台，支持 `github` / `linuxdo` |
| `accounts[].sso_username` | string | 条件必填 | 无 | SSO 平台用户名或邮箱 |
| `accounts[].sso_password` | string | 条件必填 | 无 | SSO 平台密码 |
| `accounts[].sso_email` | object | 否 | 无 | GitHub 触发设备验证时，从该邮箱自动读取验证码 |
| `accounts[].sso_email.imap_host` | string | 条件必填 | 无 | IMAP 服务器，如 `imap.qq.com` |
| `accounts[].sso_email.imap_port` | number | 否 | `993` | IMAP 端口（SSL） |
| `accounts[].sso_email.username` | string | 条件必填 | 无 | 邮箱登录名 |
| `accounts[].sso_email.password` | string | 条件必填 | 无 | IMAP 授权码（QQ/163 等非登录密码） |
| `accounts[].sso_email.mailbox` | string | 否 | `INBOX` | 邮箱文件夹 |

### `accounts` 必填规则

- `api_user` 永远必填
- `cookies`、`username + password`、`sso_provider + sso_username + sso_password` 至少要提供一组
- 如果提供了 `username`，就必须同时提供 `password`
- 如果提供了 `password`，就必须同时提供 `username`
- 如果提供了任一 SSO 字段，`sso_provider`、`sso_username`、`sso_password` 必须同时提供

### `accounts[].cookies` 支持格式

#### 对象格式

```json
{
  "cookies": {
    "session": "xxx",
    "other_cookie": "yyy"
  }
}
```

#### 字符串格式

```json
{
  "cookies": "session=xxx; other_cookie=yyy"
}
```

#### 旧版兼容字段

`cookies` 对象内还兼容以下旧字段：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `cookies._username` | string | 否 | 旧版用户名字段，等价于顶层 `username` |
| `cookies._password` | string | 否 | 旧版密码字段，等价于顶层 `password` |
| `cookies._sso_provider` | string | 否 | SSO 平台兼容字段，等价于顶层 `sso_provider` |
| `cookies._sso_username` | string | 否 | SSO 用户名兼容字段，等价于顶层 `sso_username` |
| `cookies._sso_password` | string | 否 | SSO 密码兼容字段，等价于顶层 `sso_password` |

推荐使用顶层 `username` / `password` / `sso_*` 字段，不要再写 `_username` / `_password` / `_sso_*`。

### SSO 登录说明

当没有有效 `session` Cookie 时，程序会通过站点的 `/api/oauth/state` 接口与 `client_id` 直接跳转到 GitHub / LinuxDo 的 OAuth 授权页（部分站点登录页不再渲染第三方登录按钮，因此不依赖页面按钮），在第三方登录页填写 `sso_username` / `sso_password`，授权完成后返回 AnyRouter 或 AgentRouter，并从浏览器上下文中提取 Cookie 信息。

SSO 登录依赖第三方平台页面结构和风控策略。首次配置建议将 `runtime.playwright_headless` 设置为 `false`，便于处理验证码、二次验证或授权确认。若第三方平台需要 2FA 则自动流程无法完成。

#### GitHub 设备验证（邮箱验证码）

GitHub 在新设备 / 新 IP 登录时常要求设备验证，会向账号邮箱发送 6 位验证码。为该账号配置 `sso_email`（IMAP）后，程序会自动登录邮箱、读取最新的 GitHub 验证码、填入验证页并提交，从而完成 SSO。

- QQ 邮箱：`imap_host = imap.qq.com`，`password` 填 **IMAP/SMTP 授权码**（在邮箱设置中开启 IMAP 服务后生成），不是登录密码。
- 163 邮箱：`imap_host = imap.163.com`，同样使用授权码。
- 仅 `sso_email` 三件套（`imap_host` / `username` / `password`）齐全时才会启用自动取码；否则遇到设备验证会直接报出可读错误。
- 2FA（TOTP 动态码）无法自动完成，请改用 cookie 登录。

## 当前公开配置项中的“枚举/受限取值”说明

当前 `conf.json` 里没有严格意义上的固定枚举字段，但有以下受限取值或特殊语义字段：

| 字段 | 可选值/语义 | 说明 |
|------|-------------|------|
| `accounts[].provider` | `anyrouter`、`agentrouter`、自定义 Provider 名称 | 必须指向一个存在的 Provider |
| `accounts[].sso_provider` | `github`、`linuxdo` | 触发对应第三方 SSO 登录 |
| `providers.<name>.sign_in_path` | 字符串 或 `null` | 字符串表示手动签到，`null` 表示自动签到 |
| `runtime.playwright_headless` | `true` / `false` | 是否显示浏览器窗口 |

## 环境变量回退与覆盖规则

虽然项目现在推荐使用 `conf.json`，但仍保留环境变量回退能力。

| 环境变量 | 必填 | 用途 | 优先级说明 |
|----------|------|------|-----------|
| `ANYROUTER_CONFIG_FILE` | 否 | 指定配置文件路径 | 高于默认 `conf.json` |
| `ANYROUTER_ACCOUNTS` | 条件必填 | 账号 JSON 数组 | 仅当未找到配置文件时回退使用 |
| `PROVIDERS` | 否 | 自定义 Provider JSON 对象 | 仅当未找到配置文件时回退使用 |
| `PYTHON_BIN` | 否 | Python 解释器路径 | 低于 `runtime.python_bin` |
| `PLAYWRIGHT_HEADLESS` | 否 | 是否无头模式 | 低于 `runtime.playwright_headless` |
| `PLAYWRIGHT_SCRIPT` | 否 | Playwright 子脚本路径 | 低于 `runtime.playwright_script` |
| `EMAIL_USER` | 条件必填 | SMTP 登录邮箱 | 仅当 `email` 配置缺失时回退使用 |
| `EMAIL_PASS` | 条件必填 | SMTP 密码或授权码 | 仅当 `email` 配置缺失时回退使用 |
| `EMAIL_TO` | 条件必填 | 收件人邮箱 | 仅当 `email` 配置缺失时回退使用 |
| `EMAIL_SENDER` | 否 | 发件人邮箱 | 仅当 `email` 配置缺失时回退使用 |
| `CUSTOM_SMTP_SERVER` | 否 | SMTP 服务器地址 | 仅当 `email` 配置缺失时回退使用 |
| `ANYROUTER_USERNAME_<N>` | 否 | 第 N 个账号用户名 | 仅作为账号级账密最终兜底 |
| `ANYROUTER_PASSWORD_<N>` | 否 | 第 N 个账号密码 | 仅作为账号级账密最终兜底 |
| `ANYROUTER_SSO_PROVIDER_<N>` | 否 | 第 N 个账号 SSO 平台 | 仅作为账号级 SSO 最终兜底 |
| `ANYROUTER_SSO_USERNAME_<N>` | 否 | 第 N 个账号 SSO 用户名或邮箱 | 仅作为账号级 SSO 最终兜底 |
| `ANYROUTER_SSO_PASSWORD_<N>` | 否 | 第 N 个账号 SSO 密码 | 仅作为账号级 SSO 最终兜底 |

### 账号级账密优先级

从高到低：

1. `accounts[].username` / `accounts[].password`
2. `accounts[].cookies._username` / `accounts[].cookies._password`
3. `ANYROUTER_USERNAME_<N>` / `ANYROUTER_PASSWORD_<N>`

### 账号级 SSO 优先级

从高到低：

1. `accounts[].sso_provider` / `accounts[].sso_username` / `accounts[].sso_password`
2. `accounts[].cookies._sso_provider` / `accounts[].cookies._sso_username` / `accounts[].cookies._sso_password`
3. `ANYROUTER_SSO_PROVIDER_<N>` / `ANYROUTER_SSO_USERNAME_<N>` / `ANYROUTER_SSO_PASSWORD_<N>`

## 日志输出行为

- 控制台日志只对“级别标签”着色
- 时间、前缀、正文保持默认终端颜色
- 日志默认写入 `logs/YYYY-MM-DD/`

## 程序执行流程

1. 读取配置文件
2. 合并内置 Provider 与自定义 Provider
3. 校验账号配置
4. 启动 Playwright
5. 对每个账号执行：打开登录页、按 Cookie / SSO / 账密尝试登录、查询余额、签到、再次查询余额
6. 计算余额哈希并判断是否变化
7. 发送签到邮件报告

## 常见问题

### 1. 没有 `conf.json` 会怎样？

程序会尝试回退到环境变量模式。如果 `ANYROUTER_ACCOUNTS` 也不存在，则启动失败。

### 2. 只配置 `username` 不配置 `password` 可以吗？

不可以。`username` 和 `password` 必须成对出现。

### 3. 可以只配置 `cookies` 吗？

可以。只要 `cookies` 能完成认证，就不需要 `username` / `password`。

也可以只配置 SSO 三件套：`sso_provider`、`sso_username`、`sso_password`。程序会通过 GitHub 或 LinuxDo 登录站点并获取 Cookie。

### 4. `sign_in_path` 什么时候写 `null`？

当站点是“访问即签到”模式时写 `null`，例如内置 `agentrouter`。

### 5. 邮件配置不填会怎样？

不会报错。如果 `conf.json`、环境变量和 `docs/reference/邮件信息.txt` 都没有可用邮件配置，程序会跳过邮件发送。

## 验证命令

```bash
cargo check
cargo test
python3 -m py_compile scripts/playwright_checkin.py
```
