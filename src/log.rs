use chrono::Local;

/// 日志级别
#[allow(dead_code)]
enum Level {
    Info,
    Success,
    Warn,
    Error,
    Debug,
    System,
    Network,
    Processing,
}

impl Level {
    fn tag(&self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Success => "SUCCESS",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
            Level::Debug => "DEBUG",
            Level::System => "SYSTEM",
            Level::Network => "NETWORK",
            Level::Processing => "PROCESSING",
        }
    }
}

fn timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

fn log(level: Level, msg: &str) {
    println!("[{}] [{}] {}", timestamp(), level.tag(), msg);
}

fn log_with_prefix(level: Level, prefix: &str, msg: &str) {
    if prefix.is_empty() {
        log(level, msg);
    } else {
        println!("[{}] [{}] [{}] {}", timestamp(), level.tag(), prefix, msg);
    }
}

// ---- 公共日志函数 ----

pub fn info(msg: &str) { log(Level::Info, msg); }
pub fn info_f(prefix: &str, msg: &str) { log_with_prefix(Level::Info, prefix, msg); }

pub fn success(msg: &str) { log(Level::Success, msg); }
#[allow(dead_code)]
pub fn success_f(prefix: &str, msg: &str) { log_with_prefix(Level::Success, prefix, msg); }

pub fn warn(msg: &str) { log(Level::Warn, msg); }
#[allow(dead_code)]
pub fn warn_f(prefix: &str, msg: &str) { log_with_prefix(Level::Warn, prefix, msg); }

pub fn error(msg: &str) { log(Level::Error, msg); }
pub fn error_f(prefix: &str, msg: &str) { log_with_prefix(Level::Error, prefix, msg); }

pub fn debug(msg: &str) { log(Level::Debug, msg); }
#[allow(dead_code)]
pub fn debug_f(prefix: &str, msg: &str) { log_with_prefix(Level::Debug, prefix, msg); }

#[allow(dead_code)]
pub fn system(msg: &str) { log(Level::System, msg); }

#[allow(dead_code)]
pub fn network(prefix: &str, msg: &str) { log_with_prefix(Level::Network, prefix, msg); }

#[allow(dead_code)]
pub fn processing(msg: &str) { log(Level::Processing, msg); }
#[allow(dead_code)]
pub fn processing_f(prefix: &str, msg: &str) { log_with_prefix(Level::Processing, prefix, msg); }

/// 打印分隔线
#[allow(dead_code)]
pub fn separator() {
    println!("────────────────────────────────────────────────────");
}

/// 打印阶段标题
#[allow(dead_code)]
pub fn phase(title: &str) {
    separator();
    println!("[{}] [PHASE] {}", timestamp(), title);
    separator();
}

/// 打印当前时间
#[allow(dead_code)]
pub fn now_time(label: &str) {
    println!("[{}] [TIME] {}", timestamp(), label);
}

/// 打印原始信息（不带标签，用于通知内容等）
#[allow(dead_code)]
pub fn raw(msg: &str) {
    println!("{}", msg);
}
