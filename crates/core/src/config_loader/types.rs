//! 跨源规范化配置类型（不含 SQLite 自增 id，便于跨源去重）。

/// 站点配置（= CLI 的 provider）
#[derive(Debug, Clone, PartialEq)]
pub struct RawSite {
    pub name: String,
    pub domain: String,
    pub login_path: String,
    pub sign_in_path: Option<String>,
    pub user_info_path: String,
    pub tokens_path: String,
    pub logs_path: String,
    pub chart_path: String,
    pub api_user_key: String,
}

impl RawSite {
    /// 用 new-api 系默认路径构造站点，仅需提供 name/domain/sign_in_path。
    pub fn with_defaults(
        name: impl Into<String>,
        domain: impl Into<String>,
        sign_in_path: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            domain: domain.into(),
            login_path: "/login".to_string(),
            sign_in_path,
            user_info_path: "/api/user/self".to_string(),
            tokens_path: "/api/token/".to_string(),
            logs_path: "/api/log/self".to_string(),
            chart_path: "/api/data/self".to_string(),
            api_user_key: "new-api-user".to_string(),
        }
    }
}

/// 账户凭据
#[derive(Debug, Clone, PartialEq)]
pub struct RawAccount {
    pub site_name: String,
    pub api_user: String,
    pub display_name: Option<String>,
    pub cookies: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl RawAccount {
    /// 去重键：(站点名, api_user)
    pub fn key(&self) -> (String, String) {
        (self.site_name.clone(), self.api_user.clone())
    }
}

/// 邮件通知配置（单例）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawEmail {
    pub user: String,
    pub pass: String,
    pub to: String,
    pub sender: String,
    pub smtp_server: String,
}

impl RawEmail {
    pub fn is_empty(&self) -> bool {
        self.user.is_empty() && self.pass.is_empty() && self.to.is_empty()
    }

    /// 用后源的非空字段覆盖 self 的对应字段（字段级合并）。
    pub fn overlay(&mut self, other: &RawEmail) {
        if !other.user.is_empty() {
            self.user = other.user.clone();
        }
        if !other.pass.is_empty() {
            self.pass = other.pass.clone();
        }
        if !other.to.is_empty() {
            self.to = other.to.clone();
        }
        if !other.sender.is_empty() {
            self.sender = other.sender.clone();
        }
        if !other.smtp_server.is_empty() {
            self.smtp_server = other.smtp_server.clone();
        }
    }
}

/// 单源读取结果 / 合并最终结果
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawConfig {
    pub sites: Vec<RawSite>,
    pub accounts: Vec<RawAccount>,
    pub email: Option<RawEmail>,
}

pub type UnifiedConfig = RawConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_overlay_keeps_filled_and_applies_nonempty() {
        let mut base = RawEmail {
            user: "a@x.com".into(),
            ..Default::default()
        };
        let next = RawEmail {
            pass: "secret".into(),
            ..Default::default()
        };
        base.overlay(&next);
        assert_eq!(base.user, "a@x.com"); // 前源保留
        assert_eq!(base.pass, "secret"); // 后源补上
    }

    #[test]
    fn account_key_is_site_and_api_user() {
        let a = RawAccount {
            site_name: "AnyRouter".into(),
            api_user: "151687".into(),
            display_name: None,
            cookies: None,
            username: None,
            password: None,
        };
        assert_eq!(a.key(), ("AnyRouter".to_string(), "151687".to_string()));
    }
}
