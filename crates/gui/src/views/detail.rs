use gpui::{AnyElement, Context, Entity, IntoElement, ParentElement, Styled, div, prelude::*, px};

use anyrouter_core::models::{Account, CacheKind};
use crate::app_state::{AppState, ViewKind};
use crate::theme;
use crate::views::root::RootView;

pub fn render(
    state: Entity<AppState>,
    account_id: i64,
    cx: &mut Context<RootView>,
) -> AnyElement {
    let snap = state.read(cx);
    let active_tab = snap.active_tab;

    // 从 storage 拉取当前账户与站点信息
    let (account_opt, site_name) = if let Some(ref storage) = snap.storage {
        let account = storage.get_account(account_id).ok().flatten();
        let site_name = account
            .as_ref()
            .and_then(|a| storage.get_site(a.site_id).ok().flatten())
            .map(|s| s.name)
            .unwrap_or_else(|| "未知站点".into());
        (account, site_name)
    } else {
        (None, "未连接数据库".into())
    };

    let account_display = account_opt
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| format!("账户 #{}", account_id));

    // 拉取四类缓存
    let (overview_cache, tokens_cache, logs_cache, chart_cache) =
        if let (Some(ref storage), Some(_)) = (snap.storage.as_ref(), account_opt.as_ref()) {
            (
                storage
                    .get_cache(account_id, CacheKind::Overview)
                    .ok()
                    .flatten(),
                storage
                    .get_cache(account_id, CacheKind::Tokens)
                    .ok()
                    .flatten(),
                storage
                    .get_cache(account_id, CacheKind::Logs)
                    .ok()
                    .flatten(),
                storage
                    .get_cache(account_id, CacheKind::Chart)
                    .ok()
                    .flatten(),
            )
        } else {
            (None, None, None, None)
        };

    let last_updated = match active_tab {
        0 => overview_cache.as_ref().map(|c| c.fetched_at.clone()),
        1 => tokens_cache.as_ref().map(|c| c.fetched_at.clone()),
        2 => logs_cache.as_ref().map(|c| c.fetched_at.clone()),
        3 => chart_cache.as_ref().map(|c| c.fetched_at.clone()),
        _ => None,
    };

    let state_back = state.clone();
    let state_for_tabs = state.clone();
    let state_for_refresh = state.clone();
    let account_for_tab = account_opt.clone();

    div()
        .id("detail-scroll")
        .size_full()
        .overflow_y_scroll()
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        // 面包屑 + 操作按钮
        .child(render_header(
            state_back,
            state_for_refresh,
            account_id,
            account_display.clone(),
            site_name,
            last_updated,
        ))
        // Tab 栏
        .child(render_tabs(active_tab, state_for_tabs))
        // Tab 内容
        .child(match active_tab {
            0 => render_overview_tab(account_for_tab, overview_cache),
            1 => render_tokens_tab(tokens_cache),
            2 => render_logs_tab(logs_cache),
            3 => render_chart_tab(chart_cache),
            _ => render_overview_tab(account_for_tab, overview_cache),
        })
        .into_any_element()
}

