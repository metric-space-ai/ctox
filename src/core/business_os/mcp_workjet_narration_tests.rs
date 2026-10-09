// Origin: CTOX
// License: AGPL-3.0-only

/// Execute the real authenticated /mcp handler, including token verification,
/// policy and tool dispatch, on the same bare std thread as local serve_mcp_channel.
fn http_call(root: &Path, token: &str, args: Value, managed: bool) -> anyhow::Result<Value> {
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|error| anyhow::anyhow!("test MCP listener: {error}"))?;
    let listener = server
        .server_addr()
        .to_ip()
        .context("MCP listener address")?;
    let bearer = super::super::mcp_operator_auth_token(root)?;
    let root = root.to_owned();
    let handler = std::thread::spawn(move || -> anyhow::Result<()> {
        let request = server
            .recv_timeout(std::time::Duration::from_secs(10))?
            .context("MCP request not received")?;
        if managed {
            // Protect the existing managed-gateway spawn_blocking path too.
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()?;
            runtime.block_on(async move {
                tokio::task::spawn_blocking(move || {
                    super::super::handle_mcp_http_request(&root, listener, request)
                })
                .await?
            })
        } else {
            assert!(tokio::runtime::Handle::try_current().is_err());
            super::super::handle_mcp_http_request(&root, listener, request)
        }
    });
    let response = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .post(&format!("http://{listener}/mcp"))
        .set("authorization", &format!("Bearer {bearer}"))
        .set(super::super::MCP_INTERNAL_SESSION_HEADER, token)
        .send_json(json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":super::super::workjet_jour_fixe::WRITE_TOOL,"arguments":args}}));
    handler.join().expect("MCP handler panicked")?;
    Ok(response?.into_json()?)
}

