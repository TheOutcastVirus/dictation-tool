# dictation

Hold **Right Alt** to record your voice. Release to transcribe and type the result into the active window. Runs locally — no network calls.

## Requirements

- Linux with Wayland
- `ydotool` + `ydotoold` daemon running
- User in the `input` group (`sudo usermod -aG input $USER`, then log out/in)
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
| Speech-to-text | [faster-whisper](https://github.com/SYSTRAN/faster-whisper) (`base.en`, CPU int8) |
| Audio capture | sounddevice + numpy |
| Keyboard input | evdev |
| Text output | ydotool |
| Recording indicator | tkinter |

## Files

```
main.py          # entry point — hotkey listener and orchestration
recorder.py      # audio capture
transcriber.py   # Whisper model wrapper
indicator.py     # "Recording..." UI overlay
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

> **Note:** `ydotoold` must be running before the service starts. The service file starts it via `ExecStartPre`, but you may need to adjust the path or run it separately.
