# dictation

Hold **Right Alt** to record your voice. Release to transcribe and type the result into the active window. Runs locally — no network calls.

## Requirements

- Linux with Wayland (tested on GNOME 42 / Mutter)
- `/dev/uinput` writable by your user (the user must be in the `input` group, or have an ACL on `/dev/uinput`)
- A built `whisper.cpp` with `whisper-server` and a model at `whisper.cpp/models/ggml-medium.en.bin`
- Python 3.10+

## Setup

```bash
pip install -r requirements.txt
```

## Usage

```bash
python main.py
```

Hold **Right Alt** to start recording. Release to stop — the transcribed text will be typed at the cursor.

## Stack

| Component | Library |
|-----------|---------|
| Speech-to-text | whisper.cpp (`ggml-medium.en`, GPU via ROCm/HIP, persistent server) |
| Audio capture | sounddevice + numpy |
| Hotkey listener | evdev |
| Text output | Mutter RemoteDesktop keysym injection (uinput typing as fallback) |
| Recording indicator | tkinter |

Text is injected as keysyms ("capital H") through Mutter's private
`org.gnome.Mutter.RemoteDesktop` D-Bus API — the same interface
gnome-remote-desktop uses. Mutter resolves each keysym to keycode +
modifiers on its own input thread, so the modifier-state race that
corrupts uinput typing ("Hello" -> "hEllo") cannot happen, arbitrary
Unicode works, and no inter-key delays are needed. No permission dialog
is shown.

On non-GNOME compositors (or if the D-Bus session fails) the typer falls
back to character-by-character uinput typing with conservative delays
(~80 cps) to stay clear of the compositor's xkbcommon modifier-state
race. Tuning for the fallback lives in `typer.py`.

## Files

```
main.py          # entry point — hotkey listener and orchestration
recorder.py      # audio capture
transcriber.py   # whisper.cpp server wrapper
typer.py         # text injection: Mutter keysym API, uinput fallback
indicator.py     # "Recording..." UI overlay
logger.py        # JSONL transcription log
requirements.txt # Python dependencies
dictation.service # optional systemd user service
```

## Running as a service (optional)

Copy and enable the included systemd user service:

```bash
cp dictation.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now dictation
```

> **Note:** the unit sets `WAYLAND_DISPLAY` and `XDG_RUNTIME_DIR` so the indicator window can reach the user's compositor.
