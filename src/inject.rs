//! Port of `typer.py`. Two backends: Mutter's private `RemoteDesktop` D-Bus
//! keysym API (primary, no permission dialog) and a uinput fallback for
//! non-GNOME compositors.
//!
//! Per-character keystroke injection is racy on GNOME/Mutter: modifier state
//! propagates to clients asynchronously, so capitalization and shifted
//! punctuation corrupt when events arrive faster than the client can keep up.
//! Handing Mutter whole keysyms does not avoid this by itself, which an
//! earlier version of this file claimed. Mutter resolves a keysym needing
//! level 2 by wrapping it -- `Shift press, key press` on our press event and
//! `key release, Shift release` on our release -- and a release sent
//! immediately behind the press pushes the `Shift release` out before the
//! client has applied the `Shift press`. The key is then evaluated unshifted.
//!
//! Measured against a native Wayland client: sending the pairs back to back
//! loses *every* shifted character ("Is this working? Yes!" arrives as "is
//! this working/ yes1"), and a partial delay leaves shift trailing a
//! character behind ("WorKiNg"). X11 clients are immune -- XWayland tracks
//! modifier state server-side and delivers it with the event -- which is why
//! this went unnoticed. Holding each keysym down for `KEYSYM_HOLD` before
//! releasing it fixes it; a gap *between* characters instead does not, at any
//! length, because it does not separate the press from its own release.

use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;
use zbus::blocking::Connection;
use zbus::message::Flags;
use zbus::Message;

const RD_BUS: &str = "org.gnome.Mutter.RemoteDesktop";
const RD_PATH: &str = "/org/gnome/Mutter/RemoteDesktop";
const RD_IFACE: &str = "org.gnome.Mutter.RemoteDesktop";
const RD_SESSION_IFACE: &str = "org.gnome.Mutter.RemoteDesktop.Session";

/// How long a keysym is held between its press and its release.
///
/// This is the whole fix: it is the window in which the client applies the
/// `Shift press` Mutter emitted alongside the key. Measured on gfx1152/GNOME
/// 42 against a native Wayland client, 2ms still corrupted the first shifted
/// character and 4ms passed; 6ms leaves margin and costs ~0.9s for a 134
/// character utterance, against transcription's own ~0.5-1s. A gap between
/// characters is not a substitute -- 8ms of it still lost every capital.
const KEYSYM_HOLD: Duration = Duration::from_micros(6_000);

fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\u{2018}' | '\u{2019}' => out.push('\''),
            '\u{201c}' | '\u{201d}' => out.push('"'),
            '\u{2013}' | '\u{2014}' => out.push('-'),
            '\u{2026}' => out.push_str("..."),
            other => out.push(other),
        }
    }
    out
}

fn char_keysym(ch: char) -> u32 {
    match ch {
        '\n' => 0xff0d, // XK_Return
        '\t' => 0xff09, // XK_Tab
        _ => {
            let cp = ch as u32;
            if (0x20..=0x7e).contains(&cp) || (0xa0..=0xff).contains(&cp) {
                cp // ASCII + Latin-1 keysyms equal their codepoints
            } else {
                0x0100_0000 | cp // Unicode keysym convention
            }
        }
    }
}

struct MutterKeyboard {
    conn: Connection,
    session_path: String,
}

impl MutterKeyboard {
    fn new() -> zbus::Result<Self> {
        let conn = Connection::session()?;
        let reply = conn.call_method(
            Some(RD_BUS),
            RD_PATH,
            Some(RD_IFACE),
            "CreateSession",
            &(),
        )?;
        let session_path: zbus::zvariant::OwnedObjectPath = reply.body().deserialize()?;
        let session_path = session_path.to_string();

        conn.call_method(Some(RD_BUS), session_path.as_str(), Some(RD_SESSION_IFACE), "Start", &())?;

        Ok(MutterKeyboard { conn, session_path })
    }

    /// Fires each event with `NoReplyExpected` rather than waiting on a reply,
    /// but holds every keysym down for `KEYSYM_HOLD` before releasing it so
    /// Mutter's implicit `Shift` reaches the client while the key is still
    /// down. The final release round-trips, fencing the run so a dead session
    /// surfaces as an error here rather than silently dropping text.
    fn type_text(&self, text: &str) -> zbus::Result<()> {
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            let keysym = char_keysym(ch);
            self.send_keysym(keysym, true)?;
            sleep(KEYSYM_HOLD);
            if chars.peek().is_some() {
                self.send_keysym(keysym, false)?;
            } else {
                self.conn.call_method(
                    Some(RD_BUS),
                    self.session_path.as_str(),
                    Some(RD_SESSION_IFACE),
                    "NotifyKeyboardKeysym",
                    &(keysym, false),
                )?;
            }
        }
        Ok(())
    }

    fn send_keysym(&self, keysym: u32, pressed: bool) -> zbus::Result<()> {
        let msg = Message::method_call(self.session_path.as_str(), "NotifyKeyboardKeysym")?
            .destination(RD_BUS)?
            .interface(RD_SESSION_IFACE)?
            .with_flags(Flags::NoReplyExpected)?
            .build(&(keysym, pressed))?;
        self.conn.send(&msg)?;
        Ok(())
    }

    fn close(&self) {
        let _ = self.conn.call_method(
            Some(RD_BUS),
            self.session_path.as_str(),
            Some(RD_SESSION_IFACE),
            "Stop",
            &(),
        );
    }
}

