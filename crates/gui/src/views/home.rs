use gpui::{AnyElement, Context, Entity, IntoElement, ParentElement, Styled, div, prelude::*, px};

use crate::app_state::{AppState, ModalKind};
use crate::theme;
use crate::views::root::RootView;

pub fn render(state: Entity<AppState>, cx: &mut Context<RootView>) -> AnyElement {
    let snap = state.read(cx);
    let site_count = snap.sites.len();
    let account_count: usize = snap.sites.iter().map(|s| s.account_count).sum();
    let total_balance: f64 = snap.sites.iter().map(|s| s.total_balance).sum();
    let total_today: usize = snap.sites.iter().map(|s| s.checkin_today).sum();
    let total_today_target = account_count;

    let sites_data: Vec<_> = snap.sites.clone();

    let is_empty = sites_data.is_empty();
    let sqlite_enabled = snap.sqlite_enabled;

    div()
        .id("home-scroll")
        .size_full()
        .overflow_y_scroll()
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(render_stats_strip(
            site_count,
            account_count,
            total_balance,
            total_today,
            total_today_target,
        ))
        .when(is_empty, |this| this.child(render_empty_hint()))
        .child(render_site_grid(sites_data, state, sqlite_enabled))
        .into_any_element()
}

fn render_empty_hint() -> AnyElement {
    div()
        .w_full()
        .p(px(24.0))
        .bg(theme::bg_card())
        .border_1()
        .border_dashed()
        .border_color(theme::border_normal())
        .rounded(px(10.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .items_center()
        .text_color(theme::text_muted())
        .text_size(px(12.0))
        .child(
            div()
                .text_color(theme::text_primary())
                .text_size(px(14.0))
                .child("欢迎使用 AnyRouter 客户端 👋"),
        )
        .child(div().child("当前没有任何站点。可通过以下方式添加："))
        .child(
            div()
                .text_color(theme::text_weakest())
                .text_size(px(11.0))
                .child("• 在 .env 文件设置 ANYROUTER_ACCOUNTS 后重启应用（自动导入）"),
        )
        .child(
            div()
                .text_color(theme::text_weakest())
                .text_size(px(11.0))
                .child("• 直接编辑 SQLite 数据库 %APPDATA%/anyrouter-checkin/data.db"),
        )
        .into_any_element()
}

fn render_stats_strip(
    sites: usize,
    accounts: usize,
    total_balance: f64,
    today: usize,
    today_total: usize,
) -> impl IntoElement {
    div()
        .w_full()
        .h(px(40.0))
        .bg(theme::bg_stats_bar())
        .border_1()
        .border_color(theme::border_normal())
        .rounded(px(8.0))
        .flex()
        .items_center()
        .px(px(16.0))
        .gap(px(28.0))
        .text_color(theme::text_muted())
        .text_size(px(11.0))
        .child(stat_item("站点", format!("{}", sites)))
        .child(stat_item("账户", format!("{}", accounts)))
        .child(stat_item("总余额", format!("${:.2}", total_balance)))
        .child(stat_item("今日签到", format!("{}/{}", today, today_total)))
}

fn stat_item(label: &'static str, value: String) -> impl IntoElement {
    div()
        .flex()
        .gap(px(6.0))
        .child(div().child(label))
        .child(div().text_color(theme::text_primary()).child(value))
}

fn render_site_grid(
    sites: Vec<anyrouter_core::models::SiteWithStats>,
    state: Entity<AppState>,
    sqlite_enabled: bool,
) -> impl IntoElement {
    let mut grid = div().flex().flex_wrap().gap(px(12.0));

    for s in sites {
        grid = grid.child(render_site_card(s, state.clone(), sqlite_enabled));
    }

    if sqlite_enabled {
        grid = grid.child(render_new_site_card(state));
    }
    grid
}

fn render_site_card(
    site_stats: anyrouter_core::models::SiteWithStats,
    state: Entity<AppState>,
    sqlite_enabled: bool,
) -> impl IntoElement {
    let site_id = site_stats.site.id;
    let site_name = site_stats.site.name.clone();
    let domain = site_stats.site.domain.clone();
    let account_count = site_stats.account_count;
    let expired_count = site_stats.expired_count;
    let total_balance = site_stats.total_balance;

    let state_for_view = state.clone();
    let state_for_edit = state.clone();
    let state_for_delete = state;
    let name_for_delete = site_name.clone();

    div()
        .w(px(280.0))
        .bg(theme::bg_card())
        .border_1()
        .border_color(theme::border_normal())
        .rounded(px(8.0))
        .p(px(12.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_color(theme::text_primary())
                .text_size(px(13.0))
                .child(site_name),
        )
        .child(
            div()
                .text_color(theme::text_weakest())
                .text_size(px(10.0))
                .child(domain),
        )
        .child(
            div()
                .text_color(theme::text_secondary())
                .text_size(px(11.0))
                .child(if expired_count > 0 {
                    format!("账户：{} 个（{} 个 Cookie 过期 ⚠）", account_count, expired_count)
                } else {
                    format!("账户：{} 个", account_count)
                }),
        )
        .child(
            div()
                .text_color(theme::text_secondary())
                .text_size(px(11.0))
                .child(format!("余额合计：${:.2}", total_balance)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .mt(px(6.0))
                .child(
                    div()
                        .id(("site-view-btn", site_id as usize))
                        .px(px(10.0))
                        .py(px(3.0))
                        .bg(theme::btn_primary_bg())
                        .border_1()
                        .border_color(theme::btn_primary_border())
                        .rounded(px(5.0))
                        .text_color(theme::accent_blue())
                        .text_size(px(10.0))
                        .cursor_pointer()
                        .hover(|this| this.opacity(0.85))
                        .child("查看账户")
                        .on_click(move |_, _w, cx| {
                            state_for_view.update(cx, |st, cx| {
                                st.active_modal = Some(ModalKind::AccountList(site_id));
                                cx.notify();
                            });
                        }),
                )
                .when(sqlite_enabled, |row| {
                    row.child(
                        div()
                            .id(("site-edit-btn", site_id as usize))
                            .text_color(theme::text_weakest())
                            .text_size(px(10.0))
                            .cursor_pointer()
                            .hover(|this| this.text_color(theme::text_secondary()))
                            .child("✎ 编辑")
                            .on_click(move |_, _w, cx| {
                                state_for_edit.update(cx, |st, cx| {
                                    st.form_error = None;
                                    if let Some(ref storage) = st.storage {
                                        if let Ok(Some(site)) = storage.get_site(site_id) {
                                            st.site_form = Some(
                                                crate::app_state::SiteFormFields::new_edit(cx, &site),
                                            );
                                            st.active_modal =
                                                Some(ModalKind::SiteForm(Some(site_id)));
                                        }
                                    }
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        div()
                            .id(("site-delete-btn", site_id as usize))
                            .text_color(theme::text_weakest())
                            .text_size(px(10.0))
                            .cursor_pointer()
                            .hover(|this| this.text_color(theme::error_red()))
                            .child("🗑 删除")
                            .on_click(move |_, _w, cx| {
                                let nm = name_for_delete.clone();
                                state_for_delete.update(cx, |st, cx| {
                                    st.active_modal = Some(ModalKind::ConfirmDelete(
                                        crate::app_state::DeleteTarget::Site {
                                            id: site_id,
                                            name: nm,
                                            account_count,
                                        },
                                    ));
                                    cx.notify();
                                });
                            }),
                    )
                })
                .when(!sqlite_enabled, |row| {
                    row.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::text_weakest())
                            .child("（启用 SQLite 源后可编辑）"),
                    )
                }),
        )
}

fn render_new_site_card(state: Entity<AppState>) -> impl IntoElement {
    div()
        .id("new-site-card")
        .w(px(280.0))
        .h(px(140.0))
        .bg(theme::bg_card())
        .border_1()
        .border_dashed()
        .border_color(theme::border_normal())
        .rounded(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .text_color(theme::text_weakest())
        .text_size(px(12.0))
        .cursor_pointer()
        .hover(|this| {
            this.border_color(theme::accent_blue())
                .text_color(theme::accent_blue())
        })
        .child("+ 新建站点")
        .on_click(move |_, _w, cx| {
            state.update(cx, |st, cx| {
                st.form_error = None;
                st.site_form = Some(crate::app_state::SiteFormFields::new_create(cx));
                st.active_modal = Some(ModalKind::SiteForm(None));
                cx.notify();
            });
        })
}
