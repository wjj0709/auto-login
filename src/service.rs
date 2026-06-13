use std::collections::HashMap;

use gpui::{App, WeakEntity};

use crate::app_state::{AppState, CheckInStatus, LogLevel};
use crate::config::{AccountConfig, ProviderConfig};
use crate::playwright::{self, PlaywrightResult};
use crate::storage::{account_to_legacy_config, site_to_provider_config, Site};

/// 签到范围:全部 / 单站点 / 单账户。
#[derive(Debug, Clone, Copy)]
pub enum CheckinScope {
    All,
    #[allow(dead_code)] // Milestone C「签到本站点」接入后使用
    Site(i64),
    Account(i64),
}

/// 触发签到(异步执行)。`scope` 决定参与的账户集合。
pub fn run_checkin(app_state: WeakEntity<AppState>, scope: CheckinScope, cx: &mut App) {
    // 标记为运行中
    let _ = app_state.update(cx, |state, cx| {
        if state.is_running {
            return;
        }
        state.is_running = true;
        state.reset_for_new_run();
        state.add_log(LogLevel::Info, "System", "开始签到...");
        cx.notify();
    });

    cx.spawn(async move |cx| {
        // 按 scope 选取账户,转换为旧 AccountConfig/ProviderConfig 喂给现有 Playwright 通路。
        // 协议 name 设为 account_id 字符串:展示名跨站点可能重复,id 才能无歧义回填结果。
        let prepared = app_state.read_with(cx, |state, _cx| {
            let site_by_id: HashMap<i64, &Site> = state.sites.iter().map(|s| (s.id, s)).collect();
            let mut dispatch: Vec<(i64, AccountConfig)> = Vec::new();
            let mut providers: HashMap<String, ProviderConfig> = HashMap::new();
            for acc in &state.accounts {
                let included = match scope {
                    CheckinScope::All => true,
                    CheckinScope::Site(sid) => acc.site_id == sid,
                    CheckinScope::Account(aid) => acc.id == aid,
                };
                if !included {
                    continue;
                }
                let Some(site) = site_by_id.get(&acc.site_id) else {
                    continue;
                };
                providers
                    .entry(site.name.clone())
                    .or_insert_with(|| site_to_provider_config(site));
                let mut cfg = account_to_legacy_config(acc, &site.name);
                cfg.name = Some(acc.id.to_string());
                dispatch.push((acc.id, cfg));
            }
            (dispatch, providers)
        });

        let Ok((dispatch, providers)) = prepared else {
            return;
        };

        if dispatch.is_empty() {
            let _ = app_state.update(cx, |state, cx| {
                state.is_running = false;
                state.add_log(LogLevel::Warn, "System", "没有匹配的账户可签到");
                cx.notify();
            });
            return;
        }

        // 逐账户标记为 Running
        for (id, _) in &dispatch {
            let id = *id;
            let _ = app_state.update(cx, |state, cx| {
                state.set_status(id, CheckInStatus::Running);
                let name = state
                    .account_by_id(id)
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| id.to_string());
                state.add_log(LogLevel::Info, &name, "签到开始");
                cx.notify();
            });
        }

        let configs: Vec<AccountConfig> = dispatch.iter().map(|(_, c)| c.clone()).collect();

        let _ = app_state.update(cx, |state, cx| {
            state.add_log(
                LogLevel::Info,
                "System",
                &format!("启动 Playwright,共 {} 个账户...", configs.len()),
            );
            cx.notify();
        });

        let result = playwright::run_checkin(&configs, &providers).await;

        match result {
            Ok(results) => {
                // 结果按协议 name(account_id 字符串)回填
                let mut result_map: HashMap<i64, PlaywrightResult> = HashMap::new();
                for r in results {
                    match r.name.parse::<i64>() {
                        Ok(id) => {
                            result_map.insert(id, r);
                        }
                        Err(_) => {
                            let _ = app_state.update(cx, |state, cx| {
                                state.add_log(
                                    LogLevel::Warn,
                                    "System",
                                    &format!("收到无法识别的结果标识: {}", r.name),
                                );
                                cx.notify();
                            });
                        }
                    }
                }

                for (id, _) in &dispatch {
                    let id = *id;
                    if let Some(r) = result_map.remove(&id) {
                        let success = r.success;
                        let err = r.error.clone();
                        let _ = app_state.update(cx, |state, cx| {
                            let name = state
                                .account_by_id(id)
                                .map(|a| a.name.clone())
                                .unwrap_or_else(|| id.to_string());
                            if success {
                                state.add_log(LogLevel::Success, &name, "签到成功");
                            } else {
                                let e = err.as_deref().unwrap_or("unknown error");
                                state.add_log(LogLevel::Error, &name, &format!("签到失败: {}", e));
                            }
                            state.process_result(id, r);
                            cx.notify();
                        });
                    } else {
                        let _ = app_state.update(cx, |state, cx| {
                            let name = state
                                .account_by_id(id)
                                .map(|a| a.name.clone())
                                .unwrap_or_else(|| id.to_string());
                            state.add_log(LogLevel::Error, &name, "签到器未返回结果");
                            state.set_status(id, CheckInStatus::Failed("无结果".into()));
                            state.fail_count += 1;
                            cx.notify();
                        });
                    }
                }
            }
            Err(e) => {
                let _ = app_state.update(cx, |state, cx| {
                    state.add_log(LogLevel::Error, "System", &format!("Playwright 运行失败: {}", e));
                    for (id, _) in &dispatch {
                        state.set_status(*id, CheckInStatus::Failed(e.clone()));
                        state.fail_count += 1;
                    }
                    cx.notify();
                });
            }
        }

        // 标记运行结束
        let _ = app_state.update(cx, |state, cx| {
            state.is_running = false;
            state.add_log(
                LogLevel::Info,
                "System",
                &format!("签到完成:成功 {},失败 {}", state.success_count, state.fail_count),
            );
            cx.notify();
        });
    })
    .detach();
}
