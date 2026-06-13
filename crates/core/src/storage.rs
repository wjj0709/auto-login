use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};

use crate::models::{Account, AccountCache, AccountInput, CacheKind, Site, SiteInput};

const SCHEMA_VERSION: i64 = 1;

pub struct Storage {
    conn: Connection,
}

impl Storage {
    /// 打开数据库，启用 WAL 和 foreign_keys，执行迁移
    pub fn open(path: &PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create data directory: {:?}", parent))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database at {:?}", path))?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

        let mut storage = Self { conn };
        storage.migrate()?;
        Ok(storage)
    }

    /// 返回用户数据目录路径（Windows: %APPDATA%/anyrouter-checkin/data.db）
    pub fn default_path() -> PathBuf {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("anyrouter-checkin").join("data.db")
    }

    /// 创建四张表（sites, accounts, account_cache, meta），设置 schema_version
    fn migrate(&mut self) -> Result<()> {
        let current_version = self.get_schema_version();

        if current_version < SCHEMA_VERSION {
            self.conn.execute_batch(
                "
                CREATE TABLE IF NOT EXISTS meta (
                    key   TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS sites (
                    id             INTEGER PRIMARY KEY AUTOINCREMENT,
                    name           TEXT NOT NULL,
                    domain         TEXT NOT NULL,
                    login_path     TEXT NOT NULL DEFAULT '/auth/login',
                    sign_in_path   TEXT,
                    user_info_path TEXT NOT NULL DEFAULT '/api/user/getSubInfo',
                    tokens_path    TEXT NOT NULL DEFAULT '/api/user/getSubTokens',
                    logs_path      TEXT NOT NULL DEFAULT '/api/user/getSubLogs',
                    chart_path     TEXT NOT NULL DEFAULT '/api/user/getSubChart',
                    api_user_key   TEXT NOT NULL DEFAULT 'user',
                    created_at     TEXT NOT NULL,
                    updated_at     TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS accounts (
                    id               INTEGER PRIMARY KEY AUTOINCREMENT,
                    site_id          INTEGER NOT NULL REFERENCES sites(id) ON DELETE CASCADE,
                    name             TEXT NOT NULL,
                    api_user         TEXT NOT NULL,
                    username_enc     BLOB,
                    password_enc     BLOB,
                    cookies_enc      BLOB,
                    cookie_issued_at TEXT,
                    cookie_expires_at TEXT,
                    created_at       TEXT NOT NULL,
                    updated_at       TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS account_cache (
                    account_id   INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                    kind         TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    fetched_at   TEXT NOT NULL,
                    PRIMARY KEY (account_id, kind)
                );
                ",
            )?;

            self.set_meta("schema_version", &SCHEMA_VERSION.to_string())?;
        }

        Ok(())
    }

    fn get_schema_version(&self) -> i64 {
        // 尝试查询 meta 表中的 schema_version；表不存在或无记录时返回 0
        self.conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0)
    }

    // ========== meta 表 CRUD ==========

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let result = self.conn.query_row(
            "SELECT value FROM meta WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ========== sites 表 CRUD ==========

    pub fn list_sites(&self) -> Result<Vec<Site>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, domain, login_path, sign_in_path, user_info_path,
                    tokens_path, logs_path, chart_path, api_user_key, created_at, updated_at
             FROM sites ORDER BY id",
        )?;

        let rows = stmt.query_map([], |row| {
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
        })?;

        let mut sites = Vec::new();
        for row in rows {
            sites.push(row?);
        }
        Ok(sites)
    }

    pub fn get_site(&self, id: i64) -> Result<Option<Site>> {
        let result = self.conn.query_row(
            "SELECT id, name, domain, login_path, sign_in_path, user_info_path,
                    tokens_path, logs_path, chart_path, api_user_key, created_at, updated_at
             FROM sites WHERE id = ?1",
            params![id],
            |row| {
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
            },
        );
        match result {
            Ok(site) => Ok(Some(site)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn insert_site(&self, input: &SiteInput) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO sites (name, domain, login_path, sign_in_path, user_info_path,
                               tokens_path, logs_path, chart_path, api_user_key, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                input.name,
                input.domain,
                input.login_path,
                input.sign_in_path,
                input.user_info_path,
                input.tokens_path,
                input.logs_path,
                input.chart_path,
                input.api_user_key,
                now,
                now,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_site(&self, id: i64, input: &SiteInput) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE sites SET name = ?1, domain = ?2, login_path = ?3, sign_in_path = ?4,
                             user_info_path = ?5, tokens_path = ?6, logs_path = ?7,
                             chart_path = ?8, api_user_key = ?9, updated_at = ?10
             WHERE id = ?11",
            params![
                input.name,
                input.domain,
                input.login_path,
                input.sign_in_path,
                input.user_info_path,
                input.tokens_path,
                input.logs_path,
                input.chart_path,
                input.api_user_key,
                now,
                id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_site(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM sites WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ========== accounts 表 CRUD ==========

    pub fn list_accounts_by_site(&self, site_id: i64) -> Result<Vec<Account>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, site_id, name, api_user, username_enc, password_enc, cookies_enc,
                    cookie_issued_at, cookie_expires_at, created_at, updated_at
             FROM accounts WHERE site_id = ?1 ORDER BY id",
        )?;

        let rows = stmt.query_map(params![site_id], |row| {
            Ok(Account {
                id: row.get(0)?,
                site_id: row.get(1)?,
                name: row.get(2)?,
                api_user: row.get(3)?,
                username: row.get::<_, Option<Vec<u8>>>(4)?
                    .map(|b| String::from_utf8_lossy(&b).into_owned()),
                password: row.get::<_, Option<Vec<u8>>>(5)?
                    .map(|b| String::from_utf8_lossy(&b).into_owned()),
                cookies: row.get::<_, Option<Vec<u8>>>(6)?
                    .map(|b| String::from_utf8_lossy(&b).into_owned()),
                cookie_issued_at: row.get(7)?,
                cookie_expires_at: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?;

        let mut accounts = Vec::new();
        for row in rows {
            accounts.push(row?);
        }
        Ok(accounts)
    }

    pub fn list_all_accounts(&self) -> Result<Vec<Account>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, site_id, name, api_user, username_enc, password_enc, cookies_enc,
                    cookie_issued_at, cookie_expires_at, created_at, updated_at
             FROM accounts ORDER BY id",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(Account {
                id: row.get(0)?,
                site_id: row.get(1)?,
                name: row.get(2)?,
                api_user: row.get(3)?,
                username: row.get::<_, Option<Vec<u8>>>(4)?
                    .map(|b| String::from_utf8_lossy(&b).into_owned()),
                password: row.get::<_, Option<Vec<u8>>>(5)?
                    .map(|b| String::from_utf8_lossy(&b).into_owned()),
                cookies: row.get::<_, Option<Vec<u8>>>(6)?
                    .map(|b| String::from_utf8_lossy(&b).into_owned()),
                cookie_issued_at: row.get(7)?,
                cookie_expires_at: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?;

        let mut accounts = Vec::new();
        for row in rows {
            accounts.push(row?);
        }
        Ok(accounts)
    }

    pub fn get_account(&self, id: i64) -> Result<Option<Account>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, site_id, name, api_user, username_enc, password_enc, cookies_enc,
                    cookie_issued_at, cookie_expires_at, created_at, updated_at
             FROM accounts WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(Account {
                id: row.get(0)?,
                site_id: row.get(1)?,
                name: row.get(2)?,
                api_user: row.get(3)?,
                username: row.get::<_, Option<Vec<u8>>>(4)?
                    .map(|b| String::from_utf8_lossy(&b).into_owned()),
                password: row.get::<_, Option<Vec<u8>>>(5)?
                    .map(|b| String::from_utf8_lossy(&b).into_owned()),
                cookies: row.get::<_, Option<Vec<u8>>>(6)?
                    .map(|b| String::from_utf8_lossy(&b).into_owned()),
                cookie_issued_at: row.get(7)?,
                cookie_expires_at: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn insert_account(&self, input: &AccountInput) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        let username_enc = input.username.as_ref().map(|s| s.as_bytes().to_vec());
        let password_enc = input.password.as_ref().map(|s| s.as_bytes().to_vec());
        let cookies_enc = input.cookies.as_ref().map(|s| s.as_bytes().to_vec());

        self.conn.execute(
            "INSERT INTO accounts (site_id, name, api_user, username_enc, password_enc, cookies_enc,
                                   created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                input.site_id,
                input.name,
                input.api_user,
                username_enc,
                password_enc,
                cookies_enc,
                now,
                now,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_account(&self, id: i64, input: &AccountInput) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let username_enc = input.username.as_ref().map(|s| s.as_bytes().to_vec());
        let password_enc = input.password.as_ref().map(|s| s.as_bytes().to_vec());
        let cookies_enc = input.cookies.as_ref().map(|s| s.as_bytes().to_vec());

        self.conn.execute(
            "UPDATE accounts SET site_id = ?1, name = ?2, api_user = ?3,
                                 username_enc = ?4, password_enc = ?5, cookies_enc = ?6,
                                 updated_at = ?7
             WHERE id = ?8",
            params![
                input.site_id,
                input.name,
                input.api_user,
                username_enc,
                password_enc,
                cookies_enc,
                now,
                id,
            ],
        )?;
        Ok(())
    }

    pub fn update_account_cookies(
        &self,
        id: i64,
        cookies: &str,
        issued_at: &str,
        expires_at: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let cookies_enc = cookies.as_bytes().to_vec();

        self.conn.execute(
            "UPDATE accounts SET cookies_enc = ?1, cookie_issued_at = ?2,
                                 cookie_expires_at = ?3, updated_at = ?4
             WHERE id = ?5",
            params![cookies_enc, issued_at, expires_at, now, id],
        )?;
        Ok(())
    }

    pub fn delete_account(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ========== account_cache 表 ==========

    pub fn get_cache(&self, account_id: i64, kind: CacheKind) -> Result<Option<AccountCache>> {
        let result = self.conn.query_row(
            "SELECT account_id, kind, payload_json, fetched_at
             FROM account_cache WHERE account_id = ?1 AND kind = ?2",
            params![account_id, kind.as_str()],
            |row| {
                let kind_str: String = row.get(1)?;
                Ok(AccountCache {
                    account_id: row.get(0)?,
                    kind: parse_cache_kind(&kind_str),
                    payload_json: row.get(2)?,
                    fetched_at: row.get(3)?,
                })
            },
        );
        match result {
            Ok(cache) => Ok(Some(cache)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn upsert_cache(&self, account_id: i64, kind: CacheKind, payload: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO account_cache (account_id, kind, payload_json, fetched_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(account_id, kind) DO UPDATE SET payload_json = excluded.payload_json,
                                                         fetched_at = excluded.fetched_at",
            params![account_id, kind.as_str(), payload, now],
        )?;
        Ok(())
    }
}

fn parse_cache_kind(s: &str) -> CacheKind {
    match s {
        "overview" => CacheKind::Overview,
        "tokens" => CacheKind::Tokens,
        "logs" => CacheKind::Logs,
        "chart" => CacheKind::Chart,
        _ => CacheKind::Overview,
    }
}
