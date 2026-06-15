// ============================================================================
// checkin.rs — 签到结果格式化模块
// ============================================================================
// 功能：
// 1. 签到邮件报告结构体 — 保存单个账号的签到详情
// 2. format_check_in_email_report() — 将全部账号结果格式化为邮件正文
// 3. format_user_info_summary() — 从 /api/user/self 返回的用户信息中提取摘要
// ============================================================================

use serde_json::{Map, Value};

use crate::playwright::CookieReport;

/// 邮件报告中的余额快照。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BalanceSnapshot {
    quota: f64,
    used_quota: f64,
}

impl_copy_accessors!(BalanceSnapshot {
    quota: f64 => quota, set_quota;
    used_quota: f64 => used_quota, set_used_quota;
});

impl BalanceSnapshot {
    pub fn new(quota: f64, used_quota: f64) -> Self {
        Self { quota, used_quota }
    }
}

/// 邮件报告中的余额变化摘要。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BalanceChangeSummary {
    check_in_reward: f64,
    usage_increase: f64,
    balance_change: f64,
}

impl_copy_accessors!(BalanceChangeSummary {
    check_in_reward: f64 => check_in_reward, set_check_in_reward;
    usage_increase: f64 => usage_increase, set_usage_increase;
    balance_change: f64 => balance_change, set_balance_change;
});

impl BalanceChangeSummary {
    pub fn new(check_in_reward: f64, usage_increase: f64, balance_change: f64) -> Self {
        Self {
            check_in_reward,
            usage_increase,
            balance_change,
        }
    }
}

/// 单个账号在邮件报告中的展示项。
#[derive(Debug, Clone, PartialEq)]
pub struct CheckInReportItem {
    name: String,
    success: bool,
    user_info: Option<String>,
    before: Option<BalanceSnapshot>,
    after: Option<BalanceSnapshot>,
    balance_change: Option<BalanceChangeSummary>,
    error: Option<String>,
}

impl_ref_accessors!(CheckInReportItem {
    name: String => name, set_name;
    user_info: Option<String> => user_info, set_user_info;
    before: Option<BalanceSnapshot> => before, set_before;
    after: Option<BalanceSnapshot> => after, set_after;
    balance_change: Option<BalanceChangeSummary> => balance_change, set_balance_change;
    error: Option<String> => error, set_error;
});

impl_copy_accessors!(CheckInReportItem {
    success: bool => success, set_success;
});

impl CheckInReportItem {
    pub fn new(name: String, success: bool) -> Self {
        Self {
            name,
            success,
            user_info: None,
            before: None,
            after: None,
            balance_change: None,
            error: None,
        }
    }
}

/// 将全部账号结果格式化为邮件正文。
///
/// 邮件正文使用纯文本，字段固定、分隔清晰，避免不同邮箱客户端对富文本的兼容问题。
pub fn format_check_in_email_report(executed_at: &str, items: &[CheckInReportItem]) -> String {
    let total = items.len();
    let success_count = items.iter().filter(|item| item.success).count();
    let failed_count = total.saturating_sub(success_count);

    let mut lines = vec![
        "AnyRouter 签到报告".to_string(),
        "====================".to_string(),
        format!("执行时间: {}", executed_at),
        format!("账户总数: {}", total),
        format!("成功: {}/{}", success_count, total),
        format!("失败: {}/{}", failed_count, total),
        String::new(),
        "账号明细".to_string(),
        "--------".to_string(),
    ];

    if items.is_empty() {
        lines.push("无账号结果。".to_string());
    } else {
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                lines.push(String::new());
            }
            lines.push(format_check_in_report_item(index, total, item));
        }
    }

    lines.join("\n")
}

/// 将单个账号结果格式化为邮件明细块。
pub fn format_check_in_report_item(index: usize, total: usize, item: &CheckInReportItem) -> String {
    let mut lines = vec![
        format!("账号 {}/{}: {}", index + 1, total, item.name),
        format!(
            "状态: {}",
            if item.success {
                "签到成功"
            } else {
                "签到失败"
            }
        ),
    ];

    if let Some(user_info) = item.user_info.as_ref().filter(|s| !s.trim().is_empty()) {
        lines.push(format!("用户信息: {}", user_info.trim()));
    }

    if !item.success {
        lines.push(format!(
            "失败信息: {}",
            item.error
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("未知错误")
        ));
    }

    lines.push(format!("签到前: {}", format_balance_snapshot(item.before)));
    lines.push(format!("签到后: {}", format_balance_snapshot(item.after)));

    if let Some(change) = item.balance_change {
        lines.push(format_money_change("签到获得", change.check_in_reward));
        if change.usage_increase != 0.0 {
            lines.push(format!("期间消耗: ${:.2}", change.usage_increase));
        }
        lines.push(format_money_change("余额变化", change.balance_change));
    } else {
        lines.push("余额变化: 无法计算".to_string());
    }

    lines.join("\n")
}

