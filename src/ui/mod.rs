pub mod history_view;
pub mod main_window;
pub mod settings_view;
pub mod status_bar;
pub mod theme;
pub mod waveform;

use crate::overlay::OverlayView;
use crate::state::AppState;
use gpui::{
    prelude::*, px, size, App, Bounds, Entity, Global, WindowBounds, WindowDecorations,
    WindowHandle, WindowOptions,
};
use main_window::MainWindow;

/// Handles to the windows this process may have open. Both are optional:
/// the main window can be closed by the user (the daemon keeps running) and
/// the overlay only exists while recording/processing.
#[derive(Default)]
pub struct Windows {
    pub main: Option<WindowHandle<MainWindow>>,
    pub overlay: Option<WindowHandle<OverlayView>>,
}

impl Global for Windows {}

/// "ggml-large-v3-turbo.bin" -> "large v3 turbo". The filename is an
/// implementation detail of whisper.cpp; the model has a name.
pub fn model_label(file: &str) -> String {
    file.trim_start_matches("ggml-")
        .trim_end_matches(".bin")
        .replace('-', " ")
}

/// Opens the main window, or raises it if it is already open.
pub fn show_main(state: &Entity<AppState>, cx: &mut App) {
    if let Some(handle) = cx.global::<Windows>().main {
        if handle.is_active(cx).is_some() {
            handle
                .update(cx, |_, window, _| window.activate_window())
                .ok();
            return;
        }
    }

    let bounds = Bounds::centered(None, size(px(780.), px(580.)), cx);
    let state = state.clone();
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // No system titlebar: MainWindow draws its own header, which
                // doubles as the drag handle and carries the window controls.
                titlebar: None,
                window_decorations: Some(WindowDecorations::Client),
                app_id: Some("dictation-tool".to_string()),
                window_min_size: Some(size(px(480.), px(320.))),
                ..Default::default()
            },
            move |_, cx| cx.new(|cx| MainWindow::new(state, cx)),
        )
        .ok();
    cx.global_mut::<Windows>().main = handle;
}
