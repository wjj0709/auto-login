//! 站点 / 账户的弹窗:账户列表、站点表单、账户表单、删除确认。
//!
//! 全部基于 gpui-component 的命令式 `window.open_dialog`:表单输入用 `Entity<InputState>`
//! 句柄(在打开时创建一次,被 builder 闭包捕获,关闭即随之释放),
//! 保存/删除在 on_ok 内同步写库后调用 `reload_state` 刷新 AppState。

use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::dialog::DialogButtonProps;
use gpui_component::input::{Input, InputState};
use gpui_component::notification::Notification;
use gpui_component::{Sizable, WindowExt};

use crate::app_state::{AppState, AppView, CheckInStatus};
use crate::cookie::CookieStatus;
use crate::service::{self, CheckinScope};
use crate::storage::{
    cookies_value_to_entries, host_of, now_iso, AccountInput, Site, SiteInput,
};
use crate::theme::Glass;

/// 重新从数据库读取站点/账户并覆盖 AppState(界面 CRUD 后调用)。
fn reload_state(app_state: &Entity<AppState>, cx: &mut App) {
    let db = app_state.read(cx).db.clone();
    let loaded = {
        let guard = db.lock().unwrap();
        match (guard.list_sites(), guard.list_all_accounts()) {
            (Ok(s), Ok(a)) => Some((s, a)),
            _ => None,
        }
    };
    if let Some((sites, accounts)) = loaded {
        app_state.update(cx, |state, cx| {
            state.replace_data(sites, accounts);
            cx.notify();
        });
    }
}

/// 创建一个文本输入状态(可选密码掩码 / 多行 / 初值)。
fn text_field(
    initial: &str,
    placeholder: &str,
    masked: bool,
    multiline: bool,
    window: &mut Window,
    cx: &mut App,
) -> Entity<InputState> {
    let placeholder = placeholder.to_string();
    let entity = cx.new(|cx| {
        let mut st = InputState::new(window, cx).placeholder(placeholder);
        if masked {
            st = st.masked(true);
        }
        if multiline {
            st = st.multi_line(true);
        }
        st
    });
    if !initial.is_empty() {
        let v = initial.to_string();
        entity.update(cx, |st, cx| st.set_value(v, window, cx));
    }
    entity
}

/// 表单字段(标签 + 输入框)
fn field(label: &str, input: Input) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_xs().text_color(Glass::text_muted()).child(label.to_string()))
        .child(input)
}

fn nonempty_or(value: SharedString, fallback: &str) -> String {
    let t = value.trim();
    if t.is_empty() {
        fallback.to_string()
    } else {
        t.to_string()
    }
}

// ===================== 站点表单 =====================

