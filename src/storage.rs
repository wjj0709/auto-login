//! SQLite 持久化层:站点 / 账户 / 详情缓存 / 元信息。
//! 同步 rusqlite;上层在后台线程/spawn_blocking 中调用,避免阻塞 UI。

use std::path::Path;

use anyhow::{Context, Result};
use base64::Engine;
use chrono::{Local, SecondsFormat};
use rusqlite::{Connection, OptionalExtension};

use crate::config::{AccountConfig, ProviderConfig};
use crate::crypto::Crypto;
use crate::log;

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

/// 导入结果摘要(写日志用)
#[allow(dead_code)] // Task 8 应用接线接入后使用
#[derive(Debug, Default, PartialEq)]
pub struct ImportReport {
    pub sites_added: usize,
    pub accounts_added: usize,
    pub accounts_skipped: usize,
    /// JSON 解析失败的来源数(PROVIDERS / ANYROUTER_ACCOUNTS 各计 1);>0 时不写 env_imported
    pub parse_errors: usize,
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

/// meta 表主密钥金丝雀的键名:开库时校验系统凭据库中的主密钥是否与建库时一致,
/// 防止 keyring 密钥更换/丢失后静默生成新密钥、旧密文全部不可解。
const META_KEY_CANARY: &str = "key_canary";
/// 金丝雀明文;其密文以 base64 存于 meta,能解出此值即证明主密钥未变。
const CANARY_PLAINTEXT: &str = "anyrouter-checkin-canary";

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
        self.migrate()?;
        self.verify_master_key()
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

    /// 主密钥金丝雀校验:首次建库时写入一段已知明文的密文,此后每次开库验证仍可解出;
    /// 主密钥更换/丢失时开库即快速失败,避免与首次运行不可区分地静默用新密钥。
    fn verify_master_key(&self) -> Result<()> {
        use base64::engine::general_purpose::STANDARD;
        match self.get_meta(META_KEY_CANARY)? {
            None => {
                // 首次建库:写入金丝雀密文
                let blob = self.crypto.encrypt(CANARY_PLAINTEXT)?;
                self.set_meta(META_KEY_CANARY, &STANDARD.encode(blob))
            }
            Some(b64) => {
                // base64 格式异常同样按失配处理
                let decrypted = STANDARD
                    .decode(b64.trim())
                    .ok()
                    .and_then(|blob| self.crypto.decrypt(&blob).ok());
                if decrypted.as_deref() != Some(CANARY_PLAINTEXT) {
                    anyhow::bail!(
                        "主密钥校验失败:系统凭据库中的主密钥与数据库不匹配(密钥可能已更换或丢失),\
                         历史加密数据无法解密。如确认放弃旧数据,可删除数据目录中的 data.db 后重启。"
                    );
                }
                Ok(())
            }
        }
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

    #[allow(dead_code)] // Task 7/8 接入后使用
    pub fn insert_account(&self, input: &AccountInput) -> Result<i64> {
        let now = now_iso();
        self.conn
            .execute(
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
            )
            .with_context(|| format!("新增账户 {} 失败", input.name))?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 整行覆盖,含 site_id(跨站点移动账户);UNIQUE(site_id,name) 与外键约束天然兜底。
    #[allow(dead_code)] // Task 8 应用接线接入后使用
    pub fn update_account(&self, id: i64, input: &AccountInput) -> Result<()> {
        let n = self
            .conn
            .execute(
                "UPDATE accounts SET site_id=?1, name=?2, api_user=?3, username_enc=?4,
                                 password_enc=?5, cookies_enc=?6, cookie_issued_at=?7,
                                 cookie_expires_at=?8, updated_at=?9
             WHERE id=?10",
                rusqlite::params![
                    input.site_id, input.name, input.api_user,
                    self.encrypt_opt(input.username.as_deref())?,
                    self.encrypt_opt(input.password.as_deref())?,
                    self.encrypt_opt(input.cookies_json.as_deref())?,
                    input.cookie_issued_at, input.cookie_expires_at, now_iso(), id,
                ],
            )
            .with_context(|| format!("更新账户失败 id={id}"))?;
        anyhow::ensure!(n == 1, "更新账户失败:id={id} 不存在");
        Ok(())
    }

    #[allow(dead_code)] // Task 8 应用接线接入后使用
    pub fn delete_account(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM accounts WHERE id=?1", [id])
            .context("删除账户失败")?;
        Ok(())
    }

    #[allow(dead_code)] // Task 8 应用接线接入后使用
    pub fn get_account(&self, id: i64) -> Result<Option<Account>> {
        let accounts = self.query_accounts("WHERE id=?1", rusqlite::params![id])?;
        Ok(accounts.into_iter().next())
    }

    #[allow(dead_code)] // Task 8 应用接线接入后使用
    pub fn list_accounts(&self, site_id: i64) -> Result<Vec<Account>> {
        self.query_accounts("WHERE site_id=?1 ORDER BY id", rusqlite::params![site_id])
    }

    #[allow(dead_code)] // Task 8 应用接线接入后使用
    pub fn list_all_accounts(&self) -> Result<Vec<Account>> {
        self.query_accounts("ORDER BY site_id, id", rusqlite::params![])
    }

    #[allow(dead_code)] // Task 8 应用接线接入后使用
    pub fn count_accounts(&self, site_id: i64) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM accounts WHERE site_id=?1",
                [site_id],
                |r| r.get(0),
            )
            .context("统计账户数失败")
    }

