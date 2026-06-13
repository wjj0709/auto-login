use gpui::{AnyElement, Context, Entity, IntoElement, ParentElement, Styled, div, prelude::*, px};

use crate::app_state::{AppState, ViewKind};
use crate::theme;
use crate::views::root::RootView;

pub fn render(
    state: Entity<AppState>,
    account_id: i64,
    _cx: &mut Context<RootView>,
) -> AnyElement {
    let state_back = state.clone();
    div()
        .id("detail-scroll")
        .size_full()
        .overflow_y_scroll()
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .id("breadcrumb")
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
                            state_back.update(cx, |st, cx| {
                                st.current_view = ViewKind::Home;
                                cx.notify();
                            });
                        }),
                )
                .child(
                    div()
                        .text_color(theme::text_weakest())
                        .child(format!("/ 账户 #{}", account_id)),
                ),
        )
        .child(
            div()
                .w_full()
                .p(px(16.0))
                .bg(theme::bg_card())
                .border_1()
                .border_color(theme::border_normal())
                .rounded(px(8.0))
                .text_color(theme::text_secondary())
                .text_size(px(12.0))
                .child("账户详情页 — 待实现完整四 Tab（概览 / API 密钥 / 使用日志 / 消耗图表）"),
        )
        .into_any_element()
}
