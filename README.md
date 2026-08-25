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
- An AMD GPU with the ROCm HIP SDK (`hipcc`, `rocblas`, `hipblas`); the
  runtime alone is not enough to build
- At least one Whisper model; see **Models** below
- Rust stable toolchain, Vulkan (for GPUI's renderer), `libxkbcommon-x11-dev`

## Install

```bash
./install.sh
```

Builds, then installs into the XDG user directories -- no root, nothing
outside `$HOME`:

| | |
|---|---|
| `~/.local/bin/dictation-tool` | the binary |
| `~/.local/share/applications/` | launcher, so **Dictation** appears in your applications |
| `~/.local/share/icons/hicolor/` | icon, for the launcher and the status tray |
| `~/.local/share/dictation-tool/models/` | models, moved out of the checkout |

Models live under the data dir rather than in the checkout so an installed
copy keeps working if the checkout moves or is deleted. `install.sh
--uninstall` removes everything and prints the one command that clears that
directory too; `--no-build` installs a binary you already built.

The script works out two things about the GPU on its own, so it stays
correct on hardware other than the machine it was written on:

- **Which architecture to build for.** rocBLAS ships prebuilt kernels per
  architecture and has none for some newer parts. gfx1152 is one: inference
  dies at the first matrix multiply unless the runtime presents the GPU as a
  sibling that *is* shipped. The script finds the nearest shipped
  architecture in the same ISA generation, builds for that, and writes the
  matching `HSA_OVERRIDE_GFX_VERSION` into the launcher -- the only place it
  can go, since a desktop launch inherits no login shell.
- **Whether `hipcc` can link.** Its clang picks the newest GCC directory it
  can see, which on Ubuntu may be one whose `libstdc++` development files
  were never installed (`ld.lld: unable to find library -lstdc++`). The
  script probes for that and wraps `hipcc` with `--gcc-install-dir` pointing
  at a GCC that has them. Installing `libstdc++-N-dev` for hipcc's preferred
  N also fixes it, permanently.

`LD_PRELOAD` / `LD_LIBRARY_PATH` are cleared for the build: whatever the
ambient shell exports (a ROS setup, for one) gets injected into the compiler
itself and can crash it. The built binary needs neither.

To build without installing:

```bash
env -u LD_PRELOAD -u LD_LIBRARY_PATH \
    PATH=/opt/rocm/bin:$PATH AMDGPU_TARGETS=<arch> \
    cargo build --release
```

## Run

Launch **Dictation** from your applications, or run `dictation-tool`.

- Closing the window with its **x** leaves the app running in the status
  tray: the hotkey keeps working with no window open. Click the tray icon to
  bring the window back, or use its **Quit** item to stop the daemon --
  closing the window never does.
- First launch writes `~/.config/systemd/user/dictation-tool.service`
  (pointing at the binary you launched, and carrying any
  `HSA_OVERRIDE_GFX_VERSION` it was started with) and reloads the user
  manager. Flip **Run at login** in Settings to enable it.
- The binary is single-instance (session-bus name `dev.dictation.Tool`).
  Launching it again just raises the running instance's window.
- The overlay bubble appears at the bottom of the primary display while
  recording (live trace) and while transcribing (spinner); it disappears
  only after the text has been typed.
- The main window has no system titlebar. Its own header is the drag
  handle, right-clicking it opens the window menu, and the window resizes
  from a 5 px band along its edges.
- GPUI is steered onto its X11 backend (XWayland under a Wayland session):
  its Wayland backend cannot stop the overlay bubble taking focus, and would
  type each transcription into the bubble instead of the window you meant.
  `DICTATION_FORCE_WAYLAND=1` opts out.

## Models

Anything named `ggml-*.bin` in the models directory shows up in Settings
with its size; picking one reloads it in place and remembers the choice.
That directory is `~/.local/share/dictation-tool/models/` once installed,
and `whisper.cpp/models/` in a checkout that has never been installed.
Fetch more from the whisper.cpp model repo:

```bash
cd ~/.local/share/dictation-tool/models
B=https://huggingface.co/ggerganov/whisper.cpp/resolve/main
curl -L -O $B/ggml-small.en.bin        # 466 MB, fastest usable
curl -L -O $B/ggml-large-v3-turbo.bin  # 1.6 GB, best quality
```

The status bar's memory figure is what *this process* holds on the card,
read per DRM client from `/proc/self/fdinfo`: the model weights plus
whisper's compute buffers, not how full the card is overall. Both of
amdgpu's pools count, because which one the weights land in depends on the
part -- a discrete card puts them in dedicated VRAM, an APU carves a small
VRAM window out of system RAM and lands the bulk in GTT. The capacity shown
beside the figure is whichever pool they actually went to.

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
| Status icon | ksni (StatusNotifierItem), sharing the zbus stack |
| Type | Berenis ADF Pro for prose, Go Mono for measurements |

Text injection fires each keysym event with `NoReplyExpected` rather than
waiting on a reply, but holds every key down for 6 ms before releasing it.
Mutter resolves a shifted keysym by wrapping it -- `Shift press, key press`
on the press, `key release, Shift release` on the release -- so a release
sent straight behind the press retracts the shift before the client has
applied it, and every capital and shifted punctuation mark is lost. The hold
is what makes the modifier land. See ISSUES.md.

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
  vram.rs          per-process AMD GPU memory reader (/proc/self/fdinfo)
  autostart.rs     systemd user unit install / enable
  instance.rs      D-Bus single-instance guard
  tray.rs          StatusNotifierItem status icon (show window / quit)
  state.rs         AppState + engine event/command enums
  ui/              main window, history view, settings view, status bar,
                   theme, waveform (the trace painter)
  overlay/         floating bubble, spinner
assets/            icon (hicolor SVG) and .desktop template
install.sh         build + install into the XDG user directories
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
