//! Floating "recording / transcribing" bubble, top-center of the primary
//! display. Opened on Idle -> Recording, closed on -> Idle (which the engine
//! only signals after the text has been typed).

pub mod level_meter;
pub mod spinner;

use crate::state::{AppState, EngineState};
use crate::ui::{theme, Windows};
use gpui::{
    div, point, prelude::*, px, size, App, Bounds, Context, Entity, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions,
};

const WIDTH: f32 = 220.0;
const HEIGHT: f32 = 56.0;
const TOP_MARGIN: f32 = 48.0;

pub struct OverlayView {
    state: Entity<AppState>,
}

impl OverlayView {
    fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        OverlayView { state }
    }
}

impl Render for OverlayView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let content = match state.engine_state {
            EngineState::Recording => div()
                .flex()
                .items_center()
                .gap_3()
                .child(level_meter::render(&state.level_history))
                .child("Listening"),
            EngineState::Processing => div()
                .flex()
                .items_center()
                .gap_3()
                .child(spinner::render())
                .child("Transcribing"),
            EngineState::Idle => div(),
        };

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px_4()
                    .h(px(44.))
                    .min_w(px(200.))
                    .rounded_full()
                    .bg(theme::bg())
                    .border_1()
                    .border_color(theme::border())
                    .text_sm()
                    .text_color(theme::text())
                    .child(content),
            )
    }
}

/// Reconciles the overlay window with the engine state. Call after every
/// batch of engine events.
pub fn sync(state: &Entity<AppState>, cx: &mut App) {
    let want_open = state.read(cx).engine_state != EngineState::Idle;
    let existing = cx.global::<Windows>().overlay;
    let is_open = existing.map(|h| h.is_active(cx).is_some()).unwrap_or(false);

    if want_open && !is_open {
        let handle = open(state.clone(), cx);
        cx.global_mut::<Windows>().overlay = handle;
    } else if !want_open && is_open {
        if let Some(handle) = existing {
            handle
                .update(cx, |_, window, _| window.remove_window())
                .ok();
        }
        cx.global_mut::<Windows>().overlay = None;
    }
}

fn open(state: Entity<AppState>, cx: &mut App) -> Option<gpui::WindowHandle<OverlayView>> {
    let display = cx.primary_display();
    let screen = display
        .as_ref()
        .map(|d| d.bounds())
        .unwrap_or_else(|| Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(1920.), px(1080.)),
        });
    // GPUI's X11 backend reports the whole root screen as one display, so
    // on a multi-monitor X11 setup "top-center of the display" can land in
    // the dead space between monitors. Prefer the RandR primary output.
    let monitor = primary_monitor_via_xrandr(&screen).unwrap_or(screen);
    let bounds = Bounds {
        origin: point(
            monitor.origin.x + (monitor.size.width - px(WIDTH)) / 2.0,
            monitor.origin.y + px(TOP_MARGIN),
        ),
        size: size(px(WIDTH), px(HEIGHT)),
    };

    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id: display.map(|d| d.id()),
            titlebar: None,
            // Never take focus: text injection targets whatever is focused.
            focus: false,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            app_id: Some("dictation-tool".to_string()),
            ..Default::default()
        },
        move |_, cx| cx.new(|cx| OverlayView::new(state, cx)),
    )
    .map_err(|err| eprintln!("[overlay] failed to open window: {err}"))
    .ok()
}

/// Bounds of the RandR primary output (or the first connected one), in
/// GPUI's logical pixels. `None` when xrandr is unavailable (Wayland) or
/// its output can't be parsed -- callers fall back to the GPUI display.
fn primary_monitor_via_xrandr(screen: &Bounds<gpui::Pixels>) -> Option<Bounds<gpui::Pixels>> {
    let output = std::process::Command::new("xrandr").arg("--query").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let parse = |line: &str| -> Option<(f32, f32, f32, f32)> {
        // e.g. "DisplayPort-0 connected primary 2560x1440+1440+623 (normal ...)"
        let geom = line
            .split_whitespace()
            .find(|tok| tok.contains('x') && tok.contains('+'))?;
        let (wh, xy) = geom.split_once('+')?;
        let (w, h) = wh.split_once('x')?;
        let (x, y) = xy.split_once('+')?;
        Some((w.parse().ok()?, h.parse().ok()?, x.parse().ok()?, y.parse().ok()?))
    };
    let connected: Vec<&str> = text
        .lines()
        .filter(|l| l.contains(" connected"))
        .collect();
    let line = connected
        .iter()
        .find(|l| l.contains(" connected primary"))
        .or_else(|| connected.first())?;
    let (w, h, x, y) = parse(line)?;

    // xrandr reports device pixels; GPUI's screen bounds are already scaled.
    let root_width: f32 = text
        .lines()
        .find(|l| l.starts_with("Screen "))
        .and_then(|l| l.split("current ").nth(1))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|w| w.parse().ok())?;
    let scale = if root_width > 0.0 { root_width / f32::from(screen.size.width) } else { 1.0 };
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };

    Some(Bounds {
        origin: point(px(x / scale), px(y / scale)),
        size: size(px(w / scale), px(h / scale)),
    })
}

/// A permanently hidden 1x1 window. GPUI's Linux backends stop the event
/// loop when the last window closes, and we want the daemon to outlive the
/// main window.
pub struct Anchor;

impl Render for Anchor {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

pub fn open_anchor(cx: &mut App) {
    let result = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(0.), px(0.)),
                size: size(px(1.), px(1.)),
            })),
            titlebar: None,
            focus: false,
            show: false,
            kind: WindowKind::PopUp,
            is_movable: false,
            ..Default::default()
        },
        |_, cx| cx.new(|_| Anchor),
    );
    if let Err(err) = result {
        eprintln!("[overlay] failed to open anchor window: {err}");
    }
}
