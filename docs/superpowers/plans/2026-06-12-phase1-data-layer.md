# 阶段 1:SQLite 数据层 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 建立 SQLite 持久化层(站点/账户/缓存/元信息四张表)与加密模块,完成 `.env` 一次性导入,并把现有应用的数据源从环境变量切换到 SQLite——界面与签到行为保持不变。

**Architecture:** 新增 `crypto.rs`(keyring 主密钥 + AES-256-GCM 列加密)与 `storage.rs`(rusqlite 同步封装:迁移、CRUD、导入);`main.rs` 启动时初始化并通过适配函数把 SQLite 数据转成现有 `AccountConfig`/`ProviderConfig` 喂给现有 UI。本阶段不改界面。

**Tech Stack:** rusqlite(bundled)、keyring 3(windows-native)、aes-gcm 0.10、rand 0.8、base64 0.22、dirs 5、chrono(已有)。

**规格:** `docs/superpowers/specs/2026-06-12-sqlite-account-management-design.md`(本计划覆盖其「阶段 1」;阶段 2-4 另行撰写计划)

**约定:** 所有命令在 `D:\anyrouter-auto-login\rust-version` 下执行。每个任务以提交结尾,提交信息用中文。

---

### Task 1: 添加依赖

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: 在 `[dependencies]` 末尾追加依赖**

```toml
# SQLite 数据层
rusqlite = { version = "0.32", features = ["bundled"] }
keyring = { version = "3", features = ["windows-native"] }
aes-gcm = "0.10"
rand = "0.8"
base64 = "0.22"
dirs = "5"
```

- [ ] **Step 2: 验证依赖可解析编译**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` 字样,无 error(首次会下载并编译 bundled SQLite,耗时数分钟)

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore(deps): 添加 SQLite 数据层依赖(rusqlite/keyring/aes-gcm 等)"
```

---

### Task 2: crypto 模块(AES-256-GCM + keyring 主密钥)

**Files:**
- Create: `src/crypto.rs`
- Modify: `src/main.rs`(注册模块)
- Test: `src/crypto.rs` 内联 `#[cfg(test)]`

- [ ] **Step 1: 写失败测试 —— 创建 `src/crypto.rs`,只含测试**

```rust
//! 敏感字段加密:主密钥存系统凭据库(Windows 凭据管理器),
//! 值用 AES-256-GCM 加密,存储格式为 nonce(12B) ‖ ciphertext。

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: [u8; 32] = [7u8; 32];

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let c = Crypto::from_key(&TEST_KEY);
        let blob = c.encrypt("session=abc; 中文✓").unwrap();
        assert_ne!(blob, b"session=abc");
        assert_eq!(c.decrypt(&blob).unwrap(), "session=abc; 中文✓");
    }

    #[test]
    fn nonce_is_random_each_time() {
        let c = Crypto::from_key(&TEST_KEY);
        assert_ne!(c.encrypt("x").unwrap(), c.encrypt("x").unwrap());
    }

    #[test]
    fn decrypt_garbage_fails() {
        let c = Crypto::from_key(&TEST_KEY);
        assert!(c.decrypt(b"short").is_err());
        assert!(c.decrypt(&[0u8; 40]).is_err());
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let blob = Crypto::from_key(&TEST_KEY).encrypt("secret").unwrap();
        assert!(Crypto::from_key(&[8u8; 32]).decrypt(&blob).is_err());
    }
}
```

在 `src/main.rs` 的 `mod config;` 之后加一行:

```rust
mod crypto;
```

- [ ] **Step 2: 运行测试确认编译失败**

Run: `cargo test crypto 2>&1 | tail -5`
Expected: FAIL,`cannot find type Crypto`(或同类编译错误)

- [ ] **Step 3: 实现 —— 在 `src/crypto.rs` 顶部(测试模块之前)加入**

```rust
use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{anyhow, Context, Result};
use base64::Engine;

const KEYRING_SERVICE: &str = "anyrouter-checkin";
const KEYRING_USER: &str = "master-key";
const NONCE_LEN: usize = 12;

pub struct Crypto {
    cipher: Aes256Gcm,
}

impl Crypto {
    /// 用给定的 32 字节密钥构造(测试与 keyring 加载共用)。
    pub fn from_key(key: &[u8; 32]) -> Self {
        Self {
            cipher: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key)),
        }
    }

    /// 从系统凭据库加载主密钥;不存在则生成 32 字节随机密钥并写入。
    pub fn from_keyring() -> Result<Self> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .context("无法访问系统凭据库")?;
        let key: [u8; 32] = match entry.get_password() {
            Ok(b64) => base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .ok()
                .and_then(|raw| raw.try_into().ok())
                .ok_or_else(|| anyhow!("系统凭据库中的主密钥格式异常"))?,
            Err(keyring::Error::NoEntry) => {
                let mut key = [0u8; 32];
                use rand::RngCore;
                rand::rngs::OsRng.fill_bytes(&mut key);
                entry
                    .set_password(&base64::engine::general_purpose::STANDARD.encode(key))
                    .context("无法写入系统凭据库")?;
                key
            }
            Err(e) => return Err(anyhow!(e)).context("读取系统凭据库失败"),
        };
        Ok(Self::from_key(&key))
    }

    /// 加密文本,输出 nonce ‖ ciphertext。
    pub fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ct = self
            .cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|e| anyhow!("加密失败: {e}"))?;
        let mut out = nonce.to_vec();
        out.extend_from_slice(&ct);
        Ok(out)
    }

    /// 解密 nonce ‖ ciphertext 格式的数据。
    pub fn decrypt(&self, blob: &[u8]) -> Result<String> {
        if blob.len() <= NONCE_LEN {
            return Err(anyhow!("密文长度异常"));
        }
        let (nonce, ct) = blob.split_at(NONCE_LEN);
        let pt = self
            .cipher
            .decrypt(Nonce::from_slice(nonce), ct)
            .map_err(|e| anyhow!("解密失败: {e}"))?;
        String::from_utf8(pt).context("解密结果不是合法 UTF-8")
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test crypto 2>&1 | tail -5`
Expected: `test result: ok. 4 passed`

- [ ] **Step 5: Commit**

