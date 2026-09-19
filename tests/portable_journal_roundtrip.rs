use ctox_core::config::ConfigBuilder;
use ctox_core::{EventPersistenceMode, RolloutRecorder, RolloutRecorderParams};
use ctox_protocol::portable_journal::{
    artifact_ref_for, validate_portable_journal, ExternalEffectState, PortableJournalExpectation,
    PortableJournalLimits, ProviderContinuationState,
};
use ctox_protocol::protocol::{EventMsg, RolloutItem, SessionSource, UserMessageEvent};
use ctox_protocol::BaseInstructions;
use ctox_protocol::ThreadId;
use ctox_sync::capture::{CaptureEntry, CaptureRequest};
use ctox_sync::checkpoint::CheckpointStore;
use ctox_sync::contracts::{PendingEffect, SessionManifest, WorkspaceEntryKind};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

const SESSION_ID: &str = "11111111-1111-1111-1111-111111111111";

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn session() -> SessionManifest {
    SessionManifest {
        version: 1,
        scope_id: "test-scope".into(),
        session_id: SESSION_ID.into(),
        harness: "codex".into(),
        harness_version: "strict-portable-test".into(),
        model_route_id: "test-route".into(),
        gateway_account_id: "test-account-reference".into(),
        model_id: "test-model".into(),
        required_capabilities: BTreeSet::new(),
        credential_references: BTreeSet::new(),
    }
}

fn capture_request(root: &Path, journal: Vec<u8>) -> CaptureRequest {
    CaptureRequest {
        session: session(),
        sequence: 1,
        workspace_root: root.to_path_buf(),
        history: vec![journal],
        attachments: Vec::new(),
        workspace: Vec::new(),
        provider_state: vec![CaptureEntry {
            path: "provider-state.json".into(),
            kind: WorkspaceEntryKind::File,
            bytes: br#"{"state":"quiescent"}"#.to_vec(),
            executable: false,
        }],
        pending_effects: Vec::new(),
    }
}

