//! 邮件 / 推送通知子系统（EmailNotifier、NotificationKit）。
//! 尚未接入 GPUI 流程，暂以 `#![allow(dead_code)]` 整体保留，待后续集成。
#![allow(dead_code)]

use std::time::Instant;
use lettre::{
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

use crate::log;

/// 邮件通知配置
pub struct EmailNotifier {
    pub email_user: String,
    pub email_pass: String,
    pub email_to: String,
    pub email_sender: String,
    pub smtp_server: String,
}

impl EmailNotifier {
    /// 从环境变量加载配置
    pub fn from_env() -> Self {
        log::info("Loading email notification configuration...");
        let email_user = std::env::var("EMAIL_USER").unwrap_or_default();
        let email_pass = std::env::var("EMAIL_PASS").unwrap_or_default();
        let email_to = std::env::var("EMAIL_TO").unwrap_or_default();
        let email_sender = std::env::var("EMAIL_SENDER").unwrap_or_default();
        let smtp_server = std::env::var("CUSTOM_SMTP_SERVER").unwrap_or_default();

        let configured = !email_user.is_empty() && !email_pass.is_empty() && !email_to.is_empty();
        let sender = if email_sender.is_empty() { email_user.clone() } else { email_sender.clone() };

        if configured {
            log::info_f("Email", &format!("Configuration loaded: from={}, to={}, smtp_server={}",
                if email_sender.is_empty() { &email_user } else { &email_sender },
                &email_to,
                if smtp_server.is_empty() { "(auto-detect)" } else { &smtp_server },
            ));
        } else {
            log::info("Email notification not configured (missing EMAIL_USER, EMAIL_PASS, or EMAIL_TO)");
        }

        Self {
            email_sender: sender,
            email_user,
            email_pass,
            email_to,
            smtp_server,
        }
    }

    /// 检查配置是否有效
    pub fn is_configured(&self) -> bool {
        !self.email_user.is_empty() && !self.email_pass.is_empty() && !self.email_to.is_empty()
    }

    /// 发送邮件
    pub async fn send(&self, title: &str, content: &str) -> Result<(), String> {
        if !self.is_configured() {
            return Err("Email configuration not set".to_string());
        }

        log::info_f("Email", &format!("Building email: subject=\"{}\", body length={} chars", title, content.len()));

        let email = Message::builder()
            .from(format!("AnyRouter Assistant <{}>", self.email_sender).parse().map_err(|e| format!("Invalid sender: {}", e))?)
            .to(self.email_to.parse().map_err(|e| format!("Invalid recipient: {}", e))?)
            .subject(title)
            .header(ContentType::TEXT_PLAIN)
            .body(content.to_string())
            .map_err(|e| format!("Failed to build email: {}", e))?;

        let smtp_server = if self.smtp_server.is_empty() {
            let domain = self.email_user.split('@').last().unwrap_or("smtp.gmail.com");
            let auto = format!("smtp.{}", domain);
            log::info_f("Email", &format!("Auto-detected SMTP server: {}", auto));
            auto
        } else {
            self.smtp_server.clone()
        };

        let creds = Credentials::new(self.email_user.clone(), self.email_pass.clone());

        log::info_f("Email", &format!("Connecting to SMTP server: {}:465 (SSL)...", smtp_server));
        let start = Instant::now();

        let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_server)
            .map_err(|e| format!("Failed to create SMTP transport: {}", e))?
            .port(465)
            .credentials(creds)
            .build();

        mailer.send(email).await.map_err(|e| format!("Failed to send email after {:.1?}: {}", start.elapsed(), e))?;

        log::info_f("Email", &format!("Email sent successfully in {:.1?}", start.elapsed()));
        Ok(())
    }
}

/// 通知管理器
pub struct NotificationKit {
    pub email: EmailNotifier,
}

impl NotificationKit {
    pub fn from_env() -> Self {
        log::info("Initializing notification system...");
        Self {
            email: EmailNotifier::from_env(),
        }
    }

    /// 推送消息（尝试所有已配置的通知渠道）
    pub async fn push_message(&self, title: &str, content: &str) {
        log::info(&format!("Pushing notification: title=\"{}\", content length={} chars", title, content.len()));

        // 邮件
        if self.email.is_configured() {
            match self.email.send(title, content).await {
                Ok(_) => log::success_f("Email", "Message push successful!"),
                Err(e) => log::error_f("Email", &format!("Message push failed: {}", e)),
            }
        } else {
            log::debug("Email notification skipped (not configured)");
        }
    }
}
