use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

fn vram_used_mib() -> u64 {
    let bytes: u64 = std::fs::read_to_string("/sys/class/drm/card1/device/mem_info_vram_used")
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    bytes / 1024 / 1024
}

fn main() {
    let model_path = "whisper.cpp/models/ggml-medium.en.bin";
    let wav_path = "whisper.cpp/samples/jfk.wav";

    println!("VRAM before load: {} MiB", vram_used_mib());

    let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
        .expect("failed to load model");

    println!("VRAM after load:  {} MiB", vram_used_mib());

    let mut state = ctx.create_state().expect("failed to create state");

    let samples: Vec<i16> = hound::WavReader::open(wav_path)
        .unwrap()
        .into_samples::<i16>()
        .map(|x| x.unwrap())
        .collect();
    let mut inter_samples = vec![0f32; samples.len()];
    whisper_rs::convert_integer_to_float_audio(&samples, &mut inter_samples)
        .expect("failed to convert audio data");

    let mut params = FullParams::new(SamplingStrategy::BeamSearch {
        beam_size: 5,
        patience: -1.0,
    });
    params.set_language(Some("en"));
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);

    let t0 = std::time::Instant::now();
    state.full(params, &inter_samples[..]).expect("failed to run model");
    let elapsed = t0.elapsed();

    let mut text = String::new();
    for segment in state.as_iter() {
        text.push_str(&segment.to_string());
    }
    println!("Transcribed in {:?}: {:?}", elapsed, text.trim());
    println!("VRAM during/after inference: {} MiB", vram_used_mib());
}
