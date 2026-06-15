// ============================================================================
// log.rs — FastLog-inspired async logging package
// ============================================================================
// 功能：
// 1. 控制台按日志级别彩色输出
// 2. 异步写入文件，避免业务线程阻塞在磁盘 I/O
// 3. 按日期目录分类，按文件大小切分
// 4. 每个级别单独落盘，同时写入 all.log
// ============================================================================

use chrono::{DateTime, Local};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const DEFAULT_LOG_DIR: &str = "logs";
const DEFAULT_MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;
const DEFAULT_FLUSH_INTERVAL_MS: u64 = 250;
const DEFAULT_CONFIG_FILE: &str = "conf.json";

/// 日志级别枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Success,
    Warn,
    Error,
    System,
    Network,
    Processing,
    Fatal,
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Success => "SUCCESS",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
            Level::System => "SYSTEM",
            Level::Network => "NETWORK",
            Level::Processing => "PROCESSING",
            Level::Fatal => "FATAL",
        }
    }

    fn file_stem(self) -> &'static str {
        match self {
            Level::Trace => "trace",
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Success => "success",
            Level::Warn => "warn",
            Level::Error => "error",
            Level::System => "system",
            Level::Network => "network",
            Level::Processing => "processing",
            Level::Fatal => "fatal",
        }
    }

    fn ansi_color(self) -> &'static str {
        match self {
            Level::Trace => "\x1b[90m",
            Level::Debug => "\x1b[36m",
            Level::Info => "\x1b[34m",
            Level::Success => "\x1b[32m",
            Level::Warn => "\x1b[33m",
            Level::Error => "\x1b[31m",
            Level::System => "\x1b[35m",
            Level::Network => "\x1b[96m",
            Level::Processing => "\x1b[95m",
            Level::Fatal => "\x1b[1;31m",
        }
    }
}

/// 日志配置
#[derive(Debug, Clone)]
pub struct LogConfig {
    directory: PathBuf,
    max_file_size: u64,
    console_colors: bool,
    flush_interval: Duration,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            directory: PathBuf::from(DEFAULT_LOG_DIR),
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            console_colors: true,
            flush_interval: Duration::from_millis(DEFAULT_FLUSH_INTERVAL_MS),
        }
    }
}

impl_ref_accessors!(LogConfig {
    directory: PathBuf => directory, set_directory;
    flush_interval: Duration => flush_interval, set_flush_interval;
});

impl_copy_accessors!(LogConfig {
    max_file_size: u64 => max_file_size, set_max_file_size;
    console_colors: bool => console_colors, set_console_colors;
});

impl LogConfig {
    pub fn from_config_file() -> Self {
        let mut config = Self::default();
        if let Some(file_config) = load_file_log_config() {
            if let Some(path) = file_config.path {
                config.set_directory(PathBuf::from(path));
            }
            if let Some(path) = file_config.log_dir {
                config.set_directory(PathBuf::from(path));
            }
            if let Some(max_file_size) = file_config.max_file_size {
                config.set_max_file_size(max_file_size.max(1));
            }
            if let Some(max_file_size_mb) = file_config.max_file_size_mb {
                config.set_max_file_size(max_file_size_mb.max(1) * 1024 * 1024);
            }
        }
        config
    }
}

/// 单条日志记录
#[derive(Debug, Clone)]
struct LogRecord {
    level: Level,
    timestamp: DateTime<Local>,
    prefix: Option<String>,
    message: String,
    raw: bool,
}

impl LogRecord {
    fn new(level: Level, prefix: Option<String>, message: String) -> Self {
        Self {
            level,
            timestamp: Local::now(),
            prefix,
            message,
            raw: false,
        }
    }

    fn raw(message: String) -> Self {
        Self {
            level: Level::Info,
            timestamp: Local::now(),
            prefix: None,
            message,
            raw: true,
        }
    }

    fn render(&self) -> String {
        if self.raw {
            return self.message.clone();
        }

        let timestamp = self.timestamp.format("%Y-%m-%d %H:%M:%S%.3f");
        match self.prefix.as_ref().filter(|prefix| !prefix.is_empty()) {
            Some(prefix) => format!(
                "[{}] [{}] [{}] {}",
                timestamp,
                self.level.tag(),
                prefix,
                self.message
            ),
            None => format!("[{}] [{}] {}", timestamp, self.level.tag(), self.message),
        }
    }

