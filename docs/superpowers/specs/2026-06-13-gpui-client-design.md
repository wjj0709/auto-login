# AnyRouter GPUI 可视化客户端 — 设计规格

## 概述

为现有 AnyRouter 自动签到命令行工具构建桌面 GUI 客户端。使用 GPUI 框架实现深邃黑 + 玻璃拟物感的原生桌面应用，提供站点管理、账户管理、签到执行、详情查看等完整功能。

**技术栈**：Rust 2024 edition、GPUI、Tokio、rusqlite、keyring、aes-gcm

**目标平台**：Windows（首要）、macOS/Linux（GPUI 支持即可）

---

## 1. 项目结构

Cargo workspace 组织：

```
auto-login/
├── Cargo.toml                    # [workspace] members
├── crates/
│   ├── core/                     # 共享业务逻辑
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── storage.rs        # SQLite CRUD + 迁移
│   │       ├── service.rs        # 签到/登录/拉取详情编排
│   │       ├── crypto.rs         # AES-256-GCM + keyring
│   │       ├── playwright.rs     # 子进程调用（stdin/stdout JSON）
│   │       ├── models.rs         # 数据结构定义
│   │       └── env_import.rs     # .env 一次性导入逻辑
│   ├── cli/                      # 原有 CLI 工具
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   └── gui/                      # GPUI 桌面客户端
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── app_state.rs      # 全局状态 Model
│           ├── theme.rs          # 颜色/字体/间距常量
│           ├── views/
│           │   ├── root.rs       # 顶层容器：标题栏 + 内容区 + 底部栏
│           │   ├── home.rs       # 主页：统计条 + 站点卡片网格
│           │   ├── account_detail.rs  # 账户详情页（四 Tab）
│           │   └── log_drawer.rs      # 底部日志抽屉
│           ├── components/
│           │   ├── site_card.rs       # 站点卡片组件
│           │   ├── stat_strip.rs      # 统计条组件
│           │   ├── modal.rs           # 通用弹窗容器
│           │   ├── site_form.rs       # 站点新建/编辑表单
│           │   ├── account_form.rs    # 账户新建/编辑表单
│           │   ├── account_list.rs    # 账户列表弹窗内容
│           │   ├── confirm_dialog.rs  # 删除确认弹窗
│           │   └── bar_chart.rs       # div 自绘柱状图
│           └── actions.rs        # GPUI Action 定义
├── scripts/
│   └── playwright_checkin.py     # Playwright 脚本（扩展）
└── .env
```

---

## 2. 架构分层

### 2.1 UI 层（gui crate）

使用 GPUI 的 `Render` trait 构建视图树。所有视图通过 `AppState`（全局 Model）读取数据、派发 Action。

**视图层级**：
```
Window
└── RootView
    ├── TitleBar         （应用名 + "一键签到全部"按钮）
    ├── ContentArea
    │   ├── HomeView     （默认）
    │   │   ├── StatStrip
    │   │   └── SiteCardGrid
    │   │       ├── SiteCard × N
    │   │       └── NewSiteCard（虚线占位）
    │   └── AccountDetailView（整页切换）
    │       ├── Breadcrumb
    │       ├── TabBar（概览/API密钥/使用日志/消耗图表）
    │       └── TabContent
    ├── BottomBar        （状态文字 + 日志按钮）
    ├── LogDrawer        （底部抽屉，条件渲染）
    └── ModalLayer       （弹窗层，条件渲染）
        ├── AccountListModal
        ├── SiteFormModal
        ├── AccountFormModal
        └── ConfirmDialog
```

### 2.2 状态层（gui crate - app_state.rs）

```rust
pub struct AppState {
    // 视图状态
    pub current_view: ViewKind,           // Home | AccountDetail(account_id)
    pub log_drawer_open: bool,
    pub active_modal: Option<ModalKind>,

    // 数据
    pub sites: Vec<SiteWithStats>,        // 站点 + 账户数/余额合计/过期警告
    pub current_account: Option<AccountDetail>,
    pub log_entries: Vec<LogEntry>,

    // 运行状态
    pub running: bool,
    pub run_progress: Option<String>,     // "签到中 2/5..."
}
```

状态变更通过 `cx.notify()` 驱动 UI 重新渲染。

### 2.3 服务层（core crate - service.rs）

三类核心操作：

| 操作 | 触发场景 | 流程 |
|---|---|---|
| `checkin_all()` | 标题栏"一键签到" | 遍历所有账户 → 检查 Cookie → 组装 payload → 调用 Playwright(checkin) → 更新库 |
| `checkin_site(site_id)` | 站点卡片"签到本站点" | 同上，但仅该站点下账户 |
| `login_account(account_id)` | "账密登录刷新" | 调用 Playwright(login) → 更新 Cookie + 时间 |
| `fetch_detail(account_id)` | 详情页"刷新数据" | 调用 Playwright(fetch_detail) → 写 account_cache |

所有操作在 Tokio 后台任务中执行，通过 `cx.update_model()` 回写状态。

### 2.4 存储层（core crate - storage.rs）

