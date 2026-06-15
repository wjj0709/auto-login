// ============================================================================
// config.rs — 配置加载模块
// ============================================================================
// 功能：
// 1. 加载 Provider（站点）配置：内置配置 + 配置文件 + 环境变量覆盖
// 2. 加载账号配置：支持 Cookie 方式或账号密码方式登录
// 3. 支持从配置文件读取 accounts/providers（JSON / TOML）
// ============================================================================

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::log;

const DEFAULT_CONFIG_FILE: &str = "conf.json";

/// Provider（站点）配置结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider 名称（标识符），用于匹配账号配置中的 provider 字段
    #[serde(default)]
    name: String,

    /// 站点域名，含协议前缀（如 "https://anyrouter.top"）
    domain: String,

    /// 登录页路径，浏览器会访问此路径以获取 WAF cookie，默认 "/login"
    #[serde(default = "default_login_path")]
    login_path: String,

    /// 签到 API 路径
    /// Some("/api/user/sign_in") — 需要手动调用签到接口
    /// None — 访问即自动签到（如 agentrouter）
    #[serde(default = "default_sign_in_path")]
    sign_in_path: Option<String>,

    /// 用户信息 API 路径，用于查询余额和用户详情，默认 "/api/user/self"
    #[serde(default = "default_user_info_path")]
    user_info_path: String,

    /// 用户标识请求头的键名，默认 "new-api-user"
    #[serde(default = "default_api_user_key")]
    api_user_key: String,
}

impl_ref_accessors!(ProviderConfig {
    name: String => name, set_name;
    domain: String => domain, set_domain;
    login_path: String => login_path, set_login_path;
    sign_in_path: Option<String> => sign_in_path, set_sign_in_path;
    user_info_path: String => user_info_path, set_user_info_path;
    api_user_key: String => api_user_key, set_api_user_key;
});

fn default_login_path() -> String {
    "/login".to_string()
}
fn default_sign_in_path() -> Option<String> {
    Some("/api/user/sign_in".to_string())
}
fn default_user_info_path() -> String {
    "/api/user/self".to_string()
}
fn default_api_user_key() -> String {
    "new-api-user".to_string()
}

impl ProviderConfig {
    #[allow(dead_code)]
    pub fn needs_manual_check_in(&self) -> bool {
        self.sign_in_path.is_some()
    }
}

/// 应用全局配置
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Provider 名称 → ProviderConfig 的映射
    providers: HashMap<String, ProviderConfig>,
}

impl_ref_accessors!(AppConfig {
    providers: HashMap<String, ProviderConfig> => providers, set_providers;
});

impl AppConfig {
    #[allow(dead_code)]
    pub fn get_provider(&self, name: &str) -> Option<&ProviderConfig> {
        self.providers.get(name)
    }
}

/// 账号配置结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    /// Cookie 信息，用于浏览器身份认证
    /// 支持 JSON 对象、字符串和 null
    #[serde(default = "default_cookies")]
    cookies: Value,

    /// 用户标识值，对应 Provider 配置中的 api_user_key 请求头
    api_user: String,

    /// 所属 Provider 名称，默认 "anyrouter"
    #[serde(default = "default_provider")]
    provider: String,

    /// 账号别名，仅用于日志和通知显示
    #[serde(default)]
    name: Option<String>,

    /// 登录用户名（可选；若无有效 cookie，可回退登录）
    #[serde(default)]
    username: Option<String>,

    /// 登录密码（可选；若无有效 cookie，可回退登录）
    #[serde(default)]
    password: Option<String>,
}

impl_ref_accessors!(AccountConfig {
    cookies: Value => cookies, set_cookies;
    api_user: String => api_user, set_api_user;
    provider: String => provider, set_provider;
    name: Option<String> => name, set_name;
    username: Option<String> => username, set_username;
    password: Option<String> => password, set_password;
});

fn default_provider() -> String {
    "anyrouter".to_string()
}

fn default_cookies() -> Value {
    Value::Object(Map::new())
}

impl AccountConfig {
    pub fn get_display_name(&self, index: usize) -> String {
        self.name
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Account {}", index + 1))
    }

