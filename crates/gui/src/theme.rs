#![allow(dead_code)]

//! AnyRouter 暗色主题色板（GPUI 版本）
//!
//! 所有函数返回 `Hsla`，可直接传给 `.bg()`/`.text_color()`/`.border_color()` 等 GPUI 样式方法。

use gpui::{Hsla, Rgba, hsla, rgb, rgba};

// ─── 窗口 & 容器底色 ───────────────────────────────────────────
pub fn bg_window() -> Hsla {
    rgb(0x0a0c10).into()
}
pub fn bg_card() -> Hsla {
    rgb(0x141923).into()
}
pub fn bg_bar() -> Hsla {
    rgb(0x11141a).into()
}
pub fn bg_stats_bar() -> Hsla {
    rgb(0x0e1117).into()
}

// ─── 边框 ──────────────────────────────────────────────────────
pub fn border_normal() -> Hsla {
    rgb(0x2a2f3a).into()
}
pub fn border_accent() -> Hsla {
    rgb(0x3a4150).into()
}

// ─── 文字色阶 ─────────────────────────────────────────────────
pub fn text_primary() -> Hsla {
    rgb(0xdbe2ec).into()
}
pub fn text_secondary() -> Hsla {
    rgb(0xaab2c0).into()
}
pub fn text_muted() -> Hsla {
    rgb(0x8a93a5).into()
}
pub fn text_weakest() -> Hsla {
    rgb(0x56657d).into()
}

// ─── 语义色 ───────────────────────────────────────────────────
pub fn accent_blue() -> Hsla {
    rgb(0x7fb0ff).into()
}
pub fn success_green() -> Hsla {
    rgb(0x6fbf73).into()
}
pub fn error_red() -> Hsla {
    rgb(0xe07b7b).into()
}
pub fn warning_yellow() -> Hsla {
    rgb(0xdcaa50).into()
}

// ─── 按钮主色 (半透明) ────────────────────────────────────────
pub fn btn_primary_bg() -> Hsla {
    Rgba {
        r: 80.0 / 255.0,
        g: 140.0 / 255.0,
        b: 255.0 / 255.0,
        a: 0.20,
    }
    .into()
}
pub fn btn_primary_border() -> Hsla {
    Rgba {
        r: 80.0 / 255.0,
        g: 140.0 / 255.0,
        b: 255.0 / 255.0,
        a: 0.50,
    }
    .into()
}

// ─── 按钮危险色 (半透明) ──────────────────────────────────────
pub fn btn_danger_bg() -> Hsla {
    Rgba {
        r: 220.0 / 255.0,
        g: 110.0 / 255.0,
        b: 110.0 / 255.0,
        a: 0.12,
    }
    .into()
}
pub fn btn_danger_border() -> Hsla {
    Rgba {
        r: 220.0 / 255.0,
        g: 110.0 / 255.0,
        b: 110.0 / 255.0,
        a: 0.45,
    }
    .into()
}

// ─── 透明色 ───────────────────────────────────────────────────
pub fn transparent() -> Hsla {
    hsla(0.0, 0.0, 0.0, 0.0)
}

/// 半透明黑色遮罩（弹窗背景）
pub fn overlay() -> Hsla {
    rgba(0x00000099).into()
}
