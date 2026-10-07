// Origin: CTOX
// License: AGPL-3.0-only
//! Journal storage regressions on the actual native policy store.
//! Source/quorum/producer inputs remain fixtures; this is not model or handoff acceptance.
use super::*;
use std::time::Duration;

fn journal_fixture(session: &str, message: &str) -> Vec<u8> {
    let timestamp = "2026-10-07T02:00:00.123Z";
    let mut bytes = Vec::new();
    for line in [
        json!({"timestamp":timestamp,"type":"session_meta","payload":{
            "id":session,"timestamp":timestamp,"cwd":"/fixture/workspace",
            "originator":"codex_cli_rs","cli_version":ctox_core::native_harness_version(),
            "source":"exec","model_provider":"openai","base_instructions":{"text":"fixture"},
            "capability_profile":"workspace_worker"
        }}),
        json!({"timestamp":timestamp,"type":"event_msg","payload":{
            "type":"user_message","message":message
        }}),
    ] {
        serde_json::to_writer(&mut bytes, &line).unwrap();
        bytes.push(b'\n');
    }
    bytes
}

fn source_spec() -> (ExecutionSpec, Ownership) {
    (
        ExecutionSpec {
            job_id: "fixture-source-job".into(),
            session_id: "11111111-1111-1111-1111-111111111111".into(),
            scope_id: "native-test-scope".into(),
            harness: ctox_core::native_harness_name().into(),
            harness_version: ctox_core::native_harness_version().into(),
            model_route_id: "openai".into(),
            gateway_account_id: "fixture".into(),
            model_id: "model".into(),
            required_capabilities: BTreeSet::from(["fixture-requirement".into()]),
        },
        Ownership {
            node_id: 1,
            generation: 1,
        },
    )
}

#[test]
fn native_source_journal_reopens_exact_bytes_without_handoff_permission_or_payload_receipt() {
    let (root, registry, assignment) = fixture();
    let (spec, ownership) = source_spec();
    let (store, store_root, _) =
        super::super::source_journal::source_store(&assignment.destination.import_parent).unwrap();
    let bytes = journal_fixture(&spec.session_id, "private-fixture-history");
    let first = registry
        .with_policy(|tx| {
            super::super::source_journal::persist(
                tx,
                &store,
                &store_root,
                &assignment.destination,
                &spec,
                &ownership,
                "fixture-policy",
                &bytes,
            )
        })
        .unwrap();
    let second = registry
        .with_policy(|tx| {
            super::super::source_journal::persist(
                tx,
                &store,
                &store_root,
                &assignment.destination,
                &spec,
                &ownership,
                "fixture-policy",
                &bytes,
            )
        })
        .unwrap();
    assert_eq!(first, second);
    let wire = serde_json::to_string(&first).unwrap();
    assert!(!wire.contains("private-fixture-history"));
    assert!(!wire.contains("journal_bytes"));
    assert!(!wire.contains("artifact_store_path"));
    drop(store);
    let (store, reopened_root, _) =
        super::super::source_journal::source_store(&assignment.destination.import_parent).unwrap();
    assert_eq!(reopened_root, store_root);
    assert_eq!(
        std::fs::read_dir(store_root.join("manifests"))
            .unwrap()
            .count(),
        0
    );
    let conn = super::super::super::store::open_store(root.path()).unwrap();
    let artifact = ctox_sync::contracts::ArtifactRef {
        sha256: first.journal_sha256.clone(),
        size_bytes: first.journal_size_bytes,
    };
    use std::io::Read;
    let mut stored = Vec::new();
    store
        .open_blob(&artifact)
        .unwrap()
        .read_to_end(&mut stored)
        .unwrap();
    assert_eq!(stored, bytes);
    for table in [
        "business_session_handoff_bindings",
        "business_permission_grants",
    ] {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "journal publication cannot grant {table}");
    }
}

