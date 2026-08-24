//! Port of `recorder.py`: mono 16 kHz f32 microphone capture via cpal, plus a
//! coalesced RMS level stream for the overlay's level meter.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

/// How often the capture thread emits an RMS level. The waveform band reads
/// this back to turn a sample count into elapsed seconds.
pub const LEVEL_INTERVAL_MS: u64 = 60;
use std::time::{Duration, Instant};

pub const SAMPLE_RATE: u32 = 16_000;

struct Capture {
    stream: cpal::Stream,
    /// Native rate/channels of the stream actually opened. If the device
    /// refused a 16 kHz mono stream we capture at its default format and
    /// convert on `stop()`.
    sample_rate: u32,
    channels: u16,
}

pub struct Recorder {
    capture: Option<Capture>,
    buffer: Arc<Mutex<Vec<f32>>>,
}

impl Recorder {
    pub fn new() -> Self {
        Recorder {
            capture: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Starts capturing. `on_level` is called from the audio callback thread
    /// with a coalesced RMS sample at roughly 15-20 Hz — callers must not
    /// block in it.
    pub fn start(&mut self, on_level: impl FnMut(f32) + Send + 'static) -> Result<(), String> {
        self.capture = None;
        self.buffer.lock().unwrap().clear();

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("no input audio device available")?;

        let on_level = Arc::new(Mutex::new(on_level));

        // Preferred: exactly what whisper wants, no conversion needed.
        let preferred = cpal::StreamConfig {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            buffer_size: cpal::BufferSize::Default,
        };
        match self.build(&device, preferred.clone(), on_level.clone()) {
            Ok(stream) => {
                self.capture = Some(Capture {
                    stream,
                    sample_rate: SAMPLE_RATE,
                    channels: 1,
                });
                return Ok(());
            }
            Err(err) => eprintln!("[audio] 16kHz mono stream unavailable ({err}); using device default"),
        }

        // Fallback: the device's native format, converted in `stop()`.
        let default = device
            .default_input_config()
            .map_err(|e| format!("no default input config: {e}"))?;
        let config = cpal::StreamConfig {
            channels: default.channels(),
            sample_rate: default.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };
        let stream = self.build(&device, config.clone(), on_level)?;
        self.capture = Some(Capture {
            stream,
            sample_rate: config.sample_rate,
            channels: config.channels,
        });
        Ok(())
    }

    fn build(
        &self,
        device: &cpal::Device,
        config: cpal::StreamConfig,
        on_level: Arc<Mutex<impl FnMut(f32) + Send + 'static>>,
    ) -> Result<cpal::Stream, String> {
        let buffer = self.buffer.clone();
        let mut window: Vec<f32> = Vec::new();
        let mut last_emit = Instant::now();
        let emit_every = Duration::from_millis(LEVEL_INTERVAL_MS);

        let stream = device
            .build_input_stream(
                config,
                move |data: &[f32], _| {
                    buffer.lock().unwrap().extend_from_slice(data);

                    window.extend_from_slice(data);
                    if last_emit.elapsed() >= emit_every {
                        let rms = if window.is_empty() {
                            0.0
                        } else {
                            (window.iter().map(|s| s * s).sum::<f32>() / window.len() as f32).sqrt()
                        };
                        (on_level.lock().unwrap())(rms);
                        window.clear();
                        last_emit = Instant::now();
                    }
                },
                move |err| eprintln!("[audio] stream error: {err}"),
                None,
            )
            .map_err(|e| format!("failed to build input stream: {e}"))?;

        stream
            .play()
            .map_err(|e| format!("failed to start audio stream: {e}"))?;
        Ok(stream)
    }

    /// Stops capturing and returns the recorded samples as mono 16 kHz f32.
    pub fn stop(&mut self) -> Vec<f32> {
        let Some(capture) = self.capture.take() else {
            return Vec::new();
        };
        drop(capture.stream); // dropping the stream stops it
        let raw = std::mem::take(&mut *self.buffer.lock().unwrap());
        to_mono_16k(raw, capture.channels, capture.sample_rate)
    }
}

fn to_mono_16k(raw: Vec<f32>, channels: u16, sample_rate: u32) -> Vec<f32> {
    let mono: Vec<f32> = if channels <= 1 {
        raw
    } else {
        let n = channels as usize;
        raw.chunks_exact(n)
            .map(|frame| frame.iter().sum::<f32>() / n as f32)
            .collect()
    };
    if sample_rate == SAMPLE_RATE || mono.is_empty() {
        return mono;
    }
    // Linear-interpolation resample. Adequate for speech going into whisper;
    // only reached when the device refused a native 16 kHz stream.
    let ratio = sample_rate as f64 / SAMPLE_RATE as f64;
    let out_len = ((mono.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = pos as usize;
        let frac = (pos - idx as f64) as f32;
        let a = mono[idx];
        let b = mono.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}
