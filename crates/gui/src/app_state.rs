#![allow(dead_code)]

use std::sync::mpsc;
use std::sync::{Arc, atomic::AtomicBool};

use gpui::{AppContext, Entity};

use anyrouter_core::models::{Account, Site, SiteWithStats};
use anyrouter_core::storage::Storage;

use crate::components::text_input::TextInput;

/// 当前主视图
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewKind {
    Home,
    AccountDetail(i64),
}

/// 弹窗类型
#[derive(Debug, Clone)]
pub enum ModalKind {
    AccountList(i64),
    SiteForm(Option<i64>),
    AccountForm { site_id: i64, account_id: Option<i64> },
    ConfirmDelete(DeleteTarget),
}

/// 删除确认的目标
#[derive(Debug, Clone)]
pub enum DeleteTarget {
    Site { id: i64, name: String, account_count: usize },
    Account { id: i64, name: String },
}

/// 日志条目
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// 站点表单的输入框集合（持久化的 TextInput 实体）
pub struct SiteFormFields {
    pub editing_id: Option<i64>,
    pub show_advanced: bool,
    pub name: Entity<TextInput>,
    pub domain: Entity<TextInput>,
    pub login_path: Entity<TextInput>,
    pub sign_in_path: Entity<TextInput>,
    pub user_info_path: Entity<TextInput>,
    pub tokens_path: Entity<TextInput>,
    pub logs_path: Entity<TextInput>,
    pub chart_path: Entity<TextInput>,
    pub api_user_key: Entity<TextInput>,
}

/// 账户凭据录入方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredMethod {
    /// 直接粘贴 Cookie
    Cookie,
    /// 用户名 + 密码
    Password,
}

/// 账户表单的输入框集合
pub struct AccountFormFields {
    pub site_id: i64,
    pub editing_id: Option<i64>,
    pub cred_method: CredMethod,
    pub name: Entity<TextInput>,
    pub api_user: Entity<TextInput>,
    pub cookie: Entity<TextInput>,
    pub username: Entity<TextInput>,
    pub password: Entity<TextInput>,
}

impl SiteFormFields {
    /// 新建站点：填入 new-api 系站点的默认路径
    pub fn new_create<C: AppContext>(cx: &mut C) -> Self {
        Self {
            editing_id: None,
            show_advanced: false,
            name: cx.new(|cx| TextInput::new(cx, "如 AnyRouter", "", false)),
            domain: cx.new(|cx| TextInput::new(cx, "https://example.com", "", false)),
            login_path: cx.new(|cx| TextInput::new(cx, "/login", "/login", false)),
            sign_in_path: cx.new(|cx| {
                TextInput::new(cx, "留空=自动签到型", "/api/user/sign_in", false)
            }),
            user_info_path: cx.new(|cx| TextInput::new(cx, "", "/api/user/self", false)),
            tokens_path: cx.new(|cx| TextInput::new(cx, "", "/api/token/", false)),
            logs_path: cx.new(|cx| TextInput::new(cx, "", "/api/log/self", false)),
            chart_path: cx.new(|cx| TextInput::new(cx, "", "/api/data/self", false)),
            api_user_key: cx.new(|cx| TextInput::new(cx, "", "new-api-user", false)),
        }
    }

    /// 编辑站点：用已有数据回填
    pub fn new_edit<C: AppContext>(cx: &mut C, site: &Site) -> Self {
        Self {
            editing_id: Some(site.id),
            show_advanced: false,
            name: cx.new(|cx| TextInput::new(cx, "如 AnyRouter", site.name.clone(), false)),
            domain: cx.new(|cx| TextInput::new(cx, "https://example.com", site.domain.clone(), false)),
            login_path: cx.new(|cx| TextInput::new(cx, "/login", site.login_path.clone(), false)),
            sign_in_path: cx.new(|cx| {
                TextInput::new(
                    cx,
                    "留空=自动签到型",
                    site.sign_in_path.clone().unwrap_or_default(),
                    false,
                )
            }),
            user_info_path: cx.new(|cx| TextInput::new(cx, "", site.user_info_path.clone(), false)),
            tokens_path: cx.new(|cx| TextInput::new(cx, "", site.tokens_path.clone(), false)),
            logs_path: cx.new(|cx| TextInput::new(cx, "", site.logs_path.clone(), false)),
            chart_path: cx.new(|cx| TextInput::new(cx, "", site.chart_path.clone(), false)),
            api_user_key: cx.new(|cx| TextInput::new(cx, "", site.api_user_key.clone(), false)),
        }
    }
}

impl AccountFormFields {
    /// 新建账户
    pub fn new_create<C: AppContext>(cx: &mut C, site_id: i64) -> Self {
        Self {
            site_id,
            editing_id: None,
            cred_method: CredMethod::Cookie,
            name: cx.new(|cx| TextInput::new(cx, "如 主账号", "", false)),
            api_user: cx.new(|cx| TextInput::new(cx, "new-api-user 请求头值", "", false)),
            cookie: cx.new(|cx| TextInput::new_multiline(cx, "session=...; 或 JSON", "", 4)),
            username: cx.new(|cx| TextInput::new(cx, "可选：登录用户名", "", false)),
            password: cx.new(|cx| TextInput::new(cx, "可选：登录密码", "", true)),
        }
    }

