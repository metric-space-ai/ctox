//! Operator measurement candidate. This direct model-crate fixture is not an
//! installed meeting/gateway acceptance result.
use anyhow::{bail, Context, Result};
use ctox_voxtral_mini_4b_realtime_2602::{
    audio, TranscriptionRequest, VoxtralSttBackend, VoxtralSttModel,
};
use serde_json::json;
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn normalized(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        bail!("usage: stream_benchmark <cpu|cuda|metal> <model.gguf> <audio.wav>");
    }
    let backend = match args[0].as_str() {
        "cpu" => VoxtralSttBackend::Cpu,
        "cuda" => VoxtralSttBackend::Cuda,
        "metal" => VoxtralSttBackend::Metal,
        _ => bail!("unsupported backend"),
    };
    let wav_path = Path::new(&args[2]);
    anyhow::ensure!(
        std::fs::metadata(wav_path)?.len() <= 1_048_576,
        "fixture exceeds 1 MiB"
    );
    let bytes = std::fs::read(wav_path)?;
    let wav = audio::parse_wav(&bytes)?;
    anyhow::ensure!(
        !wav.samples.is_empty() && wav.samples.len() <= 15 * 16_000,
        "fixture must contain 0..15s audio"
    );
    let started = Instant::now();
    let model =
        VoxtralSttModel::from_gguf(&args[1], backend).context("load native STT candidate")?;
    let load_ms = started.elapsed().as_millis();
    let started = Instant::now();
    let batch = model.transcribe(&TranscriptionRequest {
        audio_bytes: &bytes,
        response_format: "json",
        max_tokens: Some(256),
    })?;
    let batch_ms = started.elapsed().as_millis();
    let mut stream = model.open_stream()?;
    let started = Instant::now();
    let mut partials = Vec::new();
    let mut delivered_samples = 0usize;
    let mut max_capture_backlog_ms = 0u128;
    let mut previous = String::new();
    for samples in wav.samples.chunks(320) {
        delivered_samples += samples.len();
        let capture_ready = Duration::from_secs_f64(delivered_samples as f64 / 16_000.0);
        if let Some(wait) = capture_ready.checked_sub(started.elapsed()) {
            std::thread::sleep(wait);
        }
        let pcm = samples
            .iter()
            .flat_map(|sample| ((*sample * 32768.0).clamp(-32768.0, 32767.0) as i16).to_le_bytes())
            .collect::<Vec<_>>();
        if let Some(text) = stream.append_pcm(&pcm)? {
            if text != previous {
                partials.push(json!({"text":text,"after_start_ms":started.elapsed().as_millis(),"captured_audio_ms":capture_ready.as_millis()}));
                previous = text;
            }
        }
        max_capture_backlog_ms =
            max_capture_backlog_ms.max(started.elapsed().saturating_sub(capture_ready).as_millis());
    }
    let finish_mark = Instant::now();
    let final_text = stream.finish()?;
    let finish_compute_ms = finish_mark.elapsed().as_millis();
    let stream_wall_ms = started.elapsed().as_millis();
    let audio_ms = (wav.samples.len() as u64) * 1000 / 16_000;
    let proof = json!({
        "backend":backend.label(), "model":model.config().model, "audio_path":args[2],
        "audio_duration_ms":audio_ms, "load_ms":load_ms, "batch_ms":batch_ms, "batch_text":batch.text,
        "stream_wall_ms":stream_wall_ms, "audio_file_end_to_final_ms":stream_wall_ms.saturating_sub(audio_ms as u128),
        "finish_compute_ms":finish_compute_ms, "max_capture_backlog_ms":max_capture_backlog_ms,
        "partial_snapshots":partials, "final_text":final_text,
        "batch_stream_normalized_parity":normalized(&batch.text) == normalized(&final_text),
        "through_gateway":false, "installed_meeting_acceptance":false,
        "timing_boundary":"end of the paced fixture, including any trailing silence; not microphone/VAD sentence-end"
    });
    println!("{}", serde_json::to_string(&proof)?);
    Ok(())
}
