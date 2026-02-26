#!/usr/bin/env python3
"""
Dictation daemon — hold Right Alt to record, release to transcribe and paste.
Works on Wayland via evdev + wl-copy + uinput Ctrl+V.
"""

import subprocess
import threading
import time

import evdev
from evdev import ecodes

from indicator import Indicator
from recorder import Recorder
from transcriber import Transcriber

# ── Configuration ──────────────────────────────────────────────────────────────
HOTKEY_CODE = ecodes.KEY_RIGHTALT
MIN_DURATION = 0.3  # seconds; ignore accidental taps shorter than this
# ───────────────────────────────────────────────────────────────────────────────

indicator = Indicator()
recorder = Recorder()
transcriber: Transcriber | None = None  # loaded in background thread
_uinput: evdev.UInput | None = None     # created once at startup

_press_time: float = 0.0
_recording = False
_lock = threading.Lock()


def _paste(text: str):
    """Copy text to clipboard via wl-copy, then inject Ctrl+V via uinput."""
    proc = subprocess.Popen(["wl-copy"], stdin=subprocess.PIPE)
    proc.communicate(input=text.encode())
    time.sleep(0.1)

    _uinput.write(ecodes.EV_KEY, ecodes.KEY_LEFTCTRL, 1)
    _uinput.syn()
    _uinput.write(ecodes.EV_KEY, ecodes.KEY_V, 1)
    _uinput.syn()
    _uinput.write(ecodes.EV_KEY, ecodes.KEY_V, 0)
    _uinput.syn()
    _uinput.write(ecodes.EV_KEY, ecodes.KEY_LEFTCTRL, 0)
    _uinput.syn()


VIRTUAL_NAME_FRAGMENTS = ("virtual", "ydotool", "uinput", "dictation")

def _find_keyboards():
    """Return physical keyboard evdev devices (skip virtual ones)."""
    devices = []
    for path in evdev.list_devices():
        try:
            dev = evdev.InputDevice(path)
            if any(f in dev.name.lower() for f in VIRTUAL_NAME_FRAGMENTS):
                continue
            cap = dev.capabilities()
            if ecodes.EV_KEY in cap and ecodes.KEY_A in cap[ecodes.EV_KEY]:
                devices.append(dev)
        except Exception:
            pass
    return devices


def _listen_device(dev: evdev.InputDevice):
    """Read events from one keyboard device."""
    global _press_time, _recording
    try:
        for event in dev.read_loop():
            if event.type != ecodes.EV_KEY:
                continue
            if event.code != HOTKEY_CODE:
                continue

            if event.value == 1:  # key down
                with _lock:
                    if _recording:
                        continue
                    if transcriber is None:
                        print("Model not ready yet, please wait...", flush=True)
                        continue
                    _recording = True
                    _press_time = time.monotonic()
                indicator.show()
                recorder.start()

            elif event.value == 0:  # key up
                with _lock:
                    if not _recording:
                        continue
                    _recording = False
                    duration = time.monotonic() - _press_time
                indicator.hide()
                audio = recorder.stop()

                if duration < MIN_DURATION:
                    print(f"Too short ({duration:.2f}s), ignoring.", flush=True)
                    continue

                def transcribe_and_paste(audio=audio):
                    print("Transcribing...", flush=True)
                    text = transcriber.transcribe(audio)
                    print(f"Result: {text!r}", flush=True)
                    if text:
                        _paste(text)

                threading.Thread(target=transcribe_and_paste, daemon=True).start()

    except OSError as e:
        print(f"[device lost] {dev.path}: {e}", flush=True)


def _load_model():
    global transcriber
    transcriber = Transcriber()
    print("Ready. Hold Right Alt to dictate.", flush=True)


def main():
    global _uinput

    indicator.start()

    # Create UInput device once at startup so the compositor has time to register it
    # No capability filter — full keyboard so GNOME treats it as trusted
    _uinput = evdev.UInput(name="dictation-paste")
    time.sleep(1)  # give compositor time to register the device
    print("UInput device ready.", flush=True)

    threading.Thread(target=_load_model, daemon=True).start()

    keyboards = _find_keyboards()
    if not keyboards:
        print("ERROR: No keyboard devices found. Are you in the 'input' group?")
        print("Run: sudo usermod -aG input $USER  then log out and back in.")
        return

    print(f"Listening on {len(keyboards)} device(s):", flush=True)
    for dev in keyboards:
        print(f"  {dev.path}  {dev.name}", flush=True)
        threading.Thread(target=_listen_device, args=(dev,), daemon=True).start()

    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        pass
    finally:
        _uinput.close()


if __name__ == "__main__":
    main()
