use egui::{self, Color32, RichText, Stroke, Ui};

use crate::app_state::{AppState, ViewKind};
use crate::theme;

/// 渲染账户详情页
pub fn render_account_detail(ui: &mut Ui, state: &mut AppState, account_id: i64) {
    // 面包屑导航
    render_breadcrumb(ui, state, account_id);
    ui.add_space(12.0);

    // Tab 栏 + 操作按钮
    render_tab_bar(ui, state);
    ui.add_space(16.0);

    // Tab 内容
    render_tab_content(ui, state);
}

/// 面包屑导航
fn render_breadcrumb(ui: &mut Ui, state: &mut AppState, account_id: i64) {
    ui.horizontal(|ui| {
        let back_btn = egui::Button::new(
            RichText::new("\u{25c0} 返回主页")
                .size(14.0)
                .color(theme::ACCENT_BLUE),
        )
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE);

        if ui.add(back_btn).clicked() {
            state.current_view = ViewKind::Home;
            state.active_tab = 0;
        }

        ui.label(
            RichText::new("/")
                .size(14.0)
                .color(theme::TEXT_WEAKEST),
        );

        // 模拟数据：根据 account_id 显示信息
        let (acc_name, site_name) = match account_id {
            1 => ("alice@example.com", "AnyRouter"),
            2 => ("bob@example.com", "AnyRouter"),
            3 => ("charlie@example.com", "AnyRouter"),
            _ => ("未知账户", "未知站点"),
        };

        ui.label(
            RichText::new(format!("{} @ {}", acc_name, site_name))
                .size(14.0)
                .color(theme::TEXT_PRIMARY),
        );
    });
}

/// Tab 栏
fn render_tab_bar(ui: &mut Ui, state: &mut AppState) {
    let tabs = ["概览", "API 密钥", "使用日志", "消耗图表"];

    ui.horizontal(|ui| {
        for (i, tab_name) in tabs.iter().enumerate() {
            let is_active = state.active_tab == i;
            let text_color = if is_active {
                theme::ACCENT_BLUE
            } else {
                theme::TEXT_MUTED
            };

            let btn = egui::Button::new(RichText::new(*tab_name).size(14.0).color(text_color))
                .fill(if is_active {
                    theme::BTN_PRIMARY_BG
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(if is_active {
                    Stroke::new(1.0, theme::BTN_PRIMARY_BORDER)
                } else {
                    Stroke::NONE
                });

            if ui.add(btn).clicked() {
                state.active_tab = i;
            }

            ui.add_space(4.0);
        }

        // 右侧操作按钮
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let refresh_btn = egui::Button::new(
                RichText::new("\u{21bb} 刷新数据")
                    .size(12.0)
                    .color(theme::TEXT_SECONDARY),
            )
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, theme::BORDER_NORMAL));
            if ui.add(refresh_btn).clicked() {
                // TODO: refresh
            }

            ui.add_space(8.0);

            let checkin_btn = egui::Button::new(
                RichText::new("\u{26a1} 签到")
                    .size(12.0)
                    .color(theme::ACCENT_BLUE),
            )
            .fill(theme::BTN_PRIMARY_BG)
            .stroke(Stroke::new(1.0, theme::BTN_PRIMARY_BORDER));
            if ui.add(checkin_btn).clicked() {
                // TODO: sign in
            }
        });
    });

    // Tab 下划线
    ui.add_space(2.0);
    ui.separator();
}

/// Tab 内容渲染
fn render_tab_content(ui: &mut Ui, state: &AppState) {
    match state.active_tab {
        0 => render_tab_overview(ui),
        1 => render_tab_api_keys(ui),
        2 => render_tab_logs(ui),
        3 => render_tab_chart(ui),
        _ => {}
    }
}

/// 概览 Tab
fn render_tab_overview(ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        // 余额卡片
        info_card(ui, "余额", "$12.50", theme::SUCCESS_GREEN);
        ui.add_space(12.0);
        // Cookie 状态卡片
        info_card(ui, "Cookie 状态", "有效", theme::SUCCESS_GREEN);
        ui.add_space(12.0);
        // 用户信息卡片
        info_card(ui, "用户信息", "alice@example.com", theme::TEXT_PRIMARY);
    });
}

/// 信息卡片
fn info_card(ui: &mut Ui, label: &str, value: &str, value_color: Color32) {
    egui::Frame::new()
        .fill(theme::CARD_BG)
        .stroke(Stroke::new(1.0, theme::BORDER_NORMAL))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_min_width(160.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(label).size(12.0).color(theme::TEXT_MUTED));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(value)
                        .size(18.0)
                        .strong()
                        .color(value_color),
                );
            });
        });
}

/// API 密钥 Tab
fn render_tab_api_keys(ui: &mut Ui) {
    ui.add_space(20.0);
    ui.label(
        RichText::new("API 密钥列表 \u{2014} 刷新后可查看")
            .size(14.0)
            .color(theme::TEXT_MUTED),
    );
}

/// 使用日志 Tab
fn render_tab_logs(ui: &mut Ui) {
    ui.add_space(20.0);
    ui.label(
        RichText::new("使用日志 \u{2014} 刷新后可查看")
            .size(14.0)
            .color(theme::TEXT_MUTED),
    );
}

/// 消耗图表 Tab
fn render_tab_chart(ui: &mut Ui) {
    ui.add_space(20.0);
    ui.label(
        RichText::new("消耗图表 \u{2014} 刷新后可查看")
            .size(14.0)
            .color(theme::TEXT_MUTED),
    );
}
