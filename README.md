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
| Text output | direct typing via uinput (deliberately slow to avoid Mutter's modifier-state race) |
| Recording indicator | tkinter |

Text is typed character-by-character through a uinput virtual keyboard. On
GNOME/Mutter Wayland, the compositor's xkbcommon modifier state updates
asynchronously relative to uinput events, which corrupts capitalization and
shifted punctuation when events arrive faster than ~1 ms apart. The typer
inserts ~8 ms between characters and ~20 ms after each Shift transition —
roughly 80 cps, slower than a paste but reliable. Tuning lives at the top
of `typer.py`.

## Files

```
main.py          # entry point — hotkey listener and orchestration
recorder.py      # audio capture
transcriber.py   # whisper.cpp server wrapper
typer.py         # uinput keystroke injection (with conservative delays)
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
