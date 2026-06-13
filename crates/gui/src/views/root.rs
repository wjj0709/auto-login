use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Bounds, Context, Entity, IntoElement, ParentElement, Render, Styled, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, size,
};
use gpui_platform::application;

use crate::app_state::{AppState, LogEntry, LogLevel};
use crate::theme;
use crate::views;

pub struct RootView {
    pub state: Entity<AppState>,
}

impl RootView {
    pub fn new(state: Entity<AppState>, _cx: &mut Context<Self>) -> Self {
        Self { state }
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state_ref = self.state.clone();
        let log_drawer_open;
        let active_modal_some;
        let running;
        let progress_text;
        {
            let state = state_ref.read(cx);
            log_drawer_open = state.log_drawer_open;
            active_modal_some = state.active_modal.is_some();
            running = state.running;
            progress_text = state
                .run_progress
                .clone()
                .unwrap_or_else(|| if running { "运行中…".into() } else { "● 就绪".into() });
        }

        let titlebar = self.render_titlebar(cx);
        let content = self.render_content(cx);
        let bottombar = self.render_bottombar(progress_text, cx);
        let log_drawer = if log_drawer_open {
            Some(crate::views::log_drawer::render(state_ref.clone(), cx))
        } else {
            None
        };
        let modal = if active_modal_some {
            Some(crate::views::modals::render(state_ref, cx))
        } else {
            None
        };

        div()
            .size_full()
            .bg(theme::bg_window())
            .text_color(theme::text_primary())
            .text_size(px(12.0))
            .flex()
            .flex_col()
            .child(titlebar)
            .child(div().flex_1().overflow_hidden().child(content))
            .when_some(log_drawer, |this, el| this.child(el))
            .when_some(modal, |this, el| this.child(el))
            .child(bottombar)
    }
}

impl RootView {
    fn render_titlebar(&self, _cx: &mut Context<Self>) -> AnyElement {
        let state = self.state.clone();
        div()
            .w_full()
            .h(px(40.0))
            .bg(theme::bg_bar())
            .border_b_1()
            .border_color(theme::border_normal())
            .flex()
            .items_center()
            .justify_between()
            .px(px(16.0))
            .child(
                div()
                    .text_color(theme::text_primary())
                    .text_size(px(13.0))
                    .child("🅰 AnyRouter · Auto Check-in"),
            )
            .child(
                div()
                    .id("checkin-all-btn")
                    .px(px(12.0))
                    .py(px(4.0))
                    .bg(theme::btn_primary_bg())
                    .border_1()
                    .border_color(theme::btn_primary_border())
                    .rounded(px(5.0))
                    .text_color(theme::accent_blue())
                    .text_size(px(11.0))
                    .cursor_pointer()
                    .hover(|this| this.opacity(0.85))
                    .child("⚡ 一键签到全部")
                    .on_click(move |_, _window, cx| {
                        trigger_checkin_all(state.clone(), cx);
                    }),
            )
            .into_any_element()
    }

    fn render_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let view_kind = self.state.read(cx).current_view.clone();
        match view_kind {
            crate::app_state::ViewKind::Home => {
                views::home::render(self.state.clone(), cx).into_any_element()
            }
            crate::app_state::ViewKind::AccountDetail(account_id) => {
                views::detail::render(self.state.clone(), account_id, cx).into_any_element()
            }
        }
    }

    fn render_bottombar(&self, progress_text: String, cx: &mut Context<Self>) -> AnyElement {
        let state = self.state.clone();
        let log_open = state.read(cx).log_drawer_open;
        let log_btn_bg = if log_open {
            theme::btn_primary_bg()
        } else {
            theme::bg_card()
        };
        let log_btn_border = if log_open {
            theme::btn_primary_border()
        } else {
            theme::border_normal()
        };
        let log_btn_text = if log_open {
            theme::accent_blue()
        } else {
            theme::text_muted()
        };

        div()
            .w_full()
            .h(px(28.0))
            .bg(theme::bg_bar())
            .border_t_1()
            .border_color(theme::border_normal())
            .flex()
            .items_center()
            .justify_between()
            .px(px(12.0))
            .child(
                div()
                    .text_color(theme::text_muted())
                    .text_size(px(10.0))
                    .child(progress_text),
            )
            .child(
                div()
                    .id("log-toggle-btn")
                    .px(px(8.0))
                    .py(px(2.0))
                    .bg(log_btn_bg)
                    .border_1()
                    .border_color(log_btn_border)
                    .rounded(px(5.0))
                    .text_color(log_btn_text)
                    .text_size(px(10.0))
                    .cursor_pointer()
                    .hover(|this| this.opacity(0.85))
                    .child("▤ 日志")
                    .on_click(move |_, _window, cx| {
                        state.update(cx, |state, cx| {
                            state.log_drawer_open = !state.log_drawer_open;
                            cx.notify();
                        });
                    }),
            )
            .into_any_element()
    }
}

