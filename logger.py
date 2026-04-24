import json
import os
from datetime import datetime

_LOG_DIR = os.path.join(os.path.expanduser("~"), ".local", "share", "dictation-tool")
_LOG_FILE = os.path.join(_LOG_DIR, "dictation.jsonl")


def log(text: str, audio_duration: float, transcribe_ms: int) -> None:
    os.makedirs(_LOG_DIR, exist_ok=True)
    entry = {
        "timestamp": datetime.now().isoformat(),
        "audio_duration_s": round(audio_duration, 2),
        "transcribe_ms": transcribe_ms,
        "word_count": len(text.split()),
        "char_count": len(text),
        "text": text,
    }
    with open(_LOG_FILE, "a") as f:
        f.write(json.dumps(entry) + "\n")
