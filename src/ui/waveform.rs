//! The trace: a mirrored ribbon painted from the engine's real RMS stream.
//!
//! Live while you hold the key, and at rest it holds the envelope of the
//! last thing you said, so the band is never a decoration -- it is always
//! showing actual audio. The same painter draws the band in the main window
//! and the ribbon in the floating bubble, so both windows speak one language.

use super::theme;
use gpui::{canvas, prelude::*, px, Bounds, Path, Pixels, Rgba};

/// Breathing room kept between the ribbon's peak and the band edge, so a
/// loud passage never touches the boundary. Proportional, because the same
/// painter draws an 84 px band and a 22 px bubble ribbon.
const V_PAD_MAX: f32 = 6.0;
const V_PAD_RATIO: f32 = 0.12;
/// Half-thickness of the resting line, so silence still reads as a threaded
/// tape rather than an empty box.
const REST: f32 = 0.75;
/// Horizontal spacing between sample columns.
const STEP: f32 = 2.5;
/// Fraction of the width over which the resting baseline tapers to a point.
const TAPER: f32 = 0.06;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Trace {
    /// Audio arriving right now.
    Live,
    /// The envelope of the utterance just captured, held while it is
    /// transcribed and after.
    Held,
}

impl Trace {
    fn color(self) -> Rgba {
        match self {
            Trace::Live => theme::signal(),
            Trace::Held => theme::bone_faint(),
        }
    }
}

/// Speech RMS sits roughly in 0.01..0.3; the square root spreads the quiet
/// end out so ordinary talking uses most of the band.
fn amplitude(rms: f32) -> f32 {
    (rms * 6.0).sqrt().clamp(0.0, 1.0)
}

/// Lays `levels` (oldest first) out across `count` columns.
///
/// A live trace is a window onto *now*: the right edge is the present, so
/// samples run at their true rate and a short recording fills only the
/// right-hand end, growing leftward as you keep talking. A held trace is a
/// finished utterance, and the whole utterance is the subject, so it is
/// resampled to fill the band edge to edge.
fn columns(levels: &[f32], count: usize, trace: Trace) -> Vec<f32> {
    if count == 0 {
        return Vec::new();
    }
    let mut out = vec![0.0; count];
    if levels.is_empty() {
        return out;
    }

    if trace == Trace::Held || levels.len() >= count {
        // Resample the whole span onto the columns. When there are more
        // samples than columns each column takes the loudest sample it
        // covers, so a peak is never skipped over.
        let span = levels.len() as f32 / count as f32;
        for (i, slot) in out.iter_mut().enumerate() {
            let start = (i as f32 * span).floor() as usize;
            let end = (((i + 1) as f32 * span).ceil() as usize).clamp(start + 1, levels.len());
            let peak = levels[start..end].iter().copied().fold(0.0_f32, f32::max);
            *slot = amplitude(peak);
        }
    } else {
        // Live and still filling: right-align at the true sample rate.
        let offset = count - levels.len();
        for (i, level) in levels.iter().enumerate() {
            out[offset + i] = amplitude(*level);
        }
    }
    out
}

/// Eases the *resting* thickness to nothing at both ends, so silence draws
/// as a tapered spindle rather than a hairline rule across the page. Only
/// the baseline is shaped -- measured amplitude is never touched, so the
/// trace still tells the truth about the audio.
fn taper(t: f32) -> f32 {
    let edge = (t.min(1.0 - t) / TAPER).clamp(0.0, 1.0);
    edge * edge * (3.0 - 2.0 * edge)
}

/// Rounds the column heights off so the outline reads as a drawn ribbon
/// rather than a picket fence. This shapes how the envelope is *drawn*
/// between measured points, the way any line chart interpolates; it never
/// invents or rescales a sample.
fn smooth(values: &mut [f32]) {
    if values.len() < 3 {
        return;
    }
    let source = values.to_vec();
    for i in 1..source.len() - 1 {
        values[i] = source[i - 1] * 0.25 + source[i] * 0.5 + source[i + 1] * 0.25;
    }
}