#[test]
fn native_source_journal_rejects_truncated_foreign_and_malformed_input_before_storage() {
    let (root, registry, assignment) = fixture();
    let (spec, ownership) = source_spec();
    let (store, store_root, _) =
        super::super::source_journal::source_store(&assignment.destination.import_parent).unwrap();
    let mut truncated = journal_fixture(&spec.session_id, "fixture");
    truncated.pop();
    for bytes in [
        truncated,
        journal_fixture("22222222-2222-2222-2222-222222222222", "fixture"),
        b"not a journal\n".to_vec(),
    ] {
        assert!(registry
            .with_policy(|tx| {
                super::super::source_journal::persist(
                    tx,
                    &store,
                    &store_root,
                    &assignment.destination,
                    &spec,
                    &ownership,
                    "fixture-policy",
                    &bytes,
                )
            })
            .is_err());
    }
    let conn = super::super::super::store::open_store(root.path()).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM business_native_source_journals",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        std::fs::read_dir(store_root.join("blobs")).unwrap().count(),
        0
    );
}

#[test]
fn native_source_journal_never_replaces_an_existing_capture_with_changed_policy_controller_or_bytes(
) {
    let (root, registry, assignment) = fixture();
    let (spec, ownership) = source_spec();
    let (store, store_root, _) =
        super::super::source_journal::source_store(&assignment.destination.import_parent).unwrap();
    let bytes = journal_fixture(&spec.session_id, "original-private-fixture");
    registry
        .with_policy(|tx| {
            super::super::source_journal::persist(
                tx,
                &store,
                &store_root,
                &assignment.destination,
                &spec,
                &ownership,
                "fixture-policy",
                &bytes,
            )
        })
        .unwrap();
    for mutation in ["policy", "controller", "payload"] {
        let mut destination = assignment.destination.clone();
        if mutation == "controller" {
            destination.controller_generation += 1;
        }
        let changed = if mutation == "payload" {
            journal_fixture(&spec.session_id, "different-private-fixture")
        } else {
            bytes.clone()
        };
        assert!(
            registry
                .with_policy(|tx| {
                    super::super::source_journal::persist(
                        tx,
                        &store,
                        &store_root,
                        &destination,
                        &spec,
                        &ownership,
                        if mutation == "policy" {
                            "changed-policy"
                        } else {
                            "fixture-policy"
                        },
                        &changed,
                    )
                })
                .is_err(),
            "{mutation}"
        );
    }
    let conn = super::super::super::store::open_store(root.path()).unwrap();
    let (sha256, size_bytes, policy, generation): (String, i64, String, i64) = conn
        .query_row(
            "SELECT journal_sha256,journal_size_bytes,policy_revision,controller_generation FROM business_native_source_journals",
            [],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
        )
        .unwrap();
    assert_eq!(policy, "fixture-policy");
    assert_eq!(
        generation,
        i64::try_from(assignment.destination.controller_generation).unwrap()
    );
    let artifact = ctox_sync::contracts::ArtifactRef {
        sha256,
        size_bytes: u64::try_from(size_bytes).unwrap(),
    };
    use std::io::Read;
    let mut stored = Vec::new();
    store
        .open_blob(&artifact)
        .unwrap()
        .read_to_end(&mut stored)
        .unwrap();
    assert_eq!(stored, bytes);
}

#[test]
fn native_source_journal_rejects_symlinked_or_public_artifact_directories() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let (_root, _registry, assignment) = fixture();
    let foreign = tempfile::tempdir().unwrap();
    let source = assignment.destination.import_parent.join("source-journals");
    symlink(foreign.path(), &source).unwrap();
    assert!(
        super::super::source_journal::source_store(&assignment.destination.import_parent).is_err()
    );
    assert_eq!(std::fs::read_dir(foreign.path()).unwrap().count(), 0);
    std::fs::remove_file(&source).unwrap();
    let (_store, root, _) =
        super::super::source_journal::source_store(&assignment.destination.import_parent).unwrap();
    std::fs::set_permissions(root.join("blobs"), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        super::super::source_journal::source_store(&assignment.destination.import_parent).is_err()
    );
}

