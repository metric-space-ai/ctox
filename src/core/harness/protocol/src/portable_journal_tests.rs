use super::*;
use serde_json::json;

const SESSION_ID: &str = "11111111-1111-1111-1111-111111111111";
const TIMESTAMP: &str = "2026-09-20T12:00:00.123Z";
const EVENT_TIMESTAMP: &str = "2026-09-20T12:00:02.456Z";
const SECRET_PAYLOAD: &str = "private-journal-payload";

fn expected() -> PortableJournalExpectation {
    PortableJournalExpectation {
        format: PortableJournalFormat::current(),
        session_id: ThreadId::from_string(SESSION_ID).expect("valid fixture id"),
    }
}

fn metadata_line() -> serde_json::Value {
    json!({
        "timestamp": TIMESTAMP,
        "type": "session_meta",
        "payload": {
            "id": SESSION_ID,
            "timestamp": TIMESTAMP,
            "cwd": "/original/workspace",
            "originator": "codex_cli_rs",
            "cli_version": "1.0.0",
            "source": "exec",
            "model_provider": "test-provider",
            "base_instructions": {"text": "test"},
            "capability_profile": "workspace_worker",
        },
    })
}

fn message_line() -> serde_json::Value {
    json!({
        "timestamp": EVENT_TIMESTAMP,
        "type": "event_msg",
        "payload": {
            "type": "user_message",
            "message": SECRET_PAYLOAD,
        },
    })
}

fn journal(lines: &[serde_json::Value]) -> (Vec<u8>, PortableArtifactRef) {
    let mut raw = Vec::new();
    for line in lines {
        serde_json::to_writer(&mut raw, line).expect("serialize fixture");
        raw.push(b'\n');
    }
    let artifact = artifact_ref_for(&raw);
    (raw, artifact)
}

fn raw_journal(lines: &[&str]) -> (Vec<u8>, PortableArtifactRef) {
    let mut raw = Vec::new();
    for line in lines {
        raw.extend_from_slice(line.as_bytes());
        raw.push(b'\n');
    }
    let artifact = artifact_ref_for(&raw);
    (raw, artifact)
}

fn default_limits() -> PortableJournalLimits {
    PortableJournalLimits::default()
}
fn assert_portable_error(
    actual: Result<ValidatedPortableJournal, PortableJournalError>,
    expected: Result<ValidatedPortableJournal, PortableJournalError>,
) {
    let expected = match expected {
        Err(error) => error,
        Ok(_) => panic!("expected a validation error"),
    };
    assert_eq!(actual.err(), Some(expected));
}
fn json_string(value: impl Into<String>) -> serde_json::Value {
    serde_json::Value::String(value.into())
}

#[test]
fn accepts_independent_timestamps_and_states_what_it_did_not_validate() {
    let (raw, artifact) = journal(&[metadata_line(), message_line()]);
    let validated = validate_portable_journal(&raw, &artifact, &expected(), &default_limits())
        .expect("valid portable journal");

    assert_eq!(validated.format, PORTABLE_CODEX_JOURNAL_FORMAT);
    assert_eq!(validated.format_version, 1);
    assert_eq!(validated.session_id, expected().session_id);
    assert_eq!(validated.record_count, 2);
    assert_eq!(validated.artifact, artifact);
    assert_eq!(
        validated.provider_continuation,
        ProviderContinuationState::Unresolved
    );
    assert_eq!(validated.external_effects, ExternalEffectState::Unknown);
    assert_eq!(validated.items.len(), 1);
}

#[test]
fn requires_explicit_supported_input_format_and_version() {
    let unsupported = PortableJournalExpectation {
        format: PortableJournalFormat {
            format: "other-journal",
            format_version: 99,
        },
        session_id: expected().session_id,
    };
    let (raw, artifact) = journal(&[metadata_line(), message_line()]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &unsupported, &default_limits()),
        Err(PortableJournalError::UnsupportedInputFormat),
    );

    let old_version = PortableJournalExpectation {
        format: PortableJournalFormat {
            format: PORTABLE_CODEX_JOURNAL_FORMAT,
            format_version: 0,
        },
        session_id: expected().session_id,
    };
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &old_version, &default_limits()),
        Err(PortableJournalError::UnsupportedInputFormat),
    );
}

