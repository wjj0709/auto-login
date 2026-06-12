use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

/// 编译期内嵌的图标资源表（路径 -> SVG 字节）。
///
/// gpui-component 的 `IconName` 仅返回 `icons/*.svg` 路径，并不内置 SVG 文件，
/// 需由宿主应用通过 `AssetSource` 提供。这里使用 `include_bytes!` 将图标直接
/// 编译进二进制，无需运行期文件系统依赖。
const ICONS: &[(&str, &[u8])] = &[
    // 侧边栏导航图标（Lucide）
    ("icons/user.svg", include_bytes!("../assets/icons/user.svg")),
    ("icons/square-terminal.svg", include_bytes!("../assets/icons/square-terminal.svg")),
    ("icons/layout-dashboard.svg", include_bytes!("../assets/icons/layout-dashboard.svg")),
    // 标题栏窗口控制按钮图标
    ("icons/window-minimize.svg", include_bytes!("../assets/icons/window-minimize.svg")),
    ("icons/window-maximize.svg", include_bytes!("../assets/icons/window-maximize.svg")),
    ("icons/window-restore.svg", include_bytes!("../assets/icons/window-restore.svg")),
    ("icons/window-close.svg", include_bytes!("../assets/icons/window-close.svg")),
];

/// 为 GPUI 提供图标 SVG 的资源加载器。
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect())
    }
}
