# CLI/GUI 统一多源配置加载 — 设计规格

## 概述

让 CLI 与 GUI 在启动时读取**同一份配置数据**。配置来自三个数据源，按固定顺序读取并去重合并：

```
环境变量(含 .env) → JSON 配置文件 → SQLite 数据库
        低优先级 ────────────────────────→ 高优先级
        后读覆盖先读（SQLite 优先级最高）
```

统一加载的数据范围：**账户凭据、站点(provider)配置、邮件通知配置**。支持通过开关只读/排除某个数据源，默认三源全开。合并后对「仅存在于 env/文件、SQLite 没有」的条目做**增量回写**到 SQLite。

**技术栈**：Rust 2024、serde_json、rusqlite（已有）。

---

## 1. 背景与现状

- **CLI**（`crates/cli`）：`main.rs` 通过 `dotenvy` 把 `.env` 灌进进程环境变量，再读 `ANYROUTER_ACCOUNTS`（账户 JSON 数组）、`PROVIDERS`（站点 JSON 对象）、`EMAIL_*`（邮件）。**完全不碰 SQLite**。
- **GUI**（`crates/gui`）：用 `core::storage`（SQLite）+ `core::env_import::import_env_if_needed()`（首次启动凭 `meta.env_imported` 标志一次性从环境变量导入 SQLite）。
- 两者数据源割裂：CLI 改不到 GUI 在用的库，反之亦然。

本设计引入统一加载层，让两端读同一份合并后的数据。

**术语统一**：本项目中 CLI 称 `provider`、core/GUI 称 `site`，二者为同一概念。本文统一用「站点 / site」。

---

## 2. 模块划分

`core` 新增 `config_loader` 模块，按职责拆分小文件：

```
crates/core/src/config_loader/
├── mod.rs          # 对外入口：load_unified(opts) + LoadOptions + UnifiedConfig + RawConfig
├── types.rs        # RawAccount / RawSite / RawEmail / RawConfig 定义
├── source.rs       # SourceKind 枚举 + ConfigSource trait
├── env_source.rs   # 环境变量(含 .env)读取 + 内置站点种子
├── file_source.rs  # JSON 配置文件读取
├── sqlite_source.rs# 从 SQLite 读取已有数据
└── merge.rs        # 去重合并 + 增量回写 SQLite
```

`crates/core/src/lib.rs` 增加 `pub mod config_loader;`。

---

## 3. 数据类型

跨源规范化类型（不含 SQLite 自增 id，便于跨源去重）：

```rust
// types.rs
#[derive(Debug, Clone)]
pub struct RawSite {
    pub name: String,
    pub domain: String,
    pub login_path: String,
    pub sign_in_path: Option<String>,
    pub user_info_path: String,
    pub tokens_path: String,
    pub logs_path: String,
    pub chart_path: String,
    pub api_user_key: String,
}

#[derive(Debug, Clone)]
pub struct RawAccount {
    pub site_name: String,        // 关联站点名（= provider）
    pub api_user: String,
    pub display_name: Option<String>,
    pub cookies: Option<String>,  // JSON 字符串或 "k=v; k=v"
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RawEmail {
    pub user: String,
    pub pass: String,
    pub to: String,
    pub sender: String,
    pub smtp_server: String,
}

#[derive(Debug, Clone, Default)]
pub struct RawConfig {
    pub sites: Vec<RawSite>,
    pub accounts: Vec<RawAccount>,
    pub email: Option<RawEmail>,
}

/// 合并去重后的最终结果
pub type UnifiedConfig = RawConfig;
```

**去重键**：
- 账户：`(site_name, api_user)`
- 站点：`name`
- 邮件：单例（无键，后源整组覆盖前源的非空字段）

`RawSite` 字段默认值（用于 env 内置种子与文件缺省）：`login_path="/login"`、`user_info_path="/api/user/self"`、`tokens_path="/api/token/"`、`logs_path="/api/log/self"`、`chart_path="/api/data/self"`、`api_user_key="new-api-user"`。

