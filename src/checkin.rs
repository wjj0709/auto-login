use std::collections::HashMap;
use std::time::Instant;
use reqwest::Client;
use serde_json::Value;

use crate::config::{AccountConfig, ProviderConfig};
use crate::log;

/// 用户信息
#[derive(Debug, Clone)]
pub struct UserInfo {
    pub success: bool,
    pub quota: f64,
    pub used_quota: f64,
    pub display: String,
    pub error: Option<String>,
}

/// 签到详情
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CheckInDetail {
    pub name: String,
    pub before_quota: f64,
    pub before_used: f64,
    pub after_quota: f64,
    pub after_used: f64,
    pub check_in_reward: f64,
    pub usage_increase: f64,
    pub balance_change: f64,
    pub success: bool,
}

/// 签到结果
pub struct CheckInResult {
    pub success: bool,
    pub user_info_before: Option<UserInfo>,
    pub user_info_after: Option<UserInfo>,
}

/// 解析 cookies 字符串或 JSON 对象为 HashMap
pub fn parse_cookies(cookies_value: &Value) -> HashMap<String, String> {
    match cookies_value {
        Value::Object(map) => {
            let cookies: HashMap<String, String> = map.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect();
            log::debug(&format!("Parsed {} cookie(s) from JSON object: [{}]", cookies.len(), cookies.keys().cloned().collect::<Vec<_>>().join(", ")));
            cookies
        }
        Value::String(s) => {
            let cookies: HashMap<String, String> = s.split(';')
                .filter_map(|cookie| {
                    let cookie = cookie.trim();
                    let mut parts = cookie.splitn(2, '=');
                    match (parts.next(), parts.next()) {
                        (Some(k), Some(v)) if !k.is_empty() => {
                            Some((k.trim().to_string(), v.trim().to_string()))
                        }
                        _ => None,
                    }
                })
                .collect();
            log::debug(&format!("Parsed {} cookie(s) from string: [{}]", cookies.len(), cookies.keys().cloned().collect::<Vec<_>>().join(", ")));
            cookies
        }
        _ => {
            log::warn("Cookies value is not a valid JSON object or string, returning empty map");
            HashMap::new()
        }
    }
}

/// 获取用户信息
pub async fn get_user_info(
    client: &Client,
    headers: &HashMap<String, String>,
    user_info_url: &str,
    label: &str,
) -> Option<UserInfo> {
    log::network(label, &format!("Requesting user info: GET {}", user_info_url));
    let start = Instant::now();

    let mut req = client.get(user_info_url);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }

    let response = match req.send().await {
        Ok(r) => {
            let elapsed = start.elapsed();
            log::network(label, &format!("Response received in {:.0?}, status: {}", elapsed, r.status().as_u16()));
            r
        }
        Err(e) => {
            let elapsed = start.elapsed();
            let err_msg = e.to_string();
            let short_err = &err_msg[..err_msg.len().min(80)];
            log::error_f(label, &format!("Request failed after {:.0?}: {}", elapsed, short_err));
            return Some(UserInfo {
                success: false,
                quota: 0.0,
                used_quota: 0.0,
                display: String::new(),
                error: Some(format!("Failed to get user info: {}", short_err)),
            });
        }
    };

    if response.status().as_u16() == 200 {
        if let Ok(data) = response.json::<Value>().await {
            if data.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                let user_data = data.get("data").cloned().unwrap_or(Value::Null);
                let quota_raw = user_data.get("quota").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let used_raw = user_data.get("used_quota").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let quota = (quota_raw / 500000.0 * 100.0).round() / 100.0;
                let used_quota = (used_raw / 500000.0 * 100.0).round() / 100.0;
                log::success_f(label, &format!("User info retrieved: balance=${:.2}, used=${:.2} (raw quota={}, raw used={})",
                    quota, used_quota, quota_raw, used_raw));
                return Some(UserInfo {
                    success: true,
                    quota,
                    used_quota,
                    display: format!(":money: Current balance: ${}, Used: ${}", quota, used_quota),
                    error: None,
                });
            }
            log::warn_f(label, "Response JSON does not contain success=true");
        } else {
            log::warn_f(label, "Failed to parse response as JSON");
        }
        Some(UserInfo {
            success: false,
            quota: 0.0,
            used_quota: 0.0,
            display: String::new(),
            error: Some("Failed to get user info: Invalid response".to_string()),
        })
    } else {
        let status = response.status().as_u16();
        log::error_f(label, &format!("User info request failed: HTTP {}", status));
        Some(UserInfo {
            success: false,
            quota: 0.0,
            used_quota: 0.0,
            display: String::new(),
            error: Some(format!("Failed to get user info: HTTP {}", status)),
        })
    }
}