```bash
git add src/crypto.rs src/main.rs
git commit -m "feat(crypto): AES-256-GCM 列加密与 keyring 主密钥管理"
```

---

### Task 3: storage 骨架 —— 模型、建库与迁移

**Files:**
- Create: `src/storage.rs`
- Modify: `src/main.rs`(注册模块)
- Test: `src/storage.rs` 内联 `#[cfg(test)]`

- [ ] **Step 1: 写失败测试 —— 创建 `src/storage.rs`,先放测试与测试辅助**

```rust
//! SQLite 持久化层:站点 / 账户 / 详情缓存 / 元信息。
//! 同步 rusqlite;上层在后台线程/spawn_blocking 中调用,避免阻塞 UI。

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Crypto;

    pub(super) fn test_storage() -> Storage {
        Storage::open_in_memory(Crypto::from_key(&[7u8; 32])).unwrap()
    }

    #[test]
    fn migrate_creates_tables_and_version() {
        let s = test_storage();
        assert_eq!(s.get_meta("schema_version").unwrap().as_deref(), Some("1"));
        // 业务三表为空;meta 表已含 schema_version 一行
        for table in ["sites", "accounts", "account_cache"] {
            let n: i64 = s
                .conn
                .query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{} 应为空表", table);
        }
        let meta_rows: i64 = s
            .conn
            .query_row("SELECT COUNT(*) FROM meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(meta_rows, 1);
    }

    #[test]
    fn migrate_is_idempotent() {
        let mut s = test_storage();
        s.migrate().unwrap(); // 第二次迁移不应报错
        assert_eq!(s.get_meta("schema_version").unwrap().as_deref(), Some("1"));
    }

    #[test]
    fn meta_set_get_roundtrip() {
        let s = test_storage();
        assert!(s.get_meta("env_imported").unwrap().is_none());
        s.set_meta("env_imported", "1").unwrap();
        assert_eq!(s.get_meta("env_imported").unwrap().as_deref(), Some("1"));
        s.set_meta("env_imported", "2").unwrap(); // 覆盖写
        assert_eq!(s.get_meta("env_imported").unwrap().as_deref(), Some("2"));
    }
}
```

在 `src/main.rs` 的 `mod service;` 之后加一行:

```rust
mod storage;
```

- [ ] **Step 2: 运行测试确认编译失败**

Run: `cargo test storage 2>&1 | tail -5`
Expected: FAIL,`cannot find type Storage`

- [ ] **Step 3: 实现 —— 在 `src/storage.rs` 顶部加入**

```rust
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{Local, SecondsFormat};
use rusqlite::Connection;

use crate::crypto::Crypto;

/// 站点(完整行)
#[derive(Debug, Clone)]
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

/// 新建/编辑站点的输入(id 与时间戳由 Storage 生成)
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

impl SiteInput {
    /// new-api 系站点的默认路径约定。
    pub fn with_defaults(name: &str, domain: &str) -> Self {
        Self {
            name: name.to_string(),
            domain: domain.to_string(),
            login_path: "/login".to_string(),
            sign_in_path: Some("/api/user/sign_in".to_string()),
            user_info_path: "/api/user/self".to_string(),
            tokens_path: "/api/token/".to_string(),
            logs_path: "/api/log/self".to_string(),
            chart_path: "/api/data/self".to_string(),
            api_user_key: "new-api-user".to_string(),
        }
    }
}

/// 账户(敏感字段已解密)
#[derive(Debug, Clone)]
pub struct Account {
    pub id: i64,
    pub site_id: i64,
    pub name: String,
    pub api_user: String,
    pub username: Option<String>,
    pub password: Option<String>,
    /// 结构化 cookie 数组的 JSON 文本:[{name,value,domain,path,expires,httpOnly,secure}]
    pub cookies_json: Option<String>,
    pub cookie_issued_at: Option<String>,
    pub cookie_expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 新建/编辑账户的输入
#[derive(Debug, Clone, Default)]
pub struct AccountInput {
    pub site_id: i64,
    pub name: String,
    pub api_user: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub cookies_json: Option<String>,
    pub cookie_issued_at: Option<String>,
    pub cookie_expires_at: Option<String>,
}

pub struct Storage {
    conn: Connection,
    crypto: Crypto,
}

/// ISO8601 本地时间(含时区偏移),全库统一的时间格式。
pub fn now_iso() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Secs, false)
}

impl Storage {
    /// 打开默认位置的数据库:%LOCALAPPDATA%/anyrouter-checkin/data.db
    pub fn open_default(crypto: Crypto) -> Result<Self> {
        let dir = dirs::data_local_dir()
            .context("无法定位用户数据目录")?
            .join("anyrouter-checkin");
        std::fs::create_dir_all(&dir).context("无法创建数据目录")?;
        Self::open_at(&dir.join("data.db"), crypto)
    }

    pub fn open_at(path: &Path, crypto: Crypto) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("无法打开数据库 {}", path.display()))?;
        let mut s = Self { conn, crypto };
        s.init()?;
        Ok(s)
    }

    pub fn open_in_memory(crypto: Crypto) -> Result<Self> {
        let conn = Connection::open_in_memory().context("无法创建内存数据库")?;
        let mut s = Self { conn, crypto };
        s.init()?;
        Ok(s)
    }

    fn init(&mut self) -> Result<()> {
        self.conn
            .execute_batch("PRAGMA foreign_keys = ON;")
            .context("无法启用外键约束")?;
        self.migrate()
    }

    /// 建表迁移;按 schema_version 递增,可重复执行。
    pub fn migrate(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;
        let version: i64 = self
            .get_meta("schema_version")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if version < 1 {
            self.conn.execute_batch(
                "BEGIN;
                CREATE TABLE sites (
                    id             INTEGER PRIMARY KEY AUTOINCREMENT,
                    name           TEXT NOT NULL UNIQUE,
                    domain         TEXT NOT NULL,
                    login_path     TEXT NOT NULL DEFAULT '/login',
                    sign_in_path   TEXT,
                    user_info_path TEXT NOT NULL DEFAULT '/api/user/self',
                    tokens_path    TEXT NOT NULL DEFAULT '/api/token/',
                    logs_path      TEXT NOT NULL DEFAULT '/api/log/self',
                    chart_path     TEXT NOT NULL DEFAULT '/api/data/self',
                    api_user_key   TEXT NOT NULL DEFAULT 'new-api-user',
                    created_at     TEXT NOT NULL,
                    updated_at     TEXT NOT NULL
                );
                CREATE TABLE accounts (
                    id                INTEGER PRIMARY KEY AUTOINCREMENT,
                    site_id           INTEGER NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
                    name              TEXT NOT NULL,
                    api_user          TEXT NOT NULL,
                    username_enc      BLOB,
                    password_enc      BLOB,
                    cookies_enc       BLOB,
                    cookie_issued_at  TEXT,
                    cookie_expires_at TEXT,
                    created_at        TEXT NOT NULL,
                    updated_at        TEXT NOT NULL,
                    UNIQUE(site_id, name)
                );
                CREATE TABLE account_cache (
                    account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                    kind        TEXT NOT NULL,
                    payload     TEXT NOT NULL,
                    encrypted   INTEGER NOT NULL DEFAULT 0,
                    fetched_at  TEXT NOT NULL,
                    PRIMARY KEY (account_id, kind)
                );
                INSERT INTO meta(key, value) VALUES('schema_version', '1')
                    ON CONFLICT(key) DO UPDATE SET value = excluded.value;
                COMMIT;",
            )?;
        }
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query([key])?;
        Ok(match rows.next()? {
            Some(row) => Some(row.get(0)?),
            None => None,
        })
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test storage 2>&1 | tail -5`
Expected: `test result: ok. 3 passed`

