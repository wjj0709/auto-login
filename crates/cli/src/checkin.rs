use serde_json::{Map, Value};

/// 单账号签到详情，用于格式化通知消息
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CheckInDetail {
    pub name: String,
    pub user_info: Option<String>,
    pub before_quota: f64,
    pub before_used: f64,
    pub after_quota: f64,
    pub after_used: f64,
    pub check_in_reward: f64,
    pub usage_increase: f64,
    pub balance_change: f64,
    pub success: bool,
}

/// 格式化签到通知消息
pub fn format_check_in_notification(detail: &CheckInDetail) -> String {
    let mut lines = vec![
        format!("[CHECK-IN] {}", detail.name),
        "  ━━━━━━━━━━━━━━━━━━━━".to_string(),
    ];

    if let Some(user_info) = detail.user_info.as_ref().filter(|s| !s.is_empty()) {
        lines.push(format!("  👤 用户信息: {}", user_info));
    }

    lines.extend([
        "  📍 签到前".to_string(),
        format!("     💵 余额: ${:.2}  |  📊 累计消耗: ${:.2}", detail.before_quota, detail.before_used),
        "  📍 签到后".to_string(),
        format!("     💵 余额: ${:.2}  |  📊 累计消耗: ${:.2}", detail.after_quota, detail.after_used),
    ]);

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

/// 从 /api/user/self 的 data 字段里提取适合日志和通知展示的用户摘要。
pub fn format_user_info_summary(user_info: Option<&Value>) -> Option<String> {
    let map = user_info?.as_object()?;
    let mut parts = Vec::new();
    let mut used_keys = Vec::new();

    push_first_field(
        &mut parts,
        &mut used_keys,
        map,
        "ID",
        &["id", "user_id", "uid"],
    );
    push_first_field(
        &mut parts,
        &mut used_keys,
        map,
        "用户名",
        &["username", "user_name", "name", "display_name", "nickname"],
    );
    push_first_field(&mut parts, &mut used_keys, map, "邮箱", &["email"]);
    push_first_field(&mut parts, &mut used_keys, map, "状态", &["status"]);
    push_first_field(
        &mut parts,
        &mut used_keys,
        map,
        "分组",
        &["group", "group_name", "role"],
    );
    push_first_field(&mut parts, &mut used_keys, map, "等级", &["tier", "level"]);

    for (key, value) in map {
        if parts.len() >= 6 {
            break;
        }
        if used_keys
            .iter()
            .any(|used| used == &key.to_ascii_lowercase())
            || should_hide_user_info_key(key)
        {
            continue;
        }
        if let Some(text) = value_to_summary_text(value) {
            parts.push(format!("{}: {}", key, text));
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" | "))
    }
}

fn push_first_field(
    parts: &mut Vec<String>,
    used_keys: &mut Vec<String>,
    map: &Map<String, Value>,
    label: &str,
    keys: &[&str],
) {
    for key in keys {
        if let Some((actual_key, value)) = find_case_insensitive(map, key) {
            if should_hide_user_info_key(actual_key) {
                continue;
            }
            if let Some(text) = value_to_summary_text(value) {
                parts.push(format!("{}: {}", label, text));
                used_keys.push(actual_key.to_ascii_lowercase());
                break;
            }
        }
    }
}

fn find_case_insensitive<'a>(
    map: &'a Map<String, Value>,
    key: &str,
) -> Option<(&'a str, &'a Value)> {
    map.iter()
        .find(|(actual_key, _)| actual_key.eq_ignore_ascii_case(key))
        .map(|(actual_key, value)| (actual_key.as_str(), value))
}

fn value_to_summary_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Object(map) => ["name", "title", "label", "id"].iter().find_map(|key| {
            find_case_insensitive(map, key).and_then(|(_, value)| value_to_summary_text(value))
        }),
        _ => None,
    }
}

fn should_hide_user_info_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    let exact_hidden = ["quota", "used_quota", "raw_quota", "raw_used_quota"];
    let sensitive_parts = [
        "token",
        "secret",
        "password",
        "passwd",
        "cookie",
        "session",
        "authorization",
        "auth",
        "api_key",
    ];

    exact_hidden.contains(&lowered.as_str())
        || sensitive_parts.iter().any(|part| lowered.contains(part))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn formats_known_user_info_fields() {
        let user_info = json!({
            "id": 42,
            "username": "alice",
            "email": "alice@example.com",
            "status": 1,
            "quota": 500000,
            "access_token": "secret-token"
        });

        let summary = format_user_info_summary(Some(&user_info)).unwrap();

        assert!(summary.contains("ID: 42"));
        assert!(summary.contains("用户名: alice"));
        assert!(summary.contains("邮箱: alice@example.com"));
        assert!(summary.contains("状态: 1"));
        assert!(!summary.contains("quota"));
        assert!(!summary.contains("secret-token"));
    }

    #[test]
    fn formats_nested_display_fields() {
        let user_info = json!({
            "group": { "name": "pro", "session": "hidden" },
            "level": "gold"
        });

        let summary = format_user_info_summary(Some(&user_info)).unwrap();

        assert!(summary.contains("分组: pro"));
        assert!(summary.contains("等级: gold"));
        assert!(!summary.contains("hidden"));
    }
}