// ── uinput fallback backend ─────────────────────────────────────────────────

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, KeyCode};

// Aggressive timing, pushed near the theoretical floor (USB HID polling at
// 1000Hz = 1ms minimum between events on real hardware). If shifted-char
// corruption reappears ("Hello" -> "hEllo", "?" -> "/"), bump
// MODIFIER_SETTLE first -- it races the xkbcommon state update and
// demonstrably broke at 2ms in earlier testing. Do not re-tune these.
const MODIFIER_SETTLE: Duration = Duration::from_micros(3_000);
const KEY_HOLD: Duration = Duration::from_micros(1_000);
const INTER_CHAR: Duration = Duration::from_micros(1_000);

fn char_map() -> HashMap<char, (KeyCode, bool)> {
    use evdev::KeyCode as K;
    let mut m = HashMap::new();
    let letters = [
        ('a', K::KEY_A), ('b', K::KEY_B), ('c', K::KEY_C), ('d', K::KEY_D),
        ('e', K::KEY_E), ('f', K::KEY_F), ('g', K::KEY_G), ('h', K::KEY_H),
        ('i', K::KEY_I), ('j', K::KEY_J), ('k', K::KEY_K), ('l', K::KEY_L),
        ('m', K::KEY_M), ('n', K::KEY_N), ('o', K::KEY_O), ('p', K::KEY_P),
        ('q', K::KEY_Q), ('r', K::KEY_R), ('s', K::KEY_S), ('t', K::KEY_T),
        ('u', K::KEY_U), ('v', K::KEY_V), ('w', K::KEY_W), ('x', K::KEY_X),
        ('y', K::KEY_Y), ('z', K::KEY_Z),
    ];
    for (lower, code) in letters {
        m.insert(lower, (code, false));
        m.insert(lower.to_ascii_uppercase(), (code, true));
    }
    let digits = [
        ('1', K::KEY_1), ('2', K::KEY_2), ('3', K::KEY_3), ('4', K::KEY_4),
        ('5', K::KEY_5), ('6', K::KEY_6), ('7', K::KEY_7), ('8', K::KEY_8),
        ('9', K::KEY_9), ('0', K::KEY_0),
    ];
    for (d, code) in digits {
        m.insert(d, (code, false));
    }
    m.insert(' ', (K::KEY_SPACE, false));
    m.insert('\n', (K::KEY_ENTER, false));
    m.insert('\t', (K::KEY_TAB, false));
    m.insert('`', (K::KEY_GRAVE, false));
    m.insert('~', (K::KEY_GRAVE, true));
    m.insert('-', (K::KEY_MINUS, false));
    m.insert('_', (K::KEY_MINUS, true));
    m.insert('=', (K::KEY_EQUAL, false));
    m.insert('+', (K::KEY_EQUAL, true));
    m.insert('[', (K::KEY_LEFTBRACE, false));
    m.insert('{', (K::KEY_LEFTBRACE, true));
    m.insert(']', (K::KEY_RIGHTBRACE, false));
    m.insert('}', (K::KEY_RIGHTBRACE, true));
    m.insert('\\', (K::KEY_BACKSLASH, false));
    m.insert('|', (K::KEY_BACKSLASH, true));
    m.insert(';', (K::KEY_SEMICOLON, false));
    m.insert(':', (K::KEY_SEMICOLON, true));
    m.insert('\'', (K::KEY_APOSTROPHE, false));
    m.insert('"', (K::KEY_APOSTROPHE, true));
    m.insert(',', (K::KEY_COMMA, false));
    m.insert('<', (K::KEY_COMMA, true));
    m.insert('.', (K::KEY_DOT, false));
    m.insert('>', (K::KEY_DOT, true));
    m.insert('/', (K::KEY_SLASH, false));
    m.insert('?', (K::KEY_SLASH, true));
    m.insert('!', (K::KEY_1, true));
    m.insert('@', (K::KEY_2, true));
    m.insert('#', (K::KEY_3, true));
    m.insert('$', (K::KEY_4, true));
    m.insert('%', (K::KEY_5, true));
    m.insert('^', (K::KEY_6, true));
    m.insert('&', (K::KEY_7, true));
    m.insert('*', (K::KEY_8, true));
    m.insert('(', (K::KEY_9, true));
    m.insert(')', (K::KEY_0, true));
    m
}

