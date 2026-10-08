// Origin: CTOX
// License: AGPL-3.0-only
use super::*;
const THREAD: &str = "cc6cfe73-2824-4360-9daf-3b3efb079931";
const TEXT: &str = "Native persistence is verified.";
fn fixture() -> anyhow::Result<(tempfile::TempDir, Value)> {
    let (root, trusted) = workjet_worker_dispatch::meeting_test_fixture()?;
    store::stable_instance_id(root.path())?;
    let corpus: Value = serde_json::from_str(include_str!(
        "../rxdb/tests/fixtures/workjet-jour-fixe-v1.json"
    ))?;
    let mut meeting = corpus["valid_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["type"] == "Meeting")
        .unwrap()["value"]
        .clone();
    meeting["id"] = json!("meeting-1");
    meeting["project_id"] = json!("project");
    meeting["owner_user_id"] = json!("owner");
    meeting["supervisor"] = json!({"workjet_thread_id":THREAD,"ctox_thread_key":format!("business-os/threads/{THREAD}")});
    meeting["state"] = json!("preparing");
    meeting["revision"] = json!(0);
    meeting["deck_revision"] = json!(1);
    meeting["previous_goal"] = Value::Null;
    meeting["comments"] = json!([]);
    meeting["transcript"] = json!([]);
    meeting["todos"] = Value::Null;
    meeting["slides"] = json!([{"id":"slide-1","meeting_id":"meeting-1","position":0,"title":"Progress","body_markdown":TEXT}]);
    let policy = store::open_store(root.path())?;
    policy.execute_batch("CREATE TABLE workjet_jour_fixe_meetings (meeting_id TEXT PRIMARY KEY, project_id TEXT NOT NULL,owner_user_id TEXT NOT NULL,scheduled_at_ms INTEGER NOT NULL,metadata_json TEXT NOT NULL,preparation_task_id TEXT)")?;
    policy.execute("INSERT INTO workjet_jour_fixe_meetings VALUES ('meeting-1','project','owner',1791793200000,?1,NULL)",[meeting.to_string()])?;
    Ok((root, trusted))
}
fn args() -> Value {
    json!({"action":"narrate","request":{"operation_id":"narrate-op","meeting_id":"meeting-1","slide_id":"slide-1","deck_revision":1,"expected_revision":0,"narration_text_sha256":hash(TEXT.as_bytes())}})
}
fn wav() -> Vec<u8> {
    let mut b = Vec::new();
    let data = 32000u32;
    b.extend(b"RIFF");
    b.extend((36 + data).to_le_bytes());
    b.extend(b"WAVEfmt ");
    b.extend(16u32.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(16000u32.to_le_bytes());
    b.extend(32000u32.to_le_bytes());
    b.extend(2u16.to_le_bytes());
    b.extend(16u16.to_le_bytes());
    b.extend(b"data");
    b.extend(data.to_le_bytes());
    b.extend(vec![0; data as usize]);
    b
}
fn call<F>(root: &Path, trusted: &Value, args: Value, producer: F) -> anyhow::Result<Value>
where
    F: FnOnce(&SpeechRequest) -> Result<VerifiedSpeechOutput, SpeechError>,
{
    let context = super::super::context_from_arguments_with_trusted_gateway_context(
        workjet_jour_fixe::WRITE_TOOL,
        &args,
        Some(trusted),
    )?;
    super::super::enforce_internal_command_session_scope(
        workjet_jour_fixe::WRITE_TOOL,
        &args,
        Some(trusted),
    )?;
    execute_with(root, &context, &args, trusted, producer)
}
fn output(request: &SpeechRequest) -> Result<VerifiedSpeechOutput, SpeechError> {
    Ok(crate::execution::speech::verified_fixture_output(
        &request.text,
        wav(),
    ))
}
fn saved(root: &Path) -> anyhow::Result<Value> {
    let raw: String = store::open_store(root)?.query_row(
        "SELECT metadata_json FROM workjet_jour_fixe_meetings WHERE meeting_id='meeting-1'",
        [],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
#[test]
fn native_producer_custody_ready_and_replay_keep_actual_bytes_and_hashes() -> anyhow::Result<()> {
    let (root, trusted) = fixture()?;
    let result = call(root.path(), &trusted, args(), |request| {
        assert_eq!(request.text, TEXT);
        assert!(request.voice_id.is_none());
        assert!(matches!(request.format, SpeechAudioFormat::Wav));
        output(request)
    })?;
    assert_eq!(result["mutation"]["state"], "ready");
    assert_eq!(result["native_narration"]["provider_verified"], true);
    assert_eq!(
        result["native_narration"]["audio"]["provenance"],
        "native_gateway"
    );
    assert_eq!(result["native_narration"]["audio"]["duration_ms"], 1000);
    assert_eq!(result["native_narration"]["audio"]["sha256"], hash(&wav()));
    let replay = call(root.path(), &trusted, args(), |_| {
        panic!("committed audio must not synthesize twice")
    })?;
    assert_eq!(replay, result);
    assert_eq!(saved(root.path())?["revision"], 1);
    let policy = store::open_store(root.path())?;
    let mut stmt=policy.prepare("SELECT payload_json FROM business_records WHERE collection='desktop_file_chunks' ORDER BY json_extract(payload_json,'$.idx')")?;
    let chunks = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let encoded = chunks
        .into_iter()
        .map(|raw| {
            serde_json::from_str::<Value>(&raw).unwrap()["data"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect::<String>();
    assert_eq!(
        base64::engine::general_purpose::STANDARD.decode(encoded)?,
        wav()
    );
    Ok(())
}
#[test]
fn every_slide_requires_custody_before_ready() -> anyhow::Result<()> {
    let (root, trusted) = fixture()?;
    let mut meeting = saved(root.path())?;
    meeting["slides"].as_array_mut().unwrap().push(json!({"id":"slide-2","meeting_id":"meeting-1","position":1,"title":"Decisions","body_markdown":"A decision is pending."}));
    store::open_store(root.path())?.execute(
        "UPDATE workjet_jour_fixe_meetings SET metadata_json=?1",
        [meeting.to_string()],
    )?;
    assert_eq!(
        call(root.path(), &trusted, args(), output)?["mutation"]["state"],
        "preparing"
    );
    let mut second = args();
    second["request"]["operation_id"] = json!("second");
    second["request"]["slide_id"] = json!("slide-2");
    second["request"]["expected_revision"] = json!(1);
    second["request"]["narration_text_sha256"] = json!(hash(b"A decision is pending."));
    assert_eq!(
        call(root.path(), &trusted, second, output)?["mutation"]["state"],
        "ready"
    );
    Ok(())
}
#[test]
fn no_writer_is_held_during_synthesis_and_changed_lease_cannot_publish() -> anyhow::Result<()> {
    let (root, trusted) = fixture()?;
    let result = call(root.path(), &trusted, args(), |request| {
        let mut core = Connection::open(crate::paths::core_db(root.path())).unwrap();
        let mut policy = store::open_store(root.path()).unwrap();
        let core_tx = core
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let policy_tx = policy
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        core_tx.execute("UPDATE communication_routing_state SET lease_worker_id='replacement' WHERE route_status='leased'",[]).unwrap();
        policy_tx.commit().unwrap();
        core_tx.commit().unwrap();
        output(request)
    });
    assert!(result.is_err());
    assert_eq!(saved(root.path())?["state"], "preparing");
    assert_eq!(store::open_store(root.path())?.query_row("SELECT count(*) FROM business_records WHERE collection='desktop_files' AND json_extract(payload_json,'$.source')='ctox-jour-fixe-native-audio'",[],|r|r.get::<_,u64>(0))?,0);
    Ok(())
}
#[test]
fn revoked_owner_changed_deck_or_closed_meeting_cannot_publish_after_await() -> anyhow::Result<()> {
    for change in ["revoked", "deck", "closed"] {
        let (root, trusted) = fixture()?;
        let result = call(root.path(), &trusted, args(), |request| {
            let policy = store::open_store(root.path()).unwrap();
            if change == "revoked" {
                policy
                    .execute(
                        "UPDATE business_users SET active=0 WHERE user_id='owner'",
                        [],
                    )
                    .unwrap();
            } else {
                let mut meeting = saved(root.path()).unwrap();
                if change == "deck" {
                    meeting["deck_revision"] = json!(2);
                } else {
                    meeting["state"] = json!("review");
                }
                policy
                    .execute(
                        "UPDATE workjet_jour_fixe_meetings SET metadata_json=?1",
                        [meeting.to_string()],
                    )
                    .unwrap();
            }
            output(request)
        });
        assert!(result.is_err(), "{change}");
        assert!(saved(root.path())?["slides"][0].get("audio").is_none());
    }
    Ok(())
}
#[test]
fn caller_text_model_or_audio_cannot_invoke_the_producer() -> anyhow::Result<()> {
    for field in ["text", "model", "audio_sha256", "owner_user_id"] {
        let (root, trusted) = fixture()?;
        let mut r = args();
        r["request"][field] = json!("forged");
        assert!(call(root.path(), &trusted, r, |_| panic!(
            "invalid DTO invoked provider"
        ))
        .is_err());
    }
    let (root, trusted) = fixture()?;
    let mut r = args();
    r["request"]["narration_text_sha256"] = json!("a".repeat(64));
    assert!(call(root.path(), &trusted, r, |_| panic!(
        "wrong slide invoked provider"
    ))
    .is_err());
    Ok(())
}
#[test]
fn failed_prerequisites_have_bounded_retry_but_uncertain_transport_never_resynthesizes(
) -> anyhow::Result<()> {
    let (root, trusted) = fixture()?;
    assert!(call(root.path(), &trusted, args(), |_| Err(
        SpeechError::MissingVoice
    ))
    .is_err());
    assert_eq!(
        call(root.path(), &trusted, args(), output)?["mutation"]["state"],
        "ready"
    );
    let (root, trusted) = fixture()?;
    assert!(call(root.path(), &trusted, args(), |_| Err(
        SpeechError::Transport
    ))
    .is_err());
    assert!(call(root.path(), &trusted, args(), |_| panic!(
        "uncertain synthesis repeated"
    ))
    .is_err());
    let mut other = args();
    other["request"]["operation_id"] = json!("another-attempt");
    assert!(call(root.path(), &trusted, other, |_| panic!(
        "unique slide synthesized twice"
    ))
    .is_err());
    let (root, trusted) = fixture()?;
    for _ in 0..3 {
        assert!(call(root.path(), &trusted, args(), |_| Err(
            SpeechError::MissingVoice
        ))
        .is_err());
    }
    assert!(call(root.path(), &trusted, args(), |_| panic!(
        "prerequisite retry ceiling exceeded"
    ))
    .is_err());
    Ok(())
}
#[test]
fn invalid_wav_or_wrong_native_text_receipt_never_creates_ready_audio() -> anyhow::Result<()> {
    for change in ["wav", "text"] {
        let (root, trusted) = fixture()?;
        assert!(call(root.path(), &trusted, args(), |r| Ok(
            crate::execution::speech::verified_fixture_output(
                if change == "text" { "wrong" } else { &r.text },
                if change == "wav" {
                    b"not a WAV".to_vec()
                } else {
                    wav()
                }
            )
        ))
        .is_err());
        assert_eq!(saved(root.path())?["state"], "preparing");
    }
    Ok(())
}
#[test]
fn custody_failure_rolls_back_file_chunks_reference_and_receipt_together() -> anyhow::Result<()> {
    let (root, trusted) = fixture()?;
    store::open_store(root.path())?.execute_batch("CREATE TRIGGER fail_audio_copy BEFORE INSERT ON business_records WHEN NEW.collection='desktop_file_chunks' BEGIN SELECT RAISE(ABORT,'isolated audio custody failure'); END;")?;
    assert!(call(root.path(), &trusted, args(), output).is_err());
    let policy = store::open_store(root.path())?;
    assert_eq!(policy.query_row("SELECT count(*) FROM business_records WHERE collection='desktop_files' AND json_extract(payload_json,'$.source')='ctox-jour-fixe-native-audio'",[],|r|r.get::<_,u64>(0))?,0);
    assert_eq!(policy.query_row("SELECT count(*) FROM workjet_jour_fixe_native_narration WHERE receipt_json IS NOT NULL",[],|r|r.get::<_,u64>(0))?,0);
    assert_eq!(saved(root.path())?["state"], "preparing");
    Ok(())
}
