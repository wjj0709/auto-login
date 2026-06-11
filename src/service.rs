use std::collections::HashMap;
use gpui::{App, Entity, WeakEntity};

use crate::app_state::{AppState, CheckInStatus, LogLevel};
use crate::config::{AccountConfig, AppConfig};
use crate::playwright;

/// 触发全量签到（异步执行）
pub fn run_checkin_all(
    app_state: WeakEntity<AppState>,
    cx: &mut App,
) {
    // 标记为运行中
    let _ = app_state.update(cx, |state, cx| {
        if state.is_running {
            return;
        }
        state.is_running = true;
        state.reset_for_new_run();
        state.add_log(LogLevel::Info, "System", "Starting check-in for all accounts...");
        cx.notify();
    });

    cx.spawn(async move |cx| {
        // 加载配置
        let (accounts, providers) = {
            let state = app_state.read_with(cx, |state, _cx| state.accounts.clone());
            match state {
                Ok(accounts) => {
                    let config = AppConfig::load_from_env();
                    (accounts, config.providers)
                }
                Err(_) => return,
            }
        };

        // 逐账号标记为 Running
        for (i, account) in accounts.iter().enumerate() {
            let name = account.get_display_name(i);
            let _ = app_state.update(cx, |state, cx| {
                state.set_status(&name, CheckInStatus::Running);
                state.add_log(LogLevel::Info, &name, "Check-in started");
                cx.notify();
            }).ok();
        }

        // 调用 Playwright 执行签到
        let _ = app_state.update(cx, |state, cx| {
            state.add_log(LogLevel::Info, "System",
                &format!("Launching Playwright for {} account(s)...", accounts.len()));
            cx.notify();
        }).ok();

        let result = playwright::run_checkin(&accounts, &providers).await;

        match result {
            Ok(results) => {
                // 逐个处理结果
                let mut result_map: HashMap<String, playwright::PlaywrightResult> = HashMap::new();
                for r in results {
                    result_map.insert(r.name.clone(), r);
                }

                for (i, account) in accounts.iter().enumerate() {
                    let name = account.get_display_name(i);
                    if let Some(r) = result_map.remove(&name) {
                        let success = r.success;
                        let _ = app_state.update(cx, |state, cx| {
                            if success {
                                state.add_log(LogLevel::Success, &name, "Check-in succeeded");
                            } else {
                                let err = r.error.as_deref().unwrap_or("unknown error");
                                state.add_log(LogLevel::Error, &name, &format!("Check-in failed: {}", err));
                            }
                            state.process_result(&name, r);
                            cx.notify();
                        }).ok();
                    } else {
                        let _ = app_state.update(cx, |state, cx| {
                            state.add_log(LogLevel::Error, &name, "No result from runner");
                            state.set_status(&name, CheckInStatus::Failed("No result".into()));
                            state.fail_count += 1;
                            cx.notify();
                        }).ok();
                    }
                }
            }
            Err(e) => {
                let _ = app_state.update(cx, |state, cx| {
                    state.add_log(LogLevel::Error, "System", &format!("Playwright runner failed: {}", e));
                    // 标记所有为失败
                    for (i, account) in accounts.iter().enumerate() {
                        let name = account.get_display_name(i);
                        state.set_status(&name, CheckInStatus::Failed(e.clone()));
                        state.fail_count += 1;
                    }
                    cx.notify();
                }).ok();
            }
        }

        // 标记运行结束
        let _ = app_state.update(cx, |state, cx| {
            state.is_running = false;
            state.add_log(LogLevel::Info, "System",
                &format!("Check-in complete: {} succeeded, {} failed", state.success_count, state.fail_count));
            cx.notify();
        }).ok();
    }).detach();
}