SQLite 单文件数据库，位于用户数据目录：
- Windows: `%APPDATA%/anyrouter-checkin/data.db`
- macOS: `~/Library/Application Support/anyrouter-checkin/data.db`
- Linux: `~/.local/share/anyrouter-checkin/data.db`

**表结构**：

#### sites
| 字段 | 类型 | 说明 |
|---|---|---|
| id | INTEGER PK | 自增主键 |
| name | TEXT UNIQUE | 展示名 |
| domain | TEXT | 站点域名 |
| login_path | TEXT | 默认 `/login` |
| sign_in_path | TEXT NULL | 空 = 自动签到型 |
| user_info_path | TEXT | 默认 `/api/user/self` |
| tokens_path | TEXT | 默认 `/api/token/` |
| logs_path | TEXT | 默认 `/api/log/self` |
| chart_path | TEXT | 默认 `/api/data/self` |
| api_user_key | TEXT | 默认 `new-api-user` |
| created_at | TEXT | ISO8601 |
| updated_at | TEXT | ISO8601 |

#### accounts
| 字段 | 类型 | 说明 |
|---|---|---|
| id | INTEGER PK | 自增主键 |
| site_id | INTEGER FK | → sites.id (CASCADE DELETE) |
| name | TEXT | 展示名 (site_id, name) UNIQUE |
| api_user | TEXT | 请求头值 |
| username_enc | BLOB NULL | AES-GCM 加密 |
| password_enc | BLOB NULL | AES-GCM 加密 |
| cookies_enc | BLOB NULL | AES-GCM 加密 |
| cookie_issued_at | TEXT NULL | Cookie 获取时间 |
| cookie_expires_at | TEXT NULL | Cookie 过期时间 |
| created_at | TEXT | ISO8601 |
| updated_at | TEXT | ISO8601 |

#### account_cache
| 字段 | 类型 | 说明 |
|---|---|---|
| account_id | INTEGER FK | → accounts.id (CASCADE DELETE) |
| kind | TEXT | overview / tokens / logs / chart |
| payload_json | TEXT | 脱敏后的 JSON |
| fetched_at | TEXT | 拉取时间 |

主键：(account_id, kind)

#### meta
| 字段 | 类型 | 说明 |
|---|---|---|
| key | TEXT PK | 键名 |
| value | TEXT | 值 |

预置键：`schema_version`、`env_imported`

### 2.5 加密层（core crate - crypto.rs）

- **主密钥管理**：使用 `keyring` crate 在 Windows Credential Manager 中存取 256-bit 主密钥
- **首次启动**：检测 keyring 中无密钥时，随机生成并存入
- **加密算法**：AES-256-GCM，每次加密生成随机 96-bit nonce
- **存储格式**：`nonce (12 bytes) || ciphertext || tag (16 bytes)`

### 2.6 Playwright 协议（core crate - playwright.rs + scripts/）

**输入格式**（stdin JSON）：
```json
{
  "action": "checkin" | "login" | "fetch_detail",
  "headless": true,
  "timeout_ms": 30000,
  "accounts": [...]
}
```

**输出格式**（stdout JSON）：

所有任务回传 `cookies[]`：
```json
{
  "results": [{
    "name": "...",
    "success": true,
    "cookies": [
      {"name": "session", "value": "...", "expires": 1750000000, "domain": "...", "path": "/", "httpOnly": true, "secure": true}
    ],
    // action-specific fields...
  }]
}
```

action-specific 字段：
- **checkin**：`before`, `after`, `user_info`, `error`, `used_login`
- **login**：`user_info`, `error`
- **fetch_detail**：`user_info`, `tokens[]`, `logs[]`, `chart[]`, `error`

---

## 3. 视图交互设计

### 3.1 主页 (HomeView)

- **统计条**：站点数 | 账户数 | 总余额 | 今日签到进度
- **站点卡片网格**：每个卡片显示站点名、域名、账户数（含过期警告）、余额合计
- **卡片操作**：「查看账户」→ 弹出 AccountListModal | 「编辑」→ SiteFormModal | 「删除」→ ConfirmDialog
- **新建站点**：虚线边框占位卡片，点击弹出 SiteFormModal

### 3.2 账户列表弹窗 (AccountListModal)

- 每行显示：账户名 · 余额 · Cookie 状态（有效/过期/未知）
- 行操作：[详情] → 切换到 AccountDetailView | [编辑] → AccountFormModal | [删除] → ConfirmDialog
- 底部：「+ 新增账户」「⚡ 签到本站点」

### 3.3 账户详情页 (AccountDetailView)

整页切换，面包屑导航"◀ 返回主页 / 用户名 @ 站点名"

四个 Tab：
1. **概览**：余额/累计消耗/请求数/用户信息、Cookie 状态卡片（含"账密登录刷新"按钮）
2. **API 密钥**：密钥列表（名称、key 打码、额度/已用、状态、过期时间）
3. **使用日志**：最近 50 条（时间、模型、tokens、消耗、耗时）
4. **消耗图表**：近 7 天柱状图（div 自绘） + 模型消耗 Top 排行