fn http_configured_mistral_retry(managed: bool) -> anyhow::Result<()> {
    use crate::execution::speech::{MistralTestEndpoint, SpeechBackend, SpeechRuntimeConfig};
    let (root, trusted) = fixture()?;
    let token = super::super::issue_internal_command_session_token(
        root.path(),
        trusted["command_id"].as_str().context("command id")?,
        trusted["payload_hash"].as_str().context("payload hash")?,
        "owner",
        "chef",
        "native-job-workspace",
        &json!({}),
    )?;
    let token =
        super::super::restrict_internal_command_session_to_workjet_supervisor(root.path(), &token)?;
    crate::secrets::set_credential(root.path(), "CTOX_MISTRAL_API_KEY", "fixture-mistral-key")?;
    let mut config = SpeechRuntimeConfig {
        synthesis: SpeechBackend::Mistral,
        transcription: SpeechBackend::Mistral,
        voice_id: None,
    };
    config.save(root.path())?;
    // The first HTTP call fails for a genuine prerequisite. No provider is called.
    let failed = http_call(root.path(), &token, args(), managed)?;
    assert!(
        failed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("MissingVoice"),
        "{failed}"
    );
    assert_eq!(saved(root.path())?["state"], "preparing");
    let (failed_state, failed_code): (String, String) = store::open_store(root.path())?.query_row(
        "SELECT state,error_class FROM workjet_jour_fixe_native_narration WHERE operation_id='narrate-op'",
        [], |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(failed_state, "failed_prerequisite");
    assert_eq!(
        serde_json::from_str::<Value>(&failed_code)?["code"],
        "missing_voice"
    );

    let provider = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|error| anyhow::anyhow!("test speech listener: {error}"))?;
    let address = provider
        .server_addr()
        .to_ip()
        .context("speech listener address")?;
    let _endpoint =
        MistralTestEndpoint::new(root.path(), format!("http://{address}/v1/audio/speech"));
    let server = std::thread::spawn(move || -> anyhow::Result<()> {
        let mut request = provider
            .recv_timeout(std::time::Duration::from_secs(10))?
            .context("speech request not received")?;
        assert_eq!(request.method(), &tiny_http::Method::Post);
        assert_eq!(request.url(), "/v1/audio/speech");
        let mut body = String::new();
        request.as_reader().read_to_string(&mut body)?;
        let body: Value = serde_json::from_str(&body)?;
        assert_eq!(body["model"], crate::execution::speech::MISTRAL_TTS_MODEL);
        assert_eq!(body["voice_id"], "fixture-saved-voice");
        assert_eq!(body["input"], TEXT);
        assert_eq!(body["response_format"], "wav");
        request.respond(tiny_http::Response::from_string(
            json!({"audio_data":base64::engine::general_purpose::STANDARD.encode(wav())})
                .to_string(),
        ))?;
        Ok(())
    });
    config.voice_id = Some("fixture-saved-voice".into());
    config.save(root.path())?;
    let mut retry = args();
    retry["request"]["operation_id"] = json!("http-narration-retry");
    let response = http_call(root.path(), &token, retry.clone(), managed)?;
    server.join().expect("speech fixture panicked")?;
    assert!(response.get("error").is_none(), "{response}");
    let result = &response["result"]["structuredContent"];
    assert_eq!(result["mutation"]["state"], "ready");
    assert_eq!(result["native_narration"]["provider_verified"], true);
    assert_eq!(result["native_narration"]["audio"]["sha256"], hash(&wav()));
    let meeting = saved(root.path())?;
    assert_eq!(meeting["state"], "ready");
    assert!(meeting["slides"][0]["audio"].is_object());
    let policy = store::open_store(root.path())?;
    let (operation, state, attempts): (String, String, u64) = policy.query_row(
        "SELECT operation_id,state,attempts FROM workjet_jour_fixe_native_narration",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    assert_eq!(
        (operation.as_str(), state.as_str(), attempts),
        ("http-narration-retry", "complete", 2)
    );
    let mut stmt = policy.prepare("SELECT payload_json FROM business_records WHERE collection='desktop_file_chunks' ORDER BY json_extract(payload_json,'$.idx')")?;
    let chunks = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let encoded = chunks
        .iter()
        .map(|raw| {
            serde_json::from_str::<Value>(raw).unwrap()["data"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect::<String>();
    assert_eq!(
        base64::engine::general_purpose::STANDARD.decode(encoded)?,
        wav()
    );
    // Replaying the acknowledged operation does not reach the now-closed provider.
    assert_eq!(http_call(root.path(), &token, retry, managed)?, response);
    Ok(())
}

#[test]
fn local_http_std_thread_narrates_configured_mistral_and_retries_failed_slide() -> anyhow::Result<()>
{
    http_configured_mistral_retry(false)
}

#[test]
fn managed_http_spawn_blocking_narrates_without_nested_runtime() -> anyhow::Result<()> {
    http_configured_mistral_retry(true)
}

#[test]
fn executor_failure_is_retryable_and_not_a_configuration_prerequisite() -> anyhow::Result<()> {
    let (root, trusted) = fixture()?;
    assert!(call(root.path(), &trusted, args(), |_| Err(
        SpeechError::ExecutionUnavailable
    ))
    .is_err());
    let policy = store::open_store(root.path())?;
    let (state, error): (String, String) = policy.query_row(
        "SELECT state,error_class FROM workjet_jour_fixe_native_narration",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(state, "failed");
    assert_eq!(
        serde_json::from_str::<Value>(&error)?,
        json!({"code":"execution_unavailable"})
    );
    let mut retry = args();
    retry["request"]["operation_id"] = json!("executor-retry");
    assert_eq!(
        call(root.path(), &trusted, retry, output)?["mutation"]["state"],
        "ready"
    );
    Ok(())
}
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
fn configuration_unavailable_then_new_operation_commits_audio() -> anyhow::Result<()> {
    let (root, trusted) = fixture()?;
    let configured = std::cell::Cell::new(false);
    let producer = |request: &SpeechRequest| {
        if configured.get() {
            output(request)
        } else {
            Err(SpeechError::ConfigurationUnavailable)
        }
    };
    assert!(call(root.path(), &trusted, args(), producer).is_err());
    assert!(saved(root.path())?["slides"][0]["audio"].is_null());
    let policy = store::open_store(root.path())?;
    assert_eq!(
        policy.query_row(
            "SELECT state FROM workjet_jour_fixe_native_narration WHERE operation_id='narrate-op'",
            [],
            |row| row.get::<_, String>(0),
        )?,
        "failed_prerequisite"
    );

    configured.set(true);
    let mut retry = args();
    retry["request"]["operation_id"] = json!("configured-retry");
    let result = call(root.path(), &trusted, retry.clone(), producer)?;
    assert_eq!(
        result["native_narration"]["operation_id"],
        "configured-retry"
    );
    assert_eq!(result["mutation"]["state"], "ready");
    assert_eq!(
        saved(root.path())?["slides"][0]["audio"]["sha256"],
        hash(&wav())
    );
    assert_eq!(
        policy.query_row(
            "SELECT operation_id,state,attempts FROM workjet_jour_fixe_native_narration",
            [],
            |row| Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u64>(2)?
            )),
        )?,
        ("configured-retry".into(), "complete".into(), 2)
    );
    assert_eq!(
        call(root.path(), &trusted, retry, |_| panic!(
            "retry receipt resynthesized"
        ))?,
        result
    );
    let error = call(root.path(), &trusted, args(), |_| {
        panic!("old operation resynthesized")
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("operation_id 'configured-retry'"), "{error}");
    assert!(error.contains("status 'complete'"), "{error}");
    Ok(())
}

#[test]
fn failed_slide_slot_can_retry_with_a_new_operation() -> anyhow::Result<()> {
    let (root, trusted) = fixture()?;
    assert!(call(root.path(), &trusted, args(), |_| Err(
        SpeechError::MissingVoice
    ))
    .is_err());
    store::open_store(root.path())?.execute(
        "UPDATE workjet_jour_fixe_native_narration SET state='failed' WHERE operation_id='narrate-op'",
        [],
    )?;
    let mut retry = args();
    retry["request"]["operation_id"] = json!("failed-retry");
    call(root.path(), &trusted, retry, output)?;
    assert!(saved(root.path())?["slides"][0]["audio"].is_object());
    Ok(())
}

#[test]
fn fresh_operation_ids_cannot_bypass_the_slide_retry_budget() -> anyhow::Result<()> {
    let (root, trusted) = fixture()?;
    for attempt in 1..=3 {
        let mut retry = args();
        retry["request"]["operation_id"] = json!(format!("retry-{attempt}"));
        assert!(call(root.path(), &trusted, retry, |_| Err(
            SpeechError::ConfigurationUnavailable
        ))
        .is_err());
    }
    let mut retry = args();
    retry["request"]["operation_id"] = json!("retry-4");
    let error = call(root.path(), &trusted, retry, |_| {
        panic!("slide retry ceiling exceeded")
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("operation_id 'retry-3'"), "{error}");
    assert!(error.contains("status 'failed_prerequisite'"), "{error}");
    assert!(error.contains("attempts 3"), "{error}");
    assert!(saved(root.path())?["slides"][0]["audio"].is_null());
    Ok(())
}

#[test]
fn running_and_uncertain_slot_conflicts_name_the_existing_operation() -> anyhow::Result<()> {
    let (root, trusted) = fixture()?;
    let mut retry = args();
    retry["request"]["operation_id"] = json!("overlapping-retry");
    assert!(call(root.path(), &trusted, args(), |_| {
        let error = call(root.path(), &trusted, retry.clone(), |_| {
            panic!("overlapping synthesis")
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("operation_id 'narrate-op'"), "{error}");
        assert!(error.contains("status 'reserved'"), "{error}");
        Err(SpeechError::Transport)
    })
    .is_err());
    let error = call(root.path(), &trusted, retry, |_| {
        panic!("uncertain synthesis repeated")
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("operation_id 'narrate-op'"), "{error}");
    assert!(error.contains("status 'uncertain'"), "{error}");
    Ok(())
}

#[test]
fn failed_operation_id_cannot_be_rebound_to_another_slide() -> anyhow::Result<()> {
    let (root, trusted) = fixture()?;
    let mut meeting = saved(root.path())?;
    meeting["slides"].as_array_mut().unwrap().push(json!({"id":"slide-2","meeting_id":"meeting-1","position":1,"title":"Decisions","body_markdown":"A decision is pending."}));
    store::open_store(root.path())?.execute(
        "UPDATE workjet_jour_fixe_meetings SET metadata_json=?1",
        [meeting.to_string()],
    )?;
    assert!(call(root.path(), &trusted, args(), |_| Err(
        SpeechError::ConfigurationUnavailable
    ))
    .is_err());
    let mut other = args();
    other["request"]["slide_id"] = json!("slide-2");
    other["request"]["narration_text_sha256"] = json!(hash(b"A decision is pending."));
    let error = call(root.path(), &trusted, other, |_| {
        panic!("operation ID rebound")
    })
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("narration operation intent conflicts"),
        "{error}"
    );
    let policy = store::open_store(root.path())?;
    assert_eq!(policy.query_row(
        "SELECT slide_id,state,attempts FROM workjet_jour_fixe_native_narration WHERE operation_id='narrate-op'",
        [], |row| Ok((row.get::<_, String>(0)?,row.get::<_, String>(1)?,row.get::<_, u64>(2)?)),
    )?, ("slide-1".into(), "failed_prerequisite".into(), 1));
    let mut retry = args();
    retry["request"]["operation_id"] = json!("same-slide-retry");
    call(root.path(), &trusted, retry, output)?;
    assert!(saved(root.path())?["slides"][0]["audio"].is_object());
    assert!(saved(root.path())?["slides"][1]["audio"].is_null());
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

#[test]
fn native_read_supplies_exact_narration_hashes_until_audio_is_committed() -> anyhow::Result<()> {
    let (root, trusted) = fixture()?;
    let request = json!({"action":"read_meeting","request":{"project_id":"project","meeting_id":"meeting-1"}});
    let context = super::super::context_from_arguments_with_trusted_gateway_context(
        workjet_jour_fixe::READ_TOOL,
        &request,
        Some(&trusted),
    )?;
    let read = workjet_jour_fixe::execute(
        root.path(),
        &context,
        workjet_jour_fixe::READ_TOOL,
        &request,
        Some(&trusted),
    )?;
    let input = &read["narration_inputs"][0];
    assert_eq!(input["narration_text_sha256"], hash(TEXT.as_bytes()));
    let mut narrate = args();
    for field in [
        "slide_id",
        "deck_revision",
        "expected_revision",
        "narration_text_sha256",
    ] {
        narrate["request"][field] = input[field].clone();
    }
    call(root.path(), &trusted, narrate, output)?;
    let read = workjet_jour_fixe::execute(
        root.path(),
        &context,
        workjet_jour_fixe::READ_TOOL,
        &request,
        Some(&trusted),
    )?;
    assert_eq!(read["narration_inputs"], json!([]));
    Ok(())
}