    fn render_console(&self, colorize: bool) -> String {
        if self.raw || !colorize {
            return self.render();
        }

        let timestamp = self.timestamp.format("%Y-%m-%d %H:%M:%S%.3f");
        let colored_level = format!("{}[{}]\x1b[0m", self.level.ansi_color(), self.level.tag());
        match self.prefix.as_ref().filter(|prefix| !prefix.is_empty()) {
            Some(prefix) => format!(
                "[{}] {} [{}] {}",
                timestamp, colored_level, prefix, self.message
            ),
            None => {
                format!("[{}] {}", timestamp, colored_level).to_string()
                    + &format!(" {}", self.message)
            }
        }
    }

    fn date_dir(&self) -> String {
        self.timestamp.format("%Y-%m-%d").to_string()
    }
}

enum WorkerCommand {
    Record(LogRecord),
    Flush(mpsc::Sender<()>),
    Shutdown,
}

/// 前端 logger
struct Logger {
    config: LogConfig,
    sender: mpsc::Sender<WorkerCommand>,
    worker: Option<JoinHandle<()>>,
}

impl Logger {
    fn new(config: LogConfig) -> Self {
        let (sender, receiver) = mpsc::channel();
        let worker_config = config.clone();
        let worker = thread::spawn(move || {
            let mut sink = RotatingFileSink::new(worker_config);
            worker_loop(receiver, &mut sink);
        });

        Self {
            config,
            sender,
            worker: Some(worker),
        }
    }

    fn log(&self, record: LogRecord) {
        println!("{}", record.render_console(self.config.console_colors()));
        let _ = self.sender.send(WorkerCommand::Record(record));
    }

    fn flush(&self) {
        let (tx, rx) = mpsc::channel();
        if self.sender.send(WorkerCommand::Flush(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(5));
        }
    }

