use anyhow::Result;
use serde_json::Value;

use super::source::{ConfigSource, SourceKind};
use super::types::{RawAccount, RawConfig, RawEmail, RawSite};

pub struct EnvSource;

impl EnvSource {
    pub fn new() -> Self {
        Self
    }

    /// 内置站点种子：AnyRouter（手动签到）+ AgentRouter（自动签到）。
    fn builtin_sites() -> Vec<RawSite> {
        vec![
            RawSite::with_defaults(
                "AnyRouter",
                "https://anyrouter.top",
                Some("/api/user/sign_in".to_string()),
            ),
            RawSite::with_defaults("AgentRouter", "https://agentrouter.org", None),
        ]
    }

    /// 解析 PROVIDERS 环境变量（JSON 对象：{ key: {domain, sign_in_path?, ...} }）。
    fn parse_providers(raw: &str, sites: &mut Vec<RawSite>) {
        let Ok(map) = serde_json::from_str::<serde_json::Map<String, Value>>(raw) else {
            return;
        };
        for (key, v) in map {
            let name = v
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or(&key)
                .to_string();
            let domain = v
                .get("domain")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            if domain.is_empty() {
                continue;
            }
            let sign_in_path = v
                .get("sign_in_path")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            let mut site = RawSite::with_defaults(name.clone(), domain, sign_in_path);
            if let Some(s) = v.get("login_path").and_then(|x| x.as_str()) {
                site.login_path = s.to_string();
            }
            if let Some(s) = v.get("user_info_path").and_then(|x| x.as_str()) {
                site.user_info_path = s.to_string();
            }
            if let Some(s) = v.get("tokens_path").and_then(|x| x.as_str()) {
                site.tokens_path = s.to_string();
            }
            if let Some(s) = v.get("logs_path").and_then(|x| x.as_str()) {
                site.logs_path = s.to_string();
            }
            if let Some(s) = v.get("chart_path").and_then(|x| x.as_str()) {
                site.chart_path = s.to_string();
            }
            if let Some(s) = v.get("api_user_key").and_then(|x| x.as_str()) {
                site.api_user_key = s.to_string();
            }
            // 同名覆盖内置种子
            sites.retain(|x| x.name != site.name);
            sites.push(site);
        }
    }

    /// 解析 ANYROUTER_ACCOUNTS 环境变量（JSON 数组）。
    fn parse_accounts(raw: &str) -> Vec<RawAccount> {
        let Ok(arr) = serde_json::from_str::<Vec<Value>>(raw) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in arr {
            let api_user = entry
                .get("api_user")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if api_user.is_empty() {
                continue;
            }
            let provider = entry
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("anyrouter");
            // provider 名归一化到内置站点显示名
            let site_name = match provider.to_lowercase().as_str() {
                "agentrouter" => "AgentRouter".to_string(),
                "anyrouter" => "AnyRouter".to_string(),
                other => other.to_string(),
            };
            let display_name = entry
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let username = entry
                .get("_username")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let password = entry
                .get("_password")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let cookies = entry.get("cookies").and_then(|v| {
                if v.is_null() {
                    return None;
                }
                let s = serde_json::to_string(v).ok()?;
                if s == "\"\"" || s == "[]" || s == "null" || s == "{}" {
                    None
                } else {
                    Some(s)
                }
            });
            out.push(RawAccount {
                site_name,
                api_user,
                display_name,
                cookies,
                username,
                password,
            });
        }
        out
    }

    fn parse_email() -> Option<RawEmail> {
        let email = RawEmail {
            user: std::env::var("EMAIL_USER").unwrap_or_default(),
            pass: std::env::var("EMAIL_PASS").unwrap_or_default(),
            to: std::env::var("EMAIL_TO").unwrap_or_default(),
            sender: std::env::var("EMAIL_SENDER").unwrap_or_default(),
            smtp_server: std::env::var("CUSTOM_SMTP_SERVER").unwrap_or_default(),
        };
        if email.is_empty() {
            None
        } else {
            Some(email)
        }
    }
}

impl ConfigSource for EnvSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Env
    }

    fn load(&self) -> Result<RawConfig> {
        let mut sites = Self::builtin_sites();
        if let Ok(raw) = std::env::var("PROVIDERS") {
            if !raw.trim().is_empty() {
                Self::parse_providers(&raw, &mut sites);
            }
        }
        let accounts = std::env::var("ANYROUTER_ACCOUNTS")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(|s| Self::parse_accounts(&s))
            .unwrap_or_default();
        Ok(RawConfig {
            sites,
            accounts,
            email: Self::parse_email(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accounts_maps_provider_and_underscore_fields() {
        let raw = r#"[{"api_user":"151687","provider":"agentrouter","name":"教育邮箱","_username":"u","_password":"p"}]"#;
        let accts = EnvSource::parse_accounts(raw);
        assert_eq!(accts.len(), 1);
        assert_eq!(accts[0].site_name, "AgentRouter");
        assert_eq!(accts[0].api_user, "151687");
        assert_eq!(accts[0].display_name.as_deref(), Some("教育邮箱"));
        assert_eq!(accts[0].username.as_deref(), Some("u"));
        assert_eq!(accts[0].password.as_deref(), Some("p"));
    }

    #[test]
    fn parse_accounts_skips_empty_api_user() {
        let raw = r#"[{"api_user":"","name":"x"}]"#;
        assert_eq!(EnvSource::parse_accounts(raw).len(), 0);
    }

    #[test]
    fn builtin_sites_have_correct_schema() {
        let sites = EnvSource::builtin_sites();
        let any = sites.iter().find(|s| s.name == "AnyRouter").unwrap();
        assert_eq!(any.api_user_key, "new-api-user");
        assert_eq!(any.user_info_path, "/api/user/self");
        assert_eq!(any.sign_in_path.as_deref(), Some("/api/user/sign_in"));
        let agent = sites.iter().find(|s| s.name == "AgentRouter").unwrap();
        assert_eq!(agent.sign_in_path, None);
    }
}
