mod app_state;
mod theme;
mod views;

use std::sync::mpsc;
use std::sync::atomic::Ordering;

use app_state::{AppState, LogEntry, LogLevel, ViewKind};
use anyrouter_core::env_import;
use anyrouter_core::storage::Storage;
use eframe::egui;

/// Root application struct
struct AnyRouterApp {
    state: AppState,
}

impl AnyRouterApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut state = AppState::default();

        // 1. 加载 .env
        dotenvy::dotenv().ok();

        // 2. 打开 Storage
        let db_path = Storage::default_path();
        match Storage::open(&db_path) {
            Ok(storage) => {
                state.log_entries.push(LogEntry {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    level: LogLevel::Success,
                    message: format!("数据库连接成功: {:?}", db_path),
                });

                // 3. 从 .env 导入（首次）
                if let Err(e) = env_import::import_env_if_needed(&storage) {
                    state.log_entries.push(LogEntry {
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        level: LogLevel::Warning,
                        message: format!("环境变量导入失败: {}", e),
                    });
                }

                state.storage = Some(storage);

                // 4. 加载站点数据
                state.reload_sites();

                state.log_entries.push(LogEntry {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    level: LogLevel::Info,
                    message: "应用启动完成".into(),
                });
            }
            Err(e) => {
                state.log_entries.push(LogEntry {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    level: LogLevel::Error,
                    message: format!("数据库打开失败: {}", e),
                });
            }
        }

        Self { state }
    }
}

impl eframe::App for AnyRouterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::apply_theme(ctx);

        // 轮询后台线程日志
        self.state.poll_bg_logs();

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
                        let is_running = self.state.running;
                        let btn_text = if is_running {
                            "\u{23f3} 签到中..."
                        } else {
                            "\u{26a1} 一键签到全部"
                        };

                        let btn = egui::Button::new(
                            egui::RichText::new(btn_text)
                                .color(theme::ACCENT_BLUE),
                        )
                        .fill(theme::BTN_PRIMARY_BG)
                        .stroke(egui::Stroke::new(1.0, theme::BTN_PRIMARY_BORDER));

                        if ui.add_enabled(!is_running, btn).clicked() {
                            self.trigger_checkin_all();
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
                        egui::RichText::new("\u{25cf} 运行中")
                            .color(theme::WARNING_YELLOW)
                    } else {
                        egui::RichText::new("\u{25cf} 就绪")
                            .color(theme::SUCCESS_GREEN)
                    };
                    ui.label(status_text.size(13.0));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let log_btn = egui::Button::new(
                            egui::RichText::new("\u{25a4} 日志")
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

        // ─── 日志抽屉（底部栏上方）──────────────────────────────
        if self.state.log_drawer_open {
            egui::TopBottomPanel::bottom("log_drawer_panel")
                .exact_height(200.0)
                .show(ctx, |ui| {
                    views::log_drawer::render_log_drawer(ui, &mut self.state);
                });
        }

        // ─── 中间内容区 ──────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::WINDOW_BG).inner_margin(egui::Margin::same(24)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    match self.state.current_view.clone() {
                        ViewKind::Home => {
                            views::home::render_home(ui, &mut self.state);
                        }
                        ViewKind::AccountDetail(id) => {
                            views::detail::render_account_detail(ui, &mut self.state, id);
                        }
                    }
                });
            });

        // ─── 弹窗层（在所有面板之后渲染）─────────────────────────
        views::modals::render_modals(ctx, &mut self.state);

        // 如果后台运行中，持续请求重绘以轮询日志
        if self.state.running {
            ctx.request_repaint();
        }
    }
}

