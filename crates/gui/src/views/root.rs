use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use gpui::{
    AnyElement, App, Bounds, Context, Entity, IntoElement, MouseButton, ParentElement, Render,
    Styled, TitlebarOptions, Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
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
    fn render_titlebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.state.clone();
        let running = self.state.read(cx).running;
        let label = if running {
            "⏳ 签到运行中…"
        } else {
            "⚡ 一键签到全部"
        };
        div()
            .w_full()
            .h(px(40.0))
            .bg(theme::bg_bar())
            .border_b_1()
            .border_color(theme::border_normal())
            .flex()
            .items_center()
            .justify_between()
            .pl(px(16.0))
            // 左侧：应用标题（可拖动区域）
            .child(
                div()
                    .id("titlebar-drag")
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .text_color(theme::text_primary())
                    .text_size(px(13.0))
                    .child("🅰 AnyRouter · Auto Check-in")
                    // 在标题空白区按下鼠标即可拖动窗口
                    .on_mouse_down(MouseButton::Left, |_e, window, _cx| {
                        window.start_window_move();
                    }),
            )
            // 右侧：一键签到按钮 + 窗口控制按钮
            .child(
                div()
                    .flex()
                    .items_center()
                    .h_full()
                    .child(
                        div()
                            .id("checkin-all-btn")
                            .px(px(12.0))
                            .py(px(4.0))
                            .mr(px(12.0))
                            .bg(if running {
                                theme::bg_card()
                            } else {
                                theme::btn_primary_bg()
                            })
                            .border_1()
                            .border_color(if running {
                                theme::border_normal()
                            } else {
                                theme::btn_primary_border()
                            })
                            .rounded(px(5.0))
                            .text_color(if running {
                                theme::text_weakest()
                            } else {
                                theme::accent_blue()
                            })
                            .text_size(px(11.0))
                            .when(!running, |this| {
                                this.cursor_pointer().hover(|this| this.opacity(0.85))
                            })
                            .child(label)
                            .on_click(move |_, _window, cx| {
                                trigger_checkin_all(state.clone(), cx);
                            }),
                    )
                    .child(window_control("win-min", "—", theme::text_muted(), |window, _cx| {
                        window.minimize_window();
                    }))
                    .child(window_control("win-max", "▢", theme::text_muted(), |window, _cx| {
                        window.zoom_window();
                    }))
                    .child(window_control("win-close", "✕", theme::error_red(), |_window, cx| {
                        cx.quit();
                    })),
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
        let snap = state.read(cx);
        let log_open = snap.log_drawer_open;
        let running = snap.running;
        let log_count = snap.log_entries.len();
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

        let status_color = if running {
            theme::warning_yellow()
        } else {
            theme::success_green()
        };
        let log_label = if log_count > 0 {
            format!("▤ 日志 ({})", log_count)
        } else {
            "▤ 日志".to_string()
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
                    .text_color(status_color)
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
                    .child(log_label)
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

/// 触发账密登录刷新 Cookie：spawn 后台线程执行 login_account
pub fn trigger_login_account(state: Entity<AppState>, account_id: i64, cx: &mut gpui::App) {
    state.update(cx, |st, cx| {
        if st.bg_running.load(Ordering::Relaxed) {
            return;
        }
        let now = chrono::Local::now().format("%H:%M:%S").to_string();

        let (tx, rx) = mpsc::channel::<LogEntry>();
        st.log_rx = Some(rx);
        st.log_drawer_open = true;
        st.running = true;
        st.run_progress = Some(format!("登录账户 #{}…", account_id));
        st.log_entries.push(LogEntry {
            timestamp: now,
            level: LogLevel::Info,
            message: format!("开始账密登录刷新账户 #{} Cookie", account_id),
        });

        let bg_running = Arc::new(AtomicBool::new(true));
        st.bg_running = bg_running.clone();

        let db_path = anyrouter_core::storage::Storage::default_path();
        std::thread::spawn(move || {
            run_login_in_thread(db_path, account_id, tx, bg_running);
        });

        cx.notify();
    });
}

fn run_login_in_thread(
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
        _ => {
            send(LogLevel::Error, format!("账户 #{} 不存在", account_id));
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

    if account.username.is_none() || account.password.is_none() {
        send(
            LogLevel::Warning,
            format!("账户 {} 未配置用户名密码，无法账密登录", account.name),
        );
        bg_running.store(false, Ordering::Relaxed);
        return;
    }

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

    match rt.block_on(anyrouter_core::service::login_account(
        &storage, &site, &account, true,
    )) {
        Ok(_) => send(
            LogLevel::Success,
            format!("[{}] {} 账密登录成功，Cookie 已更新", site.name, account.name),
        ),
        Err(e) => send(
            LogLevel::Error,
            format!("[{}] {} 账密登录失败：{}", site.name, account.name, e),
        ),
    }

    bg_running.store(false, Ordering::Relaxed);
}

/// 触发单个站点的签到：spawn 后台线程执行 checkin_accounts
pub fn trigger_checkin_site(state: Entity<AppState>, site_id: i64, cx: &mut gpui::App) {
    state.update(cx, |st, cx| {
        if st.bg_running.load(Ordering::Relaxed) {
            return;
        }
        let now = chrono::Local::now().format("%H:%M:%S").to_string();

        let (tx, rx) = mpsc::channel::<LogEntry>();
        st.log_rx = Some(rx);
        st.log_drawer_open = true;
        st.running = true;
        st.run_progress = Some(format!("签到站点 #{}…", site_id));
        st.log_entries.push(LogEntry {
            timestamp: now,
            level: LogLevel::Info,
            message: format!("开始签到站点 #{}", site_id),
        });

        let bg_running = Arc::new(AtomicBool::new(true));
        st.bg_running = bg_running.clone();

        let db_path = anyrouter_core::storage::Storage::default_path();
        std::thread::spawn(move || {
            run_checkin_site_in_thread(db_path, site_id, tx, bg_running);
        });

        cx.notify();
    });
}

fn run_checkin_site_in_thread(
    db_path: std::path::PathBuf,
    site_id: i64,
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

    let site = match storage.get_site(site_id) {
        Ok(Some(s)) => s,
        _ => {
            send(LogLevel::Error, format!("站点 #{} 不存在", site_id));
            bg_running.store(false, Ordering::Relaxed);
            return;
        }
    };

    let accounts = match storage.list_accounts_by_site(site_id) {
        Ok(a) => a,
        Err(e) => {
            send(LogLevel::Error, format!("读取账户失败: {}", e));
            bg_running.store(false, Ordering::Relaxed);
            return;
        }
    };

    if accounts.is_empty() {
        send(
            LogLevel::Warning,
            format!("[{}] 该站点暂无账户", site.name),
        );
        bg_running.store(false, Ordering::Relaxed);
        return;
    }

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

    send(
        LogLevel::Info,
        format!("[{}] 签到 {} 个账户中…", site.name, accounts.len()),
    );

    match rt.block_on(anyrouter_core::service::checkin_accounts(
        &storage, &site, &accounts, true,
    )) {
        Ok(results) => {
            let mut ok = 0;
            let mut fail = 0;
            for r in results {
                if r.success {
                    ok += 1;
                    let after = r
                        .balance_after
                        .map(|b| format!(" 余额 ${:.2}", b))
                        .unwrap_or_default();
                    send(
                        LogLevel::Success,
                        format!("[{}] {} 签到成功{}", site.name, r.account_name, after),
                    );
                } else {
                    fail += 1;
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
            send(
                LogLevel::Info,
                format!("[{}] 完成：成功 {} / 失败 {}", site.name, ok, fail),
            );
        }
        Err(e) => send(
            LogLevel::Error,
            format!("[{}] 调用 Playwright 失败: {}", site.name, e),
        ),
    }

    bg_running.store(false, Ordering::Relaxed);
}

/// 触发单个账户签到：spawn 后台线程执行 checkin_accounts（仅 1 个账户）
pub fn trigger_checkin_account(state: Entity<AppState>, account_id: i64, cx: &mut gpui::App) {
    state.update(cx, |st, cx| {
        if st.bg_running.load(Ordering::Relaxed) {
            return;
        }
        let now = chrono::Local::now().format("%H:%M:%S").to_string();

        let (tx, rx) = mpsc::channel::<LogEntry>();
        st.log_rx = Some(rx);
        st.log_drawer_open = true;
        st.running = true;
        st.run_progress = Some(format!("签到账户 #{}…", account_id));
        st.log_entries.push(LogEntry {
            timestamp: now,
            level: LogLevel::Info,
            message: format!("开始签到账户 #{}", account_id),
        });

        let bg_running = Arc::new(AtomicBool::new(true));
        st.bg_running = bg_running.clone();

        let db_path = anyrouter_core::storage::Storage::default_path();
        std::thread::spawn(move || {
            run_checkin_account_in_thread(db_path, account_id, tx, bg_running);
        });

        cx.notify();
    });
}

fn run_checkin_account_in_thread(
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
        _ => {
            send(LogLevel::Error, format!("账户 #{} 不存在", account_id));
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

    match rt.block_on(anyrouter_core::service::checkin_accounts(
        &storage,
        &site,
        std::slice::from_ref(&account),
        true,
    )) {
        Ok(results) => {
            if let Some(r) = results.into_iter().next() {
                if r.success {
                    let after = r
                        .balance_after
                        .map(|b| format!(" 余额 ${:.2}", b))
                        .unwrap_or_default();
                    send(
                        LogLevel::Success,
                        format!("[{}] {} 签到成功{}", site.name, r.account_name, after),
                    );
                } else {
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
        Err(e) => send(
            LogLevel::Error,
            format!("[{}] 调用 Playwright 失败: {}", site.name, e),
        ),
    }

    bg_running.store(false, Ordering::Relaxed);
}

/// 渲染一个窗口控制按钮（最小化/最大化/关闭）
fn window_control(
    id: &'static str,
    glyph: &'static str,
    hover_color: gpui::Hsla,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .w(px(40.0))
        .h(px(40.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(12.0))
        .text_color(theme::text_muted())
        .cursor_pointer()
        .hover(move |this| this.bg(theme::bg_card()).text_color(hover_color))
        .child(glyph)
        .on_click(move |_, window, cx| on_click(window, cx))
}

/// GUI 入口（由 main.rs 调用）
pub fn run_app(storage: anyrouter_core::storage::Storage) {
    let db_path = anyrouter_core::storage::Storage::default_path();
    application().run(move |cx: &mut App| {
        bind_text_input_keys(cx);
        let state = cx.new(|_| AppState::from_storage(storage));
        let bounds = Bounds::centered(None, size(px(1100.0), px(750.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // 隐藏系统标题栏，使用自绘标题栏（最小化/最大化/关闭 + 拖动）
                titlebar: Some(TitlebarOptions {
                    title: Some("AnyRouter".into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
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

/// 绑定文本输入框所需的键位（Windows 使用 ctrl，macOS 兼容 cmd）
fn bind_text_input_keys(cx: &mut App) {
    use crate::components::text_input::*;
    cx.bind_keys([
        gpui::KeyBinding::new("backspace", TextBackspace, Some("TextInput")),
        gpui::KeyBinding::new("delete", TextDelete, Some("TextInput")),
        gpui::KeyBinding::new("left", TextLeft, Some("TextInput")),
        gpui::KeyBinding::new("right", TextRight, Some("TextInput")),
        gpui::KeyBinding::new("shift-left", TextSelectLeft, Some("TextInput")),
        gpui::KeyBinding::new("shift-right", TextSelectRight, Some("TextInput")),
        gpui::KeyBinding::new("home", TextHome, Some("TextInput")),
        gpui::KeyBinding::new("end", TextEnd, Some("TextInput")),
        gpui::KeyBinding::new("ctrl-a", TextSelectAll, Some("TextInput")),
        gpui::KeyBinding::new("ctrl-c", TextCopy, Some("TextInput")),
        gpui::KeyBinding::new("ctrl-x", TextCut, Some("TextInput")),
        gpui::KeyBinding::new("ctrl-v", TextPaste, Some("TextInput")),
        gpui::KeyBinding::new("cmd-a", TextSelectAll, Some("TextInput")),
        gpui::KeyBinding::new("cmd-c", TextCopy, Some("TextInput")),
        gpui::KeyBinding::new("cmd-x", TextCut, Some("TextInput")),
        gpui::KeyBinding::new("cmd-v", TextPaste, Some("TextInput")),
    ]);
}

fn spawn_log_poller(state: Entity<AppState>, _db_path: std::path::PathBuf, cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(200))
                .await;
            let had_entry = cx.update(|cx| {
                state.update(cx, |st, _| {
                    let before_len = st.log_entries.len();
                    let was_running = st.running;
                    st.poll_bg_logs();
                    let mut had_entry = st.log_entries.len() != before_len;
                    if was_running && !st.running {
                        // 后台任务刚结束，清理进度并刷新主页统计
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
