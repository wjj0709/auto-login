# AnyRouter 自动签到工具 (Rust + Playwright)

基于 Rust + Tokio 的多站点多账号自动签到工具。**所有 HTTP 流量都通过真实 Chromium 浏览器（Playwright）发起**，规避 anyrouter.top / agentrouter.org 这类站点的 TLS 指纹拒绝（JA3）和 WAF JS Challenge。

---

## ✨ 功能特性

- **多站点支持** — 内置 AnyRouter.top 和 AgentRouter.org 站点配置，支持通过环境变量自定义扩展
- **多账号批量签到** — 一次浏览器会话内串行处理所有账号
- **真实浏览器 TLS** — Playwright + Chromium 提供与真人一致的 TLS 握手与 WAF cookie 流转，无需自行解码 `acw_sc__v2`
- **双登录方式** — 已有 `cookies` 时直接注入；提供 `username` + `password` 时浏览器自动登录获取新 cookie
- **余额变化检测** — 基于 SHA-256 快照比对，精准检测签到前后余额变动
- **邮件通知** — 签到失败或余额变动时自动发送 SMTP 邮件
- **敏感信息脱敏** — 日志输出自动对 Cookie、密码等敏感字段进行脱敏

---

## 🛠️ 技术栈

| 组件 | 技术 |
|------|------|
| 主程序 | Rust (Edition 2021) + Tokio |
| 浏览器自动化 | Python 3.9+ + Playwright (Chromium) |
| 序列化 | serde / serde_json |
| 配置加载 | dotenvy |
| 邮件发送 | lettre |
| 哈希计算 | sha2 |

---

## 📦 项目结构

```
src/
├── main.rs          # 主程序入口
├── config.rs        # 配置：Provider 与账号解析
├── playwright.rs    # 调用 Playwright 子进程
├── checkin.rs       # 通知文案格式化
├── balance.rs       # 余额快照 hash
├── notify.rs        # SMTP 邮件
└── log.rs           # 日志
scripts/
└── playwright_checkin.py   # Playwright 全流程签到脚本
```

---

## 🚀 安装与使用

### 1. 安装依赖

```bash
# Rust 主程序依赖
cargo build --release

# Playwright（Python 3.9+）
pip3 install playwright
python3 -m playwright install chromium
```

> 如果你的系统 Python 受限（例如 macOS 自带 Python），建议使用 venv：
> ```bash
> python3 -m venv .venv && source .venv/bin/activate
> pip install playwright && playwright install chromium
> # 然后在 .env 里设置 PYTHON_BIN=.venv/bin/python3
> ```

### 2. 编辑 `.env`

```env
# 账号配置：JSON 数组（必须双引号包裹并转义）
# 字段：cookies(必填)、api_user(必填)、provider(可选)、name(可选)
#       _username / _password (可选；当 cookies 失效时由浏览器自动登录)
ANYROUTER_ACCOUNTS="[{\"cookies\":{\"session\":\"xxx\"},\"api_user\":\"148714\",\"name\":\"账号A\"}]"

# 可选：自定义 Provider
# PROVIDERS={"custom":{"domain":"https://example.com","sign_in_path":"/api/user/sign_in"}}

# 可选：Playwright 调试 — 显示真实窗口
# PLAYWRIGHT_HEADLESS=false

# 可选：自定义 Python 解释器路径
# PYTHON_BIN=.venv/bin/python3

# 可选：自定义 Playwright 脚本路径
# PLAYWRIGHT_SCRIPT=scripts/playwright_checkin.py

# 可选：邮件通知
# EMAIL_USER=your_email@example.com
# EMAIL_PASS=your_password
# EMAIL_TO=recipient@example.com
```

#### 账号字段

| 字段 | 必填 | 说明 |
|------|------|------|
| `cookies` | 是 | JSON 对象 `{"session":"xxx"}` 或字符串 `"session=xxx"` |
| `api_user` | 是 | `new-api-user` header 的值 |
| `provider` | 否 | 站点名称，默认 `anyrouter` |
| `name` | 否 | 账号别名，用于日志/通知显示 |
| `_username` | 否 | 账户名；当 cookie 失效时浏览器会自动登录 |
| `_password` | 否 | 账户密码；和 `_username` 配对使用 |

> 也可通过 `ANYROUTER_USERNAME_<n>` / `ANYROUTER_PASSWORD_<n>` 形式按账号序号注入凭证（避免写在 JSON 里）。

### 3. 运行

```bash
cargo run --release
# 或
./target/release/anyrouter-checkin
```

---

## 🔄 执行流程

```
阶段 1: 加载 .env / Provider / 账号
    ↓
阶段 2: 启动 Chromium 子进程（Playwright），逐账号：
    ├── 注入用户 cookies（如有）
    ├── goto domain → 浏览器拿到 WAF cookie
    ├── 若无 session 但提供账密 → 走 /api/user/login 登录
    ├── page.evaluate(fetch) 调用 /api/user/self（签到前余额）
    ├── page.evaluate(fetch) 调用 /api/user/sign_in（签到）
    └── page.evaluate(fetch) 调用 /api/user/self（签到后余额）
    ↓
阶段 3: 余额变化检测（SHA-256 快照比对）
    ↓
阶段 4: 发送通知（失败 / 余额变动时触发）
```

**关键点**：所有目标站点的 HTTP 请求都用 `page.evaluate(fetch)` 在 Chromium 上下文里执行，复用浏览器的 TLS、cookies 和 UA — 这是绕过站点 JA3 指纹检测的唯一可靠方式。

---

## 🐛 故障排查

| 现象 | 原因 / 解决 |
|------|-------------|
| `playwright_not_installed` | `pip3 install playwright && python3 -m playwright install chromium` |
| `Failed to spawn python subprocess` | 检查 `PYTHON_BIN` 是否正确，或确认 `python3` 在 PATH |
| `login rejected` | `_username` / `_password` 错误，或站点开了图形验证码（需手动登录刷新 cookie） |
| `sign_in failed: 已经签到过` | 实际算成功，无需处理 |
| 想看浏览器窗口 | `.env` 里设 `PLAYWRIGHT_HEADLESS=false` 重跑 |

---

## 📝 注意事项

- `.env` 中 JSON 值**必须用双引号包裹**，内部双引号用 `\"` 转义，否则 dotenvy 解析失败
- `session` cookie 一般几天到几周过期；过期后只要在 `.env` 里同时配置 `_username` + `_password`，浏览器会自动登录拿新 cookie，无需手动维护
- 邮件通知仅在签到失败或余额发生变化时发送，全部成功且无变化时不发送
- 站点反爬策略可能调整，若 Chromium 也被拦，请参考 `scripts/playwright_checkin.py` 调整 UA / 浏览器参数

---

## 📄 许可证

本项目仅供学习交流使用。
