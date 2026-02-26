import threading
import numpy as np
import sounddevice as sd

SAMPLE_RATE = 16000


class Recorder:
    def __init__(self):
        self._frames = []
        self._lock = threading.Lock()
        self._stream = None

    def start(self):
        self._frames = []
        self._stream = sd.InputStream(
            samplerate=SAMPLE_RATE,
            channels=1,
            dtype="float32",
            callback=self._callback,
        )
        self._stream.start()

    def _callback(self, indata, frames, time, status):
        with self._lock:
            self._frames.append(indata.copy())

    def stop(self) -> np.ndarray:
        if self._stream is not None:
            self._stream.stop()
            self._stream.close()
            self._stream = None
        with self._lock:
            if self._frames:
                audio = np.concatenate(self._frames, axis=0).flatten()
            else:
                audio = np.zeros(SAMPLE_RATE, dtype="float32")
        return audio
