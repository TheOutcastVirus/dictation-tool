//! The recorder's desk.
//!
//! Warm ink, bone type, and one signal colour: the record light. Everything
//! else is separated by value alone, so the only thing on screen that is
//! coloured is the thing that is actually happening.

use gpui::{rgb, Rgba};

/// Berenis ADF Pro (Arkandis Digital Foundry). Carries everything human:
/// transcripts, headings, controls. A reading serif, because the content of
/// this app is prose someone spoke.
pub const DISPLAY: &str = "Berenis ADF Pro";

/// Go Mono (Bigelow & Holmes). Carries machine data only -- timestamps,
/// durations, milliseconds, byte counts, filenames. Never running text.
pub const DATA: &str = "Go Mono";

pub fn ink() -> Rgba {
    rgb(0x16130f)
}
pub fn panel() -> Rgba {
    rgb(0x1d1913)
}
pub fn lift() -> Rgba {
    rgb(0x262019)
}
/// A stroke set near the surface's own colour: an edge you feel rather than
/// a line you read. Used sparingly, never as a box around everything.
pub fn edge() -> Rgba {
    rgb(0x2f2820)
}
pub fn bone() -> Rgba {
    rgb(0xede4d6)
}
pub fn bone_dim() -> Rgba {
    rgb(0xa79b89)
}
/// Tertiary. Measured at 5.3:1 on `ink`, 5.0:1 on `panel` and 4.6:1 on
/// `lift` -- it has to clear all three, because a selected row uses `lift`
/// behind data set in this colour.
pub fn bone_faint() -> Rgba {
    rgb(0x948976)
}
/// The record light. The only saturated colour in the app.
pub fn signal() -> Rgba {
    rgb(0xe2603c)
}
/// Work in progress: analogous to `signal`, so the two harmonise instead of
/// competing. Loading a model, transcribing.
pub fn working() -> Rgba {
    rgb(0xb8894a)
}
