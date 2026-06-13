use std::time::Duration;

use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::Disableable;
use gpui_component::{Placement, Root, TitleBar, WindowExt};

use crate::account_detail::AccountDetail;
use crate::app_state::{AppState, AppView};
use crate::home_view::HomeView;
use crate::log_panel::LogPanel;
use crate::service::{self, CheckinScope};
use crate::theme::Glass;

/// 根视图:自定义标题栏 + 整页视图(主页/详情)+ 底部栏 + 日志抽屉 + 弹窗层。
pub struct RootView {
    app_state: Entity<AppState>,
    home_view: Entity<HomeView>,
    account_detail: Entity<AccountDetail>,
    log_panel: Entity<LogPanel>,
}

impl RootView {
    pub fn new(app_state: Entity<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let home_view = cx.new(|_| HomeView::new(app_state.clone()));
        let account_detail = cx.new(|_| AccountDetail::new(app_state.clone()));
        let log_panel = cx.new(|_| LogPanel::new(app_state.clone()));
        cx.observe(&app_state, |_, _, cx| cx.notify()).detach();
        Self { app_state, home_view, account_detail, log_panel }
    }

    fn trigger_checkin_all(&mut self, cx: &mut Context<Self>) {
        let weak = self.app_state.downgrade();
        service::run_checkin(weak, CheckinScope::All, cx);
    }

    fn toggle_drawer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_sheet(cx) {
            window.close_sheet(cx);
        } else {
            let log_panel = self.log_panel.clone();
            window.open_sheet_at(Placement::Bottom, cx, move |sheet, _window, _cx| {
                sheet
                    .title("运行日志")
                    .size(px(300.0))
                    .overlay(false)
                    .resizable(false)
                    .child(log_panel.clone())
            });
        }
    }

    /// 顶部状态指示点:运行中为脉冲呼吸的橙色点,空闲为静态绿色点。
    fn render_status_dot(is_running: bool) -> AnyElement {
        let color = if is_running { Glass::warning() } else { Glass::success() };
        let dot = div().w(px(8.0)).h(px(8.0)).rounded(px(4.0)).bg(color);
        if is_running {
            dot.with_animation(
                "status-pulse",
                Animation::new(Duration::from_secs_f64(1.2)).repeat(),
                move |el, delta| el.opacity(0.35 + 0.65 * (delta * std::f32::consts::PI).sin()),
            )
            .into_any_element()
        } else {
            dot.into_any_element()
        }
    }

    /// 自定义标题栏:左侧品牌标识,右侧由 TitleBar 自动渲染窗口控制按钮。
    fn render_title_bar() -> impl IntoElement {
        TitleBar::new().child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(18.0))
                        .h(px(18.0))
                        .rounded(px(5.0))
                        .bg(Glass::primary())
                        .shadow(Glass::glow(Glass::primary()))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(hsla(0.0, 0.0, 1.0, 1.0))
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .child("A"),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(Glass::text())
                        .child("AnyRouter 自动签到"),
                ),
        )
    }

    /// 内容区顶部工具条:页面标题 + 一键签到全部。
    fn render_toolbar(&self, is_running: bool, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .px_6()
            .py_3()
            .bg(Glass::panel_gradient())
            .border_b_1()
            .border_color(Glass::border())
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::BOLD)
                    .text_color(Glass::text())
                    .child("站点与账户"),
            )
            .child(
                div()
                    .rounded(px(8.0))
                    .shadow(if is_running { Vec::new() } else { Glass::glow(Glass::primary()) })
                    .child(
                        Button::new("checkin-all")
                            .primary()
                            .label(if is_running { "签到中..." } else { "一键签到全部" })
                            .disabled(is_running)
                            .on_click(cx.listener(|this, _, _, cx| this.trigger_checkin_all(cx))),
                    ),
            )
    }

    /// 底部栏:运行状态 + 计数 + 日志抽屉开关。
    fn render_bottom_bar(
        &self,
        is_running: bool,
        success: usize,
        fail: usize,
        drawer_open: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .py_2()
            .bg(Glass::panel())
            .border_t_1()
            .border_color(Glass::border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(Self::render_status_dot(is_running))
                    .child(
                        div()
                            .text_sm()
                            .text_color(Glass::text_secondary())
                            .child(if is_running { "运行中..." } else { "空闲" }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(Glass::text_muted())
                            .child(format!("成功 {} | 失败 {}", success, fail)),
                    ),
            )
            .child(
                Button::new("toggle-logs")
                    .ghost()
                    .label(if drawer_open { "▾ 日志" } else { "▤ 日志" })
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_drawer(window, cx))),
            )
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (is_running, success, fail, view) = {
            let state = self.app_state.read(cx);
            (state.is_running, state.success_count, state.fail_count, state.view)
        };
        let drawer_open = window.has_active_sheet(cx);

        let toolbar = matches!(view, AppView::Home)
            .then(|| self.render_toolbar(is_running, cx).into_any_element());

        let content = match view {
            AppView::Home => self.home_view.clone().into_any_element(),
            AppView::AccountDetail(_id) => self.account_detail.clone().into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(hsla(0.0, 0.0, 0.04, 1.0))
            .child(Self::render_title_bar())
            .children(toolbar)
            // 主内容滚动区
            .child(
                div()
                    .id("content-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_6()
                    .child(content),
            )
            .child(self.render_bottom_bar(is_running, success, fail, drawer_open, cx))
            // 通知 / 弹窗 / 抽屉层(必须由根视图渲染,否则 Dialog/Sheet 不显示)
            .children(Root::render_notification_layer(window, cx))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
    }
}
