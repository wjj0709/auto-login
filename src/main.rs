mod app_state;
mod assets;
mod balance;
mod checkin;
mod config;
mod crypto;
mod log;
mod log_panel;
mod notify;
mod playwright;
mod root_view;
mod service;
mod storage;
mod theme;

use gpui::*;

use app_state::AppState;
use assets::Assets;
use root_view::RootView;

fn main() {
    // 加载 .env(仅运维变量:PYTHON_BIN / PLAYWRIGHT_SCRIPT / PLAYWRIGHT_HEADLESS 等)
    dotenvy::dotenv().ok();

    // 初始化加密与数据库;失败则记日志退出(错误弹窗在阶段 2 UI 重构时补充)
    let crypto = match crypto::Crypto::from_keyring() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[启动失败] 系统凭据库不可用: {e:#}");
            std::process::exit(1);
        }
    };
    let storage = match storage::Storage::open_default(crypto) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[启动失败] 数据库初始化失败: {e:#}");
            std::process::exit(1);
        }
    };
    if let Err(e) = storage.import_env_if_needed() {
        eprintln!("[警告] 旧配置导入失败,以现有数据继续: {e:#}");
    }
    let sites = match storage.list_sites() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[启动失败] 读取站点数据失败: {e:#}");
            std::process::exit(1);
        }
    };
    // 解密失败(如主密钥已更换)必须显式失败,不得 unwrap_or_default 静默清空账户列表
    let accounts = match storage.list_all_accounts() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[启动失败] 读取账户数据失败(主密钥可能已更换): {e:#}");
            std::process::exit(1);
        }
    };
    let db = std::sync::Arc::new(std::sync::Mutex::new(storage));

    // 启动 GPUI 应用（注册图标资源加载器，供标题栏与侧边栏图标使用）
    let app = Application::new().with_assets(Assets);

    app.run(move |cx| {
        // 初始化 gpui-component
        gpui_component::init(cx);

        // 应用深邃黑玻璃拟物主题
        theme::apply_abyss_theme(cx);

        // 创建全局应用状态
        let app_state = cx.new(|_cx| AppState::new(sites, accounts, db));

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