---

## 4. 源抽象

```rust
// source.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Env,
    File,
    Sqlite,
}

impl SourceKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "env" => Some(Self::Env),
            "file" => Some(Self::File),
            "sqlite" | "db" => Some(Self::Sqlite),
            _ => None,
        }
    }
}

/// 读取单个源，失败不 panic：返回 Err 由调用方记日志后跳过
pub trait ConfigSource {
    fn kind(&self) -> SourceKind;
    fn load(&self) -> anyhow::Result<RawConfig>;
}
```

### 4.1 env_source

- 内置站点种子：AnyRouter（`sign_in_path=/api/user/sign_in`）、AgentRouter（`sign_in_path=None`），其余字段取默认值。
- `PROVIDERS` 环境变量（JSON 对象）→ 覆盖/追加站点。
- `ANYROUTER_ACCOUNTS` 环境变量（JSON 数组）→ 账户。字段映射沿用现有：`provider`→`site_name`（缺省 "anyrouter"）、`name`→`display_name`、`_username`/`_password`→`username`/`password`、`cookies`→`cookies`。
- `EMAIL_USER/EMAIL_PASS/EMAIL_TO/EMAIL_SENDER/CUSTOM_SMTP_SERVER` → `RawEmail`。

### 4.2 file_source

- 路径：环境变量 `ANYROUTER_CONFIG` 优先；否则 `用户数据目录/anyrouter-checkin/config.json`（与 SQLite 同目录）。
- 文件不存在 → 返回空 `RawConfig`（视为该源无数据，不报错）。
- 存在但 JSON 非法 → 返回 `Err`（调用方记警告并跳过）。
- schema：见 §6。

### 4.3 sqlite_source

- 用 `Storage` 读 `list_sites()` + 每站点 `list_accounts_by_site()`，映射为 `RawSite`/`RawAccount`（`RawAccount.site_name` = 站点名）。
- 邮件：SQLite 暂不存邮件配置（当前库无邮件表）。本期 SQLite 源的 email 恒为 `None`。

---

## 5. 加载与合并

```rust
// mod.rs
pub struct LoadOptions {
    /// 启用的源，按读取顺序排列（靠后优先级更高）
    pub sources: Vec<SourceKind>,
    /// SQLite 数据库路径（Sqlite 源 + 增量回写用）
    pub db_path: std::path::PathBuf,
    /// 配置文件路径（None = 用默认/环境变量）
    pub config_path: Option<std::path::PathBuf>,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            sources: vec![SourceKind::Env, SourceKind::File, SourceKind::Sqlite],
            db_path: crate::storage::Storage::default_path(),
            config_path: None,
        }
    }
}

pub fn load_unified(opts: &LoadOptions) -> UnifiedConfig { ... }
```

### 5.1 合并算法（merge.rs）

用 `IndexMap`（保序）按键累加，**按 `opts.sources` 顺序依次合并，后源覆盖同键**：

- 站点：`map<name, RawSite>`，后源同名覆盖。
- 账户：`map<(site_name, api_user), RawAccount>`，后源同键覆盖。
- 邮件：从前到后，后源非空字段覆盖前源对应字段（保留前源已填、后源为空的字段）。

最终顺序：`Env`（最低）→ `File` → `Sqlite`（最高）。某源被禁用则不参与。

### 5.2 增量回写

仅当 `Sqlite` 在 `opts.sources` 中时执行：

1. 打开 `Storage`（失败 → 记警告，跳过回写，返回内存合并结果）。
2. 对合并后的**站点**：库中无同名 → `insert_site`；已有 → 不动。
3. 对合并后的**账户**：按 `site_name` 在库中查站点 `site_id`（用回写后的站点）。
   - 找到站点且库中 `(site_id, api_user)` 无对应账户 → `insert_account`；已有 → 不动。
   - 找不到对应站点（账户引用了不存在的 site_name）→ 记警告，跳过该账户。