#[tokio::test]
async fn harness_journal_roundtrips_through_strict_checkpoint_capture_and_restore() {
    let home = TempDir::new().expect("journal home");
    let config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .build()
        .await
        .expect("build harness config");
    let recorder = RolloutRecorder::new(
        &config,
        RolloutRecorderParams::new(
            ThreadId::from_string(SESSION_ID).expect("test thread id"),
            None,
            SessionSource::Exec,
            BaseInstructions::default(),
            Vec::new(),
            EventPersistenceMode::Limited,
        ),
        None,
        None,
    )
    .await
    .expect("create harness recorder");

    recorder
        .record_items(&[RolloutItem::EventMsg(EventMsg::UserMessage(
            UserMessageEvent {
                message: "private-journal-payload".into(),
                images: None,
                local_images: Vec::new(),
                text_elements: Vec::new(),
            },
        ))])
        .await
        .expect("record harness turn");
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    recorder.persist().await.expect("materialize journal");
    recorder.flush().await.expect("quiesce journal");
    recorder.shutdown().await.expect("stop recorder");

    let raw = fs::read(recorder.rollout_path()).expect("read harness journal");
    let artifact = artifact_ref_for(&raw);
    let expected = PortableJournalExpectation {
        format: ctox_protocol::portable_journal::PortableJournalFormat::current(),
        session_id: ThreadId::from_string(SESSION_ID).expect("test thread id"),
    };
    let validated = validate_portable_journal(
        &raw,
        &artifact,
        &expected,
        &PortableJournalLimits::default(),
    )
    .expect("validate real harness journal");
    let first_line: serde_json::Value = serde_json::from_slice(
        raw.split(|byte| *byte == b'\n')
            .next()
            .expect("first journal line"),
    )
    .expect("first journal JSON");
    let outer_timestamp = first_line.get("timestamp").and_then(|value| value.as_str());
    let creation_timestamp = first_line
        .pointer("/payload/timestamp")
        .and_then(|value| value.as_str());
    assert_ne!(outer_timestamp, creation_timestamp);
    assert_eq!(validated.record_count, 2);
    assert_eq!(validated.items.len(), 1);
    assert_eq!(
        validated.provider_continuation,
        ProviderContinuationState::Unresolved
    );
    assert_eq!(validated.external_effects, ExternalEffectState::Unknown);

    let workspace = TempDir::new().expect("workspace");
    let source = workspace.path().join("source");
    fs::create_dir(&source).expect("source directory");
    git(&source, &["init", "-q"]);
    git(
        &source,
        &["config", "user.email", "portable@example.invalid"],
    );
    git(&source, &["config", "user.name", "Portable Journal Test"]);
    fs::write(source.join("tracked.txt"), "base\n").expect("tracked file");
    git(&source, &["add", "tracked.txt"]);
    git(&source, &["commit", "-qm", "base"]);

    let store = CheckpointStore::open(workspace.path().join("store"), 1024 * 1024)
        .expect("open checkpoint store");
    let captured = store
        .capture(capture_request(&source, raw.clone()))
        .await
        .expect("capture quiescent checkpoint");
    assert_eq!(captured.manifest.history.len(), 1);
    assert_eq!(captured.manifest.history[0].sha256, artifact.sha256);
    assert_eq!(captured.manifest.history[0].size_bytes, artifact.size_bytes);

    let target = workspace.path().join("restored");
    let restored_manifest = store
        .restore(&captured.digest, &target)
        .expect("restore validated checkpoint");
    let restored_raw = fs::read(
        target
            .join("history")
            .join(&restored_manifest.history[0].sha256),
    )
    .expect("restored journal bytes");
    assert_eq!(restored_raw, raw);
    let restored_artifact = restored_manifest
        .history
        .first()
        .expect("restored history artifact");
    let restored_identity = ctox_protocol::portable_journal::PortableArtifactRef {
        sha256: restored_artifact.sha256.clone(),
        size_bytes: restored_artifact.size_bytes,
    };
    assert!(validate_portable_journal(
        &restored_raw,
        &restored_identity,
        &expected,
        &PortableJournalLimits::default(),
    )
    .is_ok());

    let mut hash_broken = raw.clone();
    let last_payload = hash_broken.len() - 2;
    hash_broken[last_payload] ^= 1;
    assert!(validate_portable_journal(
        &hash_broken,
        &artifact,
        &expected,
        &PortableJournalLimits::default(),
    )
    .is_err());
}

#[tokio::test]
async fn corrupt_journal_and_pending_external_effects_fail_import_closed() {
    let workspace = TempDir::new().expect("workspace");
    let source = workspace.path().join("source");
    fs::create_dir(&source).expect("source directory");
    git(&source, &["init", "-q"]);
    git(
        &source,
        &["config", "user.email", "portable@example.invalid"],
    );
    git(&source, &["config", "user.name", "Portable Journal Test"]);
    fs::write(source.join("tracked.txt"), "base\n").expect("tracked file");
    git(&source, &["add", "tracked.txt"]);
    git(&source, &["commit", "-qm", "base"]);
    let store = CheckpointStore::open(workspace.path().join("store"), 1024 * 1024)
        .expect("open checkpoint store");

    let corrupt = b"not-a-codex-journal\n".to_vec();
    assert!(store
        .capture(capture_request(&source, corrupt))
        .await
        .is_err());

    let valid = b"{\"timestamp\":\"2026-09-20T12:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"11111111-1111-1111-1111-111111111111\",\"timestamp\":\"2026-09-20T12:00:00Z\",\"cwd\":\"/original\",\"originator\":\"codex_cli_rs\",\"cli_version\":\"1.0.0\",\"source\":\"exec\",\"model_provider\":\"test-provider\",\"base_instructions\":{},\"capability_profile\":\"workspace_worker\"}}\n{\"timestamp\":\"2026-09-20T12:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"ready\",\"kind\":\"plain\"}}\n".to_vec();
    let mut pending = capture_request(&source, valid);
    pending.pending_effects.push(PendingEffect {
        effect_id: "external-publish".into(),
        idempotency_key: Some("external-key".into()),
        description: "unreconciled external effect".into(),
    });
    let captured = store
        .capture(pending)
        .await
        .expect("capture pending evidence");
    let target = workspace.path().join("pending-target");
    assert!(store.restore(&captured.digest, &target).is_err());
    assert!(!target.exists());
}
