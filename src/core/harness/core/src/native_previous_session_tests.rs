//! Real Core capture/restore/re-capture; native owner policy is a separate boundary.
use crate::config::ConfigBuilder;
use crate::features::Feature;
use crate::{AuthManager, NativeSessionState, ThreadManager, models_manager};
use ctox_protocol::protocol::SessionSource;
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[tokio::test]
async fn actual_previous_core_input_requires_verified_native_owner_before_recapture()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let home = root.path().join("home");
    std::fs::create_dir(&home)?;
    let mut config = ConfigBuilder::default()
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
    let auth = Arc::new(AuthManager::new(
        home,
        false,
        config.cli_auth_credentials_store_mode,
    ));
    let manager = ThreadManager::new(
        &config,
        auth.clone(),
        SessionSource::Exec,
        models_manager::collaboration_mode_presets::CollaborationModesConfig::default(),
    );
    let source = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        manager.start_thread(config.clone()),
    )
    .await??;
    assert!(
        source
            .thread
            .reconcile_native_previous_session(|_| Ok(()))
            .is_err(),
        "fresh/normal Core fabricated a previous-input witness"
    );
    source.thread.register_native_source_factory()?;
    source.thread.shutdown_and_wait().await?;
    let (_, state) = source.thread.capture_native_state().await?;
    assert!(
        !state
            .core_effect_capture()
            .unwrap()
            .requires_reconciliation()
    );
    let session = state.session_id();
    let digest: [u8; 32] = Sha256::digest(state.as_bytes()).into();
    let journal = source.thread.rollout_path().unwrap();
    let working = root.path().join("working.jsonl");
    std::fs::copy(&journal, &working)?;
    config.model = Some(state.model().to_owned());
    let imported = NativeSessionState::from_checkpoint(
        state.as_bytes(),
        session,
        state.model(),
        state.provider_id(),
    )?;
    assert!(
        imported.core_effect_capture().is_none(),
        "metadata minted opaque capture"
    );
    let target_manager = ThreadManager::new(
        &config,
        auth.clone(),
        SessionSource::Exec,
        models_manager::collaboration_mode_presets::CollaborationModesConfig::default(),
    );
    let target = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        target_manager.resume_thread_from_native_checkpoint(config, working, auth, imported),
    )
    .await??;
    assert_eq!(target.thread_id, session);
    assert!(
        target
            .thread
            .reconcile_native_previous_session(|_| { Err(std::io::Error::other("owner rejected")) })
            .is_err()
    );
    let mut verified = false;
    target
        .thread
        .reconcile_native_previous_session(|snapshot| {
            assert_eq!(snapshot.session_id(), session);
            assert_eq!(snapshot.input_sha256(), &digest);
            verified = true;
            Ok(())
        })?;
    assert!(verified);
    assert!(
        target
            .thread
            .reconcile_native_previous_session(|_| Ok(()))
            .is_err()
    );
    target.thread.register_native_source_factory()?;
    target.thread.shutdown_and_wait().await?;
    let (_, repeated) = target.thread.capture_native_state().await?;
    assert_eq!(repeated.session_id(), session);
    assert!(
        !repeated
            .core_effect_capture()
            .unwrap()
            .requires_reconciliation()
    );
    Ok(())
}
