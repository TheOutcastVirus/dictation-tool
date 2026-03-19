import numpy as np
from faster_whisper import WhisperModel

MODEL_SIZE = "small.en"


class Transcriber:
    def __init__(self):
        print(f"Loading Whisper model '{MODEL_SIZE}'...")
        self._model = WhisperModel(MODEL_SIZE, device="cpu", compute_type="int8")
        print("Model loaded.")

    def transcribe(self, audio: np.ndarray) -> str:
        segments, _info = self._model.transcribe(audio, language="en")
        text = " ".join(segment.text.strip() for segment in segments)
        return text.strip()