#[test]
fn binds_validation_to_exact_size_and_hash() {
    let (mut raw, artifact) = journal(&[metadata_line(), message_line()]);
    raw.push(b'x');
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::ArtifactSizeMismatch),
    );
    raw.pop();
    let index = raw.len() - 2;
    raw[index] = raw[index].wrapping_add(1);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::ArtifactHashMismatch),
    );
}

#[test]
fn requires_metadata_as_the_initial_record() {
    let (raw, artifact) = journal(&[message_line(), metadata_line()]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::MissingInitialMetadata),
    );

    let (raw, artifact) = journal(&[metadata_line(), metadata_line(), message_line()]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::DuplicateSessionMetadata),
    );
}

#[test]
fn validates_each_line_timestamp_independently() {
    let (raw, artifact) = journal(&[metadata_line(), message_line()]);
    assert!(validate_portable_journal(&raw, &artifact, &expected(), &default_limits()).is_ok());

    let mut bad_event = message_line();
    bad_event["timestamp"] = json_string("not-a-timestamp");
    let (raw, artifact) = journal(&[metadata_line(), bad_event]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::InvalidMetadata),
    );

    let mut bad_metadata = metadata_line();
    bad_metadata["payload"]["timestamp"] = json_string("2026-09-20T99:00:00Z");
    let (raw, artifact) = journal(&[bad_metadata, message_line()]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::InvalidMetadata),
    );
}

#[test]
fn rejects_corrupt_interior_and_final_records_without_payload_leakage() {
    let corrupt_interior =
        json!({"timestamp": EVENT_TIMESTAMP, "type": "event_msg", "payload": SECRET_PAYLOAD});
    let (raw, artifact) = journal(&[metadata_line(), corrupt_interior, message_line()]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::InvalidLine),
    );
    assert!(
        !PortableJournalError::InvalidLine
            .to_string()
            .contains(SECRET_PAYLOAD)
    );

    let (mut final_raw, _) = journal(&[metadata_line(), message_line()]);
    final_raw.pop();
    let final_artifact = artifact_ref_for(&final_raw);
    assert_portable_error(
        validate_portable_journal(&final_raw, &final_artifact, &expected(), &default_limits()),
        Err(PortableJournalError::UnterminatedFinalRecord),
    );
}

#[test]
fn rejects_unknown_records_and_unknown_fields() {
    let unknown = json!({"timestamp": EVENT_TIMESTAMP, "type": "future_record", "payload": {}});
    let (raw, artifact) = journal(&[metadata_line(), unknown, message_line()]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::UnsupportedRecord),
    );

    let mut top_level_unknown = metadata_line();
    top_level_unknown["transport_hint"] = json_string("unsupported");
    let (raw, artifact) = journal(&[top_level_unknown, message_line()]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::UnsupportedRecordField),
    );

    let mut metadata_unknown = metadata_line();
    metadata_unknown["payload"]["resume_hint"] = json_string("unsupported");
    let (raw, artifact) = journal(&[metadata_unknown, message_line()]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::UnsupportedRecordField),
    );

    let mut event_unknown = message_line();
    event_unknown["payload"]["provider_resume_hint"] = json_string("unsupported");
    let (raw, artifact) = journal(&[metadata_line(), event_unknown]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::UnsupportedRecordField),
    );
}

#[test]
fn rejects_duplicate_json_keys_before_value_collapsing() {
    let metadata = format!(
        r#"{{"timestamp":"{TIMESTAMP}","type":"session_meta","payload":{{"id":"{SESSION_ID}","timestamp":"{TIMESTAMP}","cwd":"/one","cwd":"/two","originator":"codex_cli_rs","cli_version":"1.0.0","source":"exec","model_provider":"test-provider","base_instructions":{{}},"capability_profile":"workspace_worker"}}}}"#
    );
    let event = message_line().to_string();
    let (raw, artifact) = raw_journal(&[&metadata, &event]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::DuplicateJsonKey),
    );
}