fn format_balance_snapshot(snapshot: Option<BalanceSnapshot>) -> String {
    match snapshot {
        Some(snapshot) => format!(
            "余额 ${:.2} | 累计消耗 ${:.2}",
            snapshot.quota, snapshot.used_quota
        ),
        None => "未获取".to_string(),
    }
}

fn format_money_change(label: &str, amount: f64) -> String {
    if amount > 0.0 {
        format!("{}: +${:.2}", label, amount)
    } else if amount < 0.0 {
        format!("{}: -${:.2}", label, amount.abs())
    } else {
        format!("{}: $0.00", label)
    }
}

/// 从 /api/user/self 的 data 字段里提取适合日志和通知展示的用户摘要
///
/// 按优先级提取以下字段：
/// 1. ID (id / user_id / uid)
/// 2. 用户名 (username / user_name / name / display_name / nickname)
/// 3. 邮箱 (email)
/// 4. 状态 (status)
/// 5. 分组 (group / group_name / role)
/// 6. 等级 (tier / level)
///
/// 最多显示 6 个字段，自动过滤敏感字段（token、secret、password 等）
///
/// # 参数
/// - user_info: /api/user/self 返回的 data 字段（JSON Value）
///
/// # 返回
/// - Some(String): 格式化后的用户摘要，如 "ID: 42 | 用户名: alice | 邮箱: alice@example.com"
/// - None: 无法提取任何有效字段
pub fn format_user_info_summary(user_info: Option<&Value>) -> Option<String> {
    // 提取 JSON 对象部分
    let map = user_info?.as_object()?;

    let mut parts = Vec::new();
    let mut used_keys = Vec::new();

    // 按优先级提取常用字段
    push_first_field(
        &mut parts,
        &mut used_keys,
        map,
        "ID",
        // 多种可能的字段名，按优先级排序
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

    // 遍历剩余字段，最多凑够 6 个
    for (key, value) in map {
        if parts.len() >= 6 {
            break;
        }
        // 跳过已使用的 key 和敏感 key
        if used_keys
            .iter()
            .any(|used| used == &key.to_ascii_lowercase())
            || should_hide_user_info_key(key)
        {
            continue;
        }
        // 尝试将值转为可展示的文本
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

/// 将登录返回的 Cookie 分析结果格式化为日志文本
pub fn format_cookie_report(report: &CookieReport) -> Option<String> {
    if report.request_cookie_names().is_empty()
        && report.response_cookie_names().is_empty()
        && report.cookies().is_empty()
        && report.note().as_deref().unwrap_or("").trim().is_empty()
    {
        return None;
    }

    let mut lines = Vec::new();

    if !report.request_cookie_names().is_empty() {
        lines.push(format!(
            "Login request cookies: {}",
            report.request_cookie_names().join(", ")
        ));
    }

    if !report.response_cookie_names().is_empty() {
        lines.push(format!(
            "Login response Set-Cookie: {}",
            report.response_cookie_names().join(", ")
        ));
    }

    for cookie in report.cookies() {
        let lifetime = if cookie.is_session() {
            "session cookie (no explicit expiry)".to_string()
        } else if let Some(expires_at) = cookie.expires_at().as_ref() {
            match cookie.expires_in().as_ref() {
                Some(expires_in) if !expires_in.is_empty() => {
                    format!("expires at {} ({})", expires_at, expires_in)
                }
                _ => format!("expires at {}", expires_at),
            }
        } else if let Some(max_age_seconds) = cookie.max_age_seconds() {
            format!("max-age={}s", max_age_seconds)
        } else {
            "expiry unknown".to_string()
        };

        let mut attrs = Vec::new();
        if let Some(source) = cookie.source().as_ref().filter(|s| !s.is_empty()) {
            attrs.push(format!("source={}", source));
        }
        if let Some(domain) = cookie.domain().as_ref().filter(|s| !s.is_empty()) {
            attrs.push(format!("domain={}", domain));
        }
        if let Some(path) = cookie.path().as_ref().filter(|s| !s.is_empty()) {
            attrs.push(format!("path={}", path));
        }
        if let Some(same_site) = cookie.same_site().as_ref().filter(|s| !s.is_empty()) {
            attrs.push(format!("sameSite={}", same_site));
        }
        attrs.push(format!("secure={}", cookie.secure()));
        attrs.push(format!("httpOnly={}", cookie.http_only()));

        lines.push(format!(
            "Cookie {}: {} [{}]",
            cookie.name(),
            lifetime,
            attrs.join(", ")
        ));
    }

    if let Some(note) = report.note().as_ref().filter(|s| !s.trim().is_empty()) {
        lines.push(format!("Cookie note: {}", note.trim()));
    }

    Some(lines.join("\n"))
}

/// 按优先级从多个候选 key 中提取第一个有效字段
///
/// 遍历 keys 列表，找到第一个存在于 map 中且值为可展示文本的 key，
/// 将其以 "label: value" 的格式添加到 parts 中
///
/// # 参数
/// - parts: 输出列表，收集格式化后的字段
/// - used_keys: 已使用的 key 列表，避免重复
/// - map: JSON 对象
/// - label: 显示标签（如 "ID"、"用户名"）
/// - keys: 候选 key 列表，按优先级排序
fn push_first_field(
    parts: &mut Vec<String>,
    used_keys: &mut Vec<String>,
    map: &Map<String, Value>,
    label: &str,
    keys: &[&str],
) {
    for key in keys {
        // 大小写不敏感查找 key
        if let Some((actual_key, value)) = find_case_insensitive(map, key) {
            // 跳过敏感字段
            if should_hide_user_info_key(actual_key) {
                continue;
            }
            // 尝试将值转为可展示的文本
            if let Some(text) = value_to_summary_text(value) {
                parts.push(format!("{}: {}", label, text));
                // 记录已使用的 key（小写形式），避免后续重复提取
                used_keys.push(actual_key.to_ascii_lowercase());
                break;
            }
        }
    }
}

/// 大小写不敏感地在 JSON Map 中查找 key
///
/// # 参数
/// - map: JSON 对象
/// - key: 要查找的 key（不区分大小写）
///
/// # 返回
/// - Some((&str, &Value)): 找到的实际 key 和对应的值
/// - None: 未找到
fn find_case_insensitive<'a>(
    map: &'a Map<String, Value>,
    key: &str,
) -> Option<(&'a str, &'a Value)> {
    map.iter()
        .find(|(actual_key, _)| actual_key.eq_ignore_ascii_case(key))
        .map(|(actual_key, value)| (actual_key.as_str(), value))
}

/// 将 JSON Value 转为可展示的摘要文本
///
/// 支持的类型：
/// - String: 去除首尾空格后返回（空字符串返回 None）
/// - Number: 直接转字符串
/// - Bool: 直接转字符串
/// - Object: 递归查找 name/title/label/id 字段
/// - Null/Array: 返回 None
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
        // 对于嵌套对象，尝试提取 name/title/label/id 字段
        Value::Object(map) => ["name", "title", "label", "id"].iter().find_map(|key| {
            find_case_insensitive(map, key).and_then(|(_, value)| value_to_summary_text(value))
        }),
        _ => None,
    }
}

