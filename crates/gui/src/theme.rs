#![allow(dead_code)]

use egui::Color32;

// ─── 窗口 & 容器底色 ───────────────────────────────────────────
pub const WINDOW_BG: Color32 = Color32::from_rgb(0x0a, 0x0c, 0x10);
pub const CARD_BG: Color32 = Color32::from_rgb(0x14, 0x19, 0x23);
pub const BAR_BG: Color32 = Color32::from_rgb(0x11, 0x14, 0x1a);

// ─── 边框 ──────────────────────────────────────────────────────
pub const BORDER_NORMAL: Color32 = Color32::from_rgb(0x2a, 0x2f, 0x3a);
pub const BORDER_ACCENT: Color32 = Color32::from_rgb(0x3a, 0x41, 0x50);

// ─── 文字色阶 ─────────────────────────────────────────────────
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xdb, 0xe2, 0xec);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xaa, 0xb2, 0xc0);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8a, 0x93, 0xa5);
pub const TEXT_WEAKEST: Color32 = Color32::from_rgb(0x56, 0x65, 0x7d);

// ─── 语义色 ───────────────────────────────────────────────────
pub const ACCENT_BLUE: Color32 = Color32::from_rgb(0x7f, 0xb0, 0xff);
pub const SUCCESS_GREEN: Color32 = Color32::from_rgb(0x6f, 0xbf, 0x73);
pub const ERROR_RED: Color32 = Color32::from_rgb(0xe0, 0x7b, 0x7b);
pub const WARNING_YELLOW: Color32 = Color32::from_rgb(0xdc, 0xaa, 0x50);

// ─── 按钮主色 ─────────────────────────────────────────────────
pub const BTN_PRIMARY_BG: Color32 = Color32::from_rgba_premultiplied(51, 89, 163, 51); // rgba(80,140,255,0.2) premultiplied
pub const BTN_PRIMARY_BORDER: Color32 = Color32::from_rgba_premultiplied(64, 112, 204, 128); // rgba(80,140,255,0.5)

// ─── 按钮危险色 ───────────────────────────────────────────────
pub const BTN_DANGER_BG: Color32 = Color32::from_rgba_premultiplied(26, 13, 13, 31); // rgba(220,110,110,0.12)
pub const BTN_DANGER_BORDER: Color32 = Color32::from_rgba_premultiplied(99, 50, 50, 115); // rgba(220,110,110,0.45)

/// Apply the AnyRouter dark theme to the given egui context.
pub fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // Visuals
    let visuals = &mut style.visuals;
    visuals.dark_mode = true;
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.panel_fill = WINDOW_BG;
    visuals.window_fill = CARD_BG;
    visuals.extreme_bg_color = WINDOW_BG;
    visuals.faint_bg_color = CARD_BG;

    // Widget visuals
    visuals.widgets.noninteractive.bg_fill = CARD_BG;
    visuals.widgets.noninteractive.fg_stroke.color = TEXT_SECONDARY;
    visuals.widgets.noninteractive.bg_stroke.color = BORDER_NORMAL;

    visuals.widgets.inactive.bg_fill = BAR_BG;
    visuals.widgets.inactive.fg_stroke.color = TEXT_PRIMARY;
    visuals.widgets.inactive.bg_stroke.color = BORDER_NORMAL;

    visuals.widgets.hovered.bg_fill = CARD_BG;
    visuals.widgets.hovered.fg_stroke.color = ACCENT_BLUE;
    visuals.widgets.hovered.bg_stroke.color = BORDER_ACCENT;

    visuals.widgets.active.bg_fill = BTN_PRIMARY_BG;
    visuals.widgets.active.fg_stroke.color = ACCENT_BLUE;
    visuals.widgets.active.bg_stroke.color = BTN_PRIMARY_BORDER;

    ctx.set_style(style);
}
