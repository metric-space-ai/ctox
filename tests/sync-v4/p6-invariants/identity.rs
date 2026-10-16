//! Contract checks injected as a test-only child of the real foreground module.
use super::*;
use ctox_sync::authority::auth::SigningIdentity;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn p6_identity_concurrent_creators_restart_and_corrupt_store() -> anyhow::Result<()> {
    let _scope = isolate_test_roots().await;
    anyhow::ensure!(
        std::env::var_os("CTOX_STATE_ROOT").is_none(),
        "configured root forbidden"
    );
    let fixture = tempfile::Builder::new()
        .prefix("p6id-")
        .tempdir_in("/tmp")?;
    let root = fixture.path().to_owned();
    let mut foreground = Foreground::start(root.clone(), Handle::current()).await?;
    let secrets = foreground.native_owners.as_ref().unwrap()._secrets.clone();
    let remove_root = root.clone();
    crate::persistence::run_core_sync(move || {
        let (scope, name) = crate::sync_host::SIGNING_IDENTITY_SECRET_KEY;
        crate::secrets::delete_secret_record(&remove_root, scope, name)
    })
    .await?;
    // Exercise the real encrypted unique-row arbitration; no mocked secret store.
    let (left, right) = tokio::join!(
        secrets.instance_signing_identity(),
        secrets.instance_signing_identity()
    );
    let mut admitted = Vec::new();
    for result in [left, right] {
        match result {
            Ok(identity) => admitted.push(identity),
            Err(error) => assert!(
                matches!(error.downcast_ref::<StoreError>(), Some(StoreError::Busy)),
                "concurrent creator failed outside bounded store admission"
            ),
        }
    }
    assert!(
        !admitted.is_empty(),
        "neither concurrent creator stored an identity"
    );
    let left = admitted[0].clone();
    assert!(
        admitted
            .iter()
            .all(|identity| identity.public_identity() == left.public_identity()),
        "identity arbitration returned an uncommitted key"
    );
    let stored = secrets.instance_signing_identity().await?;
    assert_eq!(
        left.public_identity(),
        stored.public_identity(),
        "identity arbitration returned an uncommitted key"
    );
    foreground.shutdown().await?;
    let mut restarted = Foreground::start(root.clone(), Handle::current()).await?;
    assert_eq!(
        restarted.inputs.instance_signer()?.key_id(),
        left.public_identity(),
        "identity arbitration returned an uncommitted key"
    );
    let owned_secrets = restarted.native_owners.as_ref().unwrap()._secrets.clone();
    let corrupt_root = root.clone();
    crate::persistence::run_core_sync(move || {
        let (scope, name) = crate::sync_host::SIGNING_IDENTITY_SECRET_KEY;
        crate::secrets::write_secret_record(
            &corrupt_root,
            scope,
            name,
            "synthetic-private-key-sentinel-not-a-record",
            None,
            serde_json::json!({"source":"p6-invariant-fixture"}),
        )
    })
    .await?;
    let result = owned_secrets.instance_signing_identity().await;
    assert!(
        result.is_err(),
        "corrupt encrypted identity must not regenerate or use fallback"
    );
    let error = result.err().unwrap();
    assert!(
        !format!("{error:#?}").contains("synthetic-private-key-sentinel"),
        "key material leaked through identity error"
    );
    restarted.shutdown().await?;
    let result = Foreground::start(root, Handle::current()).await;
    assert!(
        result.is_err(),
        "restart accepted a corrupt secret-store identity"
    );
    Ok(())
}

#[test]
fn p6_signing_errors_do_not_echo_input_key_material() {
    let sentinel = b"synthetic-private-key-sentinel-not-pkcs8";
    let result = SigningIdentity::from_pkcs8(sentinel);
    let error = result.err().expect("malformed key was accepted");
    assert!(
        !format!("{error:?} {error}").contains("synthetic-private-key-sentinel"),
        "key input leaked through signing import error"
    );
    let result = crate::sync_host::signing_identity_from_record(sentinel);
    let error = result.err().expect("malformed record was accepted");
    assert!(!format!("{error:#?} {error:#}").contains("synthetic-private-key-sentinel"));
}

#[test]
fn p6_noncanonical_input_cannot_reach_instance_signing() {
    let pkcs8 = SigningIdentity::generate_pkcs8().unwrap();
    let identity = SigningIdentity::from_pkcs8(&pkcs8).unwrap();
    // Not an encoder result: deliberately noncanonical JSON whitespace/key order.
    let bytes = b"{ \"z\": 1, \"a\": 2 }";
    let signature = identity.sign_canonical_bytes(bytes);
    let hex = identity.public_identity();
    let public: Vec<u8> = hex
        .strip_prefix("ed25519:")
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect();
    assert!(
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public)
            .verify(bytes, &signature)
            .is_err(),
        "raw noncanonical bytes reached the instance signing primitive without an encoder token"
    );
}