    /// 返回最终生效的用户名。
    /// 优先级：显式字段 username > cookies._username
    pub fn resolved_username(&self) -> Option<String> {
        self.username
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| cookie_meta_field(&self.cookies, "_username"))
    }

    /// 返回最终生效的密码。
    /// 优先级：显式字段 password > cookies._password
    pub fn resolved_password(&self) -> Option<String> {
        self.password
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| cookie_meta_field(&self.cookies, "_password"))
    }

    /// 判断是否显式提供了可用的 cookie。
    /// cookies 对象中的 `_username` / `_password` 不计入 cookie 数量。
    pub fn has_cookie_material(&self) -> bool {
        match &self.cookies {
            Value::Object(map) => map.keys().any(|key| !key.starts_with('_')),
            Value::String(s) => s.split(';').any(|item| item.trim().contains('=')),
            _ => false,
        }
    }

    pub fn cookie_summary(&self) -> String {
        match &self.cookies {
            Value::Object(map) => {
                let keys = map
                    .keys()
                    .filter(|key| !key.starts_with('_'))
                    .cloned()
                    .collect::<Vec<_>>();
                if keys.is_empty() {
                    "no cookies".to_string()
                } else {
                    keys.join(", ")
                }
            }
            Value::String(s) => {
                let count = s
                    .split(';')
                    .filter(|item| item.trim().contains('='))
                    .count();
                if count == 0 {
                    "no cookies".to_string()
                } else {
                    format!("{} cookie(s) from string", count)
                }
            }
            _ => "no cookies".to_string(),
        }
    }
}

/// 一次性加载运行时所需的全部配置
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    app_config: AppConfig,
    accounts: Vec<AccountConfig>,
    config_file_path: Option<PathBuf>,
}

impl_ref_accessors!(RuntimeConfig {
    app_config: AppConfig => app_config, set_app_config;
    accounts: Vec<AccountConfig> => accounts, set_accounts;
    config_file_path: Option<PathBuf> => config_file_path, set_config_file_path;
});

#[derive(Debug, Clone, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    accounts: Vec<AccountConfig>,
}

#[derive(Debug, Clone)]
struct LoadedFileConfig {
    path: PathBuf,
    data: FileConfig,
}

/// 加载运行时配置。
///
/// 配置优先级：
/// 1. 若存在 conf.json（或 ANYROUTER_CONFIG_FILE 指定的文件），优先使用文件配置
/// 2. 若文件不存在，再回退到环境变量
pub fn load_runtime_config() -> Result<RuntimeConfig, String> {
    log::info("Loading provider configurations...");

    let file_config = load_optional_file_config()?;
    if let Some(file_config) = file_config.as_ref() {
        log::info(&format!(
            "Configuration file loaded: {}",
            file_config.path.display()
        ));
    } else {
        log::debug(
            "No configuration file found, using environment variables and built-in providers only",
        );
    }

    let mut providers = built_in_providers();
    log_builtin_providers(&providers);

    if let Some(file_config) = file_config.as_ref() {
        if !file_config.data.providers.is_empty() {
            log::info(&format!(
                "Loading {} provider(s) from configuration file...",
                file_config.data.providers.len()
            ));
            let mut from_file = file_config.data.providers.clone();
            fill_provider_names(&mut from_file);
            log_provider_map("File provider", &from_file);
            providers.extend(from_file);
        }
    }

    if file_config.is_none() {
        match std::env::var("PROVIDERS") {
            Ok(providers_str) => {
                log::info("PROVIDERS environment variable found, parsing custom providers...");
                let custom_providers =
                    parse_provider_map(&providers_str, "PROVIDERS environment variable")?;
                log_provider_map("Custom provider", &custom_providers);
                providers.extend(custom_providers);
                log::success("Custom providers from environment variable have been applied");
            }
            Err(_) => {
                log::debug("PROVIDERS environment variable not set");
            }
        }
    } else {
        log::debug("Configuration file present, PROVIDERS environment variable is ignored");
    }

    log::success(&format!(
        "Provider configuration complete: {} provider(s) available [{}]",
        providers.len(),
        providers.keys().cloned().collect::<Vec<_>>().join(", "),
    ));

    log::info("Loading account configurations...");
    let accounts = if let Some(file_config) = file_config.as_ref() {
        if file_config.data.accounts.is_empty() {
            return Err(format!(
                "No accounts found in configuration file: {}",
                file_config.path.display()
            ));
        }

        log::info(&format!(
            "Using {} account(s) from configuration file {}",
            file_config.data.accounts.len(),
            file_config.path.display()
        ));
        file_config.data.accounts.clone()
    } else {
        let accounts_str = std::env::var("ANYROUTER_ACCOUNTS").map_err(|_| {
            "conf.json not found and ANYROUTER_ACCOUNTS environment variable not found".to_string()
        })?;
        log::debug(&format!(
            "ANYROUTER_ACCOUNTS found, length: {} characters",
            accounts_str.len()
        ));
        parse_accounts(&accounts_str, "ANYROUTER_ACCOUNTS environment variable")?
    };

    validate_accounts(&accounts)?;

    Ok(RuntimeConfig {
        app_config: AppConfig { providers },
        accounts,
        config_file_path: file_config.map(|loaded| loaded.path),
    })
}

