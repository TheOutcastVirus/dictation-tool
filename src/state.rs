use crate::history::History;
use crate::vram::VramStats;
use crossbeam_channel::Sender;
use std::collections::VecDeque;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EngineState {
    Idle,
    Recording,
    Processing,
}

/// Events flowing from background threads to the GPUI foreground.
pub enum EngineEvent {
    State(EngineState),
    Level(f32),
    ModelLoaded(String),
    ModelLoadError(String),
    /// Non-fatal problem (mic unavailable, injection failed, ...). Surfaced
    /// in the status bar; the engine keeps running.
    Error(String),
    /// The engine cannot continue (no keyboards, no injection backend, ...).
    Fatal(String),
    /// Another launch of this binary asked us to bring the main window up.
    ShowWindow,
}

pub enum EngineCommand {
    SwitchModel(PathBuf),
}

/// ~11 s of level samples at the audio thread's 60 ms cadence: enough to
/// fill the waveform band at one column per 4 px.
const LEVEL_HISTORY_LEN: usize = 192;

/// Owned by the GPUI foreground as an `Entity<AppState>`; mutated only on
/// the main thread (event bridge + pollers in `main.rs`, UI handlers).
pub struct AppState {
    pub engine_state: EngineState,
    pub level_history: VecDeque<f32>,
    /// The envelope of the last completed utterance, kept so the waveform
    /// band shows real audio even when nothing is being recorded.
    pub last_envelope: Vec<f32>,
    pub current_model: String,
    pub model_status: Option<String>,
    pub vram: Option<VramStats>,
    pub history: History,
    pub engine_online: bool,
    pub last_error: Option<String>,
    pub models_dir: PathBuf,
    pub available_models: Vec<String>,
    pub autostart_enabled: bool,
    pub cmd_tx: Sender<EngineCommand>,
}

impl AppState {
    pub fn new(models_dir: PathBuf, cmd_tx: Sender<EngineCommand>) -> Self {
        let mut state = AppState {
            engine_state: EngineState::Idle,
            level_history: VecDeque::with_capacity(LEVEL_HISTORY_LEN),
            last_envelope: Vec::new(),
            current_model: crate::config::load().model,
            model_status: None,
            vram: crate::vram::read_model_vram(),
            history: History::load(),
            engine_online: false,
            last_error: None,
            models_dir,
            available_models: Vec::new(),
            autostart_enabled: crate::autostart::is_enabled(),
            cmd_tx,
        };
        state.refresh_models();
        state
    }

    /// Lists `*.bin` files in the models directory, excluding whisper.cpp's
    /// `for-tests-*` fixtures.
    pub fn refresh_models(&mut self) {
        let mut models: Vec<String> = std::fs::read_dir(&self.models_dir)
            .map(|dir| {
                dir.flatten()
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .filter(|name| name.ends_with(".bin") && !name.starts_with("for-tests-"))
                    .collect()
            })
            .unwrap_or_default();
        models.sort();
        self.available_models = models;
    }

    pub fn push_level(&mut self, level: f32) {
        if self.level_history.len() == LEVEL_HISTORY_LEN {
            self.level_history.pop_front();
        }
        self.level_history.push_back(level);
    }

    /// Moves the live level stream into `last_envelope`. A recording too
    /// short to have any shape is discarded rather than replacing a good
    /// trace with a flat line.
    fn hold_envelope(&mut self) {
        if self.level_history.len() > 2 {
            self.last_envelope = self.level_history.iter().copied().collect();
        }
        self.level_history.clear();
    }

    /// What the waveform band should draw: the live stream while recording,
    /// otherwise the envelope of the last thing that was said.
    pub fn trace(&self) -> (Vec<f32>, crate::ui::waveform::Trace) {
        if self.engine_state == EngineState::Recording {
            (
                self.level_history.iter().copied().collect(),
                crate::ui::waveform::Trace::Live,
            )
        } else {
            (self.last_envelope.clone(), crate::ui::waveform::Trace::Held)
        }
    }

    pub fn request_model(&mut self, name: &str) {
        let path = self.models_dir.join(name);
        self.model_status = Some(format!("Loading {name}..."));
        let _ = self.cmd_tx.send(EngineCommand::SwitchModel(path));
    }

    /// Applies one engine event. Returns true if the event is `ShowWindow`,
    /// which the caller handles at the window level.
    pub fn apply(&mut self, event: EngineEvent) -> bool {
        match event {
            EngineEvent::State(state) => {
                self.engine_state = state;
                if state == EngineState::Recording {
                    // A fresh session: drop any stale error from the last one.
                    self.last_error = None;
                } else {
                    self.hold_envelope();
                }
            }
            EngineEvent::Level(level) => self.push_level(level),
            EngineEvent::ModelLoaded(name) => {
                self.current_model = name;
                self.model_status = Some("Applied".to_string());
                self.engine_online = true;
            }
            EngineEvent::ModelLoadError(err) => {
                self.model_status = Some(format!("Failed: {err}"));
            }
            EngineEvent::Error(err) => {
                self.last_error = Some(err);
            }
            EngineEvent::Fatal(err) => {
                self.last_error = Some(err);
                self.engine_online = false;
                self.engine_state = EngineState::Idle;
                self.hold_envelope();
            }
            EngineEvent::ShowWindow => return true,
        }
        false
    }

    /// Called when the engine thread's event channel disconnects.
    pub fn engine_down(&mut self) {
        self.engine_online = false;
        self.engine_state = EngineState::Idle;
        self.hold_envelope();
        if self.last_error.is_none() {
            self.last_error = Some("engine thread exited".to_string());
        }
    }
}
