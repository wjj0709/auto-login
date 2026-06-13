use gpui::{AnyElement, Context, Entity, IntoElement, ParentElement, Styled, div, prelude::*, px};

use crate::app_state::{AppState, DeleteTarget, ModalKind};
use crate::theme;
use crate::views::root::RootView;

pub fn render(state: Entity<AppState>, cx: &mut Context<RootView>) -> AnyElement {
    let modal = state.read(cx).active_modal.clone();
    let (title, body_text) = match &modal {
        Some(ModalKind::AccountList(site_id)) => (
            format!("站点 #{} · 账户管理", site_id),
            "账户列表 — 完整功能待实现".to_string(),
        ),
        Some(ModalKind::SiteForm(None)) => (
            "新建站点".to_string(),
            "站点表单 — 完整功能待实现".to_string(),
        ),
        Some(ModalKind::SiteForm(Some(id))) => (
            format!("编辑站点 #{}", id),
            "站点表单 — 完整功能待实现".to_string(),
        ),
        Some(ModalKind::AccountForm { account_id: None, .. }) => (
            "新增账户".to_string(),
            "账户表单 — 完整功能待实现".to_string(),
        ),
        Some(ModalKind::AccountForm {
            account_id: Some(id),
            ..
        }) => (
            format!("编辑账户 #{}", id),
            "账户表单 — 完整功能待实现".to_string(),
        ),
        Some(ModalKind::ConfirmDelete(target)) => match target {
            DeleteTarget::Site {
                name,
                account_count,
                ..
            } => (
                "⚠ 删除确认".to_string(),
                format!(
                    "确定删除站点「{}」？将同时删除其下 {} 个账户及全部缓存数据，此操作不可恢复。",
                    name, account_count
                ),
            ),
            DeleteTarget::Account { name, .. } => (
                "⚠ 删除确认".to_string(),
                format!("确定删除账户「{}」？此操作不可恢复。", name),
            ),
        },
        None => return div().into_any_element(),
    };

    let is_danger = matches!(modal, Some(ModalKind::ConfirmDelete(_)));
    let state_close = state.clone();
    let state_confirm = state;

    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .bg(theme::overlay())
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .w(px(420.0))
                .bg(theme::bg_bar())
                .border_1()
                .border_color(theme::border_accent())
                .rounded(px(10.0))
                .p(px(16.0))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_color(theme::text_primary())
                                .text_size(px(13.0))
                                .child(title),
                        )
                        .child(
                            div()
                                .id("modal-x")
                                .text_color(theme::text_weakest())
                                .text_size(px(13.0))
                                .cursor_pointer()
                                .hover(|this| this.text_color(theme::error_red()))
                                .child("✕")
                                .on_click({
                                    let s = state_close.clone();
                                    move |_, _w, cx| {
                                        s.update(cx, |st, cx| {
                                            st.active_modal = None;
                                            cx.notify();
                                        });
                                    }
                                }),
                        ),
                )
                .child(
                    div()
                        .text_color(theme::text_secondary())
                        .text_size(px(11.0))
                        .child(body_text),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.0))
                        .mt(px(8.0))
                        .child(
                            div()
                                .id("modal-cancel")
                                .px(px(12.0))
                                .py(px(4.0))
                                .bg(theme::bg_card())
                                .border_1()
                                .border_color(theme::border_normal())
                                .rounded(px(5.0))
                                .text_color(theme::text_muted())
                                .text_size(px(11.0))
                                .cursor_pointer()
                                .hover(|this| this.opacity(0.85))
                                .child("取消")
                                .on_click({
                                    let s = state_close;
                                    move |_, _w, cx| {
                                        s.update(cx, |st, cx| {
                                            st.active_modal = None;
                                            cx.notify();
                                        });
                                    }
                                }),
                        )
                        .child(
                            div()
                                .id("modal-confirm")
                                .px(px(12.0))
                                .py(px(4.0))
                                .bg(if is_danger {
                                    theme::btn_danger_bg()
                                } else {
                                    theme::btn_primary_bg()
                                })
                                .border_1()
                                .border_color(if is_danger {
                                    theme::btn_danger_border()
                                } else {
                                    theme::btn_primary_border()
                                })
                                .rounded(px(5.0))
                                .text_color(if is_danger {
                                    theme::error_red()
                                } else {
                                    theme::accent_blue()
                                })
                                .text_size(px(11.0))
                                .cursor_pointer()
                                .hover(|this| this.opacity(0.85))
                                .child(if is_danger { "确认删除" } else { "保存" })
                                .on_click(move |_, _w, cx| {
                                    state_confirm.update(cx, |st, cx| {
                                        st.active_modal = None;
                                        cx.notify();
                                    });
                                }),
                        ),
                ),
        )
        .into_any_element()
}
