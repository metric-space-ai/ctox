use super::*;
use tokio::{
    net::TcpListener,
    time::{sleep, timeout},
};
use tokio_tungstenite::accept_async;

#[test]
fn pcm_contract_bounds_and_formats() {
    let f = PcmFormat::default();
    assert!(f.validate().is_ok());
    assert!(f.validate_chunk(&vec![0; 3200]).is_ok());
    assert_eq!(
        f.validate_chunk(&vec![0; 3202]),
        Err(SpeechError::InvalidRequest)
    );
    assert_eq!(f.validate_chunk(&[0]), Err(SpeechError::InvalidRequest));
    assert_eq!(
        PcmFormat {
            sample_rate_hz: 123
        }
        .validate(),
        Err(SpeechError::InvalidRequest)
    );
}

#[test]
fn provider_audio_is_decoded_not_returned_as_json() {
    assert_eq!(
        decode_mistral_speech(br#"{"audio_data":"UklGRg=="}"#).unwrap(),
        b"RIFF"
    );
    for raw in [
        br#"{"audio_data":"%%%"}"#.as_slice(),
        br#"{"audio_data":""}"#,
        br#"{"text":"oops"}"#,
    ] {
        assert_eq!(
            decode_mistral_speech(raw),
            Err(SpeechError::InvalidResponse)
        );
    }
}

#[test]
fn typed_configuration_persists_and_rejects_unknown_fields() {
    let root = tempfile::tempdir().unwrap();
    let config = SpeechRuntimeConfig {
        synthesis: SpeechBackend::Mistral,
        transcription: SpeechBackend::Mistral,
        voice_id: Some("voice-test".into()),
    };
    config.save(root.path()).unwrap();
    assert_eq!(SpeechRuntimeConfig::load(root.path()).unwrap(), config);
    assert!(serde_json::from_str::<SpeechRuntimeConfig>(
        r#"{"synthesis":"mistral","transcription":"mistral","api_key":"secret"}"#
    )
    .is_err());
    let invalid = SpeechRuntimeConfig {
        voice_id: Some(" ".into()),
        ..config
    };
    assert!(invalid.save(root.path()).is_err());
}

#[test]
fn configure_file_validates_before_replacing_existing_selection() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("speech-config.json");
    let original = SpeechRuntimeConfig {
        synthesis: SpeechBackend::Mistral,
        transcription: SpeechBackend::Mistral,
        voice_id: Some("approved-voice".into()),
    };
    std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
    let status = configure_from_file(root.path(), &path).unwrap();
    assert_eq!(status.config, original);
    assert!(status.mistral_voice_configured);
    assert!(status.streaming_stt_selected);
    for invalid in [
        r#"{"synthesis":"mistyped","transcription":"runtime","voice_id":null}"#,
        r#"{"synthesis":"runtime","transcription":"runtime","api_key":"not-a-secret"}"#,
        r#"{"synthesis":"runtime","transcription":"runtime","voice_id":" "}"#,
        "{}",
    ] {
        std::fs::write(&path, invalid).unwrap();
        assert!(configure_from_file(root.path(), &path).is_err());
        assert_eq!(SpeechRuntimeConfig::load(root.path()).unwrap(), original);
    }
    std::fs::write(&path, vec![b' '; 4097]).unwrap();
    assert!(configure_from_file(root.path(), &path).is_err());
    assert_eq!(SpeechRuntimeConfig::load(root.path()).unwrap(), original);
}

#[test]
fn missing_saved_voice_fails_before_provider_transport() {
    let root = tempfile::tempdir().unwrap();
    let gateway = SpeechGateway {
        root: root.path().to_owned(),
        config: SpeechRuntimeConfig {
            synthesis: SpeechBackend::Mistral,
            transcription: SpeechBackend::Mistral,
            voice_id: None,
        },
    };
    assert!(!gateway.status().mistral_voice_configured);
    assert_eq!(
        gateway
            .synthesize(&SpeechRequest {
                text: "Hi".into(),
                format: SpeechAudioFormat::Wav,
                voice_id: None,
            })
            .err(),
        Some(SpeechError::MissingVoice)
    );
}

#[tokio::test]
async fn unavailable_streaming_fails_before_transport() {
    let root = tempfile::tempdir().unwrap();
    let gateway = SpeechGateway::from_root(root.path()).unwrap();
    assert!(!gateway.status().streaming_stt_selected);
    assert!(matches!(
        gateway.open_transcription(PcmFormat::default()).await,
        Err(SpeechError::UnsupportedBackend)
    ));
}