#[test]
fn validates_surrogate_escapes_and_normalized_duplicate_keys() {
    let metadata = metadata_line().to_string();
    let event = |payload: &str| {
        format!(r#"{{"timestamp":"{EVENT_TIMESTAMP}","type":"event_msg","payload":{{{payload}}}}}"#)
    };
    let malformed = [
        r#""type":"user_message","message":"\ud83d\u0000""#,
        r#""type":"user_message","message":"\ud83d\ud83d""#,
        r#""type":"user_message","message":"\ud83d\u0041""#,
        r#""type":"user_message","message":"\ude00""#,
        r#""type":"user_message","message":"\ud83d""#,
        r#""type":"user_message","message":"\ud83d\uZZZZ""#,
    ];
    for payload in malformed {
        let event = event(payload);
        let (raw, artifact) = raw_journal(&[&metadata, &event]);
        assert_portable_error(
            validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
            Err(PortableJournalError::InvalidLine),
        );
    }

    let supplementary = event(r#""type":"user_message","message":"\ud83d\ude00""#);
    let (raw, artifact) = raw_journal(&[&metadata, &supplementary]);
    let validated = validate_portable_journal(&raw, &artifact, &expected(), &default_limits())
        .expect("valid supplementary scalar");
    assert_eq!(validated.record_count, 2);

    let duplicate = event(r#""type":"user_message","\u006dessage":"first","message":"second""#);
    let (raw, artifact) = raw_journal(&[&metadata, &duplicate]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::DuplicateJsonKey),
    );
}

#[test]
fn rejects_empty_history_and_identity_mismatch() {
    let (raw, artifact) = journal(&[metadata_line()]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::EmptyHistory),
    );

    let other = PortableJournalExpectation {
        format: PortableJournalFormat::current(),
        session_id: ThreadId::from_string("22222222-2222-2222-2222-222222222222")
            .expect("valid other id"),
    };
    let (raw, artifact) = journal(&[metadata_line(), message_line()]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &other, &default_limits()),
        Err(PortableJournalError::IdentityMismatch),
    );
}

#[test]
fn rejects_missing_or_conflicting_required_metadata() {
    let mut missing = metadata_line();
    missing["payload"]["capability_profile"] = serde_json::Value::Null;
    let (raw, artifact) = journal(&[missing, message_line()]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::MissingMetadataField),
    );

    let mut conflicting = metadata_line();
    conflicting["payload"]["timestamp"] = json_string("2026-09-20T12:00:01Z");
    conflicting["payload"]["id"] = json_string("22222222-2222-2222-2222-222222222222");
    let (raw, artifact) = journal(&[conflicting, message_line()]);
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::IdentityMismatch),
    );
}

#[test]
fn preserves_absolute_offsets_across_unequal_and_many_records() {
    let mut lines = vec![metadata_line()];
    for index in 0..250 {
        let mut message = message_line();
        message["payload"]["message"] = json_string(format!("payload-{index}"));
        lines.push(message);
    }
    let (raw, artifact) = journal(&lines);
    let validated = validate_portable_journal(&raw, &artifact, &expected(), &default_limits())
        .expect("unequal multi-record journal");
    assert_eq!(validated.record_count, 251);
    assert_eq!(validated.items.len(), 250);

    // The second record is much shorter than the metadata line; the former
    // relative-offset bug could slice `end < start` before reaching this error.
    let mut malformed = metadata_line().to_string().into_bytes();
    malformed.push(b'\n');
    malformed.extend_from_slice(b"bad\n");
    let artifact = artifact_ref_for(&malformed);
    assert_portable_error(
        validate_portable_journal(&malformed, &artifact, &expected(), &default_limits()),
        Err(PortableJournalError::InvalidLine),
    );
}

#[test]
fn enforces_line_and_record_budgets() {
    let (raw, artifact) = journal(&[metadata_line(), message_line()]);
    let limits = PortableJournalLimits {
        max_bytes: 64 * 1024 * 1024,
        max_line_bytes: 1,
        max_records: 100_000,
    };
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &limits),
        Err(PortableJournalError::OversizedLine),
    );

    let limits = PortableJournalLimits {
        max_bytes: 64 * 1024 * 1024,
        max_line_bytes: 16 * 1024 * 1024,
        max_records: 1,
    };
    assert_portable_error(
        validate_portable_journal(&raw, &artifact, &expected(), &limits),
        Err(PortableJournalError::TooManyRecords),
    );
}
