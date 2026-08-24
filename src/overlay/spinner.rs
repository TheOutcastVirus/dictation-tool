use crate::ui::theme;
use gpui::{div, prelude::*, px, Animation, AnimationExt};
use std::time::Duration;

/// Three dots pulsing in sequence.
pub fn render() -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(4.))
        .children((0..3usize).map(|i| {
            div()
                .size(px(7.))
                .rounded_full()
                .bg(theme::accent())
                .with_animation(
                    ("spinner-dot", i),
                    Animation::new(Duration::from_millis(900)).repeat(),
                    move |dot, delta| {
                        let phase = (delta + i as f32 / 3.0) % 1.0;
                        let pulse = 1.0 - (phase * 2.0 - 1.0).abs();
                        dot.opacity(0.3 + 0.7 * pulse)
                    },
                )
        }))
}