fn render_header(
    state: Entity<AppState>,
    state_refresh: Entity<AppState>,
    account_id: i64,
    account_name: String,
    site_name: String,
    last_updated: Option<String>,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_color(theme::text_muted())
                .text_size(px(11.0))
                .child(
                    div()
                        .id("back-home")
                        .cursor_pointer()
                        .hover(|this| this.text_color(theme::accent_blue()))
                        .child("◀ 返回主页")
                        .on_click(move |_, _w, cx| {
                            state.update(cx, |st, cx| {
                                st.current_view = ViewKind::Home;
                                st.active_tab = 0;
                                cx.notify();
                            });
                        }),
                )
                .child(
                    div()
                        .text_color(theme::text_primary())
                        .child(format!("{} @ {}", account_name, site_name)),
                ),
        )
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .text_size(px(10.0))
                .text_color(theme::text_weakest())
                .child(div().child(match last_updated {
                    Some(t) => format!("上次更新 {}", t.split('T').next().unwrap_or(&t)),
                    None => "尚未拉取".to_string(),
                }))
                .child(
                    div()
                        .id("detail-refresh")
                        .px(px(10.0))
                        .py(px(3.0))
                        .bg(theme::btn_primary_bg())
                        .border_1()
                        .border_color(theme::btn_primary_border())
                        .rounded(px(5.0))
                        .text_color(theme::accent_blue())
                        .text_size(px(10.0))
                        .cursor_pointer()
                        .hover(|this| this.opacity(0.85))
                        .child("↻ 刷新数据")
                        .on_click(move |_, _w, cx| {
                            crate::views::root::trigger_fetch_detail(
                                state_refresh.clone(),
                                account_id,
                                cx,
                            );
                        }),
                ),
        )
        .into_any_element()
}

fn render_tabs(active: usize, state: Entity<AppState>) -> AnyElement {
    let labels = ["概览", "API 密钥", "使用日志", "消耗图表"];

    let mut row = div()
        .flex()
        .items_center()
        .gap(px(20.0))
        .border_b_1()
        .border_color(theme::border_normal())
        .pb(px(6.0));

    for (idx, label) in labels.iter().enumerate() {
        let is_active = idx == active;
        let state_click = state.clone();
        row = row.child(
            div()
                .id(("tab", idx))
                .px(px(2.0))
                .pb(px(4.0))
                .text_size(px(11.0))
                .text_color(if is_active {
                    theme::accent_blue()
                } else {
                    theme::text_muted()
                })
                .when(is_active, |this| {
                    this.border_b_2().border_color(theme::accent_blue())
                })
                .cursor_pointer()
                .hover(|this| this.text_color(theme::text_primary()))
                .child(*label)
                .on_click(move |_, _w, cx| {
                    state_click.update(cx, |st, cx| {
                        st.active_tab = idx;
                        cx.notify();
                    });
                }),
        );
    }

    row.into_any_element()
}

fn empty_card(text: &str) -> AnyElement {
    div()
        .w_full()
        .p(px(16.0))
        .bg(theme::bg_card())
        .border_1()
        .border_color(theme::border_normal())
        .rounded(px(8.0))
        .text_color(theme::text_weakest())
        .text_size(px(11.0))
        .child(text.to_string())
        .into_any_element()
}

fn info_card(content: AnyElement) -> AnyElement {
    div()
        .w_full()
        .p(px(12.0))
        .bg(theme::bg_card())
        .border_1()
        .border_color(theme::border_normal())
        .rounded(px(8.0))
        .child(content)
        .into_any_element()
}

