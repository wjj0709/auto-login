use egui::{self, Color32, RichText, Stroke, Ui};

use crate::app_state::{AppState, DeleteTarget, ModalKind};
use crate::theme;

/// 统计条背景色
const STATS_BAR_BG: Color32 = Color32::from_rgb(0x0e, 0x11, 0x17);

/// 渲染主页视图（统计条 + 站点卡片网格）
pub fn render_home(ui: &mut Ui, state: &mut AppState) {
    render_stats_bar(ui, state);
    ui.add_space(16.0);
    render_site_cards(ui, state);
}

/// 统计条 — 一行四个指标
fn render_stats_bar(ui: &mut Ui, state: &AppState) {
    let site_count = state.sites.len();
    let account_count: usize = state.sites.iter().map(|s| s.account_count).sum();
    let total_balance: f64 = state.sites.iter().map(|s| s.total_balance).sum();
    let checkin_done: usize = state.sites.iter().map(|s| s.checkin_today).sum();
    let checkin_total = account_count;

    egui::Frame::new()
        .fill(STATS_BAR_BG)
        .stroke(Stroke::new(1.0, theme::BORDER_NORMAL))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(20, 12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                stat_item(ui, "站点数", &site_count.to_string());
                ui.add_space(32.0);
                stat_item(ui, "账户数", &account_count.to_string());
                ui.add_space(32.0);
                stat_item(ui, "总余额", &format!("${:.2}", total_balance));
                ui.add_space(32.0);
                stat_item(
                    ui,
                    "今日签到",
                    &format!("{}/{}", checkin_done, checkin_total),
                );
            });
        });
}

/// 统计条中的单个指标
fn stat_item(ui: &mut Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).size(12.0).color(theme::TEXT_MUTED));
        ui.label(
            RichText::new(value)
                .size(18.0)
                .strong()
                .color(theme::TEXT_PRIMARY),
        );
    });
}

/// 站点卡片网格
fn render_site_cards(ui: &mut Ui, state: &mut AppState) {
    // 收集需要的数据避免借用冲突
    let sites_data: Vec<_> = state
        .sites
        .iter()
        .map(|s| {
            (
                s.site.id,
                s.site.name.clone(),
                s.site.domain.clone(),
                s.account_count,
                s.total_balance,
                s.expired_count,
                s.checkin_today,
            )
        })
        .collect();

    ui.horizontal_wrapped(|ui| {
        // 已有站点卡片
        for (site_id, name, domain, account_count, total_balance, expired_count, _checkin_today) in
            &sites_data
        {
            render_site_card(
                ui,
                state,
                *site_id,
                name,
                domain,
                *account_count,
                *total_balance,
                *expired_count,
            );
            ui.add_space(8.0);
        }

        // 新建站点占位卡片
        render_add_site_card(ui, state);
    });
}

/// 单个站点卡片
fn render_site_card(
    ui: &mut Ui,
    state: &mut AppState,
    site_id: i64,
    name: &str,
    domain: &str,
    account_count: usize,
    total_balance: f64,
    expired_count: usize,
) {
    let card_width = 280.0;

    egui::Frame::new()
        .fill(theme::CARD_BG)
        .stroke(Stroke::new(1.0, theme::BORDER_NORMAL))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(card_width);

            // 站点名
            ui.label(
                RichText::new(name)
                    .size(16.0)
                    .strong()
                    .color(theme::TEXT_PRIMARY),
            );

            // 域名
            ui.label(RichText::new(domain).size(12.0).color(theme::TEXT_MUTED));

            ui.add_space(8.0);

            // 账户数 + 过期警告
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("账户: {}", account_count))
                        .size(13.0)
                        .color(theme::TEXT_SECONDARY),
                );
                if expired_count > 0 {
                    ui.label(
                        RichText::new(format!(" {} 个已过期", expired_count))
                            .size(13.0)
                            .color(theme::WARNING_YELLOW),
                    );
                }
            });

            // 余额
            ui.label(
                RichText::new(format!("余额: ${:.2}", total_balance))
                    .size(13.0)
                    .color(theme::TEXT_SECONDARY),
            );

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            // 操作按钮行
            ui.horizontal(|ui| {
                // 查看账户 — 蓝色按钮
                let view_btn = egui::Button::new(
                    RichText::new("查看账户")
                        .size(12.0)
                        .color(theme::ACCENT_BLUE),
                )
                .fill(theme::BTN_PRIMARY_BG)
                .stroke(Stroke::new(1.0, theme::BTN_PRIMARY_BORDER));

                if ui.add(view_btn).clicked() {
                    state.active_modal = Some(ModalKind::AccountList(site_id));
                }

                ui.add_space(8.0);

                // 编辑 — 文字链接
                let edit_btn = egui::Button::new(
                    RichText::new("编辑")
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE);

                if ui.add(edit_btn).clicked() {
                    state.active_modal = Some(ModalKind::SiteForm(Some(site_id)));
                }

                // 删除 — 文字链接
                let delete_btn = egui::Button::new(
                    RichText::new("删除")
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE);

                if ui.add(delete_btn).clicked() {
                    state.active_modal = Some(ModalKind::ConfirmDelete(DeleteTarget::Site {
                        id: site_id,
                        name: name.to_string(),
                        account_count,
                    }));
                }
            });
        });
}

/// 新建站点占位卡片（虚线边框）
fn render_add_site_card(ui: &mut Ui, state: &mut AppState) {
    let card_width = 280.0;

    egui::Frame::new()
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.5, theme::BORDER_NORMAL))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(card_width);
            ui.set_min_height(140.0);

            ui.centered_and_justified(|ui| {
                let add_btn = egui::Button::new(
                    RichText::new("+ 新建站点")
                        .size(16.0)
                        .color(theme::TEXT_MUTED),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE);

                if ui.add(add_btn).clicked() {
                    state.active_modal = Some(ModalKind::SiteForm(None));
                }
            });
        });
}
