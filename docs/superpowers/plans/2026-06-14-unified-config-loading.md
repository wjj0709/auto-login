# CLI/GUI 统一多源配置加载 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 CLI 与 GUI 启动时读取同一份配置（账户/站点/邮件），来源按 env → JSON 配置文件 → SQLite 顺序合并去重（后读覆盖、SQLite 最高），并把 env/文件新条目增量回写 SQLite；源可经 CLI 参数 / GUI 开关切换。

**Architecture:** `core` 新增 `config_loader` 模块：`types`（规范化中间类型）+ 三个 `*_source`（env/file/sqlite 读取）+ `merge`（去重合并 + 增量回写）+ `mod`（`load_unified` 入口）。CLI/GUI 都调用 `load_unified(&LoadOptions)`。废弃 `env_import` 的一次性导入，改为每次启动幂等增量同步。

**Tech Stack:** Rust 2024、serde/serde_json、rusqlite（均已有，零新依赖）。

---

## 文件结构

```
crates/core/src/config_loader/
├── mod.rs          # load_unified + LoadOptions + 重导出
├── types.rs        # RawSite / RawAccount / RawEmail / RawConfig
├── source.rs       # SourceKind + ConfigSource trait
├── env_source.rs   # 环境变量 + 内置站点种子
├── file_source.rs  # JSON 配置文件
├── sqlite_source.rs# 从 SQLite 读取
└── merge.rs        # 合并去重 + 增量回写
crates/core/src/lib.rs            # 注册 config_loader，移除 env_import
crates/cli/Cargo.toml             # 新增 anyrouter-core 依赖
crates/cli/src/main.rs            # 改用 load_unified
crates/cli/src/cli_args.rs        # 命令行源选择参数解析（新增）
crates/gui/src/main.rs            # 改用 load_unified + meta.cfg_sources
crates/gui/src/app_state.rs       # 增加 sqlite_enabled 字段
crates/gui/src/views/settings.rs  # 设置弹窗（新增）
crates/gui/src/views/root.rs      # 标题栏齿轮入口 + 设置弹窗渲染
```

---

## Phase 1: core — 中间类型与单源读取

### Task 1: 定义规范化中间类型

**Files:**
- Create: `crates/core/src/config_loader/types.rs`
- Create: `crates/core/src/config_loader/mod.rs`
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: 注册模块**

`crates/core/src/lib.rs` 改为（保留 env_import 暂不删，最后阶段删）：

```rust
pub mod models;
pub mod storage;
pub mod crypto;
pub mod playwright;
pub mod service;
pub mod env_import;
pub mod config_loader;
```

- [ ] **Step 2: 写 mod.rs 骨架（先只挂子模块）**

`crates/core/src/config_loader/mod.rs`：

```rust
pub mod types;

pub use types::{RawAccount, RawConfig, RawEmail, RawSite};
```

- [ ] **Step 3: 写 types.rs**

`crates/core/src/config_loader/types.rs`：

```rust
//! 跨源规范化配置类型（不含 SQLite 自增 id，便于跨源去重）。

/// 站点配置（= CLI 的 provider）
#[derive(Debug, Clone, PartialEq)]
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

impl RawSite {
    /// 用 new-api 系默认路径构造站点，仅需提供 name/domain/sign_in_path。
    pub fn with_defaults(name: impl Into<String>, domain: impl Into<String>, sign_in_path: Option<String>) -> Self {
        Self {
            name: name.into(),
            domain: domain.into(),
            login_path: "/login".to_string(),
            sign_in_path,
            user_info_path: "/api/user/self".to_string(),
            tokens_path: "/api/token/".to_string(),
            logs_path: "/api/log/self".to_string(),
            chart_path: "/api/data/self".to_string(),
            api_user_key: "new-api-user".to_string(),
        }
    }
}

/// 账户凭据
#[derive(Debug, Clone, PartialEq)]
pub struct RawAccount {
    pub site_name: String,
    pub api_user: String,
    pub display_name: Option<String>,
    pub cookies: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl RawAccount {
    /// 去重键：(站点名, api_user)
    pub fn key(&self) -> (String, String) {
        (self.site_name.clone(), self.api_user.clone())
    }
}

/// 邮件通知配置（单例）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawEmail {
    pub user: String,
    pub pass: String,
    pub to: String,
    pub sender: String,
    pub smtp_server: String,
}

impl RawEmail {
    pub fn is_empty(&self) -> bool {
        self.user.is_empty() && self.pass.is_empty() && self.to.is_empty()
    }

    /// 用后源的非空字段覆盖 self 的对应字段（字段级合并）。
    pub fn overlay(&mut self, other: &RawEmail) {
        if !other.user.is_empty() { self.user = other.user.clone(); }
        if !other.pass.is_empty() { self.pass = other.pass.clone(); }
        if !other.to.is_empty() { self.to = other.to.clone(); }
        if !other.sender.is_empty() { self.sender = other.sender.clone(); }
        if !other.smtp_server.is_empty() { self.smtp_server = other.smtp_server.clone(); }
    }
}

/// 单源读取结果 / 合并最终结果
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawConfig {
    pub sites: Vec<RawSite>,
    pub accounts: Vec<RawAccount>,
    pub email: Option<RawEmail>,
}

pub type UnifiedConfig = RawConfig;
```

- [ ] **Step 4: 写 RawEmail::overlay 单测**

在 `types.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_overlay_keeps_filled_and_applies_nonempty() {
        let mut base = RawEmail { user: "a@x.com".into(), ..Default::default() };
        let next = RawEmail { pass: "secret".into(), ..Default::default() };
        base.overlay(&next);
        assert_eq!(base.user, "a@x.com"); // 前源保留
        assert_eq!(base.pass, "secret");  // 后源补上
    }

    #[test]
    fn account_key_is_site_and_api_user() {
        let a = RawAccount {
            site_name: "AnyRouter".into(),
            api_user: "151687".into(),
            display_name: None, cookies: None, username: None, password: None,
        };
        assert_eq!(a.key(), ("AnyRouter".to_string(), "151687".to_string()));
    }
}
```

- [ ] **Step 5: 运行测试**

Run: `cargo test -p anyrouter-core config_loader::types`
Expected: 2 passed。

- [ ] **Step 6: 提交**

```bash
git add crates/core/src/lib.rs crates/core/src/config_loader/
git commit -m "feat(core): config_loader 规范化中间类型（RawSite/RawAccount/RawEmail）"
```

---

### Task 2: SourceKind 与 ConfigSource trait

**Files:**
- Create: `crates/core/src/config_loader/source.rs`
- Modify: `crates/core/src/config_loader/mod.rs`

- [ ] **Step 1: 写 source.rs**

```rust
use anyhow::Result;
use super::types::RawConfig;

/// 数据源种类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Env,
    File,
    Sqlite,
}

impl SourceKind {
    /// 解析源名（大小写不敏感）；db 是 sqlite 的别名。
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "env" => Some(Self::Env),
            "file" => Some(Self::File),
            "sqlite" | "db" => Some(Self::Sqlite),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::File => "file",
            Self::Sqlite => "sqlite",
        }
    }
}

/// 单个数据源；load 失败由调用方记日志后跳过，不中断整体加载。
pub trait ConfigSource {
    fn kind(&self) -> SourceKind;
    fn load(&self) -> Result<RawConfig>;
}
```

- [ ] **Step 2: 在 mod.rs 挂载**

`mod.rs` 增加：

```rust
pub mod source;
pub use source::{ConfigSource, SourceKind};
```

