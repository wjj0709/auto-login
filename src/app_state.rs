use std::collections::HashMap;
use gpui::SharedString;
use chrono::Local;

use crate::config::AccountConfig;
use crate::playwright::PlaywrightResult;

/// 签到状态
#[derive(Debug, Clone, PartialEq)]
pub enum CheckInStatus {
    Idle,
    Running,
    Success,
    Failed(String),
}

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogLevel {
    Info,
    Success,
    Warn,
    Error,
    Debug,
}

/// 单条日志
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: LogLevel,
    pub tag: SharedString,
    pub message: SharedString,
    pub timestamp: SharedString,
}

impl LogEntry {
    pub fn new(level: LogLevel, tag: &str, message: &str) -> Self {
        Self {
            level,
            tag: SharedString::from(tag.to_string()),
            message: SharedString::from(message.to_string()),
            timestamp: SharedString::from(
                Local::now().format("%H:%M:%S%.3f").to_string(),
            ),
        }
    }
}

/// 账号余额信息
#[derive(Debug, Clone, Default)]
pub struct BalanceInfo {
    pub quota: f64,
    pub used_quota: f64,
    pub reward: f64,
    #[allow(dead_code)]
    pub usage_increase: f64,
}

/// 全局应用状态 (GPUI Model)
pub struct AppState {
    /// 账号列表
    pub accounts: Vec<AccountConfig>,
    /// 各账号签到状态
    pub checkin_status: HashMap<String, CheckInStatus>,
    /// Playwright 签到结果
    pub results: HashMap<String, PlaywrightResult>,
    /// 日志缓冲
    pub logs: Vec<LogEntry>,
    /// 余额数据
    pub balances: HashMap<String, BalanceInfo>,
    /// 全局运行状态
    pub is_running: bool,
    /// 成功/失败计数
    pub success_count: usize,
    pub fail_count: usize,
    /// 当前活跃面板
    pub active_panel: ActivePanel,
}

/// 当前活跃面板
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActivePanel {
    Accounts,
    Logs,
    Dashboard,
}

impl AppState {
    /// 创建初始状态
    pub fn new(accounts: Vec<AccountConfig>) -> Self {
        let mut checkin_status = HashMap::new();
        for (i, account) in accounts.iter().enumerate() {
            let name = account.get_display_name(i);
            checkin_status.insert(name, CheckInStatus::Idle);
        }

        Self {
            accounts,
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

    /// 添加日志条目
    pub fn add_log(&mut self, level: LogLevel, tag: &str, message: &str) {
        self.logs.push(LogEntry::new(level, tag, message));
        // 限制日志缓冲区大小
        if self.logs.len() > 5000 {
            self.logs.drain(..2000);
        }
    }

    /// 清空日志
    pub fn clear_logs(&mut self) {
        self.logs.clear();
    }

    /// 更新签到状态
    pub fn set_status(&mut self, name: &str, status: CheckInStatus) {
        self.checkin_status.insert(name.to_string(), status);
    }

    /// 获取账号总数
    pub fn total_accounts(&self) -> usize {
        self.accounts.len()
    }

    /// 处理 Playwright 结果
    pub fn process_result(&mut self, name: &str, result: PlaywrightResult) {
        if result.success {
            self.success_count += 1;
            self.set_status(name, CheckInStatus::Success);
        } else {
            self.fail_count += 1;
            let err = result.error.clone().unwrap_or_else(|| "unknown".into());
            self.set_status(name, CheckInStatus::Failed(err));
        }

        // 提取余额信息
        if let Some(ref after) = result.after {
            let before_quota = result.before.as_ref().map(|b| b.quota).unwrap_or(0.0);
            let before_used = result.before.as_ref().map(|b| b.used_quota).unwrap_or(0.0);
            self.balances.insert(name.to_string(), BalanceInfo {
                quota: after.quota,
                used_quota: after.used_quota,
                reward: (after.quota + after.used_quota) - (before_quota + before_used),
                usage_increase: after.used_quota - before_used,
            });
        }

        self.results.insert(name.to_string(), result);
    }

    /// 重置所有状态（准备新一轮签到）
    pub fn reset_for_new_run(&mut self) {
        self.success_count = 0;
        self.fail_count = 0;
        self.results.clear();
        self.balances.clear();
        for status in self.checkin_status.values_mut() {
            *status = CheckInStatus::Idle;
        }
    }
}