async fn fixture(mode: &'static str) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(tcp).await.unwrap();
        socket
            .send(Message::Text(
                json!({"type":"session.created","session":{}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        let update = socket.next().await.unwrap().unwrap().into_text().unwrap();
        let update: Value = serde_json::from_str(&update).unwrap();
        assert_eq!(update["session"]["audio_format"]["encoding"], "pcm_s16le");
        assert_eq!(update["session"]["target_streaming_delay_ms"], 240);
        socket
            .send(Message::Text(
                json!({"type":"session.updated","session":{}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        loop {
            let Some(Ok(message)) = socket.next().await else {
                break;
            };
            match message {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    match value["type"].as_str().unwrap() {
                        "input_audio.append" => {
                            assert_eq!(
                                BASE64
                                    .decode(value["audio"].as_str().unwrap())
                                    .unwrap()
                                    .len(),
                                640
                            );
                            if mode == "error" {
                                socket.send(Message::Text(json!({"type":"error","error":{"message":"secret-private-transcript"}}).to_string().into())).await.unwrap();
                                break;
                            }
                            let count = if mode == "overflow" { 40 } else { 1 };
                            for _ in 0..count {
                                if socket
                                    .send(Message::Text(
                                        json!({"type":"transcription.text.delta","text":"Hallo "})
                                            .to_string()
                                            .into(),
                                    ))
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                        "input_audio.flush" => {}
                        "input_audio.end" => {
                            // Controlled transport timing, NOT an actual provider measurement.
                            sleep(Duration::from_millis(45)).await;
                            socket
                                .send(Message::Text(
                                    json!({"type":"transcription.done","text":"Hallo Welt."})
                                        .to_string()
                                        .into(),
                                ))
                                .await
                                .unwrap();
                        }
                        _ => panic!("unexpected audio protocol"),
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });
    (format!("ws://{addr}"), task)
}

async fn start(endpoint: &str) -> TranscriptionStream {
    let socket = connect(endpoint, "fixture-only-not-a-real-secret")
        .await
        .unwrap();
    TranscriptionStream::open(socket, PcmFormat::default())
        .await
        .unwrap()
}

#[tokio::test]
async fn real_websocket_streams_partial_then_final_with_end_mark() {
    let (endpoint, server) = fixture("normal").await;
    let mut stream = start(&endpoint).await;
    stream.append_pcm(&[0; 640]).unwrap();
    assert!(matches!(
        timeout(IO_TIMEOUT, stream.next_event())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        TranscriptEvent::Partial { sequence: 1, .. }
    ));
    stream.finish_audio().unwrap();
    assert_eq!(stream.append_pcm(&[0; 640]), Err(SpeechError::Closed));
    let event = timeout(IO_TIMEOUT, stream.next_event())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match event {
        TranscriptEvent::Final {
            sequence,
            text,
            finish_to_final_ms,
            audio_duration_ms,
            ..
        } => {
            assert_eq!(sequence, 2);
            assert_eq!(text, "Hallo Welt.");
            assert_eq!(audio_duration_ms, 20);
            assert!(finish_to_final_ms.unwrap() >= 40);
        }
        _ => panic!("expected final"),
    }
    assert!(timeout(IO_TIMEOUT, stream.next_event())
        .await
        .unwrap()
        .is_none());
    timeout(IO_TIMEOUT, server).await.unwrap().unwrap();
}

#[tokio::test]
async fn provider_messages_are_redacted() {
    let (endpoint, server) = fixture("error").await;
    let mut stream = start(&endpoint).await;
    stream.append_pcm(&[0; 640]).unwrap();
    let error = timeout(IO_TIMEOUT, stream.next_event())
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error, SpeechError::ProviderRejected { http_status: None });
    assert!(!error.to_string().contains("secret"));
    timeout(IO_TIMEOUT, server).await.unwrap().unwrap();
}

#[tokio::test]
async fn dropping_session_closes_provider_connection() {
    let (endpoint, server) = fixture("normal").await;
    let stream = start(&endpoint).await;
    drop(stream);
    timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn cancel_closes_provider_connection() {
    let (endpoint, server) = fixture("normal").await;
    let stream = start(&endpoint).await;
    stream.cancel().await;
    timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn concurrent_streams_keep_transcripts_and_accounting_separate() {
    let (endpoint1, server1) = fixture("normal").await;
    let (endpoint2, server2) = fixture("normal").await;
    let (mut first, mut second) = tokio::join!(start(&endpoint1), start(&endpoint2));
    first.append_pcm(&[0; 640]).unwrap();
    second.append_pcm(&[0; 640]).unwrap();
    second.append_pcm(&[0; 640]).unwrap();
    first.finish_audio().unwrap();
    second.finish_audio().unwrap();
    async fn final_event(stream: &mut TranscriptionStream) -> (u64, u64) {
        loop {
            match timeout(IO_TIMEOUT, stream.next_event())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
            {
                TranscriptEvent::Final {
                    sequence,
                    audio_duration_ms,
                    ..
                } => return (sequence, audio_duration_ms),
                _ => {}
            }
        }
    }
    let (a, b) = tokio::join!(final_event(&mut first), final_event(&mut second));
    assert_eq!(a, (2, 20));
    assert_eq!(b, (3, 40));
    timeout(IO_TIMEOUT, server1).await.unwrap().unwrap();
    timeout(IO_TIMEOUT, server2).await.unwrap().unwrap();
}

#[tokio::test]
async fn event_backpressure_remains_an_explicit_terminal_error() {
    let (endpoint, server) = fixture("overflow").await;
    let mut stream = start(&endpoint).await;
    stream.append_pcm(&[0; 640]).unwrap();
    sleep(Duration::from_millis(200)).await;
    let mut count = 0;
    let mut terminal = None;
    while let Some(event) = timeout(IO_TIMEOUT, stream.next_event()).await.unwrap() {
        match event {
            Ok(_) => count += 1,
            Err(error) => terminal = Some(error),
        }
    }
    assert_eq!(count, 32);
    assert_eq!(terminal, Some(SpeechError::Backpressure));
    timeout(IO_TIMEOUT, server).await.unwrap().unwrap();
}