/// 打开站点表单。`edit_id` 为 None 表示新建。
pub fn open_site_form(
    app_state: &Entity<AppState>,
    edit_id: Option<i64>,
    window: &mut Window,
    cx: &mut App,
) {
    let existing: Option<Site> = edit_id.and_then(|id| app_state.read(cx).site_by_id(id).cloned());
    let d = SiteInput::with_defaults("", "");

    let g = |f: fn(&Site) -> String, fallback: &str| -> String {
        existing.as_ref().map(f).unwrap_or_else(|| fallback.to_string())
    };
    let signin_init = match &existing {
        Some(s) => s.sign_in_path.clone().unwrap_or_default(),
        None => d.sign_in_path.clone().unwrap_or_default(),
    };

    let name_i = text_field(&g(|s| s.name.clone(), ""), "站点名称,如 AnyRouter", false, false, window, cx);
    let domain_i = text_field(&g(|s| s.domain.clone(), ""), "https://example.com", false, false, window, cx);
    let login_i = text_field(&g(|s| s.login_path.clone(), &d.login_path), "/login", false, false, window, cx);
    let signin_i = text_field(&signin_init, "留空 = 自动签到型站点", false, false, window, cx);
    let userinfo_i = text_field(&g(|s| s.user_info_path.clone(), &d.user_info_path), "/api/user/self", false, false, window, cx);
    let tokens_i = text_field(&g(|s| s.tokens_path.clone(), &d.tokens_path), "/api/token/", false, false, window, cx);
    let logs_i = text_field(&g(|s| s.logs_path.clone(), &d.logs_path), "/api/log/self", false, false, window, cx);
    let chart_i = text_field(&g(|s| s.chart_path.clone(), &d.chart_path), "/api/data/self", false, false, window, cx);
    let apikey_i = text_field(&g(|s| s.api_user_key.clone(), &d.api_user_key), "new-api-user", false, false, window, cx);

    let title = if edit_id.is_some() { "编辑站点" } else { "新建站点" };
    let app_state = app_state.clone();

    window.open_dialog(cx, move |dialog, _window, _cx| {
        let body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(field("名称 *", Input::new(&name_i)))
            .child(field("域名 *", Input::new(&domain_i)))
            .child(
                div()
                    .pt_1()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(Glass::text_secondary())
                    .child("高级设置(new-api 默认值,通常无需修改)"),
            )
            .child(field("登录路径", Input::new(&login_i)))
            .child(field("签到路径(留空=自动签到)", Input::new(&signin_i)))
            .child(field("用户信息路径", Input::new(&userinfo_i)))
            .child(field("令牌路径", Input::new(&tokens_i)))
            .child(field("日志路径", Input::new(&logs_i)))
            .child(field("图表路径", Input::new(&chart_i)))
            .child(field("请求头用户标识名", Input::new(&apikey_i)));

        dialog
            .title(title)
            .w(px(460.0))
            .button_props(DialogButtonProps::default().ok_text("保存").cancel_text("取消"))
            .confirm()
            .child(body)
            .on_ok({
                let app_state = app_state.clone();
                let name_i = name_i.clone();
                let domain_i = domain_i.clone();
                let login_i = login_i.clone();
                let signin_i = signin_i.clone();
                let userinfo_i = userinfo_i.clone();
                let tokens_i = tokens_i.clone();
                let logs_i = logs_i.clone();
                let chart_i = chart_i.clone();
                let apikey_i = apikey_i.clone();
                move |_, window, cx| {
                    let name = name_i.read(cx).value().trim().to_string();
                    let domain = domain_i.read(cx).value().trim().to_string();
                    if name.is_empty() || domain.is_empty() {
                        window.push_notification(Notification::warning("名称与域名为必填"), cx);
                        return false;
                    }
                    if !(domain.starts_with("http://") || domain.starts_with("https://")) {
                        window.push_notification(
                            Notification::warning("域名需以 http:// 或 https:// 开头"),
                            cx,
                        );
                        return false;
                    }
                    let signin = signin_i.read(cx).value().trim().to_string();
                    let input = SiteInput {
                        name,
                        domain,
                        login_path: nonempty_or(login_i.read(cx).value(), "/login"),
                        sign_in_path: if signin.is_empty() { None } else { Some(signin) },
                        user_info_path: nonempty_or(userinfo_i.read(cx).value(), "/api/user/self"),
                        tokens_path: nonempty_or(tokens_i.read(cx).value(), "/api/token/"),
                        logs_path: nonempty_or(logs_i.read(cx).value(), "/api/log/self"),
                        chart_path: nonempty_or(chart_i.read(cx).value(), "/api/data/self"),
                        api_user_key: nonempty_or(apikey_i.read(cx).value(), "new-api-user"),
                    };
                    let db = app_state.read(cx).db.clone();
                    let res = {
                        let guard = db.lock().unwrap();
                        match edit_id {
                            Some(id) => guard.update_site(id, &input),
                            None => guard.insert_site(&input).map(|_| ()),
                        }
                    };
                    match res {
                        Ok(_) => {
                            reload_state(&app_state, cx);
                            true
                        }
                        Err(e) => {
                            window.push_notification(
                                Notification::error(format!("保存失败: {e}")),
                                cx,
                            );
                            false
                        }
                    }
                }
            })
    });
}

// ===================== 账户列表弹窗 =====================

