use crate::audio::Recorder;
use crate::hotkey::{self, HotkeyEvent, MIN_DURATION};
use crate::inject::Typer;
use crate::state::{EngineCommand, EngineEvent, EngineState};
use crate::transcribe::Transcriber;
use crate::{config, logger};
use crossbeam_channel::{Receiver, Sender};
use std::path::PathBuf;
use std::thread;
use std::time::Instant;

/// Spawns the whole engine (hotkey listeners, audio capture, model, text
/// injection) on background threads. Nothing here runs on the GPUI main
/// thread -- in particular, `inject::Typer::type_text` has real blocking
/// D-Bus round trips and (in the uinput fallback) real sleeps, and must
/// never stall the UI's event loop.
///
/// Dropping `event_tx` (by returning from `run`) is how the foreground
/// learns the engine is gone.
pub fn spawn(models_dir: PathBuf, event_tx: Sender<EngineEvent>, cmd_rx: Receiver<EngineCommand>) {
    thread::Builder::new()
        .name("dictation-engine".into())
        .spawn(move || run(models_dir, event_tx, cmd_rx))
        .expect("failed to spawn engine thread");
}

fn run(models_dir: PathBuf, event_tx: Sender<EngineEvent>, cmd_rx: Receiver<EngineCommand>) {
    // Cheap prerequisites first, so a permissions problem is reported
    // instantly rather than after a multi-second model load.
    let (hotkey_tx, hotkey_rx) = crossbeam_channel::unbounded();
    let count = hotkey::spawn_listeners(hotkey_tx);
    if count == 0 {
        let msg = "no keyboard devices found -- add yourself to the 'input' group \
                   (sudo usermod -aG input $USER) and log back in";
        eprintln!("[engine] ERROR: {msg}");
        let _ = event_tx.send(EngineEvent::Fatal(msg.to_string()));
        return;
    }
    println!("[engine] listening on {count} keyboard device(s)");

    let mut typer = match Typer::new() {
        Ok(t) => t,
        Err(err) => {
            eprintln!("[engine] ERROR: {err}");
            let _ = event_tx.send(EngineEvent::Fatal(err));
            return;
        }
    };

    let cfg = config::load();
    let model_path = models_dir.join(&cfg.model);
    let mut transcriber = match Transcriber::new(&model_path) {
        Ok(t) => t,
        Err(err) => {
            eprintln!("[engine] failed to load model: {err}");
            let _ = event_tx.send(EngineEvent::ModelLoadError(err.clone()));
            let _ = event_tx.send(EngineEvent::Fatal(format!(
                "could not load {}: {err}",
                model_path.display()
            )));
            return;
        }
    };
    let _ = event_tx.send(EngineEvent::ModelLoaded(cfg.model.clone()));
    let _ = event_tx.send(EngineEvent::State(EngineState::Idle));
    println!("[engine] ready. Hold Right Alt to dictate.");

    let mut recorder = Recorder::new();
    // Single source of truth for "are we recording", so a second keyboard's
    // Right Alt (or a spurious repeat) can't restart capture mid-session.
    let mut recording = false;

    loop {
        crossbeam_channel::select! {
            recv(hotkey_rx) -> msg => {
                let Ok(msg) = msg else { break };
                match msg {
                    HotkeyEvent::Down => {
                        if recording {
                            continue;
                        }
                        let level_tx = event_tx.clone();
                        match recorder.start(move |rms| {
                            let _ = level_tx.send(EngineEvent::Level(rms));
                        }) {
                            Ok(()) => {
                                recording = true;
                                let _ = event_tx.send(EngineEvent::State(EngineState::Recording));
                            }
                            Err(err) => {
                                eprintln!("[engine] could not start recording: {err}");
                                let _ = event_tx.send(EngineEvent::Error(format!("microphone: {err}")));
                            }
                        }
                    }
                    HotkeyEvent::Up(duration) => {
                        if !recording {
                            continue;
                        }
                        recording = false;
                        let audio = recorder.stop();
                        if duration < MIN_DURATION {
                            println!("[engine] too short ({:.2}s), ignoring", duration.as_secs_f32());
                            let _ = event_tx.send(EngineEvent::State(EngineState::Idle));
                            continue;
                        }
                        let _ = event_tx.send(EngineEvent::State(EngineState::Processing));

                        let t0 = Instant::now();
                        let text = transcriber.transcribe(&audio);
                        let transcribe_ms = t0.elapsed().as_millis() as u64;
                        let audio_duration_s = audio.len() as f64 / crate::audio::SAMPLE_RATE as f64;
                        println!(
                            "[engine] result: {text:?} ({audio_duration_s:.1}s audio, {transcribe_ms}ms)"
                        );

                        if !text.is_empty() {
                            logger::log(&text, audio_duration_s, transcribe_ms);
                            if let Err(err) = typer.type_text(&text) {
                                eprintln!("[engine] text injection failed: {err}");
                                let _ = event_tx.send(EngineEvent::Error(format!("typing: {err}")));
                            }
                        }
                        // Idle only after the text has actually been typed --
                        // the overlay's spinner stays up through injection.
                        let _ = event_tx.send(EngineEvent::State(EngineState::Idle));
                    }
                }
            }
            recv(cmd_rx) -> msg => {
                let Ok(msg) = msg else { break };
                match msg {
                    EngineCommand::SwitchModel(path) => {
                        match transcriber.switch_model(&path) {
                            Ok(()) => {
                                let name = path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or_default()
                                    .to_string();
                                let mut cfg = config::load();
                                cfg.model = name.clone();
                                config::save(&cfg);
                                let _ = event_tx.send(EngineEvent::ModelLoaded(name));
                            }
                            Err(err) => {
                                let _ = event_tx.send(EngineEvent::ModelLoadError(err));
                            }
                        }
                    }
                }
            }
        }
    }

    typer.close();
}