/// Builds the ribbon as an explicit triangle strip between adjacent columns.
///
/// `Path::line_to` fans every segment back to the path's first point, which
/// only fills correctly for star-shaped outlines -- a mirrored waveform is
/// not one, and filling it that way produces a solid wedge. Emitting the
/// triangles directly is exact for any outline.
fn ribbon(bounds: Bounds<Pixels>, values: &[f32]) -> Option<Path<Pixels>> {
    if values.len() < 2 {
        return None;
    }
    let left = f32::from(bounds.origin.x);
    let width = f32::from(bounds.size.width);
    // The ribbon is mirrored around the exact vertical centre of the band.
    let mid = f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.0;
    let height = f32::from(bounds.size.height);
    let pad = (height * V_PAD_RATIO).min(V_PAD_MAX);
    let reach = (height / 2.0 - pad).max(REST);
    let last = (values.len() - 1) as f32;
    let dx = width / last;

    let at = |i: usize, sign: f32| {
        let rest = REST * taper(i as f32 / last);
        let half = (rest + values[i] * (reach - REST)).min(reach);
        gpui::point(px(left + dx * i as f32), px(mid + sign * half))
    };
    // st (0, 1) marks a vertex as solid interior rather than part of a
    // quadratic edge; it is what gpui's own straight segments use.
    let solid = gpui::point(0., 1.);

    let mut path = Path::new(at(0, -1.0));
    for i in 0..values.len() - 1 {
        let (top_l, bot_l) = (at(i, -1.0), at(i, 1.0));
        let (top_r, bot_r) = (at(i + 1, -1.0), at(i + 1, 1.0));
        path.push_triangle((top_l, bot_l, top_r), (solid, solid, solid));
        path.push_triangle((bot_l, bot_r, top_r), (solid, solid, solid));
    }

    Some(path)
}

/// The ribbon, sized by its parent. `levels` is oldest-first.
pub fn render(levels: Vec<f32>, trace: Trace) -> impl IntoElement {
    canvas(
        move |_, _, _| levels,
        move |bounds, levels, window, _| {
            let count = (f32::from(bounds.size.width) / STEP).floor() as usize;
            let mut values = columns(&levels, count.max(2), trace);
            smooth(&mut values);
            if let Some(path) = ribbon(bounds, &values) {
                window.paint_path(path, trace.color());
            }
            // A playhead at the live edge: the newest sample is "now".
            if trace == Trace::Live {
                let tick = Bounds {
                    origin: gpui::point(
                        bounds.origin.x + bounds.size.width - px(1.),
                        bounds.origin.y,
                    ),
                    size: gpui::size(px(1.), bounds.size.height),
                };
                window.paint_quad(gpui::fill(tick, theme::signal()));
            }
        },
    )
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_traces_fill_the_band_and_live_ones_grow_from_the_right() {
        // A finished utterance is the subject, so it spans the whole band.
        let held = columns(&[0.3, 0.3, 0.3], 12, Trace::Held);
        assert!(held.iter().all(|v| *v > 0.0), "held trace left gaps: {held:?}");

        // A live one is a window onto now: it fills from the right as you talk.
        let live = columns(&[0.3, 0.3, 0.3], 12, Trace::Live);
        assert!(live[..9].iter().all(|v| *v == 0.0), "live trace was stretched");
        assert!(live[9..].iter().all(|v| *v > 0.0), "live trace lost its tail");
    }

    #[test]
    fn downsampling_keeps_peaks() {
        // A single loud sample among quiet ones must survive being squeezed
        // into fewer columns -- averaging would swallow a shout.
        let mut levels = vec![0.001; 64];
        levels[30] = 0.4;
        let out = columns(&levels, 8, Trace::Held);
        let loudest = out.iter().copied().fold(0.0_f32, f32::max);
        assert!(loudest > 0.9, "peak was lost in resampling: {out:?}");
    }

    #[test]
    fn the_resting_baseline_tapers_to_nothing_at_both_ends() {
        assert_eq!(taper(0.0), 0.0);
        assert_eq!(taper(1.0), 0.0);
        assert_eq!(taper(0.5), 1.0);
        assert!(taper(TAPER / 2.0) > 0.0 && taper(TAPER / 2.0) < 1.0);
    }

    #[test]
    fn amplitude_is_bounded_even_when_the_microphone_clips() {
        assert_eq!(amplitude(0.0), 0.0);
        assert_eq!(amplitude(10.0), 1.0);
        assert!(amplitude(0.01) < amplitude(0.1));
    }

    #[test]
    fn smoothing_preserves_the_ends_and_the_overall_level() {
        let mut values = vec![0.0, 1.0, 0.0, 1.0, 0.0];
        let before = values.clone();
        smooth(&mut values);
        assert_eq!(values[0], before[0]);
        assert_eq!(values[4], before[4]);
        assert!(values.iter().all(|v| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn an_empty_trace_still_produces_a_full_row_of_columns() {
        assert_eq!(columns(&[], 16, Trace::Held).len(), 16);
        assert_eq!(columns(&[], 16, Trace::Live), vec![0.0; 16]);
    }
}
