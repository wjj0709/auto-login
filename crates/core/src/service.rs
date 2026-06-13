use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json;

use crate::models::{Account, CacheKind, Site};
use crate::playwright::{
    PlaywrightAccountInput, PlaywrightAction, PlaywrightPayload, PlaywrightResult, run_playwright,
};
use crate::storage::Storage;

/// 签到结果
pub struct CheckinResult {
    pub account_name: String,
    pub success: bool,
    pub balance_before: Option<f64>,
    pub balance_after: Option<f64>,
    pub error: Option<String>,
}

/// 默认超时时间（毫秒）
const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// 将账户和站点信息组装为 PlaywrightAccountInput
fn build_playwright_input(site: &Site, account: &Account) -> PlaywrightAccountInput {
    let cookies: serde_json::Value = account
        .cookies
        .as_ref()
        .and_then(|c| serde_json::from_str(c).ok())
        .unwrap_or(serde_json::Value::Array(vec![]));

    PlaywrightAccountInput {
        name: account.name.clone(),
        provider: site.name.clone(),
        domain: site.domain.clone(),
        login_path: site.login_path.clone(),
        sign_in_path: site.sign_in_path.clone(),
        user_info_path: site.user_info_path.clone(),
        tokens_path: Some(site.tokens_path.clone()),
        logs_path: Some(site.logs_path.clone()),
        chart_path: Some(site.chart_path.clone()),
        api_user_key: site.api_user_key.clone(),
        api_user: account.api_user.clone(),
        cookies,
        username: account.username.clone(),
        password: account.password.clone(),
    }
}

/// 从 cookies 数组中提取 session cookie 的 expires 字段，转换为 RFC3339 字符串
fn extract_session_expires(cookies: &[crate::playwright::CookieInfo]) -> Option<String> {
    cookies
        .iter()
        .find(|c| c.name == "session")
        .and_then(|c| c.expires)
        .and_then(|ts| DateTime::from_timestamp(ts as i64, 0))
        .map(|dt| dt.to_rfc3339())
}

/// 更新账户的 cookies 到数据库
fn update_cookies(storage: &Storage, account_id: i64, result: &PlaywrightResult) -> Result<()> {
    if result.cookies.is_empty() {
        return Ok(());
    }

    let cookies_json = serde_json::to_string(&result.cookies)
        .context("Failed to serialize cookies")?;
    let issued_at = Utc::now().to_rfc3339();
    let expires_at = extract_session_expires(&result.cookies)
        .unwrap_or_else(|| issued_at.clone());

    storage.update_account_cookies(account_id, &cookies_json, &issued_at, &expires_at)?;
    Ok(())
}

/// 批量签到账户
///
/// 组装 PlaywrightAccountInput 列表，调用 run_playwright 执行签到动作，
/// 遍历结果更新 cookies 并返回 CheckinResult 列表。
pub async fn checkin_accounts(
    storage: &Storage,
    site: &Site,
    accounts: &[Account],
    headless: bool,
) -> Result<Vec<CheckinResult>> {
    let inputs: Vec<PlaywrightAccountInput> = accounts
        .iter()
        .map(|acc| build_playwright_input(site, acc))
        .collect();

    let payload = PlaywrightPayload {
        action: PlaywrightAction::Checkin,
        headless,
        timeout_ms: DEFAULT_TIMEOUT_MS,
        accounts: inputs,
    };

    let output = run_playwright(&payload).await?;

    let mut results = Vec::new();
    for pw_result in &output.results {
        // 找到对应的账户以更新 cookies
        if let Some(account) = accounts.iter().find(|a| a.name == pw_result.name) {
            if !pw_result.cookies.is_empty() {
                let _ = update_cookies(storage, account.id, pw_result);
            }
        }

        let balance_before = pw_result.before.as_ref().map(|q| q.quota - q.used_quota);
        let balance_after = pw_result.after.as_ref().map(|q| q.quota - q.used_quota);

        results.push(CheckinResult {
            account_name: pw_result.name.clone(),
            success: pw_result.success,
            balance_before,
            balance_after,
            error: pw_result.error.clone(),
        });
    }

    Ok(results)
}

/// 登录单个账户
///
/// 组装单个账户的 PlaywrightAccountInput，调用 run_playwright 执行登录，
/// 并将返回的 cookies 更新到数据库。
pub async fn login_account(
    storage: &Storage,
    site: &Site,
    account: &Account,
    headless: bool,
) -> Result<()> {
    let input = build_playwright_input(site, account);

    let payload = PlaywrightPayload {
        action: PlaywrightAction::Login,
        headless,
        timeout_ms: DEFAULT_TIMEOUT_MS,
        accounts: vec![input],
    };

    let output = run_playwright(&payload).await?;

    if let Some(pw_result) = output.results.first() {
        update_cookies(storage, account.id, pw_result)?;
    }

    Ok(())
}

/// 拉取账户详情
///
/// 调用 run_playwright 执行 FetchDetail 动作，将 user_info/tokens/logs/chart
/// 写入 account_cache 表，并更新 cookies。
pub async fn fetch_detail(
    storage: &Storage,
    site: &Site,
    account: &Account,
    headless: bool,
) -> Result<()> {
    let input = build_playwright_input(site, account);

    let payload = PlaywrightPayload {
        action: PlaywrightAction::FetchDetail,
        headless,
        timeout_ms: DEFAULT_TIMEOUT_MS,
        accounts: vec![input],
    };

    let output = run_playwright(&payload).await?;

    if let Some(pw_result) = output.results.first() {
        // 写入 user_info 到 overview cache
        if let Some(ref user_info) = pw_result.user_info {
            let payload_str = serde_json::to_string(user_info)?;
            storage.upsert_cache(account.id, CacheKind::Overview, &payload_str)?;
        }

        // 写入 tokens cache
        if let Some(ref tokens) = pw_result.tokens {
            let payload_str = serde_json::to_string(tokens)?;
            storage.upsert_cache(account.id, CacheKind::Tokens, &payload_str)?;
        }

        // 写入 logs cache
        if let Some(ref logs) = pw_result.logs {
            let payload_str = serde_json::to_string(logs)?;
            storage.upsert_cache(account.id, CacheKind::Logs, &payload_str)?;
        }

        // 写入 chart cache
        if let Some(ref chart) = pw_result.chart {
            let payload_str = serde_json::to_string(chart)?;
            storage.upsert_cache(account.id, CacheKind::Chart, &payload_str)?;
        }

        // 更新 cookies
        update_cookies(storage, account.id, pw_result)?;
    }

    Ok(())
}
