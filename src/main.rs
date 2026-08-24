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
                s.vram = vram::read_primary_amd_vram();
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
