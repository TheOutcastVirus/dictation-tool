use super::theme;
use crate::autostart;
use crate::state::AppState;
use gpui::{div, prelude::*, px, Context, Entity, FontWeight, Window};

pub struct SettingsView {
    state: Entity<AppState>,
}

impl SettingsView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        SettingsView { state }
    }
}

/// A section opens with its heading in the reading face and says what it is
/// underneath. No tracked-out kicker above it.
fn heading(label: &'static str, note: impl Into<gpui::SharedString>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .px_2()
        .child(
            div()
                .text_size(px(16.))
                .font_weight(FontWeight::BOLD)
                .text_color(theme::bone())
                .child(label),
        )
        .child(
            div()
                .text_size(px(13.))
                .text_color(theme::bone_faint())
                .child(note.into()),
        )
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
                s.models_dir.clone(),
            )
        };

        let model_rows = models.into_iter().enumerate().map(|(i, name)| {
            let active = name == current;
            let name_for_click = name.clone();
            let size = std::fs::metadata(models_dir.join(&name))
                .map(|m| format!("{:.1} GB", m.len() as f64 / 1e9))
                .unwrap_or_default();
            div()
                .id(("model", i))
                .flex()
                .items_center()
                .gap_3()
                .px_2()
                .h(px(34.))
                .rounded_md()
                .cursor_pointer()
                .when(active, |el| el.bg(theme::lift()))
                .hover(|s| s.bg(theme::panel()))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(14.))
                        .when(active, |el| el.font_weight(FontWeight::BOLD))
                        .text_color(if active { theme::bone() } else { theme::bone_dim() })
                        .child(super::model_label(&name)),
                )
                .child(
                    // Fixed column: a size never decides where "loaded"
                    // lands, however many digits it has.
                    div()
                        .flex()
                        .flex_none()
                        .justify_end()
                        .w(px(56.))
                        .font_family(theme::DATA)
                        .text_size(px(11.))
                        .text_color(theme::bone_faint())
                        .child(size),
                )
                .child(
                    div()
                        .w(px(52.))
                        .font_family(theme::DATA)
                        .text_size(px(11.))
                        .text_color(theme::bone_dim())
                        .child(if active { "loaded" } else { "" }),
                )
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
            .px_2()
            .py_4()
            .gap_6()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(heading(
                        "Model",
                        "Bigger models hear better and take longer. Switching reloads immediately.",
                    ))
                    .children(model_rows)
                    .when_some(status, |el, status| {
                        let color = if status.starts_with("Failed") {
                            theme::signal()
                        } else if status.starts_with("Loading") {
                            theme::working()
                        } else {
                            theme::bone_dim()
                        };
                        el.child(
                            div()
                                .px_2()
                                .pt_1()
                                .text_size(px(13.))
                                .text_color(color)
                                .child(status),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(heading(
                        "Run at login",
                        "Starts the daemon with your session, via a systemd user unit.",
                    ))
                    .child(
                        div().px_2().child(
                            switch("autostart", autostart_on).on_click(cx.listener(
                                move |this, _, _, cx| {
                                    let target = !autostart_on;
                                    if autostart::set_enabled(target) {
                                        this.state.update(cx, |s, cx| {
                                            s.autostart_enabled = target;
                                            cx.notify();
                                        });
                                    }
                                },
                            )),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(heading("Where things live", "Read-only, for when something breaks."))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .px_2()
                            .font_family(theme::DATA)
                            .text_size(px(11.))
                            .text_color(theme::bone_faint())
                            .child(crate::logger::log_path().display().to_string())
                            .child(crate::config::config_path().display().to_string())
                            .child(models_dir.display().to_string()),
                    ),
            )
    }
}

/// A plain two-state switch: a track, and a knob that sits at one end of it.
/// The knob is the same bone as the type, so the control belongs to the page
/// rather than lighting up inside it.
fn switch(id: &'static str, on: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_3()
        .cursor_pointer()
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .w(px(44.))
                .h(px(24.))
                .px(px(3.))
                .rounded_full()
                .bg(if on { theme::bone_dim() } else { theme::lift() })
                .when(!on, |el| el.justify_start())
                .when(on, |el| el.justify_end())
                .child(
                    div()
                        .size(px(18.))
                        .rounded_full()
                        .bg(if on { theme::ink() } else { theme::bone_faint() }),
                ),
        )
        .child(
            div()
                .text_size(px(14.))
                .text_color(if on { theme::bone() } else { theme::bone_dim() })
                .child(if on { "On" } else { "Off" }),
        )
}