fn render_overview_tab(
    account: Option<Account>,
    cache: Option<anyrouter_core::models::AccountCache>,
) -> AnyElement {
    let mut quota = "—".to_string();
    let mut used_quota = "—".to_string();
    let mut user_id = "—".to_string();
    let mut user_email = "—".to_string();
    let mut user_group = "—".to_string();

    if let Some(c) = cache.as_ref() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&c.payload_json) {
            if let Some(q) = v.get("quota").and_then(|x| x.as_f64()) {
                quota = format!("${:.2}", q / 500_000.0);
            }
            if let Some(u) = v.get("used_quota").and_then(|x| x.as_f64()) {
                used_quota = format!("${:.2}", u / 500_000.0);
            }
            if let Some(id) = v.get("id").and_then(|x| x.as_i64()) {
                user_id = id.to_string();
            }
            if let Some(e) = v.get("email").and_then(|x| x.as_str()) {
                user_email = e.to_string();
            }
            if let Some(g) = v.get("group").and_then(|x| x.as_str()) {
                user_group = g.to_string();
            }
        }
    }

    let cookie_status = match account.as_ref() {
        Some(a) => match (&a.cookie_issued_at, &a.cookie_expires_at) {
            (Some(issued), Some(exp)) => format!(
                "🍪 Cookie 生效 {} / 过期 {}",
                issued.split('T').next().unwrap_or(issued),
                exp.split('T').next().unwrap_or(exp),
            ),
            (Some(issued), None) => format!(
                "🍪 Cookie 录入于 {}（有效期未知）",
                issued.split('T').next().unwrap_or(issued)
            ),
            (None, _) => "🍪 暂无 Cookie".to_string(),
        },
        None => "🍪 未加载账户".to_string(),
    };

    let api_user = account
        .as_ref()
        .map(|a| a.api_user.clone())
        .unwrap_or_else(|| "—".into());

    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(info_card(
            div()
                .flex()
                .gap(px(28.0))
                .text_size(px(12.0))
                .text_color(theme::text_secondary())
                .child(stat_pair("💵 余额", quota))
                .child(stat_pair("📊 累计消耗", used_quota))
                .child(stat_pair("🆔 API 用户", api_user))
                .into_any_element(),
        ))
        .child(info_card(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .text_size(px(11.0))
                .text_color(theme::text_secondary())
                .child(div().child(cookie_status))
                .child(
                    div()
                        .text_color(theme::text_weakest())
                        .child("如需手动续期 Cookie，请在弹窗中执行账密登录"),
                )
                .into_any_element(),
        ))
        .child(info_card(
            div()
                .flex()
                .gap(px(28.0))
                .text_size(px(11.0))
                .text_color(theme::text_secondary())
                .child(stat_pair("👤 ID", user_id))
                .child(stat_pair("✉ 邮箱", user_email))
                .child(stat_pair("分组", user_group))
                .into_any_element(),
        ))
        .into_any_element()
}

fn stat_pair(label: &str, value: String) -> AnyElement {
    div()
        .flex()
        .gap(px(6.0))
        .child(
            div()
                .text_color(theme::text_muted())
                .child(label.to_string()),
        )
        .child(
            div()
                .text_color(theme::text_primary())
                .child(value),
        )
        .into_any_element()
}

fn render_tokens_tab(cache: Option<anyrouter_core::models::AccountCache>) -> AnyElement {
    let Some(c) = cache else {
        return empty_card("尚未拉取 API 密钥列表，点击右上角「↻ 刷新数据」获取");
    };

    let Ok(arr) = serde_json::from_str::<serde_json::Value>(&c.payload_json) else {
        return empty_card("API 密钥数据解析失败");
    };

    let items: Vec<serde_json::Value> = arr.as_array().cloned().unwrap_or_default();
    if items.is_empty() {
        return empty_card("暂无 API 密钥");
    }

    let mut col = div().flex().flex_col().gap(px(6.0));
    for item in items.into_iter().take(20) {
        let name = item
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("(未命名)")
            .to_string();
        let key_raw = item
            .get("key")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let masked = mask_key(&key_raw);
        let status_text = if item
            .get("status")
            .and_then(|x| x.as_i64())
            .map(|s| s == 1)
            .unwrap_or(true)
        {
            "正常"
        } else {
            "禁用"
        };
        col = col.child(
            div()
                .w_full()
                .px(px(12.0))
                .py(px(8.0))
                .bg(theme::bg_card())
                .border_1()
                .border_color(theme::border_normal())
                .rounded(px(6.0))
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(11.0))
                .child(
                    div()
                        .flex()
                        .gap(px(12.0))
                        .child(div().text_color(theme::text_primary()).child(name))
                        .child(
                            div()
                                .text_color(theme::text_weakest())
                                .child(masked),
                        ),
                )
                .child(
                    div()
                        .text_color(theme::text_muted())
                        .child(status_text.to_string()),
                ),
        );
    }
    col.into_any_element()
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        return key.to_string();
    }
    let head = &key[..4];
    let tail = &key[key.len() - 4..];
    format!("{}…{}", head, tail)
}