- [ ] **Step 3: 写 SourceKind::parse 单测**

`source.rs` 末尾：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_source_names() {
        assert_eq!(SourceKind::parse("ENV"), Some(SourceKind::Env));
        assert_eq!(SourceKind::parse(" file "), Some(SourceKind::File));
        assert_eq!(SourceKind::parse("db"), Some(SourceKind::Sqlite));
        assert_eq!(SourceKind::parse("sqlite"), Some(SourceKind::Sqlite));
        assert_eq!(SourceKind::parse("xxx"), None);
    }
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test -p anyrouter-core config_loader::source`
Expected: 1 passed。

- [ ] **Step 5: 提交**

```bash
git add crates/core/src/config_loader/source.rs crates/core/src/config_loader/mod.rs
git commit -m "feat(core): config_loader SourceKind 与 ConfigSource trait"
```

---

### Task 3: 环境变量源（含内置站点种子）

**Files:**
- Create: `crates/core/src/config_loader/env_source.rs`
- Modify: `crates/core/src/config_loader/mod.rs`

- [ ] **Step 1: 写 env_source.rs**

把现有 `env_import.rs` 的内置站点 + `ANYROUTER_ACCOUNTS` 解析逻辑迁移到这里，并补 `PROVIDERS` 与 `EMAIL_*`：

```rust
use anyhow::Result;
use serde_json::Value;

use super::source::{ConfigSource, SourceKind};
use super::types::{RawAccount, RawConfig, RawEmail, RawSite};

pub struct EnvSource;

impl EnvSource {
    pub fn new() -> Self {
        Self
    }

    /// 内置站点种子：AnyRouter（手动签到）+ AgentRouter（自动签到）。
    fn builtin_sites() -> Vec<RawSite> {
        vec![
            RawSite::with_defaults(
                "AnyRouter",
                "https://anyrouter.top",
                Some("/api/user/sign_in".to_string()),
            ),
            RawSite::with_defaults("AgentRouter", "https://agentrouter.org", None),
        ]
    }

    /// 解析 PROVIDERS 环境变量（JSON 对象：{ key: {domain, sign_in_path?, ...} }）。
    fn parse_providers(raw: &str, sites: &mut Vec<RawSite>) {
        let Ok(map) = serde_json::from_str::<serde_json::Map<String, Value>>(raw) else {
            return;
        };
        for (key, v) in map {
            let name = v.get("name").and_then(|x| x.as_str()).unwrap_or(&key).to_string();
            let domain = v.get("domain").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if domain.is_empty() {
                continue;
            }
            let sign_in_path = v
                .get("sign_in_path")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            let mut site = RawSite::with_defaults(name.clone(), domain, sign_in_path);
            if let Some(s) = v.get("login_path").and_then(|x| x.as_str()) { site.login_path = s.to_string(); }
            if let Some(s) = v.get("user_info_path").and_then(|x| x.as_str()) { site.user_info_path = s.to_string(); }
            if let Some(s) = v.get("tokens_path").and_then(|x| x.as_str()) { site.tokens_path = s.to_string(); }
            if let Some(s) = v.get("logs_path").and_then(|x| x.as_str()) { site.logs_path = s.to_string(); }
            if let Some(s) = v.get("chart_path").and_then(|x| x.as_str()) { site.chart_path = s.to_string(); }
            if let Some(s) = v.get("api_user_key").and_then(|x| x.as_str()) { site.api_user_key = s.to_string(); }
            // 同名覆盖内置种子
            sites.retain(|x| x.name != site.name);
            sites.push(site);
        }
    }

    /// 解析 ANYROUTER_ACCOUNTS 环境变量（JSON 数组）。
    fn parse_accounts(raw: &str) -> Vec<RawAccount> {
        let Ok(arr) = serde_json::from_str::<Vec<Value>>(raw) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in arr {
            let api_user = entry.get("api_user").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if api_user.is_empty() {
                continue;
            }
            let provider = entry.get("provider").and_then(|v| v.as_str()).unwrap_or("anyrouter");
            // provider 名归一化到内置站点显示名
            let site_name = match provider.to_lowercase().as_str() {
                "agentrouter" => "AgentRouter".to_string(),
                "anyrouter" => "AnyRouter".to_string(),
                other => other.to_string(),
            };
            let display_name = entry.get("name").and_then(|v| v.as_str())
                .filter(|s| !s.is_empty()).map(|s| s.to_string());
            let username = entry.get("_username").and_then(|v| v.as_str()).map(|s| s.to_string());
            let password = entry.get("_password").and_then(|v| v.as_str()).map(|s| s.to_string());
            let cookies = entry.get("cookies").and_then(|v| {
                if v.is_null() { return None; }
                let s = serde_json::to_string(v).ok()?;
                if s == "\"\"" || s == "[]" || s == "null" || s == "{}" { None } else { Some(s) }
            });
            out.push(RawAccount { site_name, api_user, display_name, cookies, username, password });
        }
        out
    }

    fn parse_email() -> Option<RawEmail> {
        let email = RawEmail {
            user: std::env::var("EMAIL_USER").unwrap_or_default(),
            pass: std::env::var("EMAIL_PASS").unwrap_or_default(),
            to: std::env::var("EMAIL_TO").unwrap_or_default(),
            sender: std::env::var("EMAIL_SENDER").unwrap_or_default(),
            smtp_server: std::env::var("CUSTOM_SMTP_SERVER").unwrap_or_default(),
        };
        if email.is_empty() { None } else { Some(email) }
    }
}

impl ConfigSource for EnvSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Env
    }

    fn load(&self) -> Result<RawConfig> {
        let mut sites = Self::builtin_sites();
        if let Ok(raw) = std::env::var("PROVIDERS") {
            if !raw.trim().is_empty() {
                Self::parse_providers(&raw, &mut sites);
            }
        }
        let accounts = std::env::var("ANYROUTER_ACCOUNTS")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(|s| Self::parse_accounts(&s))
            .unwrap_or_default();
        Ok(RawConfig { sites, accounts, email: Self::parse_email() })
    }
}
```

- [ ] **Step 2: 在 mod.rs 挂载**

```rust
pub mod env_source;
pub use env_source::EnvSource;
```

- [ ] **Step 3: 写账户解析单测**

`env_source.rs` 末尾：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accounts_maps_provider_and_underscore_fields() {
        let raw = r#"[{"api_user":"151687","provider":"agentrouter","name":"教育邮箱","_username":"u","_password":"p"}]"#;
        let accts = EnvSource::parse_accounts(raw);
        assert_eq!(accts.len(), 1);
        assert_eq!(accts[0].site_name, "AgentRouter");
        assert_eq!(accts[0].api_user, "151687");
        assert_eq!(accts[0].display_name.as_deref(), Some("教育邮箱"));
        assert_eq!(accts[0].username.as_deref(), Some("u"));
        assert_eq!(accts[0].password.as_deref(), Some("p"));
    }

    #[test]
    fn parse_accounts_skips_empty_api_user() {
        let raw = r#"[{"api_user":"","name":"x"}]"#;
        assert_eq!(EnvSource::parse_accounts(raw).len(), 0);
    }

    #[test]
    fn builtin_sites_have_correct_schema() {
        let sites = EnvSource::builtin_sites();
        let any = sites.iter().find(|s| s.name == "AnyRouter").unwrap();
        assert_eq!(any.api_user_key, "new-api-user");
        assert_eq!(any.user_info_path, "/api/user/self");
        assert_eq!(any.sign_in_path.as_deref(), Some("/api/user/sign_in"));
        let agent = sites.iter().find(|s| s.name == "AgentRouter").unwrap();
        assert_eq!(agent.sign_in_path, None);
    }
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test -p anyrouter-core config_loader::env_source`
Expected: 3 passed。

