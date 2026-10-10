// Origin: CTOX
// License: AGPL-3.0-only
//! Composition regressions use checked shutdown of real Core. Machine/quorum
//! metadata below is component input, not installed VM or execution acceptance.
use super::*;
use ctox_core::features::Feature;

async fn quiet_core_state() -> Result<ctox_core::NativeSessionState> {
    let root = tempfile::tempdir()?;
    let home = root.path().join("core");
    std::fs::create_dir(&home)?;
    let mut config = ctox_core::config::ConfigBuilder::default()
        .codex_home(home.clone())
        .build()
        .await?;
    config.cwd = root.path().into();
    config.model_provider.requires_openai_auth = false;
    config.model_provider.supports_websockets = false;
    config.notify = Some(Vec::new());
    for feature in [
        Feature::ShellSnapshot,
        Feature::ShellZshFork,
        Feature::CodexHooks,
        Feature::MemoryTool,
        Feature::GhostCommit,
        Feature::Plugins,
        Feature::SkillMcpDependencyInstall,
        Feature::SkillEnvVarDependencyPrompt,
        Feature::JsRepl,
        Feature::CodeMode,
        Feature::CodeModeOnly,
        Feature::JsReplToolsOnly,
        Feature::RealtimeConversation,
    ] {
        config.features.disable(feature)?;
    }
    let auth = Arc::new(ctox_core::AuthManager::new(
        home,
        false,
        config.cli_auth_credentials_store_mode,
    ));
    let manager = ctox_core::ThreadManager::new(
        &config,
        auth,
        ctox_protocol::protocol::SessionSource::Exec,
        ctox_core::models_manager::collaboration_mode_presets::CollaborationModesConfig::default(),
    );
    let loaded = tokio::time::timeout(Duration::from_secs(30), manager.start_thread(config))
        .await
        .context("source-effect Core startup deadline")??;
    loaded.thread.register_native_source_factory()?;
    tokio::time::timeout(Duration::from_secs(10), loaded.thread.shutdown_and_wait())
        .await
        .context("source-effect Core shutdown deadline")??;
    let (_, state) = loaded.thread.capture_native_state().await?;
    ensure!(
        !state
            .core_effect_capture()
            .context("actual Core capture missing")?
            .requires_reconciliation(),
        "quiet original Core has unknown startup effects"
    );
    Ok(state)
}

