// ref: internal/pluginhost/quota_provider_test.go @ 5fc580b1d72d87a7bcfe5cfb6f479c1aea5625b3
// License: MIT (upstream); modifications AGPL-3.0-only
use super::{
    abi::{
        PluginArtifact, PluginCall, PluginClient, PluginClientError, PluginFuture, PluginLoader,
        PluginStream,
    },
    auth_provider::HostConfigSummarySource,
    config::{RuntimeConfig, RuntimeItemConfig},
    host::PluginHost,
    platform::PluginFileInfo,
};
use crate::sdk::{
    pluginabi::*,
    pluginapi::{HostConfigSummary, QuotaFetchRequest, QuotaResetRequest},
};
use serde_json::{json, value::to_raw_value};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::Notify;
struct Host;
impl HostConfigSummarySource for Host {
    fn snapshot(&self) -> HostConfigSummary {
        Default::default()
    }
}
struct Client {
    id: &'static str,
    provider: &'static str,
    auth: Option<&'static str>,
    aliases: Vec<&'static str>,
    describes: AtomicUsize,
    fail_once: AtomicBool,
    block: AtomicBool,
    started: Notify,
    release: Notify,
}
impl Client {
    fn new(id: &'static str, provider: &'static str, aliases: Vec<&'static str>) -> Arc<Self> {
        Arc::new(Self {
            id,
            provider,
            auth: None,
            aliases,
            describes: AtomicUsize::new(0),
            fail_once: AtomicBool::new(false),
            block: AtomicBool::new(false),
            started: Notify::new(),
            release: Notify::new(),
        })
    }
}
impl PluginClient for Client {
    fn call<'a>(&'a self, call: PluginCall) -> PluginFuture<'a, Envelope> {
        Box::pin(async move {
            let value = match call.method.as_str() {
                METHOD_PLUGIN_REGISTER | METHOD_PLUGIN_RECONFIGURE => {
                    json!({"schema_version":2,"metadata":{"Name":format!("Quota {}",self.id)},"capabilities":{"quota_provider":true,"auth_provider":self.auth.is_some()}})
                }
                METHOD_QUOTA_IDENTIFIER => json!({"identifier":self.provider}),
                METHOD_AUTH_IDENTIFIER => json!({"identifier":self.auth}),
                METHOD_QUOTA_DESCRIBE => {
                    self.describes.fetch_add(1, Ordering::SeqCst);
                    self.started.notify_one();
                    if self.block.load(Ordering::SeqCst) {
                        self.release.notified().await;
                    }
                    if self.fail_once.swap(false, Ordering::SeqCst) {
                        return Err(PluginClientError::Transport(
                            "temporary description failure".into(),
                        ));
                    }
                    json!({"supported_providers":self.aliases,"supports_reset":true})
                }
                METHOD_QUOTA_FETCH => json!({"subscription":{"plan":self.provider}}),
                METHOD_QUOTA_RESET => json!({"success":false,"message":"declined"}),
                _ => return Err(PluginClientError::UnsupportedCapability),
            };
            Ok(Envelope::success(Some(to_raw_value(&value).unwrap())))
        })
    }
    fn call_stream<'a>(&'a self, _: PluginCall) -> PluginFuture<'a, PluginStream> {
        Box::pin(async { Err(PluginClientError::UnsupportedCapability) })
    }
    fn shutdown<'a>(&'a self) -> PluginFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}