- [ ] **Step 5: 提交**

```bash
git add crates/core/src/config_loader/env_source.rs crates/core/src/config_loader/mod.rs
git commit -m "feat(core): config_loader 环境变量源（内置站点种子 + PROVIDERS + 账户 + 邮件）"
```

---

### Task 4: JSON 配置文件源

**Files:**
- Create: `crates/core/src/config_loader/file_source.rs`
- Modify: `crates/core/src/config_loader/mod.rs`

- [ ] **Step 1: 写 file_source.rs**

```rust
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::source::{ConfigSource, SourceKind};
use super::types::{RawAccount, RawConfig, RawEmail, RawSite};

/// 配置文件中的站点（字段可缺省）
#[derive(Debug, Deserialize)]
struct FileSite {
    name: String,
    domain: String,
    #[serde(default = "default_login_path")]
    login_path: String,
    #[serde(default)]
    sign_in_path: Option<String>,
    #[serde(default = "default_user_info_path")]
    user_info_path: String,
    #[serde(default = "default_tokens_path")]
    tokens_path: String,
    #[serde(default = "default_logs_path")]
    logs_path: String,
    #[serde(default = "default_chart_path")]
    chart_path: String,
    #[serde(default = "default_api_user_key")]
    api_user_key: String,
}

fn default_login_path() -> String { "/login".into() }
fn default_user_info_path() -> String { "/api/user/self".into() }
fn default_tokens_path() -> String { "/api/token/".into() }
fn default_logs_path() -> String { "/api/log/self".into() }
fn default_chart_path() -> String { "/api/data/self".into() }
fn default_api_user_key() -> String { "new-api-user".into() }

#[derive(Debug, Deserialize)]
struct FileAccount {
    site_name: String,
    api_user: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    cookies: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct FileEmail {
    #[serde(default)]
    user: String,
    #[serde(default)]
    pass: String,
    #[serde(default)]
    to: String,
    #[serde(default)]
    sender: String,
    #[serde(default)]
    smtp_server: String,
}

#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    #[serde(default)]
    sites: Vec<FileSite>,
    #[serde(default)]
    accounts: Vec<FileAccount>,
    #[serde(default)]
    email: Option<FileEmail>,
}

pub struct FileSource {
    path: PathBuf,
}

impl FileSource {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// 默认路径：ANYROUTER_CONFIG 优先，否则 SQLite 同目录下 config.json。
    pub fn default_path() -> PathBuf {
        if let Ok(p) = std::env::var("ANYROUTER_CONFIG") {
            if !p.trim().is_empty() {
                return PathBuf::from(p);
            }
        }
        crate::storage::Storage::default_path()
            .parent()
            .map(|d| d.join("config.json"))
            .unwrap_or_else(|| PathBuf::from("config.json"))
    }
}

impl ConfigSource for FileSource {
    fn kind(&self) -> SourceKind {
        SourceKind::File
    }

    fn load(&self) -> Result<RawConfig> {
        if !self.path.exists() {
            // 文件不存在视为空源，不报错
            return Ok(RawConfig::default());
        }
        let text = std::fs::read_to_string(&self.path)
            .with_context(|| format!("读取配置文件失败: {}", self.path.display()))?;
        let parsed: FileConfig = serde_json::from_str(&text)
            .with_context(|| format!("配置文件 JSON 解析失败: {}", self.path.display()))?;

        let sites = parsed.sites.into_iter().map(|s| RawSite {
            name: s.name,
            domain: s.domain,
            login_path: s.login_path,
            sign_in_path: s.sign_in_path,
            user_info_path: s.user_info_path,
            tokens_path: s.tokens_path,
            logs_path: s.logs_path,
            chart_path: s.chart_path,
            api_user_key: s.api_user_key,
        }).collect();

        let accounts = parsed.accounts.into_iter()
            .filter(|a| !a.api_user.is_empty())
            .map(|a| RawAccount {
                site_name: a.site_name,
                api_user: a.api_user,
                display_name: a.name,
                cookies: a.cookies,
                username: a.username,
                password: a.password,
            }).collect();

        let email = parsed.email.map(|e| RawEmail {
            user: e.user, pass: e.pass, to: e.to, sender: e.sender, smtp_server: e.smtp_server,
        }).filter(|e| !e.is_empty());

        Ok(RawConfig { sites, accounts, email })
    }
}
```

- [ ] **Step 2: 在 mod.rs 挂载**

```rust
pub mod file_source;
pub use file_source::FileSource;
```

- [ ] **Step 3: 写文件解析单测（用临时文件）**

`file_source.rs` 末尾：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn missing_file_is_empty_source() {
        let src = FileSource::new(PathBuf::from("/no/such/file_xyz.json"));
        let cfg = src.load().unwrap();
        assert!(cfg.sites.is_empty() && cfg.accounts.is_empty());
    }

    #[test]
    fn parses_valid_json_with_defaults() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, r#"{{"sites":[{{"name":"S","domain":"https://s.com"}}],"accounts":[{{"site_name":"S","api_user":"1","name":"n"}}]}}"#).unwrap();
        let src = FileSource::new(f.path().to_path_buf());
        let cfg = src.load().unwrap();
        assert_eq!(cfg.sites.len(), 1);
        assert_eq!(cfg.sites[0].api_user_key, "new-api-user"); // 默认值
        assert_eq!(cfg.accounts[0].site_name, "S");
    }

    #[test]
    fn invalid_json_returns_err() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "{{ not json").unwrap();
        let src = FileSource::new(f.path().to_path_buf());
        assert!(src.load().is_err());
    }
}
```

- [ ] **Step 2.5: 给 core 添加 dev 依赖 tempfile**

`crates/core/Cargo.toml` 增加：

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: 运行测试**

Run: `cargo test -p anyrouter-core config_loader::file_source`
Expected: 3 passed。

- [ ] **Step 5: 提交**

```bash
git add crates/core/src/config_loader/file_source.rs crates/core/src/config_loader/mod.rs crates/core/Cargo.toml
git commit -m "feat(core): config_loader JSON 配置文件源（缺省容错 + 非法 JSON 报错）"
```

---

### Task 5: SQLite 源

**Files:**
- Create: `crates/core/src/config_loader/sqlite_source.rs`
- Modify: `crates/core/src/config_loader/mod.rs`

- [ ] **Step 1: 写 sqlite_source.rs**

```rust
use std::path::PathBuf;

use anyhow::Result;

use super::source::{ConfigSource, SourceKind};
use super::types::{RawAccount, RawConfig, RawSite};
use crate::storage::Storage;

pub struct SqliteSource {
    db_path: PathBuf,
}

impl SqliteSource {
    pub fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }
}

