use egui::{self, Color32, RichText, Stroke, Ui};

use crate::app_state::{AppState, LogLevel};
use crate::theme;

/// 渲染日志抽屉面板（底部状态栏上方）
pub fn render_log_drawer(ui: &mut Ui, state: &mut AppState) {
    if !state.log_drawer_open {
        return;
    }

    egui::Frame::new()
        .fill(theme::BAR_BG)
        .stroke(Stroke::new(1.0, theme::BORDER_NORMAL))
        .inner_margin(egui::Margin::symmetric(16, 8))
        .show(ui, |ui| {
            ui.set_min_height(200.0);
            ui.set_max_height(200.0);

            // 顶部行：标题 + 操作按钮
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("运行日志")
                        .size(14.0)
                        .strong()
                        .color(theme::TEXT_PRIMARY),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // 关闭按钮
                    let close_btn = egui::Button::new(
                        RichText::new("\u{2715}").size(14.0).color(theme::TEXT_MUTED),
                    )
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::NONE);
                    if ui.add(close_btn).clicked() {
                        state.log_drawer_open = false;
                    }

                    // 清空按钮
                    let clear_btn = egui::Button::new(
                        RichText::new("清空").size(12.0).color(theme::TEXT_MUTED),
                    )
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, theme::BORDER_NORMAL));
                    if ui.add(clear_btn).clicked() {
                        state.log_entries.clear();
                    }
                });
            });

            ui.separator();

            // 日志内容滚动区域
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if state.log_entries.is_empty() {
                        ui.add_space(20.0);
                        ui.label(
                            RichText::new("暂无日志")
                                .size(13.0)
                                .color(theme::TEXT_WEAKEST),
                        );
                    } else {
                        for entry in &state.log_entries {
                            let level_color = match entry.level {
                                LogLevel::Info => theme::TEXT_SECONDARY,
                                LogLevel::Success => theme::SUCCESS_GREEN,
                                LogLevel::Warning => theme::WARNING_YELLOW,
                                LogLevel::Error => theme::ERROR_RED,
                            };

                            let level_tag = match entry.level {
                                LogLevel::Info => "[INFO]",
                                LogLevel::Success => "[OK]",
                                LogLevel::Warning => "[WARN]",
                                LogLevel::Error => "[ERR]",
                            };

                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&entry.timestamp)
                                        .size(11.0)
                                        .color(theme::TEXT_WEAKEST),
                                );
                                ui.label(
                                    RichText::new(level_tag)
                                        .size(11.0)
                                        .strong()
                                        .color(level_color),
                                );
                                ui.label(
                                    RichText::new(&entry.message)
                                        .size(12.0)
                                        .color(theme::TEXT_PRIMARY),
                                );
                            });
                        }
                    }
                });
        });
}
