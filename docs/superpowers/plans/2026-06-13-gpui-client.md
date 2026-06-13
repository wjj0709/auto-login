# AnyRouter GPUI 可视化客户端 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 AnyRouter 签到工具构建 GPUI 桌面客户端，实现站点/账户管理、一键签到、详情查看等完整功能。

**Architecture:** Cargo workspace 三 crate 结构（core 共享业务逻辑 + cli 原命令行 + gui 桌面客户端）。UI 使用 GPUI 的 Render trait 构建视图树，通过全局 AppState Model 驱动数据流。后台操作通过 Tokio spawn 执行，结果通过 cx.update_model() 回写 UI。

**Tech Stack:** Rust 2024、GPUI (git dep from zed repo)、gpui-component (UI 组件库)、Tokio、rusqlite、keyring、aes-gcm、serde/serde_json

---

## Phase 1: 项目骨架与 Workspace 搭建

### Task 1: 初始化 Cargo Workspace

**Files:**
- Modify: `Cargo.toml` (转为 workspace root)
- Create: `crates/cli/Cargo.toml`
- Create: `crates/cli/src/main.rs`
- Create: `crates/core/Cargo.toml`
- Create: `crates/core/src/lib.rs`
- Create: `crates/gui/Cargo.toml`
- Create: `crates/gui/src/main.rs`

- [ ] **Step 1: 将现有项目改造为 workspace root**

将根 `Cargo.toml` 改为 workspace 定义，现有源码移入 `crates/cli/`：

```toml
# Cargo.toml (workspace root)
[workspace]
members = ["crates/core", "crates/cli", "crates/gui"]
resolver = "2"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
chrono = "0.4"
anyhow = "1"
```

