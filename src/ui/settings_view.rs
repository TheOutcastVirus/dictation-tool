use super::theme;
use crate::autostart;
use crate::state::AppState;
use gpui::{div, prelude::*, Context, Entity, Window};

pub struct SettingsView {
    state: Entity<AppState>,
}

impl SettingsView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        SettingsView { state }
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (models, current, status, autostart_on, models_dir) = {
            let s = self.state.read(cx);
            (
                s.available_models.clone(),
                s.current_model.clone(),
                s.model_status.clone(),
                s.autostart_enabled,
                s.models_dir.display().to_string(),
            )
        };

        let model_rows = models.into_iter().enumerate().map(|(i, name)| {
            let active = name == current;
            let name_for_click = name.clone();
            div()
                .id(("model", i))
                .flex()
                .items_center()
                .gap_3()
                .px_3()
                .py_2()
                .rounded_md()
                .cursor_pointer()
                .bg(if active { theme::surface_hi() } else { theme::surface() })
                .border_1()
                .border_color(if active { theme::accent() } else { theme::border() })
                .hover(|s| s.bg(theme::hover()))
                .child(
                    div()
                        .size_2()
                        .rounded_full()
                        .bg(if active { theme::accent() } else { theme::border() }),
                )
                .child(name)
                .when(active, |el| {
                    el.child(div().text_xs().text_color(theme::muted()).child("active"))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    let name = name_for_click.clone();
                    this.state.update(cx, |s, cx| {
                        s.request_model(&name);
                        cx.notify();
                    });
                }))
        });

        div()
            .id("settings")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p_4()
            .gap_6()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(theme::section_title("Model"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::muted())
                            .child(format!("Whisper models in {models_dir}. Selecting one reloads it immediately.")),
                    )
                    .children(model_rows)
                    .when_some(status, |el, status| {
                        let color = if status.starts_with("Failed") {
                            theme::red()
                        } else if status.starts_with("Loading") {
                            theme::yellow()
                        } else {
                            theme::green()
                        };
                        el.child(div().text_xs().text_color(color).child(status))
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(theme::section_title("Startup"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                theme::button(
                                    "autostart",
                                    if autostart_on { "Run at login: On" } else { "Run at login: Off" },
                                    autostart_on,
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let target = !autostart_on;
                                    if autostart::set_enabled(target) {
                                        this.state.update(cx, |s, cx| {
                                            s.autostart_enabled = target;
                                            cx.notify();
                                        });
                                    }
                                })),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme::muted())
                                    .child("Manages the dictation-tool.service systemd user unit."),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_xs()
                    .text_color(theme::muted())
                    .child(theme::section_title("Paths"))
                    .child(format!("Log: {}", crate::logger::log_path().display()))
                    .child(format!("Config: {}", crate::config::config_path().display())),
            )
    }
}
