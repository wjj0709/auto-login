use gpui::*;

use crate::app_state::{AppState, CheckInStatus};
use crate::theme::Glass;

/// 账号管理面板
pub struct AccountPanel {
    app_state: Entity<AppState>,
}

impl AccountPanel {
    pub fn new(app_state: Entity<AppState>, _cx: &mut Context<Self>) -> Self {
        Self { app_state }
    }

    fn render_status_badge(status: &CheckInStatus) -> impl IntoElement {
        let (text, color) = match status {
            CheckInStatus::Idle => ("Idle", Glass::text_muted()),
            CheckInStatus::Running => ("Running...", Glass::warning()),
            CheckInStatus::Success => ("Success", Glass::success()),
            CheckInStatus::Failed(_) => ("Failed", Glass::danger()),
        };

        div()
            .px_2()
            .py_0p5()
            .rounded(px(6.0))
            .bg(rgba(0, 0, 0, 0))
            .border_1()
            .border_color(color.opacity(0.3))
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(color)
            .child(text)
    }

    fn render_account_card(
        name: &str,
        provider: &str,
        api_user: &str,
        status: &CheckInStatus,
        balance: Option<&crate::app_state::BalanceInfo>,
    ) -> impl IntoElement {
        div()
            .p_4()
            .rounded(px(12.0))
            .bg(Glass::card())
            .border_1()
            .border_color(Glass::border())
            .hover(|s| s.border_color(Glass::border_bright()).bg(Glass::card_hover()))
            .child(
                // 头部: 名称 + 状态
                div().flex().items_center().justify_between().mb_3()
                    .child(
                        div().flex().items_center().gap_2()
                            .child(
                                div().w(px(32.0)).h(px(32.0)).rounded(px(8.0))
                                    .bg(Glass::primary().opacity(0.15))
                                    .flex().items_center().justify_center()
                                    .text_color(Glass::primary())
                                    .text_sm().font_weight(FontWeight::BOLD)
                                    .child(name.chars().next().unwrap_or('A').to_string())
                            )
                            .child(
                                div().flex().flex_col()
                                    .child(
                                        div().text_sm().font_weight(FontWeight::SEMIBOLD)
                                            .text_color(Glass::text())
                                            .child(name.to_string())
                                    )
                                    .child(
                                        div().text_xs().text_color(Glass::text_muted())
                                            .child(format!("{} | {}", provider, &api_user[..api_user.len().min(8)]))
                                    )
                            )
                    )
                    .child(Self::render_status_badge(status))
            )
            // 余额信息
            .child(
                if let Some(bal) = balance {
                    div().flex().gap_4().pt_2().border_t_1().border_color(Glass::border())
                        .child(
                            div().flex().flex_col()
                                .child(div().text_xs().text_color(Glass::text_muted()).child("Balance"))
                                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(Glass::text())
                                    .child(format!("${:.2}", bal.quota)))
                        )
                        .child(
                            div().flex().flex_col()
                                .child(div().text_xs().text_color(Glass::text_muted()).child("Reward"))
                                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD)
                                    .text_color(if bal.reward > 0.0 { Glass::success() } else { Glass::text() })
                                    .child(format!("+${:.2}", bal.reward)))
                        )
                        .child(
                            div().flex().flex_col()
                                .child(div().text_xs().text_color(Glass::text_muted()).child("Used"))
                                .child(div().text_sm().text_color(Glass::text_secondary())
                                    .child(format!("${:.2}", bal.used_quota)))
                        )
                        .into_any_element()
                } else {
                    div().pt_2().border_t_1().border_color(Glass::border())
                        .text_xs().text_color(Glass::text_muted())
                        .child("No balance data")
                        .into_any_element()
                }
            )
            // 错误信息
            .child(
                if let CheckInStatus::Failed(err) = status {
                    div().mt_2().text_xs().text_color(Glass::danger().opacity(0.8))
                        .child(format!("Error: {}", err))
                        .into_any_element()
                } else {
                    div().into_any_element()
                }
            )
    }
}

impl Render for AccountPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.app_state.read(cx);
        let accounts = &state.accounts;
        let statuses = &state.checkin_status;
        let balances = &state.balances;

        let mut cards = Vec::new();
        for (i, account) in accounts.iter().enumerate() {
            let name = account.get_display_name(i);
            let status = statuses.get(&name).unwrap_or(&CheckInStatus::Idle);
            let balance = balances.get(&name);

            cards.push(
                Self::render_account_card(
                    &name,
                    &account.provider,
                    &account.api_user,
                    status,
                    balance,
                ).into_any_element()
            );
        }

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div().text_lg().font_weight(FontWeight::BOLD)
                    .text_color(Glass::text())
                    .child("Account Management")
            )
            .child(
                div().text_sm().text_color(Glass::text_secondary())
                    .child(format!("{} account(s) configured", accounts.len()))
            )
            .child(
                div().flex().flex_col().gap_3()
                    .children(cards)
            )
    }
}
