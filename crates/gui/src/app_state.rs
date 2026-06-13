#![allow(dead_code)]

use anyrouter_core::models::SiteWithStats;

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
            sites: Vec::new(),
            log_entries: Vec::new(),
            running: false,
            run_progress: None,
        }
    }
}
