mod app_state;
mod theme;

use app_state::AppState;
use eframe::egui;

/// Root application struct
struct AnyRouterApp {
    state: AppState,
}

impl AnyRouterApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            state: AppState::default(),
        }
    }
}

impl eframe::App for AnyRouterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::apply_theme(ctx);

        // ─── 顶部标题栏 ───────────────────────────────────────────
        egui::TopBottomPanel::top("title_bar")
            .exact_height(48.0)
            .frame(egui::Frame::new().fill(theme::BAR_BG).inner_margin(egui::Margin::symmetric(16, 8)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        egui::RichText::new("\u{1f170} AnyRouter \u{00b7} Auto Check-in")
                            .size(18.0)
                            .color(theme::TEXT_PRIMARY),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = egui::Button::new(
                            egui::RichText::new("\u{26a1} \u{4e00}\u{952e}\u{7b7e}\u{5230}\u{5168}\u{90e8}")
                                .color(theme::ACCENT_BLUE),
                        )
                        .fill(theme::BTN_PRIMARY_BG)
                        .stroke(egui::Stroke::new(1.0, theme::BTN_PRIMARY_BORDER));

                        if ui.add(btn).clicked() {
                            // TODO: trigger sign-in-all
                        }
                    });
                });
            });

        // ─── 底部状态栏 ───────────────────────────────────────────
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(32.0)
            .frame(egui::Frame::new().fill(theme::BAR_BG).inner_margin(egui::Margin::symmetric(16, 4)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let status_text = if self.state.running {
                        egui::RichText::new("\u{25cf} \u{8fd0}\u{884c}\u{4e2d}")
                            .color(theme::WARNING_YELLOW)
                    } else {
                        egui::RichText::new("\u{25cf} \u{5c31}\u{7eea}")
                            .color(theme::SUCCESS_GREEN)
                    };
                    ui.label(status_text.size(13.0));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let log_btn = egui::Button::new(
                            egui::RichText::new("\u{25a4} \u{65e5}\u{5fd7}")
                                .size(13.0)
                                .color(theme::TEXT_SECONDARY),
                        )
                        .fill(egui::Color32::TRANSPARENT);

                        if ui.add(log_btn).clicked() {
                            self.state.log_drawer_open = !self.state.log_drawer_open;
                        }
                    });
                });
            });

        // ─── 中间内容区 ──────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::WINDOW_BG).inner_margin(egui::Margin::same(24)))
            .show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new("\u{4e3b}\u{9875}\u{89c6}\u{56fe} \u{2014} \u{5f85}\u{5b9e}\u{73b0}")
                            .size(20.0)
                            .color(theme::TEXT_MUTED),
                    );
                });
            });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 750.0])
            .with_title("AnyRouter \u{00b7} Auto Check-in")
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "AnyRouter",
        options,
        Box::new(|cc| Ok(Box::new(AnyRouterApp::new(cc)))),
    )
}
