use super::*;
use ctox_sync::{
    authority::auth::speech_wire::SignedSpeechRequest,
    contracts::{ExecutionPeer, SyncHostMember, SyncHostTiming},
    host_config::HostConfiguration,
};
use std::collections::BTreeMap;

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
