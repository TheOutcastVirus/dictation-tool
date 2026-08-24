//! Dictation tool: hold Right Alt, speak, release -- the transcription is
//! typed into the focused window. One binary owns hotkey capture, mic
//! capture, in-process Whisper (ROCm/HIP), text injection and the GPUI
//! companion window.

mod audio;
mod autostart;
mod config;
mod engine;
mod history;
mod hotkey;
mod inject;
mod instance;
mod logger;
mod overlay;
mod state;
mod transcribe;
mod ui;
mod vram;

use crossbeam_channel::Receiver;
use gpui::{prelude::*, App, Application, Entity};
use state::{AppState, EngineEvent};
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    let (event_tx, event_rx) = crossbeam_channel::unbounded();
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();

    let _instance = match instance::claim(event_tx.clone()) {
        instance::Instance::Primary(conn) => conn,
        instance::Instance::Secondary => {
            println!("dictation-tool is already running; raised its window.");
            return;
        }
    };

    let repo_root = locate_repo_root();
    let models_dir = repo_root.join("whisper.cpp").join("models");
    if let Ok(exe) = std::env::current_exe() {
        autostart::ensure_unit_installed(&repo_root, &exe);
    }

    engine::spawn(models_dir.clone(), event_tx, cmd_rx);

    prefer_x11_backend();

    Application::new().run(move |cx: &mut App| {
        let state = cx.new(|_| AppState::new(models_dir, cmd_tx));
        cx.set_global(ui::Windows::default());

        overlay::open_anchor(cx);
        ui::show_main(&state, cx);
        spawn_event_bridge(state.clone(), event_rx, cx);
        spawn_pollers(state, cx);

        cx.activate(true);
    });
}

/// Steers GPUI onto its X11 backend when the session offers one.
///
/// The overlay bubble needs two things from a window: that focusing it never
/// steals the caret from whatever is being dictated into, and that it can put
/// itself bottom-centre of the display. GPUI's Wayland backend gives neither.
/// xdg-shell has no "do not focus me" hint -- `WindowOptions::focus` is
/// `allow(dead_code)` on Linux and `WindowKind::PopUp` is ignored there -- so
/// Mutter focuses the bubble the instant it maps and the transcription is
/// typed into the bubble rather than the target window. Wayland also forbids
/// a client positioning its own surface, so the bubble lands wherever the
/// compositor likes.
///
/// The X11 backend has both: `PopUp` becomes `_NET_WM_WINDOW_TYPE_NOTIFICATION`,
/// which Mutter never focuses, and absolute bounds are honoured (what
/// `primary_monitor_via_xrandr` in the overlay is already written against).
/// A Wayland session runs XWayland, so hiding `WAYLAND_DISPLAY` from GPUI is
/// enough to get it. Set `DICTATION_FORCE_WAYLAND` to opt back out.
fn prefer_x11_backend() {
    if std::env::var_os("DICTATION_FORCE_WAYLAND").is_some() {
        return;
    }
    let has_x11 = std::env::var_os("DISPLAY").is_some_and(|d| !d.is_empty());
    let on_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|d| !d.is_empty());
    if has_x11 && on_wayland {
        std::env::remove_var("WAYLAND_DISPLAY");
    }
}

/// The directory containing `whisper.cpp/models`: the working directory
/// (what the systemd unit sets), else walking up from the executable
/// (`target/{debug,release}/dictation-tool` inside the repo).
fn locate_repo_root() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.join("whisper.cpp").join("models").is_dir() {
        return cwd;
    }
    if let Ok(exe) = std::env::current_exe() {
        for dir in exe.ancestors().skip(1) {
            if dir.join("whisper.cpp").join("models").is_dir() {
                return dir.to_path_buf();
            }
        }
    }
    cwd
}

/// Background thread -> channel -> foreground. `AsyncApp` is not `Send`, so
/// the blocking `recv` runs on GPUI's background executor and the result is
/// awaited from a foreground task that owns the `AppState` updates.
fn spawn_event_bridge(state: Entity<AppState>, event_rx: Receiver<EngineEvent>, cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            let rx = event_rx.clone();
            let first = cx
                .background_executor()
                .spawn(async move { rx.recv().ok() })
                .await;

            let Some(first) = first else {
                // Every sender dropped: the engine thread has exited.
                let _ = cx.update(|cx| {
                    state.update(cx, |s, cx| {
                        s.engine_down();
                        cx.notify();
                    });
                    overlay::sync(&state, cx);
                });
                break;
            };

            let mut batch = vec![first];
            while let Ok(event) = event_rx.try_recv() {
                batch.push(event);
            }

            let alive = cx
                .update(|cx| {
                    let mut show_window = false;
                    state.update(cx, |s, cx| {
                        for event in batch {
                            if s.apply(event) {
                                show_window = true;
                            }
                        }
                        cx.notify();
                    });
                    overlay::sync(&state, cx);
                    if show_window {
                        ui::show_main(&state, cx);
                    }
                })
                .is_ok();
            if !alive {
                break;
            }
        }
    })
    .detach();
}

/// Once a second: VRAM readout, tail the history log, rescan the models dir.
fn spawn_pollers(state: Entity<AppState>, cx: &mut App) {
    cx.spawn(async move |cx| loop {
        cx.background_executor()
            .timer(Duration::from_secs(1))
            .await;
        let alive = state
            .update(cx, |s, cx| {
                s.vram = vram::read_model_vram();
                s.history.poll();
                s.refresh_models();
                cx.notify();
            })
            .is_ok();
        if !alive {
            break;
        }
    })
    .detach();
}
