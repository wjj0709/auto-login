// ============================================================================
// notify.rs — 邮件通知模块
// ============================================================================
// 功能：通过 SMTP 发送邮件通知
// 使用 lettre 库实现异步 SMTP 邮件发送，支持 SSL 加密（465 端口）
// SMTP 服务器可自动检测（根据邮箱域名推断）或通过环境变量手动指定
// ============================================================================

use lettre::{
    message::header::ContentType, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use crate::log;

const DEFAULT_CONFIG_FILE: &str = "conf.json";
const REFERENCE_EMAIL_FILE: &str = "docs/reference/邮件信息.txt";

/// 邮件通知配置
/// 从环境变量加载 SMTP 邮件发送所需的全部信息
pub struct EmailNotifier {
    /// SMTP 登录用户名（通常与邮箱地址相同）
    email_user: String,
    /// SMTP 登录密码或授权码
    email_pass: String,
    /// 通知收件人邮箱地址
    email_to: String,
    /// 邮件发件人地址（显示在邮件的 From 字段）
    email_sender: String,
    /// 自定义 SMTP 服务器地址（可选，不设置时自动检测）
    smtp_server: String,
}

impl_ref_accessors!(EmailNotifier {
    email_user: String => email_user, set_email_user;
    email_pass: String => email_pass, set_email_pass;
    email_to: String => email_to, set_email_to;
    email_sender: String => email_sender, set_email_sender;
    smtp_server: String => smtp_server, set_smtp_server;
});

impl EmailNotifier {
    fn from_parts(
        email_user: String,
        email_pass: String,
        email_to: String,
        email_sender: String,
        smtp_server: String,
    ) -> Self {
        let sender = if email_sender.is_empty() {
            email_user.clone()
        } else {
            email_sender
        };

        Self {
            email_sender: sender,
            email_user,
            email_pass,
            email_to,
            smtp_server,
        }
    }

    /// 从环境变量加载邮件通知配置
    ///
    /// 环境变量：
    /// - EMAIL_USER: SMTP 登录用户名（必填）
    /// - EMAIL_PASS: SMTP 登录密码/授权码（必填）
    /// - EMAIL_TO: 通知收件人（必填）
    /// - EMAIL_SENDER: 邮件发件人（可选，默认为 EMAIL_USER）
    /// - CUSTOM_SMTP_SERVER: 自定义 SMTP 服务器（可选，默认自动检测）
    pub fn from_env() -> Self {
        log::info("Loading email notification configuration...");
        for file_config in [
            load_email_config_from_file(),
            Some(load_email_config_from_env()),
            load_email_config_from_reference_file(),
        ]
        .into_iter()
        .flatten()
        {
            let notifier = EmailNotifier::from(file_config);
            if notifier.is_configured() {
                log_email_config(&notifier);
                return notifier;
            }
        }

        let notifier = Self::from_parts(
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        );
        log_email_config(&notifier);
        notifier
    }

    /// 检查邮件通知是否已正确配置
    ///
    /// 必须同时设置 EMAIL_USER、EMAIL_PASS、EMAIL_TO 三个环境变量
    /// 否则邮件通知功能将被跳过
    pub fn is_configured(&self) -> bool {
        !self.email_user.is_empty() && !self.email_pass.is_empty() && !self.email_to.is_empty()
    }

    /// 发送邮件
    ///
    /// 通过 SMTP SSL（465 端口）发送纯文本邮件
    /// SMTP 服务器自动检测逻辑：根据发件人邮箱域名推断
    ///   - xxx@gmail.com → smtp.gmail.com
    ///   - xxx@qq.com → smtp.qq.com
    ///   - 其他同理
    ///
    /// # 参数
    /// - title: 邮件主题
    /// - content: 邮件正文（纯文本）
    ///
    /// # 返回
    /// - Ok(()): 发送成功
    /// - Err(String): 发送失败的错误信息
    pub async fn send(&self, title: &str, content: &str) -> Result<(), String> {
        if !self.is_configured() {
            return Err("Email configuration not set".to_string());
        }

        log::info_f(
            "Email",
            &format!(
                "Building email: subject=\"{}\", body length={} chars",
                title,
                content.len()
            ),
        );

        // 构建邮件消息
        let email = Message::builder()
            // 发件人地址，格式为 "显示名 <邮箱>"
            .from(
                format!("AnyRouter Assistant <{}>", self.email_sender)
                    .parse()
                    .map_err(|e| format!("Invalid sender: {}", e))?,
            )
            // 收件人地址
            .to(self
                .email_to
                .parse()
                .map_err(|e| format!("Invalid recipient: {}", e))?)
            // 邮件主题
            .subject(title)
            // 内容类型：纯文本
            .header(ContentType::TEXT_PLAIN)
            // 邮件正文
            .body(content.to_string())
            .map_err(|e| format!("Failed to build email: {}", e))?;

        // 确定 SMTP 服务器地址
        // 如果未设置自定义 SMTP 服务器，则根据发件人邮箱域名自动推断
        let smtp_server = if self.smtp_server.is_empty() {
            // 提取邮箱域名（@ 后面的部分），拼接 "smtp." 前缀
            let domain = self
                .email_user
                .split('@')
                .last()
                .unwrap_or("smtp.gmail.com");
            let auto = format!("smtp.{}", domain);
            log::info_f("Email", &format!("Auto-detected SMTP server: {}", auto));
            auto
        } else {
            self.smtp_server.clone()
        };

        // 构建 SMTP 认证凭据
        let creds = Credentials::new(self.email_user.clone(), self.email_pass.clone());

        log::info_f(
            "Email",
            &format!("Connecting to SMTP server: {}:465 (SSL)...", smtp_server),
        );
        let start = Instant::now();

        // 构建 SMTP 传输客户端
        // 使用 SSL 加密连接（465 端口，即 SMTPS）
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_server)
            .map_err(|e| format!("Failed to create SMTP transport: {}", e))?
            .port(465) // SMTPS 端口
            .credentials(creds) // 设置认证凭据
            .build();

        // 发送邮件
        mailer
            .send(email)
            .await
            .map_err(|e| format!("Failed to send email after {:.1?}: {}", start.elapsed(), e))?;

        log::info_f(
            "Email",
            &format!("Email sent successfully in {:.1?}", start.elapsed()),
        );
        Ok(())
    }
}

