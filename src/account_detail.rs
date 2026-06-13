//! 账户详情页:面包屑 + 签到/刷新 + 四标签(概览 / API 密钥 / 使用日志 / 消耗图表)。
//!
//! 当前账户来自 `AppState::view = AppView::AccountDetail(id)`;选中的标签页是本视图自身状态。
//! 概览页直接展示账户已有的 Cookie/余额信息;其余三个标签的数据来自 `account_cache`,
//! 在 Phase 4 接入「刷新数据」后填充,此前显示「尚未拉取」。

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Disableable, Sizable};

use crate::app_state::{AppState, AppView, BalanceInfo, DetailData};
use crate::cookie::{self, CookieStatus};
use crate::service::{self, CheckinScope};
use crate::storage::Account;
use crate::theme::{Glass, GlassExt};

const TABS: [&str; 4] = ["概览", "API 密钥", "使用日志", "消耗图表"];

pub struct AccountDetail {
    app_state: Entity<AppState>,
    selected_tab: usize,
}

impl AccountDetail {
    pub fn new(app_state: Entity<AppState>) -> Self {
        Self { app_state, selected_tab: 0 }
    }

    fn current_account_id(&self, cx: &Context<Self>) -> Option<i64> {
        match self.app_state.read(cx).view {
            AppView::AccountDetail(id) => Some(id),
            _ => None,
        }
    }

    fn go_home(&self, cx: &mut Context<Self>) {
        self.app_state.update(cx, |state, cx| {
            state.view = AppView::Home;
            cx.notify();
        });
    }

    fn trigger_checkin(&self, id: i64, cx: &mut Context<Self>) {
        service::run_checkin(self.app_state.downgrade(), CheckinScope::Account(id), cx);
    }

    fn trigger_login(&self, id: i64, cx: &mut Context<Self>) {
        service::run_login(self.app_state.downgrade(), id, cx);
    }

    fn trigger_fetch(&self, id: i64, cx: &mut Context<Self>) {
        service::run_fetch_detail(self.app_state.downgrade(), id, cx);
    }