/// 触发刷新单个账户的详情数据：spawn 后台线程执行 fetch_detail
pub fn trigger_fetch_detail(state: Entity<AppState>, account_id: i64, cx: &mut gpui::App) {
    state.update(cx, |st, cx| {
        if st.bg_running.load(Ordering::Relaxed) {
            return;
        }
        let now = chrono::Local::now().format("%H:%M:%S").to_string();

        let (tx, rx) = mpsc::channel::<LogEntry>();
        st.log_rx = Some(rx);
        st.log_drawer_open = true;
        st.running = true;
        st.run_progress = Some(format!("拉取账户 #{} 详情…", account_id));
        st.log_entries.push(LogEntry {
            timestamp: now,
            level: LogLevel::Info,
            message: format!("开始刷新账户 #{} 详情", account_id),
        });

        let bg_running = Arc::new(AtomicBool::new(true));
        st.bg_running = bg_running.clone();

        let db_path = anyrouter_core::storage::Storage::default_path();
        std::thread::spawn(move || {
            run_fetch_detail_in_thread(db_path, account_id, tx, bg_running);
        });

        cx.notify();
    });
}

fn run_fetch_detail_in_thread(
    db_path: std::path::PathBuf,
    account_id: i64,
    tx: mpsc::Sender<LogEntry>,
    bg_running: Arc<AtomicBool>,
) {
    let send = |level: LogLevel, msg: String| {
        let _ = tx.send(LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            level,
            message: msg,
        });
    };

    let storage = match anyrouter_core::storage::Storage::open(&db_path) {
        Ok(s) => s,
        Err(e) => {
            send(LogLevel::Error, format!("打开数据库失败: {}", e));
            bg_running.store(false, Ordering::Relaxed);
            return;
        }
    };

    let account = match storage.get_account(account_id) {
        Ok(Some(a)) => a,
        Ok(None) => {
            send(LogLevel::Error, format!("账户 #{} 不存在", account_id));
            bg_running.store(false, Ordering::Relaxed);
            return;
        }
        Err(e) => {
            send(LogLevel::Error, format!("读取账户失败: {}", e));
            bg_running.store(false, Ordering::Relaxed);
            return;
        }
    };

    let site = match storage.get_site(account.site_id) {
        Ok(Some(s)) => s,
        _ => {
            send(LogLevel::Error, format!("站点 #{} 不存在", account.site_id));
            bg_running.store(false, Ordering::Relaxed);
            return;
        }
    };

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            send(LogLevel::Error, format!("创建 runtime 失败: {}", e));
            bg_running.store(false, Ordering::Relaxed);
            return;
        }
    };

    match rt.block_on(anyrouter_core::service::fetch_detail(
        &storage, &site, &account, true,
    )) {
        Ok(_) => send(
            LogLevel::Success,
            format!("[{}] {} 详情已刷新", site.name, account.name),
        ),
        Err(e) => send(
            LogLevel::Error,
            format!("[{}] {} 刷新失败：{}", site.name, account.name, e),
        ),
    }

    bg_running.store(false, Ordering::Relaxed);
}

/// GUI 入口（由 main.rs 调用）
pub fn run_app(storage: anyrouter_core::storage::Storage) {
    let db_path = anyrouter_core::storage::Storage::default_path();
    application().run(move |cx: &mut App| {
        let state = cx.new(|_| AppState::from_storage(storage));
        let bounds = Bounds::centered(None, size(px(1100.0), px(750.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| RootView::new(state.clone(), cx)),
        )
        .unwrap();
        cx.activate(true);

        // 启动后台日志轮询：每 200ms 把 channel 中的日志写入 state
        spawn_log_poller(state, db_path, cx);
    });
}

fn spawn_log_poller(state: Entity<AppState>, _db_path: std::path::PathBuf, cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(200))
                .await;
            let had_entry = cx.update(|cx| {
                state.update(cx, |st, _| {
                    let mut had_entry = false;
                    if let Some(ref rx) = st.log_rx {
                        while let Ok(entry) = rx.try_recv() {
                            st.log_entries.push(entry);
                            had_entry = true;
                        }
                    }
                    let bg_run = st.bg_running.load(Ordering::Relaxed);
                    if !bg_run && st.running {
                        st.running = false;
                        st.run_progress = None;
                        st.reload_sites();
                        had_entry = true;
                    }
                    had_entry
                })
            });
            if had_entry {
                cx.update(|cx| state.update(cx, |_, cx| cx.notify()));
            }
        }
    })
    .detach();
}

