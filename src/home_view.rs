use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::{Sizable, WindowExt};

use crate::app_state::{AppState, DeleteTarget};
use crate::theme::{Glass, GlassExt};

/// 主页:统计条 + 站点卡片网格 + 新建站点卡片。
pub struct HomeView {
    app_state: Entity<AppState>,
}

impl HomeView {
    pub fn new(app_state: Entity<AppState>) -> Self {
        Self { app_state }
    }

    /// 打开站点表单(Milestone C 替换为真实表单)。
    fn open_site_form(&self, edit_id: Option<i64>, window: &mut Window, _cx: &mut Context<Self>) {
        let title = if edit_id.is_some() { "编辑站点" } else { "新建站点" };
        window.open_dialog(_cx, move |dialog, _window, _cx| {
            dialog.title(title).w(px(460.0)).child(
                div()
                    .py_6()
                    .text_sm()
                    .text_color(Glass::text_muted())
                    .child("站点表单即将上线(Milestone C)"),
            )
        });
    }

    fn open_account_list(&self, site_id: i64, cx: &mut Context<Self>) {
        self.app_state.update(cx, |state, cx| {
            state.account_list_modal = Some(site_id);
            cx.notify();
        });
    }

    fn request_delete_site(&self, site_id: i64, cx: &mut Context<Self>) {
        self.app_state.update(cx, |state, cx| {
            state.confirm_delete = Some(DeleteTarget::Site(site_id));
            cx.notify();
        });
    }