- [ ] **Step 5: Commit**

```bash
git add src/storage.rs src/main.rs
git commit -m "feat(storage): SQLite 建库迁移与元信息表"
```

---

### Task 4: 站点 CRUD

**Files:**
- Modify: `src/storage.rs`
- Test: `src/storage.rs` 内联

- [ ] **Step 1: 在 `tests` 模块中追加失败测试**

```rust
    #[test]
    fn site_crud_roundtrip() {
        let s = test_storage();
        let id = s
            .insert_site(&SiteInput::with_defaults("AnyRouter", "https://anyrouter.top"))
            .unwrap();
        let site = s.get_site(id).unwrap().expect("应能查到");
        assert_eq!(site.name, "AnyRouter");
        assert_eq!(site.login_path, "/login");
        assert_eq!(site.sign_in_path.as_deref(), Some("/api/user/sign_in"));
        assert_eq!(site.tokens_path, "/api/token/");
        assert!(!site.created_at.is_empty());

        let mut input = SiteInput::with_defaults("AnyRouter2", "https://x.example.com");
        input.sign_in_path = None; // 改为自动签到型
        s.update_site(id, &input).unwrap();
        let site = s.get_site(id).unwrap().unwrap();
        assert_eq!(site.name, "AnyRouter2");
        assert_eq!(site.sign_in_path, None);

        assert_eq!(s.list_sites().unwrap().len(), 1);
        s.delete_site(id).unwrap();
        assert!(s.get_site(id).unwrap().is_none());
        assert!(s.list_sites().unwrap().is_empty());
    }

    #[test]
    fn site_name_must_be_unique() {
        let s = test_storage();
        s.insert_site(&SiteInput::with_defaults("A", "https://a.com")).unwrap();
        assert!(s.insert_site(&SiteInput::with_defaults("A", "https://b.com")).is_err());
    }
```

- [ ] **Step 2: 运行测试确认编译失败**

Run: `cargo test storage 2>&1 | tail -5`
Expected: FAIL,`no method named insert_site`

- [ ] **Step 3: 在 `impl Storage` 中追加实现**

```rust
    pub fn insert_site(&self, input: &SiteInput) -> Result<i64> {
        let now = now_iso();
        self.conn.execute(
            "INSERT INTO sites(name, domain, login_path, sign_in_path, user_info_path,
                               tokens_path, logs_path, chart_path, api_user_key,
                               created_at, updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",
            rusqlite::params![
                input.name, input.domain, input.login_path, input.sign_in_path,
                input.user_info_path, input.tokens_path, input.logs_path,
                input.chart_path, input.api_user_key, now,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_site(&self, id: i64, input: &SiteInput) -> Result<()> {
        self.conn.execute(
            "UPDATE sites SET name=?1, domain=?2, login_path=?3, sign_in_path=?4,
                              user_info_path=?5, tokens_path=?6, logs_path=?7,
                              chart_path=?8, api_user_key=?9, updated_at=?10
             WHERE id=?11",
            rusqlite::params![
                input.name, input.domain, input.login_path, input.sign_in_path,
                input.user_info_path, input.tokens_path, input.logs_path,
                input.chart_path, input.api_user_key, now_iso(), id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_site(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM sites WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn get_site(&self, id: i64) -> Result<Option<Site>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, domain, login_path, sign_in_path, user_info_path,
                    tokens_path, logs_path, chart_path, api_user_key, created_at, updated_at
             FROM sites WHERE id=?1",
        )?;
        let mut rows = stmt.query([id])?;
        Ok(match rows.next()? {
            Some(row) => Some(Self::row_to_site(row)?),
            None => None,
        })
    }

    pub fn find_site_by_name(&self, name: &str) -> Result<Option<Site>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, domain, login_path, sign_in_path, user_info_path,
                    tokens_path, logs_path, chart_path, api_user_key, created_at, updated_at
             FROM sites WHERE name=?1",
        )?;
        let mut rows = stmt.query([name])?;
        Ok(match rows.next()? {
            Some(row) => Some(Self::row_to_site(row)?),
            None => None,
        })
    }

    pub fn list_sites(&self) -> Result<Vec<Site>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, domain, login_path, sign_in_path, user_info_path,
                    tokens_path, logs_path, chart_path, api_user_key, created_at, updated_at
             FROM sites ORDER BY id",
        )?;
        let rows = stmt.query_map([], Self::row_to_site)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 行 → Site 的唯一映射来源(三个查询共用,列顺序以 SELECT 为准)。
    fn row_to_site(row: &rusqlite::Row<'_>) -> rusqlite::Result<Site> {
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
    }
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test storage 2>&1 | tail -5`
Expected: `test result: ok. 5 passed`

