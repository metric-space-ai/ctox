use super::*;
use ctox_sync::{
    authority::auth::speech_wire::SignedSpeechRequest,
    contracts::{ExecutionPeer, SyncHostMember, SyncHostTiming},
    host_config::HostConfiguration,
};
use std::collections::BTreeMap;

#[test]
fn speech_target_issuer_contention_has_a_bounded_fail_closed_wait() {
    let f = fixture();
    let (policy, _) = TargetPolicy::verify(f.root.path(), open(&f, "busy-budget")).unwrap();
    let holder = hold_current_issuer(&f, false, 300);
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let started = std::time::Instant::now();
    let result = policy.with_current(|_| {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    });
    let elapsed = started.elapsed();
    holder.join().unwrap();
    assert!(matches!(result, Err(Denial::RouteRetired)));
    assert!(elapsed < std::time::Duration::from_millis(250));
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

fn hold_current_issuer(f: &Fixture, revoke: bool, hold_ms: u64) -> std::thread::JoinHandle<()> {
    let root = f.root.path().to_path_buf();
    let (ready, started) = std::sync::mpsc::sync_channel(0);
    let task = std::thread::spawn(move || {
        crate::sync_host::with_current_signing_identity(&root, |_| {
            if revoke {
                // Prepare the real revocation under the issuer fence before
                // announcing the bounded contention interval. SQLite commit
                // latency is not part of the requested 20 ms hold.
                crate::persistence::store_json_payload(
                    &root,
                    CONFIG_KEY,
                    Some(&Saved {
                        epoch: Uuid::new_v4().to_string(),
                        config: SpeechTargetConfig::default(),
                    }),
                )?;
            }
            ready.send(()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(hold_ms));
            Ok(())
        })
        .unwrap();
    });
    started
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();
    // Prove this test reaches the real encrypted-store contention boundary.
    assert!(crate::sync_host::with_current_signing_identity(f.root.path(), |_| Ok(())).is_err());
    task
}

#[test]
fn speech_target_waits_for_short_issuer_contention_without_replaying_effects() {
    let f = fixture();
    let (policy, _) = TargetPolicy::verify(f.root.path(), open(&f, "contention")).unwrap();
    let holder = hold_current_issuer(&f, false, 20);
    let calls = std::sync::atomic::AtomicUsize::new(0);
    policy
        .with_current(|_| {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
    holder.join().unwrap();
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn speech_target_rechecks_revocation_after_issuer_contention() {
    let f = fixture();
    let (policy, _) = TargetPolicy::verify(f.root.path(), open(&f, "revocation")).unwrap();
    let holder = hold_current_issuer(&f, true, 20);
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let result = policy.with_current(|_| {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    });
    holder.join().unwrap();
    assert!(matches!(result, Err(Denial::GrantDenied)));
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn speech_target_rechecks_expiry_after_issuer_contention() {
    let mut f = fixture();
    let expiry = now_ms() + 2_000;
    f.config.grants[0].expires_at_unix_ms = expiry;
    f.config.save(f.root.path()).unwrap();
    let (policy, _) = TargetPolicy::verify(f.root.path(), open(&f, "expiry")).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(
        expiry.saturating_sub(now_ms() + 10),
    ));
    let holder = hold_current_issuer(&f, false, 20);
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let result = policy.with_current(|_| {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    });
    holder.join().unwrap();
    assert!(matches!(result, Err(Denial::GrantExpired)));
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn speech_target_issuer_does_not_retry_an_entered_callback_failure() {
    let f = fixture();
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let result: anyhow::Result<()> = with_speech_key(f.root.path(), |_| {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        anyhow::bail!("secret master-key authority is unavailable")
    });
    assert!(result.is_err());
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

struct Fixture {
    root: tempfile::TempDir,
    source: SigningIdentity,
    target: Arc<SigningIdentity>,
    config: SpeechTargetConfig,
}
fn key() -> SigningIdentity {
    SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap()
}
fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    crate::sync_host::handle_command(root.path(), &["init".into()]).unwrap();
    let target = crate::sync_host::signing_identity(root.path()).unwrap();
    let source = key();
    let third = key();
    let peer = |identity: String| ExecutionPeer {
        identity,
        executor: true,
        data_replica: true,
    };
    let host = HostConfiguration {
        version: 1,
        scope_id: "isolated-speech".into(),
        local: SyncHostMember::Voter { node_id: 1 },
        voters: BTreeMap::from([
            (1, peer(target.public_identity())),
            (2, peer(source.public_identity())),
            (3, peer(third.public_identity())),
        ]),
        timing: SyncHostTiming {
            heartbeat_ms: 100,
            election_min_ms: 300,
            election_max_ms: 600,
        },
    };
    crate::persistence::store_text_value(root.path(), "speech-fixture", Some("present")).unwrap();
    ctox_sync::host_config::save(
        &mut rusqlite::Connection::open(crate::inference::runtime_env::runtime_config_path(
            root.path(),
        ))
        .unwrap(),
        &host,
    )
    .unwrap();
    let config = SpeechTargetConfig {
        grants: vec![SpeechTargetGrant {
            scope_id: host.scope_id,
            source_signing_identity: source.public_identity(),
            target_signing_identity: target.public_identity(),
            expires_at_unix_ms: now_ms() + 60_000,
            binding: SpeechComputerBinding {
                source_instance_id: "meeting-instance".into(),
                target_instance_id: "isolated-gpu".into(),
                computer_id: "gpu3".into(),
                owner_user_id: "owner-a".into(),
                grant_id: "speech-grant".into(),
                grant_revision: 1,
                workload: SpeechWorkload::Transcription,
                model: "engineai/Voxtral-Mini-4B-Realtime-2602".into(),
            },
        }],
    };
    config.save(root.path()).unwrap();
    Fixture {
        root,
        source,
        target,
        config,
    }
}
fn signed(
    f: &Fixture,
    binding: SpeechComputerBinding,
    operation: Op,
    source: &SigningIdentity,
) -> Value {
    SignedSpeechRequest::new(
        source,
        &f.target.public_identity(),
        "isolated-speech",
        SpeechComputerRequest { binding, operation },
    )
    .unwrap()
    .envelope
}
fn open(f: &Fixture, id: &str) -> Value {
    signed(
        f,
        f.config.grants[0].binding.clone(),
        Op::OpenTranscription {
            intent_id: id.into(),
            sample_rate_hz: 16_000,
        },
        &f.source,
    )
}

#[test]
fn speech_target_policy_requires_the_exact_grant_owner_model_revision_and_sender() {
    let f = fixture();
    assert!(TargetPolicy::verify(f.root.path(), open(&f, "intent")).is_ok());
    for changed in ["owner", "model", "revision", "computer"] {
        let mut binding = f.config.grants[0].binding.clone();
        match changed {
            "owner" => binding.owner_user_id = "owner-b".into(),
            "model" => binding.model = "cloud-model".into(),
            "revision" => binding.grant_revision += 1,
            "computer" => binding.computer_id = "gpu4".into(),
            _ => unreachable!(),
        }
        let envelope = signed(
            &f,
            binding,
            Op::OpenTranscription {
                intent_id: "intent".into(),
                sample_rate_hz: 16_000,
            },
            &f.source,
        );
        assert!(
            matches!(
                TargetPolicy::verify(f.root.path(), envelope),
                Err(Denial::GrantDenied)
            ),
            "{changed}"
        );
    }
    let foreign = key();
    let envelope = signed(
        &f,
        f.config.grants[0].binding.clone(),
        Op::OpenTranscription {
            intent_id: "intent".into(),
            sample_rate_hz: 16_000,
        },
        &foreign,
    );
    assert!(matches!(
        TargetPolicy::verify(f.root.path(), envelope),
        Err(Denial::GrantDenied)
    ));
}

#[test]
fn speech_target_policy_reconfiguration_revokes_prepared_authority_without_rotating_identity() {
    let f = fixture();
    let (policy, _) = TargetPolicy::verify(f.root.path(), open(&f, "intent")).unwrap();
    let before = f.target.public_identity();
    policy.with_current(|_| Ok(())).unwrap();
    SpeechTargetConfig::default().save(f.root.path()).unwrap();
    assert!(matches!(
        policy.with_current(|_| Ok(())),
        Err(Denial::GrantDenied)
    ));
    assert_eq!(
        crate::sync_host::signing_identity(f.root.path())
            .unwrap()
            .public_identity(),
        before
    );
}

#[test]
fn speech_target_policy_intents_survive_restart_and_never_restart_the_same_effect() {
    let f = fixture();
    let envelope = open(&f, "same-intent");
    let (policy, request) = TargetPolicy::verify(f.root.path(), envelope.clone()).unwrap();
    let IntentAdmission::New(first) = policy
        .reserve_intent(&request, "host-generation-a")
        .unwrap()
    else {
        panic!("first admission");
    };
    let (loaded, replayed) = TargetPolicy::verify(f.root.path(), envelope).unwrap();
    let IntentAdmission::Existing(again) = loaded
        .reserve_intent(&replayed, "host-generation-a")
        .unwrap()
    else {
        panic!("replay");
    };
    assert_eq!(first, again);
    assert!(matches!(
        loaded.reserve_intent(&replayed, "host-generation-b"),
        Err(Denial::RouteRetired)
    ));
    let (_, next) = TargetPolicy::verify(f.root.path(), open(&f, "new-intent")).unwrap();
    assert!(matches!(
        loaded.reserve_intent(&next, "host-generation-b"),
        Ok(IntentAdmission::New(_))
    ));
}

#[test]
fn speech_target_policy_same_synthesis_intent_cannot_change_text_or_voice() {
    let mut f = fixture();
    f.config.grants[0].binding.workload = SpeechWorkload::Synthesis;
    f.config.grants[0].binding.model = "engineai/Voxtral-4B-TTS-2603".into();
    f.config.save(f.root.path()).unwrap();
    let operation = |text: &str, voice: &str| Op::StartSynthesis {
        intent_id: "narration-intent".into(),
        text: text.into(),
        voice_id: voice.into(),
    };
    let text = "Confidential meeting narration";
    let envelope = signed(
        &f,
        f.config.grants[0].binding.clone(),
        operation(text, "neutral_female"),
        &f.source,
    );
    let (policy, request) = TargetPolicy::verify(f.root.path(), envelope).unwrap();
    assert!(matches!(
        policy.reserve_intent(&request, "host-a"),
        Ok(IntentAdmission::New(_))
    ));
    for op in [
        operation("different text", "neutral_female"),
        operation(text, "other_voice"),
    ] {
        let envelope = signed(&f, f.config.grants[0].binding.clone(), op, &f.source);
        let (_, request) = TargetPolicy::verify(f.root.path(), envelope).unwrap();
        assert!(matches!(
            policy.reserve_intent(&request, "host-a"),
            Err(Denial::InvalidSequence)
        ));
    }
    let ledger: Ledger = crate::persistence::load_json_payload(f.root.path(), LEDGER_KEY)
        .unwrap()
        .unwrap();
    assert!(
        !serde_json::to_string(&ledger).unwrap().contains(text),
        "raw narration persisted in intent ledger"
    );
}

#[test]
fn speech_target_policy_rejects_cloud_grants_and_preserves_existing_configuration() {
    let f = fixture();
    let mut invalid = f.config.clone();
    invalid.grants[0].binding.model = "paid-cloud-model".into();
    assert!(invalid.save(f.root.path()).is_err());
    invalid = f.config.clone();
    invalid.grants[0].target_signing_identity = key().public_identity();
    assert!(invalid.save(f.root.path()).is_err());
    invalid = f.config.clone();
    invalid.grants[0].expires_at_unix_ms = now_ms() + 86_400_100;
    assert!(invalid.save(f.root.path()).is_err());
    invalid = f.config.clone();
    invalid.grants.push(invalid.grants[0].clone());
    assert!(invalid.save(f.root.path()).is_err());
    assert!(TargetPolicy::verify(f.root.path(), open(&f, "still-valid")).is_ok());
}
