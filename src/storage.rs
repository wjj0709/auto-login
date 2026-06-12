//! SQLite 持久化层:站点 / 账户 / 详情缓存 / 元信息。
//! 同步 rusqlite;上层在后台线程/spawn_blocking 中调用,避免阻塞 UI。

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{Local, SecondsFormat};
use rusqlite::Connection;

use crate::crypto::Crypto;

/// 站点(完整行)
#[allow(dead_code)] // Task 4 站点 CRUD 接入后使用
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
#[allow(dead_code)] // Task 4 站点 CRUD 接入后使用
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
    #[allow(dead_code)] // Task 4/7 接入后使用
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
#[allow(dead_code)] // Task 5 账户 CRUD 接入后使用
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
#[allow(dead_code)] // Task 5 账户 CRUD 接入后使用
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
    #[allow(dead_code)] // Task 5 账户敏感字段加解密接入后使用
    crypto: Crypto,
}

/// ISO8601 本地时间(含时区偏移),全库统一的时间格式。
#[allow(dead_code)] // Task 4 起写库时间戳使用
pub fn now_iso() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Secs, false)
}

impl Storage {
    /// 打开默认位置的数据库:%LOCALAPPDATA%/anyrouter-checkin/data.db
    #[allow(dead_code)] // Task 8 应用接线接入后使用(生产入口)
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

    #[allow(dead_code)] // 测试入口;cfg(test) 内的调用不计入活性分析
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
                COMMIT;",
            )?;
            self.set_meta("schema_version", "1")?;
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
        // 四张表都应存在(查询不报错);业务表为空,meta 仅含 schema_version 一行
        for (table, expected) in [("sites", 0), ("accounts", 0), ("account_cache", 0), ("meta", 1)]
        {
            let n: i64 = s
                .conn
                .query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, expected, "{} 行数不符", table);
        }
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
