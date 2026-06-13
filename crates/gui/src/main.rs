mod app_state;
mod components;
mod theme;
mod views;

use anyrouter_core::env_import;
use anyrouter_core::storage::Storage;

fn main() {
    dotenvy::dotenv().ok();

    let db_path = Storage::default_path();
    let storage = Storage::open(&db_path).expect("failed to open database");
    let _ = env_import::import_env_if_needed(&storage);

    views::root::run_app(storage);
}
