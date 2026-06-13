#![allow(dead_code)]

use anyrouter_core::models::{Site, SiteWithStats};

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
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            current_view: ViewKind::Home,
            log_drawer_open: false,
            active_modal: None,
            sites: vec![
                SiteWithStats {
                    site: Site {
                        id: 1,
                        name: "AnyRouter".into(),
                        domain: "https://anyrouter.top".into(),
                        login_path: "/login".into(),
                        sign_in_path: Some("/sign-in".into()),
                        user_info_path: "/api/user".into(),
                        tokens_path: "/api/tokens".into(),
                        logs_path: "/api/logs".into(),
                        chart_path: "/api/chart".into(),
                        api_user_key: "email".into(),
                        created_at: "2025-01-01".into(),
                        updated_at: "2025-01-01".into(),
                    },
                    account_count: 3,
                    total_balance: 37.50,
                    expired_count: 1,
                    checkin_today: 2,
                },
                SiteWithStats {
                    site: Site {
                        id: 2,
                        name: "AgentRouter".into(),
                        domain: "https://agentrouter.org".into(),
                        login_path: "/login".into(),
                        sign_in_path: Some("/sign-in".into()),
                        user_info_path: "/api/user".into(),
                        tokens_path: "/api/tokens".into(),
                        logs_path: "/api/logs".into(),
                        chart_path: "/api/chart".into(),
                        api_user_key: "email".into(),
                        created_at: "2025-01-01".into(),
                        updated_at: "2025-01-01".into(),
                    },
                    account_count: 2,
                    total_balance: 19.30,
                    expired_count: 0,
                    checkin_today: 2,
                },
            ],
            log_entries: Vec::new(),
            running: false,
            run_progress: None,
        }
    }
}