impl From<EmailFileConfig> for EmailNotifier {
    fn from(config: EmailFileConfig) -> Self {
        Self::from_parts(
            config.user.unwrap_or_default(),
            config.pass.unwrap_or_default(),
            config.to.unwrap_or_default(),
            config.sender.unwrap_or_default(),
            config.smtp_server.unwrap_or_default(),
        )
    }
}

/// 通知管理器
/// 统一管理所有通知渠道（目前仅邮件），方便后续扩展（如推送通知、Webhook 等）
pub struct NotificationKit {
    /// 邮件通知器
    email: EmailNotifier,
}

impl_ref_accessors!(NotificationKit {
    email: EmailNotifier => email, set_email;
});

impl NotificationKit {
    /// 从环境变量初始化通知系统
    pub fn from_env() -> Self {
        log::info("Initializing notification system...");
        Self {
            email: EmailNotifier::from_env(),
        }
    }

    /// 推送消息到所有已配置的通知渠道
    ///
    /// 目前仅支持邮件通知，后续可扩展：
    /// - Bark 推送
    /// - 企业微信 Webhook
    /// - Telegram Bot
    /// - Server Chan 等
    ///
    /// # 参数
    /// - title: 通知标题
    /// - content: 通知内容
    pub async fn push_message(&self, title: &str, content: &str) {
        log::info(&format!(
            "Pushing notification: title=\"{}\", content length={} chars",
            title,
            content.len()
        ));

        // 邮件通知渠道
        if self.email().is_configured() {
            match self.email().send(title, content).await {
                Ok(_) => log::success_f("Email", "Message push successful!"),
                Err(e) => log::error_f("Email", &format!("Message push failed: {}", e)),
            }
        } else {
            // 邮件未配置，跳过
            log::debug("Email notification skipped (not configured)");
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct EmailFileConfig {
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    pass: Option<String>,
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    sender: Option<String>,
    #[serde(default)]
    smtp_server: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RootEmailConfig {
    #[serde(default)]
    email: Option<EmailFileConfig>,
}

fn log_email_config(notifier: &EmailNotifier) {
    if notifier.is_configured() {
        log::info_f(
            "Email",
            &format!(
                "Configuration loaded: from={}, to={}, smtp_server={}",
                if notifier.email_sender.is_empty() {
                    &notifier.email_user
                } else {
                    &notifier.email_sender
                },
                &notifier.email_to,
                if notifier.smtp_server.is_empty() {
                    "(auto-detect)"
                } else {
                    &notifier.smtp_server
                },
            ),
        );
    } else {
        log::info("Email notification not configured");
    }
}

fn load_email_config_from_file() -> Option<EmailFileConfig> {
    let path = discover_config_file_path()?;
    let content = fs::read_to_string(path).ok()?;
    let parsed = parse_root_email_config(&content)?;
    parsed.email
}

fn load_email_config_from_env() -> EmailFileConfig {
    EmailFileConfig {
        user: std::env::var("EMAIL_USER").ok(),
        pass: std::env::var("EMAIL_PASS").ok(),
        to: std::env::var("EMAIL_TO").ok(),
        sender: std::env::var("EMAIL_SENDER").ok(),
        smtp_server: std::env::var("CUSTOM_SMTP_SERVER").ok(),
    }
}

fn load_email_config_from_reference_file() -> Option<EmailFileConfig> {
    let content = fs::read_to_string(REFERENCE_EMAIL_FILE).ok()?;
    parse_reference_email_config(&content)
}

fn parse_root_email_config(content: &str) -> Option<RootEmailConfig> {
    serde_json::from_str(content)
        .ok()
        .or_else(|| toml::from_str(content).ok())
}

fn parse_reference_email_config(content: &str) -> Option<EmailFileConfig> {
    let wrapped = format!("{{{}}}", content.trim().trim_start_matches('\u{feff}'));
    let values = serde_json::from_str::<HashMap<String, String>>(&wrapped).ok()?;

    let config = EmailFileConfig {
        user: lookup_trimmed(&values, &["发送方邮箱", "email_user", "user"]),
        pass: lookup_trimmed(&values, &["16位授权码", "email_pass", "pass"]),
        to: lookup_trimmed(&values, &["接收邮件的邮箱", "email_to", "to"]),
        sender: lookup_trimmed(&values, &["sender", "发件人"]),
        smtp_server: lookup_trimmed(
            &values,
            &["邮件服务器地址", "custom_smtp_server", "smtp_server"],
        ),
    };

    if config.user.is_some()
        || config.pass.is_some()
        || config.to.is_some()
        || config.sender.is_some()
        || config.smtp_server.is_some()
    {
        Some(config)
    } else {
        None
    }
}

fn lookup_trimmed(values: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| values.get(*key).map(|value| value.trim().to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_email_config_from_json() {
        let parsed = parse_root_email_config(
            r#"{
                "email": {
                    "user": "user@example.com",
                    "pass": "secret",
                    "to": "to@example.com",
                    "sender": "sender@example.com",
                    "smtp_server": "smtp.example.com"
                }
            }"#,
        )
        .unwrap();

        let email = parsed.email.unwrap();
        assert_eq!(email.user.as_deref(), Some("user@example.com"));
        assert_eq!(email.smtp_server.as_deref(), Some("smtp.example.com"));
    }

    #[test]
    fn parses_email_config_from_toml() {
        let parsed = parse_root_email_config(
            r#"
            [email]
            user = "user@example.com"
            pass = "secret"
            to = "to@example.com"
            "#,
        )
        .unwrap();

        let email = parsed.email.unwrap();
        assert_eq!(email.user.as_deref(), Some("user@example.com"));
        assert_eq!(email.to.as_deref(), Some("to@example.com"));
    }

    #[test]
    fn parses_reference_email_config_with_chinese_labels() {
        let parsed = parse_reference_email_config(
            r#"
            "发送方邮箱": "sender@qq.com",
            "16位授权码": "abcdefghijklmnop",
            "接收邮件的邮箱": "receiver@qq.com",
            "sender": "",
            "邮件服务器地址": "smtp.qq.com"
            "#,
        )
        .unwrap();

        assert_eq!(parsed.user.as_deref(), Some("sender@qq.com"));
        assert_eq!(parsed.pass.as_deref(), Some("abcdefghijklmnop"));
        assert_eq!(parsed.to.as_deref(), Some("receiver@qq.com"));
        assert_eq!(parsed.sender.as_deref(), Some(""));
        assert_eq!(parsed.smtp_server.as_deref(), Some("smtp.qq.com"));
    }
}