/// 执行签到请求
pub async fn execute_check_in(
    client: &Client,
    account_name: &str,
    provider_config: &ProviderConfig,
    headers: &HashMap<String, String>,
) -> bool {
    let sign_in_path = provider_config.sign_in_path.as_deref().unwrap_or("/api/user/sign_in");
    let sign_in_url = format!("{}{}", provider_config.domain, sign_in_path);

    log::network(account_name, &format!("Executing check-in: POST {}", sign_in_url));
    log::debug_f(account_name, "Headers: Content-Type=application/json, X-Requested-With=XMLHttpRequest");

    let start = Instant::now();

    let mut req = client.post(&sign_in_url)
        .header("Content-Type", "application/json")
        .header("X-Requested-With", "XMLHttpRequest");
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }

    let response = match req.send().await {
        Ok(r) => {
            let elapsed = start.elapsed();
            log::network(account_name, &format!("Check-in response received in {:.0?}, status: {}", elapsed, r.status().as_u16()));
            r
        }
        Err(e) => {
            let elapsed = start.elapsed();
            log::error_f(account_name, &format!("Check-in request failed after {:.0?}: {}", elapsed, e));
            return false;
        }
    };

    let status = response.status().as_u16();

    if status == 200 {
        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => {
                log::error_f(account_name, &format!("Failed to read response body: {}", e));
                return false;
            }
        };

        log::debug_f(account_name, &format!("Response body ({}): {}", body.len().min(200), &body[..body.len().min(200)]));

        if let Ok(result) = serde_json::from_str::<Value>(&body) {
            let ret = result.get("ret").and_then(|v| v.as_i64());
            let code = result.get("code").and_then(|v| v.as_i64());
            let success_flag = result.get("success").and_then(|v| v.as_bool());
            log::debug_f(account_name, &format!("Response fields: ret={:?}, code={:?}, success={:?}", ret, code, success_flag));

            let is_success = ret.map(|v| v == 1).unwrap_or(false)
                || code.map(|v| v == 0).unwrap_or(false)
                || success_flag.unwrap_or(false);

            if is_success {
                log::success_f(account_name, "Check-in successful!");
                return true;
            }

            let error_msg = result.get("msg")
                .or_else(|| result.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error")
                .to_lowercase();

            let already_checked_keywords = ["已经签到", "已签到", "重复签到", "already checked", "already signed"];
            if let Some(matched_kw) = already_checked_keywords.iter().find(|kw| error_msg.contains(**kw)) {
                log::success_f(account_name, &format!("Already checked in today (matched keyword: \"{}\")", matched_kw));
                return true;
            }

            log::error_f(account_name, &format!("Check-in failed: {}", error_msg));
            false
        } else {
            log::warn_f(account_name, "Response is not valid JSON, checking for 'success' keyword...");
            if body.to_lowercase().contains("success") {
                log::success_f(account_name, "Check-in successful! (detected 'success' in non-JSON response)");
                true
            } else {
                log::error_f(account_name, "Check-in failed: Invalid response format (no 'success' keyword found)");
                false
            }
        }
    } else {
        log::error_f(account_name, &format!("Check-in failed: HTTP {}", status));
        false
    }
}

