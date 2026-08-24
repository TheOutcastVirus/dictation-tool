//! Zed-like dark palette and a couple of shared building blocks.

use gpui::{div, prelude::*, rgb, Div, ElementId, Rgba, SharedString, Stateful};

pub fn bg() -> Rgba {
    rgb(0x1b1d24)
}
pub fn surface() -> Rgba {
    rgb(0x23262f)
}
pub fn surface_hi() -> Rgba {
    rgb(0x2c303b)
}
pub fn hover() -> Rgba {
    rgb(0x343948)
}
pub fn border() -> Rgba {
    rgb(0x363a47)
}
pub fn text() -> Rgba {
    rgb(0xd7d9e0)
}
pub fn muted() -> Rgba {
    rgb(0x8a8fa3)
}
pub fn accent() -> Rgba {
    rgb(0x5b9cf5)
}
pub fn green() -> Rgba {
    rgb(0x4fc38a)
}
pub fn red() -> Rgba {
    rgb(0xe06c75)
}
pub fn yellow() -> Rgba {
    rgb(0xe5c07b)
}

/// Small pill button. Caller attaches `.on_click(...)`.
pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, primary: bool) -> Stateful<Div> {
    let label: SharedString = label.into();
    div()
        .id(id)
        .flex_none()
        .px_2()
        .py_0p5()
        .rounded_md()
        .text_xs()
        .cursor_pointer()
        .border_1()
        .border_color(border())
        .bg(if primary { accent() } else { surface_hi() })
        .text_color(if primary { rgb(0xffffff) } else { text() })
        .hover(|s| s.bg(hover()))
        .active(|s| s.opacity(0.8))
        .child(label)
}

pub fn section_title(label: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_xs()
        .text_color(muted())
        .child(label.into().to_uppercase())
}