#[tokio::test]
async fn native_source_clean_effects_require_every_retained_witness() -> Result<()> {
    let state = quiet_core_state().await?;
    let spec = ExecutionSpec {
        job_id: "composition-job".into(),
        session_id: state.session_id().to_string(),
        scope_id: "composition-scope".into(),
        harness: ctox_core::native_harness_name().into(),
        harness_version: ctox_core::native_harness_version().into(),
        model_route_id: state.provider_id().into(),
        gateway_account_id: "fixture".into(),
        model_id: state.model().into(),
        required_capabilities: BTreeSet::new(),
    };
    let ownership = Ownership {
        node_id: 1,
        generation: 1,
    };
    let make = || -> Result<SourceEffects> {
        let mut effects = SourceEffects::fixture(&spec, &ownership, BTreeSet::new());
        effects.bind_core_state(&state)?;
        // The production equivalents come only from verify_controller and its
        // actual exported child, with fresh authority checked before and after IO.
        effects.process = Some(GuestProcessEffect {
            effect_id: "composition-process".into(),
            job_id: spec.job_id.clone(),
            ownership: ownership.clone(),
            controller_id: "composition-controller".into(),
            controller_generation: 1,
            process_instance_id: "composition-instance".into(),
        });
        effects
            .job
            .completed_effects
            .insert("composition-process".into());
        effects.child_stop_observed = true;
        effects.process_effect_reconciled = true;
        effects
            .machine_entries
            .push(ctox_sync::contracts::WorkspaceEntry {
                path: "composition-machine".into(),
                kind: ctox_sync::contracts::WorkspaceEntryKind::File,
                artifact: ctox_sync::contracts::ArtifactRef {
                    sha256: format!("{:x}", Sha256::digest(b"component input")),
                    size_bytes: 15,
                },
                executable: false,
            });
        Ok(effects)
    };
    let complete = make()?;
    assert!(complete.reconciled());
    assert!(complete.pending("composition-capture")?.is_empty());
    let encoded: serde_json::Value = serde_json::from_slice(&complete.bytes(&spec, &ownership)?)?;
    assert_eq!(encoded["reconciled"], true);
    assert_eq!(encoded["externalEffects"], "reconciled");
    for missing in 0..10 {
        let mut effects = make()?;
        match missing {
            0 => effects.core_effects = None,
            1 => effects.child_stop_observed = false,
            2 => effects.process_effect_reconciled = false,
            3 => effects.machine_entries.clear(),
            4 => effects.process = None,
            5 => {
                effects
                    .job
                    .pending_effects
                    .insert("unresolved-effect".into());
            }
            6 => effects.process.as_mut().unwrap().job_id = "foreign-job".into(),
            7 => effects.process.as_mut().unwrap().ownership.generation += 1,
            8 => effects.job.completed_effects.clear(),
            _ => effects.job.spec.session_id = uuid::Uuid::new_v4().to_string(),
        }
        assert!(
            !effects.reconciled(),
            "missing witness {missing} permitted continuation"
        );
        let pending = effects.pending("composition-capture")?;
        assert!(pending
            .iter()
            .any(|p| p.effect_id == "native-effects-composition-capture"));
        let encoded: serde_json::Value =
            serde_json::from_slice(&effects.bytes(&effects.job.spec, &effects.job.ownership)?)?;
        assert_eq!(encoded["reconciled"], false);
        assert_eq!(encoded["externalEffects"], "unknown");
    }
    let imported = ctox_core::NativeSessionState::from_checkpoint(
        state.as_bytes(),
        state.session_id(),
        state.model(),
        state.provider_id(),
    )?;
    let mut descriptive = make()?;
    descriptive.bind_core_state(&imported)?;
    assert!(
        !descriptive.reconciled(),
        "wire metadata reconstructed a local Core witness"
    );
    assert_eq!(descriptive.pending("composition-capture")?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn native_source_core_only_capture_requires_actual_controller_and_local_core() -> Result<()> {
    let state = quiet_core_state().await?;
    let spec = ExecutionSpec {
        job_id: "core-only-component-job".into(),
        session_id: state.session_id().to_string(),
        scope_id: "component-scope".into(),
        harness: ctox_core::native_harness_name().into(),
        harness_version: ctox_core::native_harness_version().into(),
        model_route_id: state.provider_id().into(),
        gateway_account_id: "component-account".into(),
        model_id: state.model().into(),
        required_capabilities: BTreeSet::new(),
    };
    let ownership = Ownership {
        node_id: 1,
        generation: 1,
    };
    let (_root, registry, assignment) = super::super::tests::fixture();
    let registration = registry.registration(&assignment.destination.guest_id)?;
    let mut entry = registration.lock().unwrap();
    let mut effects = SourceEffects::fixture(&spec, &ownership, BTreeSet::new());
    effects.bind_core_state(&state)?;
    assert!(
        !effects.reconciled(),
        "Core alone supplied no controller/configuration witness"
    );
    effects.verify_controller(&entry, &registry)?;
    assert!(effects.reconciled());
    assert!(effects.pending("core-only-capture")?.is_empty());
    let runtime = effects
        .core_runtime_bytes()?
        .context("Core-only native identity absent")?;
    let identity: checkpoint_identity::CoreRuntimeIdentity = serde_json::from_slice(&runtime)?;
    assert_eq!(identity.guest_id, assignment.destination.guest_id);
    assert_eq!(identity.session_id, state.session_id().to_string());
    checkpoint_identity::tests::assert_core_only_identity(&state, &spec, &runtime)?;

    let imported = ctox_core::NativeSessionState::from_checkpoint(
        state.as_bytes(),
        state.session_id(),
        state.model(),
        state.provider_id(),
    )?;
    effects.bind_core_state(&imported)?;
    assert!(
        !effects.reconciled(),
        "a descriptive checkpoint reconstructed local Core authority"
    );
    effects.bind_core_state(&state)?;
    effects
        .job
        .pending_effects
        .insert("unknown-external-effect".into());
    assert!(!effects.reconciled());
    effects.job.pending_effects.clear();
    entry.process_effect = Some("uncertain-process".into());
    assert!(effects.verify_controller(&entry, &registry).is_err());
    assert!(
        !effects.reconciled(),
        "failed controller verification retained a Core-only witness"
    );
    entry.process_effect = None;
    effects.verify_controller(&entry, &registry)?;
    let before = effects.core_runtime_bytes()?;
    *registry.machine_configuration.lock().unwrap() =
        Some(serde_json::from_value(serde_json::json!({
            "program": "/component/qemu", "baseRaw": "/component/base.raw",
            "memoryMib": 64, "vcpus": 1, "acceleration": "tcg"
        }))?);
    effects.verify_controller(&entry, &registry)?;
    assert!(
        !effects.reconciled(),
        "configured but missing machine silently became Core-only"
    );
    assert_ne!(before, effects.core_runtime_bytes()?);
    Ok(())
}