- [ ] **Step 5: Commit**

```bash
git add src/storage.rs
git commit -m "feat(storage): 站点 CRUD"
```

---

### Task 5: 账户 CRUD(敏感列加密)

**Files:**
- Modify: `src/storage.rs`
- Test: `src/storage.rs` 内联

- [ ] **Step 1: 在 `tests` 模块中追加失败测试**

```rust
    fn sample_account(site_id: i64, name: &str) -> AccountInput {
        AccountInput {
            site_id,
            name: name.to_string(),
            api_user: "12345".to_string(),
            username: Some("alice".to_string()),
            password: Some("p@ss".to_string()),
            cookies_json: Some(r#"[{"name":"session","value":"abc","domain":"anyrouter.top","path":"/","expires":-1,"httpOnly":false,"secure":true}]"#.to_string()),
            cookie_issued_at: Some(now_iso()),
            cookie_expires_at: None,
        }
    }

    #[test]
    fn account_crud_roundtrip_with_encryption() {
        let s = test_storage();
        let site_id = s.insert_site(&SiteInput::with_defaults("A", "https://a.com")).unwrap();
        let id = s.insert_account(&sample_account(site_id, "用户A")).unwrap();

        let acc = s.get_account(id).unwrap().expect("应能查到");
        assert_eq!(acc.name, "用户A");
        assert_eq!(acc.username.as_deref(), Some("alice"));
        assert_eq!(acc.password.as_deref(), Some("p@ss"));
        assert!(acc.cookies_json.as_deref().unwrap().contains("session"));

        // 落库的必须是密文:原文不应出现在任何 BLOB 中
        let (u, p, c) = s.raw_account_blobs(id);
        for blob in [u, p, c] {
            let blob = blob.expect("加密列应有值");
            assert!(!String::from_utf8_lossy(&blob).contains("alice"));
            assert!(!String::from_utf8_lossy(&blob).contains("p@ss"));
            assert!(!String::from_utf8_lossy(&blob).contains("session"));
        }

        // 更新:清空账密、改名
        let mut input = sample_account(site_id, "用户B");
        input.username = None;
        input.password = None;
        s.update_account(id, &input).unwrap();
        let acc = s.get_account(id).unwrap().unwrap();
        assert_eq!(acc.name, "用户B");
        assert_eq!(acc.username, None);

        // 列表与删除
        assert_eq!(s.list_accounts(site_id).unwrap().len(), 1);
        assert_eq!(s.count_accounts(site_id).unwrap(), 1);
        s.delete_account(id).unwrap();
        assert!(s.get_account(id).unwrap().is_none());
    }

    #[test]
    fn deleting_site_cascades_accounts() {
        let s = test_storage();
        let site_id = s.insert_site(&SiteInput::with_defaults("A", "https://a.com")).unwrap();
        s.insert_account(&sample_account(site_id, "用户A")).unwrap();
        s.delete_site(site_id).unwrap();
        assert!(s.list_all_accounts().unwrap().is_empty());
    }

    #[test]
    fn account_name_unique_within_site() {
        let s = test_storage();
        let site_id = s.insert_site(&SiteInput::with_defaults("A", "https://a.com")).unwrap();
        s.insert_account(&sample_account(site_id, "用户A")).unwrap();
        assert!(s.insert_account(&sample_account(site_id, "用户A")).is_err());
    }
```

- [ ] **Step 2: 运行测试确认编译失败**

Run: `cargo test storage 2>&1 | tail -5`
Expected: FAIL,`no method named insert_account`

- [ ] **Step 3: 在 `impl Storage` 中追加实现**

```rust
    pub fn insert_account(&self, input: &AccountInput) -> Result<i64> {
        let now = now_iso();
        self.conn.execute(
            "INSERT INTO accounts(site_id, name, api_user, username_enc, password_enc,
                                  cookies_enc, cookie_issued_at, cookie_expires_at,
                                  created_at, updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",
            rusqlite::params![
                input.site_id, input.name, input.api_user,
                self.encrypt_opt(input.username.as_deref())?,
                self.encrypt_opt(input.password.as_deref())?,
                self.encrypt_opt(input.cookies_json.as_deref())?,
                input.cookie_issued_at, input.cookie_expires_at, now,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_account(&self, id: i64, input: &AccountInput) -> Result<()> {
        self.conn.execute(
            "UPDATE accounts SET name=?1, api_user=?2, username_enc=?3, password_enc=?4,
                                 cookies_enc=?5, cookie_issued_at=?6, cookie_expires_at=?7,
                                 updated_at=?8
             WHERE id=?9",
            rusqlite::params![
                input.name, input.api_user,
                self.encrypt_opt(input.username.as_deref())?,
                self.encrypt_opt(input.password.as_deref())?,
                self.encrypt_opt(input.cookies_json.as_deref())?,
                input.cookie_issued_at, input.cookie_expires_at, now_iso(), id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_account(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM accounts WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn get_account(&self, id: i64) -> Result<Option<Account>> {
        let accounts = self.query_accounts("WHERE id=?1", rusqlite::params![id])?;
        Ok(accounts.into_iter().next())
    }

    pub fn list_accounts(&self, site_id: i64) -> Result<Vec<Account>> {
        self.query_accounts("WHERE site_id=?1 ORDER BY id", rusqlite::params![site_id])
    }

    pub fn list_all_accounts(&self) -> Result<Vec<Account>> {
        self.query_accounts("ORDER BY site_id, id", rusqlite::params![])
    }

    pub fn count_accounts(&self, site_id: i64) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM accounts WHERE site_id=?1",
            [site_id],
            |r| r.get(0),
        )?)
    }

    fn query_accounts(
        &self,
        suffix: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<Account>> {
        let sql = format!(
            "SELECT id, site_id, name, api_user, username_enc, password_enc, cookies_enc,
                    cookie_issued_at, cookie_expires_at, created_at, updated_at
             FROM accounts {}",
            suffix
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(Account {
                id: row.get(0)?,
                site_id: row.get(1)?,
                name: row.get(2)?,
                api_user: row.get(3)?,
                username: self.decrypt_opt(row.get::<_, Option<Vec<u8>>>(4)?)?,
                password: self.decrypt_opt(row.get::<_, Option<Vec<u8>>>(5)?)?,
                cookies_json: self.decrypt_opt(row.get::<_, Option<Vec<u8>>>(6)?)?,
                cookie_issued_at: row.get(7)?,
                cookie_expires_at: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            });
        }
        Ok(out)
    }

    fn encrypt_opt(&self, value: Option<&str>) -> Result<Option<Vec<u8>>> {
        match value {
            Some(v) if !v.is_empty() => Ok(Some(self.crypto.encrypt(v)?)),
            _ => Ok(None),
        }
    }

    fn decrypt_opt(&self, blob: Option<Vec<u8>>) -> Result<Option<String>> {
        match blob {
            Some(b) => Ok(Some(self.crypto.decrypt(&b)?)),
            None => Ok(None),
        }
    }

    /// 仅测试用:读取账户三个加密列的原始 BLOB。
    #[cfg(test)]
    pub(crate) fn raw_account_blobs(
        &self,
        id: i64,
    ) -> (Option<Vec<u8>>, Option<Vec<u8>>, Option<Vec<u8>>) {
        self.conn
            .query_row(
                "SELECT username_enc, password_enc, cookies_enc FROM accounts WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
    }
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test storage 2>&1 | tail -5`
Expected: `test result: ok. 8 passed`