    /// 账户查询的唯一 SELECT 来源(列清单仅出现于此),行内同时完成敏感列解密。
    /// suffix 仅限本模块内常量字符串,不得拼接外部输入。
    #[allow(dead_code)] // Task 8 应用接线接入后使用
    fn query_accounts(&self, suffix: &str, params: impl rusqlite::Params) -> Result<Vec<Account>> {
        let sql = format!(
            "SELECT id, site_id, name, api_user, username_enc, password_enc, cookies_enc,
                    cookie_issued_at, cookie_expires_at, created_at, updated_at
             FROM accounts {suffix}"
        );
        let mut stmt = self.conn.prepare(&sql).context("查询账户失败")?;
        let mut rows = stmt.query(params).context("查询账户失败")?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().context("查询账户失败")? {
            // 先取出定位字段,解密失败时错误信息能指明具体账户
            let id: i64 = row.get(0)?;
            let name: String = row.get(2)?;
            out.push(Account {
                id,
                site_id: row.get(1)?,
                name: name.clone(),
                api_user: row.get(3)?,
                username: self
                    .decrypt_opt(row.get::<_, Option<Vec<u8>>>(4)?)
                    .with_context(|| format!("账户 {name}(id={id}) 解密失败,主密钥可能已更换"))?,
                password: self
                    .decrypt_opt(row.get::<_, Option<Vec<u8>>>(5)?)
                    .with_context(|| format!("账户 {name}(id={id}) 解密失败,主密钥可能已更换"))?,
                cookies_json: self
                    .decrypt_opt(row.get::<_, Option<Vec<u8>>>(6)?)
                    .with_context(|| format!("账户 {name}(id={id}) 解密失败,主密钥可能已更换"))?,
                cookie_issued_at: row.get(7)?,
                cookie_expires_at: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            });
        }
        Ok(out)
    }

    /// 空串与 None 一律存 NULL,避免空值产生无意义密文。
    #[allow(dead_code)] // Task 7/8 接入后使用
    fn encrypt_opt(&self, value: Option<&str>) -> Result<Option<Vec<u8>>> {
        match value {
            Some(v) if !v.is_empty() => Ok(Some(self.crypto.encrypt(v)?)),
            _ => Ok(None),
        }
    }

    #[allow(dead_code)] // Task 8 应用接线接入后使用
    fn decrypt_opt(&self, blob: Option<Vec<u8>>) -> Result<Option<String>> {
        match blob {
            Some(b) => Ok(Some(self.crypto.decrypt(&b)?)),
            None => Ok(None),
        }
    }