4. 邮件：本期不回写（无邮件表）。

> 回写"不覆盖已有"保证 SQLite 高优先级值不被低优先级源回灌。回写后，SQLite 全量 ⊇ 合并结果，二者等价。

### 5.3 幂等性（废弃 env_imported）

- 删除 `env_import::import_env_if_needed()` 的「`meta.env_imported` 一次性」逻辑。
- 改为**每次启动**调用 `load_unified()`，其中的增量回写是幂等的（已存在的键不会重复插入）。
- 原 `env_import.rs` 的站点种子/账户解析逻辑迁移到 `env_source.rs`。

---

## 6. JSON 配置文件 schema

```json
{
  "sites": [
    {
      "name": "AnyRouter",
      "domain": "https://anyrouter.top",
      "login_path": "/login",
      "sign_in_path": "/api/user/sign_in",
      "user_info_path": "/api/user/self",
      "tokens_path": "/api/token/",
      "logs_path": "/api/log/self",
      "chart_path": "/api/data/self",
      "api_user_key": "new-api-user"
    }
  ],
  "accounts": [
    {
      "site_name": "AnyRouter",
      "api_user": "151687",
      "name": "教育邮箱",
      "cookies": null,
      "username": "user@example.com",
      "password": "secret"
    }
  ],
  "email": {
    "user": "bot@example.com",
    "pass": "app-password",
    "to": "me@example.com",
    "sender": "",
    "smtp_server": ""
  }
}
```

- 顶层三个字段（`sites`/`accounts`/`email`）均可选；缺省视为空。
- 站点除 `name`/`domain` 外其余字段可缺省（用 §3 默认值）。
- 用 `#[serde(default)]` 容忍缺失字段。

---

## 7. 源选择开关

### 7.1 CLI（命令行参数）

引入 `clap`（或手写极简解析，避免新依赖；本设计选手写极简解析保持零新依赖）：

- `--sources env,file,sqlite`：显式指定启用源与顺序（逗号分隔）。
- `--skip-source <name>`：排除某源，可重复（`--skip-source file --skip-source sqlite`）。
- 二者不传 → 默认 `[Env, File, Sqlite]`。
- 解析后构造 `LoadOptions.sources`。无法识别的源名 → 记警告并忽略。

### 7.2 GUI（meta 表 + 设置弹窗）

- 开关持久化在 SQLite `meta` 表：键 `cfg_sources`，值如 `env,file,sqlite`（缺省时按全开）。
- `meta` 是引导配置命名空间，**始终读取**，不属于可切换的数据源 → 无循环依赖。
- 新增「设置」入口（标题栏齿轮图标或底部栏按钮）打开设置弹窗：
  - 三个勾选框：环境变量 / 配置文件 / SQLite 数据库。
  - 至少保留一个勾选（全不选时禁用保存并提示）。
  - 保存 → 写 `meta.cfg_sources` → 重新 `load_unified()` 刷新 `AppState`。
- 启动时：先读 `meta.cfg_sources` 决定 `LoadOptions.sources`，再 `load_unified()`。
  - 注意：读 `cfg_sources` 需要打开 SQLite（仅读 meta），这与"是否把 SQLite 作为数据源"无关 —— meta 永远可读。

---

## 8. CLI / GUI 接入改造

### 8.1 CLI

- `main.rs`：删除直接读 `ANYROUTER_ACCOUNTS`/`PROVIDERS`/`EMAIL_*` 的逻辑；解析命令行参数 → `LoadOptions` → `load_unified()` → 得到 `UnifiedConfig`。
- 原 `config.rs`（AppConfig/AccountConfig）与 `notify.rs::from_env` 的 env 解析细节迁移/复用进 `core::config_loader::env_source`。CLI 改为消费 `UnifiedConfig`。
- 签到流程改为遍历 `UnifiedConfig.accounts`，按 `site_name` 找 `UnifiedConfig.sites`。