fn built_in_providers() -> HashMap<String, ProviderConfig> {
    let mut providers = HashMap::new();

    providers.insert(
        "anyrouter".to_string(),
        ProviderConfig {
            name: "anyrouter".to_string(),
            domain: "https://anyrouter.top".to_string(),
            login_path: "/login".to_string(),
            sign_in_path: Some("/api/user/sign_in".to_string()),
            user_info_path: "/api/user/self".to_string(),
            api_user_key: "new-api-user".to_string(),
        },
    );

    providers.insert(
        "agentrouter".to_string(),
        ProviderConfig {
            name: "agentrouter".to_string(),
            domain: "https://agentrouter.org".to_string(),
            login_path: "/login".to_string(),
            sign_in_path: None,
            user_info_path: "/api/user/self".to_string(),
            api_user_key: "new-api-user".to_string(),
        },
    );

    providers
}

fn log_builtin_providers(providers: &HashMap<String, ProviderConfig>) {
    let mut keys = providers.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    for key in keys {
        if let Some(provider) = providers.get(&key) {
            log::info_f(
                &key,
                &format!(
                    "Built-in provider loaded: domain={}, sign_in_path={}, user_info_path={}, api_user_key={}",
                    provider.domain,
                    provider.sign_in_path.as_deref().unwrap_or("None"),
                    provider.user_info_path,
                    provider.api_user_key,
                ),
            );
        }
    }
}

fn fill_provider_names(providers: &mut HashMap<String, ProviderConfig>) {
    for (key, provider) in providers.iter_mut() {
        if provider.name.trim().is_empty() {
            provider.name = key.clone();
        }
    }
}

fn parse_provider_map(raw: &str, source: &str) -> Result<HashMap<String, ProviderConfig>, String> {
    let mut providers = serde_json::from_str::<HashMap<String, ProviderConfig>>(raw)
        .map_err(|err| format!("Failed to parse {}: {}", source, err))?;
    fill_provider_names(&mut providers);
    Ok(providers)
}

fn log_provider_map(prefix: &str, providers: &HashMap<String, ProviderConfig>) {
    let mut keys = providers.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    for key in keys {
        if let Some(provider) = providers.get(&key) {
            log::info_f(
                &key,
                &format!(
                    "{} loaded: domain={}, sign_in_path={}, user_info_path={}, api_user_key={}",
                    prefix,
                    provider.domain,
                    provider.sign_in_path.as_deref().unwrap_or("None"),
                    provider.user_info_path,
                    provider.api_user_key,
                ),
            );
        }
    }
}

fn parse_accounts(raw: &str, source: &str) -> Result<Vec<AccountConfig>, String> {
    serde_json::from_str::<Vec<AccountConfig>>(raw).map_err(|err| {
        format!(
            "Account configuration JSON parse failed from {}: {}",
            source, err
        )
    })
}

fn validate_accounts(accounts: &[AccountConfig]) -> Result<(), String> {
    for (i, account) in accounts.iter().enumerate() {
        let display_name = account.get_display_name(i);

        if account.api_user.trim().is_empty() {
            return Err(format!(
                "[{}] Missing required field: api_user",
                display_name
            ));
        }

        if account.provider.trim().is_empty() {
            return Err(format!(
                "[{}] Missing required field: provider",
                display_name
            ));
        }

        let username = account.resolved_username();
        let password = account.resolved_password();
        if username.is_some() ^ password.is_some() {
            return Err(format!(
                "[{}] username/password must be provided together",
                display_name
            ));
        }

        if !account.has_cookie_material() && username.is_none() {
            return Err(format!(
                "[{}] Missing authentication material: provide cookies or username/password",
                display_name
            ));
        }

        let auth_mode = match (account.has_cookie_material(), username.is_some()) {
            (true, true) => "cookies + username/password fallback",
            (true, false) => "cookies only",
            (false, true) => "username/password only",
            (false, false) => "invalid",
        };

        log::info_f(
            &display_name,
            &format!(
                "provider=\"{}\", api_user=\"{}...\", auth_mode=\"{}\", cookies=[{}]",
                account.provider,
                &account.api_user[..account.api_user.len().min(8)],
                auth_mode,
                account.cookie_summary(),
            ),
        );
    }

    log::success(&format!(
        "Account configuration complete: {} account(s) validated",
        accounts.len()
    ));
    Ok(())
}