/// 触发"一键签到全部"：spawn 后台线程执行所有站点的签到
fn trigger_checkin_all(state: Entity<AppState>, cx: &mut App) {
    state.update(cx, |st, cx| {
        if st.running {
            return;
        }
        if st.bg_running.load(Ordering::Relaxed) {
            return;
        }
        let now = chrono::Local::now().format("%H:%M:%S").to_string();

        // 创建 channel
        let (tx, rx) = mpsc::channel::<LogEntry>();
        st.log_rx = Some(rx);
        st.log_drawer_open = true;
        st.running = true;
        st.run_progress = Some("签到准备中…".into());
        st.log_entries.push(LogEntry {
            timestamp: now.clone(),
            level: LogLevel::Info,
            message: "开始执行一键签到".to_string(),
        });

        let bg_running = Arc::new(AtomicBool::new(true));
        st.bg_running = bg_running.clone();

        let db_path = anyrouter_core::storage::Storage::default_path();
        std::thread::spawn(move || {
            run_checkin_in_thread(db_path, tx, bg_running);
        });

        cx.notify();
    });
}

fn run_checkin_in_thread(
    db_path: std::path::PathBuf,
    tx: mpsc::Sender<LogEntry>,
    bg_running: Arc<AtomicBool>,
) {
    let send = |level: LogLevel, msg: String| {
        let _ = tx.send(LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            level,
            message: msg,
        });
    };

    // 在子线程中重新打开 Storage（rusqlite Connection 是 !Send）
    let storage = match anyrouter_core::storage::Storage::open(&db_path) {
        Ok(s) => s,
        Err(e) => {
            send(LogLevel::Error, format!("打开数据库失败: {}", e));
            bg_running.store(false, Ordering::Relaxed);
            return;
        }
    };

    let sites = match storage.list_sites() {
        Ok(s) => s,
        Err(e) => {
            send(LogLevel::Error, format!("读取站点列表失败: {}", e));
            bg_running.store(false, Ordering::Relaxed);
            return;
        }
    };

    if sites.is_empty() {
        send(LogLevel::Warning, "未找到任何站点配置".into());
        bg_running.store(false, Ordering::Relaxed);
        return;
    }

    // 创建 tokio runtime 用于驱动 async service
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            send(LogLevel::Error, format!("创建 runtime 失败: {}", e));
            bg_running.store(false, Ordering::Relaxed);
            return;
        }
    };

    let mut total_success = 0usize;
    let mut total_fail = 0usize;

    for site in &sites {
        let accounts = match storage.list_accounts_by_site(site.id) {
            Ok(a) => a,
            Err(e) => {
                send(
                    LogLevel::Error,
                    format!("[{}] 读取账户失败: {}", site.name, e),
                );
                continue;
            }
        };
        if accounts.is_empty() {
            send(
                LogLevel::Info,
                format!("[{}] 该站点暂无账户，跳过", site.name),
            );
            continue;
        }
        send(
            LogLevel::Info,
            format!("[{}] 开始签到 {} 个账户…", site.name, accounts.len()),
        );

        let result = rt.block_on(anyrouter_core::service::checkin_accounts(
            &storage, site, &accounts, true,
        ));

        match result {
            Ok(results) => {
                for r in results {
                    if r.success {
                        total_success += 1;
                        let after = r.balance_after.map(|b| format!(" 余额 ${:.2}", b)).unwrap_or_default();
                        send(
                            LogLevel::Success,
                            format!("[{}] {} 签到成功{}", site.name, r.account_name, after),
                        );
                    } else {
                        total_fail += 1;
                        send(
                            LogLevel::Error,
                            format!(
                                "[{}] {} 签到失败：{}",
                                site.name,
                                r.account_name,
                                r.error.unwrap_or_else(|| "未知错误".into())
                            ),
                        );
                    }
                }
            }
            Err(e) => {
                send(
                    LogLevel::Error,
                    format!("[{}] 调用 Playwright 失败: {}", site.name, e),
                );
                total_fail += accounts.len();
            }
        }
    }

    send(
        LogLevel::Info,
        format!("签到完成：成功 {} / 失败 {}", total_success, total_fail),
    );
    bg_running.store(false, Ordering::Relaxed);
}
