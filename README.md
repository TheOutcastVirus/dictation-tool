# dictation

Hold **Right Alt** to record your voice. Release to transcribe and type the
result into the active window. Runs locally on the GPU -- no network calls.

One Rust binary owns the whole pipeline: hotkey capture, microphone capture,
in-process Whisper inference (whisper.cpp via ROCm/HIP), text injection, and
a small GPUI companion window (history, model picker, VRAM readout,
run-at-login toggle) plus a floating "Listening / Transcribing" bubble.

## Requirements

- GNOME / Mutter (X11 or Wayland session -- text is injected through
  Mutter's private `org.gnome.Mutter.RemoteDesktop` D-Bus API, no
  permission dialog; falls back to uinput typing elsewhere)
- Your user in the `input` group (evdev hotkey capture; uinput fallback)
- An AMD GPU with ROCm/HIP installed (`hipcc` on the PATH at build time)
- A Whisper model in `whisper.cpp/models/` (default `ggml-medium.en.bin`)
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
- The overlay bubble appears at the top of the primary display while
  recording (level meter) and while transcribing (spinner); it disappears
  only after the text has been typed.

Config: `~/.config/dictation-tool/config.toml` (`model = "<file>"`).
History: `~/.local/share/dictation-tool/dictation.jsonl` -- one JSON object
per line, the same schema the earlier Python version wrote, so old entries
still show up in the History tab.

## Stack

| Component | Library |
|-----------|---------|
| UI | GPUI 0.2 (Zed's UI framework) |
| Speech-to-text | whisper-rs 0.16 (`hipblas`), model resident in VRAM |
| Audio capture | cpal, 16 kHz mono f32 (resampled if the device refuses) |
| Hotkey | evdev, one listener thread per physical keyboard |
| Text output | Mutter RemoteDesktop keysyms via zbus; uinput fallback |
| Single instance | zbus well-known name |

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
  vram.rs          AMD sysfs VRAM reader
  autostart.rs     systemd user unit install / enable
  instance.rs      D-Bus single-instance guard
  state.rs         AppState + engine event/command enums
  ui/              main window, history view, settings view, status bar, theme
  overlay/         floating bubble, level meter, spinner
```
