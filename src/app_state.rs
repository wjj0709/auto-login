use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Local;
use gpui::SharedString;

use crate::playwright::PlaywrightResult;
use crate::storage::{Account, Site, Storage};

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
            timestamp: SharedString::from(Local::now().format("%H:%M:%S%.3f").to_string()),
        }
    }
}

/// 账号余额信息
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // 字段由 Milestone B/D 的统计与详情页读取
pub struct BalanceInfo {
    pub quota: f64,
    pub used_quota: f64,
    pub reward: f64,
    pub usage_increase: f64,
}

/// 当前整页视图。详情页通过面包屑返回主页。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppView {
    Home,
    /// 账户详情页(account_id)
    AccountDetail(i64),
}

/// 全局应用状态 (GPUI Model)。
///
/// 数据源为 SQLite 实体(`sites` / `accounts`),运行态(状态/结果/余额)以 `account_id` 为键。
pub struct AppState {
    /// 站点列表(来自 SQLite)
    pub sites: Vec<Site>,
    /// 账户列表(来自 SQLite,敏感字段已解密驻留内存)
    pub accounts: Vec<Account>,
    /// 数据库句柄(界面 CRUD 与后台任务共享)
    pub db: Arc<Mutex<Storage>>,

    /// 各账户签到状态,键为 account_id
    pub checkin_status: HashMap<i64, CheckInStatus>,
    /// Playwright 签到结果,键为 account_id
    pub results: HashMap<i64, PlaywrightResult>,
    /// 余额数据,键为 account_id
    pub balances: HashMap<i64, BalanceInfo>,
    /// 日志缓冲
    pub logs: Vec<LogEntry>,
    /// 全局运行状态
    pub is_running: bool,
    /// 本次运行会话内的成功/失败计数(不持久化)
    pub success_count: usize,
    pub fail_count: usize,

    /// 当前整页视图
    pub view: AppView,
    /// 底部日志抽屉是否展开
    pub log_drawer_open: bool,
}

impl AppState {
    /// 创建初始状态
    pub fn new(sites: Vec<Site>, accounts: Vec<Account>, db: Arc<Mutex<Storage>>) -> Self {
        let checkin_status = accounts
            .iter()
            .map(|a| (a.id, CheckInStatus::Idle))
            .collect();

        Self {
            sites,
            accounts,
            db,
            checkin_status,
            results: HashMap::new(),
            balances: HashMap::new(),
            logs: Vec::new(),
            is_running: false,
            success_count: 0,
            fail_count: 0,
            view: AppView::Home,
            log_drawer_open: false,
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
    pub fn set_status(&mut self, account_id: i64, status: CheckInStatus) {
        self.checkin_status.insert(account_id, status);
    }

    /// 读取签到状态(缺省 Idle)
    #[allow(dead_code)] // 由界面层使用
    pub fn status_of(&self, account_id: i64) -> CheckInStatus {
        self.checkin_status
            .get(&account_id)
            .cloned()
            .unwrap_or(CheckInStatus::Idle)
    }

    /// 账户总数
    pub fn total_accounts(&self) -> usize {
        self.accounts.len()
    }

    /// 按 id 查账户
    #[allow(dead_code)] // 由界面层使用
    pub fn account_by_id(&self, id: i64) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    /// 按 id 查站点
    #[allow(dead_code)] // 由界面层使用
    pub fn site_by_id(&self, id: i64) -> Option<&Site> {
        self.sites.iter().find(|s| s.id == id)
    }

    /// 取某站点下的账户
    #[allow(dead_code)] // 由界面层使用
    pub fn accounts_of_site(&self, site_id: i64) -> Vec<&Account> {
        self.accounts.iter().filter(|a| a.site_id == site_id).collect()
    }

    /// 取某账户的余额缓存
    #[allow(dead_code)] // 由界面层使用
    pub fn balance_of(&self, account_id: i64) -> Option<&BalanceInfo> {
        self.balances.get(&account_id)
    }

    /// 用最新的 sites/accounts 覆盖,并同步状态键(界面 CRUD 后调用)。
    #[allow(dead_code)] // Milestone C 接入 CRUD 后使用
    pub fn replace_data(&mut self, sites: Vec<Site>, accounts: Vec<Account>) {
        // 保留仍存在账户的状态,丢弃已删除账户的残留键
        let valid: std::collections::HashSet<i64> = accounts.iter().map(|a| a.id).collect();
        self.checkin_status.retain(|id, _| valid.contains(id));
        self.results.retain(|id, _| valid.contains(id));
        self.balances.retain(|id, _| valid.contains(id));
        for a in &accounts {
            self.checkin_status.entry(a.id).or_insert(CheckInStatus::Idle);
        }
        self.sites = sites;
        self.accounts = accounts;
    }

    /// 处理 Playwright 结果(键为 account_id)
    pub fn process_result(&mut self, account_id: i64, result: PlaywrightResult) {
        if result.success {
            self.success_count += 1;
            self.set_status(account_id, CheckInStatus::Success);
        } else {
            self.fail_count += 1;
            let err = result.error.clone().unwrap_or_else(|| "unknown".into());
            self.set_status(account_id, CheckInStatus::Failed(err));
        }

        // 提取余额信息
        if let Some(ref after) = result.after {
            let before_quota = result.before.as_ref().map(|b| b.quota).unwrap_or(0.0);
            let before_used = result.before.as_ref().map(|b| b.used_quota).unwrap_or(0.0);
            self.balances.insert(
                account_id,
                BalanceInfo {
                    quota: after.quota,
                    used_quota: after.used_quota,
                    reward: (after.quota + after.used_quota) - (before_quota + before_used),
                    usage_increase: after.used_quota - before_used,
                },
            );
        }

        self.results.insert(account_id, result);
    }

    /// 重置所有状态(准备新一轮签到)
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