### 8.2 GUI

- `main.rs`：启动时先读 `meta.cfg_sources` → 构造 `LoadOptions` → 调 `load_unified()`（含增量回写），替代 `env_import::import_env_if_needed()`。
- **显示数据来源规则**（消除歧义，二选一按 SQLite 是否启用决定）：
  - **SQLite 源启用（默认）**：回写后 SQLite 即合并全量，GUI 仍走现有 `reload_sites()`（读 SQLite）显示站点/账户与余额统计（`account_cache`）。GUI 其余代码（CRUD、详情页、缓存）**保持不变**。这是常态路径。
  - **SQLite 源禁用**：不读不写 SQLite，GUI 用 `load_unified()` 返回的内存 `UnifiedConfig` 填充 `AppState.sites`；此时无账户 id、无 `account_cache`，余额/统计显示「—」；账户/站点 CRUD 按钮禁用并提示「需启用 SQLite 数据源后编辑」；详情页刷新不可用。
- `AppState` 增加字段标记当前是否启用 SQLite（`sqlite_enabled: bool`）以驱动上述 CRUD 禁用逻辑。
- 设置弹窗保存源开关后：写 `meta.cfg_sources` → 重新 `load_unified()` → 刷新 `AppState`。
- 删除 `env_import.rs`（逻辑并入 `config_loader::env_source` 与 `merge` 的回写）。

---

## 9. 错误处理

- 单源 `load()` 失败（文件缺失除外）：记日志警告，跳过该源，继续其余源。
- 文件不存在：静默跳过（正常场景）。
- SQLite 打不开：跳过 Sqlite 源 + 跳过回写，仅用 env/file 合并结果。
- 增量回写中单条插入失败：记警告，继续其余条目。
- 全部源为空 → `UnifiedConfig` 账户为空：CLI 报错退出（exit 1），GUI 显示空状态提示。

---

## 10. 测试

`crates/core/src/config_loader/` 下 `#[cfg(test)]` 单测：

1. **合并去重**：构造 env+file+sqlite 三组含同 `(site,api_user)` 的账户，断言去重后保留最高优先级源的值，且顺序稳定。
2. **优先级方向**：同站点名在三源有不同 domain，断言 SQLite 值生效。
3. **邮件字段级覆盖**：env 填 user，file 填 pass，断言合并后两者都在。
4. **源选择**：`sources=[Env]` 时不读 file/sqlite；`skip sqlite` 时不读不写库。
5. **增量回写**：env-only 账户被 upsert 进库；库已有 `(site,api_user)` 不被覆盖（改值后断言库值不变）。
6. **JSON 容错**：非法 JSON 文件 → `load()` 返回 Err 且整体加载不 panic。
7. **CLI 参数解析**：`--sources`/`--skip-source` 组合解析正确。

---

## 11. 范围外（本期不做）

- 邮件配置写入 SQLite（当前无邮件表；本期邮件仅来自 env/file，SQLite 源 email 恒 None）。
- 配置文件热重载（仅启动时加载）。
- 配置文件加密（敏感字段明文存于用户自管的 config.json，与 .env 同等信任级别）。
- GUI 中编辑「配置文件」内容的界面（用户手动编辑 config.json）。

---

## 12. 关键取舍记录

- **后读覆盖（SQLite 最高）**：SQLite 是 GUI 中用户增删改的「活」数据，应最高优先级；env/文件是外部注入的种子，优先级低。
- **增量回写而非全量覆盖**：保证 SQLite 高优先级值不被低优先级源回灌，同时让 env/文件新增条目能进入 GUI 可见的库。
- **废弃 env_imported 一次性标志**：改为每次启动幂等增量同步，使 env/文件后续新增的账户也能被纳入。
- **开关放 CLI 参数 / GUI meta+UI**：均为最外层控制，不用被控数据源自身配置，无循环依赖。