    fn shutdown(&mut self) {
        let _ = self.sender.send(WorkerCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for Logger {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 按日期和大小轮转的文件 sink
struct RotatingFileSink {
    config: LogConfig,
    current_sizes: HashMap<PathBuf, u64>,
}

impl RotatingFileSink {
    fn new(config: LogConfig) -> Self {
        Self {
            config,
            current_sizes: HashMap::new(),
        }
    }

    fn log(&mut self, record: &LogRecord) {
        let rendered = record.render();
        let line_size = rendered.len() as u64 + 1;
        let date_dir = record.date_dir();

        let _ = self.write_to_target(&date_dir, "all", &rendered, line_size);
        let _ = self.write_to_target(&date_dir, record.level.file_stem(), &rendered, line_size);
    }

    fn flush(&mut self) {}

    fn write_to_target(
        &mut self,
        date_dir: &str,
        stem: &str,
        rendered: &str,
        line_size: u64,
    ) -> io::Result<()> {
        let path = self.resolve_path(date_dir, stem, line_size)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(file, "{}", rendered)?;
        let size = self.current_file_size(&path)? + line_size;
        self.current_sizes.insert(path, size);
        Ok(())
    }

    fn resolve_path(&mut self, date_dir: &str, stem: &str, line_size: u64) -> io::Result<PathBuf> {
        let mut index = 0usize;
        loop {
            let path = self.file_path(date_dir, stem, index);
            let size = self.current_file_size(&path)?;
            if size == 0 || size + line_size <= self.config.max_file_size() {
                return Ok(path);
            }
            index += 1;
        }
    }

    fn file_path(&self, date_dir: &str, stem: &str, index: usize) -> PathBuf {
        let file_name = if index == 0 {
            format!("{}.log", stem)
        } else {
            format!("{}-{}.log", stem, index)
        };
        self.config.directory().join(date_dir).join(file_name)
    }

    fn current_file_size(&mut self, path: &Path) -> io::Result<u64> {
        if let Some(size) = self.current_sizes.get(path) {
            return Ok(*size);
        }
        let size = match fs::metadata(path) {
            Ok(metadata) => metadata.len(),
            Err(err) if err.kind() == io::ErrorKind::NotFound => 0,
            Err(err) => return Err(err),
        };
        self.current_sizes.insert(path.to_path_buf(), size);
        Ok(size)
    }
}

fn worker_loop(receiver: mpsc::Receiver<WorkerCommand>, sink: &mut RotatingFileSink) {
    let flush_interval = *sink.config.flush_interval();
    loop {
        match receiver.recv_timeout(flush_interval) {
            Ok(WorkerCommand::Record(record)) => {
                sink.log(&record);
                loop {
                    match receiver.try_recv() {
                        Ok(WorkerCommand::Record(record)) => {
                            sink.log(&record);
                        }
                        Ok(WorkerCommand::Flush(done)) => {
                            sink.flush();
                            let _ = done.send(());
                        }
                        Ok(WorkerCommand::Shutdown) => {
                            sink.flush();
                            return;
                        }
                        Err(mpsc::TryRecvError::Empty) => {
                            break;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => {
                            sink.flush();
                            return;
                        }
                    }
                }
            }
            Ok(WorkerCommand::Flush(done)) => {
                sink.flush();
                let _ = done.send(());
            }
            Ok(WorkerCommand::Shutdown) => {
                sink.flush();
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                sink.flush();
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                sink.flush();
                break;
            }
        }
    }
}

static LOGGER: OnceLock<Mutex<Logger>> = OnceLock::new();

fn global_logger() -> &'static Mutex<Logger> {
    LOGGER.get_or_init(|| Mutex::new(Logger::new(LogConfig::default())))
}

pub fn init(config: LogConfig) {
    let mut guard = global_logger().lock().expect("logger mutex poisoned");
    let mut old = std::mem::replace(&mut *guard, Logger::new(config));
    old.shutdown();
}

pub fn init_from_config_file() {
    init(LogConfig::from_config_file());
}

pub fn flush() {
    let guard = global_logger().lock().expect("logger mutex poisoned");
    guard.flush();
}

fn emit(level: Level, prefix: Option<&str>, message: &str) {
    let record = LogRecord::new(level, prefix.map(ToOwned::to_owned), message.to_string());
    let guard = global_logger().lock().expect("logger mutex poisoned");
    guard.log(record);
}

/// 输出 INFO 级别日志
pub fn info(msg: &str) {
    emit(Level::Info, None, msg);
}

/// 输出 TRACE 级别日志
#[allow(dead_code)]
pub fn trace(msg: &str) {
    emit(Level::Trace, None, msg);
}

/// 输出带前缀的 TRACE 级别日志
#[allow(dead_code)]
pub fn trace_f(prefix: &str, msg: &str) {
    emit(Level::Trace, Some(prefix), msg);
}

/// 输出带前缀的 INFO 级别日志
pub fn info_f(prefix: &str, msg: &str) {
    emit(Level::Info, Some(prefix), msg);
}

/// 输出 SUCCESS 级别日志
pub fn success(msg: &str) {
    emit(Level::Success, None, msg);
}

/// 输出带前缀的 SUCCESS 级别日志
pub fn success_f(prefix: &str, msg: &str) {
    emit(Level::Success, Some(prefix), msg);
}

/// 输出 WARN 级别日志
pub fn warn(msg: &str) {
    emit(Level::Warn, None, msg);
}

/// 输出带前缀的 WARN 级别日志
#[allow(dead_code)]
pub fn warn_f(prefix: &str, msg: &str) {
    emit(Level::Warn, Some(prefix), msg);
}

/// 输出 ERROR 级别日志
pub fn error(msg: &str) {
    emit(Level::Error, None, msg);
}

/// 输出带前缀的 ERROR 级别日志
pub fn error_f(prefix: &str, msg: &str) {
    emit(Level::Error, Some(prefix), msg);
}

/// 输出 FATAL 级别日志
#[allow(dead_code)]
pub fn fatal(msg: &str) {
    emit(Level::Fatal, None, msg);
}

/// 输出带前缀的 FATAL 级别日志
#[allow(dead_code)]
pub fn fatal_f(prefix: &str, msg: &str) {
    emit(Level::Fatal, Some(prefix), msg);
}

/// 输出 DEBUG 级别日志
pub fn debug(msg: &str) {
    emit(Level::Debug, None, msg);
}

/// 输出带前缀的 DEBUG 级别日志
#[allow(dead_code)]
pub fn debug_f(prefix: &str, msg: &str) {
    emit(Level::Debug, Some(prefix), msg);
}

/// 输出 SYSTEM 级别日志
pub fn system(msg: &str) {
    emit(Level::System, None, msg);
}

/// 输出带前缀的 NETWORK 级别日志
#[allow(dead_code)]
pub fn network(prefix: &str, msg: &str) {
    emit(Level::Network, Some(prefix), msg);
}

/// 输出 PROCESSING 级别日志
#[allow(dead_code)]
pub fn processing(msg: &str) {
    emit(Level::Processing, None, msg);
}

/// 输出带前缀的 PROCESSING 级别日志
#[allow(dead_code)]
pub fn processing_f(prefix: &str, msg: &str) {
    emit(Level::Processing, Some(prefix), msg);
}

/// 打印分隔线
pub fn separator() {
    raw("────────────────────────────────────────────────────");
}

/// 打印阶段标题
pub fn phase(title: &str) {
    separator();
    emit(Level::Info, Some("PHASE"), title);
    separator();
}

/// 打印带标签的当前时间
pub fn now_time(label: &str) {
    emit(Level::Info, Some("TIME"), label);
}

/// 打印原始信息
pub fn raw(msg: &str) {
    let record = LogRecord::raw(msg.to_string());
    let guard = global_logger().lock().expect("logger mutex poisoned");
    guard.log(record);
}

#[derive(Debug, Default, Deserialize)]
struct FileLogConfig {
    #[serde(default)]
    log_dir: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    max_file_size: Option<u64>,
    #[serde(default)]
    max_file_size_mb: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct RootLogConfig {
    #[serde(default)]
    log_dir: Option<String>,
    #[serde(default)]
    logging: Option<FileLogConfig>,
    #[serde(default)]
    log: Option<FileLogConfig>,
}

fn load_file_log_config() -> Option<FileLogConfig> {
    let path = discover_config_file_path()?;
    let content = fs::read_to_string(path).ok()?;
    let parsed = parse_root_log_config(&content)?;
    let mut config = parsed.logging.or(parsed.log).unwrap_or_default();
    if config.log_dir.is_none() {
        config.log_dir = parsed.log_dir;
    }
    Some(config)
}

fn discover_config_file_path() -> Option<PathBuf> {
    if let Ok(raw_path) = std::env::var("ANYROUTER_CONFIG_FILE") {
        let trimmed = raw_path.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    [
        DEFAULT_CONFIG_FILE,
        "anyrouter-config.json",
        "anyrouter-config.toml",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())
}

fn parse_root_log_config(content: &str) -> Option<RootLogConfig> {
    serde_json::from_str(content)
        .ok()
        .or_else(|| toml::from_str(content).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn lock_tests() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("test mutex poisoned")
    }

    fn unique_log_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("anyrouter-{}-{}", name, nanos))
    }

    #[test]
    fn writes_all_and_level_logs_under_date_directory() {
        let _guard = lock_tests();
        let dir = unique_log_dir("log-files");
        let mut config = LogConfig::default();
        config.set_directory(dir.clone()).set_console_colors(false);
        init(config);

        info("hello");
        warn("careful");
        flush();

        let date = Local::now().format("%Y-%m-%d").to_string();
        let date_dir = dir.join(date);
        let all = fs::read_to_string(date_dir.join("all.log")).unwrap();
        let info = fs::read_to_string(date_dir.join("info.log")).unwrap();
        let warn = fs::read_to_string(date_dir.join("warn.log")).unwrap();

        assert!(all.contains("hello"));
        assert!(all.contains("careful"));
        assert!(info.contains("hello"));
        assert!(warn.contains("careful"));
    }

    #[test]
    fn rotates_when_file_exceeds_configured_size() {
        let _guard = lock_tests();
        let dir = unique_log_dir("rotation");
        let mut config = LogConfig::default();
        config
            .set_directory(dir.clone())
            .set_max_file_size(120)
            .set_console_colors(false);
        init(config);

        for i in 0..8 {
            info(&format!("message number {} with enough bytes to rotate", i));
        }
        flush();

        let date = Local::now().format("%Y-%m-%d").to_string();
        let date_dir = dir.join(date);
        assert!(date_dir.join("all.log").is_file());
        assert!(date_dir.join("all-1.log").is_file());
        assert!(date_dir.join("info.log").is_file());
        assert!(date_dir.join("info-1.log").is_file());
    }

    #[test]
    fn parses_logging_config_from_toml_content() {
        let parsed = parse_root_log_config(
            r#"
            [logging]
            path = "custom-logs"
            max_file_size_mb = 2
            "#,
        )
        .unwrap();
        let logging = parsed.logging.unwrap();

        assert_eq!(logging.path.as_deref(), Some("custom-logs"));
        assert_eq!(logging.max_file_size_mb, Some(2));
    }

    #[test]
    fn colorizes_only_level_in_console_output() {
        let record = LogRecord {
            level: Level::Warn,
            timestamp: Local::now(),
            prefix: Some("demo".to_string()),
            message: "message".to_string(),
            raw: false,
        };

        let rendered = record.render_console(true);

        assert!(rendered.contains("\x1b[33m[WARN]\x1b[0m"));
        assert!(!rendered.starts_with("\x1b[33m"));
        assert!(rendered.contains(" [demo] message"));
    }
}
