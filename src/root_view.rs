use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::Disableable;

use crate::app_state::{AppState, ActivePanel, LogLevel};
use crate::theme::Glass;
use crate::account_panel::AccountPanel;
use crate::log_panel::LogPanel;
use crate::dashboard::Dashboard;
use crate::service;

/// 侧边栏导航项
#[derive(Debug, Clone, Copy, PartialEq)]
enum NavItem {
    Accounts,
    Logs,
    Dashboard,
}

impl NavItem {
    fn label(&self) -> &'static str {
        match self {
            NavItem::Accounts => "Accounts",
            NavItem::Logs => "Logs",
            NavItem::Dashboard => "Dashboard",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            NavItem::Accounts => "Users",
            NavItem::Logs => "Terminal",
            NavItem::Dashboard => "BarChart",
        }
    }

    fn to_panel(&self) -> ActivePanel {
        match self {
            NavItem::Accounts => ActivePanel::Accounts,
            NavItem::Logs => ActivePanel::Logs,
            NavItem::Dashboard => ActivePanel::Dashboard,
        }
    }
}

/// 根视图
pub struct RootView {
    app_state: Entity<AppState>,
    account_panel: Entity<AccountPanel>,
    log_panel: Entity<LogPanel>,
    dashboard: Entity<Dashboard>,
}

impl RootView {
    pub fn new(app_state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let account_panel = cx.new(|cx| AccountPanel::new(app_state.clone(), cx));
        let log_panel = cx.new(|_| LogPanel::new(app_state.clone()));
        let dashboard = cx.new(|_| Dashboard::new(app_state.clone()));

        cx.observe(&app_state, |_, _, cx| cx.notify()).detach();

        Self {
            app_state,
            account_panel,
            log_panel,
            dashboard,
        }
    }

    fn switch_panel(&mut self, panel: ActivePanel, cx: &mut Context<Self>) {
        self.app_state.update(cx, |state, cx| {
            state.active_panel = panel;
            cx.notify();
        });
    }

    fn trigger_checkin(&mut self, cx: &mut Context<Self>) {
        let weak = self.app_state.downgrade();
        service::run_checkin_all(weak, cx);
    }

    fn render_nav_item(
        &self,
        item: NavItem,
        active_panel: ActivePanel,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = active_panel == item.to_panel();
        let panel = item.to_panel();

        div()
            .id(SharedString::from(format!("nav-{}", item.label())))
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .rounded(px(8.0))
            .cursor_pointer()
            .bg(if is_active { Glass::card() } else { Glass::transparent() })
            .border_1()
            .border_color(if is_active { Glass::border_bright() } else { Glass::transparent() })
            .text_color(if is_active { Glass::primary() } else { Glass::text_secondary() })
            .hover(|s| s.bg(Glass::card_hover()))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.switch_panel(panel, cx);
            }))
            .child(
                div().text_sm().font_weight(FontWeight::MEDIUM)
                    .child(item.label())
            )
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_panel = self.app_state.read(cx).active_panel;
        let is_running = self.app_state.read(cx).is_running;
        let success_count = self.app_state.read(cx).success_count;
        let fail_count = self.app_state.read(cx).fail_count;
        let total = self.app_state.read(cx).total_accounts();

        let content = match active_panel {
            ActivePanel::Accounts => self.account_panel.clone().into_any_element(),
            ActivePanel::Logs => self.log_panel.clone().into_any_element(),
            ActivePanel::Dashboard => self.dashboard.clone().into_any_element(),
        };

        div()
            .flex()
            .size_full()
            .bg(hsla(0.0, 0.0, 0.04, 1.0))
            // 侧边栏
            .child(
                div()
                    .w(px(200.0))
                    .h_full()
                    .flex()
                    .flex_col()
                    .bg(Glass::sidebar())
                    .border_r_1()
                    .border_color(Glass::border())
                    .py_4()
                    .px_3()
                    .gap_1()
                    // 应用标题
                    .child(
                        div()
                            .px_3()
                            .pb_4()
                            .mb_2()
                            .border_b_1()
                            .border_color(Glass::border())
                            .child(
                                div().text_base().font_weight(FontWeight::BOLD)
                                    .text_color(Glass::primary())
                                    .child("AnyRouter")
                            )
                            .child(
                                div().text_xs().mt_1()
                                    .text_color(Glass::text_muted())
                                    .child("Auto Check-in")
                            )
                    )
                    // 导航项
                    .child(self.render_nav_item(NavItem::Accounts, active_panel, cx))
                    .child(self.render_nav_item(NavItem::Logs, active_panel, cx))
                    .child(self.render_nav_item(NavItem::Dashboard, active_panel, cx))
                    // 弹性空白
                    .child(div().flex_grow())
                    // 底部状态
                    .child(
                        div()
                            .px_3()
                            .pt_3()
                            .border_t_1()
                            .border_color(Glass::border())
                            .child(
                                div().text_xs()
                                    .text_color(Glass::text_muted())
                                    .child(format!("v0.1.0 | {} accounts", total))
                            )
                    )
            )
            // 内容区域
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    // 顶部状态栏
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px_6()
                            .py_3()
                            .bg(Glass::panel())
                            .border_b_1()
                            .border_color(Glass::border())
                            // 左侧: 状态信息
                            .child(
                                div().flex().items_center().gap_3()
                                    .child(
                                        div().text_sm().text_color(Glass::text_secondary())
                                            .child(if is_running { "Running..." } else { "Ready" })
                                    )
                                    .child(
                                        div().text_xs().text_color(Glass::text_muted())
                                            .child(format!("Success: {} | Failed: {}", success_count, fail_count))
                                    )
                            )
                            // 右侧: 签到按钮
                            .child(
                                Button::new("checkin-all")
                                    .primary()
                                    .label(if is_running { "Running..." } else { "Check-in All" })
                                    .disabled(is_running)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.trigger_checkin(cx);
                                    }))
                            )
                    )
                    // 面板内容
                    .child(
                        div().flex_1().overflow_y_scroll().p_6()
                            .child(content)
                    )
            )
    }
}
