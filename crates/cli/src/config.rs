use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::log;

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

/// 应用配置
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub providers: HashMap<String, ProviderConfig>,
}

impl AppConfig {
    /// 从环境变量加载配置
    pub fn load_from_env() -> Self {
        log::info("Loading provider configurations...");
        let mut providers = HashMap::new();

        // 内置 anyrouter provider
        let anyrouter = ProviderConfig {
            name: "anyrouter".to_string(),
            domain: "https://anyrouter.top".to_string(),
            login_path: "/login".to_string(),
            sign_in_path: Some("/api/user/sign_in".to_string()),
            user_info_path: "/api/user/self".to_string(),
            api_user_key: "new-api-user".to_string(),
        };
        log::info_f("anyrouter", &format!("Built-in provider loaded: domain={}, sign_in_path={}, user_info_path={}, api_user_key={}",
            anyrouter.domain,
            anyrouter.sign_in_path.as_deref().unwrap_or("None"),
            anyrouter.user_info_path,
            anyrouter.api_user_key,
        ));
        providers.insert("anyrouter".to_string(), anyrouter);

        // 内置 agentrouter provider
        let agentrouter = ProviderConfig {
            name: "agentrouter".to_string(),
            domain: "https://agentrouter.org".to_string(),
            login_path: "/login".to_string(),
            sign_in_path: None,
            user_info_path: "/api/user/self".to_string(),
            api_user_key: "new-api-user".to_string(),
        };
        log::info_f("agentrouter", &format!("Built-in provider loaded: domain={}, sign_in_path={} (auto check-in), user_info_path={}, api_user_key={}",
            agentrouter.domain,
            agentrouter.sign_in_path.as_deref().unwrap_or("None"),
            agentrouter.user_info_path,
            agentrouter.api_user_key,
        ));
        providers.insert("agentrouter".to_string(), agentrouter);

        // 尝试从环境变量加载自定义 providers
        match std::env::var("PROVIDERS") {
            Ok(providers_str) => {
                log::info("PROVIDERS environment variable found, parsing custom providers...");
                match serde_json::from_str::<HashMap<String, ProviderConfig>>(&providers_str) {
                    Ok(mut custom_providers) => {
                        for (key, provider) in custom_providers.iter_mut() {
                            if provider.name.is_empty() {
                                provider.name = key.clone();
                            }
                            log::info_f(key, &format!("Custom provider loaded: domain={}, sign_in_path={}, user_info_path={}, api_user_key={}",
                                provider.domain,
                                provider.sign_in_path.as_deref().unwrap_or("None"),
                                provider.user_info_path,
                                provider.api_user_key,
                            ));
                        }
                        log::success(&format!("Loaded {} custom provider(s) from PROVIDERS environment variable", custom_providers.len()));
                        providers.extend(custom_providers);
                    }
                    Err(e) => {
                        log::warn(&format!("Failed to parse PROVIDERS environment variable: {}, using default configuration only", e));
                    }
                }
            }
            Err(_) => {
                log::debug("PROVIDERS environment variable not set, using built-in providers only");
            }
        }

        log::success(&format!("Provider configuration complete: {} provider(s) available [{}]",
            providers.len(),
            providers.keys().cloned().collect::<Vec<_>>().join(", "),
        ));
        Self { providers }
    }

    #[allow(dead_code)]
    pub fn get_provider(&self, name: &str) -> Option<&ProviderConfig> {
        self.providers.get(name)
    }
}

/// 账号配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    #[serde(default = "default_cookies")]
    pub cookies: serde_json::Value,
    pub api_user: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    pub name: Option<String>,
    #[serde(default)]
    pub _username: Option<String>,
    #[serde(default)]
    pub _password: Option<String>,
}

fn default_provider() -> String { "anyrouter".to_string() }
fn default_cookies() -> serde_json::Value { serde_json::Value::Object(serde_json::Map::new()) }

impl AccountConfig {
    pub fn get_display_name(&self, index: usize) -> String {
        self.name.clone().unwrap_or_else(|| format!("Account {}", index + 1))
    }
}

/// 从环境变量加载账号配置
pub fn load_accounts_config() -> Option<Vec<AccountConfig>> {
    log::info("Loading account configurations from ANYROUTER_ACCOUNTS...");

    let accounts_str = match std::env::var("ANYROUTER_ACCOUNTS") {
        Ok(s) => {
            log::debug(&format!("ANYROUTER_ACCOUNTS found, length: {} characters", s.len()));
            s
        }
        Err(_) => {
            log::error("ANYROUTER_ACCOUNTS environment variable not found");
            return None;
        }
    };

    let accounts_data: Vec<AccountConfig> = match serde_json::from_str::<Vec<AccountConfig>>(&accounts_str) {
        Ok(data) => {
            log::info(&format!("JSON parsing successful, found {} account(s)", data.len()));
            data
        }
        Err(e) => {
            log::error(&format!("Account configuration JSON parse failed: {}", e));
            return None;
        }
    };

    // 验证必填字段并打印摘要
    for (i, account) in accounts_data.iter().enumerate() {
        let display_name = account.get_display_name(i);
        if account.api_user.is_empty() {
            log::error_f(&display_name, "Missing required field: api_user");
            return None;
        }

        // 打印账号摘要（隐藏敏感信息）
        let cookie_keys = match &account.cookies {
            serde_json::Value::Object(map) => {
                map.keys().cloned().collect::<Vec<_>>().join(", ")
            }
            serde_json::Value::String(s) => {
                let count = s.split(';').filter(|c| c.contains('=')).count();
                format!("{} cookie(s) from string", count)
            }
            _ => "unknown format".to_string(),
        };
        log::info_f(&display_name, &format!(
            "provider=\"{}\", api_user=\"{}...\", cookies=[{}]",
            account.provider,
            &account.api_user[..account.api_user.len().min(8)],
            cookie_keys,
        ));
    }

    log::success(&format!("Account configuration complete: {} account(s) validated", accounts_data.len()));
    Some(accounts_data)
}
