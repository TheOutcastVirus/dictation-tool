use evdev::{Device, EventSummary, KeyCode};
use std::thread;
use std::time::{Duration, Instant};

/// Right Alt is the dictation hotkey (`main.py:20`'s `HOTKEY_CODE`).
const HOTKEY: KeyCode = KeyCode::KEY_RIGHTALT;
/// Taps shorter than this are treated as accidental and discarded
/// (`main.py:21`'s `MIN_DURATION`).
pub const MIN_DURATION: Duration = Duration::from_millis(300);

const VIRTUAL_NAME_FRAGMENTS: [&str; 3] = ["virtual", "uinput", "dictation-uinput"];

pub enum HotkeyEvent {
    Down,
    /// Up, with the hold duration — the caller decides whether it clears
    /// `MIN_DURATION`.
    Up(Duration),
}

fn is_physical_keyboard(device: &Device) -> bool {
    let name = device.name().unwrap_or("").to_lowercase();
    if VIRTUAL_NAME_FRAGMENTS.iter().any(|f| name.contains(f)) {
        return false;
    }
    device
        .supported_keys()
        .map(|keys| keys.contains(KeyCode::KEY_A))
        .unwrap_or(false)
}

fn find_keyboards() -> Vec<(std::path::PathBuf, Device)> {
    evdev::enumerate()
        .filter(|(_, dev)| is_physical_keyboard(dev))
        .collect()
}

/// Spawns one listener thread per physical keyboard device and forwards
/// hotkey up/down transitions on `tx`. Debouncing across multiple keyboards
/// (so a second device's key-down while already recording is a no-op) is
/// handled by the caller, which owns the single source of truth for
/// "are we currently recording" — mirrors `main.py`'s `_recording`/`_lock`
/// guard (`main.py:30-32, 61-84`).
pub fn spawn_listeners(tx: crossbeam_channel::Sender<HotkeyEvent>) -> usize {
    let keyboards = find_keyboards();
    let count = keyboards.len();

    for (path, mut device) in keyboards {
        let tx = tx.clone();
        thread::spawn(move || {
            let mut press_time: Option<Instant> = None;
            loop {
                let events = match device.fetch_events() {
                    Ok(events) => events,
                    Err(err) => {
                        eprintln!("[hotkey] device lost {}: {err}", path.display());
                        return;
                    }
                };
                for event in events {
                    if let EventSummary::Key(_, HOTKEY, value) = event.destructure() {
                        match value {
                            1 => {
                                if press_time.is_none() {
                                    press_time = Some(Instant::now());
                                    let _ = tx.send(HotkeyEvent::Down);
                                }
                            }
                            0 => {
                                if let Some(start) = press_time.take() {
                                    let _ = tx.send(HotkeyEvent::Up(start.elapsed()));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        });
    }

    count
}
