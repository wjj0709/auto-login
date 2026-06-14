use gpui::{AnyElement, Context, Entity, IntoElement, ParentElement, Styled, div, prelude::*, px};

use anyrouter_core::config_loader::SourceKind;

use crate::app_state::{AppState, LogEntry, LogLevel};
use crate::theme;
use crate::views::modals::{panel, PanelButton};
use crate::views::root::RootView;

pub fn render(state: Entity<AppState>, cx: &mut Context<RootView>) -> AnyElement {
    let snap = state.read(cx);
    let env_on = snap.settings_env;
    let file_on = snap.settings_file;
    let sqlite_on = snap.settings_sqlite;
    let none_selected = !env_on && !file_on && !sqlite_on;

    let body = div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .text_size(px(11.0))
                .text_color(theme::text_muted())
                .child("选择启用的配置数据源（按 环境变量 → 配置文件 → 数据库 顺序合并，后者优先）："),
        )
        .child(checkbox_row("环境变量 (含 .env)", env_on, SourceKind::Env, state.clone()))
        .child(checkbox_row("JSON 配置文件", file_on, SourceKind::File, state.clone()))
        .child(checkbox_row("SQLite 数据库", sqlite_on, SourceKind::Sqlite, state.clone()))
        .when(none_selected, |this| {
            this.child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme::error_red())
                    .child("至少需启用一个数据源"),
            )
        })
        .into_any_element();

    let state_close = state.clone();
    let state_save = state;

    panel(
        "设置 · 数据源".to_string(),
        body,
        vec![
            PanelButton {
                label: "取消".into(),
                danger: false,
                on_click: Box::new(move |cx| {
                    state_close.update(cx, |st, cx| {
                        st.active_modal = None;
                        cx.notify();
                    });
                }),
            },
            PanelButton {
                label: "保存".into(),
                danger: false,
                on_click: Box::new(move |cx| {
                    state_save.update(cx, |st, cx| {
                        if !st.settings_env && !st.settings_file && !st.settings_sqlite {
                            return; // 至少一个
                        }
                        let mut list: Vec<&str> = Vec::new();
                        if st.settings_env {
                            list.push("env");
                        }
                        if st.settings_file {
                            list.push("file");
                        }
                        if st.settings_sqlite {
                            list.push("sqlite");
                        }
                        let val = list.join(",");
                        if let Some(ref storage) = st.storage {
                            let _ = storage.set_meta("cfg_sources", &val);
                        }
                        st.sqlite_enabled = st.settings_sqlite;
                        st.active_modal = None;
                        st.log_entries.push(LogEntry {
                            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                            level: LogLevel::Info,
                            message: format!("数据源已更新: [{}]（重启后完全生效）", val),
                        });
                        st.reload_sites();
                        cx.notify();
                    });
                }),
            },
        ],
        px(360.0),
    )
}

/// 一个勾选行（方块 + 文字），点击切换对应源的临时勾选状态。
fn checkbox_row(
    label: &str,
    on: bool,
    kind: SourceKind,
    state: Entity<AppState>,
) -> AnyElement {
    let id = match kind {
        SourceKind::Env => "set-env",
        SourceKind::File => "set-file",
        SourceKind::Sqlite => "set-sqlite",
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(8.0))
        .cursor_pointer()
        .child(
            div()
                .w(px(14.0))
                .h(px(14.0))
                .rounded(px(3.0))
                .border_1()
                .border_color(if on {
                    theme::accent_blue()
                } else {
                    theme::border_accent()
                })
                .flex()
                .items_center()
                .justify_center()
                .when(on, |this| {
                    this.bg(theme::accent_blue()).child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::bg_window())
                            .child("✓"),
                    )
                }),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme::text_primary())
                .child(label.to_string()),
        )
        .on_click(move |_, _w, cx| {
            state.update(cx, |st, cx| {
                match kind {
                    SourceKind::Env => st.settings_env = !st.settings_env,
                    SourceKind::File => st.settings_file = !st.settings_file,
                    SourceKind::Sqlite => st.settings_sqlite = !st.settings_sqlite,
                }
                cx.notify();
            });
        })
        .into_any_element()
}