- [ ] **Step 5: Commit**

```bash
git add src/storage.rs
git commit -m "feat(storage): 账户 CRUD,用户名/密码/Cookie 列加密存储"
```

---

### Task 6: Cookie 格式工具(旧格式 ↔ 结构化数组)

**Files:**
- Modify: `src/storage.rs`
- Test: `src/storage.rs` 内联

旧配置里 cookies 是 `{"k":"v"}` 对象或 `"k=v; k2=v2"` 串,且可能夹带 `_username`/`_password`;库内统一存结构化数组。现有 Playwright 通路(本阶段不改)仍吃 `{"k":"v"}` 对象,需要反向转换。

- [ ] **Step 1: 在 `tests` 模块中追加失败测试**

```rust
    #[test]
    fn cookie_value_to_entries_extracts_credentials() {
        let raw = serde_json::json!({
            "session": "abc", "acw_tc": "x",
            "_username": "alice", "_password": "p@ss"
        });
        let (entries, username, password) =
            cookies_value_to_entries(&raw, "anyrouter.top");
        assert_eq!(username.as_deref(), Some("alice"));
        assert_eq!(password.as_deref(), Some("p@ss"));
        assert_eq!(entries.len(), 2); // _username/_password 不进 cookie
        let session = entries.iter().find(|e| e["name"] == "session").unwrap();
        assert_eq!(session["value"], "abc");
        assert_eq!(session["domain"], "anyrouter.top");
        assert_eq!(session["expires"], -1);
    }

    #[test]
    fn cookie_string_to_entries() {
        let raw = serde_json::json!("session=abc; acw_tc=x");
        let (entries, username, password) = cookies_value_to_entries(&raw, "a.com");
        assert_eq!(username, None);
        assert_eq!(password, None);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn cookie_entries_to_map_roundtrip() {
        let raw = serde_json::json!({"session": "abc", "k": "v"});
        let (entries, _, _) = cookies_value_to_entries(&raw, "a.com");
        let json = serde_json::to_string(&entries).unwrap();
        let map = cookie_entries_to_map(&json);
        assert_eq!(map.get("session").and_then(|v| v.as_str()), Some("abc"));
        assert_eq!(map.get("k").and_then(|v| v.as_str()), Some("v"));
    }

    #[test]
    fn host_of_strips_scheme_and_path() {
        assert_eq!(host_of("https://anyrouter.top"), "anyrouter.top");
        assert_eq!(host_of("https://a.com/path"), "a.com");
        assert_eq!(host_of("http://a.com:8080"), "a.com:8080");
        assert_eq!(host_of("a.com"), "a.com");
    }
```

- [ ] **Step 2: 运行测试确认编译失败**

Run: `cargo test storage 2>&1 | tail -5`
Expected: FAIL,`cannot find function cookies_value_to_entries`

- [ ] **Step 3: 在 `src/storage.rs`(`impl Storage` 之外)追加自由函数**

```rust
/// 从域名/URL 提取 host(含端口),供 cookie 的 domain 字段使用。
pub fn host_of(domain: &str) -> &str {
    let no_scheme = domain
        .strip_prefix("https://")
        .or_else(|| domain.strip_prefix("http://"))
        .unwrap_or(domain);
    no_scheme.split('/').next().unwrap_or(no_scheme)
}

/// 旧格式 cookies(对象或 "k=v; " 串)→ (结构化数组, _username, _password)。
/// 手动录入/导入场景:expires 置 -1(未知),domain 取站点 host。
pub fn cookies_value_to_entries(
    raw: &serde_json::Value,
    domain_host: &str,
) -> (Vec<serde_json::Value>, Option<String>, Option<String>) {
    let mut pairs: Vec<(String, String)> = Vec::new();
    match raw {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                if let Some(s) = v.as_str() {
                    pairs.push((k.clone(), s.to_string()));
                }
            }
        }
        serde_json::Value::String(s) => {
            for part in s.split(';') {
                if let Some((k, v)) = part.split_once('=') {
                    pairs.push((k.trim().to_string(), v.trim().to_string()));
                }
            }
        }
        _ => {}
    }

    let mut username = None;
    let mut password = None;
    let mut entries = Vec::new();
    for (k, v) in pairs {
        match k.as_str() {
            "_username" => username = Some(v),
            "_password" => password = Some(v),
            _ => entries.push(serde_json::json!({
                "name": k, "value": v,
                "domain": domain_host, "path": "/",
                "expires": -1, "httpOnly": false, "secure": true,
            })),
        }
    }
    (entries, username, password)
}

/// 结构化数组 JSON 文本 → {name: value} 对象(现有 Playwright 通路的格式)。
pub fn cookie_entries_to_map(entries_json: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str(entries_json) {
        for e in arr {
            if let (Some(name), Some(value)) =
                (e.get("name").and_then(|v| v.as_str()), e.get("value").and_then(|v| v.as_str()))
            {
                map.insert(name.to_string(), serde_json::Value::String(value.to_string()));
            }
        }
    }
    map
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test storage 2>&1 | tail -5`
Expected: `test result: ok. 12 passed`

