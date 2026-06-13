use gpui::{
    App, Application, Bounds, Context, Render, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};
use gpui_platform::application;

struct HelloView;

impl Render for HelloView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x0a0c10))
            .flex()
            .justify_center()
            .items_center()
            .text_color(rgb(0xdbe2ec))
            .text_size(px(16.0))
            .child("AnyRouter — 中文测试 你好世界")
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| HelloView),
        )
        .unwrap();
        cx.activate(true);
    });
}
