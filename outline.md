# Local Dictation Tool — Outline

## Stack
- **whisper** (OpenAI) — local speech-to-text
- **sounddevice** + **soundfile** — audio capture
- **pynput** or **keyboard** — global hotkey listener
- **xdotool** / **xclip** (Linux) — type/paste output into active window

## Flow
1. User presses hotkey (e.g. `Ctrl+Alt+Space`) → recording starts
2. User speaks, releases hotkey → recording stops
3. Audio buffer passed to Whisper → transcribed to text
4. Text written to `output.md` (appended with timestamp)
5. Optionally: text also typed/pasted into the active window

## File Structure
```
dictation/
├── main.py          # entry point, hotkey listener loop
├── recorder.py      # audio capture with sounddevice
├── transcriber.py   # whisper model wrapper
├── output.py        # writes/appends to output.md
└── output.md        # transcription output
```

## Key Decisions to Make
- **Whisper model size**: `tiny`/`base` = fast, `medium`/`large` = accurate
- **Hotkey style**: hold-to-record vs. toggle (press once to start, again to stop)
- **Output behavior**: append-only log vs. overwrite each time
- **Active window typing**: auto-paste after transcription, or just write to file
