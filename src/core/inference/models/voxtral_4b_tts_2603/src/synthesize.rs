use ctox_voxtral_4b_tts_2603::{SpeechRequest, VoxtralTtsBackend, VoxtralTtsModel};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 5 {
        eprintln!("usage: ctox-voxtral-tts-synthesize <cpu|cuda> <model-dir> <output.wav> <preset-voice> <text>");
        std::process::exit(2);
    }
    let backend = match args[0].as_str() {
        "cpu" => VoxtralTtsBackend::Cpu,
        "cuda" => VoxtralTtsBackend::Cuda,
        _ => return Err("backend must be cpu or cuda".into()),
    };
    let start = std::time::Instant::now();
    let model = VoxtralTtsModel::from_model_dir(&args[1], backend)?;
    let load_ms = start.elapsed().as_millis();
    let request = SpeechRequest {
        input: &args[4],
        voice: Some(&args[3]),
        response_format: "wav",
    };
    let start = std::time::Instant::now();
    let output = model.synthesize(&request)?;
    let first_ms = start.elapsed().as_millis();
    std::fs::write(&args[2], &output.audio)?;
    let start = std::time::Instant::now();
    let second = model.synthesize(&request)?;
    let warm_ms = start.elapsed().as_millis();
    println!("{{\"ok\":true,\"load_ms\":{load_ms},\"complete_audio_ms\":{first_ms},\"warm_complete_audio_ms\":{warm_ms},\"audio_bytes\":{},\"warm_audio_bytes\":{},\"sample_rate\":24000,\"first_audio_streaming\":false}}", output.audio.len(), second.audio.len());
    Ok(())
}