顶部操作：「⚡ 签到」「↻ 刷新数据」

### 3.4 日志抽屉 (LogDrawer)

- 底部滑出，高度约窗口 30%
- 实时显示签到/登录/拉取过程中的日志
- 操作：[清空] [关闭]
- 底部栏图标在运行中时带脉冲动画

### 3.5 表单设计

**站点表单**：
- 必填：站点名称、域名（http(s):// 校验）
- 高级设置（默认收起）：login_path、sign_in_path、user_info_path、tokens_path、logs_path、chart_path、api_user_key

**账户表单**：
- 必填：账户名称、API 用户标识
- 凭据（至少填一种）：
  - 方式一：粘贴 Cookie（k=v; 串或 JSON 对象）
  - 方式二：用户名 + 密码
- 额外按钮：「登录获取 Cookie」— 保存后立即触发 login 任务

---

## 4. Cookie 生命周期

### 4.1 三种来源

| 来源 | 触发 | issued_at | expires_at |
|---|---|---|---|
| 手动录入 | 账户表单粘贴 | 录入时刻 | 空（显示"有效期未知"） |
| 账密登录 | 表单"登录获取"或详情页"刷新" | 登录时刻 | session cookie 的 expires |
| 任务顺带续期 | 每次 checkin/fetch_detail 回传 | 任务完成时刻 | 回传的 expires |

### 4.2 过期检查

任务执行前：
- 未过期 → 直接执行
- 已过期且有账密 → 自动先 login 再继续（用户无感）
- 已过期且无账密 → 中止，标 ⚠ 提示

---

## 5. 视觉规范

### 5.1 色板

| 用途 | 色值 |
|---|---|
| 窗口底色 | #0a0c10 |
| 卡片/面板底色 | #141923 |
| 标题栏/底部栏 | #11141a |
| 边框常规 | #2a2f3a |
| 边框强调 | #3a4150 |
| 文字主色 | #dbe2ec |
| 文字次级 | #aab2c0 |
| 文字弱化 | #8a93a5 |
| 文字最弱 | #56657d |
| 主题蓝 | #7fb0ff |
| 成功绿 | #6fbf73 |
| 错误红 | #e07b7b |
| 警告黄 | #dcaa50 |

### 5.2 玻璃拟物感实现

- 卡片：`background: #141923`、`border: 1px solid #2a2f3a`、`border-radius: 8px`
- 弹窗：`background: #11141a`、`border: 1px solid #3a4150`、`border-radius: 10px`、`box-shadow: 0 8px 30px rgba(0,0,0,0.6)`
- 按钮主色：`background: rgba(80,140,255,0.2)`、`border: 1px solid rgba(80,140,255,0.5)`
- 按钮危险：`background: rgba(220,110,110,0.12)`、`border: 1px solid rgba(220,110,110,0.45)`

### 5.3 排版

- 标题：13px、#e2e8f2
- 正文：12px、#aab2c0
- 辅助：10-11px、#8a93a5
- 日志：等宽字体、10px、#7a8496

---

## 6. 首次启动流程

1. 检测 SQLite 数据库是否存在 → 不存在则创建并执行迁移
2. 检测 `meta.env_imported` 是否已设置
3. 若未设置：
   - 写入内置站点（AnyRouter、AgentRouter）到 sites
   - 读取 `ANYROUTER_ACCOUNTS` 环境变量，解析并导入 accounts
   - 设置 `meta.env_imported = "true"`
4. 此后环境变量不再参与运行，所有配置从 SQLite 读取

---

## 7. 错误处理策略

- **网络错误**：日志记录 + 卡片/详情页状态标记"最后尝试失败"
- **Playwright 子进程崩溃**：捕获 exit code + stderr，日志显示
- **Cookie 过期**：卡片显示 ⚠ 图标，提示"Cookie 已过期"
- **加密/解密失败**：keyring 不可用时降级为明文存储 + 日志警告
- **数据库锁冲突**：CLI 和 GUI 不应同时写入，GUI 启动时检测

---

## 8. 非功能性需求

- **窗口尺寸**：默认 1100×750，最小 900×600
- **响应性**：所有网络/IO 操作在后台 Tokio 任务中执行，UI 线程不阻塞
- **文案语言**：全部中文
- **编码**：Python 脚本输出强制 UTF-8

---

## 9. CLI 迁移策略

CLI crate 保持现有功能不变，但内部实现改为调用 core crate：
- `core::service::checkin_all()` 替代原有的 `playwright::run_checkin()`
- `core::storage` 替代原有的 `.env` 直接读取（CLI 仍支持纯 .env 模式作为后备）
- 迁移分两步：先让 GUI 独立工作，再逐步让 CLI 也用 core（可在后续迭代）

**本期优先级**：GUI 客户端为主，CLI 重构为次要目标。初期 CLI 保持原样不动，core 的 playwright.rs 从现有 cli/src/playwright.rs 提取而来。

---

## 10. 范围外（不在本期实现）

- 定时签到调度（cron）
- 多窗口支持
- 系统托盘
- 自动更新
- 国际化
