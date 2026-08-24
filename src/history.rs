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

    #[cfg(test)]
    fn load_from(path: &std::path::Path) -> Self {
        let mut history = History {
            entries: Vec::new(),
            read_offset: 0,
        };
        history.poll_from(path);
        history
    }

    /// Reads any lines appended since the last poll and prepends them
    /// (most-recent-first ordering). Cheap enough to call on a ~1-2s timer.
    pub fn poll(&mut self) -> bool {
        self.poll_from(&crate::logger::log_path())
    }

    fn poll_from(&mut self, path: &std::path::Path) -> bool {
        let Ok(file) = std::fs::File::open(path) else {
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

    /// Removes one entry from the log file, matching on the line's own
    /// timestamp and text rather than on its position, so a line appended
    /// since the last poll cannot shift what gets deleted.
    ///
    /// The rewrite is a write-then-rename, and it is abandoned if the engine
    /// appended anything while we were reading -- better to leave the entry
    /// in place and let the user click again than to drop a dictation that
    /// landed a millisecond ago.
    pub fn delete(&mut self, index: usize) -> bool {
        self.delete_from(index, &crate::logger::log_path())
    }

    fn delete_from(&mut self, index: usize, path: &std::path::Path) -> bool {
        let Some(entry) = self.entries.get(index) else {
            return false;
        };
        let (timestamp, text) = (entry.timestamp.clone(), entry.text.clone());
        let Ok(contents) = std::fs::read_to_string(path) else {
            return false;
        };

        let mut removed = false;
        let mut kept = String::with_capacity(contents.len());
        for line in contents.lines() {
            if !removed {
                if let Ok(parsed) = serde_json::from_str::<DictationEntry>(line) {
                    if parsed.timestamp == timestamp && parsed.text == text {
                        removed = true;
                        continue;
                    }
                }
            }
            kept.push_str(line);
            kept.push('\n');
        }
        if !removed {
            return false;
        }

        // Nothing may have been appended between the read and the swap.
        if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) != contents.len() as u64 {
            return false;
        }

        let temp = path.with_extension("jsonl.rewrite");
        if std::fs::write(&temp, &kept).is_err() {
            let _ = std::fs::remove_file(&temp);
            return false;
        }
        if std::fs::rename(&temp, path).is_err() {
            let _ = std::fs::remove_file(&temp);
            return false;
        }

        // The file just changed underneath us in a way `poll` cannot follow
        // incrementally, and it may also contain a dictation appended since
        // the last poll. Re-read it whole so the window matches the log.
        self.entries.clear();
        self.read_offset = 0;
        self.poll_from(path);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn line(timestamp: &str, text: &str) -> String {
        format!(
            r#"{{"timestamp":"{timestamp}","audio_duration_s":1.0,"transcribe_ms":10,"word_count":1,"char_count":{},"text":"{text}"}}"#,
            text.len()
        )
    }

    /// Writes a log and the matching in-memory view (newest first, as `poll`
    /// builds it).
    fn fixture(dir: &std::path::Path, rows: &[(&str, &str)]) -> (std::path::PathBuf, History) {
        let path = dir.join("dictation.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        for (timestamp, text) in rows {
            writeln!(file, "{}", line(timestamp, text)).unwrap();
        }
        let entries = rows
            .iter()
            .rev()
            .map(|(timestamp, text)| {
                serde_json::from_str::<DictationEntry>(&line(timestamp, text)).unwrap()
            })
            .collect();
        (
            path.clone(),
            History {
                entries,
                read_offset: std::fs::metadata(&path).unwrap().len(),
            },
        )
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dictation-history-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn deleting_removes_that_line_and_leaves_the_others() {
        let dir = temp_dir("basic");
        let (path, mut history) = fixture(
            &dir,
            &[("2026-01-01T10:00:00", "first"), ("2026-01-01T11:00:00", "second")],
        );

        // Entries are newest first, so index 0 is "second".
        assert!(history.delete_from(0, &path));

        let left = std::fs::read_to_string(&path).unwrap();
        assert!(left.contains("first"), "deleted the wrong line: {left}");
        assert!(!left.contains("second"), "line survived: {left}");
        assert_eq!(history.entries().len(), 1);
        // The offset must match the rewritten file or the next poll re-reads
        // old lines and duplicates them.
        assert_eq!(history.read_offset, std::fs::metadata(&path).unwrap().len());
    }

    #[test]
    fn deleting_one_of_two_identical_texts_removes_only_one() {
        let dir = temp_dir("dupes");
        let (path, mut history) = fixture(
            &dir,
            &[("2026-01-01T10:00:00", "same"), ("2026-01-01T11:00:00", "same")],
        );

        assert!(history.delete_from(0, &path));
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 1);
    }

    #[test]
    fn a_dictation_appended_since_the_last_poll_survives_a_delete() {
        let dir = temp_dir("race");
        let (path, mut history) = fixture(&dir, &[("2026-01-01T10:00:00", "first")]);

        // The engine appends while the window is open, so the in-memory view
        // is a line behind the file.
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "{}", line("2026-01-01T12:00:00", "just spoken")).unwrap();
        drop(file);

        assert!(history.delete_from(0, &path));

        let left = std::fs::read_to_string(&path).unwrap();
        assert!(!left.contains("first"), "wrong line deleted: {left}");
        assert!(left.contains("just spoken"), "lost a fresh dictation: {left}");
        // And it has to be visible, not merely present on disk.
        assert!(
            history.entries().iter().any(|e| e.text == "just spoken"),
            "the new dictation never reached the window"
        );
    }

    #[test]
    fn the_view_matches_the_file_after_a_delete() {
        let dir = temp_dir("reload");
        let (path, mut history) = fixture(
            &dir,
            &[
                ("2026-01-01T10:00:00", "a"),
                ("2026-01-01T11:00:00", "b"),
                ("2026-01-01T12:00:00", "c"),
            ],
        );

        assert!(history.delete_from(1, &path));

        let reloaded = History::load_from(&path);
        let seen: Vec<&str> = history.entries().iter().map(|e| e.text.as_str()).collect();
        let expected: Vec<&str> = reloaded.entries().iter().map(|e| e.text.as_str()).collect();
        assert_eq!(seen, expected);
        assert_eq!(seen, vec!["c", "a"]);
    }

    #[test]
    fn deleting_out_of_range_is_a_no_op() {
        let dir = temp_dir("range");
        let (path, mut history) = fixture(&dir, &[("2026-01-01T10:00:00", "only")]);
        assert!(!history.delete_from(7, &path));
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 1);
    }
}
