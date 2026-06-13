use gpui::{AnyElement, Context, Entity, IntoElement, ParentElement, Styled, div, prelude::*, px};

use crate::app_state::{AppState, DeleteTarget, LogEntry, LogLevel, ModalKind, ViewKind};
use crate::theme;
use crate::views::root::RootView;

pub fn render(state: Entity<AppState>, cx: &mut Context<RootView>) -> AnyElement {
    let modal_kind = state.read(cx).active_modal.clone();
    let Some(modal_kind) = modal_kind else {
        return div().into_any_element();
    };

    let inner = match &modal_kind {
        ModalKind::AccountList(site_id) => render_account_list(*site_id, state.clone(), cx),
        ModalKind::ConfirmDelete(target) => render_confirm_delete(target.clone(), state.clone()),
        ModalKind::SiteForm(id) => render_site_form_placeholder(*id, state.clone()),
        ModalKind::AccountForm { site_id, account_id } => {
            render_account_form_placeholder(*site_id, *account_id, state.clone())
        }
    };

    let state_close = state;
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .bg(theme::overlay())
        .flex()
        .items_center()
        .justify_center()
        .id("modal-overlay")
        .on_click(move |_, _w, cx| {
            // 点击遮罩层关闭弹窗
            state_close.update(cx, |st, cx| {
                st.active_modal = None;
                cx.notify();
            });
        })
        .child(inner)
        .into_any_element()
}

// ============================================================================
// 账户列表弹窗
// ============================================================================
fn render_account_list(
    site_id: i64,
    state: Entity<AppState>,
    cx: &mut Context<RootView>,
) -> AnyElement {
    let snap = state.read(cx);
    let (site_name, accounts) = if let Some(ref storage) = snap.storage {
        let site_name = storage
            .get_site(site_id)
            .ok()
            .flatten()
            .map(|s| s.name)
            .unwrap_or_else(|| "未知站点".into());
        let accounts = storage.list_accounts_by_site(site_id).unwrap_or_default();
        (site_name, accounts)
    } else {
        ("未连接".into(), Vec::new())
    };

    let mut list = div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .max_h(px(360.0))
        .id("account-list-scroll")
        .overflow_y_scroll();

    if accounts.is_empty() {
        list = list.child(
            div()
                .px(px(12.0))
                .py(px(8.0))
                .text_color(theme::text_weakest())
                .text_size(px(11.0))
                .child("暂无账户"),
        );
    }

    for account in accounts {
        let acc_id = account.id;
        let acc_name = account.name.clone();
        let cookie_status = match (&account.cookie_issued_at, &account.cookie_expires_at) {
            (Some(_), Some(exp)) => {
                let now = chrono::Utc::now().to_rfc3339();
                if exp < &now {
                    "⚠ Cookie 已过期".to_string()
                } else {
                    format!("Cookie 至 {}", exp.split('T').next().unwrap_or(exp))
                }
            }
            (Some(_), None) => "Cookie（有效期未知）".to_string(),
            _ => "无 Cookie".to_string(),
        };

        let state_detail = state.clone();
        let state_delete = state.clone();
        let acc_name_for_delete = acc_name.clone();

        list = list.child(
            div()
                .w_full()
                .px(px(12.0))
                .py(px(8.0))
                .bg(theme::bg_card())
                .border_1()
                .border_color(theme::border_normal())
                .rounded(px(6.0))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .gap(px(12.0))
                        .text_size(px(11.0))
                        .child(div().text_color(theme::text_primary()).child(acc_name))
                        .child(
                            div()
                                .text_color(theme::text_weakest())
                                .child(cookie_status),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .text_size(px(10.0))
                        .child(
                            div()
                                .id(("acc-detail", acc_id as usize))
                                .px(px(8.0))
                                .py(px(2.0))
                                .bg(theme::btn_primary_bg())
                                .border_1()
                                .border_color(theme::btn_primary_border())
                                .rounded(px(4.0))
                                .text_color(theme::accent_blue())
                                .cursor_pointer()
                                .hover(|this| this.opacity(0.85))
                                .child("详情")
                                .on_click(move |_, _w, cx| {
                                    state_detail.update(cx, |st, cx| {
                                        st.current_view = ViewKind::AccountDetail(acc_id);
                                        st.active_modal = None;
                                        st.active_tab = 0;
                                        cx.notify();
                                    });
                                }),
                        )
                        .child(
                            div()
                                .id(("acc-delete", acc_id as usize))
                                .text_color(theme::text_weakest())
                                .cursor_pointer()
                                .hover(|this| this.text_color(theme::error_red()))
                                .child("🗑 删除")
                                .on_click(move |_, _w, cx| {
                                    let nm = acc_name_for_delete.clone();
                                    state_delete.update(cx, |st, cx| {
                                        st.active_modal = Some(ModalKind::ConfirmDelete(
                                            DeleteTarget::Account { id: acc_id, name: nm },
                                        ));
                                        cx.notify();
                                    });
                                }),
                        ),
                ),
        );
    }

    let state_close = state.clone();
    let state_checkin = state;
    panel(
        format!("{} · 账户管理", site_name),
        list.into_any_element(),
        vec![
            PanelButton {
                label: "⚡ 签到本站点".into(),
                danger: false,
                on_click: Box::new(move |cx| {
                    crate::views::root::trigger_checkin_site(state_checkin.clone(), site_id, cx);
                    state_checkin.update(cx, |st, cx| {
                        st.active_modal = None;
                        cx.notify();
                    });
                }),
            },
            PanelButton {
                label: "关闭".into(),
                danger: false,
                on_click: Box::new(move |cx| {
                    state_close.update(cx, |st, cx| {
                        st.active_modal = None;
                        cx.notify();
                    });
                }),
            },
        ],
        px(520.0),
    )
}