- [ ] **Step 2: 迁移现有 CLI 代码到 crates/cli/**

```bash
mkdir -p crates/cli/src
# 移动现有 src/ 到 crates/cli/src/
mv src/* crates/cli/src/
```

创建 `crates/cli/Cargo.toml`：

```toml
[package]
name = "anyrouter-checkin"
version = "0.1.0"
edition = "2024"
description = "AnyRouter auto check-in tool (CLI)"

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
chrono = { workspace = true }
dotenvy = "0.15"
sha2 = "0.10"
lettre = { version = "0.11", features = ["tokio1-native-tls", "builder", "smtp-transport"] }
```

- [ ] **Step 3: 创建 core crate 骨架**

`crates/core/Cargo.toml`：

```toml
[package]
name = "anyrouter-core"
version = "0.1.0"
edition = "2024"
description = "AnyRouter shared core library"

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
chrono = { workspace = true }
anyhow = { workspace = true }
rusqlite = { version = "0.31", features = ["bundled"] }
keyring = "3"
aes-gcm = "0.10"
rand = "0.8"
dirs = "5"
```

`crates/core/src/lib.rs`：

```rust
pub mod models;
pub mod storage;
pub mod crypto;
pub mod playwright;
pub mod service;
pub mod env_import;
```

- [ ] **Step 4: 创建 gui crate 骨架**

`crates/gui/Cargo.toml`：

```toml
[package]
name = "anyrouter-gui"
version = "0.1.0"
edition = "2024"
description = "AnyRouter GPUI desktop client"

[dependencies]
anyrouter-core = { path = "../core" }
gpui = { git = "https://github.com/zed-industries/zed", package = "gpui" }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
chrono = { workspace = true }
anyhow = { workspace = true }
```

`crates/gui/src/main.rs`：

```rust
use gpui::*;

struct HelloView;

impl Render for HelloView {
    fn render(&mut self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x0a0c10))
            .flex()
            .justify_center()
            .items_center()
            .text_color(rgb(0xdbe2ec))
            .child("AnyRouter — 正在初始化...")
    }
}

fn main() {
    App::new().run(|cx: &mut AppContext| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1100.0), px(750.0)),
                    cx,
                ))),
                ..Default::default()
            },
            |cx| cx.new_view(|_cx| HelloView),
        )
        .unwrap();
    });
}
```

- [ ] **Step 5: 验证 workspace 编译通过**

```bash
cargo build --workspace
```

Expected: 三个 crate 均编译成功（gui 可能需要调整 GPUI 依赖版本）。

- [ ] **Step 6: 提交**

```bash
git add -A
git commit -m "feat: 初始化 Cargo workspace 结构（core/cli/gui 三 crate）"
```

---

## Phase 2: Core — 数据模型与存储层

### Task 2: 定义数据模型

**Files:**
- Create: `crates/core/src/models.rs`

- [ ] **Step 1: 编写数据模型结构体**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: i64,
    pub name: String,
    pub domain: String,
    pub login_path: String,
    pub sign_in_path: Option<String>,
    pub user_info_path: String,
    pub tokens_path: String,
    pub logs_path: String,
    pub chart_path: String,
    pub api_user_key: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: i64,
    pub site_id: i64,
    pub name: String,
    pub api_user: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub cookies: Option<String>,
    pub cookie_issued_at: Option<String>,
    pub cookie_expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountCache {
    pub account_id: i64,
    pub kind: CacheKind,
    pub payload_json: String,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheKind {
    Overview,
    Tokens,
    Logs,
    Chart,
}

impl CacheKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Tokens => "tokens",
            Self::Logs => "logs",
            Self::Chart => "chart",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "overview" => Some(Self::Overview),
            "tokens" => Some(Self::Tokens),
            "logs" => Some(Self::Logs),
            "chart" => Some(Self::Chart),
            _ => None,
        }
    }
}

/// 站点 + 统计信息（用于主页卡片展示）
#[derive(Debug, Clone)]
pub struct SiteWithStats {
    pub site: Site,
    pub account_count: usize,
    pub total_balance: f64,
    pub expired_count: usize,
    pub checkin_today: usize,
}

/// 创建/更新站点时的输入
#[derive(Debug, Clone)]
pub struct SiteInput {
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

/// 创建/更新账户时的输入
#[derive(Debug, Clone)]
pub struct AccountInput {
    pub site_id: i64,
    pub name: String,
    pub api_user: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub cookies: Option<String>,
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/core/src/models.rs
git commit -m "feat(core): 定义数据模型（Site/Account/AccountCache）"
```

---

### Task 3: 实现 SQLite 存储层

**Files:**
- Create: `crates/core/src/storage.rs`

- [ ] **Step 1: 实现数据库初始化与迁移**

```rust
use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;

use crate::models::*;

const SCHEMA_VERSION: i64 = 1;

pub struct Storage {
    conn: Connection,
}

impl Storage {
    pub fn open(path: &PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let mut storage = Self { conn };
        storage.migrate()?;
        Ok(storage)
    }

    pub fn default_path() -> PathBuf {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("anyrouter-checkin").join("data.db")
    }

    fn migrate(&mut self) -> Result<()> {
        let current = self.get_meta("schema_version")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);

        if current < 1 {
            self.conn.execute_batch(r#"
                CREATE TABLE IF NOT EXISTS sites (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT UNIQUE NOT NULL,
                    domain TEXT NOT NULL,
                    login_path TEXT NOT NULL DEFAULT '/login',
                    sign_in_path TEXT,
                    user_info_path TEXT NOT NULL DEFAULT '/api/user/self',
                    tokens_path TEXT NOT NULL DEFAULT '/api/token/',
                    logs_path TEXT NOT NULL DEFAULT '/api/log/self',
                    chart_path TEXT NOT NULL DEFAULT '/api/data/self',
                    api_user_key TEXT NOT NULL DEFAULT 'new-api-user',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS accounts (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    site_id INTEGER NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
                    name TEXT NOT NULL,
                    api_user TEXT NOT NULL,
                    username_enc BLOB,
                    password_enc BLOB,
                    cookies_enc BLOB,
                    cookie_issued_at TEXT,
                    cookie_expires_at TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    UNIQUE(site_id, name)
                );

                CREATE TABLE IF NOT EXISTS account_cache (
                    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                    kind TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    fetched_at TEXT NOT NULL,
                    PRIMARY KEY (account_id, kind)
                );

                CREATE TABLE IF NOT EXISTS meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
            "#)?;
            self.set_meta("schema_version", &SCHEMA_VERSION.to_string())?;
        }

        Ok(())
    }
}
```

- [ ] **Step 2: 实现 meta 表 CRUD**

```rust
impl Storage {
    pub fn get_meta(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| row.get(0))
            .ok()
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            [key, value],
        )?;
        Ok(())
    }
}
```

- [ ] **Step 3: 实现 sites 表 CRUD**

```rust
impl Storage {
    pub fn list_sites(&self) -> Result<Vec<Site>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, domain, login_path, sign_in_path, user_info_path, tokens_path, logs_path, chart_path, api_user_key, created_at, updated_at FROM sites ORDER BY id"
        )?;
        let sites = stmt.query_map([], |row| {
            Ok(Site {
                id: row.get(0)?,
                name: row.get(1)?,
                domain: row.get(2)?,
                login_path: row.get(3)?,
                sign_in_path: row.get(4)?,
                user_info_path: row.get(5)?,
                tokens_path: row.get(6)?,
                logs_path: row.get(7)?,
                chart_path: row.get(8)?,
                api_user_key: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(sites)
    }

    pub fn get_site(&self, id: i64) -> Result<Option<Site>> {
        let sites = self.list_sites()?;
        Ok(sites.into_iter().find(|s| s.id == id))
    }

    pub fn insert_site(&self, input: &SiteInput) -> Result<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO sites (name, domain, login_path, sign_in_path, user_info_path, tokens_path, logs_path, chart_path, api_user_key, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            rusqlite::params![input.name, input.domain, input.login_path, input.sign_in_path, input.user_info_path, input.tokens_path, input.logs_path, input.chart_path, input.api_user_key, now, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_site(&self, id: i64, input: &SiteInput) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE sites SET name=?1, domain=?2, login_path=?3, sign_in_path=?4, user_info_path=?5, tokens_path=?6, logs_path=?7, chart_path=?8, api_user_key=?9, updated_at=?10 WHERE id=?11",
            rusqlite::params![input.name, input.domain, input.login_path, input.sign_in_path, input.user_info_path, input.tokens_path, input.logs_path, input.chart_path, input.api_user_key, now, id],
        )?;
        Ok(())
    }

    pub fn delete_site(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM sites WHERE id = ?1", [id])?;
        Ok(())
    }
}
```

- [ ] **Step 4: 实现 accounts 表 CRUD（加密字段暂用明文占位）**

```rust
impl Storage {
    pub fn list_accounts_by_site(&self, site_id: i64) -> Result<Vec<Account>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, site_id, name, api_user, username_enc, password_enc, cookies_enc, cookie_issued_at, cookie_expires_at, created_at, updated_at FROM accounts WHERE site_id = ?1 ORDER BY id"
        )?;
        let accounts = stmt.query_map([site_id], |row| {
            Ok(Account {
                id: row.get(0)?,
                site_id: row.get(1)?,
                name: row.get(2)?,
                api_user: row.get(3)?,
                username: row.get::<_, Option<Vec<u8>>>(4)?.map(|b| String::from_utf8_lossy(&b).to_string()),
                password: row.get::<_, Option<Vec<u8>>>(5)?.map(|b| String::from_utf8_lossy(&b).to_string()),
                cookies: row.get::<_, Option<Vec<u8>>>(6)?.map(|b| String::from_utf8_lossy(&b).to_string()),
                cookie_issued_at: row.get(7)?,
                cookie_expires_at: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(accounts)
    }

    pub fn list_all_accounts(&self) -> Result<Vec<Account>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, site_id, name, api_user, username_enc, password_enc, cookies_enc, cookie_issued_at, cookie_expires_at, created_at, updated_at FROM accounts ORDER BY id"
        )?;
        let accounts = stmt.query_map([], |row| {
            Ok(Account {
                id: row.get(0)?,
                site_id: row.get(1)?,
                name: row.get(2)?,
                api_user: row.get(3)?,
                username: row.get::<_, Option<Vec<u8>>>(4)?.map(|b| String::from_utf8_lossy(&b).to_string()),
                password: row.get::<_, Option<Vec<u8>>>(5)?.map(|b| String::from_utf8_lossy(&b).to_string()),
                cookies: row.get::<_, Option<Vec<u8>>>(6)?.map(|b| String::from_utf8_lossy(&b).to_string()),
                cookie_issued_at: row.get(7)?,
                cookie_expires_at: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(accounts)
    }

    pub fn insert_account(&self, input: &AccountInput) -> Result<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO accounts (site_id, name, api_user, username_enc, password_enc, cookies_enc, cookie_issued_at, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            rusqlite::params![
                input.site_id, input.name, input.api_user,
                input.username.as_ref().map(|s| s.as_bytes().to_vec()),
                input.password.as_ref().map(|s| s.as_bytes().to_vec()),
                input.cookies.as_ref().map(|s| s.as_bytes().to_vec()),
                Option::<String>::None, now, now
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_account(&self, id: i64, input: &AccountInput) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE accounts SET name=?1, api_user=?2, username_enc=?3, password_enc=?4, cookies_enc=?5, updated_at=?6 WHERE id=?7",
            rusqlite::params![
                input.name, input.api_user,
                input.username.as_ref().map(|s| s.as_bytes().to_vec()),
                input.password.as_ref().map(|s| s.as_bytes().to_vec()),
                input.cookies.as_ref().map(|s| s.as_bytes().to_vec()),
                now, id
            ],
        )?;
        Ok(())
    }

    pub fn update_account_cookies(&self, id: i64, cookies: &str, issued_at: &str, expires_at: Option<&str>) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE accounts SET cookies_enc=?1, cookie_issued_at=?2, cookie_expires_at=?3, updated_at=?4 WHERE id=?5",
            rusqlite::params![cookies.as_bytes().to_vec(), issued_at, expires_at, now, id],
        )?;
        Ok(())
    }

    pub fn delete_account(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM accounts WHERE id = ?1", [id])?;
        Ok(())
    }
}
```

- [ ] **Step 5: 实现 account_cache 表操作**

```rust
impl Storage {
    pub fn get_cache(&self, account_id: i64, kind: CacheKind) -> Result<Option<AccountCache>> {
        let result = self.conn.query_row(
            "SELECT account_id, kind, payload_json, fetched_at FROM account_cache WHERE account_id=?1 AND kind=?2",
            rusqlite::params![account_id, kind.as_str()],
            |row| Ok(AccountCache {
                account_id: row.get(0)?,
                kind,
                payload_json: row.get(2)?,
                fetched_at: row.get(3)?,
            }),
        );
        match result {
            Ok(cache) => Ok(Some(cache)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn upsert_cache(&self, account_id: i64, kind: CacheKind, payload: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT OR REPLACE INTO account_cache (account_id, kind, payload_json, fetched_at) VALUES (?1,?2,?3,?4)",
            rusqlite::params![account_id, kind.as_str(), payload, now],
        )?;
        Ok(())
    }
}
```

- [ ] **Step 6: 验证编译**

```bash
cargo build -p anyrouter-core
```

Expected: 编译成功。

- [ ] **Step 7: 提交**

```bash
git add crates/core/src/storage.rs
git commit -m "feat(core): 实现 SQLite 存储层（四张表 CRUD + 自动迁移）"
```

---

### Task 4: 实现加密模块

**Files:**
- Create: `crates/core/src/crypto.rs`

- [ ] **Step 1: 实现 keyring 主密钥管理 + AES-256-GCM 加解密**

```rust
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use aes_gcm::aead::Aead;
use anyhow::{Result, Context};
use rand::RngCore;

const SERVICE_NAME: &str = "anyrouter-checkin";
const KEY_NAME: &str = "master-key";

pub struct Crypto {
    cipher: Aes256Gcm,
}

impl Crypto {
    pub fn new() -> Result<Self> {
        let key_bytes = Self::load_or_create_key()?;
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| anyhow::anyhow!("failed to create cipher: {}", e))?;
        Ok(Self { cipher })
    }

    fn load_or_create_key() -> Result<Vec<u8>> {
        let entry = keyring::Entry::new(SERVICE_NAME, KEY_NAME)
            .context("failed to create keyring entry")?;

        match entry.get_password() {
            Ok(encoded) => {
                let bytes = hex::decode(&encoded)
                    .context("failed to decode master key from keyring")?;
                if bytes.len() == 32 {
                    Ok(bytes)
                } else {
                    anyhow::bail!("invalid key length in keyring: {}", bytes.len());
                }
            }
            Err(_) => {
                let mut key = vec![0u8; 32];
                rand::thread_rng().fill_bytes(&mut key);
                let encoded = hex::encode(&key);
                entry.set_password(&encoded)
                    .context("failed to save master key to keyring")?;
                Ok(key)
            }
        }
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self.cipher.encrypt(nonce, plaintext)
            .map_err(|e| anyhow::anyhow!("encryption failed: {}", e))?;

        let mut output = Vec::with_capacity(12 + ciphertext.len());
        output.extend_from_slice(&nonce_bytes);
        output.extend_from_slice(&ciphertext);
        Ok(output)
    }

    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 12 {
            anyhow::bail!("ciphertext too short");
        }
        let nonce = Nonce::from_slice(&data[..12]);
        let ciphertext = &data[12..];

        self.cipher.decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("decryption failed: {}", e))
    }

    pub fn encrypt_string(&self, plaintext: &str) -> Result<Vec<u8>> {
        self.encrypt(plaintext.as_bytes())
    }

    pub fn decrypt_string(&self, data: &[u8]) -> Result<String> {
        let bytes = self.decrypt(data)?;
        String::from_utf8(bytes).context("decrypted data is not valid UTF-8")
    }
}
```

- [ ] **Step 2: 在 core/Cargo.toml 添加 hex 依赖**

在 `crates/core/Cargo.toml` 的 `[dependencies]` 中添加：

```toml
hex = "0.4"
```

- [ ] **Step 3: 验证编译**

```bash
cargo build -p anyrouter-core
```

- [ ] **Step 4: 提交**

```bash
git add crates/core/src/crypto.rs crates/core/Cargo.toml
git commit -m "feat(core): 实现 AES-256-GCM 加密模块（keyring 主密钥管理）"
```

---

### Task 5: 实现 Playwright 子进程调用

**Files:**
- Create: `crates/core/src/playwright.rs`

- [ ] **Step 1: 定义协议数据结构**

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use anyhow::{Result, Context};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaywrightAction {
    Checkin,
    Login,
    FetchDetail,
}

#[derive(Debug, Serialize)]
pub struct PlaywrightPayload {
    pub action: PlaywrightAction,
    pub headless: bool,
    pub timeout_ms: u64,
    pub accounts: Vec<PlaywrightAccountInput>,
}

#[derive(Debug, Serialize)]
pub struct PlaywrightAccountInput {
    pub name: String,
    pub provider: String,
    pub domain: String,
    pub login_path: String,
    pub sign_in_path: Option<String>,
    pub user_info_path: String,
    pub tokens_path: Option<String>,
    pub logs_path: Option<String>,
    pub chart_path: Option<String>,
    pub api_user_key: String,
    pub api_user: String,
    pub cookies: serde_json::Value,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PlaywrightOutput {
    #[serde(default)]
    pub results: Vec<PlaywrightResult>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PlaywrightResult {
    pub name: String,
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub before: Option<QuotaInfo>,
    #[serde(default)]
    pub after: Option<QuotaInfo>,
    #[serde(default)]
    pub user_info: Option<serde_json::Value>,
    #[serde(default)]
    pub tokens: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub logs: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub chart: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub used_login: bool,
    #[serde(default)]
    pub cookies: Vec<CookieInfo>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct QuotaInfo {
    pub quota: f64,
    pub used_quota: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CookieInfo {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub expires: Option<f64>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}
```

- [ ] **Step 2: 实现子进程调用**

```rust
fn locate_script() -> PathBuf {
    if let Ok(p) = std::env::var("PLAYWRIGHT_SCRIPT") {
        return PathBuf::from(p);
    }
    let rel = PathBuf::from("scripts/playwright_checkin.py");
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors().take(5) {
            let candidate = ancestor.join(&rel);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    rel
}

fn locate_python() -> String {
    std::env::var("PYTHON_BIN").unwrap_or_else(|_| {
        if cfg!(windows) { "python".to_string() } else { "python3".to_string() }
    })
}

pub async fn run_playwright(payload: &PlaywrightPayload) -> Result<PlaywrightOutput> {
    let script = locate_script();
    let python = locate_python();

    if !script.is_file() {
        anyhow::bail!("Playwright script not found: {}", script.display());
    }

    let payload_str = serde_json::to_string(payload)?;

    let mut child = Command::new(&python)
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context(format!("failed to spawn python ({})", python))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(payload_str.as_bytes()).await?;
        stdin.shutdown().await?;
    }

    let output = child.wait_with_output().await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Playwright exited with {:?}: {}", output.status.code(), stderr.chars().take(500).collect::<String>());
    }

    let parsed: PlaywrightOutput = serde_json::from_slice(&output.stdout)
        .context("failed to parse Playwright JSON output")?;

    if let Some(err) = &parsed.error {
        anyhow::bail!("Playwright error: {}", err);
    }

    Ok(parsed)
}
```

- [ ] **Step 3: 提交**

```bash
git add crates/core/src/playwright.rs
git commit -m "feat(core): 实现 Playwright 子进程调用协议"
```

---

### Task 6: 实现服务层

**Files:**
- Create: `crates/core/src/service.rs`

- [ ] **Step 1: 实现签到与登录服务编排**

```rust
use anyhow::Result;
use crate::models::*;
use crate::playwright::*;
use crate::storage::Storage;

pub struct CheckinResult {
    pub account_name: String,
    pub success: bool,
    pub balance_before: Option<f64>,
    pub balance_after: Option<f64>,
    pub error: Option<String>,
}

pub async fn checkin_accounts(
    storage: &Storage,
    site: &Site,
    accounts: &[Account],
    headless: bool,
) -> Result<Vec<CheckinResult>> {
    let pw_accounts: Vec<PlaywrightAccountInput> = accounts.iter().map(|a| {
        PlaywrightAccountInput {
            name: a.name.clone(),
            provider: site.name.clone(),
            domain: site.domain.clone(),
            login_path: site.login_path.clone(),
            sign_in_path: site.sign_in_path.clone(),
            user_info_path: site.user_info_path.clone(),
            tokens_path: Some(site.tokens_path.clone()),
            logs_path: Some(site.logs_path.clone()),
            chart_path: Some(site.chart_path.clone()),
            api_user_key: site.api_user_key.clone(),
            api_user: a.api_user.clone(),
            cookies: a.cookies.as_ref()
                .and_then(|c| serde_json::from_str(c).ok())
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
            username: a.username.clone(),
            password: a.password.clone(),
        }
    }).collect();

    let payload = PlaywrightPayload {
        action: PlaywrightAction::Checkin,
        headless,
        timeout_ms: 30000,
        accounts: pw_accounts,
    };

    let output = run_playwright(&payload).await?;

    let mut results = Vec::new();
    for r in output.results {
        // 更新 Cookie（如果回传了新的）
        if !r.cookies.is_empty() {
            if let Some(account) = accounts.iter().find(|a| a.name == r.name) {
                let cookies_json = serde_json::to_string(&r.cookies)?;
                let now = chrono::Utc::now().to_rfc3339();
                let expires = r.cookies.iter()
                    .find(|c| c.name == "session")
                    .and_then(|c| c.expires)
                    .map(|ts| chrono::DateTime::from_timestamp(ts as i64, 0)
                        .map(|dt| dt.to_rfc3339()))
                    .flatten();
                let _ = storage.update_account_cookies(
                    account.id, &cookies_json, &now, expires.as_deref()
                );
            }
        }

        results.push(CheckinResult {
            account_name: r.name,
            success: r.success,
            balance_before: r.before.map(|q| q.quota),
            balance_after: r.after.map(|q| q.quota),
            error: r.error,
        });
    }

    Ok(results)
}

pub async fn login_account(
    storage: &Storage,
    site: &Site,
    account: &Account,
    headless: bool,
) -> Result<()> {
    let pw_account = PlaywrightAccountInput {
        name: account.name.clone(),
        provider: site.name.clone(),
        domain: site.domain.clone(),
        login_path: site.login_path.clone(),
        sign_in_path: site.sign_in_path.clone(),
        user_info_path: site.user_info_path.clone(),
        tokens_path: None,
        logs_path: None,
        chart_path: None,
        api_user_key: site.api_user_key.clone(),
        api_user: account.api_user.clone(),
        cookies: serde_json::Value::Object(serde_json::Map::new()),
        username: account.username.clone(),
        password: account.password.clone(),
    };

    let payload = PlaywrightPayload {
        action: PlaywrightAction::Login,
        headless,
        timeout_ms: 30000,
        accounts: vec![pw_account],
    };

    let output = run_playwright(&payload).await?;
    let result = output.results.into_iter().next()
        .ok_or_else(|| anyhow::anyhow!("no result from login"))?;

    if !result.success {
        anyhow::bail!("login failed: {}", result.error.unwrap_or_default());
    }

    if !result.cookies.is_empty() {
        let cookies_json = serde_json::to_string(&result.cookies)?;
        let now = chrono::Utc::now().to_rfc3339();
        let expires = result.cookies.iter()
            .find(|c| c.name == "session")
            .and_then(|c| c.expires)
            .map(|ts| chrono::DateTime::from_timestamp(ts as i64, 0)
                .map(|dt| dt.to_rfc3339()))
            .flatten();
        storage.update_account_cookies(account.id, &cookies_json, &now, expires.as_deref())?;
    }

    Ok(())
}

pub async fn fetch_detail(
    storage: &Storage,
    site: &Site,
    account: &Account,
    headless: bool,
) -> Result<()> {
    let pw_account = PlaywrightAccountInput {
        name: account.name.clone(),
        provider: site.name.clone(),
        domain: site.domain.clone(),
        login_path: site.login_path.clone(),
        sign_in_path: site.sign_in_path.clone(),
        user_info_path: site.user_info_path.clone(),
        tokens_path: Some(site.tokens_path.clone()),
        logs_path: Some(site.logs_path.clone()),
        chart_path: Some(site.chart_path.clone()),
        api_user_key: site.api_user_key.clone(),
        api_user: account.api_user.clone(),
        cookies: account.cookies.as_ref()
            .and_then(|c| serde_json::from_str(c).ok())
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        username: account.username.clone(),
        password: account.password.clone(),
    };

    let payload = PlaywrightPayload {
        action: PlaywrightAction::FetchDetail,
        headless,
        timeout_ms: 60000,
        accounts: vec![pw_account],
    };

    let output = run_playwright(&payload).await?;
    let result = output.results.into_iter().next()
        .ok_or_else(|| anyhow::anyhow!("no result from fetch_detail"))?;

    if !result.success {
        anyhow::bail!("fetch_detail failed: {}", result.error.unwrap_or_default());
    }

    // 保存缓存
    if let Some(info) = &result.user_info {
        storage.upsert_cache(account.id, CacheKind::Overview, &serde_json::to_string(info)?)?;
    }
    if let Some(tokens) = &result.tokens {
        storage.upsert_cache(account.id, CacheKind::Tokens, &serde_json::to_string(tokens)?)?;
    }
    if let Some(logs) = &result.logs {
        storage.upsert_cache(account.id, CacheKind::Logs, &serde_json::to_string(logs)?)?;
    }
    if let Some(chart) = &result.chart {
        storage.upsert_cache(account.id, CacheKind::Chart, &serde_json::to_string(chart)?)?;
    }

    // 更新 Cookie
    if !result.cookies.is_empty() {
        let cookies_json = serde_json::to_string(&result.cookies)?;
        let now = chrono::Utc::now().to_rfc3339();
        let expires = result.cookies.iter()
            .find(|c| c.name == "session")
            .and_then(|c| c.expires)
            .map(|ts| chrono::DateTime::from_timestamp(ts as i64, 0)
                .map(|dt| dt.to_rfc3339()))
            .flatten();
        storage.update_account_cookies(account.id, &cookies_json, &now, expires.as_deref())?;
    }

    Ok(())
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/core/src/service.rs
git commit -m "feat(core): 实现服务层（签到/登录/拉取详情编排）"
```

---

### Task 7: 实现 .env 导入逻辑

**Files:**
- Create: `crates/core/src/env_import.rs`

- [ ] **Step 1: 实现首次启动的 .env 导入**

```rust
use anyhow::Result;
use crate::models::*;
use crate::storage::Storage;

pub fn import_env_if_needed(storage: &Storage) -> Result<()> {
    if storage.get_meta("env_imported").is_some() {
        return Ok(());
    }

    // 插入内置站点
    let anyrouter = SiteInput {
        name: "AnyRouter".to_string(),
        domain: "https://anyrouter.top".to_string(),
        login_path: "/login".to_string(),
        sign_in_path: Some("/api/user/sign_in".to_string()),
        user_info_path: "/api/user/self".to_string(),
        tokens_path: "/api/token/".to_string(),
        logs_path: "/api/log/self".to_string(),
        chart_path: "/api/data/self".to_string(),
        api_user_key: "new-api-user".to_string(),
    };
    let anyrouter_id = storage.insert_site(&anyrouter)?;

    let agentrouter = SiteInput {
        name: "AgentRouter".to_string(),
        domain: "https://agentrouter.org".to_string(),
        login_path: "/login".to_string(),
        sign_in_path: None,
        user_info_path: "/api/user/self".to_string(),
        tokens_path: "/api/token/".to_string(),
        logs_path: "/api/log/self".to_string(),
        chart_path: "/api/data/self".to_string(),
        api_user_key: "new-api-user".to_string(),
    };
    let agentrouter_id = storage.insert_site(&agentrouter)?;

    // 导入 ANYROUTER_ACCOUNTS 环境变量
    if let Ok(accounts_json) = std::env::var("ANYROUTER_ACCOUNTS") {
        if let Ok(entries) = serde_json::from_str::<Vec<serde_json::Value>>(&accounts_json) {
            for (i, entry) in entries.iter().enumerate() {
                let provider = entry.get("provider")
                    .and_then(|v| v.as_str())
                    .unwrap_or("anyrouter");
                let site_id = match provider {
                    "agentrouter" => agentrouter_id,
                    _ => anyrouter_id,
                };
                let name = entry.get("name")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("Account {}", i + 1));
                let api_user = entry.get("api_user")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let username = entry.get("_username")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let password = entry.get("_password")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let cookies = entry.get("cookies")
                    .map(|v| serde_json::to_string(v).unwrap_or_default())
                    .filter(|s| s != "null" && s != "[]" && s != "{}");

                let input = AccountInput {
                    site_id,
                    name,
                    api_user,
                    username,
                    password,
                    cookies,
                };
                let _ = storage.insert_account(&input);
            }
        }
    }

    storage.set_meta("env_imported", "true")?;
    Ok(())
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/core/src/env_import.rs
git commit -m "feat(core): 实现 .env 一次性导入逻辑"
```

---

## Phase 3: GUI — 基础框架与主题

### Task 8: 实现主题常量

**Files:**
- Create: `crates/gui/src/theme.rs`

- [ ] **Step 1: 定义深邃黑 + 玻璃拟物感色板**

```rust
use gpui::*;

pub struct Theme;

impl Theme {
    // 背景色
    pub fn bg_window() -> Rgba { rgba(0x0a0c10ff) }
    pub fn bg_card() -> Rgba { rgba(0x141923ff) }
    pub fn bg_bar() -> Rgba { rgba(0x11141aff) }
    pub fn bg_input() -> Rgba { rgba(0x0a0c10ff) }

    // 边框
    pub fn border_normal() -> Rgba { rgba(0x2a2f3aff) }
    pub fn border_strong() -> Rgba { rgba(0x3a4150ff) }

    // 文字
    pub fn text_primary() -> Rgba { rgba(0xdbe2ecff) }
    pub fn text_secondary() -> Rgba { rgba(0xaab2c0ff) }
    pub fn text_muted() -> Rgba { rgba(0x8a93a5ff) }
    pub fn text_dim() -> Rgba { rgba(0x56657dff) }

    // 语义色
    pub fn accent() -> Rgba { rgba(0x7fb0ffff) }
    pub fn success() -> Rgba { rgba(0x6fbf73ff) }
    pub fn error() -> Rgba { rgba(0xe07b7bff) }
    pub fn warning() -> Rgba { rgba(0xdcaa50ff) }

    // 按钮
    pub fn btn_primary_bg() -> Rgba { rgba(0x508cff33) }
    pub fn btn_primary_border() -> Rgba { rgba(0x508cff80) }
    pub fn btn_danger_bg() -> Rgba { rgba(0xdc6e6e1f) }
    pub fn btn_danger_border() -> Rgba { rgba(0xdc6e6e73) }

    // 尺寸
    pub fn radius_card() -> Pixels { px(8.0) }
    pub fn radius_modal() -> Pixels { px(10.0) }
    pub fn radius_button() -> Pixels { px(5.0) }

    // 字号
    pub fn font_title() -> Pixels { px(13.0) }
    pub fn font_body() -> Pixels { px(12.0) }
    pub fn font_small() -> Pixels { px(11.0) }
    pub fn font_tiny() -> Pixels { px(10.0) }
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/gui/src/theme.rs
git commit -m "feat(gui): 定义深邃黑玻璃拟物风格主题常量"
```

---

### Task 9: 实现全局状态 AppState

**Files:**
- Create: `crates/gui/src/app_state.rs`
- Create: `crates/gui/src/actions.rs`

- [ ] **Step 1: 定义 AppState 结构与 Action**

`crates/gui/src/actions.rs`：

```rust
use gpui::*;

// 视图导航
actions!(app, [
    NavigateHome,
    ToggleLogDrawer,
    ClearLog,
    CheckinAll,
]);

#[derive(Debug, Clone, PartialEq)]
pub struct NavigateDetail {
    pub account_id: i64,
}

impl_actions!(app, [NavigateDetail]);

#[derive(Debug, Clone, PartialEq)]
pub struct OpenAccountList {
    pub site_id: i64,
}

impl_actions!(app, [OpenAccountList]);

#[derive(Debug, Clone, PartialEq)]
pub struct OpenSiteForm {
    pub site_id: Option<i64>, // None = 新建
}

impl_actions!(app, [OpenSiteForm]);

#[derive(Debug, Clone, PartialEq)]
pub struct OpenAccountForm {
    pub site_id: i64,
    pub account_id: Option<i64>, // None = 新建
}

impl_actions!(app, [OpenAccountForm]);

#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmDelete {
    pub kind: DeleteKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeleteKind {
    Site(i64),
    Account(i64),
}

impl_actions!(app, [ConfirmDelete]);
```

`crates/gui/src/app_state.rs`：

```rust
use anyrouter_core::models::*;
use chrono::Local;

#[derive(Debug, Clone, PartialEq)]
pub enum ViewKind {
    Home,
    AccountDetail(i64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModalKind {
    AccountList(i64),        // site_id
    SiteForm(Option<i64>),   // None = 新建, Some = 编辑
    AccountForm { site_id: i64, account_id: Option<i64> },
    ConfirmDelete(DeleteTarget),
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeleteTarget {
    Site { id: i64, name: String, account_count: usize },
    Account { id: i64, name: String },
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
}

pub struct AppState {
    pub current_view: ViewKind,
    pub log_drawer_open: bool,
    pub active_modal: Option<ModalKind>,

    pub sites: Vec<SiteWithStats>,
    pub accounts_cache: std::collections::HashMap<i64, Vec<Account>>,
    pub log_entries: Vec<LogEntry>,

    pub running: bool,
    pub run_progress: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            current_view: ViewKind::Home,
            log_drawer_open: false,
            active_modal: None,
            sites: Vec::new(),
            accounts_cache: std::collections::HashMap::new(),
            log_entries: Vec::new(),
            running: false,
            run_progress: None,
        }
    }

    pub fn add_log(&mut self, level: LogLevel, message: impl Into<String>) {
        let timestamp = Local::now().format("%H:%M:%S").to_string();
        self.log_entries.push(LogEntry {
            timestamp,
            level,
            message: message.into(),
        });
    }

    pub fn total_accounts(&self) -> usize {
        self.sites.iter().map(|s| s.account_count).sum()
    }

    pub fn total_balance(&self) -> f64 {
        self.sites.iter().map(|s| s.total_balance).sum()
    }
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/gui/src/app_state.rs crates/gui/src/actions.rs
git commit -m "feat(gui): 实现全局状态 AppState 与 Action 定义"
```

---

### Task 10: 实现 RootView（标题栏 + 内容区 + 底部栏）

**Files:**
- Create: `crates/gui/src/views/root.rs`
- Create: `crates/gui/src/views/mod.rs`
- Modify: `crates/gui/src/main.rs`

- [ ] **Step 1: 创建 views 模块**

`crates/gui/src/views/mod.rs`：

```rust
pub mod root;
pub mod home;
pub mod account_detail;
pub mod log_drawer;
```

- [ ] **Step 2: 实现 RootView 布局**

`crates/gui/src/views/root.rs`：

```rust
use gpui::*;
use crate::app_state::*;
use crate::theme::Theme;

pub struct RootView {
    state: Model<AppState>,
}

impl RootView {
    pub fn new(state: Model<AppState>, cx: &mut ViewContext<Self>) -> Self {
        Self { state }
    }
}

impl Render for RootView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let running = state.running;
        let log_open = state.log_drawer_open;

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(Theme::bg_window())
            .text_color(Theme::text_primary())
            .font_size(Theme::font_body())
            .child(self.render_title_bar(cx))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child("内容区域占位")
            )
            .child(self.render_bottom_bar(running, log_open, cx))
    }
}

impl RootView {
    fn render_title_bar(&self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .w_full()
            .h(px(40.0))
            .bg(Theme::bg_bar())
            .border_b_1()
            .border_color(Theme::border_normal())
            .flex()
            .items_center()
            .px(px(16.0))
            .justify_between()
            .child(
                div()
                    .text_color(Theme::text_muted())
                    .font_size(Theme::font_title())
                    .child("🅰 AnyRouter · Auto Check-in")
            )
            .child(
                div()
                    .px(px(12.0))
                    .py(px(4.0))
                    .bg(Theme::btn_primary_bg())
                    .border_1()
                    .border_color(Theme::btn_primary_border())
                    .rounded(Theme::radius_button())
                    .text_color(Theme::accent())
                    .font_size(Theme::font_small())
                    .cursor_pointer()
                    .child("⚡ 一键签到全部")
            )
    }

    fn render_bottom_bar(&self, running: bool, log_open: bool, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let status_text = if running { "● 运行中…" } else { "● 就绪" };
        let status_color = if running { Theme::warning() } else { Theme::success() };
        let log_btn_border = if log_open { Theme::btn_primary_border() } else { Theme::border_normal() };
        let log_btn_text = if log_open { Theme::accent() } else { Theme::text_muted() };

        div()
            .w_full()
            .h(px(30.0))
            .bg(Theme::bg_bar())
            .border_t_1()
            .border_color(Theme::border_normal())
            .flex()
            .items_center()
            .px(px(12.0))
            .justify_between()
            .child(
                div()
                    .text_color(status_color)
                    .font_size(Theme::font_tiny())
                    .child(status_text)
            )
            .child(
                div()
                    .px(px(8.0))
                    .py(px(2.0))
                    .border_1()
                    .border_color(log_btn_border)
                    .rounded(Theme::radius_button())
                    .text_color(log_btn_text)
                    .font_size(Theme::font_tiny())
                    .cursor_pointer()
                    .child("▤ 日志")
            )
    }
}
```

- [ ] **Step 3: 更新 main.rs 使用 RootView**

```rust
mod app_state;
mod actions;
mod theme;
mod views;

use gpui::*;
use app_state::AppState;
use views::root::RootView;

fn main() {
    App::new().run(|cx: &mut AppContext| {
        let state = cx.new_model(|_cx| AppState::new());

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1100.0), px(750.0)),
                    cx,
                ))),
                ..Default::default()
            },
            |cx| cx.new_view(|cx| RootView::new(state.clone(), cx)),
        )
        .unwrap();
    });
}
```

- [ ] **Step 4: 验证编译并运行**

```bash
cargo build -p anyrouter-gui
cargo run -p anyrouter-gui
```

Expected: 窗口打开，显示深黑色背景 + 标题栏 + 底部栏。

- [ ] **Step 5: 提交**

```bash
git add crates/gui/src/
git commit -m "feat(gui): 实现 RootView 基础布局（标题栏 + 底部栏）"
```

---

## Phase 4: GUI — 主页视图

### Task 11: 实现统计条组件

**Files:**
- Create: `crates/gui/src/components/mod.rs`
- Create: `crates/gui/src/components/stat_strip.rs`

- [ ] **Step 1: 创建 components 模块**

`crates/gui/src/components/mod.rs`：

```rust
pub mod stat_strip;
pub mod site_card;
pub mod modal;
pub mod account_list;
pub mod site_form;
pub mod account_form;
pub mod confirm_dialog;
pub mod bar_chart;
```

- [ ] **Step 2: 实现统计条**

`crates/gui/src/components/stat_strip.rs`：

```rust
use gpui::*;
use crate::theme::Theme;

pub struct StatStripData {
    pub site_count: usize,
    pub account_count: usize,
    pub total_balance: f64,
    pub checkin_today: String,
}

pub fn render_stat_strip(data: &StatStripData) -> impl IntoElement {
    div()
        .w_full()
        .px(px(12.0))
        .py(px(6.0))
        .mb(px(12.0))
        .bg(Theme::bg_bar())
        .border_1()
        .border_color(Theme::border_normal())
        .rounded(Theme::radius_card())
        .flex()
        .gap(px(20.0))
        .font_size(Theme::font_small())
        .text_color(Theme::text_muted())
        .child(render_stat_item("站点", &data.site_count.to_string()))
        .child(render_stat_item("账户", &data.account_count.to_string()))
        .child(render_stat_item("总余额", &format!("${:.2}", data.total_balance)))
        .child(render_stat_item("今日签到", &data.checkin_today))
}

fn render_stat_item(label: &str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .gap(px(4.0))
        .child(div().child(label.to_string()))
        .child(
            div()
                .text_color(Theme::text_primary())
                .font_weight(FontWeight::BOLD)
                .child(value.to_string())
        )
}
```

- [ ] **Step 3: 提交**

```bash
git add crates/gui/src/components/
git commit -m "feat(gui): 实现统计条组件"
```

---

### Task 12: 实现站点卡片组件

**Files:**
- Create: `crates/gui/src/components/site_card.rs`

- [ ] **Step 1: 实现站点卡片**

```rust
use gpui::*;
use anyrouter_core::models::SiteWithStats;
use crate::theme::Theme;

pub fn render_site_card(site: &SiteWithStats) -> impl IntoElement {
    let has_expired = site.expired_count > 0;
    let account_text = if has_expired {
        format!("账户: {} 个（{} 个 Cookie 过期 ⚠）", site.account_count, site.expired_count)
    } else {
        format!("账户: {} 个", site.account_count)
    };

    div()
        .w(px(200.0))
        .p(px(12.0))
        .bg(Theme::bg_card())
        .border_1()
        .border_color(Theme::border_normal())
        .rounded(Theme::radius_card())
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .font_size(Theme::font_title())
                .text_color(Theme::text_primary())
                .font_weight(FontWeight::SEMIBOLD)
                .child(site.site.name.clone())
        )
        .child(
            div()
                .font_size(Theme::font_tiny())
                .text_color(Theme::text_dim())
                .child(site.site.domain.clone())
        )
        .child(
            div()
                .font_size(Theme::font_small())
                .text_color(if has_expired { Theme::warning() } else { Theme::text_secondary() })
                .child(account_text)
        )
        .child(
            div()
                .font_size(Theme::font_small())
                .text_color(Theme::text_secondary())
                .child(format!("余额合计: ${:.2}", site.total_balance))
        )
        .child(
            div()
                .mt(px(8.0))
                .flex()
                .gap(px(8.0))
                .items_center()
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(3.0))
                        .bg(Theme::btn_primary_bg())
                        .border_1()
                        .border_color(Theme::btn_primary_border())
                        .rounded(Theme::radius_button())
                        .text_color(Theme::accent())
                        .font_size(Theme::font_tiny())
                        .cursor_pointer()
                        .child("查看账户")
                )
                .child(
                    div()
                        .text_color(Theme::text_dim())
                        .font_size(Theme::font_tiny())
                        .cursor_pointer()
                        .child("✎ 编辑 · 🗑")
                )
        )
}

pub fn render_new_site_card() -> impl IntoElement {
    div()
        .w(px(200.0))
        .h(px(140.0))
        .border_1()
        .border_color(Theme::border_normal())
        .rounded(Theme::radius_card())
        .flex()
        .items_center()
        .justify_center()
        .text_color(Theme::text_dim())
        .font_size(Theme::font_body())
        .cursor_pointer()
        .child("+ 新建站点")
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/gui/src/components/site_card.rs
git commit -m "feat(gui): 实现站点卡片组件"
```

---

### Task 13: 实现 HomeView

**Files:**
- Create: `crates/gui/src/views/home.rs`
- Modify: `crates/gui/src/views/root.rs` (接入 HomeView)

- [ ] **Step 1: 实现主页视图**

`crates/gui/src/views/home.rs`：

```rust
use gpui::*;
use crate::app_state::AppState;
use crate::components::stat_strip::*;
use crate::components::site_card::*;
use crate::theme::Theme;

pub struct HomeView {
    state: Model<AppState>,
}

impl HomeView {
    pub fn new(state: Model<AppState>) -> Self {
        Self { state }
    }
}

impl Render for HomeView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let stat_data = StatStripData {
            site_count: state.sites.len(),
            account_count: state.total_accounts(),
            total_balance: state.total_balance(),
            checkin_today: format!("{}/{}", 0, state.total_accounts()),
        };

        let mut content = div()
            .size_full()
            .p(px(16.0))
            .flex()
            .flex_col()
            .child(render_stat_strip(&stat_data))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(12.0))
            );

        // 动态添加站点卡片 - 在实际实现中通过迭代 state.sites 生成
        // 这里展示结构，实际代码需要根据 GPUI 的 children API 调整
        div()
            .size_full()
            .p(px(16.0))
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .child(render_stat_strip(&stat_data))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(12.0))
                    .children(
                        state.sites.iter().map(|site| render_site_card(site)).collect::<Vec<_>>()
                    )
                    .child(render_new_site_card())
            )
    }
}
```

- [ ] **Step 2: 在 RootView 中接入 HomeView**

修改 `root.rs` 中内容区域占位为 HomeView 组件渲染。

- [ ] **Step 3: 验证编译运行**

```bash
cargo run -p anyrouter-gui
```

Expected: 窗口显示统计条 + "新建站点"占位卡片。

- [ ] **Step 4: 提交**

```bash
git add crates/gui/src/views/home.rs crates/gui/src/views/root.rs
git commit -m "feat(gui): 实现主页视图（统计条 + 站点卡片网格）"
```

---

## Phase 5: GUI — 弹窗与表单

### Task 14: 实现通用弹窗容器

**Files:**
- Create: `crates/gui/src/components/modal.rs`

- [ ] **Step 1: 实现 Modal 容器组件**

```rust
use gpui::*;
use crate::theme::Theme;

pub fn render_modal(title: &str, width: Pixels, content: impl IntoElement) -> impl IntoElement {
    // 遮罩层
    div()
        .absolute()
        .inset_0()
        .bg(rgba(0x00000088))
        .flex()
        .items_center()
        .justify_center()
        .child(
            // 弹窗主体
            div()
                .w(width)
                .max_h(px(500.0))
                .bg(Theme::bg_bar())
                .border_1()
                .border_color(Theme::border_strong())
                .rounded(Theme::radius_modal())
                .p(px(16.0))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .font_size(Theme::font_title())
                                .text_color(Theme::text_primary())
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title.to_string())
                        )
                        .child(
                            div()
                                .cursor_pointer()
                                .text_color(Theme::text_muted())
                                .child("✕")
                        )
                )
                .child(content)
        )
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/gui/src/components/modal.rs
git commit -m "feat(gui): 实现通用弹窗容器组件"
```

---

### Task 15: 实现账户列表弹窗

**Files:**
- Create: `crates/gui/src/components/account_list.rs`

- [ ] **Step 1: 实现账户行 + 列表**

```rust
use gpui::*;
use anyrouter_core::models::Account;
use crate::theme::Theme;

pub fn render_account_list(accounts: &[Account], site_name: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .children(accounts.iter().map(|a| render_account_row(a)).collect::<Vec<_>>())
        .child(
            div()
                .mt(px(8.0))
                .flex()
                .gap(px(8.0))
                .child(
                    div()
                        .px(px(10.0))
                        .py(px(4.0))
                        .bg(Theme::btn_primary_bg())
                        .border_1()
                        .border_color(Theme::btn_primary_border())
                        .rounded(Theme::radius_button())
                        .text_color(Theme::accent())
                        .font_size(Theme::font_small())
                        .cursor_pointer()
                        .child("+ 新增账户")
                )
                .child(
                    div()
                        .px(px(10.0))
                        .py(px(4.0))
                        .bg(Theme::btn_primary_bg())
                        .border_1()
                        .border_color(Theme::btn_primary_border())
                        .rounded(Theme::radius_button())
                        .text_color(Theme::accent())
                        .font_size(Theme::font_small())
                        .cursor_pointer()
                        .child("⚡ 签到本站点")
                )
        )
}

fn render_account_row(account: &Account) -> impl IntoElement {
    let cookie_status = match &account.cookie_expires_at {
        Some(exp) => format!("Cookie 至 {}", &exp[5..10]),
        None if account.cookies.is_some() => "Cookie 有效期未知".to_string(),
        None if account.username.is_some() => "仅账密（未登录过）".to_string(),
        _ => "无凭据".to_string(),
    };

    div()
        .w_full()
        .px(px(10.0))
        .py(px(6.0))
        .bg(Theme::bg_card())
        .border_1()
        .border_color(Theme::border_normal())
        .rounded(px(6.0))
        .flex()
        .justify_between()
        .items_center()
        .child(
            div()
                .font_size(Theme::font_small())
                .text_color(Theme::text_secondary())
                .child(format!("{} · {}", account.name, cookie_status))
        )
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .font_size(Theme::font_tiny())
                .text_color(Theme::text_dim())
                .child(div().cursor_pointer().child("[详情]"))
                .child(div().cursor_pointer().child("[✎]"))
                .child(div().cursor_pointer().child("[🗑]"))
        )
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/gui/src/components/account_list.rs
git commit -m "feat(gui): 实现账户列表弹窗组件"
```

---

### Task 16: 实现站点/账户表单与确认弹窗

**Files:**
- Create: `crates/gui/src/components/site_form.rs`
- Create: `crates/gui/src/components/account_form.rs`
- Create: `crates/gui/src/components/confirm_dialog.rs`

- [ ] **Step 1: 实现站点表单组件**

`crates/gui/src/components/site_form.rs`：

```rust
use gpui::*;
use crate::theme::Theme;

pub struct SiteFormState {
    pub name: String,
    pub domain: String,
    pub show_advanced: bool,
    pub login_path: String,
    pub sign_in_path: String,
    pub user_info_path: String,
    pub tokens_path: String,
    pub logs_path: String,
    pub chart_path: String,
    pub api_user_key: String,
}

impl SiteFormState {
    pub fn new_empty() -> Self {
        Self {
            name: String::new(),
            domain: String::new(),
            show_advanced: false,
            login_path: "/login".to_string(),
            sign_in_path: "/api/user/sign_in".to_string(),
            user_info_path: "/api/user/self".to_string(),
            tokens_path: "/api/token/".to_string(),
            logs_path: "/api/log/self".to_string(),
            chart_path: "/api/data/self".to_string(),
            api_user_key: "new-api-user".to_string(),
        }
    }
}

pub fn render_site_form(state: &SiteFormState) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(render_field("站点名称 *", &state.name))
        .child(render_field("域名 *", &state.domain))
        .child(
            div()
                .text_color(Theme::text_muted())
                .font_size(Theme::font_tiny())
                .cursor_pointer()
                .child(if state.show_advanced { "▾ 高级设置" } else { "▸ 高级设置" })
        )
        .child(render_form_buttons())
}

fn render_field(label: &str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .font_size(Theme::font_tiny())
                .text_color(Theme::text_muted())
                .child(label.to_string())
        )
        .child(
            div()
                .w_full()
                .px(px(8.0))
                .py(px(4.0))
                .bg(Theme::bg_input())
                .border_1()
                .border_color(Theme::border_normal())
                .rounded(Theme::radius_button())
                .font_size(Theme::font_small())
                .text_color(Theme::text_primary())
                .child(if value.is_empty() { "—".to_string() } else { value.to_string() })
        )
}

fn render_form_buttons() -> impl IntoElement {
    div()
        .mt(px(8.0))
        .flex()
        .justify_end()
        .gap(px(8.0))
        .child(
            div()
                .px(px(12.0))
                .py(px(4.0))
                .bg(Theme::bg_card())
                .border_1()
                .border_color(Theme::border_normal())
                .rounded(Theme::radius_button())
                .text_color(Theme::text_muted())
                .font_size(Theme::font_small())
                .cursor_pointer()
                .child("取消")
        )
        .child(
            div()
                .px(px(12.0))
                .py(px(4.0))
                .bg(Theme::btn_primary_bg())
                .border_1()
                .border_color(Theme::btn_primary_border())
                .rounded(Theme::radius_button())
                .text_color(Theme::accent())
                .font_size(Theme::font_small())
                .cursor_pointer()
                .child("保存")
        )
}
```

- [ ] **Step 2: 实现账户表单组件**

`crates/gui/src/components/account_form.rs`（结构类似 site_form，增加 Cookie/密码字段）。

- [ ] **Step 3: 实现确认删除弹窗**

`crates/gui/src/components/confirm_dialog.rs`：

```rust
use gpui::*;
use crate::theme::Theme;

pub fn render_confirm_dialog(title: &str, message: &str, danger_text: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .font_size(Theme::font_body())
                .text_color(Theme::text_secondary())
                .line_height(px(20.0))
                .child(message.to_string())
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(
                    div()
                        .px(px(12.0))
                        .py(px(4.0))
                        .bg(Theme::bg_card())
                        .border_1()
                        .border_color(Theme::border_normal())
                        .rounded(Theme::radius_button())
                        .text_color(Theme::text_muted())
                        .font_size(Theme::font_small())
                        .cursor_pointer()
                        .child("取消")
                )
                .child(
                    div()
                        .px(px(12.0))
                        .py(px(4.0))
                        .bg(Theme::btn_danger_bg())
                        .border_1()
                        .border_color(Theme::btn_danger_border())
                        .rounded(Theme::radius_button())
                        .text_color(Theme::error())
                        .font_size(Theme::font_small())
                        .cursor_pointer()
                        .child(danger_text.to_string())
                )
        )
}
```

- [ ] **Step 4: 提交**

```bash
git add crates/gui/src/components/site_form.rs crates/gui/src/components/account_form.rs crates/gui/src/components/confirm_dialog.rs
git commit -m "feat(gui): 实现站点表单、账户表单与确认删除弹窗组件"
```

---

## Phase 6: GUI — 账户详情页

### Task 17: 实现账户详情视图

**Files:**
- Create: `crates/gui/src/views/account_detail.rs`

- [ ] **Step 1: 实现详情页框架（面包屑 + Tab 切换 + 概览 Tab）**

```rust
use gpui::*;
use crate::app_state::AppState;
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DetailTab {
    Overview,
    Tokens,
    Logs,
    Chart,
}

pub struct AccountDetailView {
    state: Model<AppState>,
    account_id: i64,
    active_tab: DetailTab,
}

impl AccountDetailView {
    pub fn new(state: Model<AppState>, account_id: i64) -> Self {
        Self { state, account_id, active_tab: DetailTab::Overview }
    }
}

impl Render for AccountDetailView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p(px(16.0))
            .flex()
            .flex_col()
            .child(self.render_breadcrumb(cx))
            .child(self.render_tabs(cx))
            .child(
                div()
                    .flex_1()
                    .mt(px(12.0))
                    .overflow_y_scroll()
                    .child(self.render_tab_content(cx))
            )
    }
}

impl AccountDetailView {
    fn render_breadcrumb(&self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .mb(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .text_color(Theme::text_dim())
                    .font_size(Theme::font_small())
                    .cursor_pointer()
                    .child("◀ 返回主页")
            )
            .child(
                div()
                    .text_color(Theme::text_dim())
                    .font_size(Theme::font_small())
                    .child("/")
            )
            .child(
                div()
                    .text_color(Theme::text_primary())
                    .font_size(Theme::font_small())
                    .child("账户详情")
            )
    }

    fn render_tabs(&self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        let tabs = [
            (DetailTab::Overview, "概览"),
            (DetailTab::Tokens, "API 密钥"),
            (DetailTab::Logs, "使用日志"),
            (DetailTab::Chart, "消耗图表"),
        ];

        div()
            .flex()
            .gap(px(16.0))
            .border_b_1()
            .border_color(Theme::border_normal())
            .pb(px(6.0))
            .children(tabs.into_iter().map(|(tab, label)| {
                let is_active = tab == self.active_tab;
                let text_color = if is_active { Theme::accent() } else { Theme::text_muted() };
                let mut el = div()
                    .font_size(Theme::font_small())
                    .text_color(text_color)
                    .cursor_pointer()
                    .child(label);
                if is_active {
                    el = el.border_b_2().border_color(Theme::accent());
                }
                el
            }).collect::<Vec<_>>())
    }

    fn render_tab_content(&self, _cx: &mut ViewContext<Self>) -> impl IntoElement {
        match self.active_tab {
            DetailTab::Overview => self.render_overview(),
            DetailTab::Tokens => div().child("API 密钥列表（待实现）"),
            DetailTab::Logs => div().child("使用日志（待实现）"),
            DetailTab::Chart => div().child("消耗图表（待实现）"),
        }
    }

    fn render_overview(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(self.render_info_card("💵 余额 $0.00 │ 📊 累计消耗 $0.00 │ 🔢 请求数 0"))
            .child(self.render_info_card("🍪 Cookie: 无"))
            .child(self.render_info_card("👤 用户信息: 加载中..."))
    }

    fn render_info_card(&self, text: &str) -> impl IntoElement {
        div()
            .w_full()
            .px(px(10.0))
            .py(px(8.0))
            .bg(Theme::bg_card())
            .border_1()
            .border_color(Theme::border_normal())
            .rounded(px(6.0))
            .font_size(Theme::font_small())
            .text_color(Theme::text_secondary())
            .child(text.to_string())
    }
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/gui/src/views/account_detail.rs
git commit -m "feat(gui): 实现账户详情页视图（面包屑 + Tab 切换 + 概览）"
```

---

### Task 18: 实现柱状图组件

**Files:**
- Create: `crates/gui/src/components/bar_chart.rs`

- [ ] **Step 1: 实现 div 自绘柱状图**

```rust
use gpui::*;
use crate::theme::Theme;

pub struct BarChartData {
    pub bars: Vec<BarItem>,
    pub max_value: f64,
}

pub struct BarItem {
    pub label: String,
    pub value: f64,
}

pub fn render_bar_chart(data: &BarChartData, height: Pixels) -> impl IntoElement {
    let max_val = if data.max_value > 0.0 { data.max_value } else { 1.0 };

    div()
        .w_full()
        .h(height)
        .flex()
        .items_end()
        .gap(px(4.0))
        .px(px(8.0))
        .pb(px(20.0))
        .children(data.bars.iter().map(|bar| {
            let bar_height = (bar.value / max_val * 100.0).min(100.0);
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_end()
                .h_full()
                .child(
                    div()
                        .w_full()
                        .h(Pixels(bar_height * height.0 / 120.0))
                        .bg(Theme::accent())
                        .rounded_t(px(3.0))
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .font_size(Theme::font_tiny())
                        .text_color(Theme::text_dim())
                        .child(bar.label.clone())
                )
        }).collect::<Vec<_>>())
}
```

- [ ] **Step 2: 提交**

```bash
git add crates/gui/src/components/bar_chart.rs
git commit -m "feat(gui): 实现 div 自绘柱状图组件"
```

---

## Phase 7: GUI — 日志抽屉

### Task 19: 实现日志抽屉视图

**Files:**
- Create: `crates/gui/src/views/log_drawer.rs`

- [ ] **Step 1: 实现底部日志抽屉**

```rust
use gpui::*;
use crate::app_state::*;
use crate::theme::Theme;

pub struct LogDrawerView {
    state: Model<AppState>,
}

impl LogDrawerView {
    pub fn new(state: Model<AppState>) -> Self {
        Self { state }
    }
}

impl Render for LogDrawerView {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let state = self.state.read(cx);

        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(30.0)) // above bottom bar
            .h(px(200.0))
            .bg(Theme::bg_bar())
            .border_t_1()
            .border_color(Theme::border_strong())
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(12.0))
                    .py(px(4.0))
                    .flex()
                    .justify_between()
                    .items_center()
                    .border_b_1()
                    .border_color(Theme::border_normal())
                    .child(
                        div()
                            .text_color(Theme::text_muted())
                            .font_size(Theme::font_small())
                            .child("运行日志")
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .font_size(Theme::font_tiny())
                            .text_color(Theme::text_dim())
                            .child(div().cursor_pointer().child("[清空]"))
                            .child(div().cursor_pointer().child("[✕]"))
                    )
            )
            .child(
                div()
                    .flex_1()
                    .overflow_y_scroll()
                    .px(px(12.0))
                    .py(px(4.0))
                    .children(
                        state.log_entries.iter().map(|entry| {
                            let color = match entry.level {
                                LogLevel::Success => Theme::success(),
                                LogLevel::Warning => Theme::warning(),
                                LogLevel::Error => Theme::error(),
                                LogLevel::Info => Theme::text_secondary(),
                            };
                            div()
                                .font_size(Theme::font_tiny())
                                .text_color(color)
                                .child(format!("{} {}", entry.timestamp, entry.message))
                        }).collect::<Vec<_>>()
                    )
            )
    }
}
```

- [ ] **Step 2: 在 RootView 中条件渲染日志抽屉**

当 `state.log_drawer_open == true` 时在底部栏上方渲染 LogDrawerView。

- [ ] **Step 3: 提交**

```bash
git add crates/gui/src/views/log_drawer.rs crates/gui/src/views/root.rs
git commit -m "feat(gui): 实现底部日志抽屉视图"
```

---

## Phase 8: 集成与连通

### Task 20: 连接 Core 到 GUI（初始化 + 数据加载）

**Files:**
- Modify: `crates/gui/src/main.rs`

- [ ] **Step 1: 启动时初始化 Storage + 导入 .env + 加载数据**

```rust
mod app_state;
mod actions;
mod theme;
mod views;
mod components;

use gpui::*;
use anyrouter_core::{storage::Storage, env_import, models::*};
use app_state::*;
use views::root::RootView;

fn main() {
    dotenvy::dotenv().ok();

    // 初始化存储
    let db_path = Storage::default_path();
    let storage = Storage::open(&db_path).expect("failed to open database");

    // 首次启动导入 .env
    env_import::import_env_if_needed(&storage).ok();

    // 加载数据
    let sites = storage.list_sites().unwrap_or_default();
    let mut site_stats: Vec<SiteWithStats> = Vec::new();
    for site in &sites {
        let accounts = storage.list_accounts_by_site(site.id).unwrap_or_default();
        let expired = accounts.iter().filter(|a| {
            a.cookie_expires_at.as_ref()
                .map(|exp| exp < &chrono::Utc::now().to_rfc3339())
                .unwrap_or(false)
        }).count();
        site_stats.push(SiteWithStats {
            site: site.clone(),
            account_count: accounts.len(),
            total_balance: 0.0, // 从缓存中获取
            expired_count: expired,
            checkin_today: 0,
        });
    }

    App::new().run(|cx: &mut AppContext| {
        let state = cx.new_model(|_cx| {
            let mut s = AppState::new();
            s.sites = site_stats;
            s
        });

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1100.0), px(750.0)),
                    cx,
                ))),
                ..Default::default()
            },
            |cx| cx.new_view(|cx| RootView::new(state.clone(), cx)),
        )
        .unwrap();
    });
}
```

- [ ] **Step 2: 在 gui/Cargo.toml 添加 dotenvy 依赖**

```toml
dotenvy = "0.15"
```

- [ ] **Step 3: 验证完整流程**

```bash
cargo run -p anyrouter-gui
```

Expected: 窗口启动 → 从 SQLite 加载数据（首次自动导入 .env）→ 显示站点卡片。

- [ ] **Step 4: 提交**

```bash
git add crates/gui/
git commit -m "feat(gui): 连接 Core 层，实现启动初始化与数据加载"
```

---

### Task 21: 实现签到按钮的后台执行逻辑

**Files:**
- Modify: `crates/gui/src/views/root.rs`

- [ ] **Step 1: 为"一键签到全部"按钮绑定 Action 处理**

在 RootView 中注册 `CheckinAll` action handler，spawn Tokio 后台任务执行签到，通过日志回写进度：

```rust
// 在 RootView::new 中注册 action
cx.on_action(|this: &mut Self, _: &crate::actions::CheckinAll, cx| {
    let state = this.state.clone();
    cx.spawn(|_, mut cx| async move {
        // 执行签到逻辑...
        // 通过 cx.update_model(&state, ...) 更新进度
    }).detach();
});
```

- [ ] **Step 2: 提交**

```bash
git add crates/gui/src/views/root.rs
git commit -m "feat(gui): 实现一键签到全部的后台执行逻辑"
```

---

### Task 22: 实现弹窗交互逻辑

**Files:**
- Modify: `crates/gui/src/views/root.rs`

- [ ] **Step 1: 在 RootView 中根据 active_modal 条件渲染弹窗**

```rust
// render() 末尾添加弹窗层
if let Some(modal_kind) = &state.active_modal {
    match modal_kind {
        ModalKind::AccountList(site_id) => {
            // render_modal("账户管理", px(500.0), render_account_list(...))
        }
        ModalKind::SiteForm(id) => {
            // render_modal("站点", px(350.0), render_site_form(...))
        }
        // ...
    }
}
```

- [ ] **Step 2: 绑定卡片按钮的点击事件**

为站点卡片的"查看账户"、"编辑"、"删除"按钮绑定对应的 Action dispatch。

- [ ] **Step 3: 验证弹窗流程**

```bash
cargo run -p anyrouter-gui
```

Expected: 点击站点卡片按钮 → 弹窗出现 → 点击关闭 → 弹窗消失。

- [ ] **Step 4: 提交**

```bash
git add crates/gui/src/
git commit -m "feat(gui): 实现弹窗交互逻辑（账户列表/表单/确认删除）"
```

---

## Phase 9: Python 脚本扩展

### Task 23: 扩展 Playwright 脚本支持 login 和 fetch_detail 任务

**Files:**
- Modify: `scripts/playwright_checkin.py`

- [ ] **Step 1: 添加 action 字段解析和路由**

在 `run()` 函数中根据 payload 的 `action` 字段分发到不同的处理逻辑：

```python
async def run(payload: dict) -> dict:
    action = payload.get("action", "checkin")
    # ...existing setup...

    if action == "login":
        # 仅执行登录，回传 cookies
        pass
    elif action == "fetch_detail":
        # 登录 + 拉取用户信息/密钥/日志/图表
        pass
    else:
        # 现有 checkin 逻辑
        pass
```

- [ ] **Step 2: 实现 fetch_detail 逻辑（拉取四类数据）**

```python
async def process_fetch_detail(context, account, timeout_ms):
    # 1. 注入 Cookie + 过 WAF
    # 2. 必要时自动登录
    # 3. GET /api/user/self → user_info
    # 4. GET /api/token/?p=1&size=100 → tokens
    # 5. GET /api/log/self?p=1&page_size=50 → logs
    # 6. GET /api/data/self?start_timestamp=7天前 → chart
    # 7. 回传所有数据 + context.cookies()
```

- [ ] **Step 3: 确保所有任务回传 cookies[]**

在每个任务结束时调用 `context.cookies()` 并过滤出当前站点域名的 cookie，添加到 result 中。

- [ ] **Step 4: 测试脚本**

```bash
echo '{"action":"checkin","headless":true,"timeout_ms":30000,"accounts":[...]}' | python scripts/playwright_checkin.py
```

- [ ] **Step 5: 提交**

```bash
git add scripts/playwright_checkin.py
git commit -m "feat(scripts): 扩展 Playwright 脚本支持 login 和 fetch_detail 任务"
```

---

## Phase 10: 最终集成与验收

### Task 24: 端到端验证

- [ ] **Step 1: 验证首次启动**

```bash
# 删除旧数据库（如果存在）
rm -f "%APPDATA%/anyrouter-checkin/data.db"
cargo run -p anyrouter-gui
```

Expected: 自动从 .env 导入 → 显示站点卡片。

- [ ] **Step 2: 验证签到流程**

点击"一键签到全部" → 日志抽屉显示进度 → 完成后余额更新。

- [ ] **Step 3: 验证详情页**

点击账户"详情" → 进入详情页 → 点击"刷新数据" → Tab 内容更新。

- [ ] **Step 4: 验证 CRUD 流程**

新建站点 → 新建账户 → 编辑 → 删除 → 确认级联删除。

- [ ] **Step 5: 最终提交**

```bash
git add -A
git commit -m "feat: 完成 GPUI 可视化客户端 v0.1 端到端集成"
```

---

## 依赖注意事项

- **GPUI git 依赖**：GPUI API 不稳定，建议锁定到特定 commit rev
- **gpui-component**：如果需要现成的 Input/Button 组件，可额外引入 `gpui-component` crate
- **Windows 构建**：GPUI 在 Windows 上需要 DirectX/Vulkan 支持，确保安装了 Visual Studio Build Tools
- **rusqlite bundled**：使用 `features = ["bundled"]` 避免需要系统 SQLite 库
