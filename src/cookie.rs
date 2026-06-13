//! Cookie 生命周期工具:过期时间换算与状态分类(纯逻辑,带单测)。
//!
//! Playwright 回传的 cookie `expires` 为 Unix 秒,`-1`(或非正)表示会话期 cookie。
//! 库内 `accounts.cookie_expires_at` 存 ISO8601 本地时间字符串;会话期/未知存 NULL。

use chrono::{DateTime, Duration, Local, SecondsFormat, TimeZone, Utc};

/// Cookie 整体状态,用于界面着色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieStatus {
    /// 有效(绿)
    Valid,
    /// 24 小时内到期(黄)
    ExpiringSoon,
    /// 已过期(红)
    Expired,
    /// 有效期未知 / 会话期(灰)
    Unknown,
}

/// Playwright `expires`(Unix 秒;`<=0` 或非有限值表示会话期)→ 存库 ISO 字符串。
/// 会话期 / 非法值返回 None。
pub fn expires_to_iso(expires: f64) -> Option<String> {
    if !expires.is_finite() || expires <= 0.0 {
        return None;
    }
    Utc.timestamp_opt(expires as i64, 0)
        .single()
        .map(|dt| dt.with_timezone(&Local).to_rfc3339_opts(SecondsFormat::Secs, false))
}

/// 依据过期时间字符串与给定「当前时间」分类(`now` 注入便于测试)。
pub fn classify_at(expires_at: Option<&str>, now: DateTime<Local>) -> CookieStatus {
    let Some(s) = expires_at else {
        return CookieStatus::Unknown;
    };
    match DateTime::parse_from_rfc3339(s) {
        Ok(dt) => {
            let exp = dt.with_timezone(&Local);
            if exp <= now {
                CookieStatus::Expired
            } else if exp <= now + Duration::hours(24) {
                CookieStatus::ExpiringSoon
            } else {
                CookieStatus::Valid
            }
        }
        Err(_) => CookieStatus::Unknown,
    }
}

/// 便捷:用系统当前本地时间分类。
#[allow(dead_code)] // 界面层使用
pub fn classify(expires_at: Option<&str>) -> CookieStatus {
    classify_at(expires_at, Local::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Local, TimeZone};

    #[test]
    fn session_or_invalid_expires_is_none() {
        assert_eq!(expires_to_iso(-1.0), None);
        assert_eq!(expires_to_iso(0.0), None);
        assert_eq!(expires_to_iso(f64::NAN), None);
        assert_eq!(expires_to_iso(f64::INFINITY), None);
    }

    #[test]
    fn future_expires_roundtrips_to_valid() {
        // 2100-01-01 UTC,相对 2026 必为有效
        let iso = expires_to_iso(4_102_444_800.0).expect("未来时间应可换算");
        let now = Local.with_ymd_and_hms(2026, 6, 13, 12, 0, 0).unwrap();
        assert_eq!(classify_at(Some(&iso), now), CookieStatus::Valid);
    }

    #[test]
    fn none_or_garbage_is_unknown() {
        assert_eq!(classify(None), CookieStatus::Unknown);
        assert_eq!(classify(Some("not-a-date")), CookieStatus::Unknown);
    }

    #[test]
    fn past_soon_far_classification() {
        let now = Local.with_ymd_and_hms(2026, 6, 13, 12, 0, 0).unwrap();
        let past = (now - Duration::hours(1)).to_rfc3339();
        let soon = (now + Duration::hours(5)).to_rfc3339();
        let far = (now + Duration::days(30)).to_rfc3339();
        assert_eq!(classify_at(Some(&past), now), CookieStatus::Expired);
        assert_eq!(classify_at(Some(&soon), now), CookieStatus::ExpiringSoon);
        assert_eq!(classify_at(Some(&far), now), CookieStatus::Valid);
    }

    #[test]
    fn boundary_exactly_24h_is_expiring_soon() {
        let now = Local.with_ymd_and_hms(2026, 6, 13, 12, 0, 0).unwrap();
        let at_24h = (now + Duration::hours(24)).to_rfc3339();
        assert_eq!(classify_at(Some(&at_24h), now), CookieStatus::ExpiringSoon);
    }
}