impl ConfigSource for SqliteSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Sqlite
    }

    fn load(&self) -> Result<RawConfig> {
        let storage = Storage::open(&self.db_path)?;
        let sites_db = storage.list_sites()?;

        let mut sites = Vec::new();
        let mut accounts = Vec::new();
        for s in &sites_db {
            sites.push(RawSite {
                name: s.name.clone(),
                domain: s.domain.clone(),
                login_path: s.login_path.clone(),
                sign_in_path: s.sign_in_path.clone(),
                user_info_path: s.user_info_path.clone(),
                tokens_path: s.tokens_path.clone(),
                logs_path: s.logs_path.clone(),
                chart_path: s.chart_path.clone(),
                api_user_key: s.api_user_key.clone(),
            });
            for a in storage.list_accounts_by_site(s.id)? {
                accounts.push(RawAccount {
                    site_name: s.name.clone(),
                    api_user: a.api_user,
                    display_name: Some(a.name),
                    cookies: a.cookies,
                    username: a.username,
                    password: a.password,
                });
            }
        }
        // SQLite 当前不存邮件配置
        Ok(RawConfig { sites, accounts, email: None })
    }
}
```

- [ ] **Step 2: 在 mod.rs 挂载**

```rust
pub mod sqlite_source;
pub use sqlite_source::SqliteSource;
```

- [ ] **Step 3: 写 SQLite 往返单测（临时库）**

`sqlite_source.rs` 末尾：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AccountInput, SiteInput};

    #[test]
    fn reads_sites_and_accounts_from_db() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("t.db");
        {
            let storage = Storage::open(&db).unwrap();
            let sid = storage.insert_site(&SiteInput {
                name: "S".into(), domain: "https://s.com".into(),
                login_path: "/login".into(), sign_in_path: None,
                user_info_path: "/api/user/self".into(), tokens_path: "/api/token/".into(),
                logs_path: "/api/log/self".into(), chart_path: "/api/data/self".into(),
                api_user_key: "new-api-user".into(),
            }).unwrap();
            storage.insert_account(&AccountInput {
                site_id: sid, name: "acc".into(), api_user: "99".into(),
                username: None, password: None, cookies: None,
            }).unwrap();
        }
        let cfg = SqliteSource::new(db).load().unwrap();
        assert_eq!(cfg.sites.len(), 1);
        assert_eq!(cfg.accounts.len(), 1);
        assert_eq!(cfg.accounts[0].site_name, "S");
        assert_eq!(cfg.accounts[0].api_user, "99");
    }
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test -p anyrouter-core config_loader::sqlite_source`
Expected: 1 passed。

- [ ] **Step 5: 提交**

```bash
git add crates/core/src/config_loader/sqlite_source.rs crates/core/src/config_loader/mod.rs
git commit -m "feat(core): config_loader SQLite 源（读站点与账户）"
```

---

## Phase 2: 合并去重与 load_unified

### Task 6: 合并去重

**Files:**
- Create: `crates/core/src/config_loader/merge.rs`
- Modify: `crates/core/src/config_loader/mod.rs`

- [ ] **Step 1: 写 merge.rs 的合并函数**

```rust
use std::collections::BTreeMap;

use super::types::{RawAccount, RawConfig, RawEmail, RawSite};

/// 按读取顺序合并多个源（靠后的覆盖同键），返回去重后的配置。
/// 调用方需保证 configs 的顺序即优先级从低到高。
pub fn merge_configs(configs: &[RawConfig]) -> RawConfig {
    // 站点按 name 去重，后源覆盖
    let mut sites: BTreeMap<String, RawSite> = BTreeMap::new();
    // 账户按 (site_name, api_user) 去重，后源覆盖
    let mut accounts: BTreeMap<(String, String), RawAccount> = BTreeMap::new();
    let mut email: Option<RawEmail> = None;

    for cfg in configs {
        for s in &cfg.sites {
            sites.insert(s.name.clone(), s.clone());
        }
        for a in &cfg.accounts {
            accounts.insert(a.key(), a.clone());
        }
        if let Some(e) = &cfg.email {
            match email.as_mut() {
                Some(existing) => existing.overlay(e),
                None => email = Some(e.clone()),
            }
        }
    }

    RawConfig {
        sites: sites.into_values().collect(),
        accounts: accounts.into_values().collect(),
        email,
    }
}
```

- [ ] **Step 2: 在 mod.rs 挂载**

```rust
pub mod merge;
```

- [ ] **Step 3: 写合并去重单测**

`merge.rs` 末尾：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn site(name: &str, domain: &str) -> RawSite {
        RawSite::with_defaults(name, domain, None)
    }
    fn acct(site: &str, api: &str, name: &str) -> RawAccount {
        RawAccount {
            site_name: site.into(), api_user: api.into(),
            display_name: Some(name.into()), cookies: None, username: None, password: None,
        }
    }

    #[test]
    fn later_source_overrides_same_key() {
        let env = RawConfig {
            sites: vec![site("S", "https://old.com")],
            accounts: vec![acct("S", "1", "envname")],
            email: None,
        };
        let sqlite = RawConfig {
            sites: vec![site("S", "https://new.com")],
            accounts: vec![acct("S", "1", "dbname")],
            email: None,
        };
        // 顺序：env(低) → sqlite(高)
        let merged = merge_configs(&[env, sqlite]);
        assert_eq!(merged.sites.len(), 1);
        assert_eq!(merged.sites[0].domain, "https://new.com"); // 后源优先
        assert_eq!(merged.accounts.len(), 1);
        assert_eq!(merged.accounts[0].display_name.as_deref(), Some("dbname"));
    }

    #[test]
    fn different_keys_accumulate() {
        let a = RawConfig { accounts: vec![acct("S", "1", "a")], ..Default::default() };
        let b = RawConfig { accounts: vec![acct("S", "2", "b")], ..Default::default() };
        let merged = merge_configs(&[a, b]);
        assert_eq!(merged.accounts.len(), 2);
    }

    #[test]
    fn email_field_level_overlay() {
        let env = RawConfig { email: Some(RawEmail { user: "u".into(), ..Default::default() }), ..Default::default() };
        let file = RawConfig { email: Some(RawEmail { pass: "p".into(), ..Default::default() }), ..Default::default() };
        let merged = merge_configs(&[env, file]);
        let e = merged.email.unwrap();
        assert_eq!(e.user, "u");
        assert_eq!(e.pass, "p");
    }
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test -p anyrouter-core config_loader::merge`
Expected: 3 passed。

- [ ] **Step 5: 提交**

```bash
git add crates/core/src/config_loader/merge.rs crates/core/src/config_loader/mod.rs
git commit -m "feat(core): config_loader 合并去重（站点名/账户键，后源覆盖，邮件字段级合并）"
```

---

### Task 7: 增量回写 SQLite

**Files:**
- Modify: `crates/core/src/config_loader/merge.rs`

- [ ] **Step 1: 在 merge.rs 追加回写函数**

```rust
use anyhow::Result;
use crate::models::{AccountInput, SiteInput};
use crate::storage::Storage;

