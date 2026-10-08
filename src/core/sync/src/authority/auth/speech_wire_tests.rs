use super::*;
fn key() -> SigningIdentity {
    SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap()
}
fn request(operation: SpeechOperation) -> SpeechComputerRequest {
    let workload = match &operation {
        SpeechOperation::OpenTranscription { .. }
        | SpeechOperation::Append { .. }
        | SpeechOperation::Finish { .. }
        | SpeechOperation::CancelTranscription { .. } => SpeechWorkload::Transcription,
        _ => SpeechWorkload::Synthesis,
    };
    SpeechComputerRequest {
        binding: SpeechComputerBinding {
            source_instance_id: "meeting-instance".into(),
            target_instance_id: "gpu-instance".into(),
            computer_id: "opaque-computer".into(),
            owner_user_id: "owner".into(),
            grant_id: "speech-only-grant".into(),
            grant_revision: 3,
            workload,
            model: match workload {
                SpeechWorkload::Transcription => "engineai/Voxtral-Mini-4B-Realtime-2602",
                SpeechWorkload::Synthesis => "engineai/Voxtral-4B-TTS-2603",
            }
            .into(),
        },
        operation,
    }
}
fn open() -> SpeechComputerRequest {
    request(SpeechOperation::OpenTranscription {
        intent_id: "intent-a".into(),
        sample_rate_hz: 16_000,
    })
}
#[test]
fn signed_open_roundtrip_pins_recipient_scope_model_and_nonce() {
    let source = key();
    let target = key();
    let signed =
        SignedSpeechRequest::new(&source, &target.public_identity(), "scope", open()).unwrap();
    let verified =
        verify_request(signed.envelope.clone(), &target.public_identity(), "scope").unwrap();
    assert_eq!(verified.sender(), source.public_identity());
    assert_eq!(verified.nonce(), signed.nonce());
    let reply = verified
        .reply(
            &target,
            SpeechComputerReply::TranscriptionOpened {
                stream_id: "server-stream".into(),
                model: verified.request().binding.model.clone(),
            },
        )
        .unwrap();
    assert!(matches!(
        signed.verify_reply(reply.clone()).unwrap(),
        SpeechComputerReply::TranscriptionOpened { .. }
    ));
    let second =
        SignedSpeechRequest::new(&source, &target.public_identity(), "scope", open()).unwrap();
    assert!(second.verify_reply(reply).is_err());
    assert!(verify_request(signed.envelope.clone(), &source.public_identity(), "scope").is_err());
    assert!(verify_request(signed.envelope, &target.public_identity(), "foreign-scope").is_err());
}
#[test]
fn tampering_signed_grant_or_unknown_request_fields_is_rejected() {
    let source = key();
    let target = key();
    let signed =
        SignedSpeechRequest::new(&source, &target.public_identity(), "scope", open()).unwrap();
    let mut tampered = signed.envelope.clone();
    tampered["body"]["data"]["binding"]["owner_user_id"] = Value::String("foreign".into());
    assert!(verify_request(tampered, &target.public_identity(), "scope").is_err());
    let mut envelope: Envelope = serde_json::from_value(signed.envelope).unwrap();
    envelope.body.data["unexpected"] = Value::Bool(true);
    let signed_unknown = serde_json::to_value(source.sign(envelope.body).unwrap()).unwrap();
    assert!(verify_request(signed_unknown, &target.public_identity(), "scope").is_err());
}
#[test]
fn wrong_model_wrong_phase_and_replaced_signer_cannot_reply() {
    let source = key();
    let target = key();
    let replacement = key();
    let signed =
        SignedSpeechRequest::new(&source, &target.public_identity(), "scope", open()).unwrap();
    let verified = verify_request(signed.envelope, &target.public_identity(), "scope").unwrap();
    let reply = SpeechComputerReply::TranscriptionOpened {
        stream_id: "stream".into(),
        model: "wrong".into(),
    };
    assert!(verified.reply(&target, reply.clone()).is_err());
    assert!(verified.reply(&replacement, reply).is_err());
    assert!(verified
        .reply(
            &target,
            SpeechComputerReply::SynthesisPending {
                run_id: "run".into()
            }
        )
        .is_err());
}
#[test]
fn chunks_are_pcm16_mono_16k_at_most100ms_and_workload_bound() {
    let make = |hex: String| {
        request(SpeechOperation::Append {
            stream_id: "stream".into(),
            sequence: 1,
            pcm16_hex: hex,
        })
    };
    assert!(make("00".repeat(3200)).validate().is_ok());
    for bytes in [0, 1, 3201, 3202] {
        assert!(make("00".repeat(bytes)).validate().is_err());
    }
    assert!(make("AA".repeat(2)).validate().is_err());
    assert!(make("zz".repeat(2)).validate().is_err());
    let mut req = make("0000".into());
    req.binding.workload = SpeechWorkload::Synthesis;
    assert!(req.validate().is_err());
    let req = request(SpeechOperation::OpenTranscription {
        intent_id: "a".into(),
        sample_rate_hz: 48_000,
    });
    assert!(req.validate().is_err());
}
#[test]
fn transcript_reply_cannot_advance_another_stream_sequence_or_final_stage() {
    let req = request(SpeechOperation::Append {
        stream_id: "stream".into(),
        sequence: 7,
        pcm16_hex: "0000".into(),
    });
    let good = SpeechComputerReply::Transcript {
        stream_id: "stream".into(),
        sequence: 7,
        text: "hello".into(),
        model: req.binding.model.clone(),
        audio_duration_ms: 100,
        is_final: false,
    };
    assert!(req.validate_reply(&good).is_ok());
    let mut wrong = good.clone();
    if let SpeechComputerReply::Transcript { sequence, .. } = &mut wrong {
        *sequence = 8;
    }
    assert!(req.validate_reply(&wrong).is_err());
    let mut wrong = good.clone();
    if let SpeechComputerReply::Transcript { stream_id, .. } = &mut wrong {
        *stream_id = "other".into();
    }
    assert!(req.validate_reply(&wrong).is_err());
    let mut wrong = good;
    if let SpeechComputerReply::Transcript { is_final, .. } = &mut wrong {
        *is_final = true;
    }
    assert!(req.validate_reply(&wrong).is_err());
}
#[test]
fn synthesis_text_and_audio_reads_have_real_byte_bounds() {
    let make = |text: String| {
        request(SpeechOperation::StartSynthesis {
            intent_id: "intent".into(),
            text,
            voice_id: "neutral_female".into(),
        })
    };
    assert!(make("é".repeat(2048)).validate().is_ok());
    assert!(make("é".repeat(2049)).validate().is_err());
    assert!(make("   ".into()).validate().is_err());
    let req = request(SpeechOperation::ReadSynthesis {
        run_id: "run".into(),
        offset: 5,
        max_bytes: 3,
    });
    let good = SpeechComputerReply::SynthesisChunk {
        run_id: "run".into(),
        offset: 5,
        total_bytes: 8,
        hex: "000102".into(),
    };
    assert!(req.validate_reply(&good).is_ok());
    for (offset, total_bytes, hex) in [
        (6, 9, "000102"),
        (5, 7, "000102"),
        (5, 8, ""),
        (5, 9, "00010203"),
    ] {
        assert!(req
            .validate_reply(&SpeechComputerReply::SynthesisChunk {
                run_id: "run".into(),
                offset,
                total_bytes,
                hex: hex.into(),
            })
            .is_err());
    }
}
#[test]
fn signed_response_cannot_echo_another_grant_revision() {
    let source = key();
    let target = key();
    let signed =
        SignedSpeechRequest::new(&source, &target.public_identity(), "scope", open()).unwrap();
    let verified =
        verify_request(signed.envelope.clone(), &target.public_identity(), "scope").unwrap();
    let reply = verified
        .reply(
            &target,
            SpeechComputerReply::Denied {
                reason: SpeechDenial::GrantDenied,
            },
        )
        .unwrap();
    let mut envelope: Envelope = serde_json::from_value(reply).unwrap();
    envelope.body.data["binding"]["grant_revision"] = Value::from(4);
    let forged = serde_json::to_value(target.sign(envelope.body).unwrap()).unwrap();
    assert!(signed.verify_reply(forged).is_err());
}
#[test]
fn oversized_envelopes_are_rejected_before_deserialization() {
    let huge = serde_json::json!({"padding":"a".repeat(MAX_WIRE_BYTES)});
    assert!(verify_request(huge, "unused", "unused").is_err());
}