    fn render_breadcrumb(&self, account_name: &str, site_name: &str, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id("back-home")
                    .cursor_pointer()
                    .text_sm()
                    .text_color(Glass::text_secondary())
                    .hover(|s| s.text_color(Glass::primary()))
                    .on_click(cx.listener(|this, _, _, cx| this.go_home(cx)))
                    .child("◀ 返回主页"),
            )
            .child(div().text_sm().text_color(Glass::text_muted()).child("/"))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(Glass::text())
                    .child(format!("{} @ {}", account_name, site_name)),
            )
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected_tab;
        let mut bar = div()
            .flex()
            .items_center()
            .gap_1()
            .border_b_1()
            .border_color(Glass::border());
        for (idx, label) in TABS.iter().enumerate() {
            let active = idx == selected;
            bar = bar.child(
                div()
                    .id(SharedString::from(format!("tab-{}", idx)))
                    .px_3()
                    .py_2()
                    .cursor_pointer()
                    .rounded_t(px(6.0))
                    .bg(if active { Glass::primary().opacity(0.15) } else { Glass::transparent() })
                    .text_sm()
                    .font_weight(if active { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                    .text_color(if active { Glass::primary() } else { Glass::text_secondary() })
                    .hover(|s| s.text_color(Glass::text()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_tab = idx;
                        cx.notify();
                    }))
                    .child(label.to_string()),
            );
        }
        bar
    }

    /// 概览:余额卡片 + Cookie 卡片
    fn render_overview(
        &self,
        account: &Account,
        balance: Option<&BalanceInfo>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let balance_card = match balance {
            Some(b) => div()
                .flex()
                .gap_6()
                .child(Self::metric("余额", format!("${:.2}", b.quota), Glass::text()))
                .child(Self::metric("已用", format!("${:.2}", b.used_quota), Glass::text_secondary()))
                .child(Self::metric(
                    "本次奖励",
                    format!("+${:.2}", b.reward),
                    if b.reward > 0.0 { Glass::success() } else { Glass::text_muted() },
                ))
                .into_any_element(),
            None => div()
                .text_sm()
                .text_color(Glass::text_muted())
                .child("尚未签到,点击右上角「签到」获取余额")
                .into_any_element(),
        };

        let (status_text, status_color): (&str, Hsla) = if account.cookies_json.is_none() {
            ("无 Cookie,待登录", Glass::warning())
        } else {
            match cookie::classify(account.cookie_expires_at.as_deref()) {
                CookieStatus::Valid => ("有效", Glass::success()),
                CookieStatus::ExpiringSoon => ("24 小时内到期", Glass::warning()),
                CookieStatus::Expired => ("已过期", Glass::danger()),
                CookieStatus::Unknown => ("会话期 / 未知", Glass::text_muted()),
            }
        };
        let has_creds = account.username.is_some() && account.password.is_some();
        let account_id = account.id;

        div()
            .flex()
            .flex_col()
            .gap_4()
            // 余额卡片
            .child(
                div()
                    .glass_card()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(Glass::text()).child("余额"))
                    .child(balance_card),
            )
            // Cookie 卡片
            .child(
                div()
                    .glass_card()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(Glass::text()).child("Cookie"))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .px_2()
                                            .py_0p5()
                                            .rounded(px(6.0))
                                            .bg(status_color.opacity(0.12))
                                            .border_1()
                                            .border_color(status_color.opacity(0.35))
                                            .text_xs()
                                            .text_color(status_color)
                                            .child(status_text),
                                    )
                                    .when(has_creds, |this| {
                                        this.child(
                                            Button::new("cookie-relogin")
                                                .ghost()
                                                .small()
                                                .label("账密登录刷新")
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.trigger_login(account_id, cx)
                                                })),
                                        )
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_6()
                            .child(Self::metric(
                                "签发时间",
                                account.cookie_issued_at.clone().unwrap_or_else(|| "—".into()),
                                Glass::text_secondary(),
                            ))
                            .child(Self::metric(
                                "过期时间",
                                account.cookie_expires_at.clone().unwrap_or_else(|| "未知 / 会话期".into()),
                                Glass::text_secondary(),
                            )),
                    ),
            )
            .into_any_element()
    }

    fn metric(label: &str, value: String, color: Hsla) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_xs().text_color(Glass::text_muted()).child(label.to_string()))
            .child(div().text_base().font_weight(FontWeight::SEMIBOLD).text_color(color).child(value))
    }

    /// 其余标签页的「尚未拉取」占位
    fn render_pending(hint: &str) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .py_12()
            .child(div().text_xl().text_color(Glass::text_muted().opacity(0.4)).child("◷"))
            .child(div().text_sm().text_color(Glass::text_muted()).child(hint.to_string()))
            .child(
                div()
                    .text_xs()
                    .text_color(Glass::text_muted().opacity(0.7))
                    .child("点击右上角「刷新数据」获取"),
            )
            .into_any_element()
    }

    /// API 密钥标签:打码展示 key,点击「复制」写入完整值到剪贴板。
    fn render_tokens(detail: Option<&DetailData>) -> AnyElement {
        let Some(d) = detail else {
            return Self::render_pending("尚未拉取 API 密钥");
        };
        let arr = parse_array(&d.tokens_json);
        if arr.is_empty() {
            return Self::render_pending("尚未拉取 API 密钥");
        }
        let rows: Vec<AnyElement> = arr
            .iter()
            .filter_map(|v| v.as_object())
            .enumerate()
            .map(|(i, o)| {
                let name = ostr(o, &["name"]).filter(|s| !s.is_empty()).unwrap_or_else(|| "(未命名)".into());
                let raw_key = ostr(o, &["key"]).unwrap_or_default();
                let full_key = if raw_key.starts_with("sk-") { raw_key } else { format!("sk-{}", raw_key) };
                let masked = mask_key(&full_key);
                let remain = onum(o, &["remain_quota"]) / 500000.0;
                let used = onum(o, &["used_quota"]) / 500000.0;
                let unlimited = o.get("unlimited_quota").and_then(|v| v.as_bool()).unwrap_or(false);
                let (st_txt, st_col) = if onum(o, &["status"]) == 1.0 {
                    ("启用", Glass::success())
                } else {
                    ("禁用", Glass::text_muted())
                };
                let copy_key = full_key.clone();
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(Glass::border())
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_sm().text_color(Glass::text()).child(name))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(div().text_xs().text_color(Glass::text_muted()).child(masked))
                                    .child(
                                        Button::new(SharedString::from(format!("copy-key-{}", i)))
                                            .ghost()
                                            .small()
                                            .label("复制")
                                            .on_click(move |_, _window, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(copy_key.clone()))
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        div().w(px(120.0)).text_right().text_xs().text_color(Glass::text_secondary()).child(
                            if unlimited { "额度 无限".to_string() } else { format!("额度 ${:.2}", remain) },
                        ),
                    )
                    .child(div().w(px(90.0)).text_right().text_xs().text_color(Glass::text_muted()).child(format!("已用 ${:.2}", used)))
                    .child(div().w(px(48.0)).text_right().text_xs().text_color(st_col).child(st_txt))
                    .into_any_element()
            })
            .collect();
        div().glass_card().overflow_hidden().children(rows).into_any_element()
    }

    /// 使用日志标签:最近最多 50 条。
    fn render_logs(detail: Option<&DetailData>) -> AnyElement {
        let Some(d) = detail else {
            return Self::render_pending("尚未拉取使用日志");
        };
        let arr = parse_array(&d.logs_json);
        if arr.is_empty() {
            return Self::render_pending("尚未拉取使用日志");
        }
        let rows: Vec<AnyElement> = arr
            .iter()
            .filter_map(|v| v.as_object())
            .take(50)
            .map(|o| {
                let time = ostr(o, &["created_at"]).unwrap_or_else(|| {
                    let ts = onum(o, &["created_at", "time"]) as i64;
                    if ts > 0 { fmt_unix(ts) } else { "—".into() }
                });
                let model = ostr(o, &["model_name", "model"]).unwrap_or_default();
                let prompt = onum(o, &["prompt_tokens"]) as i64;
                let completion = onum(o, &["completion_tokens"]) as i64;
                let quota = onum(o, &["quota"]) / 500000.0;
                let use_time = onum(o, &["use_time"]) as i64;
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(Glass::border())
                    .child(div().w(px(120.0)).text_xs().text_color(Glass::text_muted()).child(time))
                    .child(div().flex_1().text_xs().text_color(Glass::text()).child(model))
                    .child(div().w(px(120.0)).text_right().text_xs().text_color(Glass::text_secondary()).child(format!("{}/{} tok", prompt, completion)))
                    .child(div().w(px(80.0)).text_right().text_xs().text_color(Glass::text()).child(format!("${:.3}", quota)))
                    .child(div().w(px(64.0)).text_right().text_xs().text_color(Glass::text_muted()).child(format!("{}s", use_time)))
                    .into_any_element()
            })
            .collect();
        div().glass_card().overflow_hidden().children(rows).into_any_element()
    }
}

