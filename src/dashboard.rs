use gpui::*;

use crate::app_state::AppState;
use crate::theme::Glass;

/// 余额仪表盘
pub struct Dashboard {
    app_state: Entity<AppState>,
}

impl Dashboard {
    pub fn new(app_state: Entity<AppState>) -> Self {
        Self { app_state }
    }

    fn render_stat_card(
        label: &str,
        value: &str,
        color: Hsla,
    ) -> impl IntoElement {
        div()
            .flex_1()
            .p_4()
            .rounded(px(12.0))
            .bg(Glass::card())
            .border_1()
            .border_color(Glass::border())
            .child(
                div().flex().flex_col().gap_1()
                    .child(
                        div().text_xs().text_color(Glass::text_muted())
                            .child(label.to_string())
                    )
                    .child(
                        div().text_xl().font_weight(FontWeight::BOLD)
                            .text_color(color)
                            .child(value.to_string())
                    )
            )
    }

    fn render_balance_row(
        name: &str,
        quota: f64,
        used: f64,
        reward: f64,
    ) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(Glass::border())
            .hover(|s| s.bg(Glass::card_hover()))
            .child(
                div().flex_1().text_sm().text_color(Glass::text())
                    .child(name.to_string())
            )
            .child(
                div().w(px(100.0)).text_right().text_sm().text_color(Glass::text())
                    .child(format!("${:.2}", quota))
            )
            .child(
                div().w(px(100.0)).text_right().text_sm().text_color(Glass::text_secondary())
                    .child(format!("${:.2}", used))
            )
            .child(
                div().w(px(100.0)).text_right().text_sm()
                    .text_color(if reward > 0.0 { Glass::success() } else { Glass::text_muted() })
                    .child(format!("+${:.2}", reward))
            )
    }
}

impl Render for Dashboard {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.app_state.read(cx);
        let total = state.total_accounts();
        let success = state.success_count;
        let failed = state.fail_count;
        let total_reward: f64 = state.balances.values().map(|b| b.reward).sum();

        // 余额明细行
        let mut balance_rows = Vec::new();
        for (i, account) in state.accounts.iter().enumerate() {
            let name = account.get_display_name(i);
            if let Some(bal) = state.balances.get(&name) {
                balance_rows.push(
                    Self::render_balance_row(&name, bal.quota, bal.used_quota, bal.reward)
                        .into_any_element()
                );
            }
        }

        div()
            .flex()
            .flex_col()
            .gap_5()
            // 标题
            .child(
                div().text_lg().font_weight(FontWeight::BOLD)
                    .text_color(Glass::text())
                    .child("Dashboard")
            )
            // 统计卡片行
            .child(
                div().flex().gap_3()
                    .child(Self::render_stat_card("Total", &total.to_string(), Glass::text()))
                    .child(Self::render_stat_card("Success", &success.to_string(), Glass::success()))
                    .child(Self::render_stat_card("Failed", &failed.to_string(), Glass::danger()))
                    .child(Self::render_stat_card("Total Reward", &format!("+${:.2}", total_reward), Glass::primary()))
            )
            // 余额表格
            .child(
                div()
                    .rounded(px(12.0))
                    .bg(Glass::card())
                    .border_1()
                    .border_color(Glass::border())
                    .overflow_hidden()
                    // 表头
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .px_4()
                            .py_2()
                            .bg(Glass::panel())
                            .border_b_1()
                            .border_color(Glass::border())
                            .child(
                                div().flex_1().text_xs().font_weight(FontWeight::SEMIBOLD)
                                    .text_color(Glass::text_muted())
                                    .child("Account")
                            )
                            .child(
                                div().w(px(100.0)).text_right().text_xs().font_weight(FontWeight::SEMIBOLD)
                                    .text_color(Glass::text_muted())
                                    .child("Balance")
                            )
                            .child(
                                div().w(px(100.0)).text_right().text_xs().font_weight(FontWeight::SEMIBOLD)
                                    .text_color(Glass::text_muted())
                                    .child("Used")
                            )
                            .child(
                                div().w(px(100.0)).text_right().text_xs().font_weight(FontWeight::SEMIBOLD)
                                    .text_color(Glass::text_muted())
                                    .child("Reward")
                            )
                    )
                    // 数据行
                    .child(
                        if balance_rows.is_empty() {
                            div().p_6().text_center().text_sm().text_color(Glass::text_muted())
                                .child("No balance data. Run check-in first.")
                                .into_any_element()
                        } else {
                            div().flex().flex_col()
                                .children(balance_rows)
                                .into_any_element()
                        }
                    )
            )
    }
}
