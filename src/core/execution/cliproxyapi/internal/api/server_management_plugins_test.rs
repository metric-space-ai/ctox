// ref: internal/api/handlers/management/plugins.go @ 5fc580b1d72d87a7bcfe5cfb6f479c1aea5625b3
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;
use crate::internal::api::handlers::management::{
    ManagementPluginConfig, ManagementPluginConfigStore, ManagementPluginConfigStoreError,
    ManagementPluginError, ManagementPluginRuntimeRecord, ManagementPluginRuntimeSource,
    ManagementPluginSnapshot, SystemManagementAuthClock,
};
use crate::sdk::pluginapi::{ConfigField, Metadata};
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Store {
    calls: AtomicUsize,
    fail: bool,
}
impl ManagementPluginConfigStore for Store {
    fn load(&self) -> Result<ManagementPluginSnapshot, ManagementPluginConfigStoreError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(ManagementPluginConfigStoreError::Unavailable);
        }
        Ok(ManagementPluginSnapshot {
            plugins_enabled: true,
            configs: BTreeMap::from([(
                "sample".to_owned(),
                ManagementPluginConfig {
                    enabled: true,
                    values: BTreeMap::from([(
                        "mode".to_owned(),
                        serde_json::json!("configured-value-must-not-leak"),
                    )]),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        })
    }
    fn replace(
        &self,
        _: u64,
        _: &ManagementPluginSnapshot,
    ) -> Result<(), ManagementPluginConfigStoreError> {
        panic!("a metadata GET must never write configuration")
    }
}
struct Runtime {
    calls: AtomicUsize,
    fail: bool,
}
impl ManagementPluginRuntimeSource for Runtime {
    fn snapshot(&self) -> Result<Vec<ManagementPluginRuntimeRecord>, ManagementPluginError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(ManagementPluginError::RuntimeUnavailable);
        }
        Ok(vec![ManagementPluginRuntimeRecord {
            id: "sample".to_owned(),
            registered: true,
            metadata: Some(Metadata {
                name: "<plugin>".to_owned(),
                logo: "<logo>".to_owned(),
                config_fields: vec![ConfigField {
                    name: "mode".to_owned(),
                    default_value: Some(serde_json::json!({"label":"<safe>","flag":false})),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        }])
    }
}
fn handler() -> ManagementHandler {
    ManagementHandler::new(Arc::new(
        ManagementAuthenticator::new(
            "metadata-fixture",
            false,
            Arc::new(SystemManagementAuthClock),
        )
        .unwrap(),
    ))
}
fn sources(fail_store: bool, fail_runtime: bool) -> (Arc<Store>, Arc<Runtime>, ManagementHandler) {
    let store = Arc::new(Store {
        calls: AtomicUsize::new(0),
        fail: fail_store,
    });
    let runtime = Arc::new(Runtime {
        calls: AtomicUsize::new(0),
        fail: fail_runtime,
    });
    let handler = handler().attach_plugins(Arc::new(ManagementPluginService::new(
        store.clone(),
        runtime.clone(),
    )));
    (store, runtime, handler)
}
fn headers() -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([(
        "x-management-key".to_owned(),
        vec!["metadata-fixture".to_owned()],
    )])
}
fn assert_schema(body: &serde_json::Value) {
    assert_eq!(body["plugins_enabled"], true);
    let row = &body["plugins"][0];
    assert_eq!(row["metadata"]["name"], "&lt;plugin&gt;");
    assert_eq!(row["logo"], "&lt;logo&gt;");
    assert_eq!(row["config_fields"], row["metadata"]["config_fields"]);
    assert_eq!(
        row["config_fields"][0]["default"],
        serde_json::json!({"label":"&lt;safe&gt;","flag":false})
    );
    assert!(!body.to_string().contains("configured-value-must-not-leak"));
}
#[test]
fn metadata_route_checks_authority_before_source_and_only_get_reads() {
    let (store, runtime, handler) = sources(false, false);
    let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
    assert_eq!(
        handler
            .handle("GET", PLUGINS_PATH, &BTreeMap::new(), &[], local)
            .status(),
        401
    );
    assert_eq!(
        handler
            .handle("POST", PLUGINS_PATH, &headers(), &[], local)
            .status(),
        405
    );
    assert_eq!(store.calls.load(Ordering::SeqCst), 0);
    assert_eq!(runtime.calls.load(Ordering::SeqCst), 0);
    let response = handler.handle("GET", PLUGINS_PATH, &headers(), &[], local);
    assert_eq!(response.status(), 200);
    assert_schema(&serde_json::from_slice(response.body()).unwrap());
    assert_eq!(store.calls.load(Ordering::SeqCst), 1);
    assert_eq!(runtime.calls.load(Ordering::SeqCst), 1);
}
#[test]
fn metadata_route_does_not_report_unconfigured_or_failed_sources_as_ready() {
    let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
    assert_eq!(
        handler()
            .handle("GET", PLUGINS_PATH, &headers(), &[], local)
            .status(),
        404
    );
    for (store_error, runtime_error) in [(true, false), (false, true)] {
        let (_, _, handler) = sources(store_error, runtime_error);
        let response = handler.handle("GET", PLUGINS_PATH, &headers(), &[], local);
        assert_eq!(response.status(), 503);
        let body: serde_json::Value = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"error":"plugin metadata unavailable"})
        );
    }
}
#[tokio::test]
async fn metadata_route_serves_sanitized_defaults_over_real_http() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    tokio::time::timeout(std::time::Duration::from_secs(5),async {
        let (_,_,handler)=sources(false,false);
        let listener=TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
        let server=tokio::spawn(async move { serve_one_management_connection(&listener,&handler).await.unwrap() });
        let mut client=TcpStream::connect(address).await.unwrap();
        client.write_all(b"GET /v0/management/plugins HTTP/1.1\r\nHost: localhost\r\nX-Management-Key: metadata-fixture\r\n\r\n").await.unwrap();
        let mut response=Vec::new();client.read_to_end(&mut response).await.unwrap();server.await.unwrap();
        let split=response.windows(4).position(|part|part==b"\r\n\r\n").unwrap();
        let head=std::str::from_utf8(&response[..split]).unwrap();assert!(head.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(head.to_ascii_lowercase().contains("cache-control: no-store\r\n"));
        assert_schema(&serde_json::from_slice(&response[split+4..]).unwrap());
    }).await.expect("bounded management HTTP fixture timed out");
}
