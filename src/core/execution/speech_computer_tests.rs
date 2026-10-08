#[test]
fn speech_computer_batching_preserves_pcm_and_the_wire_budget() {
    let (tx, mut rx) = mpsc::channel(8);
    let mut expected = Vec::new();
    for frame in 0..8u8 {
        let pcm = vec![frame; 640];
        expected.extend_from_slice(&pcm);
        assert!(tx.try_send(Input::Audio(pcm)).is_ok());
    }
    assert!(matches!(
        tx.try_send(Input::Audio(vec![9; 640])),
        Err(mpsc::error::TrySendError::Full(_))
    ));
    let mut pending = None;
    let Input::Audio(first) = rx.try_recv().unwrap() else {
        panic!("audio expected")
    };
    let first = coalesce_audio(first, &mut rx, &mut pending);
    assert_eq!(first.len(), 3200);
    PcmFormat::default().validate_chunk(&first).unwrap();
    assert!(pending.is_none());
    let Input::Audio(next) = rx.try_recv().unwrap() else {
        panic!("audio expected")
    };
    let second = coalesce_audio(next, &mut rx, &mut pending);
    assert_eq!(second.len(), 1920);
    PcmFormat::default().validate_chunk(&second).unwrap();
    assert_eq!([first, second].concat(), expected);
}

#[test]
fn speech_computer_batching_defers_an_oversized_next_frame_without_splitting() {
    let (tx, mut rx) = mpsc::channel(8);
    assert!(tx.try_send(Input::Audio(vec![2; 1280])).is_ok());
    assert!(tx.try_send(Input::Audio(vec![3; 640])).is_ok());
    let mut pending = None;
    let first = coalesce_audio(vec![1; 2560], &mut rx, &mut pending);
    assert_eq!(first, vec![1; 2560]);
    let Some(Input::Audio(next)) = pending.take() else {
        panic!("overflow frame retained")
    };
    assert_eq!(next, vec![2; 1280]);
    let second = coalesce_audio(next, &mut rx, &mut pending);
    assert_eq!(second, [vec![2; 1280], vec![3; 640]].concat());
    assert!(pending.is_none());
}

#[test]
fn speech_computer_batching_does_not_cross_flush_finish_or_cancel() {
    for command in [Input::Flush, Input::Finish(Instant::now()), Input::Cancel] {
        let finish = match &command {
            Input::Finish(mark) => Some(*mark),
            _ => None,
        };
        let (tx, mut rx) = mpsc::channel(8);
        assert!(tx.try_send(Input::Audio(vec![2; 640])).is_ok());
        assert!(tx.try_send(command).is_ok());
        assert!(tx.try_send(Input::Audio(vec![3; 640])).is_ok());
        let mut pending = None;
        let batch = coalesce_audio(vec![1; 640], &mut rx, &mut pending);
        assert_eq!(batch, [vec![1; 640], vec![2; 640]].concat());
        match pending.take().unwrap() {
            Input::Finish(mark) => assert_eq!(Some(mark), finish),
            Input::Flush | Input::Cancel => assert!(finish.is_none()),
            Input::Audio(_) => panic!("control command reordered"),
        }
        let Input::Audio(after) = rx.try_recv().unwrap() else {
            panic!("audio expected")
        };
        assert_eq!(after, vec![3; 640]);
    }
}