struct Loader(Mutex<BTreeMap<String, Arc<Client>>>);
impl PluginLoader for Loader {
    fn open<'a>(&'a self, artifact: &'a PluginArtifact) -> PluginFuture<'a, Arc<dyn PluginClient>> {
        Box::pin(async move {
            self.0
                .lock()
                .unwrap()
                .get(&artifact.plugin_id)
                .cloned()
                .map(|client| client as Arc<dyn PluginClient>)
                .ok_or(PluginClientError::Closed)
        })
    }
}
struct Fixture {
    host: Arc<PluginHost>,
    loader: Arc<Loader>,
    config: RuntimeConfig,
    files: Vec<PluginFileInfo>,
}
async fn fixture(clients: Vec<(Arc<Client>, i32)>) -> Fixture {
    let loader = Arc::new(Loader(Mutex::new(BTreeMap::new())));
    let host = Arc::new(PluginHost::new(loader.clone()));
    let mut config = RuntimeConfig {
        enabled: true,
        ..Default::default()
    };
    let mut files = vec![];
    for (client, priority) in clients {
        config.items.insert(
            client.id.into(),
            RuntimeItemConfig {
                enabled: true,
                priority,
                ..Default::default()
            },
        );
        files.push(PluginFileInfo {
            id: client.id.into(),
            path: PathBuf::from(format!("/fixture/{}", client.id)),
            version: Some("1".into()),
        });
        loader.0.lock().unwrap().insert(client.id.into(), client);
    }
    assert!(host.apply_config(&config, &files).await.failures.is_empty());
    Fixture {
        host,
        loader,
        config,
        files,
    }
}
#[tokio::test]
async fn quota_lookup_prioritizes_direct_quota_plugin_and_auth_ids_before_all_aliases() {
    let high = Client::new(
        "alias-plugin",
        "quota-alias",
        vec!["exact", "exact-plugin", "auth-provider", "shared"],
    );
    let mut low = Client::new("exact-plugin", "exact", vec!["shared"]);
    Arc::get_mut(&mut low).unwrap().auth = Some("auth-provider");
    let f = fixture(vec![(high.clone(), 20), (low.clone(), 10)]).await;
    let ids = f.host.quota_provider_plugin_ids().await;
    for key in ["exact", "exact-plugin", "auth-provider"] {
        assert_eq!(ids[key], "exact-plugin");
    }
    assert_eq!(ids["shared"], "alias-plugin");
    let result = f
        .host
        .fetch_quota(
            QuotaFetchRequest {
                provider: " EXACT ".into(),
                ..Default::default()
            },
            Arc::new(Host),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.subscription.unwrap().plan, "exact");
    let result = f
        .host
        .fetch_quota(
            QuotaFetchRequest {
                provider: "shared".into(),
                ..Default::default()
            },
            Arc::new(Host),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.subscription.unwrap().plan, "quota-alias");
    assert_eq!(high.describes.load(Ordering::SeqCst), 1);
    assert_eq!(low.describes.load(Ordering::SeqCst), 1);
    assert!(f
        .host
        .fetch_quota_by_plugin("missing", QuotaFetchRequest::default(), Arc::new(Host))
        .await
        .unwrap()
        .is_none());
    assert!(f.host.has_quota_provider_for_plugin("exact-plugin"));
    assert!(f
        .host
        .describe_quota("exact-plugin")
        .await
        .unwrap()
        .is_some());
    let reset = f
        .host
        .reset_quota(
            QuotaResetRequest {
                provider: "exact".into(),
                ..Default::default()
            },
            Arc::new(Host),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(!reset.success);
    let reset = f
        .host
        .reset_quota_by_plugin("alias-plugin", QuotaResetRequest::default(), Arc::new(Host))
        .await
        .unwrap()
        .unwrap();
    assert!(!reset.success);
}
#[tokio::test]
async fn quota_description_errors_do_not_poison_alias_cache() {
    let client = Client::new("one", "quota-fixture", vec!["alias"]);
    client.fail_once.store(true, Ordering::SeqCst);
    let f = fixture(vec![(client.clone(), 0)]).await;
    let list = f.host.quota_providers().await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].display_name, "Quota one");
    assert_eq!(list[0].supported_providers, vec!["quota-fixture"]);
    assert!(!list[0].supports_reset);
    assert!(f.host.has_quota_provider("alias").await);
    assert_eq!(client.describes.load(Ordering::SeqCst), 2);
}
#[tokio::test]
async fn quota_alias_cache_is_not_shared_between_hosts() {
    let first = Client::new("one", "quota-fixture", vec!["first"]);
    let second = Client::new("one", "quota-fixture", vec!["second"]);
    let a = fixture(vec![(first.clone(), 0)]).await;
    let b = fixture(vec![(second.clone(), 0)]).await;
    assert!(a.host.has_quota_provider("first").await);
    assert!(!a.host.has_quota_provider("second").await);
    assert!(b.host.has_quota_provider("second").await);
    assert!(!b.host.has_quota_provider("first").await);
    assert_eq!(first.describes.load(Ordering::SeqCst), 1);
    assert_eq!(second.describes.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn quota_reload_invalidates_cached_aliases() {
    let first = Client::new("one", "quota-fixture", vec!["first"]);
    let mut f = fixture(vec![(first.clone(), 0)]).await;
    assert!(f.host.has_quota_provider("first").await);
    let next = Client::new("one", "quota-fixture", vec!["next"]);
    f.loader
        .0
        .lock()
        .unwrap()
        .insert("one".into(), next.clone());
    f.files[0].version = Some("2".into());
    assert!(f
        .host
        .apply_config(&f.config, &f.files)
        .await
        .failures
        .is_empty());
    assert!(!f.host.has_quota_provider("first").await);
    assert!(f.host.has_quota_provider("next").await);
    assert_eq!(first.describes.load(Ordering::SeqCst), 1);
    assert_eq!(next.describes.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn quota_interleaved_reload_discards_stale_maps_and_description_cache() {
    let first = Client::new("one", "old-provider", vec!["first"]);
    first.block.store(true, Ordering::SeqCst);
    let mut f = fixture(vec![(first.clone(), 0)]).await;
    let host = f.host.clone();
    let task = tokio::spawn(async move { host.quota_provider_plugin_ids().await });
    first.started.notified().await;
    let next = Client::new("one", "new-provider", vec!["next"]);
    f.loader
        .0
        .lock()
        .unwrap()
        .insert("one".into(), next.clone());
    f.files[0].version = Some("2".into());
    assert!(f
        .host
        .apply_config(&f.config, &f.files)
        .await
        .failures
        .is_empty());
    first.release.notify_one();
    assert!(task.await.unwrap().is_empty());
    let ids = f.host.quota_provider_plugin_ids().await;
    assert!(!ids.contains_key("old-provider"));
    assert!(!ids.contains_key("first"));
    assert_eq!(ids["new-provider"], "one");
    assert_eq!(ids["next"], "one");
    assert_eq!(next.describes.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn quota_concurrent_alias_queries_share_one_description() {
    let client = Client::new("one", "quota-fixture", vec!["alias"]);
    client.block.store(true, Ordering::SeqCst);
    let f = fixture(vec![(client.clone(), 0)]).await;
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..16 {
        let host = f.host.clone();
        tasks.spawn(async move { host.has_quota_provider("alias").await });
    }
    client.started.notified().await;
    tokio::task::yield_now().await;
    assert_eq!(client.describes.load(Ordering::SeqCst), 1);
    client.release.notify_one();
    tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(value) = tasks.join_next().await {
            assert!(value.unwrap());
        }
    })
    .await
    .unwrap();
    assert_eq!(client.describes.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn quota_canceled_description_is_retryable() {
    let client = Client::new("one", "quota-fixture", vec!["alias"]);
    client.block.store(true, Ordering::SeqCst);
    let f = fixture(vec![(client.clone(), 0)]).await;
    let host = f.host.clone();
    let task = tokio::spawn(async move { host.has_quota_provider("alias").await });
    client.started.notified().await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    client.block.store(false, Ordering::SeqCst);
    assert!(f.host.has_quota_provider("alias").await);
    assert_eq!(client.describes.load(Ordering::SeqCst), 2);
}
#[tokio::test]
async fn quota_invalid_identifier_is_not_advertised_and_empty_host_is_empty() {
    let invalid = Client::new("one", "bad\nid", vec!["alias"]);
    let loader = Arc::new(Loader(Mutex::new(BTreeMap::from([(
        "one".into(),
        invalid,
    )]))));
    let host = PluginHost::new(loader);
    let config = RuntimeConfig {
        enabled: true,
        items: BTreeMap::from([(
            "one".into(),
            RuntimeItemConfig {
                enabled: true,
                ..Default::default()
            },
        )]),
        ..Default::default()
    };
    assert_eq!(
        host.apply_config(
            &config,
            &[PluginFileInfo {
                id: "one".into(),
                path: PathBuf::from("/fixture/one"),
                version: None
            }]
        )
        .await
        .failures
        .len(),
        1
    );
    assert!(host.quota_provider_identifiers().is_empty());
    assert!(host.quota_providers().await.is_empty());
    assert!(!host.has_quota_provider("alias").await);
}
