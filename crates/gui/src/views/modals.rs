use egui::{self, Align2, Color32, RichText, Stroke, Vec2};

use crate::app_state::{AppState, DeleteTarget, LogEntry, LogLevel, ModalKind, ViewKind};
use crate::theme;

/// 渲染当前活动弹窗（如果有）
pub fn render_modals(ctx: &egui::Context, state: &mut AppState) {
    let modal = match &state.active_modal {
        Some(m) => m.clone(),
        None => return,
    };

    // 半透明遮罩层
    egui::Area::new(egui::Id::new("modal_overlay"))
        .fixed_pos(egui::pos2(0.0, 0.0))
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            let screen = ctx.screen_rect();
            let (resp, painter) =
                ui.allocate_painter(screen.size(), egui::Sense::click());
            painter.rect_filled(screen, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 180));
            // 点击遮罩关闭弹窗
            if resp.clicked() {
                state.active_modal = None;
            }
        });

    match modal {
        ModalKind::AccountList(site_id) => render_account_list_modal(ctx, state, site_id),
        ModalKind::SiteForm(site_id) => render_site_form_modal(ctx, state, site_id),
        ModalKind::AccountForm {
            site_id,
            account_id,
        } => render_account_form_modal(ctx, state, site_id, account_id),
        ModalKind::ConfirmDelete(target) => render_confirm_delete_modal(ctx, state, target),
    }
}

