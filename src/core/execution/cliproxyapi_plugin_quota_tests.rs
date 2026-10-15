#[path = "cliproxyapi_quota_runtime_tests.rs"]
mod runtime_tests;
// Origin: CTOX; License: AGPL-3.0-only
use super::*;
use std::{
    io::Write,
    net::TcpListener,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::Instant,
};

struct Fixture {
    root: tempfile::TempDir,
    source: CtoxQuotaCredentials,
}
fn write(root: &Path, name: &str, value: &str) {
    crate::secrets::write_secret_record(
        root,
        "provider-subscriptions",
        name,
        value,
        None,
        serde_json::json!({"test":true}),
    )
    .unwrap();
}
fn fixture(origin: &str) -> Fixture {
    let base = if cfg!(target_os = "macos") {
        "/Volumes/tmp"
    } else {
        "/tmp"
    };
    let root = tempfile::Builder::new()
        .prefix("nq-")
        .tempdir_in(base)
        .unwrap();
    write(root.path(), "quota-access", "fixture-access-only");
    write(root.path(), "quota-refresh", "fixture-refresh-only");
    let parsed = url::Url::parse(origin).unwrap();
    let runtime: CliproxyRuntimeConfig = serde_json::from_value(serde_json::json!({
        "claude_accounts":[{"id":"quota-account","access_token_secret":{"scope":"provider-subscriptions","name":"quota-access"},"refresh_token_secret":{"scope":"provider-subscriptions","name":"quota-refresh"},"upstream_scheme":parsed.scheme(),"upstream_authority":parsed[url::Position::BeforeHost..url::Position::AfterPort]}]
    })).unwrap();
    save_instance_proxy_config(root.path(), 0, "claude", runtime).unwrap();
    let source = CtoxQuotaCredentials::new(root.path());
    Fixture { root, source }
}
#[test]
fn native_quota_missing_topology_does_not_initialize_storage() {
    let base = if cfg!(target_os = "macos") {
        "/Volumes/tmp"
    } else {
        "/tmp"
    };
    let root = tempfile::Builder::new()
        .prefix("nq-")
        .tempdir_in(base)
        .unwrap();
    assert!(CtoxQuotaCredentials::new(root.path())
        .resolve("quota-account")
        .unwrap()
        .is_none());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
#[test]
fn native_quota_resolves_only_enabled_canonical_accounts_and_never_serializes_credentials() {
    let f = fixture("https://api.anthropic.com");
    let credential = f.source.resolve("quota-account").unwrap().unwrap();
    let wire = serde_json::to_string(&credential.request).unwrap();
    for private in [
        "fixture-access-only",
        "fixture-refresh-only",
        credential.binding.as_str(),
    ] {
        assert!(!wire.contains(private));
        assert!(!format!("{credential:?}").contains(private));
    }
    assert_eq!(credential.request.provider, "claude");
    assert!(f.source.resolve("other-account").unwrap().is_none());
    let mut config = load_instance_proxy_config(f.root.path()).unwrap().unwrap();
    config.runtime.claude_accounts[0].disabled = true;
    // The topology permits all portable accounts to be disabled when an outer
    // extension owns execution; this reader still refuses the disabled account.
    save_instance_proxy_config(f.root.path(), config.revision, "", config.runtime).unwrap();
    assert!(f.source.resolve("quota-account").unwrap().is_none());
    let mut called = false;
    assert!(!f
        .source
        .with_current_binding("quota-account", &credential.binding, &mut || called = true)
        .unwrap());
    assert!(!called);
}
#[test]
fn native_quota_rotation_even_to_same_value_retires_binding_and_fences_topology_writes() {
    let f = fixture("https://api.anthropic.com");
    let old = f.source.resolve("quota-account").unwrap().unwrap();
    write(f.root.path(), "quota-access", "fixture-access-only");
    let fresh = f.source.resolve("quota-account").unwrap().unwrap();
    assert_ne!(old.binding, fresh.binding);
    assert!(!f
        .source
        .with_current_binding("quota-account", &old.binding, &mut || panic!(
            "retired publication"
        ))
        .unwrap());
    let mut published = 0;
    assert!(f
        .source
        .with_current_binding("quota-account", &fresh.binding, &mut || {
            let conn = Connection::open(crate::inference::runtime_env::runtime_config_path(
                f.root.path(),
            ))
            .unwrap();
            conn.busy_timeout(Duration::ZERO).unwrap();
            assert!(
                conn.execute_batch("BEGIN IMMEDIATE").is_err(),
                "topology was not fenced"
            );
            published += 1;
        })
        .unwrap());
    assert_eq!(published, 1);
}
fn serve(
    listener: TcpListener,
    rotate: Option<PathBuf>,
    calls: Arc<AtomicUsize>,
) -> thread::JoinHandle<String> {
    listener.set_nonblocking(true).unwrap();
    thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(3);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < end =>
                {
                    thread::sleep(Duration::from_millis(5))
                }
                other => panic!("fixture accept deadline: {other:?}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            stream.read_exact(&mut b).unwrap();
            header.push(b[0]);
            assert!(header.len() < 65536);
        }
        calls.fetch_add(1, Ordering::SeqCst);
        if let Some(root) = rotate {
            write(&root, "quota-access", "fixture-replaced-access");
        }
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{\"reported\":true}").unwrap();
        String::from_utf8(header).unwrap().to_ascii_lowercase()
    })
}
#[tokio::test]
async fn native_quota_http_injects_server_credential_and_rejects_other_origins() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let f = fixture(&origin);
    let calls = Arc::new(AtomicUsize::new(0));
    let thread = serve(listener, None, calls.clone());
    let credential = f.source.resolve("quota-account").unwrap().unwrap();
    let client = credential.request.http_client.unwrap();
    assert!(client
        .execute(HttpRequest {
            method: "GET".into(),
            url: "https://example.invalid/quota".into(),
            ..Default::default()
        })
        .await
        .is_err());
    let response = client
        .execute(HttpRequest {
            method: "GET".into(),
            url: format!("{origin}/quota"),
            headers: BTreeMap::from([
                ("Authorization".into(), vec!["Bearer forged".into()]),
                ("Cookie".into(), vec!["bad=cookie".into()]),
            ]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(response.status_code, 200);
    assert_eq!(response.body, b"{\"reported\":true}");
    let header = thread.join().unwrap();
    assert!(header.contains("authorization: bearer fixture-access-only\r\n"));
    assert!(!header.contains("forged"));
    assert!(!header.contains("cookie"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn native_quota_http_drops_response_after_rotation_without_holding_secret_fence_over_network()
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let f = fixture(&origin);
    let calls = Arc::new(AtomicUsize::new(0));
    let thread = serve(listener, Some(f.root.path().to_path_buf()), calls.clone());
    let credential = f.source.resolve("quota-account").unwrap().unwrap();
    let client = credential.request.http_client.unwrap();
    let error = client
        .execute(HttpRequest {
            method: "GET".into(),
            url: format!("{origin}/quota"),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "quota account binding retired");
    thread.join().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(!f
        .source
        .with_current_binding("quota-account", &credential.binding, &mut || panic!(
            "retired quota publication"
        ))
        .unwrap());
    assert!(f.source.resolve("quota-account").unwrap().is_some());
}