struct UinputTyper {
    device: VirtualDevice,
    map: HashMap<char, (KeyCode, bool)>,
}

impl UinputTyper {
    fn new() -> std::io::Result<Self> {
        let map = char_map();
        let mut keys = AttributeSet::<KeyCode>::new();
        for &(code, _) in map.values() {
            keys.insert(code);
        }
        keys.insert(KeyCode::KEY_LEFTSHIFT);

        let device = VirtualDevice::builder()?
            .name("dictation-uinput")
            .with_keys(&keys)?
            .build()?;
        // Let the kernel register the new device before use.
        sleep(Duration::from_millis(500));

        Ok(UinputTyper { device, map })
    }

    fn type_text(&mut self, text: &str) {
        let mut shift = false;
        let mut dropped: Vec<char> = Vec::new();

        for ch in text.chars() {
            let Some(&(code, need_shift)) = self.map.get(&ch) else {
                dropped.push(ch);
                continue;
            };

            if need_shift != shift {
                let _ = self
                    .device
                    .emit(&[*evdev::KeyEvent::new(KeyCode::KEY_LEFTSHIFT, if need_shift { 1 } else { 0 })]);
                shift = need_shift;
                sleep(MODIFIER_SETTLE);
            }

            let _ = self.device.emit(&[*evdev::KeyEvent::new(code, 1)]);
            sleep(KEY_HOLD);
            let _ = self.device.emit(&[*evdev::KeyEvent::new(code, 0)]);
            sleep(INTER_CHAR);
        }

        if shift {
            let _ = self
                .device
                .emit(&[*evdev::KeyEvent::new(KeyCode::KEY_LEFTSHIFT, 0)]);
        }

        if !dropped.is_empty() {
            eprintln!("[typer] dropped unmappable characters: {dropped:?}");
        }
    }
}

// ── Public interface ────────────────────────────────────────────────────────

pub struct Typer {
    mutter: Option<MutterKeyboard>,
    uinput: Option<UinputTyper>,
}

impl Typer {
    pub fn new() -> Result<Self, String> {
        match MutterKeyboard::new() {
            Ok(mutter) => {
                println!("[typer] using Mutter RemoteDesktop keysym injection");
                Ok(Typer {
                    mutter: Some(mutter),
                    uinput: None,
                })
            }
            Err(err) => {
                eprintln!("[typer] Mutter RemoteDesktop unavailable ({err}); using uinput typing");
                let uinput = UinputTyper::new()
                    .map_err(|e| format!("neither Mutter RemoteDesktop nor uinput available: {e}"))?;
                Ok(Typer {
                    mutter: None,
                    uinput: Some(uinput),
                })
            }
        }
    }

    /// Types `text` into whatever window currently has focus. Must be called
    /// from a dedicated thread, never GPUI's main thread -- this blocks for
    /// the duration of any fallback uinput timing delays and D-Bus round
    /// trips. Returns an error only if every backend failed.
    pub fn type_text(&mut self, text: &str) -> Result<(), String> {
        let text = normalize(text);
        if text.is_empty() {
            return Ok(());
        }

        if let Some(mutter) = self.mutter.take() {
            match mutter.type_text(&text) {
                Ok(()) => {
                    self.mutter = Some(mutter);
                    return Ok(());
                }
                Err(err) => {
                    // Session died (e.g. gnome-shell restart) -- one
                    // reconnect attempt, then permanent fallback to uinput.
                    eprintln!("[typer] keysym injection failed ({err}); recreating session");
                    mutter.close();
                    match MutterKeyboard::new() {
                        Ok(new_mutter) => {
                            let result = new_mutter.type_text(&text);
                            self.mutter = Some(new_mutter);
                            if result.is_ok() {
                                return Ok(());
                            }
                            // Fresh session and still failing: give up on Mutter.
                            eprintln!("[typer] injection failed again; falling back to uinput typing");
                            if let Some(m) = self.mutter.take() {
                                m.close();
                            }
                        }
                        Err(err2) => {
                            eprintln!("[typer] reconnect failed ({err2}); falling back to uinput typing");
                        }
                    }
                }
            }
        }

        if self.uinput.is_none() {
            let uinput = UinputTyper::new()
                .map_err(|e| format!("failed to create uinput device: {e}"))?;
            self.uinput = Some(uinput);
        }
        self.uinput.as_mut().unwrap().type_text(&text);
        Ok(())
    }

    pub fn close(&mut self) {
        if let Some(mutter) = self.mutter.take() {
            mutter.close();
        }
    }
}