impl AnyRouterApp {
    /// 触发全部账户签到
    fn trigger_checkin_all(&mut self) {
        // 收集所有站点和账户数据
        let storage = match &self.state.storage {
            Some(s) => s,
            None => {
                self.state.log_entries.push(LogEntry {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    level: LogLevel::Error,
                    message: "数据库未连接，无法签到".into(),
                });
                return;
            }
        };

        // 收集所有站点和对应的账户
        let mut site_accounts = Vec::new();
        let sites = match storage.list_sites() {
            Ok(s) => s,
            Err(e) => {
                self.state.log_entries.push(LogEntry {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    level: LogLevel::Error,
                    message: format!("加载站点失败: {}", e),
                });
                return;
            }
        };

        for site in &sites {
            match storage.list_accounts_by_site(site.id) {
                Ok(accounts) if !accounts.is_empty() => {
                    site_accounts.push((site.clone(), accounts));
                }
                _ => {}
            }
        }

        if site_accounts.is_empty() {
            self.state.log_entries.push(LogEntry {
                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                level: LogLevel::Warning,
                message: "没有可签到的账户".into(),
            });
            return;
        }

        // 设置运行状态
        self.state.running = true;
        self.state.bg_running.store(true, Ordering::Relaxed);

        self.state.log_entries.push(LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            level: LogLevel::Info,
            message: format!("开始签到... 共 {} 个站点", site_accounts.len()),
        });

        // 创建 channel
        let (tx, rx) = mpsc::channel::<LogEntry>();
        self.state.log_rx = Some(rx);

        let bg_running = self.state.bg_running.clone();
        let db_path = Storage::default_path();

        // 启动后台线程
        std::thread::spawn(move || {
            // 在后台线程中创建独立的 tokio runtime 和 Storage 实例
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(LogEntry {
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        level: LogLevel::Error,
                        message: format!("创建 runtime 失败: {}", e),
                    });
                    bg_running.store(false, Ordering::Relaxed);
                    return;
                }
            };

            let bg_storage = match Storage::open(&db_path) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(LogEntry {
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        level: LogLevel::Error,
                        message: format!("后台线程打开数据库失败: {}", e),
                    });
                    bg_running.store(false, Ordering::Relaxed);
                    return;
                }
            };

            rt.block_on(async {
                for (site, accounts) in &site_accounts {
                    let _ = tx.send(LogEntry {
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        level: LogLevel::Info,
                        message: format!(
                            "正在签到站点 {} ({} 个账户)...",
                            site.name,
                            accounts.len()
                        ),
                    });

                    match anyrouter_core::service::checkin_accounts(
                        &bg_storage,
                        site,
                        accounts,
                        true, // headless
                    )
                    .await
                    {
                        Ok(results) => {
                            for r in &results {
                                let entry = if r.success {
                                    let balance_info = match (r.balance_before, r.balance_after) {
                                        (Some(before), Some(after)) => {
                                            format!(" (余额: {:.2} -> {:.2})", before, after)
                                        }
                                        _ => String::new(),
                                    };
                                    LogEntry {
                                        timestamp: chrono::Local::now()
                                            .format("%H:%M:%S")
                                            .to_string(),
                                        level: LogLevel::Success,
                                        message: format!(
                                            "[{}] {} 签到成功{}",
                                            site.name, r.account_name, balance_info
                                        ),
                                    }
                                } else {
                                    LogEntry {
                                        timestamp: chrono::Local::now()
                                            .format("%H:%M:%S")
                                            .to_string(),
                                        level: LogLevel::Error,
                                        message: format!(
                                            "[{}] {} 签到失败: {}",
                                            site.name,
                                            r.account_name,
                                            r.error.as_deref().unwrap_or("未知错误")
                                        ),
                                    }
                                };
                                let _ = tx.send(entry);
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(LogEntry {
                                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                                level: LogLevel::Error,
                                message: format!("[{}] 签到执行出错: {}", site.name, e),
                            });
                        }
                    }
                }
            });

            let _ = tx.send(LogEntry {
                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                level: LogLevel::Info,
                message: "签到任务完成".into(),
            });

            bg_running.store(false, Ordering::Relaxed);
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