/// 把合并结果中「SQLite 没有」的站点/账户增量插入库；已有的不动。
/// 返回插入的 (站点数, 账户数)。
pub fn writeback_to_sqlite(storage: &Storage, merged: &RawConfig) -> Result<(usize, usize)> {
    let mut inserted_sites = 0;
    let mut inserted_accounts = 0;

    // 1. 站点：库中无同名则插入
    let existing_sites = storage.list_sites()?;
    for s in &merged.sites {
        if existing_sites.iter().any(|x| x.name == s.name) {
            continue;
        }
        storage.insert_site(&SiteInput {
            name: s.name.clone(),
            domain: s.domain.clone(),
            login_path: s.login_path.clone(),
            sign_in_path: s.sign_in_path.clone(),
            user_info_path: s.user_info_path.clone(),
            tokens_path: s.tokens_path.clone(),
            logs_path: s.logs_path.clone(),
            chart_path: s.chart_path.clone(),
            api_user_key: s.api_user_key.clone(),
        })?;
        inserted_sites += 1;
    }

    // 2. 账户：按 site_name 找 site_id；库中 (site_id, api_user) 无对应则插入
    let sites_now = storage.list_sites()?;
    for a in &merged.accounts {
        let Some(site) = sites_now.iter().find(|x| x.name == a.site_name) else {
            // 账户引用了不存在的站点，跳过
            eprintln!("[config_loader] 跳过账户：站点 '{}' 不存在", a.site_name);
            continue;
        };
        let existing = storage.list_accounts_by_site(site.id)?;
        if existing.iter().any(|x| x.api_user == a.api_user) {
            continue;
        }
        storage.insert_account(&AccountInput {
            site_id: site.id,
            name: a.display_name.clone().unwrap_or_else(|| a.api_user.clone()),
            api_user: a.api_user.clone(),
            username: a.username.clone(),
            password: a.password.clone(),
            cookies: a.cookies.clone(),
        })?;
        inserted_accounts += 1;
    }

    Ok((inserted_sites, inserted_accounts))
}
```

- [ ] **Step 2: 写增量回写单测**

`merge.rs` 的 tests mod 内追加：

```rust
    #[test]
    fn writeback_inserts_new_and_keeps_existing() {
        use crate::models::{AccountInput, SiteInput};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("w.db");
        let storage = Storage::open(&db).unwrap();
        // 库中已有 S + 账户 (S,1)，display name = "old"
        let sid = storage.insert_site(&SiteInput {
            name: "S".into(), domain: "https://s.com".into(), login_path: "/login".into(),
            sign_in_path: None, user_info_path: "/api/user/self".into(),
            tokens_path: "/api/token/".into(), logs_path: "/api/log/self".into(),
            chart_path: "/api/data/self".into(), api_user_key: "new-api-user".into(),
        }).unwrap();
        storage.insert_account(&AccountInput {
            site_id: sid, name: "old".into(), api_user: "1".into(),
            username: None, password: None, cookies: None,
        }).unwrap();

        // 合并结果含已有 (S,1) 改名 + 新账户 (S,2)
        let merged = RawConfig {
            sites: vec![site("S", "https://s.com")],
            accounts: vec![acct("S", "1", "newname"), acct("S", "2", "b")],
            email: None,
        };
        let (si, ai) = writeback_to_sqlite(&storage, &merged).unwrap();
        assert_eq!(si, 0); // 站点已存在
        assert_eq!(ai, 1); // 仅新增 (S,2)

        let accts = storage.list_accounts_by_site(sid).unwrap();
        assert_eq!(accts.len(), 2);
        // 已有 (S,1) 未被覆盖，仍是 "old"
        let a1 = accts.iter().find(|a| a.api_user == "1").unwrap();
        assert_eq!(a1.name, "old");
    }
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p anyrouter-core config_loader::merge`
Expected: 4 passed。

- [ ] **Step 4: 提交**

```bash
git add crates/core/src/config_loader/merge.rs
git commit -m "feat(core): config_loader 增量回写 SQLite（新条目插入，已有不覆盖）"
```

---

### Task 8: load_unified 入口

**Files:**
- Modify: `crates/core/src/config_loader/mod.rs`

- [ ] **Step 1: 在 mod.rs 写 LoadOptions + load_unified**

mod.rs 顶部已有 `pub mod` 与 `pub use` 声明，在其后追加：

```rust
use std::path::PathBuf;

use source::{ConfigSource, SourceKind};
use types::{RawConfig, UnifiedConfig};

pub struct LoadOptions {
    /// 启用的源，按读取顺序（靠后优先级更高）。
    pub sources: Vec<SourceKind>,
    /// SQLite 数据库路径。
    pub db_path: PathBuf,
    /// 配置文件路径；None = 用 FileSource::default_path()。
    pub config_path: Option<PathBuf>,
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

/// 按 opts.sources 顺序读取各源 → 合并去重 → （若启用 Sqlite）增量回写 → 返回合并结果。
pub fn load_unified(opts: &LoadOptions) -> UnifiedConfig {
    let mut configs: Vec<RawConfig> = Vec::new();

    for kind in &opts.sources {
        let source: Box<dyn ConfigSource> = match kind {
            SourceKind::Env => Box::new(env_source::EnvSource::new()),
            SourceKind::File => {
                let path = opts.config_path.clone()
                    .unwrap_or_else(file_source::FileSource::default_path);
                Box::new(file_source::FileSource::new(path))
            }
            SourceKind::Sqlite => Box::new(sqlite_source::SqliteSource::new(opts.db_path.clone())),
        };
        match source.load() {
            Ok(cfg) => configs.push(cfg),
            Err(e) => eprintln!("[config_loader] 源 {} 读取失败，跳过: {}", kind.as_str(), e),
        }
    }

    let merged = merge::merge_configs(&configs);

    // 增量回写（仅当 Sqlite 在启用列表）
    if opts.sources.contains(&SourceKind::Sqlite) {
        match crate::storage::Storage::open(&opts.db_path) {
            Ok(storage) => {
                if let Err(e) = merge::writeback_to_sqlite(&storage, &merged) {
                    eprintln!("[config_loader] 增量回写失败: {}", e);
                }
            }
            Err(e) => eprintln!("[config_loader] 打开数据库失败，跳过回写: {}", e),
        }
    }

    merged
}
```

- [ ] **Step 2: 写 load_unified 集成单测（env-only，避免依赖真实 .env）**

`mod.rs` 末尾：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_env_only_returns_builtin_sites() {
        // 仅 env 源，不读文件/库；不设 ANYROUTER_ACCOUNTS 时账户为空但有内置站点
        let opts = LoadOptions {
            sources: vec![SourceKind::Env],
            db_path: std::env::temp_dir().join("nonexistent_unified.db"),
            config_path: None,
        };
        let cfg = load_unified(&opts);
        assert!(cfg.sites.iter().any(|s| s.name == "AnyRouter"));
        assert!(cfg.sites.iter().any(|s| s.name == "AgentRouter"));
    }
}
```

> 注：该测试不启用 Sqlite，故不触发回写，不会创建文件。

- [ ] **Step 3: 运行全部 core 测试**

Run: `cargo test -p anyrouter-core config_loader`
Expected: 全部 passed（types/source/env/file/sqlite/merge/mod）。

- [ ] **Step 4: 提交**

```bash
git add crates/core/src/config_loader/mod.rs
git commit -m "feat(core): config_loader load_unified 入口（按源顺序加载+合并+条件回写）"
```

---

## Phase 3: CLI 接入

### Task 9: CLI 命令行源选择参数

**Files:**
- Create: `crates/cli/src/cli_args.rs`
- Modify: `crates/cli/Cargo.toml`

- [ ] **Step 1: CLI 添加 core 依赖**

`crates/cli/Cargo.toml` 的 `[dependencies]` 增加：

```toml
anyrouter-core = { path = "../core" }
```

- [ ] **Step 2: 写 cli_args.rs（手写极简解析，零新依赖）**

```rust
//! 极简命令行参数解析：仅处理数据源选择。
//! 支持：
//!   --sources env,file,sqlite     显式指定启用源与顺序
//!   --skip-source <name>          排除某源（可重复）
//! 二者都不传 → 默认 [Env, File, Sqlite]

use anyrouter_core::config_loader::SourceKind;

pub fn parse_sources<I: IntoIterator<Item = String>>(args: I) -> Vec<SourceKind> {
    let default = vec![SourceKind::Env, SourceKind::File, SourceKind::Sqlite];
    let mut explicit: Option<Vec<SourceKind>> = None;
    let mut skip: Vec<SourceKind> = Vec::new();

    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--sources" => {
                if let Some(val) = it.next() {
                    let list: Vec<SourceKind> = val
                        .split(',')
                        .filter_map(SourceKind::parse)
                        .collect();
                    if !list.is_empty() {
                        explicit = Some(list);
                    }
                }
            }
            "--skip-source" => {
                if let Some(val) = it.next() {
                    if let Some(k) = SourceKind::parse(&val) {
                        skip.push(k);
                    }
                }
            }
            _ => {}
        }
    }

    let base = explicit.unwrap_or(default);
    base.into_iter().filter(|k| !skip.contains(k)).collect()
}
```

- [ ] **Step 3: 写参数解析单测**

`cli_args.rs` 末尾：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> { s.iter().map(|x| x.to_string()).collect() }

