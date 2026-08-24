use serde::Deserialize;
use std::io::{BufRead, BufReader, Seek, SeekFrom};

#[derive(Deserialize, Clone, Debug)]
pub struct DictationEntry {
    pub timestamp: String,
    pub audio_duration_s: f64,
    pub transcribe_ms: u64,
    pub word_count: usize,
    #[allow(dead_code)]
    pub char_count: usize,
    pub text: String,
}

pub struct History {
    entries: Vec<DictationEntry>, // most-recent-first
    read_offset: u64,
}

impl History {
    pub fn load() -> Self {
        let mut history = History {
            entries: Vec::new(),
            read_offset: 0,
        };
        history.poll();
        history
    }

    /// Reads any lines appended since the last poll and prepends them
    /// (most-recent-first ordering). Cheap enough to call on a ~1-2s timer.
    pub fn poll(&mut self) -> bool {
        let path = crate::logger::log_path();
        let Ok(file) = std::fs::File::open(&path) else {
            return false;
        };
        let Ok(metadata) = file.metadata() else {
            return false;
        };
        if metadata.len() < self.read_offset {
            // File was truncated/rotated -- restart from scratch.
            self.read_offset = 0;
            self.entries.clear();
        }
        if metadata.len() == self.read_offset {
            return false;
        }

        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(self.read_offset)).is_err() {
            return false;
        }

        let mut new_entries = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            let bytes_read = match reader.read_line(&mut line) {
                Ok(n) => n,
                Err(_) => break,
            };
            if bytes_read == 0 {
                break;
            }
            if !line.ends_with('\n') {
                // Partial trailing line -- stop here, pick it up next poll.
                break;
            }
            self.read_offset += bytes_read as u64;
            if let Ok(entry) = serde_json::from_str::<DictationEntry>(line.trim_end()) {
                new_entries.push(entry);
            }
        }

        if new_entries.is_empty() {
            return false;
        }
        // Newest first: the freshly read lines (reversed) go in front of
        // whatever we already had.
        new_entries.reverse();
        new_entries.append(&mut self.entries);
        self.entries = new_entries;
        true
    }

    pub fn entries(&self) -> &[DictationEntry] {
        &self.entries
    }
}
