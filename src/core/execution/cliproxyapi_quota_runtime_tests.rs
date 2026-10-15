// Origin: CTOX; License: AGPL-3.0-only
use super::*;
use crate::execution::cliproxyapi_host::quota_runtime as runtime;
use ctox_cliproxyapi::internal::api::{
    handlers::management::{ManagementAuthenticator, SystemManagementAuthClock},
    server_management::ManagementHandler,
};
fn document(root: &Path, revision: u64, enabled: bool, plugins: Vec<runtime::Plugin>) -> PathBuf {
    let path = root.join("quota-plugin-config.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&runtime::Config {
            schema: "ctox.cliproxyapi.quota-plugins.v1".into(),
            expected_revision: revision,
            enabled,
            plugins,
        })
        .unwrap(),
    )
    .unwrap();
    path
}
#[tokio::test]
async fn native_quota_runtime_absent_config_starts_no_plugin_and_writes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let owner = runtime::Owner::start(root.path()).await.unwrap();
    assert!(owner.service.is_none());
    assert_eq!(owner.identity, "unconfigured");
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    owner.shutdown().await;
}
#[test]
fn native_quota_runtime_configuration_is_revisioned_and_rejects_unknown_plaintext() {
    let root = tempfile::tempdir().unwrap();
    let path = document(root.path(), 0, false, vec![]);
    let status = runtime::configure_from_file(root.path(), &path).unwrap();
    assert_eq!(status["revision"], 1);
    assert!(runtime::configure_from_file(root.path(), &path).is_err());
    let old = runtime::identity(root.path()).unwrap();
    let path = document(root.path(), 1, false, vec![]);
    assert_eq!(
        runtime::configure_from_file(root.path(), &path).unwrap()["revision"],
        2
    );
    assert_ne!(runtime::identity(root.path()).unwrap(), old);
    std::fs::write(&path,b"{\"schema\":\"ctox.cliproxyapi.quota-plugins.v1\",\"expected_revision\":2,\"enabled\":false,\"plugins\":[],\"token\":\"fixture-private-value\"}").unwrap();
    let error = runtime::configure_from_file(root.path(), &path)
        .unwrap_err()
        .to_string();
    assert!(!error.contains("fixture-private-value"));
}
#[test]
fn native_quota_runtime_rejects_unverified_artifact_before_persistence() {
    let root = tempfile::tempdir().unwrap();
    let path = document(
        root.path(),
        0,
        true,
        vec![runtime::Plugin {
            id: "quota-fixture".into(),
            executable: std::env::current_exe().unwrap(),
            sha256: "0".repeat(64),
            priority: 0,
            config_secret: None,
        }],
    );
    assert!(runtime::configure_from_file(root.path(), &path).is_err());
    assert_eq!(runtime::identity(root.path()).unwrap(), "unconfigured");
}
#[tokio::test]
async fn native_quota_runtime_fault_is_scoped_and_keeps_authenticated_management_status_available()
{
    let f = fixture("https://api.anthropic.com");
    let key = "fixture-native-management-key-at-least-32";
    let authenticator = Arc::new(
        ManagementAuthenticator::new(key, false, Arc::new(SystemManagementAuthClock)).unwrap(),
    );
    let handler = ManagementHandler::with_runtime_status(
        authenticator,
        Arc::new(CtoxManagementRuntimeStatusSource::new(
            f.root.path().to_path_buf(),
        )),
    )
    .attach_quota_runtime_unavailable();
    let ip = "127.0.0.1".parse().unwrap();
    let headers = BTreeMap::from([
        ("Authorization".into(), vec![format!("Bearer {key}")]),
        ("Content-Type".into(), vec!["application/json".into()]),
    ]);
    let body = serde_json::to_vec(&serde_json::json!({"auth_index":"quota-account"})).unwrap();
    let unauthorized = handler
        .handle_async(
            "POST",
            "/v0/management/quota/fetch",
            &BTreeMap::new(),
            &body,
            ip,
        )
        .await;
    assert_ne!(unauthorized.status(), 503);
    assert!(!(200..300).contains(&unauthorized.status()));
    for (method, path) in [
        ("POST", "/v0/management/quota/fetch"),
        ("GET", "/v0/management/quota/providers"),
    ] {
        let response = handler
            .handle_async(method, path, &headers, &body, ip)
            .await;
        assert_eq!(response.status(), 503);
        let value: serde_json::Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(value["kind"], "quota_runtime_unavailable");
    }
    assert_eq!(
        handler
            .handle_async("GET", "/v0/management/runtime-status", &headers, &[], ip)
            .await
            .status(),
        200
    );
}
#[cfg(unix)]
#[tokio::test]
#[ignore = "requires the explicitly built SDK process fixture; executed by the native quota lane"]
async fn native_quota_runtime_actual_process_uses_native_secrets_and_discards_retired_result() {
    let executable = PathBuf::from(
        std::env::var("CTOX_QUOTA_TEST_PLUGIN_EXECUTABLE")
            .expect("native quota lane must build and name the SDK fixture"),
    );
    let digest = format!("{:x}", Sha256::digest(std::fs::read(&executable).unwrap()));
    for rotate in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let f = fixture(&origin);
        crate::secrets::write_secret_record(
            f.root.path(),
            "cliproxyapi-plugins",
            "fixture-quota-config",
            &format!("fixture_quota_url: {origin}/quota\n"),
            None,
            serde_json::json!({"test":true}),
        )
        .unwrap();
        let path = document(
            f.root.path(),
            0,
            true,
            vec![runtime::Plugin {
                id: "fixture-quota".into(),
                executable: executable.clone(),
                sha256: digest.clone(),
                priority: 0,
                config_secret: Some(RuntimeSecretRef {
                    scope: "cliproxyapi-plugins".into(),
                    name: "fixture-quota-config".into(),
                }),
            }],
        );
        assert_eq!(
            runtime::configure_from_file(f.root.path(), &path).unwrap()["revision"],
            1
        );
        let owner = runtime::Owner::start(f.root.path()).await.unwrap();
        let initial = owner.identity.clone();
        let key = "fixture-native-management-key-at-least-32";
        let authenticator = Arc::new(
            ManagementAuthenticator::new(key, false, Arc::new(SystemManagementAuthClock)).unwrap(),
        );
        let handler = ManagementHandler::new(authenticator)
            .attach_quota_runtime_unavailable()
            .attach_plugin_quota_service(owner.service.clone().unwrap());
        let body=serde_json::to_vec(&serde_json::json!({"auth_index":"quota-account","provider":"fixture-quota","plugin_id":"fixture-quota"})).unwrap();
        let ip = "127.0.0.1".parse().unwrap();
        let unauthorized = handler
            .handle_async(
                "POST",
                "/v0/management/quota/fetch",
                &BTreeMap::new(),
                &body,
                ip,
            )
            .await;
        assert!(!(200..300).contains(&unauthorized.status()));
        let calls = Arc::new(AtomicUsize::new(0));
        let thread = serve(
            listener,
            rotate.then(|| f.root.path().to_path_buf()),
            calls.clone(),
        );
        let response = handler
            .handle_async(
                "POST",
                "/v0/management/quota/fetch",
                &BTreeMap::from([
                    ("Authorization".into(), vec![format!("Bearer {key}")]),
                    ("Content-Type".into(), vec!["application/json".into()]),
                ]),
                &body,
                ip,
            )
            .await;
        let value: serde_json::Value = serde_json::from_slice(response.body()).unwrap();
        if rotate {
            assert_eq!(response.status(), 500);
            assert_eq!(value["kind"], "account_context_mismatch");
        } else {
            assert_eq!(response.status(), 200);
            assert_eq!(value["subscription"]["plan"], "fixture-plan");
        }
        let header = thread.join().unwrap();
        assert!(header.contains("authorization: bearer fixture-access-only\r\n"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let path = document(f.root.path(), 1, false, vec![]);
        runtime::configure_from_file(f.root.path(), &path).unwrap();
        assert_ne!(runtime::identity(f.root.path()).unwrap(), initial);
        owner.shutdown().await;
        assert_eq!(
            std::fs::read_dir(
                crate::inference::runtime_env::runtime_config_path(f.root.path())
                    .parent()
                    .unwrap()
                    .join(".qp/.cpa")
            )
            .unwrap()
            .count(),
            0
        );
        let disabled = runtime::Owner::start(f.root.path()).await.unwrap();
        assert!(disabled.service.is_none());
        disabled.shutdown().await;
    }
}
