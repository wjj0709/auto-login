use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::source::{ConfigSource, SourceKind};
use super::types::{RawAccount, RawConfig, RawEmail, RawSite};

/// 配置文件中的站点（字段可缺省）
#[derive(Debug, Deserialize)]
struct FileSite {
    name: String,
    domain: String,
    #[serde(default = "default_login_path")]
    login_path: String,
    #[serde(default)]
    sign_in_path: Option<String>,
    #[serde(default = "default_user_info_path")]
    user_info_path: String,
    #[serde(default = "default_tokens_path")]
    tokens_path: String,
    #[serde(default = "default_logs_path")]
    logs_path: String,
    #[serde(default = "default_chart_path")]
    chart_path: String,
    #[serde(default = "default_api_user_key")]
    api_user_key: String,
}

fn default_login_path() -> String {
    "/login".into()
}
fn default_user_info_path() -> String {
    "/api/user/self".into()
}
fn default_tokens_path() -> String {
    "/api/token/".into()
}
fn default_logs_path() -> String {
    "/api/log/self".into()
}
fn default_chart_path() -> String {
    "/api/data/self".into()
}
fn default_api_user_key() -> String {
    "new-api-user".into()
}

#[derive(Debug, Deserialize)]
struct FileAccount {
    site_name: String,
    api_user: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    cookies: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct FileEmail {
    #[serde(default)]
    user: String,
    #[serde(default)]
    pass: String,
    #[serde(default)]
    to: String,
    #[serde(default)]
    sender: String,
    #[serde(default)]
    smtp_server: String,
}

#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    #[serde(default)]
    sites: Vec<FileSite>,
    #[serde(default)]
    accounts: Vec<FileAccount>,
    #[serde(default)]
    email: Option<FileEmail>,
}

pub struct FileSource {
    path: PathBuf,
}

impl FileSource {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// 默认路径：ANYROUTER_CONFIG 优先，否则 SQLite 同目录下 config.json。
    pub fn default_path() -> PathBuf {
        if let Ok(p) = std::env::var("ANYROUTER_CONFIG") {
            if !p.trim().is_empty() {
                return PathBuf::from(p);
            }
        }
        crate::storage::Storage::default_path()
            .parent()
            .map(|d| d.join("config.json"))
            .unwrap_or_else(|| PathBuf::from("config.json"))
    }
}

impl ConfigSource for FileSource {
    fn kind(&self) -> SourceKind {
        SourceKind::File
    }

    fn load(&self) -> Result<RawConfig> {
        if !self.path.exists() {
            // 文件不存在视为空源，不报错
            return Ok(RawConfig::default());
        }
        let text = std::fs::read_to_string(&self.path)
            .with_context(|| format!("读取配置文件失败: {}", self.path.display()))?;
        let parsed: FileConfig = serde_json::from_str(&text)
            .with_context(|| format!("配置文件 JSON 解析失败: {}", self.path.display()))?;

        let sites = parsed
            .sites
            .into_iter()
            .map(|s| RawSite {
                name: s.name,
                domain: s.domain,
                login_path: s.login_path,
                sign_in_path: s.sign_in_path,
                user_info_path: s.user_info_path,
                tokens_path: s.tokens_path,
                logs_path: s.logs_path,
                chart_path: s.chart_path,
                api_user_key: s.api_user_key,
            })
            .collect();

        let accounts = parsed
            .accounts
            .into_iter()
            .filter(|a| !a.api_user.is_empty())
            .map(|a| RawAccount {
                site_name: a.site_name,
                api_user: a.api_user,
                display_name: a.name,
                cookies: a.cookies,
                username: a.username,
                password: a.password,
            })
            .collect();

        let email = parsed
            .email
            .map(|e| RawEmail {
                user: e.user,
                pass: e.pass,
                to: e.to,
                sender: e.sender,
                smtp_server: e.smtp_server,
            })
            .filter(|e| !e.is_empty());

        Ok(RawConfig {
            sites,
            accounts,
            email,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn missing_file_is_empty_source() {
        let src = FileSource::new(PathBuf::from("/no/such/file_xyz.json"));
        let cfg = src.load().unwrap();
        assert!(cfg.sites.is_empty() && cfg.accounts.is_empty());
    }

    #[test]
    fn parses_valid_json_with_defaults() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"{{"sites":[{{"name":"S","domain":"https://s.com"}}],"accounts":[{{"site_name":"S","api_user":"1","name":"n"}}]}}"#
        )
        .unwrap();
        let src = FileSource::new(f.path().to_path_buf());
        let cfg = src.load().unwrap();
        assert_eq!(cfg.sites.len(), 1);
        assert_eq!(cfg.sites[0].api_user_key, "new-api-user"); // 默认值
        assert_eq!(cfg.accounts[0].site_name, "S");
    }

    #[test]
    fn invalid_json_returns_err() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "{{ not json").unwrap();
        let src = FileSource::new(f.path().to_path_buf());
        assert!(src.load().is_err());
    }
}
