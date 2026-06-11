use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::Sizable;

use crate::app_state::{AppState, LogLevel};
use crate::theme::Glass;

/// 实时日志面板
pub struct LogPanel {
    app_state: Entity<AppState>,
    filter: Option<LogLevel>,
}

impl LogPanel {
    pub fn new(app_state: Entity<AppState>) -> Self {
        Self {
            app_state,
            filter: None,
        }
    }

    fn level_color(level: LogLevel) -> Hsla {
        match level {
            LogLevel::Info => Glass::info(),
            LogLevel::Success => Glass::success(),
            LogLevel::Warn => Glass::warning(),
            LogLevel::Error => Glass::danger(),
            LogLevel::Debug => Glass::text_muted(),
        }
    }

    fn level_tag(level: LogLevel) -> &'static str {
        match level {
            LogLevel::Info => "INFO",
            LogLevel::Success => "OK",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERR",
            LogLevel::Debug => "DBG",
        }
    }

    fn render_filter_button(
        &self,
        label: &str,
        level: Option<LogLevel>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = self.filter == level;
        let target = level;

        div()
            .id(SharedString::from(format!("filter-{}", label)))
            .px_2()
            .py_1()
            .rounded(px(6.0))
            .cursor_pointer()
            .bg(if is_active { Glass::primary().opacity(0.2) } else { Glass::transparent() })
            .text_xs()
            .text_color(if is_active { Glass::primary() } else { Glass::text_muted() })
            .hover(|s| s.bg(Glass::card_hover()))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.filter = target;
                cx.notify();
            }))
            .child(label.to_string())
    }
}

impl Render for LogPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.app_state.read(cx);
        let logs: Vec<_> = state.logs.iter()
            .filter(|entry| self.filter.map_or(true, |f| entry.level == f))
            .collect();
        let log_count = state.logs.len();
        let filter = self.filter;

        let mut log_elements = Vec::new();
        for entry in &logs {
            let color = Self::level_color(entry.level);
            let tag = Self::level_tag(entry.level);

            log_elements.push(
                div()
                    .flex()
                    .gap_2()
                    .py(px(2.0))
                    .px_3()
                    .hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.02)))
                    .child(
                        div().text_xs().w(px(70.0)).flex_shrink_0()
                            .text_color(Glass::text_muted())
                            .child(entry.timestamp.to_string())
                    )
                    .child(
                        div().text_xs().w(px(36.0)).flex_shrink_0()
                            .font_weight(FontWeight::BOLD)
                            .text_color(color)
                            .child(tag)
                    )
                    .child(
                        div().text_xs().w(px(60.0)).flex_shrink_0()
                            .text_color(Glass::text_secondary())
                            .child(entry.tag.to_string())
                    )
                    .child(
                        div().text_xs().flex_1()
                            .text_color(Glass::text())
                            .child(entry.message.to_string())
                    )
                    .into_any_element()
            );
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .gap_3()
            // 标题栏
            .child(
                div().flex().items_center().justify_between()
                    .child(
                        div().text_lg().font_weight(FontWeight::BOLD)
                            .text_color(Glass::text())
                            .child("Real-time Logs")
                    )
                    .child(
                        div().flex().items_center().gap_2()
                            .child(
                                div().text_xs().text_color(Glass::text_muted())
                                    .child(format!("{} entries", log_count))
                            )
                            .child(
                                Button::new("clear-logs")
                                    .small()
                                    .ghost()
                                    .label("Clear")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.app_state.update(cx, |state, cx| {
                                            state.clear_logs();
                                            cx.notify();
                                        });
                                    }))
                            )
                    )
            )
            // 过滤按钮
            .child(
                div().flex().gap_1()
                    .child(self.render_filter_button("All", None, cx))
                    .child(self.render_filter_button("Info", Some(LogLevel::Info), cx))
                    .child(self.render_filter_button("Success", Some(LogLevel::Success), cx))
                    .child(self.render_filter_button("Warn", Some(LogLevel::Warn), cx))
                    .child(self.render_filter_button("Error", Some(LogLevel::Error), cx))
                    .child(self.render_filter_button("Debug", Some(LogLevel::Debug), cx))
            )
            // 日志终端
            .child(
                div()
                    .id("log-terminal")
                    .flex_1()
                    .rounded(px(12.0))
                    .bg(Glass::terminal())
                    .border_1()
                    .border_color(Glass::border())
                    .overflow_y_scroll()
                    .py_2()
                    .child(
                        if log_elements.is_empty() {
                            div().p_4().text_sm().text_color(Glass::text_muted())
                                .child("No log entries yet. Click 'Check-in All' to start.")
                                .into_any_element()
                        } else {
                            div().flex().flex_col()
                                .children(log_elements)
                                .into_any_element()
                        }
                    )
            )
    }
}
