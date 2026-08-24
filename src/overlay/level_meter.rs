use crate::ui::theme;
use gpui::{div, prelude::*, px};
use std::collections::VecDeque;

const BARS: usize = 20;

/// Amplitude-only bar meter: newest sample on the right.
pub fn render(levels: &VecDeque<f32>) -> impl IntoElement {
    let mut values: Vec<f32> = vec![0.0; BARS.saturating_sub(levels.len())];
    values.extend(levels.iter().rev().take(BARS).rev().copied());

    div()
        .flex()
        .items_center()
        .gap(px(2.))
        .h(px(24.))
        .children(values.into_iter().map(|rms| {
            // Speech RMS sits roughly in 0.01..0.3; sqrt spreads the low end out.
            let t = (rms * 6.0).sqrt().min(1.0);
            div()
                .w(px(3.))
                .h(px(4.0 + 20.0 * t))
                .rounded_full()
                .bg(if t > 0.85 { theme::red() } else { theme::accent() })
        }))
}