/// 账户列表弹窗
fn render_account_list_modal(ctx: &egui::Context, state: &mut AppState, site_id: i64) {
    let site_name = state
        .sites
        .iter()
        .find(|s| s.site.id == site_id)
        .map(|s| s.site.name.clone())
        .unwrap_or_else(|| "未知站点".into());

    // 模拟账户数据
    let mock_accounts = vec![
        (1i64, "alice@example.com", true),
        (2i64, "bob@example.com", true),
        (3i64, "charlie@example.com", false),
    ];

    egui::Window::new(format!("{} \u{00b7} 账户管理", site_name))
        .id(egui::Id::new("modal_account_list"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .resizable(false)
        .collapsible(false)
        .min_width(480.0)
        .frame(
            egui::Frame::new()
                .fill(theme::CARD_BG)
                .stroke(Stroke::new(1.0, theme::BORDER_ACCENT))
                .corner_radius(10.0)
                .inner_margin(egui::Margin::same(20)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(460.0);

            for (acc_id, acc_name, cookie_valid) in &mock_accounts {
                ui.horizontal(|ui| {
                    // 账户名
                    ui.label(
                        RichText::new(*acc_name)
                            .size(14.0)
                            .color(theme::TEXT_PRIMARY),
                    );

                    ui.add_space(8.0);

                    // Cookie 状态
                    let (status_text, status_color) = if *cookie_valid {
                        ("有效", theme::SUCCESS_GREEN)
                    } else {
                        ("已过期", theme::ERROR_RED)
                    };
                    ui.label(RichText::new(status_text).size(12.0).color(status_color));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // 删除
                        let del_btn = egui::Button::new(
                            RichText::new("删除").size(12.0).color(theme::ERROR_RED),
                        )
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE);
                        if ui.add(del_btn).clicked() {
                            state.active_modal =
                                Some(ModalKind::ConfirmDelete(DeleteTarget::Account {
                                    id: *acc_id,
                                    name: acc_name.to_string(),
                                }));
                        }

                        // 编辑
                        let edit_btn = egui::Button::new(
                            RichText::new("编辑").size(12.0).color(theme::TEXT_MUTED),
                        )
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE);
                        if ui.add(edit_btn).clicked() {
                            state.active_modal = Some(ModalKind::AccountForm {
                                site_id,
                                account_id: Some(*acc_id),
                            });
                        }

                        // 详情
                        let detail_btn = egui::Button::new(
                            RichText::new("详情")
                                .size(12.0)
                                .color(theme::ACCENT_BLUE),
                        )
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE);
                        if ui.add(detail_btn).clicked() {
                            state.current_view = ViewKind::AccountDetail(*acc_id);
                            state.active_modal = None;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
            }

            ui.add_space(12.0);

            // 底部按钮行
            ui.horizontal(|ui| {
                let add_btn = egui::Button::new(
                    RichText::new("+ 新增账户")
                        .size(13.0)
                        .color(theme::ACCENT_BLUE),
                )
                .fill(theme::BTN_PRIMARY_BG)
                .stroke(Stroke::new(1.0, theme::BTN_PRIMARY_BORDER));
                if ui.add(add_btn).clicked() {
                    state.active_modal = Some(ModalKind::AccountForm {
                        site_id,
                        account_id: None,
                    });
                }

                ui.add_space(12.0);

                let checkin_btn = egui::Button::new(
                    RichText::new("\u{26a1} 签到本站点")
                        .size(13.0)
                        .color(theme::WARNING_YELLOW),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::new(1.0, theme::BORDER_ACCENT));
                if ui.add(checkin_btn).clicked() {
                    state.log_entries.push(LogEntry {
                        timestamp: "刚刚".into(),
                        level: LogLevel::Info,
                        message: format!("触发站点 {} 签到...", site_name),
                    });
                }
            });
        });
}

/// 站点表单弹窗
fn render_site_form_modal(ctx: &egui::Context, state: &mut AppState, site_id: Option<i64>) {
    let title = if site_id.is_some() {
        "编辑站点"
    } else {
        "新建站点"
    };

    egui::Window::new(title)
        .id(egui::Id::new("modal_site_form"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .resizable(false)
        .collapsible(false)
        .min_width(420.0)
        .frame(
            egui::Frame::new()
                .fill(theme::CARD_BG)
                .stroke(Stroke::new(1.0, theme::BORDER_ACCENT))
                .corner_radius(10.0)
                .inner_margin(egui::Margin::same(20)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(400.0);

            // 站点名称
            ui.label(RichText::new("站点名称 *").size(13.0).color(theme::TEXT_SECONDARY));
            let mut site_name_buf = state.form_site_name.clone();
            let name_resp = ui.add(
                egui::TextEdit::singleline(&mut site_name_buf)
                    .desired_width(380.0)
                    .hint_text("例如: AnyRouter"),
            );
            if name_resp.changed() {
                state.form_site_name = site_name_buf;
            }
            ui.add_space(8.0);

            // 域名
            ui.label(RichText::new("域名 *").size(13.0).color(theme::TEXT_SECONDARY));
            let mut domain_buf = state.form_site_domain.clone();
            let domain_resp = ui.add(
                egui::TextEdit::singleline(&mut domain_buf)
                    .desired_width(380.0)
                    .hint_text("例如: https://anyrouter.top"),
            );
            if domain_resp.changed() {
                state.form_site_domain = domain_buf;
            }
            ui.add_space(12.0);

            // 高级设置折叠区
            egui::CollapsingHeader::new(
                RichText::new("高级设置").size(13.0).color(theme::TEXT_MUTED),
            )
            .show(ui, |ui| {
                ui.label(RichText::new("登录路径、签到路径、API 用户标识等...").size(12.0).color(theme::TEXT_WEAKEST));
                ui.label(RichText::new("（保存时使用默认值）").size(12.0).color(theme::TEXT_WEAKEST));
            });

            ui.add_space(16.0);

            // 底部按钮
            ui.horizontal(|ui| {
                let cancel_btn = egui::Button::new(
                    RichText::new("取消").size(13.0).color(theme::TEXT_MUTED),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::new(1.0, theme::BORDER_NORMAL));
                if ui.add(cancel_btn).clicked() {
                    state.active_modal = None;
                }

                ui.add_space(12.0);

                let save_btn = egui::Button::new(
                    RichText::new("保存").size(13.0).color(theme::ACCENT_BLUE),
                )
                .fill(theme::BTN_PRIMARY_BG)
                .stroke(Stroke::new(1.0, theme::BTN_PRIMARY_BORDER));
                if ui.add(save_btn).clicked() {
                    // TODO: persist
                    state.active_modal = None;
                }
            });
        });
}

/// 账户表单弹窗
fn render_account_form_modal(
    ctx: &egui::Context,
    state: &mut AppState,
    _site_id: i64,
    account_id: Option<i64>,
) {
    let title = if account_id.is_some() {
        "编辑账户"
    } else {
        "新增账户"
    };

    egui::Window::new(title)
        .id(egui::Id::new("modal_account_form"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .resizable(false)
        .collapsible(false)
        .min_width(420.0)
        .frame(
            egui::Frame::new()
                .fill(theme::CARD_BG)
                .stroke(Stroke::new(1.0, theme::BORDER_ACCENT))
                .corner_radius(10.0)
                .inner_margin(egui::Margin::same(20)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(400.0);

            // 账户名称
            ui.label(RichText::new("账户名称 *").size(13.0).color(theme::TEXT_SECONDARY));
            let mut name_buf = state.form_account_name.clone();
            let name_resp = ui.add(
                egui::TextEdit::singleline(&mut name_buf)
                    .desired_width(380.0)
                    .hint_text("例如: alice@example.com"),
            );
            if name_resp.changed() {
                state.form_account_name = name_buf;
            }
            ui.add_space(8.0);

            // API 用户标识
            ui.label(RichText::new("API 用户标识 *").size(13.0).color(theme::TEXT_SECONDARY));
            let mut user_buf = state.form_account_api_user.clone();
            let user_resp = ui.add(
                egui::TextEdit::singleline(&mut user_buf)
                    .desired_width(380.0)
                    .hint_text("用于 API 请求的用户标识"),
            );
            if user_resp.changed() {
                state.form_account_api_user = user_buf;
            }
            ui.add_space(12.0);

            // 凭据区域
            ui.label(RichText::new("凭据").size(13.0).color(theme::TEXT_SECONDARY));
            ui.add_space(4.0);

            ui.label(RichText::new("Cookie").size(12.0).color(theme::TEXT_MUTED));
            let mut cookie_buf = state.form_account_cookie.clone();
            let cookie_resp = ui.add(
                egui::TextEdit::multiline(&mut cookie_buf)
                    .desired_width(380.0)
                    .desired_rows(3)
                    .hint_text("粘贴 Cookie 字符串..."),
            );
            if cookie_resp.changed() {
                state.form_account_cookie = cookie_buf;
            }
            ui.add_space(8.0);

            ui.label(RichText::new("或 用户名 + 密码").size(12.0).color(theme::TEXT_MUTED));
            let mut uname_buf = state.form_account_username.clone();
            let uname_resp = ui.add(
                egui::TextEdit::singleline(&mut uname_buf)
                    .desired_width(380.0)
                    .hint_text("用户名"),
            );
            if uname_resp.changed() {
                state.form_account_username = uname_buf;
            }
            let mut pwd_buf = state.form_account_password.clone();
            let pwd_resp = ui.add(
                egui::TextEdit::singleline(&mut pwd_buf)
                    .desired_width(380.0)
                    .hint_text("密码")
                    .password(true),
            );
            if pwd_resp.changed() {
                state.form_account_password = pwd_buf;
            }

            ui.add_space(16.0);

            // 底部按钮
            ui.horizontal(|ui| {
                let cancel_btn = egui::Button::new(
                    RichText::new("取消").size(13.0).color(theme::TEXT_MUTED),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::new(1.0, theme::BORDER_NORMAL));
                if ui.add(cancel_btn).clicked() {
                    state.active_modal = None;
                }

                ui.add_space(12.0);

                let save_btn = egui::Button::new(
                    RichText::new("保存").size(13.0).color(theme::ACCENT_BLUE),
                )
                .fill(theme::BTN_PRIMARY_BG)
                .stroke(Stroke::new(1.0, theme::BTN_PRIMARY_BORDER));
                if ui.add(save_btn).clicked() {
                    // TODO: persist
                    state.active_modal = None;
                }
            });
        });
}

/// 确认删除弹窗
fn render_confirm_delete_modal(
    ctx: &egui::Context,
    state: &mut AppState,
    target: DeleteTarget,
) {
    let description = match &target {
        DeleteTarget::Site {
            name,
            account_count,
            ..
        } => format!(
            "确定删除站点 \"{}\"？将同时删除其下 {} 个账户及所有关联数据。此操作不可恢复。",
            name, account_count
        ),
        DeleteTarget::Account { name, .. } => {
            format!("确定删除账户 \"{}\"？此操作不可恢复。", name)
        }
    };

    egui::Window::new("\u{26a0} 删除确认")
        .id(egui::Id::new("modal_confirm_delete"))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .resizable(false)
        .collapsible(false)
        .min_width(380.0)
        .frame(
            egui::Frame::new()
                .fill(theme::CARD_BG)
                .stroke(Stroke::new(1.0, theme::BORDER_ACCENT))
                .corner_radius(10.0)
                .inner_margin(egui::Margin::same(20)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(360.0);

            ui.label(
                RichText::new(&description)
                    .size(14.0)
                    .color(theme::TEXT_PRIMARY),
            );

            ui.add_space(20.0);

            ui.horizontal(|ui| {
                let cancel_btn = egui::Button::new(
                    RichText::new("取消").size(13.0).color(theme::TEXT_MUTED),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::new(1.0, theme::BORDER_NORMAL));
                if ui.add(cancel_btn).clicked() {
                    state.active_modal = None;
                }

                ui.add_space(12.0);

                let delete_btn = egui::Button::new(
                    RichText::new("确认删除").size(13.0).color(theme::ERROR_RED),
                )
                .fill(theme::BTN_DANGER_BG)
                .stroke(Stroke::new(1.0, theme::BTN_DANGER_BORDER));
                if ui.add(delete_btn).clicked() {
                    // TODO: actually delete
                    state.active_modal = None;
                }
            });
        });
}
