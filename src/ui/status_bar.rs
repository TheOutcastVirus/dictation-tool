use super::theme;
use crate::state::{AppState, EngineState};
use gpui::{div, prelude::*, px};

pub fn render(state: &AppState) -> impl IntoElement {
    let (dot, label) = if !state.engine_online {
        (theme::red(), "Engine offline")
    } else {
        match state.engine_state {
            EngineState::Idle => (theme::green(), "Idle -- hold Right Alt to dictate"),
            EngineState::Recording => (theme::red(), "Recording"),
            EngineState::Processing => (theme::yellow(), "Transcribing"),
        }
    };

    let vram = state
        .vram
        .map(|v| {
            format!(
                "VRAM {:.1} / {:.1} GiB",
                v.used_mib as f64 / 1024.0,
                v.total_mib as f64 / 1024.0
            )
        })
        .unwrap_or_else(|| "VRAM n/a".to_string());

    div()
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .h(px(28.))
        .flex_none()
        .border_t_1()
        .border_color(theme::border())
        .bg(theme::surface())
        .text_xs()
        .text_color(theme::muted())
        .child(div().size(px(8.)).rounded_full().bg(dot))
        .child(label)
        .when_some(state.last_error.clone(), |el, err| {
            el.child(div().text_color(theme::red()).truncate().child(err))
        })
        .child(div().flex_1())
        .child(format!("Model: {}", state.current_model))
        .child(vram)
}