fn render_logs_tab(cache: Option<anyrouter_core::models::AccountCache>) -> AnyElement {
    let Some(c) = cache else {
        return empty_card("尚未拉取使用日志，点击「↻ 刷新数据」获取");
    };

    let Ok(arr) = serde_json::from_str::<serde_json::Value>(&c.payload_json) else {
        return empty_card("使用日志数据解析失败");
    };

    let items: Vec<serde_json::Value> = arr.as_array().cloned().unwrap_or_default();
    if items.is_empty() {
        return empty_card("暂无使用日志");
    }

    let mut col = div().flex().flex_col().gap(px(4.0));
    for item in items.into_iter().take(50) {
        let model = item
            .get("model_name")
            .and_then(|x| x.as_str())
            .unwrap_or("?")
            .to_string();
        let prompt_tokens = item.get("prompt_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
        let completion = item
            .get("completion_tokens")
            .and_then(|x| x.as_i64())
            .unwrap_or(0);
        let quota = item.get("quota").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let created_at = item.get("created_at").and_then(|x| x.as_i64()).unwrap_or(0);
        let ts_str = if created_at > 0 {
            chrono::DateTime::from_timestamp(created_at, 0)
                .map(|d| d.format("%m-%d %H:%M").to_string())
                .unwrap_or_else(|| created_at.to_string())
        } else {
            "—".to_string()
        };
        col = col.child(
            div()
                .w_full()
                .px(px(10.0))
                .py(px(4.0))
                .bg(theme::bg_card())
                .border_1()
                .border_color(theme::border_normal())
                .rounded(px(4.0))
                .flex()
                .gap(px(16.0))
                .text_size(px(10.0))
                .text_color(theme::text_secondary())
                .child(div().w(px(80.0)).text_color(theme::text_muted()).child(ts_str))
                .child(div().w(px(140.0)).child(model))
                .child(
                    div()
                        .w(px(100.0))
                        .text_color(theme::text_weakest())
                        .child(format!("in:{} out:{}", prompt_tokens, completion)),
                )
                .child(
                    div()
                        .text_color(theme::accent_blue())
                        .child(format!("${:.4}", quota / 500_000.0)),
                ),
        );
    }
    col.into_any_element()
}

fn render_chart_tab(cache: Option<anyrouter_core::models::AccountCache>) -> AnyElement {
    let Some(c) = cache else {
        return empty_card("尚未拉取图表数据，点击「↻ 刷新数据」获取");
    };

    let Ok(arr) = serde_json::from_str::<serde_json::Value>(&c.payload_json) else {
        return empty_card("图表数据解析失败");
    };

    let items: Vec<serde_json::Value> = arr.as_array().cloned().unwrap_or_default();
    if items.is_empty() {
        return empty_card("近 7 天暂无消耗数据");
    }

    // 从数据中提取每日 quota，绘制柱状图
    let mut daily: Vec<(String, f64)> = Vec::new();
    for item in items.iter() {
        let day = item
            .get("day")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let quota = item.get("quota").and_then(|x| x.as_f64()).unwrap_or(0.0);
        if !day.is_empty() {
            daily.push((day, quota));
        }
    }
    let max_q = daily.iter().map(|(_, q)| *q).fold(1.0_f64, f64::max);

    let mut bars = div()
        .flex()
        .items_end()
        .gap(px(8.0))
        .h(px(160.0))
        .p(px(12.0));
    for (day, q) in daily.iter().take(14) {
        let pct = (q / max_q * 140.0).clamp(2.0, 140.0) as f32;
        bars = bars.child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.0))
                .child(
                    div()
                        .w(px(28.0))
                        .h(px(pct))
                        .bg(theme::accent_blue())
                        .rounded(px(3.0)),
                )
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(theme::text_weakest())
                        .child(day.split('-').last().unwrap_or(day).to_string()),
                ),
        );
    }

    info_card(bars.into_any_element())
}