- [ ] **Step 5: Commit**

```bash
git add src/storage.rs
git commit -m "feat(storage): Cookie 新旧格式转换工具"
```

---

### Task 7: `.env` 一次性导入

**Files:**
- Modify: `src/storage.rs`
- Test: `src/storage.rs` 内联

复用 `config.rs` 现有的 `ProviderConfig` / `AccountConfig` serde 结构解析旧 JSON。注意 `ProviderConfig.sign_in_path` 的 serde 默认值是 `Some("/api/user/sign_in")`,显式 `null` 才是自动签到。

- [ ] **Step 1: 在 `tests` 模块中追加失败测试**

```rust
    #[test]
    fn import_seeds_builtins_and_parses_env() {
        let s = test_storage();
        let providers = r#"{"custom":{"domain":"https://c.example.com","sign_in_path":null}}"#;
        let accounts = r#"[
            {"cookies":{"session":"abc","_username":"u1","_password":"p1"},
             "api_user":"111","provider":"anyrouter","name":"用户A"},
            {"cookies":"session=s2; k=v","api_user":"222","provider":"custom"},
            {"cookies":{},"api_user":"333","provider":"不存在的站点"}
        ]"#;
        let report = s.import_from_strings(Some(providers), Some(accounts)).unwrap();
        assert_eq!(report.sites_added, 3); // anyrouter + agentrouter + custom
        assert_eq!(report.accounts_added, 2);
        assert_eq!(report.accounts_skipped, 1);

        // 内置站点
        let any = s.find_site_by_name("anyrouter").unwrap().unwrap();
        assert_eq!(any.domain, "https://anyrouter.top");
        assert_eq!(any.sign_in_path.as_deref(), Some("/api/user/sign_in"));
        let agent = s.find_site_by_name("agentrouter").unwrap().unwrap();
        assert_eq!(agent.sign_in_path, None);
        // 自定义站点:显式 null → 自动签到
        let custom = s.find_site_by_name("custom").unwrap().unwrap();
        assert_eq!(custom.sign_in_path, None);

        // 账户:凭据抽取 + cookie 结构化 + 名称缺省
        let a = &s.list_accounts(any.id).unwrap()[0];
        assert_eq!(a.name, "用户A");
        assert_eq!(a.username.as_deref(), Some("u1"));
        assert!(a.cookies_json.as_deref().unwrap().contains("session"));
        assert!(!a.cookies_json.as_deref().unwrap().contains("_username"));
        assert!(a.cookie_issued_at.is_some());
        assert!(a.cookie_expires_at.is_none());
        let b = &s.list_accounts(custom.id).unwrap()[0];
        assert_eq!(b.name, "Account 2"); // 与旧 get_display_name 序号规则一致

        // 幂等:重复导入不新增
        let report2 = s.import_from_strings(Some(providers), Some(accounts)).unwrap();
        assert_eq!(report2.sites_added, 0);
        assert_eq!(report2.accounts_added, 0);
    }

    #[test]
    fn import_tolerates_bad_json() {
        let s = test_storage();
        let report = s.import_from_strings(Some("{bad"), Some("[bad")).unwrap();
        assert_eq!(report.sites_added, 2); // 内置站点仍写入
        assert_eq!(report.accounts_added, 0);
    }
```

- [ ] **Step 2: 运行测试确认编译失败**

Run: `cargo test storage 2>&1 | tail -5`
Expected: FAIL,`no method named import_from_strings`

- [ ] **Step 3: 在 `src/storage.rs` 追加导入实现**

文件顶部补充导入:

```rust
use crate::config::{AccountConfig, ProviderConfig};
use crate::log;
```

模型区追加:

```rust
/// 导入结果摘要(写日志用)
#[derive(Debug, Default, PartialEq)]
pub struct ImportReport {
    pub sites_added: usize,
    pub accounts_added: usize,
    pub accounts_skipped: usize,
}
```

`impl Storage` 内追加:

```rust
    /// 首次启动导入:meta.env_imported 存在则跳过。
    pub fn import_env_if_needed(&self) -> Result<()> {
        if self.get_meta("env_imported")?.is_some() {
            return Ok(());
        }
        let providers = std::env::var("PROVIDERS").ok();
        let accounts = std::env::var("ANYROUTER_ACCOUNTS").ok();
        let report = self.import_from_strings(providers.as_deref(), accounts.as_deref())?;
        log::success(&format!(
            "环境变量导入完成:新增站点 {},账户 {},跳过 {}",
            report.sites_added, report.accounts_added, report.accounts_skipped
        ));
        self.set_meta("env_imported", "1")
    }

    /// 导入内置站点 + PROVIDERS + ANYROUTER_ACCOUNTS;可重复调用(按唯一键去重)。
    pub fn import_from_strings(
        &self,
        providers_json: Option<&str>,
        accounts_json: Option<&str>,
    ) -> Result<ImportReport> {
        let mut report = ImportReport::default();

        // 1) 内置站点
        let mut anyrouter = SiteInput::with_defaults("anyrouter", "https://anyrouter.top");
        anyrouter.sign_in_path = Some("/api/user/sign_in".to_string());
        let mut agentrouter = SiteInput::with_defaults("agentrouter", "https://agentrouter.org");
        agentrouter.sign_in_path = None;
        for input in [anyrouter, agentrouter] {
            if self.find_site_by_name(&input.name)?.is_none() {
                self.insert_site(&input)?;
                report.sites_added += 1;
            }
        }

        // 2) 自定义站点(PROVIDERS)
        if let Some(json) = providers_json {
            match serde_json::from_str::<std::collections::HashMap<String, ProviderConfig>>(json) {
                Ok(customs) => {
                    for (key, p) in customs {
                        if self.find_site_by_name(&key)?.is_some() {
                            continue;
                        }
                        let mut input = SiteInput::with_defaults(&key, &p.domain);
                        input.login_path = p.login_path.clone();
                        input.sign_in_path = p.sign_in_path.clone();
                        input.user_info_path = p.user_info_path.clone();
                        input.api_user_key = p.api_user_key.clone();
                        self.insert_site(&input)?;
                        report.sites_added += 1;
                    }
                }
                Err(e) => log::warn(&format!("PROVIDERS 解析失败,跳过导入: {}", e)),
            }
        }

        // 3) 账户(ANYROUTER_ACCOUNTS)
        if let Some(json) = accounts_json {
            match serde_json::from_str::<Vec<AccountConfig>>(json) {
                Ok(accounts) => {
                    for (i, acc) in accounts.iter().enumerate() {
                        let name = acc.get_display_name(i);
                        let Some(site) = self.find_site_by_name(&acc.provider)? else {
                            log::warn(&format!("账户 {} 引用了不存在的站点 {},跳过", name, acc.provider));
                            report.accounts_skipped += 1;
                            continue;
                        };
                        if self
                            .list_accounts(site.id)?
                            .iter()
                            .any(|a| a.name == name)
                        {
                            continue; // 幂等:同站点同名已存在
                        }
                        let (entries, username, password) =
                            cookies_value_to_entries(&acc.cookies, host_of(&site.domain));
                        let has_cookies = !entries.is_empty();
                        self.insert_account(&AccountInput {
                            site_id: site.id,
                            name,
                            api_user: acc.api_user.clone(),
                            username,
                            password,
                            cookies_json: if has_cookies {
                                Some(serde_json::to_string(&entries)?)
                            } else {
                                None
                            },
                            cookie_issued_at: if has_cookies { Some(now_iso()) } else { None },
                            cookie_expires_at: None,
                        })?;
                        report.accounts_added += 1;
                    }
                }
                Err(e) => log::warn(&format!("ANYROUTER_ACCOUNTS 解析失败,跳过导入: {}", e)),
            }
        }

        Ok(report)
    }
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test storage 2>&1 | tail -5`
Expected: `test result: ok. 14 passed`