/// 打开某站点的账户列表弹窗。
pub fn open_account_list(
    app_state: &Entity<AppState>,
    site_id: i64,
    window: &mut Window,
    cx: &mut App,
) {
    let app_state = app_state.clone();
    window.open_dialog(cx, move |dialog, _window, cx| {
        struct RowData {
            id: i64,
            name: String,
            st_txt: &'static str,
            st_col: Hsla,
            ck_txt: &'static str,
            ck_col: Hsla,
            balance: String,
        }
        let (site_name, rows_data): (String, Vec<RowData>) = {
            let state = app_state.read(cx);
            let site_name = state
                .site_by_id(site_id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "站点".to_string());
            let mut rows = Vec::new();
            for a in state.accounts_of_site(site_id) {
                let s = state.status_of(a.id);
                let (st_txt, st_col) = status_badge(&s);
                let (ck_txt, ck_col) = cookie_badge(a.cookies_json.is_some(), a.cookie_expires_at.as_deref());
                let balance = state
                    .balances
                    .get(&a.id)
                    .map(|b| format!("${:.2}", b.quota))
                    .unwrap_or_else(|| "—".to_string());
                rows.push(RowData {
                    id: a.id,
                    name: a.name.clone(),
                    st_txt,
                    st_col,
                    ck_txt,
                    ck_col,
                    balance,
                });
            }
            (site_name, rows)
        };

        let row_els: Vec<AnyElement> = rows_data
            .into_iter()
            .map(|r| {
                let as_detail = app_state.clone();
                let as_edit = app_state.clone();
                let as_del = app_state.clone();
                let rid = r.id;
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded(px(8.0))
                    .bg(Glass::card())
                    .border_1()
                    .border_color(Glass::border())
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_sm().text_color(Glass::text()).child(r.name))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(pill(r.st_txt, r.st_col))
                                    .child(pill(r.ck_txt, r.ck_col))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(Glass::text_muted())
                                            .child(r.balance),
                                    ),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!("acc-detail-{}", rid)))
                            .ghost()
                            .small()
                            .label("详情")
                            .on_click(move |_, window, cx| {
                                as_detail.update(cx, |state, cx| {
                                    state.view = AppView::AccountDetail(rid);
                                    cx.notify();
                                });
                                window.close_dialog(cx);
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("acc-edit-{}", rid)))
                            .ghost()
                            .small()
                            .label("编辑")
                            .on_click(move |_, window, cx| {
                                open_account_form(&as_edit, site_id, Some(rid), window, cx)
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("acc-del-{}", rid)))
                            .ghost()
                            .small()
                            .danger()
                            .label("删除")
                            .on_click(move |_, window, cx| {
                                confirm_delete_account(&as_del, rid, window, cx)
                            }),
                    )
                    .into_any_element()
            })
            .collect();

        let empty = row_els.is_empty();
        let as_add = app_state.clone();
        let as_checkin = app_state.clone();

        dialog.title(format!("{} · 账户", site_name)).w(px(560.0)).child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(if empty {
                    div()
                        .py_6()
                        .text_sm()
                        .text_color(Glass::text_muted())
                        .child("该站点暂无账户,点击下方「新增账户」添加。")
                        .into_any_element()
                } else {
                    div().flex().flex_col().gap_2().children(row_els).into_any_element()
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .pt_2()
                        .border_t_1()
                        .border_color(Glass::border())
                        .child(
                            Button::new("acc-add")
                                .outline()
                                .label("+ 新增账户")
                                .on_click(move |_, window, cx| {
                                    open_account_form(&as_add, site_id, None, window, cx)
                                }),
                        )
                        .child(
                            Button::new("checkin-site")
                                .primary()
                                .label("⚡ 签到本站点")
                                .on_click(move |_, _window, cx| {
                                    service::run_checkin(
                                        as_checkin.downgrade(),
                                        CheckinScope::Site(site_id),
                                        cx,
                                    )
                                }),
                        ),
                ),
        )
    });
}

// ===================== 账户表单 =====================

