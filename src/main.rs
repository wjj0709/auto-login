mod account_panel;
mod app_state;
mod assets;
mod balance;
mod checkin;
mod config;
mod crypto;
mod dashboard;
mod log;
mod log_panel;
mod notify;
mod playwright;
mod root_view;
mod service;
mod theme;

use gpui::*;

use app_state::AppState;
use assets::Assets;
use config::load_accounts_config;
use root_view::RootView;

fn main() {
    // 加载 .env 配置
    dotenvy::dotenv().ok();

    // 预加载账号配置
    let accounts = load_accounts_config().unwrap_or_default();

    // 启动 GPUI 应用（注册图标资源加载器，供标题栏与侧边栏图标使用）
    let app = Application::new().with_assets(Assets);

    app.run(move |cx| {
        // 初始化 gpui-component
        gpui_component::init(cx);

        // 应用深邃黑玻璃拟物主题
        theme::apply_abyss_theme(cx);

        // 创建全局应用状态
        let app_state = cx.new(|_cx| AppState::new(accounts));

        // 打开主窗口
        let bounds = Bounds::centered(None, size(px(1100.0), px(700.0)), cx);

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            // 使用 gpui-component 的标题栏选项（透明化原生标题栏，由自定义标题栏接管）
            titlebar: Some(gpui_component::TitleBar::title_bar_options()),
            window_background: WindowBackgroundAppearance::Blurred,
            ..Default::default()
        };

        cx.open_window(window_options, |window, cx| {
            let view = cx.new(|cx| RootView::new(app_state, window, cx));
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("无法打开主窗口");
    });
}
