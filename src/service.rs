use std::collections::HashMap;

use gpui::{App, WeakEntity};

use crate::app_state::{AppState, CheckInStatus, LogLevel};
use crate::config::{AccountConfig, ProviderConfig};
use crate::cookie;
use crate::playwright::{self, CookieEntry, PlaywrightResult, RunOutput};
use crate::storage::{account_to_legacy_config, site_to_provider_config, AccountInput, Site};

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
            Ok(RunOutput { results, stderr_lines }) => {
                // 将子进程 API 调用日志输出到 UI
                for line in &stderr_lines {
                    let _ = app_state.update(cx, |state, cx| {
                        state.add_log(LogLevel::Info, "Playwright", line);
                        cx.notify();
                    });
                }

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
                        let cookies = r.cookies.clone();
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
                            // 顺带续期:回传 cookie 的 session 变化时落库
                            persist_cookies_into_state(state, id, &cookies, false);
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

/// 账密登录获取 / 刷新单个账户的 Cookie(详情页「账密登录刷新」)。
pub fn run_login(app_state: WeakEntity<AppState>, account_id: i64, cx: &mut App) {
    let _ = app_state.update(cx, |state, cx| {
        state.set_status(account_id, CheckInStatus::Running);
        let name = state.account_by_id(account_id).map(|a| a.name.clone()).unwrap_or_default();
        state.add_log(LogLevel::Info, &name, "账密登录中...");
        cx.notify();
    });

    cx.spawn(async move |cx| {
        let prepared = app_state.read_with(cx, |state, _cx| {
            let acc = state.account_by_id(account_id)?;
            if acc.username.is_none() || acc.password.is_none() {
                return None;
            }
            let site = state.site_by_id(acc.site_id)?;
            let mut cfg = account_to_legacy_config(acc, &site.name);
            cfg.name = Some(account_id.to_string());
            let mut providers: HashMap<String, ProviderConfig> = HashMap::new();
            providers.insert(site.name.clone(), site_to_provider_config(site));
            Some((cfg, providers))
        });

        let Ok(Some((cfg, providers))) = prepared else {
            let _ = app_state.update(cx, |state, cx| {
                state.set_status(account_id, CheckInStatus::Failed("缺少账号密码".into()));
                let name = state.account_by_id(account_id).map(|a| a.name.clone()).unwrap_or_default();
                state.add_log(LogLevel::Error, &name, "账密登录需要先填写用户名和密码");
                cx.notify();
            });
            return;
        };

        let result = playwright::run_login(&[cfg], &providers).await;

        // 输出子进程 API 日志到 UI
        if let Ok(ref output) = result {
            for line in &output.stderr_lines {
                let _ = app_state.update(cx, |state, cx| {
                    state.add_log(LogLevel::Info, "Playwright", line);
                    cx.notify();
                });
            }
        }

        let _ = app_state.update(cx, |state, cx| {
            let name = state.account_by_id(account_id).map(|a| a.name.clone()).unwrap_or_default();
            match result {
                Ok(RunOutput { mut results, .. }) => match results.pop() {
                    Some(r) if r.success => {
                        let cookies = r.cookies.clone();
                        state.set_status(account_id, CheckInStatus::Success);
                        persist_cookies_into_state(state, account_id, &cookies, true);
                        state.add_log(LogLevel::Success, &name, "账密登录成功,已更新 Cookie");
                    }
                    Some(r) => {
                        let e = r.error.unwrap_or_else(|| "登录失败".into());
                        state.set_status(account_id, CheckInStatus::Failed(e.clone()));
                        state.add_log(LogLevel::Error, &name, &format!("账密登录失败: {}", e));
                    }
                    None => {
                        state.set_status(account_id, CheckInStatus::Failed("无结果".into()));
                        state.add_log(LogLevel::Error, &name, "登录器未返回结果");
                    }
                },
                Err(e) => {
                    state.set_status(account_id, CheckInStatus::Failed(e.clone()));
                    state.add_log(LogLevel::Error, &name, &format!("登录器运行失败: {}", e));
                }
            }
            cx.notify();
        });
    })
    .detach();
}

/// 回传 cookie 数组 → 结构化 JSON 数组(保留真实 expires,供存档)。
fn build_cookie_entries(cookies: &[CookieEntry]) -> Vec<serde_json::Value> {
    cookies
        .iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name, "value": c.value,
                "domain": c.domain, "path": c.path,
                "expires": c.expires, "httpOnly": c.http_only, "secure": c.secure,
            })
        })
        .collect()
}

fn session_value(cookies: &[CookieEntry]) -> Option<String> {
    cookies.iter().find(|c| c.name == "session").map(|c| c.value.clone())
}

fn session_expires_at(cookies: &[CookieEntry]) -> Option<String> {
    cookies
        .iter()
        .find(|c| c.name == "session")
        .and_then(|c| cookie::expires_to_iso(c.expires))
}

