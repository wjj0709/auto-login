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
        ModalKind::SiteForm(_) => render_site_form(state.clone(), cx),
        ModalKind::AccountForm { .. } => render_account_form(state.clone(), cx),
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
        let state_edit = state.clone();
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
                                    // 进入详情页后自动登录并拉取详情数据
                                    crate::views::root::trigger_fetch_detail(
                                        state_detail.clone(),
                                        acc_id,
                                        cx,
                                    );
                                }),
                        )
                        .child(
                            div()
                                .id(("acc-edit", acc_id as usize))
                                .text_color(theme::text_weakest())
                                .cursor_pointer()
                                .hover(|this| this.text_color(theme::text_secondary()))
                                .child("✎ 编辑")
                                .on_click(move |_, _w, cx| {
                                    state_edit.update(cx, |st, cx| {
                                        st.form_error = None;
                                        if let Some(ref storage) = st.storage {
                                            if let Ok(Some(acc)) = storage.get_account(acc_id) {
                                                st.account_form = Some(
                                                    crate::app_state::AccountFormFields::new_edit(
                                                        cx, &acc,
                                                    ),
                                                );
                                                st.active_modal = Some(ModalKind::AccountForm {
                                                    site_id: acc.site_id,
                                                    account_id: Some(acc_id),
                                                });
                                            }
                                        }
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
    let state_checkin = state.clone();
    let state_add = state;
    panel(
        format!("{} · 账户管理", site_name),
        list.into_any_element(),
        vec![
            PanelButton {
                label: "+ 新增账户".into(),
                danger: false,
                on_click: Box::new(move |cx| {
                    state_add.update(cx, |st, cx| {
                        st.form_error = None;
                        st.account_form =
                            Some(crate::app_state::AccountFormFields::new_create(cx, site_id));
                        st.active_modal = Some(ModalKind::AccountForm {
                            site_id,
                            account_id: None,
                        });
                        cx.notify();
                    });
                }),
            },
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
// 站点表单弹窗（真实文本输入）
// ============================================================================
fn render_site_form(state: Entity<AppState>, cx: &mut Context<RootView>) -> AnyElement {
    let snap = state.read(cx);
    let Some(form) = snap.site_form.as_ref() else {
        return div().into_any_element();
    };
    let title = if form.editing_id.is_some() {
        "编辑站点".to_string()
    } else {
        "新建站点".to_string()
    };
    let show_advanced = form.show_advanced;
    let form_error = snap.form_error.clone();

    // 克隆输入框句柄（用于渲染与保存闭包）
    let name = form.name.clone();
    let domain = form.domain.clone();
    let login_path = form.login_path.clone();
    let sign_in_path = form.sign_in_path.clone();
    let user_info_path = form.user_info_path.clone();
    let tokens_path = form.tokens_path.clone();
    let logs_path = form.logs_path.clone();
    let chart_path = form.chart_path.clone();
    let api_user_key = form.api_user_key.clone();

    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(field_row("站点名称", true, name.clone()))
        .child(field_row("域名（http:// 或 https://）", true, domain.clone()));

    // 高级设置折叠区
    let state_toggle = state.clone();
    body = body.child(
        div()
            .id("toggle-advanced")
            .text_size(px(10.0))
            .text_color(theme::text_muted())
            .cursor_pointer()
            .hover(|this| this.text_color(theme::accent_blue()))
            .child(if show_advanced {
                "▾ 高级设置"
            } else {
                "▸ 高级设置（路径配置）"
            })
            .on_click(move |_, _w, cx| {
                state_toggle.update(cx, |st, cx| {
                    if let Some(f) = st.site_form.as_mut() {
                        f.show_advanced = !f.show_advanced;
                    }
                    cx.notify();
                });
            }),
    );

    if show_advanced {
        body = body
            .child(field_row("登录页路径", false, login_path.clone()))
            .child(field_row("签到接口（留空=自动签到型）", false, sign_in_path.clone()))
            .child(field_row("用户信息接口", false, user_info_path.clone()))
            .child(field_row("密钥列表接口", false, tokens_path.clone()))
            .child(field_row("使用日志接口", false, logs_path.clone()))
            .child(field_row("图表数据接口", false, chart_path.clone()))
            .child(field_row("API 用户请求头名", false, api_user_key.clone()));
    }

    if let Some(err) = form_error {
        body = body.child(
            div()
                .text_size(px(10.0))
                .text_color(theme::error_red())
                .child(err),
        );
    }

    let state_cancel = state.clone();
    let state_save = state;

    panel(
        title,
        body.into_any_element(),
        vec![
            PanelButton {
                label: "取消".into(),
                danger: false,
                on_click: Box::new(move |cx| {
                    state_cancel.update(cx, |st, cx| {
                        st.active_modal = None;
                        st.site_form = None;
                        st.form_error = None;
                        cx.notify();
                    });
                }),
            },
            PanelButton {
                label: "保存".into(),
                danger: false,
                on_click: Box::new(move |cx| {
                    // 先读取所有输入框文本（owned），再进入 update 避免借用冲突
                    let name_v = name.read(cx).text().trim().to_string();
                    let domain_v = domain.read(cx).text().trim().to_string();
                    let login_v = login_path.read(cx).text().trim().to_string();
                    let sign_in_v = sign_in_path.read(cx).text().trim().to_string();
                    let user_info_v = user_info_path.read(cx).text().trim().to_string();
                    let tokens_v = tokens_path.read(cx).text().trim().to_string();
                    let logs_v = logs_path.read(cx).text().trim().to_string();
                    let chart_v = chart_path.read(cx).text().trim().to_string();
                    let api_key_v = api_user_key.read(cx).text().trim().to_string();

                    state_save.update(cx, |st, cx| {
                        // 校验
                        if name_v.is_empty() || domain_v.is_empty() {
                            st.form_error = Some("站点名称和域名为必填项".into());
                            cx.notify();
                            return;
                        }
                        if !(domain_v.starts_with("http://") || domain_v.starts_with("https://")) {
                            st.form_error = Some("域名必须以 http:// 或 https:// 开头".into());
                            cx.notify();
                            return;
                        }

                        let input = anyrouter_core::models::SiteInput {
                            name: name_v.clone(),
                            domain: domain_v,
                            login_path: if login_v.is_empty() { "/login".into() } else { login_v },
                            sign_in_path: if sign_in_v.is_empty() { None } else { Some(sign_in_v) },
                            user_info_path: if user_info_v.is_empty() {
                                "/api/user/self".into()
                            } else {
                                user_info_v
                            },
                            tokens_path: if tokens_v.is_empty() { "/api/token/".into() } else { tokens_v },
                            logs_path: if logs_v.is_empty() { "/api/log/self".into() } else { logs_v },
                            chart_path: if chart_v.is_empty() { "/api/data/self".into() } else { chart_v },
                            api_user_key: if api_key_v.is_empty() {
                                "new-api-user".into()
                            } else {
                                api_key_v
                            },
                        };

                        let editing_id = st.site_form.as_ref().and_then(|f| f.editing_id);
                        let now = chrono::Local::now().format("%H:%M:%S").to_string();
                        let result = if let Some(ref storage) = st.storage {
                            match editing_id {
                                Some(id) => storage.update_site(id, &input).map(|_| {
                                    format!("已更新站点「{}」", name_v)
                                }),
                                None => storage
                                    .insert_site(&input)
                                    .map(|_| format!("已新建站点「{}」", name_v)),
                            }
                        } else {
                            Err(anyhow::anyhow!("数据库未初始化"))
                        };

                        match result {
                            Ok(msg) => {
                                st.log_entries.push(LogEntry {
                                    timestamp: now,
                                    level: LogLevel::Success,
                                    message: msg,
                                });
                                st.active_modal = None;
                                st.site_form = None;
                                st.form_error = None;
                                st.reload_sites();
                            }
                            Err(e) => {
                                st.form_error = Some(format!("保存失败: {}", e));
                            }
                        }
                        cx.notify();
                    });
                }),
            },
        ],
        px(440.0),
    )
}

// ============================================================================
// 账户表单弹窗（真实文本输入）
// ============================================================================
fn render_account_form(state: Entity<AppState>, cx: &mut Context<RootView>) -> AnyElement {
    let snap = state.read(cx);
    let Some(form) = snap.account_form.as_ref() else {
        return div().into_any_element();
    };
    let title = if form.editing_id.is_some() {
        "编辑账户".to_string()
    } else {
        "新增账户".to_string()
    };
    let form_error = snap.form_error.clone();
    let cred_method = form.cred_method;

    let name = form.name.clone();
    let api_user = form.api_user.clone();
    let cookie = form.cookie.clone();
    let username = form.username.clone();
    let password = form.password.clone();

    use crate::app_state::CredMethod;

    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(field_row("账户名称", true, name.clone()))
        .child(field_row("API 用户标识（new-api-user 值）", true, api_user.clone()))
        // 凭据方式单选
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(16.0))
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme::text_muted())
                        .child("凭据方式："),
                )
                .child(radio_option(
                    "Cookie",
                    cred_method == CredMethod::Cookie,
                    state.clone(),
                    CredMethod::Cookie,
                ))
                .child(radio_option(
                    "账号密码",
                    cred_method == CredMethod::Password,
                    state.clone(),
                    CredMethod::Password,
                )),
        );

    // 根据所选方式展示对应输入框
    body = match cred_method {
        CredMethod::Cookie => body.child(field_row("Cookie（支持多行/自动换行）", true, cookie.clone())),
        CredMethod::Password => body
            .child(field_row("登录用户名", true, username.clone()))
            .child(field_row("登录密码", true, password.clone())),
    };

    if let Some(err) = form_error {
        body = body.child(
            div()
                .text_size(px(10.0))
                .text_color(theme::error_red())
                .child(err),
        );
    }

    let state_cancel = state.clone();
    let state_save = state;

    panel(
        title,
        body.into_any_element(),
        vec![
            PanelButton {
                label: "取消".into(),
                danger: false,
                on_click: Box::new(move |cx| {
                    state_cancel.update(cx, |st, cx| {
                        st.active_modal = None;
                        st.account_form = None;
                        st.form_error = None;
                        cx.notify();
                    });
                }),
            },
            PanelButton {
                label: "保存".into(),
                danger: false,
                on_click: Box::new(move |cx| {
                    let name_v = name.read(cx).text().trim().to_string();
                    let api_user_v = api_user.read(cx).text().trim().to_string();
                    let cookie_v = cookie.read(cx).text().trim().to_string();
                    let username_v = username.read(cx).text().trim().to_string();
                    let password_v = password.read(cx).text().trim().to_string();

                    state_save.update(cx, |st, cx| {
                        if name_v.is_empty() || api_user_v.is_empty() {
                            st.form_error = Some("账户名称和 API 用户标识为必填项".into());
                            cx.notify();
                            return;
                        }

                        let method = st
                            .account_form
                            .as_ref()
                            .map(|f| f.cred_method)
                            .unwrap_or(crate::app_state::CredMethod::Cookie);

                        // 按所选方式校验，并仅保存对应方式的数据
                        let (final_cookie, final_username, final_password) = match method {
                            crate::app_state::CredMethod::Cookie => {
                                if cookie_v.is_empty() {
                                    st.form_error = Some("请填写 Cookie".into());
                                    cx.notify();
                                    return;
                                }
                                (Some(cookie_v.clone()), None, None)
                            }
                            crate::app_state::CredMethod::Password => {
                                if username_v.is_empty() || password_v.is_empty() {
                                    st.form_error = Some("请同时填写用户名和密码".into());
                                    cx.notify();
                                    return;
                                }
                                (None, Some(username_v.clone()), Some(password_v.clone()))
                            }
                        };

                        let (site_id, editing_id) = st
                            .account_form
                            .as_ref()
                            .map(|f| (f.site_id, f.editing_id))
                            .unwrap_or((0, None));

                        let input = anyrouter_core::models::AccountInput {
                            site_id,
                            name: name_v.clone(),
                            api_user: api_user_v,
                            username: final_username,
                            password: final_password,
                            cookies: final_cookie,
                        };

                        let now = chrono::Local::now().format("%H:%M:%S").to_string();
                        let result = if let Some(ref storage) = st.storage {
                            match editing_id {
                                Some(id) => storage
                                    .update_account(id, &input)
                                    .map(|_| format!("已更新账户「{}」", name_v)),
                                None => storage
                                    .insert_account(&input)
                                    .map(|_| format!("已新增账户「{}」", name_v)),
                            }
                        } else {
                            Err(anyhow::anyhow!("数据库未初始化"))
                        };

                        match result {
                            Ok(msg) => {
                                st.log_entries.push(LogEntry {
                                    timestamp: now,
                                    level: LogLevel::Success,
                                    message: msg,
                                });
                                st.active_modal = None;
                                st.account_form = None;
                                st.form_error = None;
                                st.reload_sites();
                            }
                            Err(e) => {
                                st.form_error = Some(format!("保存失败: {}", e));
                            }
                        }
                        cx.notify();
                    });
                }),
            },
        ],
        px(440.0),
    )
}

/// 渲染一个带标签的输入框行
fn field_row(label: &str, required: bool, input: Entity<crate::components::text_input::TextInput>) -> AnyElement {
    let label_text = if required {
        format!("{} *", label)
    } else {
        label.to_string()
    };
    div()
        .flex()
        .flex_col()
        .gap(px(3.0))
        .child(
            div()
                .text_size(px(10.0))
                .text_color(if required {
                    theme::text_secondary()
                } else {
                    theme::text_muted()
                })
                .child(label_text),
        )
        .child(input)
        .into_any_element()
}

/// 渲染一个凭据方式单选项（圆点 + 文字），点击切换 cred_method
fn radio_option(
    label: &str,
    selected: bool,
    state: Entity<AppState>,
    method: crate::app_state::CredMethod,
) -> AnyElement {
    let id_str = match method {
        crate::app_state::CredMethod::Cookie => "radio-cookie",
        crate::app_state::CredMethod::Password => "radio-password",
    };
    div()
        .id(id_str)
        .flex()
        .items_center()
        .gap(px(5.0))
        .cursor_pointer()
        .child(
            // 圆点指示器
            div()
                .w(px(12.0))
                .h(px(12.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(if selected {
                    theme::accent_blue()
                } else {
                    theme::border_accent()
                })
                .flex()
                .items_center()
                .justify_center()
                .when(selected, |this| {
                    this.child(
                        div()
                            .w(px(6.0))
                            .h(px(6.0))
                            .rounded(px(3.0))
                            .bg(theme::accent_blue()),
                    )
                }),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(if selected {
                    theme::text_primary()
                } else {
                    theme::text_muted()
                })
                .child(label.to_string()),
        )
        .on_click(move |_, _w, cx| {
            state.update(cx, |st, cx| {
                if let Some(f) = st.account_form.as_mut() {
                    f.cred_method = method;
                }
                st.form_error = None;
                cx.notify();
            });
        })
        .into_any_element()
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
        .on_click(|_, _w, cx| {
            // 阻止点击面板（含内部按钮）时事件冒泡到遮罩层导致弹窗被关闭
            cx.stop_propagation();
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