#[test]
fn native_session_state_artifact_uses_actual_stopped_core_and_exact_policy_capture() {
    let (root, registry, assignment) = fixture();
    let workspace = super::workspace_tests::grant_workspace(root.path(), &assignment);
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(&workspace)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "fixture Git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "-q"]);
    git(&["config", "user.name", "Native capture fixture"]);
    git(&["config", "user.email", "capture@example.invalid"]);
    std::fs::write(workspace.join("tracked.txt"), "base\n").unwrap();
    std::fs::write(workspace.join("removed.txt"), "removed\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "unpublished native base"]);
    std::fs::write(workspace.join("tracked.txt"), "staged\n").unwrap();
    git(&["add", "tracked.txt"]);
    std::fs::write(workspace.join("tracked.txt"), "unstaged\n").unwrap();
    git(&["rm", "-q", "removed.txt"]);
    std::fs::write(workspace.join("untracked.bin"), b"required\0bytes").unwrap();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let model_url = format!("http://{}/v1", server.server_addr());
    let model = std::thread::spawn(move || {
        for _ in 0..4 {
            let mut request = server
                .recv_timeout(Duration::from_secs(30))
                .unwrap()
                .expect("bounded fixture model request");
            if request.method() != &tiny_http::Method::Post
                || !request.url().ends_with("/responses")
            {
                request
                    .respond(tiny_http::Response::from_string("{\"models\":[]}"))
                    .unwrap();
                continue;
            }
            use std::io::Read;
            let mut input = String::new();
            request
                .as_reader()
                .take(2 * 1024 * 1024)
                .read_to_string(&mut input)
                .unwrap();
            let events = [
                json!({"type":"response.created","response":{"id":"fixture-completed-response"}}),
                json!({"type":"response.output_item.done","item":{
                    "type":"message","role":"assistant","id":"fixture-message",
                    "content":[{"type":"output_text","text":"actual fixture reply"}]
                }}),
                json!({"type":"response.completed","response":{"id":"fixture-completed-response",
                    "usage":{"input_tokens":0,"input_tokens_details":null,"output_tokens":0,"output_tokens_details":null,"total_tokens":0}
                }}),
            ];
            let body = events
                .iter()
                .map(|event| {
                    format!(
                        "event: {}\ndata: {}\n\n",
                        event["type"].as_str().unwrap(),
                        event
                    )
                })
                .collect::<String>();
            let response = tiny_http::Response::from_string(body).with_header(
                tiny_http::Header::from_bytes("Content-Type", "text/event-stream").unwrap(),
            );
            request.respond(response).unwrap();
            return input;
        }
        panic!("fixture did not receive the model request");
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (configuration, state, journal_bytes) = runtime.block_on(async {
        let home = root.path().join("isolated-core-home");
        std::fs::create_dir(&home).unwrap();
        let mut config = ctox_core::config::ConfigBuilder::default().codex_home(home.clone()).build().await.unwrap();
        config.cwd = workspace.clone();
        config.model = Some("gpt-5.1".into());
        config.model_provider = ctox_core::ModelProviderInfo::create_openai_provider(Some(model_url));
        config.model_provider.requires_openai_auth = false;
        config.model_provider.supports_websockets = false;
        config.model_provider.env_key = None;
        config.model_provider.http_headers = None;
        config.model_provider.env_http_headers = None;
        config.model_provider.request_max_retries = Some(0);
        config.model_provider.stream_max_retries = Some(0);
        config.model_providers.insert("openai".into(), config.model_provider.clone());
        let auth = Arc::new(ctox_core::AuthManager::new(home, false, config.cli_auth_credentials_store_mode));
        let manager = ctox_core::ThreadManager::new(
            &config, auth, ctox_protocol::protocol::SessionSource::Exec,
            ctox_core::models_manager::collaboration_mode_presets::CollaborationModesConfig::default(),
        );
        let started = tokio::time::timeout(Duration::from_secs(15), manager.start_thread(config)).await.unwrap().unwrap();
        assert!(started.thread.capture_native_state().await.is_err());
        assert!(started.thread.retain_native_journal().await.is_err(), "unmaterialized journal remains rejected");
        let turn = started.thread.submit(ctox_protocol::protocol::Op::UserInput {
            items: vec![ctox_protocol::user_input::UserInput::Text {
                text: "capture original fixture turn".into(), text_elements: Vec::new()
            }],
            final_output_json_schema: None,
            required_initial_tool: None,
        }).await.unwrap();
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let event = started.thread.next_event().await.unwrap();
                if event.id == turn && matches!(event.msg, ctox_protocol::protocol::EventMsg::TurnComplete(_)) {
                    break;
                }
                assert!(!matches!(event.msg, ctox_protocol::protocol::EventMsg::Error(_)), "fixture model turn failed");
            }
        }).await.expect("bounded completed model fixture turn");
        let journal = started.thread.retain_native_journal().await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), started.thread.shutdown_and_wait()).await.unwrap().unwrap();
        let (configuration, state) = started.thread.capture_native_state().await.unwrap();
        assert_eq!(state.session_id(), started.thread_id);
        let journal_bytes = journal.read_bytes(64 * 1024 * 1024).unwrap();
        (configuration, state, journal_bytes)
    });
    let request = model.join().unwrap();
    assert!(request.contains("capture original fixture turn"));
    let captured: serde_json::Value = serde_json::from_slice(state.as_bytes()).unwrap();
    assert_eq!(
        captured["provider"]["lastResponse"]["responseId"],
        "fixture-completed-response"
    );
    assert!(captured["history"]
        .to_string()
        .contains("actual fixture reply"));
    let (mut spec, ownership) = source_spec();
    spec.session_id = state.session_id().to_string();
    spec.model_id = configuration.model.clone();
    let native_policy_revision = registry
        .with_policy(|tx| {
            super::super::accounts::configure_in_transaction(
                tx,
                "computer",
                &[super::super::accounts::ProviderAssignmentInput {
                    owner_user_id: "owner".into(),
                    worker_profile_id: "profile".into(),
                    gateway_account_id: spec.gateway_account_id.clone(),
                    model_id: spec.model_id.clone(),
                }],
            )?;
            super::super::validate_policy(tx, &assignment.destination)
        })
        .unwrap();
    let (store, store_root, _) =
        super::super::source_journal::source_store(&assignment.destination.import_parent).unwrap();
    let (receipt, artifact) = registry
        .with_policy(|tx| {
            let receipt = super::super::source_journal::persist(
                tx,
                &store,
                &store_root,
                &assignment.destination,
                &spec,
                &ownership,
                &native_policy_revision,
                &journal_bytes,
            )?;
            let artifact = super::super::source_journal::persist_session_state(
                tx, &store, &spec, &receipt, &state,
            )?;
            Ok((receipt, artifact))
        })
        .unwrap();
    let repeated = registry
        .with_policy(|tx| {
            super::super::source_journal::persist_session_state(tx, &store, &spec, &receipt, &state)
        })
        .unwrap();
    assert_eq!(artifact, repeated);
    let checkpoint = registry
        .with_policy(|tx| {
            super::super::source_checkpoint::persist(
                tx,
                &store,
                &store_root,
                &assignment.destination,
                &spec,
                &ownership,
                &receipt,
                &configuration,
                &state,
                &journal_bytes,
            )
        })
        .unwrap();

    let manifest = store.load(&checkpoint.digest).unwrap();
    assert_eq!(manifest.session.session_id, spec.session_id);
    assert_eq!(manifest.session.gateway_account_id, spec.gateway_account_id);
    assert_eq!(manifest.history.len(), 1);
    assert_eq!(manifest.history[0].sha256, receipt.journal_sha256);
    assert_eq!(manifest.provider_state.len(), 3);
    assert_eq!(
        manifest.pending_effects.len(),
        1,
        "external effects remain unresolved"
    );
    assert!(manifest.workspace_state.index_patch.size_bytes > 0);
    assert!(manifest.workspace_state.worktree_patch.size_bytes > 0);
    assert!(manifest
        .workspace_state
        .deleted_paths
        .contains("removed.txt"));
    assert!(manifest
        .workspace_state
        .required_untracked
        .iter()
        .any(|e| e.path == "untracked.bin"));
    let same = registry
        .with_policy(|tx| {
            super::super::source_checkpoint::persist(
                tx,
                &store,
                &store_root,
                &assignment.destination,
                &spec,
                &ownership,
                &receipt,
                &configuration,
                &state,
                &journal_bytes,
            )
        })
        .unwrap();
    assert_eq!(checkpoint, same);
    let restored = root.path().join("protected-source-copy");
    assert!(
        store.restore(&checkpoint.digest, &restored).is_err(),
        "unknown effects must also stop artifact restoration"
    );
    assert!(!restored.exists());
    let read_blob = |artifact: &ctox_sync::contracts::ArtifactRef| {
        let mut bytes = Vec::new();
        store
            .open_blob(artifact)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        bytes
    };
    let untracked = manifest
        .workspace_state
        .required_untracked
        .iter()
        .find(|e| e.path == "untracked.bin")
        .unwrap();
    assert_eq!(read_blob(&untracked.artifact), b"required\0bytes");
    let captured_state = manifest
        .provider_state
        .iter()
        .find(|e| e.path == "native-session-state.json")
        .unwrap();
    assert_eq!(read_blob(&captured_state.artifact), state.as_bytes());
    let captured_bundle = manifest
        .provider_state
        .iter()
        .find(|e| e.path == "native-workspace.bundle")
        .unwrap();
    // Source-owner artifact inspection, not a target activation bypass.
    let bundle = root.path().join("protected-source-repository.bundle");
    std::fs::write(&bundle, read_blob(&captured_bundle.artifact)).unwrap();
    let offline = root.path().join("offline-repository");
    let cloned = std::process::Command::new("git")
        .args(["clone", "--bare", "--quiet"])
        .arg(&bundle)
        .arg(&offline)
        .output()
        .unwrap();
    assert!(
        cloned.status.success(),
        "actual HEAD bundle must work without the source repository"
    );
    let object = std::process::Command::new("git")
        .arg("-C")
        .arg(&offline)
        .args([
            "cat-file",
            "-e",
            &format!("{}^{{commit}}", manifest.workspace_state.base_commit),
        ])
        .output()
        .unwrap();
    assert!(
        object.status.success(),
        "unpublished base objects were retained"
    );
    assert!(
        runtime
            .block_on(store.reconstruct_workspace(
                &checkpoint.digest,
                &offline,
                &root.path().join("forbidden-resume")
            ))
            .is_err(),
        "unknown effects must stop reconstruction"
    );
    let mut wrong_configuration = configuration.clone();
    wrong_configuration.cwd = root.path().into();
    assert!(registry
        .with_policy(|tx| {
            super::super::source_checkpoint::persist(
                tx,
                &store,
                &store_root,
                &assignment.destination,
                &spec,
                &ownership,
                &receipt,
                &wrong_configuration,
                &state,
                &journal_bytes,
            )
        })
        .is_err());
    std::fs::write(workspace.join("tracked.txt"), "late conflicting change\n").unwrap();
    assert!(
        registry
            .with_policy(|tx| {
                super::super::source_checkpoint::persist(
                    tx,
                    &store,
                    &store_root,
                    &assignment.destination,
                    &spec,
                    &ownership,
                    &receipt,
                    &configuration,
                    &state,
                    &journal_bytes,
                )
            })
            .is_err(),
        "exact retry cannot replace the first checkpoint"
    );
    // Handoff grant/regrant tests intentionally advance native policy epochs.
    // Run after capture retry checks, before explicit workspace revocation.
    super::source_handoff_tests::assert_native_source_handoff_enrollment(
        root.path(),
        &registry,
        &receipt,
        &spec,
        &ownership,
    );
    super::super::workspaces::revoke_workspace_assignment(
        root.path(),
        "owner",
        "profile",
        "project",
    )
    .unwrap();
    assert!(
        registry
            .with_policy(|tx| {
                super::super::source_checkpoint::persist(
                    tx,
                    &store,
                    &store_root,
                    &assignment.destination,
                    &spec,
                    &ownership,
                    &receipt,
                    &configuration,
                    &state,
                    &journal_bytes,
                )
            })
            .is_err(),
        "revocation denies subsequent protected source capture"
    );
    let mut foreign = spec.clone();
    foreign.session_id = uuid::Uuid::new_v4().to_string();
    assert!(registry
        .with_policy(|tx| {
            super::super::source_journal::persist_session_state(
                tx, &store, &foreign, &receipt, &state,
            )
        })
        .is_err());
    drop(store);
    let (store, _, _) =
        super::super::source_journal::source_store(&assignment.destination.import_parent).unwrap();
    use std::io::Read;
    let mut bytes = Vec::new();
    store
        .open_blob(&artifact)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, state.as_bytes());
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(parsed["sessionId"], spec.session_id);
    assert_eq!(parsed["provider"]["conversationId"], spec.session_id);
    assert_eq!(parsed["targetAuthority"], "reauthorization-required");
    registry.with_policy(|tx| {
        tx.execute("UPDATE business_native_source_session_states SET artifact_sha256=?1 WHERE capture_id=?2",
            rusqlite::params!["a".repeat(64),receipt.capture_id])?;
        Ok(())
    }).unwrap();
    assert!(registry
        .with_policy(|tx| {
            super::super::source_journal::persist_session_state(tx, &store, &spec, &receipt, &state)
        })
        .is_err());
}