/// 为单个账号执行签到操作
pub async fn check_in_account(
    account: &AccountConfig,
    account_index: usize,
    provider_config: &ProviderConfig,
    waf_cookies: Option<HashMap<String, String>>,
) -> CheckInResult {
    let account_name = account.get_display_name(account_index);
    let account_start = Instant::now();

    log::phase(&format!("Processing account: {}", account_name));
    log::processing_f(&account_name, &format!("Provider: \"{}\" ({})", account.provider, provider_config.domain));
    log::processing_f(&account_name, &format!("Sign-in path: {}, User info path: {}",
        provider_config.sign_in_path.as_deref().unwrap_or("None (auto)"),
        provider_config.user_info_path,
    ));

    let mut user_cookies = parse_cookies(&account.cookies);
    if user_cookies.is_empty() {
        log::error_f(&account_name, "Invalid cookies configuration: no cookies parsed");
        return CheckInResult { success: false, user_info_before: None, user_info_after: None };
    }
    log::info_f(&account_name, &format!("{} user cookie(s) ready: [{}]", user_cookies.len(), user_cookies.keys().cloned().collect::<Vec<_>>().join(", ")));

    // 合并 WAF cookies（如 acw_sc__v2）到用户 cookies 中
    if let Some(ref waf) = waf_cookies {
        log::info_f(&account_name, &format!("Merging {} WAF cookie(s): [{}]", waf.len(), waf.keys().cloned().collect::<Vec<_>>().join(", ")));
        for (k, v) in waf {
            user_cookies.insert(k.clone(), v.clone());
        }
        log::info_f(&account_name, &format!("Total cookies after merge: {} cookie(s)", user_cookies.len()));
    } else {
        log::warn_f(&account_name, "No WAF cookies available, API requests may be blocked by WAF");
    }

    // 构建 HTTP client（带 cookie store）
    let client = match Client::builder()
        .cookie_store(true)
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => {
            log::debug_f(&account_name, "HTTP client created (cookie_store=true, timeout=30s)");
            c
        }
        Err(e) => {
            log::error_f(&account_name, &format!("Failed to create HTTP client: {}", e));
            return CheckInResult { success: false, user_info_before: None, user_info_after: None };
        }
    };

    // 通过 Cookie header 传递所有 cookies（用户 + WAF）
    let cookie_header: String = user_cookies
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("; ");

    let mut headers = HashMap::new();
    headers.insert("User-Agent".to_string(), "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36".to_string());
    headers.insert("Accept".to_string(), "application/json, text/plain, */*".to_string());
    headers.insert("Accept-Language".to_string(), "zh-CN,zh;q=0.9,en;q=0.8".to_string());
    headers.insert("Accept-Encoding".to_string(), "gzip, deflate, br, zstd".to_string());
    headers.insert("Referer".to_string(), provider_config.domain.clone());
    headers.insert("Origin".to_string(), provider_config.domain.clone());
    headers.insert("Connection".to_string(), "keep-alive".to_string());
    headers.insert("Sec-Fetch-Dest".to_string(), "empty".to_string());
    headers.insert("Sec-Fetch-Mode".to_string(), "cors".to_string());
    headers.insert("Sec-Fetch-Site".to_string(), "same-origin".to_string());
    headers.insert(provider_config.api_user_key.clone(), account.api_user.clone());
    headers.insert("Cookie".to_string(), cookie_header);

    log::debug_f(&account_name, &format!("Request headers configured: {} header(s) including Cookie", headers.len()));

    let user_info_url = format!("{}{}", provider_config.domain, provider_config.user_info_path);

    // 获取签到前用户信息
    log::info_f(&account_name, "--- Step 1/3: Fetching user info BEFORE check-in ---");
    let user_info_before = get_user_info(&client, &headers, &user_info_url, &format!("{}/before", account_name)).await;
    if let Some(ref info) = user_info_before {
        if info.success {
            log::info_f(&account_name, &format!("BEFORE check-in -> balance: ${:.2}, used: ${:.2}", info.quota, info.used_quota));
        } else if let Some(ref err) = info.error {
            log::warn_f(&account_name, &format!("Failed to get user info before check-in: {}", err));
        }
    }

    // 执行签到
    if provider_config.needs_manual_check_in() {
        log::info_f(&account_name, "--- Step 2/3: Executing manual check-in ---");
        let success = execute_check_in(&client, &account_name, provider_config, &headers).await;

        // 签到后获取用户信息
        log::info_f(&account_name, "--- Step 3/3: Fetching user info AFTER check-in ---");
        let user_info_after = get_user_info(&client, &headers, &user_info_url, &format!("{}/after", account_name)).await;
        if let Some(ref info) = user_info_after {
            if info.success {
                log::info_f(&account_name, &format!("AFTER check-in  -> balance: ${:.2}, used: ${:.2}", info.quota, info.used_quota));
            }
        }

        let elapsed = account_start.elapsed();
        let status_str = if success { "SUCCESS" } else { "FAILED" };
        log::info_f(&account_name, &format!("Account processing complete: {} (total time: {:.1?})", status_str, elapsed));

        CheckInResult { success, user_info_before, user_info_after }
    } else {
        log::info_f(&account_name, "--- Step 2/3: No manual check-in needed (auto check-in via user info request) ---");
        log::info_f(&account_name, "--- Step 3/3: Fetching user info AFTER auto check-in ---");
        let user_info_after = get_user_info(&client, &headers, &user_info_url, &format!("{}/after", account_name)).await;
        if let Some(ref info) = user_info_after {
            if info.success {
                log::info_f(&account_name, &format!("AFTER check-in  -> balance: ${:.2}, used: ${:.2}", info.quota, info.used_quota));
            }
        }

        let elapsed = account_start.elapsed();
        log::success_f(&account_name, &format!("Account processing complete: SUCCESS - auto check-in (total time: {:.1?})", elapsed));

        CheckInResult { success: true, user_info_before, user_info_after }
    }
}

