/// 单账号签到详情，用于格式化通知消息
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