fn cookie_meta_field(cookies: &Value, key: &str) -> Option<String> {
    cookies
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn load_optional_file_config() -> Result<Option<LoadedFileConfig>, String> {
    let Some(path) = discover_config_file_path()? else {
        return Ok(None);
    };

    let content = fs::read_to_string(&path).map_err(|err| {
        format!(
            "Failed to read configuration file {}: {}",
            path.display(),
            err
        )
    })?;
    let data = parse_file_config(&path, &content)?;

    Ok(Some(LoadedFileConfig { path, data }))
}

fn discover_config_file_path() -> Result<Option<PathBuf>, String> {
    if let Ok(raw_path) = std::env::var("ANYROUTER_CONFIG_FILE") {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            return Err("ANYROUTER_CONFIG_FILE is set but empty".to_string());
        }
        return Ok(Some(PathBuf::from(trimmed)));
    }

    for candidate in [
        DEFAULT_CONFIG_FILE,
        "anyrouter-config.json",
        "anyrouter-config.toml",
    ] {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

fn parse_file_config(path: &Path, content: &str) -> Result<FileConfig, String> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("json") => serde_json::from_str(content).map_err(|err| {
            format!(
                "Failed to parse JSON configuration file {}: {}",
                path.display(),
                err
            )
        }),
        Some("toml") => toml::from_str(content).map_err(|err| {
            format!(
                "Failed to parse TOML configuration file {}: {}",
                path.display(),
                err
            )
        }),
        _ => serde_json::from_str(content)
            .or_else(|_| toml::from_str(content))
            .map_err(|err| {
                format!(
                    "Failed to parse configuration file {} as JSON or TOML: {}",
                    path.display(),
                    err
                )
            }),
    }
}

// ============================================================================
// 单元测试
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_json_config_file() {
        let config = parse_file_config(
            Path::new("anyrouter-config.json"),
            r#"
            {
              "providers": {
                "custom": {
                  "domain": "https://example.com",
                  "sign_in_path": "/custom/sign"
                }
              },
              "accounts": [
                {
                  "name": "json-account",
                  "provider": "custom",
                  "api_user": "123456",
                  "username": "alice",
                  "password": "secret"
                }
              ]
            }
            "#,
        )
        .unwrap();

        assert_eq!(config.providers["custom"].domain, "https://example.com");
        assert_eq!(config.accounts.len(), 1);
        assert_eq!(
            config.accounts[0].resolved_username().as_deref(),
            Some("alice")
        );
        assert!(!config.accounts[0].has_cookie_material());
    }

    #[test]
    fn parses_toml_config_file() {
        let config = parse_file_config(
            Path::new("anyrouter-config.toml"),
            r#"
            [providers.custom]
            domain = "https://example.com"
            login_path = "/auth/login"
            sign_in_path = "/api/user/sign_in"

            [[accounts]]
            name = "toml-account"
            provider = "custom"
            api_user = "654321"
            username = "bob"
            password = "secret"
            cookies = { session = "abc123" }
            "#,
        )
        .unwrap();

        assert_eq!(config.providers["custom"].login_path, "/auth/login");
        assert_eq!(config.accounts.len(), 1);
        assert_eq!(config.accounts[0].cookie_summary(), "session");
    }

    #[test]
    fn validates_username_password_only_account() {
        let account = AccountConfig {
            cookies: Value::Null,
            api_user: "148714".to_string(),
            provider: "anyrouter".to_string(),
            name: Some("pwd-only".to_string()),
            username: Some("alice".to_string()),
            password: Some("secret".to_string()),
        };

        assert!(validate_accounts(&[account]).is_ok());
    }

    #[test]
    fn rejects_account_without_auth_material() {
        let account = AccountConfig {
            cookies: Value::Null,
            api_user: "148714".to_string(),
            provider: "anyrouter".to_string(),
            name: Some("invalid".to_string()),
            username: None,
            password: None,
        };

        let err = validate_accounts(&[account]).unwrap_err();
        assert!(err.contains("Missing authentication material"));
    }

    #[test]
    fn resolves_legacy_username_password_from_cookies() {
        let account = AccountConfig {
            cookies: json!({
                "_username": "legacy-user",
                "_password": "legacy-pass"
            }),
            api_user: "148714".to_string(),
            provider: "anyrouter".to_string(),
            name: None,
            username: None,
            password: None,
        };

        assert_eq!(account.resolved_username().as_deref(), Some("legacy-user"));
        assert_eq!(account.resolved_password().as_deref(), Some("legacy-pass"));
        assert!(!account.has_cookie_material());
    }
}
