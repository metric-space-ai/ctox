use super::*;
#[cfg(unix)]
use tokio::{io::AsyncReadExt, net::UnixListener};

#[cfg(unix)]
#[tokio::test]
async fn private_runtime_stream_preserves_delta_sequence_and_final_mark() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("speech.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = BufReader::new(socket);
        let mut line = String::new();
        socket.read_line(&mut line).await.unwrap();
        let open: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(open["kind"], "transcription_open");
        assert_eq!(open["model"], "test-native-stt");
        assert_eq!(open["sample_rate_hz"], 16_000);
        socket
            .get_mut()
            .write_all(b"{\"kind\":\"stream_ready\",\"model\":\"test-native-stt\"}\n")
            .await
            .unwrap();
        for (sequence, text) in [(1, "H"), (2, "Hi")] {
            line.clear();
            socket.read_line(&mut line).await.unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["kind"], "transcription_append");
            assert_eq!(
                BASE64
                    .decode(request["pcm_base64"].as_str().unwrap())
                    .unwrap(),
                vec![0; 640]
            );
            let response = json!({"kind":"stream_update","sequence":sequence,"text":text,"audio_duration_ms":sequence*20});
            socket
                .get_mut()
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        }
        line.clear();
        socket.read_line(&mut line).await.unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap()["kind"],
            "transcription_finish"
        );
        socket.get_mut().write_all(b"{\"kind\":\"transcription_final\",\"sequence\":3,\"model\":\"test-native-stt\",\"text\":\"Hi\",\"audio_duration_ms\":40}\n").await.unwrap();
    });
    let mut stream = TranscriptionStream::open_runtime(
        crate::inference::local_transport::LocalTransport::UnixSocket { path },
        "test-native-stt".into(),
        PcmFormat::default(),
    )
    .await
    .unwrap();
    stream.append_pcm(&vec![0; 640]).unwrap();
    stream.append_pcm(&vec![0; 640]).unwrap();
    stream.finish_audio().unwrap();
    assert!(
        matches!(stream.next_event().await, Some(Ok(TranscriptEvent::Partial{sequence:1,text,..})) if text == "H")
    );
    assert!(
        matches!(stream.next_event().await, Some(Ok(TranscriptEvent::Partial{sequence:2,text,..})) if text == "i")
    );
    assert!(
        matches!(stream.next_event().await, Some(Ok(TranscriptEvent::Final{sequence:3,text,finish_to_final_ms:Some(_),audio_duration_ms:40,..})) if text == "Hi")
    );
    assert!(stream.next_event().await.is_none());
    server.await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn dropping_private_runtime_stream_closes_owned_socket() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("cancel.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = BufReader::new(socket);
        let mut line = String::new();
        socket.read_line(&mut line).await.unwrap();
        socket
            .get_mut()
            .write_all(b"{\"kind\":\"stream_ready\",\"model\":\"test-native-stt\"}\n")
            .await
            .unwrap();
        let mut byte = [0];
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), socket.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    });
    let stream = TranscriptionStream::open_runtime(
        crate::inference::local_transport::LocalTransport::UnixSocket { path },
        "test-native-stt".into(),
        PcmFormat::default(),
    )
    .await
    .unwrap();
    drop(stream);
    server.await.unwrap();
}

#[tokio::test]
async fn local_stream_rejects_legacy_tcp_and_wrong_sample_rate() {
    use crate::inference::local_transport::LocalTransport;
    for format in [
        PcmFormat::default(),
        PcmFormat {
            sample_rate_hz: 48_000,
        },
    ] {
        let error = TranscriptionStream::open_runtime(
            LocalTransport::TcpLoopback {
                host: "127.0.0.1".into(),
                port: 1,
            },
            "test".into(),
            format,
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(
            error,
            SpeechError::UnsupportedBackend | SpeechError::InvalidRequest
        ));
    }
}
