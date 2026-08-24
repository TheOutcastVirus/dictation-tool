# dictation

Hold **Right Alt** to record your voice. Release to transcribe and type the
result into the active window. Runs locally on the GPU -- no network calls.

One Rust binary owns the whole pipeline: hotkey capture, microphone capture,
in-process Whisper inference (whisper.cpp via ROCm/HIP), text injection, and
a GPUI companion window (history, model picker, VRAM readout, run-at-login
toggle) plus a floating "Listening / Transcribing" bubble.

## Requirements

- GNOME / Mutter (X11 or Wayland session -- text is injected through
  Mutter's private `org.gnome.Mutter.RemoteDesktop` D-Bus API, no
  permission dialog; falls back to uinput typing elsewhere)
- Your user in the `input` group (evdev hotkey capture; uinput fallback)
- An AMD GPU with ROCm/HIP installed (`hipcc` on the PATH at build time)
- At least one Whisper model in `whisper.cpp/models/` (default
  `ggml-medium.en.bin`); see **Models** below
- Rust stable toolchain, Vulkan (for GPUI's renderer), `libxkbcommon-x11-dev`

## Build

```bash
env -u LD_PRELOAD -u LD_LIBRARY_PATH AMDGPU_TARGETS=gfx1030 cargo build --release
```

Two build notes specific to this machine:

- `whisper-rs-sys` compiles its own copy of whisper.cpp with `hipcc`. This
  shell has a stray global `LD_PRELOAD` / ROS `LD_LIBRARY_PATH` that gets
  injected into the compiler itself and crashes it ("pure virtual method
  called"). Unsetting both for the build fixes it; the resulting binary
  does not need the workaround at runtime.
- `AMDGPU_TARGETS=gfx1030` pins the discrete GPU target (the iGPU is
  `gfx1036` and not worth compiling for).

## Run

```bash
./target/release/dictation-tool
```

- First launch writes `~/.config/systemd/user/dictation-tool.service`
  (pointing at the binary you launched) and reloads the user manager.
  Flip **Run at login** in Settings to enable it.
- The binary is single-instance (session-bus name `dev.dictation.Tool`).
  Launching it again just raises the running instance's window.
- Closing the main window does **not** stop the daemon -- the hotkey keeps
  working. Relaunch the binary to get the window back; `systemctl --user
  stop dictation-tool` (or `kill`) stops it.
- The overlay bubble appears at the bottom of the primary display while
  recording (live trace) and while transcribing (spinner); it disappears
  only after the text has been typed.
- The main window has no system titlebar. Its own header is the drag
  handle, right-clicking it opens the window menu, and the window resizes
  from a 5 px band along its edges.

## Models

Anything named `ggml-*.bin` in `whisper.cpp/models/` shows up in Settings
with its size; picking one reloads it in place and remembers the choice.
Fetch more from the whisper.cpp model repo:

```bash
cd whisper.cpp/models
B=https://huggingface.co/ggerganov/whisper.cpp/resolve/main
curl -L -O $B/ggml-small.en.bin        # 466 MB, fastest usable
curl -L -O $B/ggml-large-v3-turbo.bin  # 1.6 GB, best quality
```

The status bar's VRAM figure is this process's own resident VRAM on the
discrete card, read per DRM client from `/proc/self/fdinfo`: the model
weights plus whisper's compute buffers, not how full the card is overall.

Config: `~/.config/dictation-tool/config.toml` (`model = "<file>"`).
History: `~/.local/share/dictation-tool/dictation.jsonl` -- one JSON object
per line, the same schema the earlier Python version wrote, so old entries
still show up in the History tab. Deleting an entry rewrites that file
(write-then-rename) and reloads it, so a dictation logged while the window
was open is never lost.

## Stack

| Component | Library |
|-----------|---------|
| UI | GPUI 0.2 (Zed's UI framework) |
| Speech-to-text | whisper-rs 0.16 (`hipblas`), model resident in VRAM |
| Audio capture | cpal, 16 kHz mono f32 (resampled if the device refuses) |
| Hotkey | evdev, one listener thread per physical keyboard |
| Text output | Mutter RemoteDesktop keysyms via zbus; uinput fallback |
| Single instance | zbus well-known name |
| Type | Berenis ADF Pro for prose, Go Mono for measurements |

Text injection sends every keysym event except the last with
`NoReplyExpected`, so the whole utterance lands as one burst (~6 ms for 80
chars) and appears at once like a paste. Mutter resolves each keysym to
keycode + modifiers on its own input thread, which is what makes it immune
to the uinput modifier-timing race that corrupted "Hello" into "hEllo".

## Layout

```
src/
  main.rs          entry: single-instance guard, engine thread, GPUI app, event bridge
  engine.rs        background orchestrator: hotkey -> record -> transcribe -> log -> type
  hotkey.rs        evdev Right-Alt hold detection
  audio.rs         cpal capture + RMS level stream
  transcribe.rs    whisper-rs wrapper (load / switch / transcribe)
  inject.rs        Mutter keysym injection + uinput fallback
  logger.rs        JSONL history writer
  history.rs       JSONL tailer for the History tab
  config.rs        config.toml
  vram.rs          per-process AMD VRAM reader (/proc/self/fdinfo)
  autostart.rs     systemd user unit install / enable
  instance.rs      D-Bus single-instance guard
  state.rs         AppState + engine event/command enums
  ui/              main window, history view, settings view, status bar,
                   theme, waveform (the trace painter)
  overlay/         floating bubble, spinner
```

## The interface

The window is a recorder, so it is built like one. Everything is warm ink
and bone type separated by value alone, and the only colour in the app is
the record light: the lamp in the status bar, the trace while it is live,
and an error. Nothing else is allowed to be coloured, so anything that is
means something is happening.

The strip under the header is the signature and it always shows real
audio. It runs live while you hold the key, and afterwards it holds the
envelope of what you just said, resampled to fill the band, with the
length beside it. Before the first dictation of a session there is no
audio to draw, so it carries the one instruction the app has instead.

Prose is set in Berenis ADF Pro (Arkandis Digital Foundry) because the
content of this app is prose somebody spoke. Go Mono (Bigelow & Holmes)
is reserved for genuine machine data -- timestamps, durations,
milliseconds, byte counts, paths -- and never for running text.
