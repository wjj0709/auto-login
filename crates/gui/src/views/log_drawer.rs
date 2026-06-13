use gpui::{AnyElement, Context, Entity, IntoElement, ParentElement, Styled, div, prelude::*, px};

use crate::app_state::{AppState, LogLevel};
use crate::theme;
use crate::views::root::RootView;

pub fn render(state: Entity<AppState>, cx: &mut Context<RootView>) -> AnyElement {
    let snap = state.read(cx);
    // 倒序：最新日志显示在顶部，无需手动滚动
    let entries: Vec<_> = snap.log_entries.iter().rev().cloned().collect();
    let state_clear = state.clone();
    let state_close = state;

    div()
        .w_full()
        .h(px(220.0))
        .bg(theme::bg_bar())
        .border_t_1()
        .border_color(theme::border_accent())
        .flex()
        .flex_col()
        .child(
            div()
                .w_full()
                .h(px(28.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(theme::border_normal())
                .child(
                    div()
                        .text_color(theme::text_secondary())
                        .text_size(px(11.0))
                        .child("运行日志"),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(10.0))
                        .text_size(px(10.0))
                        .text_color(theme::text_weakest())
                        .child(
                            div()
                                .id("log-clear")
                                .cursor_pointer()
                                .hover(|this| this.text_color(theme::text_secondary()))
                                .child("[清空]")
                                .on_click(move |_, _w, cx| {
                                    state_clear.update(cx, |st, cx| {
                                        st.log_entries.clear();
                                        cx.notify();
                                    });
                                }),
                        )
                        .child(
                            div()
                                .id("log-close")
                                .cursor_pointer()
                                .hover(|this| this.text_color(theme::error_red()))
                                .child("[✕]")
                                .on_click(move |_, _w, cx| {
                                    state_close.update(cx, |st, cx| {
                                        st.log_drawer_open = false;
                                        cx.notify();
                                    });
                                }),
                        ),
                ),
        )
        .child(
            div()
                .id("log-scroll")
                .flex_1()
                .overflow_y_scroll()
                .px(px(12.0))
                .py(px(6.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .text_size(px(10.0))
                .children(entries.into_iter().map(|entry| {
                    let color = match entry.level {
                        LogLevel::Info => theme::text_secondary(),
                        LogLevel::Success => theme::success_green(),
                        LogLevel::Warning => theme::warning_yellow(),
                        LogLevel::Error => theme::error_red(),
                    };
                    div()
                        .text_color(color)
                        .child(format!("{}  {}", entry.timestamp, entry.message))
                })),
        )
        .into_any_element()
}
