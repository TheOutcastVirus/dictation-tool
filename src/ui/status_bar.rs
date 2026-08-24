use super::theme;
use crate::state::{AppState, EngineState};
use gpui::{div, prelude::*, px, Rgba};

/// The record light: a filled lamp while something is happening, a hollow
/// ring at rest. The state you can read across the room.
fn lamp(color: Rgba, filled: bool) -> impl IntoElement {
    div()
        .flex_none()
        .size(px(9.))
        .rounded_full()
        .when(filled, |el| el.bg(color))
        .when(!filled, |el| el.border_1().border_color(color))
}

pub fn render(state: &AppState) -> impl IntoElement {
    let (color, filled, label) = if !state.engine_online {
        (theme::signal(), true, "Engine offline")
    } else {
        match state.engine_state {
            EngineState::Idle => (theme::bone_faint(), false, "Idle"),
            EngineState::Recording => (theme::signal(), true, "Listening"),
            EngineState::Processing => (theme::working(), true, "Transcribing"),
        }
    };

    div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .h(px(30.))
        .flex_none()
        .bg(theme::panel())
        .child(lamp(color, filled))
        .child(
            div()
                .flex_none()
                .text_size(px(13.))
                .text_color(theme::bone_dim())
                .child(label),
        )
        .when_some(state.last_error.clone(), |el, err| {
            el.child(
                div()
                    .min_w_0()
                    .text_size(px(13.))
                    .text_color(theme::signal())
                    .truncate()
                    .child(err),
            )
        })
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .flex_none()
                .gap_4()
                .font_family(theme::DATA)
                .text_size(px(11.))
                .text_color(theme::bone_faint())
                .child(super::model_label(&state.current_model))
                .child(match state.vram {
                    // How much of the card this process is holding: the
                    // weights plus whisper's compute buffers.
                    Some(v) if v.total_mib > 0 => format!(
                        "{:.2} of {:.0} GiB VRAM",
                        v.model_mib as f64 / 1024.0,
                        v.total_mib as f64 / 1024.0
                    ),
                    _ => "VRAM n/a".to_string(),
                }),
        )
}
