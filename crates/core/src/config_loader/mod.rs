pub mod types;
pub mod source;
pub mod env_source;
pub mod file_source;
pub mod sqlite_source;
pub mod merge;

pub use env_source::EnvSource;
pub use file_source::FileSource;
pub use source::{ConfigSource, SourceKind};
pub use sqlite_source::SqliteSource;
pub use types::{RawAccount, RawConfig, RawEmail, RawSite, UnifiedConfig};

use std::path::PathBuf;

/// 多源加载选项
pub struct LoadOptions {
    /// 启用的源，按读取顺序（靠后优先级更高）。
    pub sources: Vec<SourceKind>,
    /// SQLite 数据库路径。
    pub db_path: PathBuf,
    /// 配置文件路径；None = 用 FileSource::default_path()。
    pub config_path: Option<PathBuf>,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            sources: vec![SourceKind::Env, SourceKind::File, SourceKind::Sqlite],
            db_path: crate::storage::Storage::default_path(),
            config_path: None,
        }
    }
}

/// 按 opts.sources 顺序读取各源 → 合并去重 →（若启用 Sqlite）增量回写 → 返回合并结果。
pub fn load_unified(opts: &LoadOptions) -> UnifiedConfig {
    let mut configs: Vec<RawConfig> = Vec::new();

    for kind in &opts.sources {
        let source: Box<dyn ConfigSource> = match kind {
            SourceKind::Env => Box::new(env_source::EnvSource::new()),
            SourceKind::File => {
                let path = opts
                    .config_path
                    .clone()
                    .unwrap_or_else(file_source::FileSource::default_path);
                Box::new(file_source::FileSource::new(path))
            }
            SourceKind::Sqlite => {
                Box::new(sqlite_source::SqliteSource::new(opts.db_path.clone()))
            }
        };
        match source.load() {
            Ok(cfg) => configs.push(cfg),
            Err(e) => eprintln!("[config_loader] 源 {} 读取失败，跳过: {}", kind.as_str(), e),
        }
    }

    let merged = merge::merge_configs(&configs);

    // 增量回写（仅当 Sqlite 在启用列表）
    if opts.sources.contains(&SourceKind::Sqlite) {
        match crate::storage::Storage::open(&opts.db_path) {
            Ok(storage) => {
                if let Err(e) = merge::writeback_to_sqlite(&storage, &merged) {
                    eprintln!("[config_loader] 增量回写失败: {}", e);
                }
            }
            Err(e) => eprintln!("[config_loader] 打开数据库失败，跳过回写: {}", e),
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_env_only_returns_builtin_sites() {
        // 仅 env 源，不读文件/库；不设 ANYROUTER_ACCOUNTS 时账户为空但有内置站点
        let opts = LoadOptions {
            sources: vec![SourceKind::Env],
            db_path: std::env::temp_dir().join("nonexistent_unified.db"),
            config_path: None,
        };
        let cfg = load_unified(&opts);
        assert!(cfg.sites.iter().any(|s| s.name == "AnyRouter"));
        assert!(cfg.sites.iter().any(|s| s.name == "AgentRouter"));
    }
}
