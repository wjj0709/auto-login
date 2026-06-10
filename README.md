# AnyRouter 自动签到工具 (Rust 版)

基于 Rust + Tokio 异步运行时的高性能自动签到工具，支持 AnyRouter.top / AgentRouter.org 等多站点多账号批量签到，内置 WAF JS Challenge 自动解决能力。

---

## ✨ 功能特性

- **多站点支持** — 内置 AnyRouter.top 和 AgentRouter.org 站点配置，支持通过环境变量自定义扩展
- **多账号批量签到** — 支持配置多个账号，自动依次签到
- **WAF Challenge 自动解决** — 通过 Node.js 执行 acw_sc__v2 JS Challenge，无需手动处理反爬拦截
- **余额变化检测** — 基于 SHA-256 快照比对，精准检测签到前后余额变动
- **邮件通知** — 签到失败或余额变动时自动发送 SMTP 邮件通知
- **敏感信息脱敏** — 日志输出自动对 Cookie、密码等敏感字段进行脱敏处理
- **高性能** — 全异步架构（Tokio），总执行时间 < 2 秒

---

## 🛠️ 技术栈

| 组件 | 技术 |
|------|------|
| 语言 | Rust (Edition 2021) |
| 异步运行时 | Tokio |
| HTTP 客户端 | reqwest (native-tls) |
| JS 引擎 | Node.js (系统安装) |
| 序列化 | serde + serde_json |
| 配置加载 | dotenvy |
| 邮件发送 | lettre |
| 哈希计算 | sha2 |

---

## 📦 项目结构

```
src/
├── main.rs       # 主程序入口，四阶段流水线编排
├── config.rs     # 配置管理：Provider 与账号解析
├── checkin.rs    # 签到核心逻辑：用户信息获取 + 签到 + 余额查询
├── waf.rs        # WAF JS Challenge 解决模块（Node.js 方案）
├── balance.rs    # 余额快照 Hash 比对
├── notify.rs     # SMTP 邮件通知
└── log.rs        # 统一日志格式输出
```

---

## ⚙️ 配置说明

### 1. 创建 `.env` 文件

参考 `.env.example` 创建配置文件：

```env
# 账号配置（JSON 数组，必须用双引号包裹并转义内部双引号）
ANYROUTER_ACCOUNTS="[{\"cookies\":{\"session\":\"你的session值\"},\"api_user\":\"你的api_user值\"}]"

# 可选：自定义 Provider
# PROVIDERS={"custom":{"domain":"https://example.com","sign_in_path":"/api/user/sign_in"}}

# 可选：邮件通知
# EMAIL_USER=your_email@example.com
# EMAIL_PASS=your_password
# EMAIL_TO=recipient@example.com
# EMAIL_SENDER=
# CUSTOM_SMTP_SERVER=
```

### 2. 账号字段说明

| 字段 | 必填 | 说明 |
|------|------|------|
| `cookies` | 是 | JSON 对象 `{"session":"xxx"}` 或字符串 `"session=xxx"` |
| `api_user` | 是 | API 用户标识 |
| `provider` | 否 | 站点名称，默认 `anyrouter` |
| `name` | 否 | 账号别名，用于日志显示 |

### 3. 多账号示例

```env
ANYROUTER_ACCOUNTS="[{\"cookies\":{\"session\":\"aaa\"},\"api_user\":\"111\",\"name\":\"账号A\"},{\"cookies\":{\"session\":\"bbb\"},\"api_user\":\"222\",\"provider\":\"agentrouter\",\"name\":\"账号B\"}]"
```

---

## 🚀 使用方法

### 前置要求

- **Rust** ≥ 1.70（用于编译）
- **Node.js** ≥ 18（用于执行 WAF JS Challenge）

### 编译运行

```bash
# 克隆项目
git clone <repo-url>
cd rust-version

# 复制并编辑配置
cp .env.example .env
# 编辑 .env 填入账号信息

# 编译（Release 模式）
cargo build --release

# 运行
cargo run --release
```

### 直接运行编译产物

```bash
./target/release/anyrouter-checkin.exe
```

---

## 🔄 执行流程

程序按以下四阶段流水线执行：

```
阶段 1: 加载配置
    ↓
阶段 1.5: 解决 WAF Challenge（仅对有手动签到的 Provider）
    ↓
阶段 2: 批量执行签到（逐账号处理）
    ├── Step 1: 获取签到前用户信息（余额基准）
    ├── Step 2: 执行签到 API
    └── Step 3: 获取签到后用户信息（余额比对）
    ↓
阶段 3: 余额变化检测（SHA-256 快照比对）
    ↓
阶段 4: 发送通知（失败/余额变动时触发）
```

---

## 🛡️ WAF Challenge 解决方案

AnyRouter.top 部署了 `acw_sc__v2` WAF JS Challenge，程序通过以下方式自动解决：

1. **获取 Challenge** — 向签到 API 发送请求，获取包含 JS Challenge 的 HTML 页面
2. **提取 JS** — 解析 `<script>` 标签内的 Challenge 脚本
3. **注入 DOM Stub** — 为 `document.cookie` 添加 setter 拦截，捕获计算出的 cookie 值
4. **Node.js 执行** — 调用系统 Node.js 运行修改后的 JS，提取 `acw_sc__v2` cookie
5. **合并 Cookie** — 将 WAF cookie 注入后续所有 API 请求

> ⚠️ 系统需安装 Node.js（v18+），这是执行 WAF JS 的必要依赖

---

## 📊 输出示例

```
────────────────────────────────────────────────
[PHASE] AnyRouter Auto Check-in Script (Rust)
────────────────────────────────────────────────
[PHASE] Phase 1: Loading Configuration
[SUCCESS] Configuration loaded: 2 provider(s), 1 account(s)

[PHASE] Phase 1.5: Solving WAF Challenge
[SUCCESS] [anyrouter] WAF challenge solved! acw_sc__v2=6a29f8bf... (40 chars) in 658ms

[PHASE] Phase 2: Processing 1 Account(s)
[SUCCESS] [Account 1] Check-in successful!

[PHASE] Final Summary
[INFO] Total accounts: 1
[INFO] Successful:     1/1
[INFO] Failed:         0/1
[SUCCESS] Program exited with code 0 (success)
```

---

## 📝 注意事项

- `.env` 中 JSON 值**必须用双引号包裹**，内部双引号用 `\"` 转义，否则 dotenvy 会解析失败
- `session` cookie 有效期有限，过期后需重新获取并更新 `.env`
- WAF Challenge 每次请求的 `arg1` 不同，程序每次运行都会重新计算
- 邮件通知仅在签到失败或余额发生变化时发送，全部成功时不发送

---

## 📄 许可证

本项目仅供学习交流使用。
