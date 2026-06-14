mod app_state;
mod components;
mod theme;
mod views;

use anyrouter_core::config_loader::{load_unified, LoadOptions, SourceKind};
use anyrouter_core::storage::Storage;

fn main() {
    dotenvy::dotenv().ok();

    let db_path = Storage::default_path();
    // meta 永远可读，用于决定启用哪些源（不属于可切换数据源，无循环依赖）
    let sources = read_cfg_sources(&db_path);

    // 按启用源加载并合并配置（含增量回写）；GUI 之后从 SQLite 读取展示
    let _ = load_unified(&LoadOptions {
        sources: sources.clone(),
        db_path: db_path.clone(),
        config_path: None,
    });

    views::root::run_app(db_path, sources);
}

/// 从 SQLite meta.cfg_sources 读取启用源；缺省/打不开 → 默认三源全开。
fn read_cfg_sources(db_path: &std::path::Path) -> Vec<SourceKind> {
    let default = vec![SourceKind::Env, SourceKind::File, SourceKind::Sqlite];
    let Ok(storage) = Storage::open(&db_path.to_path_buf()) else {
        return default;
    };
    match storage.get_meta("cfg_sources") {
        Ok(Some(val)) => {
            let list: Vec<SourceKind> = val.split(',').filter_map(SourceKind::parse).collect();
            if list.is_empty() {
                default
            } else {
                list
            }
        }
        _ => default,
    }
}
