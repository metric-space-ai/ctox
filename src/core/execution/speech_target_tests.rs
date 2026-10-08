// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
use std::sync::atomic::AtomicUsize;
use tokio::io::AsyncBufReadExt;

struct Current(AtomicBool);
impl WebRTCPublicationGuard for Current {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        if self.0.load(Ordering::Acquire) {
            publish()
        } else {
            Err(new_rx_error("TEST_REVOKED", None))
        }
    }
}

#[test]
fn pending_private_io_rechecks_authority_before_next_poll() {
    let guard = Current(AtomicBool::new(true));
    let effects = AtomicUsize::new(0);
    let first: Poll<io::Result<()>> = guarded_poll(&guard, || {
        effects.fetch_add(1, Ordering::Relaxed);
        Poll::Pending
    });
    assert!(first.is_pending());
    guard.0.store(false, Ordering::Release);
    let next: Poll<io::Result<()>> = guarded_poll(&guard, || {
        effects.fetch_add(1, Ordering::Relaxed);
        Poll::Ready(Ok(()))
    });
    assert!(
        matches!(next, Poll::Ready(Err(error)) if error.kind() == io::ErrorKind::PermissionDenied)
    );
    assert_eq!(effects.load(Ordering::Relaxed), 1);
}

#[test]
fn missing_or_repeated_guard_callback_cannot_publish_success() {
    struct Invalid(bool);
    impl WebRTCPublicationGuard for Invalid {
        fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
            if self.0 {
                publish()?;
                publish()?;
            }
            Ok(())
        }
    }
    for repeated in [false, true] {
        let effects = AtomicUsize::new(0);
        let result = guarded_poll(&Invalid(repeated), || {
            effects.fetch_add(1, Ordering::Relaxed);
            Poll::Ready(Ok(()))
        });
        assert!(matches!(result, Poll::Ready(Err(_))));
        assert_eq!(effects.load(Ordering::Relaxed), usize::from(repeated));
    }
}

#[tokio::test]
async fn queued_unix_audio_is_not_ingested_after_revocation() {
    let (local, mut runtime) = tokio::net::UnixStream::pair().unwrap();
    let authority = Arc::new(Current(AtomicBool::new(true)));
    let mut reader = BufReader::new(GuardedIo {
        inner: local,
        guard: authority.clone(),
    });
    let mut text = String::new();
    assert!(
        tokio::time::timeout(Duration::from_millis(10), reader.read_line(&mut text))
            .await
            .is_err()
    );
    authority.0.store(false, Ordering::Release);
    runtime
        .write_all(b"private audio response\n")
        .await
        .unwrap();
    let error = reader.read_line(&mut text).await.unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(text.is_empty());
}

fn fixture_wav() -> Vec<u8> {
    let data_bytes = 960u32; // 20 ms, mono PCM16 at 24 kHz; transport fixture only.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&24_000u32.to_le_bytes());
    bytes.extend_from_slice(&48_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    bytes.resize(44 + data_bytes as usize, 0);
    bytes
}

async fn tts_fixture(actual_model: String) -> Result<ReadyAudio, Denial> {
    use crate::inference::native_tts::{LocalTtsRequest, LocalTtsResponse};
    let (local, runtime) = tokio::net::UnixStream::pair().unwrap();
    let authority = Arc::new(Current(AtomicBool::new(true)));
    let socket: RuntimeSocket = Box::new(GuardedIo {
        inner: local,
        guard: authority,
    });
    let task = tokio::spawn(async move {
        let mut runtime = BufReader::new(runtime);
        let mut input = String::new();
        runtime.read_line(&mut input).await.unwrap();
        let request: LocalTtsRequest = serde_json::from_str(&input).unwrap();
        assert!(matches!(request, LocalTtsRequest::SpeechCreate {
            model: Some(model), input, voice: Some(voice), response_format: Some(format)
        } if model == "engineai/Voxtral-4B-TTS-2603" && input == "Hello." && voice == "fixture" && format == "wav"));
        let mut reply = serde_json::to_vec(&LocalTtsResponse::Speech {
            model: actual_model,
            audio_base64: BASE64.encode(fixture_wav()),
            response_format: "wav".into(),
        })
        .unwrap();
        reply.push(b'\n');
        runtime.get_mut().write_all(&reply).await.unwrap();
    });
    let result = synthesize_local(
        socket,
        "engineai/Voxtral-4B-TTS-2603".into(),
        "Hello.".into(),
        "fixture".into(),
    )
    .await;
    task.await.unwrap();
    result
}

#[tokio::test]
async fn tts_private_ipc_checks_the_actual_model_audio_and_digest() {
    let audio = tts_fixture("engineai/Voxtral-4B-TTS-2603".into())
        .await
        .unwrap_or_else(|reason| panic!("{reason:?}"));
    assert_eq!(audio.bytes, fixture_wav());
    assert_eq!(audio.duration_ms, 20);
    assert_eq!(audio.sha, format!("{:x}", Sha256::digest(fixture_wav())));
    assert!(matches!(
        tts_fixture("wrong-model".into()).await,
        Err(Denial::UnsupportedModel)
    ));
}
