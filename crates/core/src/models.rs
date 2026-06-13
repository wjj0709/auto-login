use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: i64,
    pub name: String,
    pub domain: String,
    pub login_path: String,
    pub sign_in_path: Option<String>,
    pub user_info_path: String,
    pub tokens_path: String,
    pub logs_path: String,
    pub chart_path: String,
    pub api_user_key: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: i64,
    pub site_id: i64,
    pub name: String,
    pub api_user: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub cookies: Option<String>,
    pub cookie_issued_at: Option<String>,
    pub cookie_expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountCache {
    pub account_id: i64,
    pub kind: CacheKind,
    pub payload_json: String,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheKind {
    Overview,
    Tokens,
    Logs,
    Chart,
}

impl CacheKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Tokens => "tokens",
            Self::Logs => "logs",
            Self::Chart => "chart",
        }
    }
}

/// 站点 + 统计信息（用于 GUI 主页卡片展示）
#[derive(Debug, Clone)]
pub struct SiteWithStats {
    pub site: Site,
    pub account_count: usize,
    pub total_balance: f64,
    pub expired_count: usize,
    pub checkin_today: usize,
}

/// 创建/更新站点时的输入
#[derive(Debug, Clone)]
pub struct SiteInput {
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

/// 创建/更新账户时的输入
#[derive(Debug, Clone)]
pub struct AccountInput {
    pub site_id: i64,
    pub name: String,
    pub api_user: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub cookies: Option<String>,
}