    /// 首次启动导入:meta.env_imported 存在则跳过。
    #[allow(dead_code)] // Task 8 应用接线接入后使用
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
        // 解析有错时不写标记:修复 .env 后下次启动自动重试(幂等去重保证重试安全)。
        // accounts_skipped(引用不存在站点等)不参与门控,避免用户删除的数据被反复复活。
        if report.parse_errors > 0 {
            log::warn("部分配置解析失败,未写入导入标记;修复 .env 后下次启动将自动重试");
            return Ok(());
        }
        self.set_meta("env_imported", "1")
    }

    /// 导入内置站点 + PROVIDERS + ANYROUTER_ACCOUNTS;可重复调用(按唯一键去重)。
    #[allow(dead_code)] // Task 8 应用接线接入后使用
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
                Err(e) => {
                    log::warn(&format!("PROVIDERS 解析失败,跳过导入: {}", e));
                    report.parse_errors += 1;
                }
            }
        }

        // 3) 账户(ANYROUTER_ACCOUNTS)
        if let Some(json) = accounts_json {
            match serde_json::from_str::<Vec<AccountConfig>>(json) {
                Ok(accounts) => {
                    for (i, acc) in accounts.iter().enumerate() {
                        // 缺省名按数组下标生成;若首轮导入失败且用户在重试前重排 .env,序号会漂移——
                        // 已接受的窗口,同名碰撞由 api_user 比对兜底告警
                        let name = acc.get_display_name(i);
                        let Some(site) = self.find_site_by_name(&acc.provider)? else {
                            log::warn(&format!("账户 {} 引用了不存在的站点 {},跳过", name, acc.provider));
                            report.accounts_skipped += 1;
                            continue;
                        };
                        // 幂等查重走 api_user 明文列,不触发解密:
                        // 同名且同 api_user → 重试残留,静默跳过;
                        // 同名不同 api_user → 真实命名碰撞,警告并计入跳过,避免静默丢数据。
                        if let Some(existing) = self.find_account_api_user(site.id, &name)? {
                            if existing != acc.api_user {
                                log::warn(&format!(
                                    "账户 {} 与既有账户同名但 api_user 不同,跳过导入",
                                    name
                                ));
                                report.accounts_skipped += 1;
                            }
                            continue;
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
                                Some(serde_json::to_string(&entries).context("序列化 cookie 数组失败")?)
                            } else {
                                None
                            },
                            cookie_issued_at: if has_cookies { Some(now_iso()) } else { None },
                            cookie_expires_at: None,
                        })?;
                        report.accounts_added += 1;
                    }
                }
                Err(e) => {
                    log::warn(&format!("ANYROUTER_ACCOUNTS 解析失败,跳过导入: {}", e));
                    report.parse_errors += 1;
                }
            }
        }

        Ok(report)
    }

    /// 幂等查重:按(site_id, name)取既有账户的 api_user 明文列,不触发解密。
    fn find_account_api_user(&self, site_id: i64, name: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT api_user FROM accounts WHERE site_id=?1 AND name=?2",
                rusqlite::params![site_id, name],
                |r| r.get(0),
            )
            .optional()
            .context("查询账户重名失败")
    }

    /// 适配现有 UI/签到通路:站点 → ProviderConfig 映射(键为站点名)。
    #[allow(dead_code)] // 旧适配入口;新通路直接用 site_to_provider_config
    pub fn load_providers_for_ui(
        &self,
    ) -> Result<std::collections::HashMap<String, ProviderConfig>> {
        let mut map = std::collections::HashMap::new();
        for site in self.list_sites()? {
            map.insert(site.name.clone(), site_to_provider_config(&site));
        }
        Ok(map)
    }

    /// 适配现有 UI/签到通路:账户 → AccountConfig 列表。
    /// cookies 还原为 {k:v} 对象;账密以 _username/_password 注入(沿用旧偷渡约定)。
    #[allow(dead_code)] // 旧适配入口;新通路按 account_id 走 account_to_legacy_config
    pub fn load_accounts_for_ui(&self) -> Result<Vec<AccountConfig>> {
        let sites: std::collections::HashMap<i64, String> = self
            .list_sites()?
            .into_iter()
            .map(|s| (s.id, s.name))
            .collect();
        let mut out = Vec::new();
        for acc in self.list_all_accounts()? {
            let Some(provider) = sites.get(&acc.site_id) else { continue };
            out.push(account_to_legacy_config(&acc, provider));
        }
        Ok(out)
    }

    /// 仅测试用:读取账户三个加密列的原始 BLOB。
    #[cfg(test)]
    #[allow(clippy::type_complexity)] // 一次性测试辅助,三列元组不值得提类型别名
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
}

/// 从域名/URL 提取 host(含端口),供 cookie 的 domain 字段使用。
#[allow(dead_code)] // Task 7 .env 导入接入后使用
pub fn host_of(domain: &str) -> &str {
    let no_scheme = domain
        .strip_prefix("https://")
        .or_else(|| domain.strip_prefix("http://"))
        .unwrap_or(domain);
    no_scheme.split('/').next().unwrap_or(no_scheme)
}

