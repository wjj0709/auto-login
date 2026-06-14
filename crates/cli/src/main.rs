mod balance;
mod checkin;
mod cli_args;
mod config;
mod log;
mod notify;
mod playwright;

use std::collections::HashMap;
use std::time::Instant;
use chrono::Local;

use balance::{generate_balance_hash, load_balance_hash, save_balance_hash};
use checkin::{format_check_in_notification, format_user_info_summary, CheckInDetail};
use config::{load_accounts_config, AppConfig};
use notify::NotificationKit;

#[tokio::main]
async fn main() {
    let program_start = Instant::now();
    dotenvy::dotenv().ok();

    log::phase("AnyRouter Auto Check-in Script (Rust + Playwright)");
    log::system("AnyRouter.top multi-account auto check-in script started");
    log::now_time(&format!("Program started at {}", Local::now().format("%Y-%m-%d %H:%M:%S")));
    log::separator();

    // ========== 阶段 1: 加载配置 ==========
    log::phase("Phase 1: Loading Configuration");

    log::info("===== .env 环境变量配置信息 =====");
    let env_keys = [
        ("ANYROUTER_ACCOUNTS", true),
        ("PROVIDERS", false),
        ("PLAYWRIGHT_HEADLESS", false),
        ("PYTHON_BIN", false),
        ("EMAIL_USER", true),
        ("EMAIL_PASS", true),
        ("EMAIL_TO", true),
        ("EMAIL_SENDER", true),
        ("CUSTOM_SMTP_SERVER", false),
    ];
    for (key, is_sensitive) in &env_keys {
        match std::env::var(key) {
            Ok(val) => {
                let display_val = if *is_sensitive && val.len() > 8 {
                    format!("{}...{} (长度: {})", &val[..4], &val[val.len().saturating_sub(4)..], val.len())
                } else if *is_sensitive {
                    format!("*** (长度: {})", val.len())
                } else {
                    val.clone()
                };
                log::info(&format!("  {} = {}", key, display_val));
            }
            Err(_) => {
                log::debug(&format!("  {} = (未设置)", key));
            }
        }
    }
    log::separator();

    let app_config = AppConfig::load_from_env();

    let accounts = match load_accounts_config() {
        Some(a) => a,
        None => {
            log::error("Unable to load account configuration, program exits");
            std::process::exit(1);
        }
    };

    log::success(&format!("Configuration loaded: {} provider(s), {} account(s)",
        app_config.providers.len(), accounts.len()));

    let last_balance_hash = load_balance_hash();

    // ========== 阶段 2: 通过 Playwright 执行签到 ==========
    log::phase(&format!("Phase 2: Processing {} Account(s) via Playwright", accounts.len()));

    let runner_results = match playwright::run_checkin(&accounts, &app_config.providers).await {
        Ok(r) => r,
        Err(e) => {
            log::error(&format!("Playwright runner failed: {}", e));
            log::error("Hint: ensure dependencies installed -> pip3 install playwright && python3 -m playwright install chromium");
            std::process::exit(1);
        }
    };

    let mut success_count = 0usize;
    let total_count = accounts.len();
    let mut notification_content: Vec<String> = Vec::new();
    let mut current_balances: HashMap<String, f64> = HashMap::new();
    let mut account_check_in_details: HashMap<String, CheckInDetail> = HashMap::new();
    let mut need_notify = false;
    let mut balance_changed = false;

    // 用 name 索引 runner 输出（保留 accounts 配置顺序，对结果做匹配）
    let mut result_by_name: HashMap<String, playwright::PlaywrightResult> = HashMap::new();
    for r in runner_results {
        result_by_name.insert(r.name.clone(), r);
    }

    for (i, account) in accounts.iter().enumerate() {
        let account_name = account.get_display_name(i);
        let account_key = format!("account_{}", i + 1);
        log::info(&format!("===== Account {}/{}: {} =====", i + 1, total_count, account_name));

        let runner = match result_by_name.remove(&account_name) {
            Some(r) => r,
            None => {
                log::error_f(&account_name, "No result returned by Playwright runner");
                need_notify = true;
                notification_content.push(format!("[FAIL] {} - missing runner result", account_name));
                continue;
            }
        };

        if runner.used_login {
            log::info_f(&account_name, "Logged in via username/password (cookie was missing or expired)");
        }

        let user_info_summary = format_user_info_summary(runner.user_info.as_ref());
        if let Some(summary) = user_info_summary.as_ref() {
            log::info_f(&account_name, &format!("User info: {}", summary));
        }

        if runner.success {
            success_count += 1;
            log::success(&format!("Account {}/{} [{}]: check-in SUCCEEDED", i + 1, total_count, account_name));
        } else {
            log::error(&format!(
                "Account {}/{} [{}]: check-in FAILED ({})",
                i + 1,
                total_count,
                account_name,
                runner.error.clone().unwrap_or_else(|| "unknown".to_string())
            ));
            need_notify = true;
        }

        // 余额信息
        if let (Some(before), Some(after)) = (runner.before.as_ref(), runner.after.as_ref()) {
            current_balances.insert(account_key.clone(), after.quota);
            let total_before = before.quota + before.used_quota;
            let total_after = after.quota + after.used_quota;
            let check_in_reward = total_after - total_before;
            let usage_increase = after.used_quota - before.used_quota;
            let balance_change = after.quota - before.quota;

            log::info_f(&account_name, &format!(
                "Balance: before=${:.2} -> after=${:.2} (change={:+.2}), reward={:+.2}, usage={:.2}",
                before.quota, after.quota, balance_change, check_in_reward, usage_increase,
            ));

            account_check_in_details.insert(account_key.clone(), CheckInDetail {
                name: account_name.clone(),
                user_info: user_info_summary.clone(),
                before_quota: before.quota,
                before_used: before.used_quota,
                after_quota: after.quota,
                after_used: after.used_quota,
                check_in_reward,
                usage_increase,
                balance_change,
                success: runner.success,
            });
        } else if let Some(after) = runner.after.as_ref() {
            current_balances.insert(account_key.clone(), after.quota);
            log::info_f(&account_name, &format!("Balance after check-in: ${:.2} (used ${:.2})", after.quota, after.used_quota));
        }

        if !runner.success {
            let mut item = format!("[FAIL] {}", account_name);
            if let Some(summary) = user_info_summary.as_ref() {
                item.push_str(&format!("\n👤 用户信息: {}", summary));
            }
            if let Some(err) = runner.error.as_ref() {
                item.push_str(&format!("\n{}", err));
            }
            if let Some(after) = runner.after.as_ref() {
                item.push_str(&format!("\n💵 当前余额: ${:.2} | 累计消耗: ${:.2}", after.quota, after.used_quota));
            }
            notification_content.push(item);
        }
    }

    // ========== 阶段 3: 余额变化检测 ==========
    log::phase("Phase 3: Balance Change Detection");

    let current_balance_hash = if !current_balances.is_empty() {
        Some(generate_balance_hash(&current_balances))
    } else {
        log::warn("No balance data available, skipping hash generation");
        None
    };

    if let Some(ref hash) = current_balance_hash {
        match &last_balance_hash {
            None => {
                balance_changed = true;
                need_notify = true;
                log::info("First run detected (no previous hash), will send notification with current balances");
            }
            Some(last) if last != hash => {
                balance_changed = true;
                need_notify = true;
                log::info(&format!(
                    "Balance changes detected: previous hash={}..., current hash={}...",
                    &last[..4.min(last.len())],
                    &hash[..4.min(hash.len())]
                ));
            }
            Some(_) => {
                log::info("No balance changes detected (hash unchanged)");
            }
        }
    }

    if balance_changed {
        log::info("Adding all account details to notification content...");
        for (i, _account) in accounts.iter().enumerate() {
            let account_key = format!("account_{}", i + 1);
            if let Some(detail) = account_check_in_details.get(&account_key) {
                let account_name = &detail.name;
                let account_result = format_check_in_notification(detail);
                if !notification_content.iter().any(|item| item.contains(account_name)) {
                    notification_content.push(account_result);
                    log::debug_f(account_name, "Added formatted notification to content");
                }
            }
        }
    }

    if let Some(ref hash) = current_balance_hash {
        save_balance_hash(hash);
    }

    // ========== 阶段 4: 发送通知 ==========
    log::phase("Phase 4: Notification");

    if need_notify && !notification_content.is_empty() {
        log::info(&format!("Notification triggered: {} content item(s) to send", notification_content.len()));

        let summary = vec![
            "[STATS] Check-in result statistics:".to_string(),
            format!("[SUCCESS] Success: {}/{}", success_count, total_count),
            format!("[FAIL] Failed: {}/{}", total_count - success_count, total_count),
            if success_count == total_count {
                "[SUCCESS] All accounts check-in successful!".to_string()
            } else if success_count > 0 {
                "[WARN] Some accounts check-in successful".to_string()
            } else {
                "[ERROR] All accounts check-in failed".to_string()
            },
        ];

        let time_info = format!("[TIME] Execution time: {}", Local::now().format("%Y-%m-%d %H:%M:%S"));

        let notify_content = format!(
            "{}\n\n{}\n\n{}",
            time_info,
            notification_content.join("\n\n"),
            summary.join("\n")
        );

        log::separator();
        log::raw(&notify_content);
        log::separator();

        let notify = NotificationKit::from_env();
        notify.push_message("AnyRouter Check-in Alert", &notify_content).await;
        log::success("Notification sent due to failures or balance changes");
    } else {
        log::info("All accounts successful and no balance changes detected, notification skipped");
    }

    // ========== 最终统计 ==========
    log::phase("Final Summary");
    log::info(&format!("Total accounts: {}", total_count));
    log::info(&format!("Successful:     {}/{}", success_count, total_count));
    log::info(&format!("Failed:         {}/{}", total_count - success_count, total_count));
    log::info(&format!("Balance changed: {}", if balance_changed { "Yes" } else { "No" }));
    log::info(&format!("Notified:       {}", if need_notify { "Yes" } else { "No" }));
    log::now_time(&format!("Program finished at {}", Local::now().format("%Y-%m-%d %H:%M:%S")));
    log::info(&format!("Total execution time: {:.1?}", program_start.elapsed()));

    if success_count > 0 {
        log::success("Program exited with code 0 (success)");
    } else {
        log::error("Program exited with code 1 (all accounts failed)");
    }

    std::process::exit(if success_count > 0 { 0 } else { 1 });
}