/// 把回传 cookie 落库并更新内存中的账户。
///
/// `force=false`(签到顺带续期):仅当 session 值变化时更新,避免 WAF cookie 抖动反复改写签发时间;
/// `force=true`(账密登录):只要回传含 session 即更新。
fn persist_cookies_into_state(
    state: &mut AppState,
    account_id: i64,
    cookies: &[CookieEntry],
    force: bool,
) {
    let new_session = match session_value(cookies) {
        Some(v) => v,
        None => return, // 无 session,不动现有 cookie
    };

    let Some(acc) = state.account_by_id(account_id) else {
        return;
    };
    if !force {
        let old_map = acc
            .cookies_json
            .as_deref()
            .map(crate::storage::cookie_entries_to_map)
            .unwrap_or_default();
        let old_session = old_map.get("session").and_then(|v| v.as_str());
        if old_session == Some(new_session.as_str()) {
            return; // session 未变化,跳过
        }
    }

    let entries = build_cookie_entries(cookies);
    let cookies_json = serde_json::to_string(&entries).ok();
    let expires_at = session_expires_at(cookies);
    let issued_at = crate::storage::now_iso();
    let input = AccountInput {
        site_id: acc.site_id,
        name: acc.name.clone(),
        api_user: acc.api_user.clone(),
        username: acc.username.clone(),
        password: acc.password.clone(),
        cookies_json: cookies_json.clone(),
        cookie_issued_at: Some(issued_at.clone()),
        cookie_expires_at: expires_at.clone(),
    };

    let write_ok = state
        .db
        .lock()
        .ok()
        .map(|guard| guard.update_account(account_id, &input).is_ok())
        .unwrap_or(false);

    if let Some(a) = state.accounts.iter_mut().find(|a| a.id == account_id) {
        a.cookies_json = cookies_json;
        a.cookie_issued_at = Some(issued_at);
        a.cookie_expires_at = expires_at;
    }
    if !write_ok {
        state.add_log(LogLevel::Warn, "System", "Cookie 续期写库失败");
    }
}

/// 拉取账户详情(tokens / logs / chart),写入 account_cache 并更新 detail_cache。
pub fn run_fetch_detail(app_state: WeakEntity<AppState>, account_id: i64, cx: &mut App) {
    let _ = app_state.update(cx, |state, cx| {
        state.fetching_detail = Some(account_id);
        let name = state.account_by_id(account_id).map(|a| a.name.clone()).unwrap_or_default();
        state.add_log(LogLevel::Info, &name, "刷新详情数据中...");
        cx.notify();
    });

    cx.spawn(async move |cx| {
        let prepared = app_state.read_with(cx, |state, _cx| {
            let acc = state.account_by_id(account_id)?;
            let site = state.site_by_id(acc.site_id)?;
            let mut cfg = account_to_legacy_config(acc, &site.name);
            cfg.name = Some(account_id.to_string());
            let mut providers: HashMap<String, ProviderConfig> = HashMap::new();
            providers.insert(site.name.clone(), site_to_provider_config(site));
            Some((cfg, providers))
        });

        let Ok(Some((cfg, providers))) = prepared else {
            let _ = app_state.update(cx, |state, cx| {
                state.fetching_detail = None;
                cx.notify();
            });
            return;
        };

        let result = playwright::run_fetch_detail(&[cfg], &providers).await;

        // 输出子进程 API 日志到 UI
        if let Ok(ref output) = result {
            for line in &output.stderr_lines {
                let _ = app_state.update(cx, |state, cx| {
                    state.add_log(LogLevel::Info, "Playwright", line);
                    cx.notify();
                });
            }
        }

        let _ = app_state.update(cx, |state, cx| {
            state.fetching_detail = None;
            let name = state.account_by_id(account_id).map(|a| a.name.clone()).unwrap_or_default();
            match result {
                Ok(RunOutput { mut results, .. }) => match results.pop() {
                    Some(r) if r.success => {
                        let tokens_json = serde_json::to_string(&r.tokens).unwrap_or_else(|_| "[]".into());
                        let logs_json = serde_json::to_string(&r.logs).unwrap_or_else(|_| "[]".into());
                        let chart_json = serde_json::to_string(&r.chart).unwrap_or_else(|_| "[]".into());
                        // tokens 含完整密钥,加密存储;logs / chart 明文
                        if let Ok(guard) = state.db.lock() {
                            let _ = guard.set_cache(account_id, "tokens", &tokens_json, true);
                            let _ = guard.set_cache(account_id, "logs", &logs_json, false);
                            let _ = guard.set_cache(account_id, "chart", &chart_json, false);
                        }
                        let cookies = r.cookies.clone();
                        persist_cookies_into_state(state, account_id, &cookies, false);
                        state.set_detail(
                            account_id,
                            crate::app_state::DetailData {
                                tokens_json,
                                logs_json,
                                chart_json,
                                fetched_at: crate::storage::now_iso(),
                            },
                        );
                        state.add_log(LogLevel::Success, &name, "详情数据已更新");
                    }
                    Some(r) => {
                        let e = r.error.unwrap_or_else(|| "拉取失败".into());
                        state.add_log(LogLevel::Error, &name, &format!("详情拉取失败: {}", e));
                    }
                    None => {
                        state.add_log(LogLevel::Error, &name, "拉取器未返回结果");
                    }
                },
                Err(e) => {
                    state.add_log(LogLevel::Error, &name, &format!("拉取器运行失败: {}", e));
                }
            }
            cx.notify();
        });
    })
    .detach();
}