impl Render for AccountDetail {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let id = self.current_account_id(cx);
        let snapshot: Option<(Account, String, Option<BalanceInfo>, Option<DetailData>, bool)> =
            id.and_then(|id| {
                let state = self.app_state.read(cx);
                state.account_by_id(id).cloned().map(|acc| {
                    let site_name = state
                        .site_by_id(acc.site_id)
                        .map(|s| s.name.clone())
                        .unwrap_or_default();
                    let bal = state.balances.get(&id).cloned();
                    let detail = state.detail_of(id).cloned();
                    let fetching = state.fetching_detail == Some(id);
                    (acc, site_name, bal, detail, fetching)
                })
            });

        let Some((account, site_name, balance, detail, fetching)) = snapshot else {
            return div()
                .flex()
                .flex_col()
                .size_full()
                .p_6()
                .gap_4()
                .child(self.render_breadcrumb("—", "—", cx))
                .child(
                    div()
                        .text_sm()
                        .text_color(Glass::text_muted())
                        .child("账户不存在或已删除。"),
                );
        };

        let account_id = account.id;
        let selected = self.selected_tab;
        let last_update = detail
            .as_ref()
            .filter(|d| !d.fetched_at.is_empty())
            .map(|d| format!("上次更新:{}", d.fetched_at))
            .unwrap_or_else(|| "上次更新:尚未拉取".to_string());

        let tab_content = match selected {
            0 => self.render_overview(&account, balance.as_ref(), cx),
            1 => Self::render_tokens(detail.as_ref()),
            2 => Self::render_logs(detail.as_ref()),
            _ => match &detail {
                Some(d) => crate::chart::render_chart(&d.chart_json),
                None => Self::render_pending("尚未拉取消耗图表"),
            },
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .p_6()
            .gap_4()
            // 头部:面包屑 + 操作
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.render_breadcrumb(&account.name, &site_name, cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("detail-checkin")
                                    .primary()
                                    .label("⚡ 签到")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.trigger_checkin(account_id, cx)
                                    })),
                            )
                            .child(
                                Button::new("detail-refresh")
                                    .ghost()
                                    .label(if fetching { "刷新中..." } else { "↻ 刷新数据" })
                                    .disabled(fetching)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.trigger_fetch(account_id, cx)
                                    })),
                            ),
                    ),
            )
            .child(div().text_xs().text_color(Glass::text_muted()).child(last_update))
            // 标签栏
            .child(self.render_tab_bar(cx))
            // 标签内容
            .child(div().id("detail-tab-content").flex_1().overflow_y_scroll().pt_2().child(tab_content))
    }
}

// ===================== 模块级解析辅助 =====================

fn parse_array(json: &str) -> Vec<serde_json::Value> {
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(serde_json::Value::Array(a)) => a,
        Ok(serde_json::Value::Object(o)) => o
            .get("items")
            .or_else(|| o.get("data"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn ostr(o: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = o.get(*k).and_then(|v| v.as_str()) {
            return Some(s.to_string());
        }
    }
    None
}

fn onum(o: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> f64 {
    for k in keys {
        if let Some(n) = o.get(*k).and_then(|v| v.as_f64()) {
            return n;
        }
    }
    0.0
}

fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 12 {
        return "•".repeat(chars.len().max(4));
    }
    let head: String = chars[..5].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{}…{}", head, tail)
}

fn fmt_unix(ts: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|d| d.format("%m-%d %H:%M").to_string())
        .unwrap_or_default()
}