/// 打开账户表单。`edit_id` 为 None 表示在 `site_id` 下新建。
pub fn open_account_form(
    app_state: &Entity<AppState>,
    site_id: i64,
    edit_id: Option<i64>,
    window: &mut Window,
    cx: &mut App,
) {
    let existing = edit_id.and_then(|id| app_state.read(cx).account_by_id(id).cloned());
    let init_name = existing.as_ref().map(|a| a.name.clone()).unwrap_or_default();
    let init_api = existing.as_ref().map(|a| a.api_user.clone()).unwrap_or_default();
    let init_user = existing.as_ref().and_then(|a| a.username.clone()).unwrap_or_default();
    // 已存在的 cookies 用结构化 JSON 文本回显(只读参考),密码不回显
    let init_cookie = existing.as_ref().and_then(|a| a.cookies_json.clone()).unwrap_or_default();

    let name_i = text_field(&init_name, "账户备注名", false, false, window, cx);
    let api_i = text_field(&init_api, "new-api-user 请求头的值(数字 id)", false, false, window, cx);
    let user_i = text_field(&init_user, "用户名(可选,用于账密登录)", false, false, window, cx);
    let pass_i = text_field("", "密码(可选;编辑时留空表示不修改)", true, false, window, cx);
    let cookie_i = text_field(
        &init_cookie,
        "粘贴 Cookie:k=v; k2=v2  或 JSON 数组/对象",
        false,
        true,
        window,
        cx,
    );

    let title = if edit_id.is_some() { "编辑账户" } else { "新增账户" };
    let app_state = app_state.clone();

    window.open_dialog(cx, move |dialog, _window, _cx| {
        let body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(field("名称 *", Input::new(&name_i)))
            .child(field("API 用户标识 *", Input::new(&api_i)))
            .child(
                div()
                    .pt_1()
                    .text_xs()
                    .text_color(Glass::text_muted())
                    .child("凭据二选一:粘贴 Cookie,或填写用户名+密码(用于自动登录)"),
            )
            .child(field("用户名", Input::new(&user_i)))
            .child(field("密码", Input::new(&pass_i)))
            .child(field("Cookie", Input::new(&cookie_i).h(px(96.0))));

        dialog
            .title(title)
            .w(px(480.0))
            .button_props(DialogButtonProps::default().ok_text("保存").cancel_text("取消"))
            .confirm()
            .child(body)
            .on_ok({
                let app_state = app_state.clone();
                let name_i = name_i.clone();
                let api_i = api_i.clone();
                let user_i = user_i.clone();
                let pass_i = pass_i.clone();
                let cookie_i = cookie_i.clone();
                move |_, window, cx| {
                    let name = name_i.read(cx).value().trim().to_string();
                    let api_user = api_i.read(cx).value().trim().to_string();
                    if name.is_empty() || api_user.is_empty() {
                        window.push_notification(
                            Notification::warning("名称与 API 用户标识为必填"),
                            cx,
                        );
                        return false;
                    }
                    let username = user_i.read(cx).value().trim().to_string();
                    let password = pass_i.read(cx).value().to_string();
                    let cookie_raw = cookie_i.read(cx).value().to_string();

                    let domain = app_state
                        .read(cx)
                        .site_by_id(site_id)
                        .map(|s| s.domain.clone())
                        .unwrap_or_default();
                    let (entries, cu, cp) = if cookie_raw.trim().is_empty() {
                        (Vec::new(), None, None)
                    } else {
                        let val = serde_json::from_str::<serde_json::Value>(&cookie_raw)
                            .unwrap_or_else(|_| serde_json::Value::String(cookie_raw.clone()));
                        cookies_value_to_entries(&val, host_of(&domain))
                    };

                    let username = if !username.is_empty() { Some(username) } else { cu };
                    let password = if !password.is_empty() { Some(password) } else { cp };
                    let cookies_json = if entries.is_empty() {
                        None
                    } else {
                        serde_json::to_string(&entries).ok()
                    };

                    if cookies_json.is_none() && username.is_none() && password.is_none() {
                        window.push_notification(
                            Notification::warning("请粘贴 Cookie 或填写用户名+密码"),
                            cx,
                        );
                        return false;
                    }

                    let input = AccountInput {
                        site_id,
                        name,
                        api_user,
                        username,
                        password,
                        cookie_issued_at: if cookies_json.is_some() {
                            Some(now_iso())
                        } else {
                            None
                        },
                        cookies_json,
                        cookie_expires_at: None,
                    };
                    let db = app_state.read(cx).db.clone();
                    let res = {
                        let guard = db.lock().unwrap();
                        match edit_id {
                            Some(id) => guard.update_account(id, &input),
                            None => guard.insert_account(&input).map(|_| ()),
                        }
                    };
                    match res {
                        Ok(_) => {
                            reload_state(&app_state, cx);
                            true
                        }
                        Err(e) => {
                            window.push_notification(
                                Notification::error(format!("保存失败: {e}")),
                                cx,
                            );
                            false
                        }
                    }
                }
            })
    });
}

