# Dictation Tool — Known Issues

## Tech Stack

| Component | Detail |
|---|---|
| Input injection | `evdev` / `uinput` (kernel virtual device) |
| Compositor | GNOME/Mutter on Wayland |
| ASR backend | `whisper.cpp` HTTP server (ROCm/HIP GPU) |
| Model | `ggml-medium.en.bin` |
| Hotkey | Right Alt (hold-to-record) |
| Audio | `sounddevice` → 16 kHz mono float32 PCM |

---

## Issue 1 — Keystroke injection races causing capitalization/punctuation corruption

**Severity:** High  
**Status:** Partially mitigated, not fully resolved  
**File:** `typer.py`

### Description

Per-character keystroke injection via uinput is racy under GNOME/Mutter on Wayland.
The compositor's `xkbcommon` modifier state updates asynchronously relative to
incoming uinput events. When Shift press/release events and key-down events arrive
faster than the compositor can process them, the modifier state is misread and
characters come out wrong — e.g., `"Hello"` becomes `"hEllo"`, `"?"` is injected
as `"/"`, etc.

### Root cause

`xkbcommon` state updates happen on the compositor's input thread; uinput events
are injected on a separate kernel path with no synchronisation barrier. There is
no API to wait for the compositor to acknowledge a modifier change before sending
the next key event.

### Current mitigation

Three timing constants in `typer.py` add deliberate sleeps:

```python
_MODIFIER_SETTLE_S = 0.003   # wait after Shift press/release
_KEY_HOLD_S        = 0.001   # hold key before releasing it
_INTER_CHAR_S      = 0.001   # gap between characters
```

At ~80 characters/second this makes a typical dictated sentence (~80 chars) take
roughly 1 second to type. This reduces but does not eliminate corruption on
heavily loaded systems or high refresh-rate compositors.

### Remaining risk

- Under CPU/GPU load the compositor may fall further behind, requiring even larger
  `_MODIFIER_SETTLE_S`.
- The values were found empirically; no lower bound is guaranteed to be safe
  across all GNOME versions or hardware.
- Consecutive shifted characters (e.g., `"HTTP"`, `"I'll"`) share a single Shift
  hold and are therefore not affected, but alternating case (e.g., `"iPhone"`)
  requires two Shift transitions and is highest risk.

### Potential fix — clipboard paste

Copy the transcribed text to the clipboard and inject `Ctrl+V` instead of typing
character-by-character. This sends only two keystrokes regardless of text length
and completely sidesteps the Shift-timing issue.

**Obstacle:** the user's existing clipboard contents would be overwritten.
Saving and restoring clipboard content is possible for plain text via `wl-clipboard`
(`wl-copy` / `wl-paste`), but clipboard entries that contain images or other
binary MIME types are harder to round-trip reliably because `wl-copy` can only
hold one MIME type at a time without a persistent clipboard manager in the loop.

---

## Issue 2 — No clipboard save/restore for paste approach

**Severity:** Medium (blocks clipboard-paste fix above)  
**Status:** Open

### Description

If clipboard injection is used (`wl-copy text && xdg-inject Ctrl+V`) the
previous clipboard contents are destroyed. For plain-text clipboard entries,
a save/restore cycle (`wl-paste` before, `wl-copy` after) works.
For non-text types (images, rich text, files) `wl-paste` does not reliably
capture all MIME types; restoring them would require a full clipboard-manager
daemon (e.g. `cliphist`, `wl-clip-persist`) to already be running.

---

## Issue 3 — Unknown / unsupported Unicode characters are silently dropped

**Severity:** Medium  
**File:** `typer.py:93–94`

### Description

If Whisper produces a character not in `_CHAR_MAP` (accented letters, emoji,
non-Latin script, currency symbols, etc.) the character is silently skipped:

```python
entry = _CHAR_MAP.get(ch)
if entry is None:
    continue   # ← character vanishes without warning
```

`_NORMALIZE` handles a small set of curly quotes and dashes, but nothing else.
The user gets no indication that part of their dictation was lost.

### Potential fix

Log dropped characters, or fall back to clipboard paste for any text that
contains unmappable characters.

---

## Issue 4 — Whisper server process not restarted on crash

**Severity:** Medium  
**File:** `transcriber.py:71–73`

### Description

The `_wait_ready` method checks for an unexpected server exit during startup,
but once the server is running there is no watchdog. If `whisper-server` crashes
mid-session, subsequent `transcribe()` calls will raise a `ConnectionRefusedError`
which is not caught, killing the transcription thread silently (daemon thread,
unhandled exception is printed but the daemon keeps the process alive in a broken
state).

### Potential fix

Wrap `transcribe()` in a try/except that detects connection failure, respawns the
server, and retries the request.

---

## Issue 5 — Audio frames list not reset on device disconnect/reconnect

**Severity:** Low  
**File:** `recorder.py` / `main.py:98–99`

### Description

If an evdev keyboard device is lost mid-recording (`OSError` in `_listen_device`),
the recorder is left in a started state. `recorder.stop()` is never called,
so `_frames` accumulates audio indefinitely if `start()` is somehow called again,
and the sounddevice stream leaks.

---

## Issue 6 — Single keyboard grab — no device hotplug support

**Severity:** Low  
**File:** `main.py:114–123`

### Description

`_find_keyboards()` is called once at startup. USB keyboards plugged in after the
daemon starts, or Bluetooth keyboards that reconnect, are never detected.
One thread per device is spawned; if a device is lost the thread exits but is
never replaced.

---

## Proposed Priority Order

1. **Clipboard paste + save/restore** — eliminates Issue 1 entirely for plain-text
   clipboard, which is the common case. Track MIME type before overwriting.
2. **Dropped-character logging** — low effort, makes Issue 3 visible immediately.
3. **Transcriber watchdog** — prevents silent failures after GPU OOM or server
   crash (Issue 4).
4. **Recorder cleanup on device loss** — defensive fix for Issue 5.
5. **Device hotplug** — quality-of-life improvement for Issue 6.
