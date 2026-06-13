use gpui::{
    AnyElement, App, Bounds, Context, Entity, IntoElement, ParentElement, Render, Styled, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, size,
};
use gpui_platform::application;

use crate::app_state::AppState;
use crate::theme;
use crate::views;

pub struct RootView {
    pub state: Entity<AppState>,
}

impl RootView {
    pub fn new(state: Entity<AppState>, _cx: &mut Context<Self>) -> Self {
        Self { state }
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state_ref = self.state.clone();
        let log_drawer_open;
        let active_modal_some;
        let running;
        let progress_text;
        {
            let state = state_ref.read(cx);
            log_drawer_open = state.log_drawer_open;
            active_modal_some = state.active_modal.is_some();
            running = state.running;
            progress_text = state
                .run_progress
                .clone()
                .unwrap_or_else(|| if running { "运行中…".into() } else { "● 就绪".into() });
        }

        let titlebar = self.render_titlebar(cx);
        let content = self.render_content(cx);
        let bottombar = self.render_bottombar(progress_text, cx);
        let log_drawer = if log_drawer_open {
            Some(crate::views::log_drawer::render(state_ref.clone(), cx))
        } else {
            None
        };
        let modal = if active_modal_some {
            Some(crate::views::modals::render(state_ref, cx))
        } else {
            None
        };

        div()
            .size_full()
            .bg(theme::bg_window())
            .text_color(theme::text_primary())
            .text_size(px(12.0))
            .flex()
            .flex_col()
            .child(titlebar)
            .child(div().flex_1().overflow_hidden().child(content))
            .when_some(log_drawer, |this, el| this.child(el))
            .when_some(modal, |this, el| this.child(el))
            .child(bottombar)
    }
}

impl RootView {
    fn render_titlebar(&self, _cx: &mut Context<Self>) -> AnyElement {
        let state = self.state.clone();
        div()
            .w_full()
            .h(px(40.0))
            .bg(theme::bg_bar())
            .border_b_1()
            .border_color(theme::border_normal())
            .flex()
            .items_center()
            .justify_between()
            .px(px(16.0))
            .child(
                div()
                    .text_color(theme::text_primary())
                    .text_size(px(13.0))
                    .child("🅰 AnyRouter · Auto Check-in"),
            )
            .child(
                div()
                    .id("checkin-all-btn")
                    .px(px(12.0))
                    .py(px(4.0))
                    .bg(theme::btn_primary_bg())
                    .border_1()
                    .border_color(theme::btn_primary_border())
                    .rounded(px(5.0))
                    .text_color(theme::accent_blue())
                    .text_size(px(11.0))
                    .cursor_pointer()
                    .hover(|this| this.opacity(0.85))
                    .child("⚡ 一键签到全部")
                    .on_click(move |_, _window, cx| {
                        state.update(cx, |state, cx| {
                            state.log_entries.push(crate::app_state::LogEntry {
                                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                                level: crate::app_state::LogLevel::Info,
                                message: "签到功能待接入后台执行".to_string(),
                            });
                            state.log_drawer_open = true;
                            cx.notify();
                        });
                    }),
            )
            .into_any_element()
    }

    fn render_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let view_kind = self.state.read(cx).current_view.clone();
        match view_kind {
            crate::app_state::ViewKind::Home => {
                views::home::render(self.state.clone(), cx).into_any_element()
            }
            crate::app_state::ViewKind::AccountDetail(account_id) => {
                views::detail::render(self.state.clone(), account_id, cx).into_any_element()
            }
        }
    }

    fn render_bottombar(&self, progress_text: String, cx: &mut Context<Self>) -> AnyElement {
        let state = self.state.clone();
        let log_open = state.read(cx).log_drawer_open;
        let log_btn_bg = if log_open {
            theme::btn_primary_bg()
        } else {
            theme::bg_card()
        };
        let log_btn_border = if log_open {
            theme::btn_primary_border()
        } else {
            theme::border_normal()
        };
        let log_btn_text = if log_open {
            theme::accent_blue()
        } else {
            theme::text_muted()
        };

        div()
            .w_full()
            .h(px(28.0))
            .bg(theme::bg_bar())
            .border_t_1()
            .border_color(theme::border_normal())
            .flex()
            .items_center()
            .justify_between()
            .px(px(12.0))
            .child(
                div()
                    .text_color(theme::text_muted())
                    .text_size(px(10.0))
                    .child(progress_text),
            )
            .child(
                div()
                    .id("log-toggle-btn")
                    .px(px(8.0))
                    .py(px(2.0))
                    .bg(log_btn_bg)
                    .border_1()
                    .border_color(log_btn_border)
                    .rounded(px(5.0))
                    .text_color(log_btn_text)
                    .text_size(px(10.0))
                    .cursor_pointer()
                    .hover(|this| this.opacity(0.85))
                    .child("▤ 日志")
                    .on_click(move |_, _window, cx| {
                        state.update(cx, |state, cx| {
                            state.log_drawer_open = !state.log_drawer_open;
                            cx.notify();
                        });
                    }),
            )
            .into_any_element()
    }
}

/// GUI 入口（由 main.rs 调用）
pub fn run_app(storage: anyrouter_core::storage::Storage) {
    application().run(move |cx: &mut App| {
        let state = cx.new(|_| AppState::from_storage(storage));
        let bounds = Bounds::centered(None, size(px(1100.0), px(750.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| RootView::new(state.clone(), cx)),
        )
        .unwrap();
        cx.activate(true);
    });
}