// ===================== 删除确认 =====================

/// 删除站点确认。
pub fn confirm_delete_site(
    app_state: &Entity<AppState>,
    site_id: i64,
    window: &mut Window,
    cx: &mut App,
) {
    let count = app_state.read(cx).accounts_of_site(site_id).len();
    let app_state = app_state.clone();
    window.open_dialog(cx, move |dialog, _window, _cx| {
        dialog
            .title("删除站点")
            .button_props(DialogButtonProps::default().ok_text("删除").cancel_text("取消"))
            .confirm()
            .child(
                div().py_2().text_sm().text_color(Glass::text_secondary()).child(format!(
                    "将同时删除其下 {} 个账户及全部缓存数据,此操作不可恢复。",
                    count
                )),
            )
            .on_ok({
                let app_state = app_state.clone();
                move |_, window, cx| {
                    let db = app_state.read(cx).db.clone();
                    let res = db.lock().unwrap().delete_site(site_id);
                    match res {
                        Ok(_) => {
                            reload_state(&app_state, cx);
                            true
                        }
                        Err(e) => {
                            window.push_notification(
                                Notification::error(format!("删除失败: {e}")),
                                cx,
                            );
                            false
                        }
                    }
                }
            })
    });
}

/// 删除账户确认。
pub fn confirm_delete_account(
    app_state: &Entity<AppState>,
    account_id: i64,
    window: &mut Window,
    cx: &mut App,
) {
    let name = app_state
        .read(cx)
        .account_by_id(account_id)
        .map(|a| a.name.clone())
        .unwrap_or_default();
    let app_state = app_state.clone();
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let name = name.clone();
        dialog
            .title("删除账户")
            .button_props(DialogButtonProps::default().ok_text("删除").cancel_text("取消"))
            .confirm()
            .child(
                div()
                    .py_2()
                    .text_sm()
                    .text_color(Glass::text_secondary())
                    .child(format!("将删除账户「{}」及其缓存数据,此操作不可恢复。", name)),
            )
            .on_ok({
                let app_state = app_state.clone();
                move |_, window, cx| {
                    let db = app_state.read(cx).db.clone();
                    let res = db.lock().unwrap().delete_account(account_id);
                    match res {
                        Ok(_) => {
                            reload_state(&app_state, cx);
                            true
                        }
                        Err(e) => {
                            window.push_notification(
                                Notification::error(format!("删除失败: {e}")),
                                cx,
                            );
                            false
                        }
                    }
                }
            })
    });
}

/// 账户行的 Cookie 状态短标(依据过期时间分类)。
fn cookie_badge(has_cookie: bool, expires_at: Option<&str>) -> (&'static str, Hsla) {
    if !has_cookie {
        return ("待登录", Glass::warning());
    }
    match crate::cookie::classify(expires_at) {
        CookieStatus::Valid => ("有效", Glass::success()),
        CookieStatus::ExpiringSoon => ("即将到期", Glass::warning()),
        CookieStatus::Expired => ("已过期", Glass::danger()),
        CookieStatus::Unknown => ("会话期", Glass::text_muted()),
    }
}

/// 签到状态短标
fn status_badge(status: &CheckInStatus) -> (&'static str, Hsla) {
    match status {
        CheckInStatus::Idle => ("空闲", Glass::text_muted()),
        CheckInStatus::Running => ("签到中", Glass::warning()),
        CheckInStatus::Success => ("成功", Glass::success()),
        CheckInStatus::Failed(_) => ("失败", Glass::danger()),
    }
}

/// 小药丸标签(状态 / Cookie 短标)
fn pill(text: &str, color: Hsla) -> impl IntoElement {
    div()
        .px_2()
        .py_0p5()
        .rounded(px(6.0))
        .bg(color.opacity(0.12))
        .border_1()
        .border_color(color.opacity(0.35))
        .text_xs()
        .text_color(color)
        .child(text.to_string())
}