// ============================================================================
// 删除确认弹窗
// ============================================================================
fn render_confirm_delete(target: DeleteTarget, state: Entity<AppState>) -> AnyElement {
    let (body, target_clone) = match &target {
        DeleteTarget::Site {
            name,
            account_count,
            ..
        } => (
            format!(
                "确定删除站点「{}」？将同时删除其下 {} 个账户及全部缓存数据，此操作不可恢复。",
                name, account_count
            ),
            target.clone(),
        ),
        DeleteTarget::Account { name, .. } => (
            format!("确定删除账户「{}」？此操作不可恢复。", name),
            target.clone(),
        ),
    };

    let state_cancel = state.clone();
    let state_confirm = state;

    panel(
        "⚠ 删除确认".to_string(),
        div()
            .text_color(theme::text_secondary())
            .text_size(px(11.0))
            .child(body)
            .into_any_element(),
        vec![
            PanelButton {
                label: "取消".into(),
                danger: false,
                on_click: Box::new(move |cx| {
                    state_cancel.update(cx, |st, cx| {
                        st.active_modal = None;
                        cx.notify();
                    });
                }),
            },
            PanelButton {
                label: "确认删除".into(),
                danger: true,
                on_click: Box::new(move |cx| {
                    state_confirm.update(cx, |st, cx| {
                        let now = chrono::Local::now().format("%H:%M:%S").to_string();
                        let result = if let Some(ref storage) = st.storage {
                            match &target_clone {
                                DeleteTarget::Site { id, name, .. } => storage
                                    .delete_site(*id)
                                    .map(|_| format!("已删除站点「{}」", name)),
                                DeleteTarget::Account { id, name } => storage
                                    .delete_account(*id)
                                    .map(|_| format!("已删除账户「{}」", name)),
                            }
                        } else {
                            Err(anyhow::anyhow!("数据库未初始化"))
                        };

                        match result {
                            Ok(msg) => st.log_entries.push(LogEntry {
                                timestamp: now,
                                level: LogLevel::Success,
                                message: msg,
                            }),
                            Err(e) => st.log_entries.push(LogEntry {
                                timestamp: now,
                                level: LogLevel::Error,
                                message: format!("删除失败: {}", e),
                            }),
                        }

                        st.active_modal = None;
                        st.reload_sites();
                        cx.notify();
                    });
                }),
            },
        ],
        px(420.0),
    )
}

