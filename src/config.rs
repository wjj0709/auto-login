//! 旧配置结构定义:仅用于 .env 一次性导入(storage::import_from_strings)
//! 与现有签到通路的适配(storage::load_*_for_ui)。运行时配置已迁移至 SQLite。

use serde::{Deserialize, Serialize};

/// Provider 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(default)]
    pub name: String,
    pub domain: String,
    #[serde(default = "default_login_path")]
    pub login_path: String,
    #[serde(default = "default_sign_in_path")]
    pub sign_in_path: Option<String>,
    #[serde(default = "default_user_info_path")]
    pub user_info_path: String,
    #[serde(default = "default_api_user_key")]
    pub api_user_key: String,
}

fn default_login_path() -> String { "/login".to_string() }
fn default_sign_in_path() -> Option<String> { Some("/api/user/sign_in".to_string()) }
fn default_user_info_path() -> String { "/api/user/self".to_string() }
fn default_api_user_key() -> String { "new-api-user".to_string() }

impl ProviderConfig {
    #[allow(dead_code)]
    pub fn needs_manual_check_in(&self) -> bool {
        self.sign_in_path.is_some()
    }
}

/// 账号配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub cookies: serde_json::Value,
    pub api_user: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    pub name: Option<String>,
}

fn default_provider() -> String { "anyrouter".to_string() }

impl AccountConfig {
    pub fn get_display_name(&self, index: usize) -> String {
        self.name.clone().unwrap_or_else(|| format!("Account {}", index + 1))
    }
}
