use gpui::Hsla;
use gpui_component::theme::ThemeColor;
use gpui_component::Theme;

/// 深邃黑玻璃拟物配色方案
/// 基础背景: #0a0a0f (近纯黑)
/// 面板背景: rgba(15, 15, 25, 0.75) (半透明深黑)
/// 卡片层:   rgba(25, 25, 40, 0.6) (玻璃层)
/// 边框:     rgba(100, 100, 140, 0.15) (微光边)
/// 主色调:   #6366f1 (靛蓝紫)

/// 将 RGB (0-255) 转换为 HSL (0.0-1.0)
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if (max - r).abs() < f32::EPSILON {
        ((g - b) / d + (if g < b { 6.0 } else { 0.0 })) / 6.0
    } else if (max - g).abs() < f32::EPSILON {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, s, l)
}

/// 从 RGBA 创建 Hsla (正确转换 RGB→HSL)
fn rgba(r: u8, g: u8, b: u8, a: f32) -> Hsla {
    let (h, s, l) = rgb_to_hsl(r, g, b);
    Hsla { h, s, l, a }
}

/// 深邃黑玻璃拟物主题色
pub fn abyss_glass_theme_color() -> ThemeColor {
    let mut colors = ThemeColor::default();

    // === 基础色彩 ===
    let base_bg = rgba(10, 10, 15, 1.0);          // #0a0a0f 近纯黑
    let card_bg = rgba(25, 25, 40, 0.6);           // 玻璃层
    let border_color = rgba(100, 100, 140, 0.15);  // 微光边
    let primary = rgba(99, 102, 241, 1.0);          // #6366f1 靛蓝紫
    let primary_hover = rgba(120, 123, 255, 1.0);
    let success = rgba(34, 197, 94, 1.0);           // #22c55e
    let danger = rgba(239, 68, 68, 1.0);            // #ef4444
    let warning = rgba(234, 179, 8, 1.0);           // #eab308
    let text_primary = rgba(226, 232, 240, 1.0);    // #e2e8f0
    let text_secondary = rgba(148, 163, 184, 1.0);  // #94a3b8
    let text_muted = rgba(100, 116, 139, 1.0);      // #64748b

    // === 映射到 ThemeColor 字段 ===
    colors.background = base_bg;
    colors.foreground = text_primary;
    colors.primary = primary;
    colors.primary_hover = primary_hover;
    colors.primary_foreground = rgba(255, 255, 255, 1.0);

    colors.secondary = card_bg;
    colors.secondary_hover = rgba(35, 35, 55, 0.7);
    colors.secondary_foreground = text_primary;

    colors.accent = rgba(99, 102, 241, 0.15);
    colors.accent_foreground = primary;

    colors.danger = danger;
    colors.danger_hover = rgba(220, 50, 50, 1.0);
    colors.danger_foreground = rgba(255, 255, 255, 1.0);

    colors.muted = rgba(30, 30, 45, 0.5);
    colors.muted_foreground = text_muted;

    colors.border = border_color;
    colors.ring = primary;
    colors.input = rgba(20, 20, 35, 0.8);
    colors.selection = rgba(99, 102, 241, 0.3);

    // 弹出层 (Popover)
    colors.popover = rgba(20, 20, 35, 0.95);
    colors.popover_foreground = text_primary;

    // Sidebar
    colors.sidebar = rgba(10, 10, 20, 0.8);
    colors.sidebar_foreground = text_secondary;
    colors.sidebar_accent = rgba(99, 102, 241, 0.15);
    colors.sidebar_accent_foreground = primary;
    colors.sidebar_border = border_color;
    colors.sidebar_primary = primary;
    colors.sidebar_primary_foreground = rgba(255, 255, 255, 1.0);

    // 标签页
    colors.tab = rgba(20, 20, 35, 0.4);
    colors.tab_foreground = text_secondary;
    colors.tab_active = rgba(99, 102, 241, 0.2);
    colors.tab_active_foreground = primary;

    // 图表色板
    colors.chart_1 = primary;
    colors.chart_2 = success;
    colors.chart_3 = warning;
    colors.chart_4 = danger;
    colors.chart_5 = rgba(168, 85, 247, 1.0); // #a855f7

    colors
}

/// 注册并应用深邃黑玻璃拟物主题
pub fn apply_abyss_theme(cx: &mut gpui::App) {
    let theme_colors = abyss_glass_theme_color();
    let theme = Theme::from(&theme_colors);
    *Theme::global_mut(cx) = theme;
}

/// 玻璃拟物风格的颜色常量，供 UI 组件直接使用
pub struct Glass;

impl Glass {
    /// 面板背景 rgba(15, 15, 25, 0.75)
    pub fn panel() -> Hsla { rgba(15, 15, 25, 0.75) }
    /// 卡片背景 rgba(25, 25, 40, 0.6)
    pub fn card() -> Hsla { rgba(25, 25, 40, 0.6) }
    /// 卡片悬停 rgba(35, 35, 55, 0.7)
    pub fn card_hover() -> Hsla { rgba(35, 35, 55, 0.7) }
    /// 微光边框 rgba(100, 100, 140, 0.15)
    pub fn border() -> Hsla { rgba(100, 100, 140, 0.15) }
    /// 亮边框 rgba(140, 140, 180, 0.25)
    pub fn border_bright() -> Hsla { rgba(140, 140, 180, 0.25) }
    /// 侧边栏背景 rgba(10, 10, 20, 0.8)
    pub fn sidebar() -> Hsla { rgba(10, 10, 20, 0.8) }
    /// 日志终端背景 #0d0d14
    pub fn terminal() -> Hsla { rgba(13, 13, 20, 1.0) }
    /// 主色调 #6366f1
    pub fn primary() -> Hsla { rgba(99, 102, 241, 1.0) }
    /// 成功色 #22c55e
    pub fn success() -> Hsla { rgba(34, 197, 94, 1.0) }
    /// 错误色 #ef4444
    pub fn danger() -> Hsla { rgba(239, 68, 68, 1.0) }
    /// 警告色 #eab308
    pub fn warning() -> Hsla { rgba(234, 179, 8, 1.0) }
    /// 信息蓝 #60a5fa
    pub fn info() -> Hsla { rgba(96, 165, 250, 1.0) }
    /// 主文字 #e2e8f0
    pub fn text() -> Hsla { rgba(226, 232, 240, 1.0) }
    /// 次文字 #94a3b8
    pub fn text_secondary() -> Hsla { rgba(148, 163, 184, 1.0) }
    /// 弱化文字 #64748b
    pub fn text_muted() -> Hsla { rgba(100, 116, 139, 1.0) }
    /// 透明背景
    pub fn transparent() -> Hsla { Hsla { h: 0.0, s: 0.0, l: 0.0, a: 0.0 } }
}