/// 判断用户信息的某个 key 是否应该隐藏
///
/// 隐藏规则：
/// 1. 精确匹配：quota、used_quota、raw_quota、raw_used_quota（余额字段在通知中单独展示）
/// 2. 包含敏感关键词：token、secret、password、passwd、cookie、session、authorization、auth、api_key
fn should_hide_user_info_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    // 精确匹配需要隐藏的 key
    let exact_hidden = ["quota", "used_quota", "raw_quota", "raw_used_quota"];
    // 包含这些子串的 key 也需要隐藏
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

// ============================================================================
// 单元测试
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 测试：正常提取用户信息字段，敏感字段应被过滤
    #[test]
    fn formats_known_user_info_fields() {
        let user_info = json!({
            "id": 42,
            "username": "alice",
            "email": "alice@example.com",
            "status": 1,
            "quota": 500000,          // 应被隐藏
            "access_token": "secret-token"  // 应被隐藏
        });

        let summary = format_user_info_summary(Some(&user_info)).unwrap();

        assert!(summary.contains("ID: 42"));
        assert!(summary.contains("用户名: alice"));
        assert!(summary.contains("邮箱: alice@example.com"));
        assert!(summary.contains("状态: 1"));
        // quota 和 access_token 是敏感字段，不应出现在摘要中
        assert!(!summary.contains("quota"));
        assert!(!summary.contains("secret-token"));
    }

    /// 测试：嵌套对象中提取 display 字段，同时过滤敏感子字段
    #[test]
    fn formats_nested_display_fields() {
        let user_info = json!({
            "group": { "name": "pro", "session": "hidden" },  // group.name 应展示，session 应隐藏
            "level": "gold"
        });

        let summary = format_user_info_summary(Some(&user_info)).unwrap();

        assert!(summary.contains("分组: pro"));
        assert!(summary.contains("等级: gold"));
        // session 是敏感字段，不应出现在摘要中
        assert!(!summary.contains("hidden"));
    }

    #[test]
    fn formats_cookie_report_summary() {
        let mut cookie = crate::playwright::CookieReportEntry::default();
        cookie
            .set_name("session".to_string())
            .set_domain(Some("anyrouter.top".to_string()))
            .set_path(Some("/".to_string()))
            .set_source(Some("set-cookie+browser-context".to_string()))
            .set_expires_at(Some("2026-06-16T00:00:00Z".to_string()))
            .set_expires_in(Some("in 12h".to_string()))
            .set_same_site(Some("Lax".to_string()))
            .set_secure(true)
            .set_http_only(true)
            .set_is_session(false);

        let mut report = CookieReport::default();
        report
            .set_request_cookie_names(vec!["acw_tc".to_string()])
            .set_response_cookie_names(vec!["session".to_string()])
            .set_cookies(vec![cookie]);

        let summary = format_cookie_report(&report).unwrap();

        assert!(summary.contains("Login request cookies: acw_tc"));
        assert!(summary.contains("Login response Set-Cookie: session"));
        assert!(summary.contains("Cookie session: expires at 2026-06-16T00:00:00Z (in 12h)"));
        assert!(summary.contains("httpOnly=true"));
    }

    #[test]
    fn formats_success_account_report_with_before_and_after_balance() {
        let mut item = CheckInReportItem::new("主账号".to_string(), true);
        item.set_user_info(Some("ID: 42 | 用户名: alice".to_string()))
            .set_before(Some(BalanceSnapshot::new(5.0, 1.2)))
            .set_after(Some(BalanceSnapshot::new(5.5, 1.2)))
            .set_balance_change(Some(BalanceChangeSummary::new(0.5, 0.0, 0.5)));

        let report = format_check_in_report_item(0, 1, &item);

        assert!(report.contains("账号 1/1: 主账号"));
        assert!(report.contains("状态: 签到成功"));
        assert!(report.contains("用户信息: ID: 42 | 用户名: alice"));
        assert!(report.contains("签到前: 余额 $5.00 | 累计消耗 $1.20"));
        assert!(report.contains("签到后: 余额 $5.50 | 累计消耗 $1.20"));
        assert!(report.contains("签到获得: +$0.50"));
        assert!(report.contains("余额变化: +$0.50"));
        assert!(!report.contains("失败信息"));
    }

    #[test]
    fn formats_failed_account_report_with_error_and_partial_balance() {
        let mut item = CheckInReportItem::new("备用账号".to_string(), false);
        item.set_after(Some(BalanceSnapshot::new(3.0, 0.75)))
            .set_error(Some("cookie expired".to_string()));

        let report = format_check_in_report_item(1, 2, &item);

        assert!(report.contains("账号 2/2: 备用账号"));
        assert!(report.contains("状态: 签到失败"));
        assert!(report.contains("失败信息: cookie expired"));
        assert!(report.contains("签到前: 未获取"));
        assert!(report.contains("签到后: 余额 $3.00 | 累计消耗 $0.75"));
        assert!(report.contains("余额变化: 无法计算"));
    }

    #[test]
    fn formats_email_report_for_all_accounts() {
        let mut success = CheckInReportItem::new("主账号".to_string(), true);
        success
            .set_before(Some(BalanceSnapshot::new(5.0, 1.0)))
            .set_after(Some(BalanceSnapshot::new(5.2, 1.0)))
            .set_balance_change(Some(BalanceChangeSummary::new(0.2, 0.0, 0.2)));

        let mut failed = CheckInReportItem::new("备用账号".to_string(), false);
        failed.set_error(Some("sign in api returned 401".to_string()));

        let email = format_check_in_email_report("2026-06-15 12:00:00", &[success, failed]);

        assert!(email.starts_with("AnyRouter 签到报告"));
        assert!(email.contains("执行时间: 2026-06-15 12:00:00"));
        assert!(email.contains("账户总数: 2"));
        assert!(email.contains("成功: 1/2"));
        assert!(email.contains("失败: 1/2"));
        assert!(email.contains("账号 1/2: 主账号"));
        assert!(email.contains("账号 2/2: 备用账号"));
        assert!(email.contains("失败信息: sign in api returned 401"));
    }
}
