//! 账户详情页:面包屑 + 签到/刷新 + 四标签(概览 / API 密钥 / 使用日志 / 消耗图表)。
//!
//! 当前账户来自 `AppState::view = AppView::AccountDetail(id)`;选中的标签页是本视图自身状态。
//! 概览页直接展示账户已有的 Cookie/余额信息;其余三个标签的数据来自 `account_cache`,
//! 在 Phase 4 接入「刷新数据」后填充,此前显示「尚未拉取」。

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::notification::Notification;
use gpui_component::{Sizable, WindowExt};

use crate::app_state::{AppState, AppView, BalanceInfo};
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
                    .child("点击右上角「刷新数据」获取(Phase 4 接入)"),
            )
            .into_any_element()
    }
}

impl Render for AccountDetail {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let id = self.current_account_id(cx);
        let snapshot: Option<(Account, String, Option<BalanceInfo>)> = id.and_then(|id| {
            let state = self.app_state.read(cx);
            state.account_by_id(id).cloned().map(|acc| {
                let site_name = state
                    .site_by_id(acc.site_id)
                    .map(|s| s.name.clone())
                    .unwrap_or_default();
                let bal = state.balances.get(&id).cloned();
                (acc, site_name, bal)
            })
        });

        let Some((account, site_name, balance)) = snapshot else {
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

        let tab_content = match selected {
            0 => self.render_overview(&account, balance.as_ref(), cx),
            1 => Self::render_pending("尚未拉取 API 密钥"),
            2 => Self::render_pending("尚未拉取使用日志"),
            _ => Self::render_pending("尚未拉取消耗图表"),
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
                                    .label("↻ 刷新数据")
                                    .on_click(cx.listener(|_, _, window, cx| {
                                        window.push_notification(
                                            Notification::info("详情数据拉取将在 Phase 4 上线"),
                                            cx,
                                        );
                                    })),
                            ),
                    ),
            )
            .child(div().text_xs().text_color(Glass::text_muted()).child("上次更新:尚未拉取"))
            // 标签栏
            .child(self.render_tab_bar(cx))
            // 标签内容
            .child(div().id("detail-tab-content").flex_1().overflow_y_scroll().pt_2().child(tab_content))
    }
}
