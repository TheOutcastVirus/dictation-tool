//! Port of `logger.py`: one JSON object per line, same five fields, same
//! path, so history written by the Python tool and by this binary interleave
//! seamlessly.

use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

#[derive(Serialize)]
struct DictationEntry<'a> {
    timestamp: String,
    audio_duration_s: f64,
    transcribe_ms: u64,
    word_count: usize,
    char_count: usize,
    text: &'a str,
}

pub fn log_path() -> PathBuf {
    dirs::data_local_dir()
        .expect("no local data dir")
        .join("dictation-tool")
        .join("dictation.jsonl")
}

pub fn log(text: &str, audio_duration_s: f64, transcribe_ms: u64) {
    let entry = DictationEntry {
        // Same shape as Python's `datetime.now().isoformat()`: local time,
        // microsecond precision, no zone suffix.
        timestamp: chrono::Local::now()
            .format("%Y-%m-%dT%H:%M:%S%.6f")
            .to_string(),
        audio_duration_s: (audio_duration_s * 100.0).round() / 100.0,
        transcribe_ms,
        word_count: text.split_whitespace().count(),
        char_count: text.chars().count(),
        text,
    };

    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let line = serde_json::to_string(&entry).expect("failed to serialize dictation entry");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{line}");
    }
}
