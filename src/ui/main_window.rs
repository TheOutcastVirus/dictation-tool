use super::{history_view::HistoryView, settings_view::SettingsView, status_bar, theme};
use crate::state::AppState;
use gpui::{div, prelude::*, px, Context, Entity, SharedString, Window};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    History,
    Settings,
}

pub struct MainWindow {
    state: Entity<AppState>,
    tab: Tab,
    history: Entity<HistoryView>,
    settings: Entity<SettingsView>,
}

impl MainWindow {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let history = cx.new(|cx| HistoryView::new(state.clone(), cx));
        let settings = cx.new(|cx| SettingsView::new(state.clone(), cx));
        MainWindow {
            state,
            tab: Tab::History,
            history,
            settings,
        }
    }

    fn tab_button(&self, tab: Tab, label: &'static str, cx: &Context<Self>) -> impl IntoElement {
        let active = self.tab == tab;
        div()
            .id(SharedString::from(label))
            .px_3()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .text_color(if active { theme::text() } else { theme::muted() })
            .when(active, |el| el.bg(theme::surface_hi()))
            .hover(|s| s.bg(theme::hover()))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.tab = tab;
                cx.notify();
            }))
    }
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match self.tab {
            Tab::History => self.history.clone().into_any_element(),
            Tab::Settings => self.settings.clone().into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::bg())
            .text_color(theme::text())
            .text_sm()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .h(px(38.))
                    .px_2()
                    .gap_1()
                    .border_b_1()
                    .border_color(theme::border())
                    .bg(theme::surface())
                    .child(self.tab_button(Tab::History, "History", cx))
                    .child(self.tab_button(Tab::Settings, "Settings", cx))
                    .child(div().flex_1())
                    .child(div().text_xs().text_color(theme::muted()).child("Dictation")),
            )
            .child(div().flex_1().min_h_0().child(content))
            .child(status_bar::render(self.state.read(cx)))
    }
}
