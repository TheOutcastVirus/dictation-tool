import atexit
import http.client
import io
import json as _json
import os
import subprocess
import sys
import time
import uuid
import wave

import numpy as np

_DIR = os.path.dirname(os.path.abspath(__file__))
_SERVER = os.path.join(_DIR, "whisper.cpp", "build", "bin", "whisper-server")
_MODEL = os.path.join(_DIR, "whisper.cpp", "models", "ggml-medium.en.bin")
_HOST = "127.0.0.1"
_PORT = 8178


def _wav_bytes(audio: np.ndarray) -> bytes:
    pcm = (audio * 32767).clip(-32768, 32767).astype(np.int16)
    buf = io.BytesIO()
    with wave.open(buf, "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(16000)
        wf.writeframes(pcm.tobytes())
    return buf.getvalue()


def _multipart(file_data: bytes, language: str = "en") -> tuple[bytes, str]:
    b = uuid.uuid4().hex
    body = (
        f"--{b}\r\n"
        f'Content-Disposition: form-data; name="language"\r\n\r\n{language}\r\n'
        f"--{b}\r\n"
        f'Content-Disposition: form-data; name="file"; filename="audio.wav"\r\n'
        f"Content-Type: audio/wav\r\n\r\n"
    ).encode() + file_data + f"\r\n--{b}--\r\n".encode()
    return body, f"multipart/form-data; boundary={b}"


class Transcriber:
    def __init__(self):
        for path, label in [(_SERVER, "whisper-server"), (_MODEL, "model")]:
            if not os.path.isfile(path):
                print(f"ERROR: {label} not found at {path}", file=sys.stderr)
                sys.exit(1)

        self._proc = subprocess.Popen(
            [_SERVER, "-m", _MODEL, "-l", "en", "--host", _HOST, "--port", str(_PORT)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        atexit.register(self._proc.terminate)
        self._wait_ready()
        print("Whisper server ready (model in VRAM).")

    def _wait_ready(self):
        for _ in range(60):
            try:
                conn = http.client.HTTPConnection(_HOST, _PORT, timeout=1)
                conn.request("GET", "/health")
                if conn.getresponse().status == 200:
                    return
            except Exception:
                time.sleep(0.5)
        print("ERROR: whisper-server did not start in time", file=sys.stderr)
        sys.exit(1)

    def transcribe(self, audio: np.ndarray) -> str:
        body, content_type = _multipart(_wav_bytes(audio))
        conn = http.client.HTTPConnection(_HOST, _PORT)
        conn.request("POST", "/inference", body, {"Content-Type": content_type})
        text = _json.loads(conn.getresponse().read()).get("text", "")
        return " ".join(text.split())