fn configuration_fixture(cwd: &Path) -> ctox_core::ThreadConfigSnapshot {
    ctox_core::ThreadConfigSnapshot {
        model: "model".into(),
        model_provider_id: "openai".into(),
        service_tier: None,
        approval_policy: ctox_protocol::protocol::AskForApproval::Never,
        approvals_reviewer: ctox_protocol::config_types::ApprovalsReviewer::User,
        sandbox_policy: ctox_protocol::protocol::SandboxPolicy::DangerFullAccess,
        cwd: cwd.to_path_buf(),
        ephemeral: false,
        reasoning_effort: None,
        personality: None,
        session_source: ctox_protocol::protocol::SessionSource::Exec,
    }
}

#[test]
fn native_source_core_configuration_reopens_exact_settings_and_cannot_replace_a_capture() {
    let (root, registry, assignment) = fixture();
    let (spec, ownership) = source_spec();
    let (store, store_root, _) =
        super::super::source_journal::source_store(&assignment.destination.import_parent).unwrap();
    let configuration = configuration_fixture(&assignment.destination.import_parent);
    let bytes = journal_fixture(&spec.session_id, "fixture");
    let (receipt, artifact) = registry
        .with_policy(|tx| {
            let receipt = super::super::source_journal::persist(
                tx,
                &store,
                &store_root,
                &assignment.destination,
                &spec,
                &ownership,
                "fixture-policy",
                &bytes,
            )?;
            let artifact = super::super::source_journal::persist_core_configuration(
                tx,
                &store,
                &spec,
                &receipt,
                &configuration,
            )?;
            Ok((receipt, artifact))
        })
        .unwrap();
    let again = registry
        .with_policy(|tx| {
            super::super::source_journal::persist_core_configuration(
                tx,
                &store,
                &spec,
                &receipt,
                &configuration,
            )
        })
        .unwrap();
    assert_eq!(artifact, again);
    let mut changed = configuration.clone();
    changed.reasoning_effort = Some(ctox_protocol::openai_models::ReasoningEffort::High);
    assert!(registry
        .with_policy(
            |tx| super::super::source_journal::persist_core_configuration(
                tx, &store, &spec, &receipt, &changed
            )
        )
        .is_err());
    let mut foreign = spec.clone();
    foreign.session_id = "22222222-2222-2222-2222-222222222222".into();
    assert!(registry
        .with_policy(
            |tx| super::super::source_journal::persist_core_configuration(
                tx,
                &store,
                &foreign,
                &receipt,
                &configuration
            )
        )
        .is_err());
    drop(store);
    let (reopened, _, _) =
        super::super::source_journal::source_store(&assignment.destination.import_parent).unwrap();
    use std::io::Read;
    let mut original = Vec::new();
    reopened
        .open_blob(&artifact)
        .unwrap()
        .read_to_end(&mut original)
        .unwrap();
    assert_eq!(
        original,
        super::super::source_journal::core_configuration_bytes(&spec, &configuration).unwrap()
    );
    let parsed: serde_json::Value = serde_json::from_slice(&original).unwrap();
    assert_eq!(parsed["sessionId"], spec.session_id);
    assert_eq!(parsed["reasoningEffort"], serde_json::Value::Null);
    assert_eq!(parsed["providerContinuation"], "unresolved");
    assert_eq!(parsed["externalEffects"], "unknown");
    assert!(parsed.get("credentials").is_none() && parsed.get("commandSessionToken").is_none());
    let conn = super::super::super::store::open_store(root.path()).unwrap();
    let hash: String = conn.query_row("SELECT artifact_sha256 FROM business_native_source_core_configurations WHERE capture_id=?1",rusqlite::params![receipt.capture_id],|row|row.get(0)).unwrap();
    assert_eq!(hash, artifact.sha256);
    for table in [
        "business_session_handoff_bindings",
        "business_permission_grants",
    ] {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "Core settings do not grant {table}");
    }
}

