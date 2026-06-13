//! SQLite 持久化层:站点 / 账户 / 详情缓存 / 元信息。
//! 同步 rusqlite;上层在后台线程/spawn_blocking 中调用,避免阻塞 UI。

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{Local, SecondsFormat};
use rusqlite::{Connection, OptionalExtension};

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
    ///
    /// Rust 侧 `with_defaults` 是权威默认值来源;建表 DDL 中的 DEFAULT 仅作裸 SQL 写入时的兜底,
    /// 两处需保持一致。
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
///
/// 比较约束:时间比较必须先 parse 成 `DateTime` 再比较,不得对字符串做裸比较
/// (混合时区偏移时字典序不可靠);SQL 端比较需用 `datetime(col)` 归一化。
#[allow(dead_code)] // Task 4 起写库时间戳使用
pub fn now_iso() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Secs, false)
}

/// sites 表查询列清单(各查询共用,列顺序必须与 `row_to_site` 的取列下标一致)。
const SITE_COLS: &str = "id, name, domain, login_path, sign_in_path, user_info_path, \
                         tokens_path, logs_path, chart_path, api_user_key, created_at, updated_at";

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
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
            )
            .context("初始化 meta 表失败")?;
        let version: i64 = self
            .get_meta("schema_version")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if version < 1 {
            // 注意:版本号必须与建表 DDL 同事务写入,避免「表已建但版本号未落盘」的不可自愈状态;
            // 业务表刻意不用 IF NOT EXISTS,严格模式下能及时暴露状态机错误。
            self.conn
                .execute_batch(
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
                    -- encrypted=1 时 payload 为 base64(nonce‖ciphertext)
                    encrypted   INTEGER NOT NULL DEFAULT 0,
                    fetched_at  TEXT NOT NULL,
                    PRIMARY KEY (account_id, kind)
                );
                INSERT INTO meta(key, value) VALUES('schema_version', '1')
                    ON CONFLICT(key) DO UPDATE SET value = excluded.value;
                COMMIT;",
                )
                .context("建表迁移(v1)失败")?;
        }
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM meta WHERE key = ?1")
            .context("读取元信息失败:预编译查询")?;
        let mut rows = stmt.query([key]).context("读取元信息失败:执行查询")?;
        Ok(match rows.next().context("读取元信息失败:遍历结果")? {
            Some(row) => Some(row.get(0)?),
            None => None,
        })
    }

    #[allow(dead_code)] // Task 7 写入 env_imported 等标记使用;迁移版本号已改为同事务 SQL 写入
    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO meta(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [key, value],
            )
            .context("写入元信息失败")?;
        Ok(())
    }

    #[allow(dead_code)] // Task 7/8 接入后使用
    pub fn insert_site(&self, input: &SiteInput) -> Result<i64> {
        let now = now_iso();
        self.conn
            .execute(
                "INSERT INTO sites(name, domain, login_path, sign_in_path, user_info_path,
                               tokens_path, logs_path, chart_path, api_user_key,
                               created_at, updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",
                rusqlite::params![
                    input.name, input.domain, input.login_path, input.sign_in_path,
                    input.user_info_path, input.tokens_path, input.logs_path,
                    input.chart_path, input.api_user_key, now,
                ],
            )
            .with_context(|| format!("新增站点 {} 失败", input.name))?;
        Ok(self.conn.last_insert_rowid())
    }

    #[allow(dead_code)] // Task 8 应用接线接入后使用
    pub fn update_site(&self, id: i64, input: &SiteInput) -> Result<()> {
        let n = self
            .conn
            .execute(
                "UPDATE sites SET name=?1, domain=?2, login_path=?3, sign_in_path=?4,
                              user_info_path=?5, tokens_path=?6, logs_path=?7,
                              chart_path=?8, api_user_key=?9, updated_at=?10
             WHERE id=?11",
                rusqlite::params![
                    input.name, input.domain, input.login_path, input.sign_in_path,
                    input.user_info_path, input.tokens_path, input.logs_path,
                    input.chart_path, input.api_user_key, now_iso(), id,
                ],
            )
            .with_context(|| format!("更新站点失败 id={id}"))?;
        anyhow::ensure!(n == 1, "更新站点失败:id={id} 不存在");
        Ok(())
    }

    #[allow(dead_code)] // Task 8 应用接线接入后使用
    pub fn delete_site(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM sites WHERE id=?1", [id])
            .context("删除站点失败")?;
        Ok(())
    }

    #[allow(dead_code)] // Task 8 应用接线接入后使用
    pub fn get_site(&self, id: i64) -> Result<Option<Site>> {
        self.conn
            .query_row(
                &format!("SELECT {SITE_COLS} FROM sites WHERE id=?1"),
                [id],
                Self::row_to_site,
            )
            .optional()
            .context("查询站点失败")
    }

    #[allow(dead_code)] // Task 7 .env 导入接入后使用
    pub fn find_site_by_name(&self, name: &str) -> Result<Option<Site>> {
        self.conn
            .query_row(
                &format!("SELECT {SITE_COLS} FROM sites WHERE name=?1"),
                [name],
                Self::row_to_site,
            )
            .optional()
            .context("按名称查询站点失败")
    }

    #[allow(dead_code)] // Task 8 应用接线接入后使用
    pub fn list_sites(&self) -> Result<Vec<Site>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {SITE_COLS} FROM sites ORDER BY id"))
            .context("查询站点列表失败")?;
        let sites = stmt
            .query_map([], Self::row_to_site)
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .context("查询站点列表失败")?;
        Ok(sites)
    }

    /// 行 → Site 的唯一映射来源(三个查询共用,列顺序以 `SITE_COLS` 为准)。
    #[allow(dead_code)] // Task 8 应用接线接入后使用
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

    #[test]
    fn fk_enforced_and_cascade_works() {
        let s = test_storage();
        let now = now_iso();
        // 省略全部路径列,验证 DDL DEFAULT 兜底生效
        s.conn
            .execute(
                "INSERT INTO sites(name, domain, created_at, updated_at) VALUES('s1', 'https://a.com', ?1, ?1)",
                [&now],
            )
            .unwrap();
        let login_path: String = s
            .conn
            .query_row("SELECT login_path FROM sites WHERE name = 's1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(login_path, "/login");

        let site_id: i64 = s
            .conn
            .query_row("SELECT id FROM sites WHERE name = 's1'", [], |r| r.get(0))
            .unwrap();
        s.conn
            .execute(
                "INSERT INTO accounts(site_id, name, api_user, created_at, updated_at) VALUES(?1, 'a1', 'u1', ?2, ?2)",
                rusqlite::params![site_id, now],
            )
            .unwrap();
        let account_id: i64 = s
            .conn
            .query_row("SELECT id FROM accounts WHERE name = 'a1'", [], |r| r.get(0))
            .unwrap();
        s.conn
            .execute(
                "INSERT INTO account_cache(account_id, kind, payload, fetched_at) VALUES(?1, 'overview', '{}', ?2)",
                rusqlite::params![account_id, now],
            )
            .unwrap();

        // 悬空外键应被拒绝
        assert!(s
            .conn
            .execute(
                "INSERT INTO accounts(site_id, name, api_user, created_at, updated_at) VALUES(9999, 'bad', 'u', ?1, ?1)",
                [&now],
            )
            .is_err());

        // 删除站点应级联清空两张子表
        s.conn.execute("DELETE FROM sites", []).unwrap();
        for table in ["accounts", "account_cache"] {
            let n: i64 = s
                .conn
                .query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{} 级联删除后应为空", table);
        }
    }

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

        // 按现名应查回同一行;不存在的名字应返回 None
        let by_name = s.find_site_by_name("AnyRouter2").unwrap().expect("按名查询应命中");
        assert_eq!(by_name.id, id);
        assert!(s.find_site_by_name("NoSuchSite").unwrap().is_none());

        assert_eq!(s.list_sites().unwrap().len(), 1);
        s.delete_site(id).unwrap();
        assert!(s.get_site(id).unwrap().is_none());
        assert!(s.list_sites().unwrap().is_empty());
    }

    #[test]
    fn update_missing_site_fails() {
        let s = test_storage();
        let input = SiteInput::with_defaults("Ghost", "https://ghost.example.com");
        let err = s.update_site(9999, &input).unwrap_err();
        assert!(err.to_string().contains("9999"), "错误信息应指明缺失的 id:{err}");
    }

    #[test]
    fn site_name_must_be_unique() {
        let s = test_storage();
        s.insert_site(&SiteInput::with_defaults("A", "https://a.com")).unwrap();
        assert!(s.insert_site(&SiteInput::with_defaults("A", "https://b.com")).is_err());
    }
}
