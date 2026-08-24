use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Transcriber {
    ctx: WhisperContext,
}

impl Transcriber {
    pub fn new(model_path: &Path) -> Result<Self, String> {
        let ctx = WhisperContext::new_with_params(
            model_path.to_str().ok_or("invalid model path")?,
            WhisperContextParameters::default(),
        )
        .map_err(|e| format!("failed to load model: {e}"))?;
        Ok(Transcriber { ctx })
    }

    /// Replaces the loaded model, holding no other lock than the caller's own
    /// -- callers must serialize this against `transcribe()` themselves
    /// (mirrors `whisper-server`'s `whisper_mutex` guarding both inference
    /// and `/load`).
    pub fn switch_model(&mut self, model_path: &Path) -> Result<(), String> {
        let ctx = WhisperContext::new_with_params(
            model_path.to_str().ok_or("invalid model path")?,
            WhisperContextParameters::default(),
        )
        .map_err(|e| format!("failed to load model: {e}"))?;
        self.ctx = ctx;
        Ok(())
    }

    /// Transcribes mono 16kHz f32 PCM samples directly -- no WAV encoding or
    /// HTTP body construction needed, since the model runs in-process.
    pub fn transcribe(&self, samples: &[f32]) -> String {
        let mut state = match self.ctx.create_state() {
            Ok(state) => state,
            Err(err) => {
                eprintln!("[transcribe] failed to create state: {err}");
                return String::new();
            }
        };

        let mut params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: 5,
            patience: -1.0,
        });
        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        // Don't decode non-speech tokens like "[BLANK_AUDIO]" / "(keyboard
        // clicking)" -- nobody wants those typed into their editor.
        params.set_suppress_nst(true);

        if let Err(err) = state.full(params, samples) {
            eprintln!("[transcribe] inference failed: {err}");
            return String::new();
        }

        let mut text = String::new();
        for segment in state.as_iter() {
            let segment = segment.to_string();
            if is_annotation(segment.trim()) {
                continue;
            }
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&segment);
        }
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

/// Whisper's non-speech annotations, e.g. "[BLANK_AUDIO]", "(music)",
/// "[Silence]", "*laughs*". `suppress_nst` catches most of them at decode
/// time; this is the belt to that suspenders.
fn is_annotation(segment: &str) -> bool {
    if segment.is_empty() {
        return true;
    }
    let pairs = [('[', ']'), ('(', ')'), ('*', '*')];
    pairs.iter().any(|&(open, close)| {
        segment.starts_with(open)
            && segment.ends_with(close)
            && segment.len() > 1
            && !segment[1..segment.len() - 1].contains(open)
    })
}