    /// 统计卡片
    fn stat(label: &str, value: String, color: Hsla, glow: bool) -> impl IntoElement {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap_1()
            .p_4()
            .bg(Glass::card_gradient())
            .border_1()
            .border_color(if glow { color.opacity(0.4) } else { Glass::border_bright() })
            .rounded(px(14.0))
            .shadow(if glow { Glass::shadow_glow(color) } else { Glass::shadow_soft() })
            .child(div().text_xs().text_color(Glass::text_muted()).child(label.to_string()))
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::BOLD)
                    .text_color(color)
                    .child(value),
            )
    }

    /// 单个站点卡片
    fn render_site_card(
        &self,
        site_id: i64,
        name: String,
        domain: String,
        account_count: usize,
        balance_sum: Option<f64>,
        missing_balance: usize,
        need_cookie: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let balance_text = match balance_sum {
            Some(v) => format!("余额合计 ${:.2}", v),
            None => "余额 未拉取".to_string(),
        };

        div()
            .w(px(320.0))
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .glass_card()
            .hover(|s| {
                s.border_color(Glass::primary().opacity(0.5))
                    .shadow(Glass::shadow_glow(Glass::primary()))
            })
            // 头部:站点名 + 域名
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(Glass::text())
                            .child(name),
                    )
                    .child(div().text_xs().text_color(Glass::text_muted()).child(domain)),
            )
            // 指标行
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_xs()
                            .text_color(Glass::text_secondary())
                            .child(format!("账户 {}", account_count)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(if balance_sum.is_some() {
                                Glass::text_secondary()
                            } else {
                                Glass::text_muted()
                            })
                            .child(balance_text),
                    )
                    .when(missing_balance > 0 && balance_sum.is_some(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(Glass::text_muted())
                                .child(format!("({} 未拉取)", missing_balance)),
                        )
                    })
                    .when(need_cookie > 0, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(Glass::warning())
                                .child(format!("⚠ {} 待登录", need_cookie)),
                        )
                    }),
            )
            // 操作行
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .pt_2()
                    .border_t_1()
                    .border_color(Glass::border())
                    .child(
                        Button::new(SharedString::from(format!("view-{}", site_id)))
                            .outline()
                            .small()
                            .label("查看账户")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_account_list(site_id, cx)
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("edit-site-{}", site_id)))
                            .ghost()
                            .small()
                            .label("编辑")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_site_form(Some(site_id), window, cx)
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("del-site-{}", site_id)))
                            .ghost()
                            .small()
                            .danger()
                            .label("删除")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.request_delete_site(site_id, cx)
                            })),
                    ),
            )
    }

    /// 新建站点的虚线卡片
    fn render_new_site_card(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("new-site-card")
            .w(px(320.0))
            .h(px(132.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .rounded(px(14.0))
            .border_1()
            .border_dashed()
            .border_color(Glass::border_bright())
            .text_color(Glass::text_muted())
            .cursor_pointer()
            .hover(|s| s.border_color(Glass::primary().opacity(0.6)).text_color(Glass::primary()))
            .on_click(cx.listener(|this, _, window, cx| this.open_site_form(None, window, cx)))
            .child(div().text_2xl().child("+"))
            .child(div().text_sm().child("新建站点"))
    }
}

impl Render for HomeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 先把渲染需要的数据从 state 中取出,避免后续 cx.listener 的可变借用冲突
        struct CardData {
            site_id: i64,
            name: String,
            domain: String,
            account_count: usize,
            balance_sum: Option<f64>,
            missing_balance: usize,
            need_cookie: usize,
        }

        let (site_count, account_count, success, total, total_balance, total_missing, cards): (
            usize,
            usize,
            usize,
            usize,
            Option<f64>,
            usize,
            Vec<CardData>,
        ) = {
            let state = self.app_state.read(cx);
            let mut cards = Vec::new();
            let mut grand_sum = 0.0_f64;
            let mut grand_has = 0usize;
            let mut grand_missing = 0usize;
            for site in &state.sites {
                let accs = state.accounts_of_site(site.id);
                let mut sum = 0.0_f64;
                let mut has = 0usize;
                let mut need_cookie = 0usize;
                for a in &accs {
                    if let Some(b) = state.balances.get(&a.id) {
                        sum += b.quota;
                        has += 1;
                    }
                    if a.cookies_json.is_none() {
                        need_cookie += 1;
                    }
                }
                let missing = accs.len().saturating_sub(has);
                grand_missing += missing;
                if has > 0 {
                    grand_sum += sum;
                    grand_has += has;
                }
                cards.push(CardData {
                    site_id: site.id,
                    name: site.name.clone(),
                    domain: site.domain.clone(),
                    account_count: accs.len(),
                    balance_sum: if has > 0 { Some(sum) } else { None },
                    missing_balance: missing,
                    need_cookie,
                });
            }
            (
                state.sites.len(),
                state.accounts.len(),
                state.success_count,
                state.total_accounts(),
                if grand_has > 0 { Some(grand_sum) } else { None },
                grand_missing,
                cards,
            )
        };

        let total_balance_text = match total_balance {
            Some(v) => format!("${:.2}", v),
            None => "未拉取".to_string(),
        };

        let card_els: Vec<AnyElement> = cards
            .into_iter()
            .map(|c| {
                self.render_site_card(
                    c.site_id,
                    c.name,
                    c.domain,
                    c.account_count,
                    c.balance_sum,
                    c.missing_balance,
                    c.need_cookie,
                    cx,
                )
                .into_any_element()
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_5()
            // 统计条
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(Self::stat("站点", site_count.to_string(), Glass::text(), false))
                    .child(Self::stat("账户", account_count.to_string(), Glass::text(), false))
                    .child(Self::stat("总余额", total_balance_text, Glass::primary(), true))
                    .child(Self::stat(
                        "今日签到",
                        format!("{} / {}", success, total),
                        Glass::success(),
                        false,
                    )),
            )
            .when(total_missing > 0 && total_balance.is_some(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(Glass::text_muted())
                        .child(format!("{} 个账户未拉取余额,未计入总额", total_missing)),
                )
            })
            // 站点卡片网格
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_4()
                    .children(card_els)
                    .child(self.render_new_site_card(cx)),
            )
    }
}
