#![allow(dead_code)]

use std::sync::mpsc;
use std::sync::{Arc, atomic::AtomicBool};

use anyrouter_core::models::SiteWithStats;
use anyrouter_core::storage::Storage;

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

    // ─── 表单缓冲区（站点表单）─────────────────────────────────
    pub form_site_name: String,
    pub form_site_domain: String,

    // ─── 表单缓冲区（账户表单）─────────────────────────────────
    pub form_account_name: String,
    pub form_account_api_user: String,
    pub form_account_cookie: String,
    pub form_account_username: String,
    pub form_account_password: String,

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
            form_site_name: String::new(),
            form_site_domain: String::new(),
            form_account_name: String::new(),
            form_account_api_user: String::new(),
            form_account_cookie: String::new(),
            form_account_username: String::new(),
            form_account_password: String::new(),
            storage: None,
            log_rx: None,
            bg_running: Arc::new(AtomicBool::new(false)),
        }
    }
}