#[test]
fn speech_computer_batching_forwards_an_isolated_frame_without_waiting() {
    let (_tx, mut rx) = mpsc::channel(8);
    let mut pending = None;
    assert_eq!(
        coalesce_audio(vec![7; 640], &mut rx, &mut pending),
        vec![7; 640]
    );
    assert!(pending.is_none());
}
#[test]
fn speech_computer_prepared_publication_rejects_reconfiguration_and_issuer_removal() {
    for change in ["epoch", "issuer"] {
        let root = tempfile::tempdir().unwrap();
        crate::sync_host::handle_command(root.path(), &["init".into()]).unwrap();
        let issuer = crate::sync_host::signing_identity(root.path()).unwrap();
        let mut route = route(SpeechWorkload::Transcription);
        route.source_signing_identity = issuer.public_identity();
        route.expires_at_unix_ms = now_ms() + 60_000;
        let saved = Saved {
            epoch: Uuid::new_v4().to_string(),
            config: SpeechComputerConfig {
                transcription: Some(route.clone()),
                synthesis: None,
            },
        };
        crate::persistence::store_json_payload(root.path(), COMPUTER_CONFIG_KEY, Some(&saved))
            .unwrap();
        let current = Current {
            root: root.path().into(),
            saved: saved.clone(),
            route,
        };
        let mut published = 0;
        WebRTCPublicationGuard::with_current(&current, &mut || {
            published += 1;
            Ok(())
        })
        .unwrap();
        match change {
            "epoch" => crate::sync_host::with_current_signing_identity(root.path(), |_| {
                let mut replaced = saved.clone();
                replaced.epoch = Uuid::new_v4().to_string();
                crate::persistence::store_json_payload(
                    root.path(),
                    COMPUTER_CONFIG_KEY,
                    Some(&replaced),
                )
            })
            .unwrap(),
            "issuer" => {
                crate::secrets::delete_secret_record(
                    root.path(),
                    "ctox-sync-host",
                    "identity-pkcs8",
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            WebRTCPublicationGuard::with_current(&current, &mut || {
                published += 1;
                Ok(())
            })
            .is_err(),
            "{change}"
        );
        assert_eq!(published, 1, "retired authority published at {change}");
    }
}

#[test]
fn speech_computer_malformed_operator_document_preserves_saved_route() {
    let root = tempfile::tempdir().unwrap();
    let original = Saved {
        epoch: "epoch-a".into(),
        config: SpeechComputerConfig::default(),
    };
    crate::persistence::store_json_payload(root.path(), COMPUTER_CONFIG_KEY, Some(&original))
        .unwrap();
    let path = root.path().join("routes.json");
    for bytes in [
        br#"{ "transcription":null, "synthesis":null, "api_key":"forbidden" }"#.to_vec(),
        vec![b' '; MAX_CONFIG_BYTES as usize + 1],
    ] {
        std::fs::write(&path, bytes).unwrap();
        assert!(configure_from_file(root.path(), &path).is_err());
        assert!(load_saved(root.path()).unwrap() == original);
    }
}
#[test]
fn speech_computer_config_can_select_a_pin_without_an_ephemeral_route() {
    let original = route(SpeechWorkload::Transcription);
    let mut value = serde_json::to_value(&original).unwrap();
    value.as_object_mut().unwrap().remove("native_peer_route");
    let restored: SpeechComputerRoute = serde_json::from_value(value).unwrap();
    assert!(restored.native_peer_route.is_empty());
    assert_eq!(
        restored.target_signing_identity,
        original.target_signing_identity
    );
    assert!(validate_route(&restored, SpeechWorkload::Transcription, 10_000).is_ok());
}

use super::*;

fn identity() -> ctox_sync::authority::auth::SigningIdentity {
    ctox_sync::authority::auth::SigningIdentity::from_pkcs8(
        &ctox_sync::authority::auth::SigningIdentity::generate_pkcs8().unwrap(),
    )
    .unwrap()
}
fn route(role: SpeechWorkload) -> SpeechComputerRoute {
    SpeechComputerRoute {
        scope_id: "isolated-speech-acceptance".into(),
        native_peer_route: "accepted-gpu-peer".into(),
        source_signing_identity: identity().public_identity(),
        target_signing_identity: identity().public_identity(),
        binding: SpeechComputerBinding {
            source_instance_id: "meeting-host".into(),
            target_instance_id: "isolated-gpu".into(),
            computer_id: "gpu3".into(),
            owner_user_id: "owner-a".into(),
            grant_id: "speech-only-grant".into(),
            grant_revision: 1,
            workload: role,
            model: match role {
                SpeechWorkload::Transcription => "engineai/Voxtral-Mini-4B-Realtime-2602",
                SpeechWorkload::Synthesis => "engineai/Voxtral-4B-TTS-2603",
            }
            .into(),
        },
        expires_at_unix_ms: 3_610_000,
    }
}
fn wav() -> Vec<u8> {
    let samples = 240usize;
    let size = samples * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&((36 + size) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&24_000u32.to_le_bytes());
    bytes.extend_from_slice(&48_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(size as u32).to_le_bytes());
    bytes.resize(44 + size, 0);
    bytes
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn speech_computer_routes_reject_expired_unbounded_or_wrong_workload_grants() {
    let mut r = route(SpeechWorkload::Transcription);
    assert!(validate_route(&r, SpeechWorkload::Transcription, 10_000).is_ok());
    assert!(validate_route(&r, SpeechWorkload::Synthesis, 10_000).is_err());
    r.expires_at_unix_ms = 10_000;
    assert!(validate_route(&r, SpeechWorkload::Transcription, 10_000).is_err());
    r.expires_at_unix_ms = 10_000 + 86_400_001;
    assert!(validate_route(&r, SpeechWorkload::Transcription, 10_000).is_err());
}

#[test]
fn speech_computer_routes_pin_distinct_identities_and_reject_endpoint_injection() {
    let mut r = route(SpeechWorkload::Transcription);
    r.target_signing_identity = r.source_signing_identity.clone();
    assert!(validate_route(&r, SpeechWorkload::Transcription, 10_000).is_err());
    r.target_signing_identity = identity().public_identity();
    r.scope_id = "scope/other".into();
    assert!(validate_route(&r, SpeechWorkload::Transcription, 10_000).is_err());
    r.scope_id = "scope".into();
    r.native_peer_route = "peer\nextra".into();
    assert!(validate_route(&r, SpeechWorkload::Transcription, 10_000).is_err());
    let mut json = serde_json::to_value(SpeechComputerConfig {
        transcription: Some(route(SpeechWorkload::Transcription)),
        synthesis: None,
    })
    .unwrap();
    json["transcription"]["api_key"] = "not-a-route-field".into();
    assert!(serde_json::from_value::<SpeechComputerConfig>(json).is_err());
}

#[test]
fn speech_computer_reply_verifier_rejects_replaced_independent_pin() {
    let source = identity();
    let target = identity();
    let mut r = route(SpeechWorkload::Transcription);
    r.source_signing_identity = source.public_identity();
    r.target_signing_identity = target.public_identity();
    let signed = SignedSpeechRequest::new(
        &source,
        &r.target_signing_identity,
        &r.scope_id,
        SpeechComputerRequest {
            binding: r.binding,
            operation: Op::OpenTranscription {
                intent_id: "intent".into(),
                sample_rate_hz: 16_000,
            },
        },
    )
    .unwrap();
    let request = ctox_sync::authority::auth::speech_wire::verify_request(
        signed.envelope.clone(),
        &r.target_signing_identity,
        &r.scope_id,
    )
    .unwrap();
    let reply = request
        .reply(
            &target,
            Reply::TranscriptionOpened {
                stream_id: "stream".into(),
                model: "engineai/Voxtral-Mini-4B-Realtime-2602".into(),
            },
        )
        .unwrap();
    let verifier = VerifiedReply {
        pin: r.target_signing_identity.clone(),
        signed,
    };
    use crate::sync_host::NativeControlReplyVerifier;
    assert!(verifier.verify(&r.target_signing_identity, &reply).is_ok());
    assert!(verifier
        .verify(&identity().public_identity(), &reply)
        .is_err());
}

#[test]
fn speech_computer_wav_checks_actual_hash_duration_and_playable_format() {
    let bytes = wav();
    assert!(verify_audio(&bytes, &digest(&bytes), 10).is_ok());
    assert!(verify_audio(&bytes, &"0".repeat(64), 10).is_err());
    assert!(verify_audio(&bytes, &digest(&bytes), 11).is_err());
    let mut stereo = bytes.clone();
    stereo[22] = 2;
    assert!(verify_audio(&stereo, &digest(&stereo), 10).is_err());
    let mut bad_rate = bytes.clone();
    bad_rate[24..28].copy_from_slice(&16_000u32.to_le_bytes());
    assert!(verify_audio(&bad_rate, &digest(&bad_rate), 10).is_err());
    let truncated = &bytes[..bytes.len() - 1];
    assert!(verify_audio(truncated, &digest(truncated), 10).is_err());
}

#[test]
fn speech_computer_wav_rejects_repeated_data_and_out_of_range_chunks() {
    let mut repeated = wav();
    repeated.extend_from_slice(b"data");
    repeated.extend_from_slice(&2u32.to_le_bytes());
    repeated.extend_from_slice(&[0, 0]);
    let size = repeated.len() as u32 - 8;
    repeated[4..8].copy_from_slice(&size.to_le_bytes());
    assert!(verify_audio(&repeated, &digest(&repeated), 10).is_err());
    let mut malformed = wav();
    malformed[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(verify_audio(&malformed, &digest(&malformed), 10).is_err());
}

#[test]
fn speech_computer_hex_transport_is_bounded_and_preserves_pcm() {
    let pcm = [0, 255, 128, 1];
    assert_eq!(decode_hex(&encode_hex(&pcm)).unwrap(), pcm);
    for invalid in ["a", "AA", "xx", "é"] {
        assert!(decode_hex(invalid).is_err());
    }
    assert!(decode_hex(&"aa".repeat(MAX_READ_BYTES as usize + 1)).is_err());
}

#[tokio::test]
async fn speech_computer_invalid_synthesis_fails_before_transport_or_credentials() {
    let root = Path::new("/not-an-installed-runtime");
    let invalid = SpeechRequest {
        text: "".into(),
        format: SpeechAudioFormat::Wav,
        voice_id: Some("voice".into()),
    };
    assert!(matches!(
        synthesize(root, &invalid, None).await,
        Err(SpeechError::InvalidRequest)
    ));
    let invalid = SpeechRequest {
        text: "hello".into(),
        format: SpeechAudioFormat::Mp3,
        voice_id: Some("voice".into()),
    };
    assert!(matches!(
        synthesize(root, &invalid, None).await,
        Err(SpeechError::InvalidRequest)
    ));
    let missing = SpeechRequest {
        text: "hello".into(),
        format: SpeechAudioFormat::Wav,
        voice_id: None,
    };
    assert!(matches!(
        synthesize(root, &missing, None).await,
        Err(SpeechError::MissingVoice)
    ));
}