// ============================================================================
// 站点表单弹窗（占位，输入框待 GPUI TextField 支持后实现）
// ============================================================================
fn render_site_form_placeholder(id: Option<i64>, state: Entity<AppState>) -> AnyElement {
    let title = if id.is_some() {
        "编辑站点".to_string()
    } else {
        "新建站点".to_string()
    };

    let state_close = state;
    panel(
        title,
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .text_color(theme::text_secondary())
            .text_size(px(11.0))
            .child("当前版本暂未支持图形化文本输入，可通过以下方式管理站点：")
            .child(
                div()
                    .pl(px(12.0))
                    .text_color(theme::text_weakest())
                    .child("• 修改 .env 中的 PROVIDERS 环境变量"),
            )
            .child(
                div()
                    .pl(px(12.0))
                    .text_color(theme::text_weakest())
                    .child("• 直接编辑 SQLite 数据库（%APPDATA%/anyrouter-checkin/data.db）"),
            )
            .child(
                div()
                    .pl(px(12.0))
                    .text_color(theme::text_weakest())
                    .child("• 后续版本将提供完整文本输入界面"),
            )
            .into_any_element(),
        vec![PanelButton {
            label: "我知道了".into(),
            danger: false,
            on_click: Box::new(move |cx| {
                state_close.update(cx, |st, cx| {
                    st.active_modal = None;
                    cx.notify();
                });
            }),
        }],
        px(420.0),
    )
}

// ============================================================================
// 账户表单弹窗（占位）
// ============================================================================
fn render_account_form_placeholder(
    _site_id: i64,
    account_id: Option<i64>,
    state: Entity<AppState>,
) -> AnyElement {
    let title = if account_id.is_some() {
        "编辑账户".to_string()
    } else {
        "新增账户".to_string()
    };

    let state_close = state;
    panel(
        title,
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .text_color(theme::text_secondary())
            .text_size(px(11.0))
            .child("账户管理建议通过 .env 中的 ANYROUTER_ACCOUNTS 完成。")
            .child(
                div()
                    .text_color(theme::text_weakest())
                    .child("修改后重启 GUI 会自动同步到 SQLite。"),
            )
            .into_any_element(),
        vec![PanelButton {
            label: "我知道了".into(),
            danger: false,
            on_click: Box::new(move |cx| {
                state_close.update(cx, |st, cx| {
                    st.active_modal = None;
                    cx.notify();
                });
            }),
        }],
        px(420.0),
    )
}

// ============================================================================
// 通用面板组件
// ============================================================================
struct PanelButton {
    label: String,
    danger: bool,
    on_click: Box<dyn Fn(&mut gpui::App) + 'static>,
}

fn panel(
    title: String,
    body: AnyElement,
    buttons: Vec<PanelButton>,
    width: gpui::Pixels,
) -> AnyElement {
    let mut btn_row = div().flex().justify_end().gap(px(8.0)).mt(px(12.0));
    for (idx, btn) in buttons.into_iter().enumerate() {
        let danger = btn.danger;
        let label = btn.label.clone();
        let on_click = btn.on_click;
        btn_row = btn_row.child(
            div()
                .id(("panel-btn", idx))
                .px(px(14.0))
                .py(px(4.0))
                .bg(if danger {
                    theme::btn_danger_bg()
                } else {
                    theme::btn_primary_bg()
                })
                .border_1()
                .border_color(if danger {
                    theme::btn_danger_border()
                } else {
                    theme::btn_primary_border()
                })
                .rounded(px(5.0))
                .text_color(if danger {
                    theme::error_red()
                } else {
                    theme::accent_blue()
                })
                .text_size(px(11.0))
                .cursor_pointer()
                .hover(|this| this.opacity(0.85))
                .child(label)
                .on_click(move |_, _w, cx| {
                    on_click(cx);
                }),
        );
    }

    div()
        .id("modal-panel")
        .w(width)
        .bg(theme::bg_bar())
        .border_1()
        .border_color(theme::border_accent())
        .rounded(px(10.0))
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .on_click(|_, _w, _cx| {
            // 阻止点击面板时关闭弹窗（事件不冒泡到 overlay）
        })
        .child(
            div()
                .text_color(theme::text_primary())
                .text_size(px(13.0))
                .child(title),
        )
        .child(body)
        .child(btn_row)
        .into_any_element()
}
