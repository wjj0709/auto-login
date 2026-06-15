// ============================================================================
// main.rs — AnyRouter 自动签到程序入口
// ============================================================================
// 功能：多账号自动签到 + 余额监控 + 余额变化通知
// 架构：Rust 主程序负责配置加载、流程编排、通知；
//       Python + Playwright 子脚本负责浏览器自动化签到
// 通信：主程序通过 stdin 传入 JSON，Python 子脚本通过 stdout 返回 JSON 结果
// ============================================================================

// 声明子模块，每个模块对应一个同名的 .rs 文件
#[macro_use]
mod macros;
mod balance; // 余额哈希检测模块
mod checkin; // 签到结果格式化模块
mod config; // 配置加载模块
mod log; // 日志工具模块
mod notify; // 邮件通知模块
mod playwright; // Playwright 子进程调用模块

use chrono::Local;
use std::collections::HashMap;
use std::time::Instant;

// 从各子模块引入需要的类型和函数
use balance::{generate_balance_hash, load_balance_hash, save_balance_hash};
use checkin::{
    format_check_in_email_report, format_cookie_report, format_user_info_summary,
    BalanceChangeSummary, BalanceSnapshot, CheckInReportItem,
};
use config::load_runtime_config;
use notify::NotificationKit;

/// 程序主入口函数
/// 使用 #[tokio::main] 宏将 async fn main 转为同步入口，
/// 以支持异步运行时（邮件发送等异步操作需要）
#[tokio::main]
async fn main() {
    // 记录程序启动时间，用于最终统计总执行时间
    let program_start = Instant::now();

    // 加载 .env 文件中的环境变量
    // dotenvy::dotenv() 会查找当前目录及父目录下的 .env 文件
    // .ok() 表示即使文件不存在也不报错（环境变量可能已在系统中配置）
    dotenvy::dotenv().ok();
    log::init_from_config_file();

    // ========== 程序启动日志 ==========
    log::phase("AnyRouter Auto Check-in Script (Rust + Playwright)");
    log::system("AnyRouter.top multi-account auto check-in script started");
    log::now_time(&format!(
        "Program started at {}",
        Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    log::separator();

    // ========== 阶段 1: 加载配置 ==========
    log::phase("Phase 1: Loading Configuration");

    // 打印 .env 中关键环境变量的配置情况
    // 敏感字段（如密码、token）只显示首尾几位，避免日志泄露
    log::info("===== .env 环境变量配置信息 =====");
    let env_keys = [
        ("ANYROUTER_ACCOUNTS", true), // 账号配置（敏感：包含 session cookie）
        ("ANYROUTER_CONFIG_FILE", false), // 配置文件路径
        ("PROVIDERS", false),         // 自定义 Provider 配置（非敏感）
        ("PLAYWRIGHT_HEADLESS", false), // 浏览器无头模式开关（非敏感）
        ("PYTHON_BIN", false),        // Python 解释器路径（非敏感）
        ("EMAIL_USER", true),         // 邮件账号（敏感）
        ("EMAIL_PASS", true),         // 邮件密码/授权码（敏感）
        ("EMAIL_TO", true),           // 通知收件人（敏感）
        ("EMAIL_SENDER", true),       // 邮件发件人（敏感）
        ("CUSTOM_SMTP_SERVER", false), // 自定义 SMTP 服务器（非敏感）
    ];
    for (key, is_sensitive) in &env_keys {
        match std::env::var(key) {
            Ok(val) => {
                // 敏感字段脱敏处理：只显示前4位和后4位
                let display_val = if *is_sensitive && val.len() > 8 {
                    format!(
                        "{}...{} (长度: {})",
                        &val[..4],
                        &val[val.len().saturating_sub(4)..],
                        val.len()
                    )
                } else if *is_sensitive {
                    // 敏感字段过短时完全隐藏
                    format!("*** (长度: {})", val.len())
                } else {
                    val.clone()
                };
                log::info(&format!("  {} = {}", key, display_val));
            }
            Err(_) => {
                // 环境变量未设置
                log::debug(&format!("  {} = (未设置)", key));
            }
        }
    }
    log::separator();

    // 加载运行时配置（Provider + Account），如果失败则直接退出程序
    let runtime_config = match load_runtime_config() {
        Ok(config) => config,
        Err(err) => {
            log::error(&format!("Unable to load runtime configuration: {}", err));
            std::process::exit(1);
        }
    };
    let config_file_path = runtime_config.config_file_path().clone();
    let app_config = runtime_config.app_config().clone();
    let accounts = runtime_config.accounts().clone();

    if let Some(path) = config_file_path.as_ref() {
        log::info(&format!("Active configuration file: {}", path.display()));
    }

    log::success(&format!(
        "Configuration loaded: {} provider(s), {} account(s)",
        app_config.providers().len(),
        accounts.len()
    ));

    // 加载上次保存的余额哈希值，用于后续比对余额是否变化
    let last_balance_hash = load_balance_hash();

    // ========== 阶段 2: 通过 Playwright 执行签到 ==========
    log::phase(&format!(
        "Phase 2: Processing {} Account(s) via Playwright",
        accounts.len()
    ));

    // 调用 Playwright 子进程执行签到
    // 内部会启动 Python 子进程，通过 stdin 传入账号数据，stdout 返回签到结果
    let runner_results = match playwright::run_checkin(&accounts, app_config.providers()).await {
        Ok(r) => r,
        Err(e) => {
            log::error(&format!("Playwright runner failed: {}", e));
            log::error("Hint: ensure dependencies installed -> pip3 install playwright && python3 -m playwright install chromium");
            let failed_items = accounts
                .iter()
                .enumerate()
                .map(|(i, account)| {
                    let mut item = CheckInReportItem::new(account.get_display_name(i), false);
                    item.set_error(Some(format!("Playwright runner failed: {}", e)));
                    item
                })
                .collect::<Vec<_>>();

            log::phase("Notification");
            send_check_in_email_report(&failed_items).await;
            log::flush();
            std::process::exit(1);
        }
    };

    // ======= 签到结果统计与处理 =======
    let mut success_count = 0usize; // 签到成功计数
    let total_count = accounts.len(); // 总账号数
    let mut current_balances: HashMap<String, f64> = HashMap::new(); // 当前各账号余额
    let mut email_report_items: Vec<CheckInReportItem> = Vec::with_capacity(accounts.len());
    let mut balance_changed = false; // 余额是否有变化

    // 将 Playwright 返回的结果按账号名称索引，方便后续按配置顺序匹配
    let mut result_by_name: HashMap<String, playwright::PlaywrightResult> = HashMap::new();
    for r in runner_results {
        result_by_name.insert(r.name().clone(), r);
    }

    // 遍历所有账号，按配置顺序处理签到结果
    for (i, account) in accounts.iter().enumerate() {
        // 获取账号显示名称（优先使用配置中的 name 字段，否则使用 "Account N"）
        let account_name = account.get_display_name(i);
        let account_key = format!("account_{}", i + 1);
        log::info(&format!(
            "===== Account {}/{}: {} =====",
            i + 1,
            total_count,
            account_name
        ));

        // 从结果映射中取出该账号的签到结果
        let runner = match result_by_name.remove(&account_name) {
            Some(r) => r,
            None => {
                // Playwright 没有返回该账号的结果（理论上不应发生）
                log::error_f(&account_name, "No result returned by Playwright runner");
                let mut report_item = CheckInReportItem::new(account_name.clone(), false);
                report_item.set_error(Some("No result returned by Playwright runner".to_string()));
                email_report_items.push(report_item);
                continue;
            }
        };

        // 判断是否使用了账密登录（cookie 失效时回退登录）
        if runner.used_login() {
            log::info_f(
                &account_name,
                "Logged in via username/password (cookie was missing or expired)",
            );
        }
        if let Some(cookie_report) = runner.cookie_report().as_ref() {
            if let Some(cookie_summary) = format_cookie_report(cookie_report) {
                for line in cookie_summary.lines() {
                    log::info_f(&account_name, line);
                }
            }
        }

        // 格式化用户信息摘要（用于日志和通知）
        let user_info_summary = format_user_info_summary(runner.user_info().as_ref());
        if let Some(summary) = user_info_summary.as_ref() {
            log::info_f(&account_name, &format!("User info: {}", summary));
        }

        // 统计签到成功/失败
        if runner.success() {
            success_count += 1;
            log::success(&format!(
                "Account {}/{} [{}]: check-in SUCCEEDED",
                i + 1,
                total_count,
                account_name
            ));
        } else {
            log::error(&format!(
                "Account {}/{} [{}]: check-in FAILED ({})",
                i + 1,
                total_count,
                account_name,
                runner
                    .error()
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string())
            ));
        }

        let mut report_item = CheckInReportItem::new(account_name.clone(), runner.success());
        report_item
            .set_user_info(user_info_summary.clone())
            .set_error(runner.error().clone());

        // 处理余额信息：计算签到奖励、余额变化等
        if let (Some(before), Some(after)) = (runner.before().as_ref(), runner.after().as_ref()) {
            // 记录当前余额到 HashMap，用于后续生成余额哈希
            current_balances.insert(account_key.clone(), after.quota());

            // 计算各项余额指标
            let total_before = before.quota() + before.used_quota(); // 签到前总额
            let total_after = after.quota() + after.used_quota(); // 签到后总额
            let check_in_reward = total_after - total_before; // 签到获得的奖励额度
            let usage_increase = after.used_quota() - before.used_quota(); // 签到期间的消耗
            let balance_change = after.quota() - before.quota(); // 余额净变化

            log::info_f(&account_name, &format!(
                "Balance: before=${:.2} -> after=${:.2} (change={:+.2}), reward={:+.2}, usage={:.2}",
                before.quota(), after.quota(), balance_change, check_in_reward, usage_increase,
            ));

            report_item
                .set_before(Some(BalanceSnapshot::new(
                    before.quota(),
                    before.used_quota(),
                )))
                .set_after(Some(BalanceSnapshot::new(
                    after.quota(),
                    after.used_quota(),
                )))
                .set_balance_change(Some(BalanceChangeSummary::new(
                    check_in_reward,
                    usage_increase,
                    balance_change,
                )));
        } else if let Some(after) = runner.after().as_ref() {
            // 只有签到后的余额（通常表示签到前查询失败）
            current_balances.insert(account_key.clone(), after.quota());
            report_item.set_after(Some(BalanceSnapshot::new(
                after.quota(),
                after.used_quota(),
            )));
            log::info_f(
                &account_name,
                &format!(
                    "Balance after check-in: ${:.2} (used ${:.2})",
                    after.quota(),
                    after.used_quota()
                ),
            );
        } else if let Some(before) = runner.before().as_ref() {
            report_item.set_before(Some(BalanceSnapshot::new(
                before.quota(),
                before.used_quota(),
            )));
        }

        email_report_items.push(report_item);
    }

    // ========== 阶段 3: 余额变化检测 ==========
    log::phase("Phase 3: Balance Change Detection");

    // 根据所有账号的当前余额生成哈希值
    let current_balance_hash = if !current_balances.is_empty() {
        Some(generate_balance_hash(&current_balances))
    } else {
        // 没有余额数据时跳过哈希生成
        log::warn("No balance data available, skipping hash generation");
        None
    };

    // 与上次保存的哈希值比对，判断余额是否变化
    if let Some(ref hash) = current_balance_hash {
        match &last_balance_hash {
            // 首次运行，没有历史哈希
            None => {
                balance_changed = true;
                log::info("First run detected (no previous hash), will send notification with current balances");
            }
            // 哈希不同，余额发生了变化
            Some(last) if last != hash => {
                balance_changed = true;
                log::info(&format!(
                    "Balance changes detected: previous hash={}..., current hash={}...",
                    &last[..4.min(last.len())],
                    &hash[..4.min(hash.len())]
                ));
            }
            // 哈希相同，余额无变化
            Some(_) => {
                log::info("No balance changes detected (hash unchanged)");
            }
        }
    }

    // 保存当前余额哈希到文件，供下次比对
    if let Some(ref hash) = current_balance_hash {
        save_balance_hash(hash);
    }

    // ========== 阶段 4: 发送通知 ==========
    log::phase("Phase 4: Notification");

    send_check_in_email_report(&email_report_items).await;
    log::success("Notification attempted after check-in completion");

    // ========== 最终统计 ==========
    log::phase("Final Summary");
    log::info(&format!("Total accounts: {}", total_count));
    log::info(&format!(
        "Successful:     {}/{}",
        success_count, total_count
    ));
    log::info(&format!(
        "Failed:         {}/{}",
        total_count - success_count,
        total_count
    ));
    log::info(&format!(
        "Balance changed: {}",
        if balance_changed { "Yes" } else { "No" }
    ));
    log::info("Notified:       Yes (attempted)");
    log::now_time(&format!(
        "Program finished at {}",
        Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    log::info(&format!(
        "Total execution time: {:.1?}",
        program_start.elapsed()
    ));

    // 退出码：至少一个账号成功返回 0，全部失败返回 1
    if success_count > 0 {
        log::success("Program exited with code 0 (success)");
    } else {
        log::error("Program exited with code 1 (all accounts failed)");
    }
    log::flush();

    std::process::exit(if success_count > 0 { 0 } else { 1 });
}

async fn send_check_in_email_report(items: &[CheckInReportItem]) {
    let executed_at = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let notify_content = format_check_in_email_report(&executed_at, items);

    log::separator();
    log::raw(&notify_content);
    log::separator();

    let notify = NotificationKit::from_env();
    notify
        .push_message("AnyRouter Check-in Report", &notify_content)
        .await;
}
