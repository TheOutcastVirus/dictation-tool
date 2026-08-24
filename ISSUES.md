# Dictation Tool -- Known Issues

## Tech Stack

| Component | Detail |
|---|---|
| Binary | single Rust process (`src/`), GPUI companion window |
| Input injection | Mutter `org.gnome.Mutter.RemoteDesktop` keysyms (primary), uinput (fallback) |
| Compositor | GNOME / Mutter, X11 or Wayland |
| ASR | whisper-rs (whisper.cpp, ROCm/HIP), in-process |
| Model | `ggml-medium.en.bin` by default; base.en / small.en / medium.en / large-v3-turbo switchable in Settings |
| Hotkey | Right Alt, hold-to-record, 300 ms minimum |
| Audio | cpal, 16 kHz mono float32, RMS level every 60 ms |
| Window | client-side decorations, no system titlebar |

---

## Resolved by the Rust port

- **Keystroke race corrupting capitalization** -- fixed, but not by keysym
  injection alone, which this file previously credited. Mutter wraps a
  shifted keysym as `Shift press, key press` / `key release, Shift release`,
  so a release sent immediately behind the press retracted the shift before
  the client had applied it: every capital and every shifted punctuation mark
  was lost on native Wayland clients (`?` arrived as `/`, `!` as `1`), while
  X11 clients were unaffected because XWayland tracks modifier state
  server-side. Each keysym is now held down for `KEYSYM_HOLD` (6 ms) before
  release. The uinput fallback keeps the empirically tuned delays from the old
  Python typer (3 ms / 1 ms / 1 ms).
- **Unicode dropped** -- partially. The uinput fallback logs anything it
  cannot map. The keysym path does *not* handle arbitrary codepoints, despite
  the earlier claim here; see Open below.
- **whisper-server crash not detected** -- there is no server any more; the
  model lives in-process. Inference errors are logged and reported in the
  status bar rather than killing a thread.
- **Recorder left running on device loss** -- recording state is owned by
  the engine loop, which always stops capture on key-up.
- **Indicator disappears before transcription finishes** -- the overlay
  now switches to a spinner on key-up and closes only after the text has
  been typed. It sits at the bottom-centre of the primary display.
- **Broken systemd unit** -- the binary writes a correct unit for its own
  path on first launch; no hardcoded `Documents/`, UID, or display vars
  (the GNOME session already imports `DISPLAY`/`WAYLAND_DISPLAY` into the
  user manager).
- **Two instances typing everything twice** -- single-instance guard via a
  session-bus name.

---

## Open

### Non-Latin-1 characters type nothing on the Mutter path

**Severity:** Low. `char_keysym` encodes anything outside ASCII/Latin-1 as a
Unicode keysym (`0x01000000 | codepoint`), but Mutter resolves a keysym by
looking for a keycode already carrying it in the active keymap, and a Unicode
keysym is not in one. Measured: a full sentence sent that way produces no
input at all -- silently, since the call still succeeds. `normalize()` folds
the characters whisper actually emits (smart quotes, dashes, ellipsis) down to
ASCII, so this is not reachable from ordinary dictation, but an emoji or a
non-Latin script would vanish rather than fall back. Fix: detect the
unmappable case and route those characters through the uinput backend, or have
Mutter remap a scratch keycode.

### Keyboard hotplug

**Severity:** Low. Keyboards are enumerated once at startup; a keyboard
plugged in later is not listened to until restart. Fix: watch
`/dev/input` with inotify/udev and spawn listeners on the fly.

### Recording while transcribing

**Severity:** Low. The engine loop is serial, so a Right Alt press while a
previous utterance is still transcribing/typing is queued until it
finishes (the Python version allowed overlap). Acceptable at ~0.5-1 s
transcription times; revisit if a larger model makes it noticeable.

### History view renders the newest 200 entries

**Severity:** Low. Rows are laid out in a plain scroll container, capped
at 200 to keep frames cheap. Switch to GPUI's `list()` with a `ListState`
for full virtualised scrolling if the full log needs to be browsable.

### Model switch peak VRAM

**Severity:** Low. `switch_model` loads the new context before dropping
the old one, so both are briefly resident. Fine for medium <-> small;
could OOM switching between two large models on a small card.

### Deleting history races an append

**Severity:** Low. `History::delete` re-reads the log, drops the matching
line, writes a temp file and renames it, then reloads. A dictation logged
between the read and the rename would be lost; the length check before the
rename catches that and abandons the delete, so the user clicks again.

### No client-side decorations without a compositor

**Severity:** Low. GPUI falls back to server-side decorations when no
compositor is running, and the window then has both a system titlebar and
its own header. The drag region and window buttons are omitted in that
case, so nothing is duplicated, but the titlebar is back.

### Overlay transparency on X11

**Severity:** Cosmetic. The bubble asks for a transparent window
background; if the X server / GPUI picks an opaque visual the rounded
corners show a dark square. Set `window_background` to `Opaque` in
`src/overlay/mod.rs` if that happens.