#[test]
fn native_source_core_configuration_denies_foreign_provider_or_workspace_before_artifacts() {
    let (root, registry, assignment) = fixture();
    let (spec, ownership) = source_spec();
    let (store, store_root, _) =
        super::super::source_journal::source_store(&assignment.destination.import_parent).unwrap();
    let bytes = journal_fixture(&spec.session_id, "fixture");
    let receipt = registry
        .with_policy(|tx| {
            super::super::source_journal::persist(
                tx,
                &store,
                &store_root,
                &assignment.destination,
                &spec,
                &ownership,
                "fixture-policy",
                &bytes,
            )
        })
        .unwrap();
    for mutation in ["model", "provider", "ephemeral", "relative", "missing"] {
        let mut configuration = configuration_fixture(&assignment.destination.import_parent);
        match mutation {
            "model" => configuration.model = "foreign-model".into(),
            "provider" => configuration.model_provider_id = "ctox_core_api".into(),
            "ephemeral" => configuration.ephemeral = true,
            "relative" => configuration.cwd = PathBuf::from("relative"),
            "missing" => configuration.cwd = assignment.destination.import_parent.join("missing"),
            _ => unreachable!(),
        }
        assert!(
            registry
                .with_policy(
                    |tx| super::super::source_journal::persist_core_configuration(
                        tx,
                        &store,
                        &spec,
                        &receipt,
                        &configuration
                    )
                )
                .is_err(),
            "{mutation}"
        );
    }
    assert_eq!(
        std::fs::read_dir(store_root.join("blobs")).unwrap().count(),
        1
    );
    let conn = super::super::super::store::open_store(root.path()).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM business_native_source_core_configurations",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}