    /// 编辑账户
    pub fn new_edit<C: AppContext>(cx: &mut C, account: &Account) -> Self {
        // 根据已有数据推断默认凭据方式
        let cred_method = if account.cookies.as_ref().map(|c| !c.is_empty()).unwrap_or(false) {
            CredMethod::Cookie
        } else if account.username.is_some() {
            CredMethod::Password
        } else {
            CredMethod::Cookie
        };
        Self {
            site_id: account.site_id,
            editing_id: Some(account.id),
            cred_method,
            name: cx.new(|cx| TextInput::new(cx, "如 主账号", account.name.clone(), false)),
            api_user: cx.new(|cx| {
                TextInput::new(cx, "new-api-user 请求头值", account.api_user.clone(), false)
            }),
            cookie: cx.new(|cx| {
                TextInput::new_multiline(
                    cx,
                    "session=...; 或 JSON",
                    account.cookies.clone().unwrap_or_default(),
                    4,
                )
            }),
            username: cx.new(|cx| {
                TextInput::new(
                    cx,
                    "可选：登录用户名",
                    account.username.clone().unwrap_or_default(),
                    false,
                )
            }),
            password: cx.new(|cx| {
                TextInput::new(
                    cx,
                    "可选：登录密码",
                    account.password.clone().unwrap_or_default(),
                    true,
                )
            }),
        }
    }
}

/// 全局应用状态
pub struct AppState {
    pub current_view: ViewKind,
    pub log_drawer_open: bool,
    pub active_modal: Option<ModalKind>,
    pub sites: Vec<SiteWithStats>,
    pub log_entries: Vec<LogEntry>,
    pub running: bool,
    pub run_progress: Option<String>,

    /// 详情页当前 Tab 索引
    pub active_tab: usize,

    /// 站点表单输入框（active_modal == SiteForm 时有效）
    pub site_form: Option<SiteFormFields>,
    /// 账户表单输入框（active_modal == AccountForm 时有效）
    pub account_form: Option<AccountFormFields>,

    /// 表单错误提示文本
    pub form_error: Option<String>,

    // ─── Core 层集成 ──────────────────────────────────────────
    /// 数据库 Storage（主线程使用）
    pub storage: Option<Storage>,

    /// 后台线程日志接收端
    pub log_rx: Option<mpsc::Receiver<LogEntry>>,

    /// 后台线程运行状态标志
    pub bg_running: Arc<AtomicBool>,
}

impl AppState {
    /// 从 Storage 构建初始 AppState（首次启动时调用）
    pub fn from_storage(storage: Storage) -> Self {
        let mut state = Self::default();
        state.storage = Some(storage);
        state.reload_sites();
        state.log_entries.push(LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            level: LogLevel::Info,
            message: format!("已加载 {} 个站点", state.sites.len()),
        });
        state
    }

    /// 从 Storage 加载站点列表（含统计信息）
    pub fn reload_sites(&mut self) {
        if let Some(ref storage) = self.storage {
            match storage.list_sites() {
                Ok(sites) => {
                    let now_rfc = chrono::Utc::now().to_rfc3339();
                    let today_prefix = chrono::Local::now().format("%Y-%m-%d").to_string();
                    self.sites = sites
                        .into_iter()
                        .map(|site| {
                            let accounts = storage
                                .list_accounts_by_site(site.id)
                                .unwrap_or_default();
                            let account_count = accounts.len();
                            let expired_count = accounts
                                .iter()
                                .filter(|a| {
                                    a.cookie_expires_at
                                        .as_ref()
                                        .map(|exp| exp.as_str() < now_rfc.as_str())
                                        .unwrap_or(false)
                                })
                                .count();

                            let mut total_balance = 0.0_f64;
                            let mut checkin_today = 0usize;
                            for a in &accounts {
                                if let Ok(Some(cache)) = storage.get_cache(
                                    a.id,
                                    anyrouter_core::models::CacheKind::Overview,
                                ) {
                                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(
                                        &cache.payload_json,
                                    ) {
                                        if let Some(q) = v.get("quota").and_then(|x| x.as_f64()) {
                                            total_balance += q / 500_000.0;
                                        }
                                    }
                                    if cache.fetched_at.starts_with(&today_prefix) {
                                        checkin_today += 1;
                                    }
                                }
                            }

                            SiteWithStats {
                                site,
                                account_count,
                                total_balance,
                                expired_count,
                                checkin_today,
                            }
                        })
                        .collect();
                }
                Err(e) => {
                    self.log_entries.push(LogEntry {
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        level: LogLevel::Error,
                        message: format!("加载站点失败: {}", e),
                    });
                }
            }
        }
    }

    /// 从 channel 中取出后台线程发送的日志
    pub fn poll_bg_logs(&mut self) {
        if let Some(ref rx) = self.log_rx {
            while let Ok(entry) = rx.try_recv() {
                self.log_entries.push(entry);
            }
        }
        // 限制日志最大条数（保留最近 500 条）
        const MAX_LOG_ENTRIES: usize = 500;
        if self.log_entries.len() > MAX_LOG_ENTRIES {
            let drop_count = self.log_entries.len() - MAX_LOG_ENTRIES;
            self.log_entries.drain(0..drop_count);
        }
        // 更新 running 标志
        if !self.bg_running.load(std::sync::atomic::Ordering::Relaxed) && self.running {
            self.running = false;
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            current_view: ViewKind::Home,
            log_drawer_open: false,
            active_modal: None,
            sites: Vec::new(),
            log_entries: vec![],
            running: false,
            run_progress: None,
            active_tab: 0,
            site_form: None,
            account_form: None,
            form_error: None,
            storage: None,
            log_rx: None,
            bg_running: Arc::new(AtomicBool::new(false)),
        }
    }
}