- [ ] **Step 5: Commit**

```bash
git add src/storage.rs
git commit -m "feat(storage): 内置站点种子与 .env 配置一次性导入"
```

---

### Task 8: 应用接线 —— 数据源切换到 SQLite

**Files:**
- Modify: `src/storage.rs`(UI 适配函数)
- Modify: `src/main.rs:23-33`(启动初始化)
- Modify: `src/app_state.rs:59-108`(新字段)
- Modify: `src/service.rs:24-35`(providers 来源)
- Modify: `src/config.rs`(删除环境变量加载函数)
- Test: `src/storage.rs` 内联 + 手动验收

- [ ] **Step 1: 在 `tests` 模块中追加适配函数的失败测试**

```rust
    #[test]
    fn ui_adapters_produce_legacy_shapes() {
        let s = test_storage();
        let providers = r#"{}"#;
        let accounts = r#"[{"cookies":{"session":"abc","_username":"u1","_password":"p1"},
             "api_user":"111","provider":"anyrouter","name":"用户A"}]"#;
        s.import_from_strings(Some(providers), Some(accounts)).unwrap();

        let provider_map = s.load_providers_for_ui().unwrap();
        assert!(provider_map.contains_key("anyrouter"));
        assert_eq!(provider_map["anyrouter"].domain, "https://anyrouter.top");

        let ui_accounts = s.load_accounts_for_ui().unwrap();
        assert_eq!(ui_accounts.len(), 1);
        assert_eq!(ui_accounts[0].provider, "anyrouter");
        assert_eq!(ui_accounts[0].name.as_deref(), Some("用户A"));
        // cookies 还原为 {k:v} 对象,且重新注入 _username/_password 以保留账密登录能力
        let map = ui_accounts[0].cookies.as_object().unwrap();
        assert_eq!(map.get("session").and_then(|v| v.as_str()), Some("abc"));
        assert_eq!(map.get("_username").and_then(|v| v.as_str()), Some("u1"));
        assert_eq!(map.get("_password").and_then(|v| v.as_str()), Some("p1"));
    }
```

- [ ] **Step 2: 运行测试确认编译失败**

Run: `cargo test storage 2>&1 | tail -5`
Expected: FAIL,`no method named load_providers_for_ui`

- [ ] **Step 3: 在 `impl Storage` 中追加适配函数**

```rust
    /// 适配现有 UI/签到通路:站点 → ProviderConfig 映射(键为站点名)。
    pub fn load_providers_for_ui(
        &self,
    ) -> Result<std::collections::HashMap<String, ProviderConfig>> {
        let mut map = std::collections::HashMap::new();
        for site in self.list_sites()? {
            map.insert(
                site.name.clone(),
                ProviderConfig {
                    name: site.name,
                    domain: site.domain,
                    login_path: site.login_path,
                    sign_in_path: site.sign_in_path,
                    user_info_path: site.user_info_path,
                    api_user_key: site.api_user_key,
                },
            );
        }
        Ok(map)
    }

    /// 适配现有 UI/签到通路:账户 → AccountConfig 列表。
    /// cookies 还原为 {k:v} 对象;账密以 _username/_password 注入(沿用旧偷渡约定)。
    pub fn load_accounts_for_ui(&self) -> Result<Vec<AccountConfig>> {
        let sites: std::collections::HashMap<i64, String> = self
            .list_sites()?
            .into_iter()
            .map(|s| (s.id, s.name))
            .collect();
        let mut out = Vec::new();
        for acc in self.list_all_accounts()? {
            let mut map = acc
                .cookies_json
                .as_deref()
                .map(cookie_entries_to_map)
                .unwrap_or_default();
            if let Some(u) = &acc.username {
                map.insert("_username".into(), serde_json::Value::String(u.clone()));
            }
            if let Some(p) = &acc.password {
                map.insert("_password".into(), serde_json::Value::String(p.clone()));
            }
            let Some(provider) = sites.get(&acc.site_id) else { continue };
            out.push(AccountConfig {
                cookies: serde_json::Value::Object(map),
                api_user: acc.api_user,
                provider: provider.clone(),
                name: Some(acc.name),
            });
        }
        Ok(out)
    }
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test storage 2>&1 | tail -5`
Expected: `test result: ok. 15 passed`

- [ ] **Step 5: 改 `src/app_state.rs` —— AppState 持有 providers 与数据库句柄**