/// 旧格式 cookies(对象或 "k=v; " 串)→ (结构化数组, _username, _password)。
/// 手动录入/导入场景:expires 置 -1(未知),domain 取站点 host。
///
/// entries 的 domain 字段可能含端口(取自站点配置),仅作存档展示;
/// 如需直接喂 Playwright add_cookies,须先去端口并重推导。
#[allow(dead_code)] // Task 7 .env 导入接入后使用
pub fn cookies_value_to_entries(
    raw: &serde_json::Value,
    domain_host: &str,
) -> (Vec<serde_json::Value>, Option<String>, Option<String>) {
    let mut pairs: Vec<(String, String)> = Vec::new();
    match raw {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                // 与 Python 通路 str(v) 对齐:数字/bool 强转字符串,不得静默丢弃
                let s = match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => continue, // 嵌套结构无 cookie 语义,跳过
                };
                pairs.push((k.clone(), s));
            }
        }
        serde_json::Value::String(s) => {
            for part in s.split(';') {
                if let Some((k, v)) = part.split_once('=') {
                    let k = k.trim();
                    if k.is_empty() {
                        continue; // 空键(如 "=v")非法 cookie,浏览器会拒,直接跳过
                    }
                    pairs.push((k.to_string(), v.trim().to_string()));
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
#[allow(dead_code)] // Task 8 应用接线接入后使用
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

/// Site → 旧 ProviderConfig(签到/Playwright 通路与 UI 适配共用的唯一转换来源)。
pub fn site_to_provider_config(site: &Site) -> ProviderConfig {
    ProviderConfig {
        name: site.name.clone(),
        domain: site.domain.clone(),
        login_path: site.login_path.clone(),
        sign_in_path: site.sign_in_path.clone(),
        user_info_path: site.user_info_path.clone(),
        api_user_key: site.api_user_key.clone(),
    }
}

/// Account → 旧 AccountConfig:cookies 还原为 {k:v} 对象,账密以 _username/_password 注入。
/// `provider_name` 为账户所属站点名(写入 AccountConfig.provider)。
pub fn account_to_legacy_config(acc: &Account, provider_name: &str) -> AccountConfig {
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
    AccountConfig {
        cookies: serde_json::Value::Object(map),
        api_user: acc.api_user.clone(),
        provider: provider_name.to_string(),
        name: Some(acc.name.clone()),
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
        // 四张表都应存在(查询不报错);业务表为空,meta 含 schema_version 与 key_canary 两行
        for (table, expected) in [("sites", 0), ("accounts", 0), ("account_cache", 0), ("meta", 2)]
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
    fn update_missing_account_fails() {
        let s = test_storage();
        let err = s.update_account(9999, &sample_account(1, "Ghost")).unwrap_err();
        assert!(err.to_string().contains("9999"), "错误信息应指明缺失的 id:{err}");
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

    #[test]
    fn update_account_can_move_between_sites() {
        let s = test_storage();
        let site_a = s.insert_site(&SiteInput::with_defaults("A", "https://a.com")).unwrap();
        let site_b = s.insert_site(&SiteInput::with_defaults("B", "https://b.com")).unwrap();
        let id = s.insert_account(&sample_account(site_a, "用户A")).unwrap();

        // 整行覆盖语义:update 的 site_id 生效,账户从 A 移动到 B
        s.update_account(id, &sample_account(site_b, "用户A")).unwrap();

        assert!(s.list_accounts(site_a).unwrap().is_empty());
        let accounts = s.list_accounts(site_b).unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, id);
        assert_eq!(accounts[0].site_id, site_b);
        assert_eq!(accounts[0].name, "用户A");
        assert_eq!(accounts[0].username.as_deref(), Some("alice"));
    }

    #[test]
    fn empty_string_credentials_stored_as_null() {
        // encrypt_opt 将空串与 None 合并存 NULL 是有意语义,钉死防止后续层依赖相反假设
        let s = test_storage();
        let site_id = s.insert_site(&SiteInput::with_defaults("A", "https://a.com")).unwrap();
        let mut input = sample_account(site_id, "用户A");
        input.username = Some("".into());
        input.password = Some("".into());
        input.cookies_json = Some("".into());
        let id = s.insert_account(&input).unwrap();

        let acc = s.get_account(id).unwrap().unwrap();
        assert_eq!(acc.username, None);
        assert_eq!(acc.password, None);
        assert_eq!(acc.cookies_json, None);
        assert_eq!(s.raw_account_blobs(id), (None, None, None));
    }

    #[test]
    fn corrupted_blob_fails_loud() {
        let s = test_storage();
        let site_id = s.insert_site(&SiteInput::with_defaults("A", "https://a.com")).unwrap();
        let id = s.insert_account(&sample_account(site_id, "用户A")).unwrap();
        s.conn
            .execute("UPDATE accounts SET username_enc = ?1", rusqlite::params![vec![0u8; 40]])
            .unwrap();
        let err = s.get_account(id).unwrap_err();
        assert!(
            err.to_string().contains("解密失败,主密钥可能已更换"),
            "错误信息应含账户定位文案:{err}"
        );
    }

    #[test]
    fn reopening_with_wrong_key_fails_fast() {
        let path =
            std::env::temp_dir().join(format!("anyrouter_canary_test_{}.db", std::process::id()));
        if path.exists() {
            std::fs::remove_file(&path).unwrap();
        }
        // key A 首次建库:写入金丝雀并正常关闭
        drop(Storage::open_at(&path, Crypto::from_key(&[7u8; 32])).unwrap());
        // key B 打开应快速失败,而非静默用新密钥(map 丢弃 Ok 值以满足 unwrap_err 的 Debug 约束)
        let err = Storage::open_at(&path, Crypto::from_key(&[8u8; 32])).map(|_| ()).unwrap_err();
        assert!(err.to_string().contains("主密钥校验失败"), "错误信息不符:{err}");
        // key A 重新打开仍应成功(校验不得破坏数据)
        drop(Storage::open_at(&path, Crypto::from_key(&[7u8; 32])).unwrap());
        std::fs::remove_file(&path).unwrap();
    }

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
    fn cookie_object_coerces_non_string_values() {
        // 现有 Python 通路(parse_cookies)用 str(v) 强转,数字/bool 值 cookie 能正常签到;
        // 导入侧若静默丢弃即丢数据,必须转字符串保留
        let raw = serde_json::json!({"n": 123, "b": true, "s": "x"});
        let (entries, username, password) = cookies_value_to_entries(&raw, "a.com");
        assert_eq!(username, None);
        assert_eq!(password, None);
        assert_eq!(entries.len(), 3);
        let n = entries.iter().find(|e| e["name"] == "n").unwrap();
        assert_eq!(n["value"], "123");
        let b = entries.iter().find(|e| e["name"] == "b").unwrap();
        assert_eq!(b["value"], "true");
    }

    #[test]
    fn cookie_string_to_entries() {
        // 钉住三个解析边界:值含 base64 padding 的 '='(split_once 只切第一个 '=')、
        // 空键段("=bad" 不得产出空名 entry)、尾随分号(空段跳过)
        let raw = serde_json::json!("session=YWJjZA==; =bad; acw_tc=x;");
        let (entries, username, password) = cookies_value_to_entries(&raw, "a.com");
        assert_eq!(username, None);
        assert_eq!(password, None);
        assert_eq!(entries.len(), 2);
        let session = entries.iter().find(|e| e["name"] == "session").unwrap();
        assert_eq!(session["value"], "YWJjZA==");
        assert!(entries.iter().all(|e| e["name"] != ""), "不得产出空名 entry");
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
        // 两个来源各计 1 个解析错误;import_env_if_needed 据此不写 env_imported 标记
        assert_eq!(report.parse_errors, 2);
    }

    #[test]
    fn env_imported_marker_skips_reimport() {
        // 标记存在时直接短路:不读环境变量,连内置站点也不种
        let s = test_storage();
        s.set_meta("env_imported", "1").unwrap();
        s.import_env_if_needed().unwrap();
        assert!(s.list_sites().unwrap().is_empty());
    }

    #[test]
    fn import_name_collision_and_empty_cookies() {
        let s = test_storage();
        // 三条同站点账户:第 1 条空 cookie 正常导入;第 2 条同名不同 api_user → 告警跳过;
        // 第 3 条与第 1 条完全相同 → 重试残留语义,静默幂等
        let accounts = r#"[
            {"cookies":{},"api_user":"1","provider":"anyrouter","name":"X"},
            {"cookies":{},"api_user":"2","provider":"anyrouter","name":"X"},
            {"cookies":{},"api_user":"1","provider":"anyrouter","name":"X"}
        ]"#;
        let report = s.import_from_strings(None, Some(accounts)).unwrap();
        assert_eq!(report.accounts_added, 1);
        assert_eq!(report.accounts_skipped, 1); // 仅同名不同 api_user 的第 2 条计数
        assert_eq!(report.parse_errors, 0);

        // 库中仍只 1 个账户,且保留首条的 api_user
        let site = s.find_site_by_name("anyrouter").unwrap().unwrap();
        let imported = s.list_accounts(site.id).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].name, "X");
        assert_eq!(imported[0].api_user, "1");
        // 空 cookie:不产出空数组密文,也不记签发时间
        assert_eq!(imported[0].cookies_json, None);
        assert_eq!(imported[0].cookie_issued_at, None);
    }

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
}
