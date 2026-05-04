use ctox_voxtral_mini_4b_realtime_2602::{
    audio, TranscriptionRequest, VoxtralSttBackend, VoxtralSttConfig, VoxtralSttModel,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const BASE_URL: &str =
    "https://raw.githubusercontent.com/andrijdavid/voxtral.cpp/7deef66c8ee473d3ceffc57fb0cd17977eeebca9/samples";

const SAMPLES: &[(&str, &str)] = &[
    ("8297-275156-0000", "WHAT ARE YOU DOING HERE HE ASKED"),
    (
        "8297-275156-0001",
        "YOU HAVE BEEN TO THE HOTEL HE BURST OUT YOU HAVE SEEN CATHERINE",
    ),
    (
        "8297-275156-0002",
        "WE HAVE BOTH SEEN THE SAME NEWSPAPER OF COURSE AND YOU HAVE BEEN THE FIRST TO CLEAR THE THING UP THAT'S IT ISN'T IT",
    ),
];

#[test]
#[ignore = "downloads public LibriSpeech-derived demo samples from GitHub"]
fn online_librispeech_demo_samples_cover_audio_path() {
    let cache_dir = sample_cache_dir();
    std::fs::create_dir_all(&cache_dir).expect("create sample cache");
    let model = VoxtralSttModel::new(VoxtralSttConfig::default(), VoxtralSttBackend::Cpu);
    let preprocess = audio::MelSpectrogramPlan::default();
    let mut preprocessing_elapsed = Duration::ZERO;

    for (id, expected) in SAMPLES {
        let wav_path = ensure_sample(&cache_dir, id, "wav");
        let txt_path = ensure_sample(&cache_dir, id, "txt");
        let expected_from_source = std::fs::read_to_string(&txt_path)
            .expect("read expected transcript")
            .trim()
            .to_string();
        assert_eq!(normalize_transcript(&expected_from_source), *expected);

        let audio_bytes = std::fs::read(&wav_path).expect("read wav sample");
        let wav = audio::parse_wav(&audio_bytes).expect("parse wav sample");
        assert_eq!(wav.sample_rate, 16_000);
        assert_eq!(wav.channels, 1);
        assert!(!wav.samples.is_empty(), "sample {id} has no PCM samples");

        let start = Instant::now();
        let padded = audio::pad_audio_streaming(&wav.samples, 32, 17);
        assert_eq!(padded.len() % 1280, 0, "sample {id} is not token-aligned");
        let mel = preprocess.compute(&padded);
        preprocessing_elapsed += start.elapsed();
        assert!(!mel.is_empty(), "sample {id} produced no mel frames");

        match model.transcribe(&TranscriptionRequest {
            audio_bytes: &audio_bytes,
            response_format: "json",
            max_tokens: Some(128),
        }) {
            Ok(output) => {
                assert_eq!(
                    normalize_transcript(&output.text),
                    *expected,
                    "sample {id} transcript mismatch"
                );
            }
            Err(err) => {
                assert!(
                    err.to_string().contains("requires a Q4 GGUF"),
                    "sample {id} failed for unexpected reason: {err}"
                );
            }
        }
    }
    assert!(
        preprocessing_elapsed < Duration::from_secs(5),
        "online sample preprocessing took {preprocessing_elapsed:?}; this is too slow for meeting STT"
    );
}

#[test]
#[ignore = "loads the 2.3GB Q4 GGUF and runs full Voxtral inference"]
fn online_librispeech_demo_samples_transcribe_with_q4_gguf() {
    let gguf = match std::env::var("CTOX_VOXTRAL_STT_GGUF") {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => {
            eprintln!("Skipping: set CTOX_VOXTRAL_STT_GGUF to a Q4 Voxtral GGUF");
            return;
        }
    };
    let cache_dir = sample_cache_dir();
    std::fs::create_dir_all(&cache_dir).expect("create sample cache");
    let model = VoxtralSttModel::from_gguf(&gguf, VoxtralSttBackend::Wgsl).expect("load Q4 model");

    let mut audio_duration = Duration::ZERO;
    let start = Instant::now();
    for (id, expected) in SAMPLES {
        let wav_path = ensure_sample(&cache_dir, id, "wav");
        let audio_bytes = std::fs::read(&wav_path).expect("read wav sample");
        let wav = audio::parse_wav(&audio_bytes).expect("parse wav sample");
        audio_duration += Duration::from_secs_f64(wav.samples.len() as f64 / 16_000.0);

        let output = model
            .transcribe(&TranscriptionRequest {
                audio_bytes: &audio_bytes,
                response_format: "json",
                max_tokens: Some(128),
            })
            .expect("transcribe sample");
        assert_eq!(
            normalize_words(&output.text),
            normalize_words(expected),
            "sample {id} transcript mismatch"
        );
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs_f64() <= audio_duration.as_secs_f64() * 1.25,
        "Q4 transcription RTF too slow: elapsed={elapsed:?}, audio={audio_duration:?}"
    );
}

fn sample_cache_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("online-samples")
}

fn ensure_sample(cache_dir: &Path, id: &str, ext: &str) -> PathBuf {
    let path = cache_dir.join(format!("{id}.{ext}"));
    if path.is_file() {
        return path;
    }
    let url = format!("{BASE_URL}/{id}.{ext}");
    let status = Command::new("curl")
        .args(["-fsSL", "--retry", "3", "-o"])
        .arg(&path)
        .arg(&url)
        .status()
        .unwrap_or_else(|err| panic!("failed to run curl for {url}: {err}"));
    assert!(status.success(), "curl failed for {url}");
    path
}

fn normalize_transcript(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '\'' {
                ch.to_ascii_uppercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_words(text: &str) -> String {
    normalize_transcript(text)
        .replace("YOU'VE", "YOU HAVE")
        .replace("WE'VE", "WE HAVE")
        .replace("I'VE", "I HAVE")
        .replace("THEY'VE", "THEY HAVE")
}