文件顶部 `use` 区追加:

```rust
use std::sync::{Arc, Mutex};

use crate::config::ProviderConfig;
use crate::storage::Storage;
```

`AppState` 结构体的 `accounts` 字段之后追加两个字段:

```rust
    /// 站点配置(来自 SQLite,键为站点名)
    pub providers: HashMap<String, ProviderConfig>,
    /// 数据库句柄(后续阶段界面 CRUD 使用)
    pub db: Arc<Mutex<Storage>>,
```

`AppState::new` 签名与构造改为:

```rust
    pub fn new(
        accounts: Vec<AccountConfig>,
        providers: HashMap<String, ProviderConfig>,
        db: Arc<Mutex<Storage>>,
    ) -> Self {
        let mut checkin_status = HashMap::new();
        for (i, account) in accounts.iter().enumerate() {
            let name = account.get_display_name(i);
            checkin_status.insert(name, CheckInStatus::Idle);
        }

        Self {
            accounts,
            providers,
            db,
            checkin_status,
            results: HashMap::new(),
            logs: Vec::new(),
            balances: HashMap::new(),
            is_running: false,
            success_count: 0,
            fail_count: 0,
            active_panel: ActivePanel::Accounts,
        }
    }
```

- [ ] **Step 6: 改 `src/main.rs` —— 启动初始化 SQLite**

把 `main` 函数开头(`dotenvy` 之后、`Application::new` 之前)替换为:

```rust
fn main() {
    // 加载 .env(仅运维变量:PYTHON_BIN / PLAYWRIGHT_SCRIPT / PLAYWRIGHT_HEADLESS 等)
    dotenvy::dotenv().ok();

    // 初始化加密与数据库;失败则记日志退出(错误弹窗在阶段 2 UI 重构时补充)
    let crypto = match crypto::Crypto::from_keyring() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[启动失败] 系统凭据库不可用: {e:#}");
            std::process::exit(1);
        }
    };
    let storage = match storage::Storage::open_default(crypto) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[启动失败] 数据库初始化失败: {e:#}");
            std::process::exit(1);
        }
    };
    if let Err(e) = storage.import_env_if_needed() {
        eprintln!("[警告] 旧配置导入失败,以现有数据继续: {e:#}");
    }
    let accounts = storage.load_accounts_for_ui().unwrap_or_default();
    let providers = storage.load_providers_for_ui().unwrap_or_default();
    let db = std::sync::Arc::new(std::sync::Mutex::new(storage));
```

`mod` 声明区确认包含(Task 2/3 已加):`mod crypto;` `mod storage;`,并把 `use config::load_accounts_config;` 一行删除。

`app.run` 内创建状态的一行改为:

```rust
        let app_state = cx.new(|_cx| AppState::new(accounts, providers, db));
```

(闭包捕获:`app.run(move |cx| { ... })` 已是 move,无需额外处理。)

- [ ] **Step 7: 改 `src/service.rs` —— providers 改读状态**

把 `run_checkin_all` 中加载配置的代码块:

```rust
        // 加载配置
        let (accounts, providers) = {
            let state = app_state.read_with(cx, |state, _cx| state.accounts.clone());
            match state {
                Ok(accounts) => {
                    let config = AppConfig::load_from_env();
                    (accounts, config.providers)
                }
                Err(_) => return,
            }
        };
```

替换为:

```rust
        // 加载配置(站点与账户均来自 SQLite,经 AppState 缓存)
        let Ok((accounts, providers)) = app_state.read_with(cx, |state, _cx| {
            (state.accounts.clone(), state.providers.clone())
        }) else {
            return;
        };
```

并删除文件顶部的 `use crate::config::AppConfig;`。

- [ ] **Step 8: 改 `src/config.rs` —— 删除环境变量运行时加载**

删除 `AppConfig` 结构体、`impl AppConfig` 整块,以及 `load_accounts_config` 函数;随之删除文件顶部失效的 `use std::collections::HashMap;` 与 `use crate::log;`。保留 `ProviderConfig`、`AccountConfig`、各 `default_*` 函数与 `get_display_name`(导入与适配仍在用)。文件头注释改为:

```rust
//! 旧配置结构定义:仅用于 .env 一次性导入(storage::import_from_strings)
//! 与现有签到通路的适配(storage::load_*_for_ui)。运行时配置已迁移至 SQLite。
```

- [ ] **Step 9: 全量编译与测试**

Run: `cargo test 2>&1 | tail -8`
Expected: 全部通过(crypto 4 + storage 15 + checkin 2 = `21 passed` 上下),无编译错误

Run: `cargo check 2>&1 | tail -3`
Expected: `Finished`,无 warning 新增(若有 unused 警告按提示清理)

- [ ] **Step 10: 手动验收(冒烟)**

Run: `cargo run`(保持 `.env` 中已有 `ANYROUTER_ACCOUNTS`)
Expected:
1. 窗口正常打开,账户面板显示与之前相同的账户列表(数据已来自 SQLite);
2. 终端日志出现「环境变量导入完成:新增站点 …,账户 …」(仅首次);
3. 关闭重开,账户仍在(持久化生效),且不再出现导入日志(幂等);
4. 用 `%LOCALAPPDATA%\anyrouter-checkin\data.db` 确认文件存在;
5. 点击「Check-in All」签到流程与改造前行为一致。

- [ ] **Step 11: Commit**

```bash
git add src/main.rs src/app_state.rs src/service.rs src/config.rs src/storage.rs
git commit -m "feat: 应用数据源从环境变量切换为 SQLite(阶段1完成)"
```

---

## 验收清单(阶段 1 完成定义)

- [ ] `cargo test` 全绿(crypto 4 项 + storage 15 项 + 既有 checkin 2 项)
- [ ] 首次启动自动导入 `.env` 旧配置,重复启动不重复导入
- [ ] 数据库文件位于 `%LOCALAPPDATA%\anyrouter-checkin\data.db`,accounts 表三个 `*_enc` 列为密文
- [ ] Windows 凭据管理器出现 `anyrouter-checkin / master-key` 条目
- [ ] 界面与签到行为与改造前完全一致(本阶段不改 UI)