/// 格式化签到通知消息
pub fn format_check_in_notification(detail: &CheckInDetail) -> String {
    let mut lines = vec![
        format!("[CHECK-IN] {}", detail.name),
        "  ━━━━━━━━━━━━━━━━━━━━".to_string(),
        "  📍 签到前".to_string(),
        format!("     💵 余额: ${:.2}  |  📊 累计消耗: ${:.2}", detail.before_quota, detail.before_used),
        "  📍 签到后".to_string(),
        format!("     💵 余额: ${:.2}  |  📊 累计消耗: ${:.2}", detail.after_quota, detail.after_used),
    ];

    let has_reward = detail.check_in_reward != 0.0;
    let has_usage = detail.usage_increase != 0.0;

    if has_reward || has_usage {
        lines.push("  ━━━━━━━━━━━━━━━━━━━━".to_string());

        if !has_reward && has_usage {
            lines.push("  ℹ️  今日已签到（期间有使用）".to_string());
        }

        if has_reward {
            lines.push(format!("  🎁 签到获得: +${:.2}", detail.check_in_reward));
        }

        if has_usage {
            lines.push(format!("  📉 期间消耗: ${:.2}", detail.usage_increase));
        }

        if detail.balance_change != 0.0 {
            let change_symbol = if detail.balance_change > 0.0 { "+" } else { "" };
            let change_emoji = if detail.balance_change > 0.0 { "📈" } else { "📉" };
            lines.push(format!("  {} 余额变化: {}${:.2}", change_emoji, change_symbol, detail.balance_change));
        }
    } else {
        lines.push("  ━━━━━━━━━━━━━━━━━━━━".to_string());
        lines.push("  ℹ️  今日已签到，无变化".to_string());
    }

    lines.join("\n")
}
