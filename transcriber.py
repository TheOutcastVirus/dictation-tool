import os
import subprocess
import sys
import tempfile
import wave

import numpy as np

_SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
_BINARY = os.path.join(_SCRIPT_DIR, "whisper.cpp", "build", "bin", "whisper-cli")
_MODEL = os.path.join(_SCRIPT_DIR, "whisper.cpp", "models", "ggml-medium.en.bin")


class Transcriber:
    def __init__(self):
        if not os.path.isfile(_BINARY):
            print(
                f"ERROR: whisper-cli not found at {_BINARY}\n"
                "Run: bash setup_whisper.sh",
                file=sys.stderr,
            )
            sys.exit(1)
        if not os.path.isfile(_MODEL):
            print(
                f"ERROR: model not found at {_MODEL}\n"
                "Run: bash setup_whisper.sh",
                file=sys.stderr,
            )
            sys.exit(1)
        print("Whisper.cpp (Vulkan) ready.")

    def transcribe(self, audio: np.ndarray) -> str:
        pcm = (audio * 32767).clip(-32768, 32767).astype(np.int16)
        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
            wav_path = f.name
        try:
            with wave.open(wav_path, "wb") as wf:
                wf.setnchannels(1)
                wf.setsampwidth(2)
                wf.setframerate(16000)
                wf.writeframes(pcm.tobytes())
            result = subprocess.run(
                [_BINARY, "-m", _MODEL, "-f", wav_path, "-l", "en", "-nt"],
                capture_output=True,
                text=True,
            )
            return result.stdout.strip()
        finally:
            os.unlink(wav_path)
