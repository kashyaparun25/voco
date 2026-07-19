//! Smoke test for the MOSS-Transcribe-Diarize finalize engine.
//!
//! Runs a real inference over a 16 kHz WAV and prints the parsed diarized
//! segments — exercises the same MossEngine path the meeting finalize uses.
//!
//! Usage:
//!   cargo run --release --example moss_smoke -- <model.gguf> <audio.wav> [en|zh]

#[cfg(feature = "moss")]
fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut args = std::env::args().skip(1);
    let model_path = args.next().expect("usage: moss_smoke <model.gguf> <audio.wav> [lang]");
    let wav_path = args.next().expect("usage: moss_smoke <model.gguf> <audio.wav> [lang]");
    let lang = args.next();

    let mut reader = hound::WavReader::open(&wav_path)?;
    let spec = reader.spec();
    anyhow::ensure!(
        spec.sample_rate == 16_000 && spec.channels == 1,
        "need 16 kHz mono WAV (got {} Hz, {} ch)",
        spec.sample_rate,
        spec.channels
    );
    let pcm: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|s| s.map(|v| v as f32 / 32768.0))
            .collect::<Result<_, _>>()?,
    };
    println!("audio: {:.1}s ({} samples)", pcm.len() as f64 / 16_000.0, pcm.len());

    let engine = voco_lib::stt::MossEngine::new(std::path::Path::new(&model_path))?;
    let t0 = std::time::Instant::now();
    let segs = engine.transcribe_diarized(&pcm, lang.as_deref())?;
    let dt = t0.elapsed();

    println!(
        "inference: {:.2}s ({:.1}x realtime), {} segments",
        dt.as_secs_f64(),
        (pcm.len() as f64 / 16_000.0) / dt.as_secs_f64(),
        segs.len()
    );
    for s in &segs {
        println!("[{:7.2} → {:7.2}] S{:02}  {}", s.start, s.end, s.speaker, s.text);
    }
    Ok(())
}

#[cfg(not(feature = "moss"))]
fn main() {
    eprintln!("build with --features moss");
}