    #[test]
    fn default_is_all_three() {
        assert_eq!(parse_sources(v(&[])),
            vec![SourceKind::Env, SourceKind::File, SourceKind::Sqlite]);
    }

    #[test]
    fn explicit_sources_respect_order() {
        assert_eq!(parse_sources(v(&["--sources", "sqlite,env"])),
            vec![SourceKind::Sqlite, SourceKind::Env]);
    }

    #[test]
    fn skip_source_removes() {
        assert_eq!(parse_sources(v(&["--skip-source", "file"])),
            vec![SourceKind::Env, SourceKind::Sqlite]);
    }
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test -p anyrouter-checkin cli_args`
Expected: 3 passed。

- [ ] **Step 5: 提交**

```bash
git add crates/cli/Cargo.toml crates/cli/src/cli_args.rs
git commit -m "feat(cli): 命令行数据源选择参数解析（--sources / --skip-source）"
```

---

### Task 10: CLI main 改用 load_unified

**Files:**
- Modify: `crates/cli/src/main.rs`

- [ ] **Step 1: 在 main.rs 注册模块并接入加载**

`main.rs` 顶部模块声明区增加 `mod cli_args;`，并把 Phase 1 配置加载段替换为统一加载。原 `AppConfig::load_from_env()` + `load_accounts_config()` 改为：

```rust
mod cli_args;
// ... 其余 mod 保留

use anyrouter_core::config_loader::{load_unified, LoadOptions};
use anyrouter_core::storage::Storage;

// 在 main() 内，dotenvy::dotenv().ok(); 之后：
let sources = cli_args::parse_sources(std::env::args().skip(1));
log::info(&format!(
    "启用数据源: [{}]",
    sources.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
));
let opts = LoadOptions {
    sources,
    db_path: Storage::default_path(),
    config_path: None,
};
let unified = load_unified(&opts);
log::success(&format!(
    "配置加载完成: {} 个站点, {} 个账户",
    unified.sites.len(),
    unified.accounts.len()
));
```

- [ ] **Step 2: 让后续签到流程消费 unified**

CLI 原先把 `accounts: Vec<AccountConfig>` 和 `app_config.providers` 传给 playwright runner。改为从 `unified` 构造 runner 输入：遍历 `unified.accounts`，按 `account.site_name` 在 `unified.sites` 找站点。具体地，CLI 的 `playwright::run_checkin` 调用点改为传入由 `unified` 组装的账户列表（站点字段内联到每个账户输入）。

将 `crates/cli/src/main.rs` 中构造 runner 输入的代码改为：

```rust
// 组装 playwright 账户输入：每账户内联其站点配置
let runner_accounts: Vec<playwright::RunnerAccount> = unified.accounts.iter().filter_map(|a| {
    let site = unified.sites.iter().find(|s| s.name == a.site_name)?;
    Some(playwright::RunnerAccount {
        name: a.display_name.clone().unwrap_or_else(|| a.api_user.clone()),
        provider: site.name.clone(),
        domain: site.domain.clone(),
        login_path: site.login_path.clone(),
        sign_in_path: site.sign_in_path.clone(),
        user_info_path: site.user_info_path.clone(),
        api_user_key: site.api_user_key.clone(),
        api_user: a.api_user.clone(),
        cookies: a.cookies.clone(),
        username: a.username.clone(),
        password: a.password.clone(),
    })
}).collect();

if runner_accounts.is_empty() {
    log::error("没有可用账户，程序退出");
    std::process::exit(1);
}
```

> 注：`playwright::RunnerAccount` 字段需与 CLI 现有 `crates/cli/src/playwright.rs` 的账户输入结构对齐。实施时先阅读 `crates/cli/src/playwright.rs` 确认结构体名与字段；若现有结构名不同，按现有结构名替换 `RunnerAccount` 并匹配字段。邮件通知改用 `unified.email`（若 `Some` 则用其字段构造 `EmailNotifier`，否则跳过）。

- [ ] **Step 3: 邮件通知改用 unified.email**

`notify::EmailNotifier::from_env()` 调用点改为从 `unified.email` 构造（保留 `from_env` 作后备亦可）。最简实现：在 `notify.rs` 增加 `EmailNotifier::from_raw(email: &anyrouter_core::config_loader::RawEmail)` 构造器，main 里：

```rust
let notifier = match &unified.email {
    Some(e) => notify::EmailNotifier::from_raw(e),
    None => notify::EmailNotifier::from_env(), // 兜底
};
```

`notify.rs` 增加：

```rust
impl EmailNotifier {
    pub fn from_raw(e: &anyrouter_core::config_loader::RawEmail) -> Self {
        let sender = if e.sender.is_empty() { e.user.clone() } else { e.sender.clone() };
        Self {
            email_user: e.user.clone(),
            email_pass: e.pass.clone(),
            email_to: e.to.clone(),
            email_sender: sender,
            smtp_server: e.smtp_server.clone(),
        }
    }
}
```

- [ ] **Step 4: 编译并运行验证**

Run: `cargo build -p anyrouter-checkin`
Expected: 编译通过。

Run: `cargo run -p anyrouter-checkin -- --skip-source sqlite`
Expected: 日志打印「启用数据源: [env, file]」与「配置加载完成: N 个站点, M 个账户」，进入签到流程（无账户则报错退出）。

- [ ] **Step 5: 提交**

```bash
git add crates/cli/src/main.rs crates/cli/src/notify.rs
git commit -m "feat(cli): main 改用统一多源配置加载，签到与邮件消费 UnifiedConfig"
```

---

## Phase 4: GUI 接入

### Task 11: GUI main 改用 load_unified + meta 开关

**Files:**
- Modify: `crates/gui/src/main.rs`
- Modify: `crates/gui/src/app_state.rs`

- [ ] **Step 1: app_state 增加 sqlite_enabled 字段**

`crates/gui/src/app_state.rs` 的 `AppState` 结构体增加字段：

```rust
    /// 当前是否启用 SQLite 数据源（禁用时 CRUD 不可用）
    pub sqlite_enabled: bool,
```

`Default for AppState` 的初始化增加 `sqlite_enabled: true,`。

- [ ] **Step 2: main.rs 读取 meta.cfg_sources 并调用 load_unified**

`crates/gui/src/main.rs` 改为：

```rust
mod app_state;
mod components;
mod theme;
mod views;

use anyrouter_core::config_loader::{load_unified, LoadOptions, SourceKind};
use anyrouter_core::storage::Storage;

fn main() {
    dotenvy::dotenv().ok();

    let db_path = Storage::default_path();
    // meta 永远可读，用于决定启用哪些源（不属于可切换数据源，无循环依赖）
    let sources = read_cfg_sources(&db_path);

    let opts = LoadOptions {
        sources: sources.clone(),
        db_path: db_path.clone(),
        config_path: None,
    };
    let _ = load_unified(&opts); // 含增量回写；GUI 之后从 SQLite 读取展示

    views::root::run_app(db_path, sources);
}

/// 从 SQLite meta.cfg_sources 读取启用源；缺省/打不开 → 默认三源全开。
fn read_cfg_sources(db_path: &std::path::Path) -> Vec<SourceKind> {
    let default = vec![SourceKind::Env, SourceKind::File, SourceKind::Sqlite];
    let Ok(storage) = Storage::open(&db_path.to_path_buf()) else {
        return default;
    };
    match storage.get_meta("cfg_sources") {
        Ok(Some(val)) => {
            let list: Vec<SourceKind> = val.split(',').filter_map(SourceKind::parse).collect();
            if list.is_empty() { default } else { list }
        }
        _ => default,
    }
}
```

> 注：`views::root::run_app` 当前签名是 `run_app(storage: Storage)`。本步改为 `run_app(db_path: PathBuf, sources: Vec<SourceKind>)`。下一步同步改 root.rs。

- [ ] **Step 3: 改 run_app 签名，按 sources 设置 sqlite_enabled**

`crates/gui/src/views/root.rs` 的 `run_app` 改为接收 `db_path` 与 `sources`，在 `AppState::from_storage` 后设置 `sqlite_enabled`：

```rust
use anyrouter_core::config_loader::SourceKind;

pub fn run_app(db_path: std::path::PathBuf, sources: Vec<SourceKind>) {
    let sqlite_enabled = sources.contains(&SourceKind::Sqlite);
    // 全局 panic hook 与启动横幅保留不变
    application().run(move |cx: &mut App| {
        bind_text_input_keys(cx);
        let storage = anyrouter_core::storage::Storage::open(&db_path)
            .expect("failed to open database");
        let state = cx.new(|_| {
            let mut s = AppState::from_storage(storage);
            s.sqlite_enabled = sqlite_enabled;
            s
        });
        // ... 其余开窗、spawn_log_poller 逻辑保持（db_path 已有）
    });
}
```

> 实施时阅读现有 `run_app` 全文，保留其窗口选项（自定义标题栏）、`spawn_log_poller` 等逻辑，仅调整入参与 `sqlite_enabled` 注入。

- [ ] **Step 4: 编译验证**

Run: `cargo build -p anyrouter-gui`
Expected: 编译通过（settings 视图尚未引用，CRUD 禁用逻辑下一任务接）。

- [ ] **Step 5: 运行验证**

Run: 命令行启动 `./target/debug/anyrouter-gui.exe`，观察 stderr 有「config_loader」无报错，窗口正常显示站点。

- [ ] **Step 6: 提交**

```bash
git add crates/gui/src/main.rs crates/gui/src/app_state.rs crates/gui/src/views/root.rs
git commit -m "feat(gui): 启动改用统一多源加载，按 meta.cfg_sources 决定启用源"
```

---

### Task 12: GUI 设置弹窗（源开关）

**Files:**
- Create: `crates/gui/src/views/settings.rs`
- Modify: `crates/gui/src/views/mod.rs`
- Modify: `crates/gui/src/app_state.rs`
- Modify: `crates/gui/src/views/root.rs`

- [ ] **Step 1: app_state 增加设置弹窗状态**

`crates/gui/src/app_state.rs`：`ModalKind` 枚举增加变体 `Settings`；`AppState` 增加临时勾选状态字段：

```rust
    /// 设置弹窗里三个源的临时勾选（保存时落库）
    pub settings_env: bool,
    pub settings_file: bool,
    pub settings_sqlite: bool,
```

`Default` 初始化为 `true,true,true`。`ModalKind` 增加 `Settings`。

- [ ] **Step 2: 写 settings.rs**

```rust
use gpui::{AnyElement, Context, Entity, IntoElement, ParentElement, Styled, div, prelude::*, px};

use anyrouter_core::config_loader::SourceKind;
use anyrouter_core::storage::Storage;
use crate::app_state::AppState;
use crate::theme;
use crate::views::root::RootView;

pub fn render(state: Entity<AppState>, cx: &mut Context<RootView>) -> AnyElement {
    let snap = state.read(cx);
    let (env_on, file_on, sqlite_on) = (snap.settings_env, snap.settings_file, snap.settings_sqlite);
    let none_selected = !env_on && !file_on && !sqlite_on;

    let row = |label: &str, on: bool, kind: SourceKind, state: Entity<AppState>| -> AnyElement {
        let id = match kind { SourceKind::Env => "set-env", SourceKind::File => "set-file", SourceKind::Sqlite => "set-sqlite" };
        div().id(id).flex().items_center().gap(px(8.0)).cursor_pointer()
            .child(
                div().w(px(14.0)).h(px(14.0)).rounded(px(3.0)).border_1()
                    .border_color(if on { theme::accent_blue() } else { theme::border_accent() })
                    .flex().items_center().justify_center()
                    .when(on, |this| this.bg(theme::accent_blue()).child(
                        div().text_size(px(10.0)).text_color(theme::bg_window()).child("✓"))),
            )
            .child(div().text_size(px(12.0)).text_color(theme::text_primary()).child(label.to_string()))
            .on_click(move |_, _w, cx| {
                state.update(cx, |st, cx| {
                    match kind {
                        SourceKind::Env => st.settings_env = !st.settings_env,
                        SourceKind::File => st.settings_file = !st.settings_file,
                        SourceKind::Sqlite => st.settings_sqlite = !st.settings_sqlite,
                    }
                    cx.notify();
                });
            })
            .into_any_element()
    };

    let body = div().flex().flex_col().gap(px(10.0))
        .child(div().text_size(px(11.0)).text_color(theme::text_muted())
            .child("选择启用的配置数据源（按 环境变量 → 配置文件 → 数据库 顺序合并，后者优先）："))
        .child(row("环境变量 (含 .env)", env_on, SourceKind::Env, state.clone()))
        .child(row("JSON 配置文件", file_on, SourceKind::File, state.clone()))
        .child(row("SQLite 数据库", sqlite_on, SourceKind::Sqlite, state.clone()))
        .when(none_selected, |this| this.child(
            div().text_size(px(10.0)).text_color(theme::error_red()).child("至少需启用一个数据源")))
        .into_any_element();

    // 复用 modals 的 panel：此处直接内联一个简化面板
    let state_close = state.clone();
    let state_save = state;
    crate::views::modals::panel(
        "设置 · 数据源".to_string(),
        body,
        vec![
            crate::views::modals::PanelButton {
                label: "取消".into(), danger: false,
                on_click: Box::new(move |cx| {
                    state_close.update(cx, |st, cx| { st.active_modal = None; cx.notify(); });
                }),
            },
            crate::views::modals::PanelButton {
                label: "保存".into(), danger: false,
                on_click: Box::new(move |cx| {
                    state_save.update(cx, |st, cx| {
                        if !st.settings_env && !st.settings_file && !st.settings_sqlite {
                            return; // 至少一个
                        }
                        let mut list: Vec<&str> = Vec::new();
                        if st.settings_env { list.push("env"); }
                        if st.settings_file { list.push("file"); }
                        if st.settings_sqlite { list.push("sqlite"); }
                        let val = list.join(",");
                        if let Some(ref storage) = st.storage {
                            let _ = storage.set_meta("cfg_sources", &val);
                        }
                        st.sqlite_enabled = st.settings_sqlite;
                        st.active_modal = None;
                        st.log_entries.push(crate::app_state::LogEntry {
                            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                            level: crate::app_state::LogLevel::Info,
                            message: format!("数据源已更新: [{}]（重启后完全生效）", val),
                        });
                        st.reload_sites();
                        cx.notify();
                    });
                }),
            },
        ],
        px(360.0),
    )
}
```

> 注：本任务需要把 `modals.rs` 的 `panel` 函数与 `PanelButton` 结构体改为 `pub`（当前是私有）。在 Step 3 处理。

- [ ] **Step 3: 公开 modals 的 panel/PanelButton 并挂载 settings 模块**

`crates/gui/src/views/modals.rs`：`struct PanelButton` → `pub struct PanelButton`，其字段 `label/danger/on_click` 加 `pub`；`fn panel(...)` → `pub fn panel(...)`。

`crates/gui/src/views/mod.rs` 增加 `pub mod settings;`。

`AppState` 需有 `storage: Option<Storage>` 字段（已存在）。设置弹窗打开时把当前 `sources` 同步到 `settings_*`：在 root.rs 打开设置入口的点击里设置（见 Step 4）。

- [ ] **Step 4: root.rs 标题栏增加齿轮入口 + 渲染设置弹窗**

`crates/gui/src/views/root.rs`：
1. 标题栏窗口控制按钮区，在「最小化」前增加齿轮按钮：

```rust
.child(window_control("win-settings", "⚙", theme::text_muted(), {
    let state = self.state.clone();
    move |_window, cx| {
        state.update(cx, |st, cx| {
            // 用当前 sqlite_enabled 等初始化勾选；env/file 默认按是否在 meta 中
            st.settings_env = true;
            st.settings_file = true;
            st.settings_sqlite = st.sqlite_enabled;
            st.active_modal = Some(crate::app_state::ModalKind::Settings);
            cx.notify();
        });
    }
}))
```

> `window_control` 当前签名为 `(id, glyph, hover_color, on_click: impl Fn(&mut Window, &mut App))`。确认闭包签名匹配；若不匹配按现有签名调整。

2. 在 modal 渲染分发处（`views::modals::render` 调用附近）增加 Settings 分支：当 `active_modal == Some(ModalKind::Settings)` 时调用 `views::settings::render(state, cx)`。

实施时先读 root.rs 中 modal 渲染调用点，按现有模式追加分支。

- [ ] **Step 5: 编译验证**

Run: `cargo build -p anyrouter-gui`
Expected: 编译通过。

- [ ] **Step 6: 运行验证**

命令行启动 GUI，点击标题栏齿轮 → 弹出设置弹窗 → 取消勾选「JSON 配置文件」→ 保存 → 日志显示「数据源已更新: [env,sqlite]」。重启 GUI 确认仍生效（meta 持久化）。

- [ ] **Step 7: 提交**

```bash
git add crates/gui/src/views/settings.rs crates/gui/src/views/mod.rs crates/gui/src/views/modals.rs crates/gui/src/app_state.rs crates/gui/src/views/root.rs
git commit -m "feat(gui): 设置弹窗切换数据源（持久化到 meta.cfg_sources）"
```

---

### Task 13: GUI CRUD 在 SQLite 禁用时不可用

**Files:**
- Modify: `crates/gui/src/views/home.rs`
- Modify: `crates/gui/src/views/modals.rs`

- [ ] **Step 1: 站点卡片「编辑/删除/查看账户」按 sqlite_enabled 禁用**

`crates/gui/src/views/home.rs`：在 `render` 读取 `let sqlite_enabled = snap.sqlite_enabled;`，传入卡片渲染。卡片上的「✎ 编辑」「🗑 删除」「+ 新建站点」按钮，当 `!sqlite_enabled` 时：去掉 `on_click`、文字置灰（`theme::text_weakest()`）、不加 `cursor_pointer`。「查看账户」保留（只读查看可用）。

在卡片操作行根据 `sqlite_enabled` 条件渲染按钮：

```rust
.when(sqlite_enabled, |row| {
    row.child(/* 编辑按钮（原样带 on_click） */)
       .child(/* 删除按钮（原样带 on_click） */)
})
.when(!sqlite_enabled, |row| {
    row.child(div().text_size(px(10.0)).text_color(theme::text_weakest())
        .child("（启用 SQLite 源后可编辑）"))
})
```

- [ ] **Step 2: 账户列表弹窗「新增/编辑/删除」同样按 sqlite_enabled 处理**

`crates/gui/src/views/modals.rs` 的 `render_account_list`：读取 `state.read(cx).sqlite_enabled`，当禁用时底部「+ 新增账户」按钮替换为灰色提示文字，行内「✎ 编辑」「🗑 删除」不渲染 on_click。

- [ ] **Step 3: 编译验证**

Run: `cargo build -p anyrouter-gui`
Expected: 编译通过。

- [ ] **Step 4: 运行验证**

GUI 设置里关掉 SQLite 源并重启 → 站点卡片不显示编辑/删除、账户弹窗不显示新增/编辑/删除，显示「启用 SQLite 源后可编辑」提示。

- [ ] **Step 5: 提交**

```bash
git add crates/gui/src/views/home.rs crates/gui/src/views/modals.rs
git commit -m "feat(gui): SQLite 源禁用时禁用 CRUD 并提示"
```

---

## Phase 5: 清理与收尾

### Task 14: 删除 env_import，全量验证

**Files:**
- Delete: `crates/core/src/env_import.rs`
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: 确认无引用残留**

Run: `grep -rn "env_import" crates/`
Expected: 仅剩 `lib.rs` 的 `pub mod env_import;` 与历史无关引用。若 `crates/gui` 仍引用，改为已不需要（Task 11 已移除）。

- [ ] **Step 2: 删除文件与模块声明**

删除 `crates/core/src/env_import.rs`；`crates/core/src/lib.rs` 移除 `pub mod env_import;`。

- [ ] **Step 3: 全量编译**

Run: `cargo build --workspace`
Expected: 编译通过，无对 env_import 的未解析引用。

- [ ] **Step 4: 全量测试**

Run: `cargo test -p anyrouter-core`
Expected: config_loader 全部单测 passed。

- [ ] **Step 5: 端到端冒烟**

- CLI：`cargo run -p anyrouter-checkin -- --sources env` 正常加载内置站点 + .env 账户。
- GUI：命令行启动，齿轮切换数据源、签到、站点/账户显示均正常；删除旧的 `%APPDATA%/anyrouter-checkin/data.db` 后首启能从 env 增量回写重建。

- [ ] **Step 6: 提交**

```bash
git add crates/core/src/lib.rs
git rm crates/core/src/env_import.rs
git commit -m "refactor(core): 删除一次性 env_import，统一由 config_loader 增量同步"
```

---

## 自审备注（规格覆盖检查）

- §2 模块划分 → Task 1-8 全覆盖。
- §3 类型 + 去重键 → Task 1。
- §4 三个源 → Task 3/4/5。
- §5 合并 + 增量回写 + 废弃 env_imported → Task 6/7/8 + Task 14。
- §6 JSON schema → Task 4（FileConfig + 默认值）。
- §7 源开关：CLI → Task 9；GUI meta+UI → Task 11/12。
- §8 CLI/GUI 接入 → Task 10/11/12/13。
- §9 错误处理（单源失败跳过、文件缺失静默、回写失败告警）→ Task 4/5/8。
- §10 测试 → 各 Task 内 `#[cfg(test)]`。
- §11 范围外（邮件不入库、SQLite email=None）→ Task 5 注释明确。
- §8.2 GUI SQLite 禁用时只读 + CRUD 禁用 → Task 11（sqlite_enabled）+ Task 13。