#[test]
fn p6_signing_identity_cannot_implement_debug() {
    // Inference becomes ambiguous if a future implementation exposes Debug.
    trait AmbiguousIfDebug<A> {
        fn marker() {}
    }
    impl<T: ?Sized> AmbiguousIfDebug<()> for T {}
    struct IsDebug;
    impl<T: ?Sized + std::fmt::Debug> AmbiguousIfDebug<IsDebug> for T {}
    let _ = <SigningIdentity as AmbiguousIfDebug<_>>::marker;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn p6_identity_two_process_first_start_has_one_stored_key() -> anyhow::Result<()> {
    let _scope = isolate_test_roots().await;
    anyhow::ensure!(
        std::env::var_os("CTOX_STATE_ROOT").is_none(),
        "configured root forbidden"
    );
    let fixture = tempfile::Builder::new()
        .prefix("p6mp-")
        .tempdir_in("/tmp")?;
    let root = fixture.path().to_owned();
    let exe = std::env::current_exe()?;
    let mut children = Vec::new();
    for i in 0..2 {
        children.push(
            std::process::Command::new(&exe)
                .args([
                    "--ignored",
                    "--exact",
                    "service::store_startup::p6_invariants::p6_identity_first_start_child",
                    "--test-threads=1",
                ])
                .env("CTOX_P6_FIRST_START_ROOT", &root)
                .env(
                    "CTOX_P6_FIRST_START_RESULT",
                    fixture.path().join(format!("result-{i}.json")),
                )
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?,
        );
    }
    let statuses = tokio::task::spawn_blocking(move || {
        children
            .into_iter()
            .map(|mut child| child.wait())
            .collect::<std::io::Result<Vec<_>>>()
    })
    .await??;
    anyhow::ensure!(
        statuses.iter().all(|s| s.success()),
        "first-start child failed before recording an ownership result"
    );
    let mut keys = Vec::new();
    let mut denied = 0;
    for i in 0..2 {
        let result: serde_json::Value = serde_json::from_slice(&std::fs::read(
            fixture.path().join(format!("result-{i}.json")),
        )?)?;
        if result["admitted"] == true {
            keys.push(result["public"].as_str().unwrap().to_owned());
        } else {
            denied += 1;
        }
    }
    assert!(!keys.is_empty(), "neither first start was admitted");
    assert!(
        keys.iter().all(|key| key == &keys[0]),
        "two processes obtained different instance identities"
    );
    assert_eq!(keys.len() + denied, 2);
    let mut restarted = Foreground::start(root.clone(), Handle::current()).await?;
    assert_eq!(restarted.inputs.instance_signer()?.key_id(), keys[0]);
    let metadata_root = root;
    let count = crate::persistence::run_core_sync(move || {
        let (scope, name) = crate::sync_host::SIGNING_IDENTITY_SECRET_KEY;
        Ok(
            crate::secrets::list_secret_records(&metadata_root, Some(scope))?
                .iter()
                .filter(|r| r.secret_name == name)
                .count(),
        )
    })
    .await?;
    assert_eq!(count, 1, "first starts left more than one stored identity");
    restarted.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "child of p6_identity_two_process_first_start_has_one_stored_key only"]
async fn p6_identity_first_start_child() -> anyhow::Result<()> {
    let root = std::path::PathBuf::from(
        std::env::var_os("CTOX_P6_FIRST_START_ROOT")
            .ok_or_else(|| anyhow::anyhow!("child scope missing"))?,
    );
    let result = std::path::PathBuf::from(
        std::env::var_os("CTOX_P6_FIRST_START_RESULT")
            .ok_or_else(|| anyhow::anyhow!("child result missing"))?,
    );
    let result = match Foreground::start(root, Handle::current()).await {
        Ok(mut owner) => {
            let public = owner.inputs.instance_signer()?.key_id().to_owned();
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            owner.shutdown().await?;
            serde_json::json!({"admitted":true,"public":public})
        }
        Err(StoreError::AlreadyOwned) => serde_json::json!({"admitted":false}),
        Err(_) => anyhow::bail!("first start failed for a reason other than instance ownership"),
    };
    std::fs::write(result, serde_json::to_vec(&result)?)?;
    Ok(())
}
