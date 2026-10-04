// ref: internal/registry/devin_models_test.go @ d7914afd
// License: MIT (upstream); modifications AGPL-3.0-only

use super::*;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

#[test]
fn candidate_devin_catalog_validation_accepts_envelopes_and_rejects_ambiguous_ids() {
    for data in [
        r#"{"devin":[{"id":" SWE-2 "}]}"#,
        r#"{"models":[{"id":" SWE-2 "}]}"#,
        r#"{"devin":null,"models":[{"id":" SWE-2 "}]}"#,
        r#"[{"id":" SWE-2 "}]"#,
    ] {
        let models = validate_devin_models_json(data.as_bytes()).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "devin/swe-2");
        assert_eq!(models[0].provider_type, "devin");
        assert_eq!(models[0].supported_input_modalities, ["text"]);
        assert_eq!(models[0].supported_output_modalities, ["text"]);
        assert_eq!(
            models[0].supported_generation_methods,
            ["generateContent", "countTokens"]
        );
    }
    for data in [
        "",
        "  ",
        "null",
        "[]",
        "{}",
        r#"{"devin":[]}"#,
        r#"{"devin":[null]}"#,
        r#"{"devin":[{"id":" "}]}"#,
        r#"{"devin":[{"id":"swe-2"},{"id":"DEVIN/SWE-2"}]}"#,
        r#"{"devin":[{"id":"devin/swe-2"},{"id":"devin/swe-2"}]}"#,
        r#"{"devin":[{"id":"valid"}],"models":42}"#,
    ] {
        assert!(
            validate_devin_models_json(data.as_bytes()).is_err(),
            "accepted {data:?}"
        );
    }
}

#[test]
fn candidate_devin_catalog_aggregates_efforts_and_retains_base_metadata() {
    let data = br#"{"devin":[
        {"id":"Base-high-fast","display_name":"Base High Thinking Fast","owned_by":"variant",
         "context_length":1000,"inputTokenLimit":1100,"max_completion_tokens":100,
         "supportedInputModalities":["image"],"supportedOutputModalities":["image"],
         "native_capabilities":{"web_search":false},"support_configuration_update":true,
         "thinking":{"min":10,"levels":["priority","zeta","medium"]}},
        {"id":"Base","display_name":"Canonical Base","owned_by":"canonical",
         "context_length":2000,"inputTokenLimit":2100,"outputTokenLimit":210,
         "max_completion_tokens":200,"supportedInputModalities":["text","image"],
         "supportedOutputModalities":["text"],"supportedGenerationMethods":["custom"],
         "thinking":{"levels":["none","medium","alpha"]}},
        {"id":"Base-low","display_name":"Base Low"},
        {"id":"Base_MAX","display_name":"Base Max"},
        {"id":"Other-thinking-1m","context_length":1000000},
        {"id":"Other-max-1m"},
        {"id":"swe-1-6-fast"}, {"id":"swe-1-6-slow"}
    ]}"#;
    let models = validate_devin_models_json(data).unwrap();
    assert_eq!(models.len(), 4);
    let base = &models[0];
    assert_eq!(base.id, "devin/base");
    assert_eq!(base.display_name, "Canonical Base");
    assert_eq!(base.owned_by, "canonical");
    assert_eq!((base.context_length, base.input_token_limit), (2000, 2100));
    assert_eq!(
        (base.max_completion_tokens, base.output_token_limit),
        (200, 210)
    );
    assert_eq!(base.supported_input_modalities, ["image", "text"]);
    assert_eq!(base.supported_output_modalities, ["image", "text"]);
    assert_eq!(base.supported_generation_methods, ["custom"]);
    assert_eq!(
        base.native_capabilities.as_ref().unwrap().web_search,
        Some(false)
    );
    assert!(base.support_configuration_update);
    let thinking = base.thinking.as_ref().unwrap();
    assert_eq!(
        thinking.levels,
        ["none", "low", "medium", "high", "max", "alpha", "zeta"]
    );
    assert_eq!(thinking.min, 0);
    assert_eq!(models[1].id, "devin/other-1m");
    assert_eq!(models[1].thinking.as_ref().unwrap().levels, ["max"]);
    assert_eq!(models[2].id, "devin/swe-1-6");
    assert_eq!(models[3].id, "devin/swe-1-6-slow");
}

#[test]
fn candidate_devin_catalog_embedded_hash_namespaces_and_fallback_hierarchy() {
    assert_eq!(
        format!("{:x}", Sha256::digest(EMBEDDED_DEVIN_MODELS_JSON)),
        "1aee55bcd7af6be17d18c04046d25aba9c63a635af1088bfa4b7dc04d4be3360"
    );
    let raw: serde_json::Value = serde_json::from_slice(EMBEDDED_DEVIN_MODELS_JSON).unwrap();
    assert_eq!(raw["devin"].as_array().unwrap().len(), 56);
    let store = DevinModelsStore::from_embedded().unwrap();
    let snapshot = store.snapshot();
    assert_eq!(snapshot.data, EMBEDDED_DEVIN_MODELS_JSON);
    assert_eq!(snapshot.revision, 1);
    assert!(snapshot.models.len() >= 30);
    for model in &snapshot.models {
        assert!(model.id.starts_with("devin/"));
        for suffix in [
            "-low-fast",
            "-medium-fast",
            "-high-fast",
            "-xhigh-fast",
            "-max-fast",
            "-none-fast",
            "-low",
            "-medium",
            "-high",
            "-xhigh",
            "-max",
            "-none",
            "-thinking-1m",
            "-thinking",
            "_none",
            "_minimal",
            "_low",
            "_medium",
            "_high",
            "_xhigh",
            "_max",
            "_thinking",
        ] {
            assert!(!model.id.ends_with(suffix), "exposed variant {}", model.id);
        }
    }
    let mut catalog = embedded_models_catalog().unwrap();
    catalog.devin = vec![RegistryModelInfo {
        id: "devin/custom".into(),
        ..RegistryModelInfo::default()
    }];
    assert!(store.lookup(" DEVIN/GPT-6-ASTRA-high ", &catalog).is_some());
    assert!(store.lookup("gpt-6-astra", &catalog).is_some());
    assert!(store.lookup("custom", &catalog).is_none());
    assert_eq!(
        store.lookup("swe-1-6-slow", &catalog).unwrap().display_name,
        "SWE-1.6 Slow"
    );
    let empty = DevinModelsStore::default();
    assert_eq!(empty.models(&catalog).len(), 2);
    catalog.devin.clear();
    assert_eq!(empty.models(&catalog).len(), 12);
    let runtime = ModelCatalogStore::from_embedded().unwrap();
    assert!(runtime.models_for_channel(" DEVIN ").unwrap().len() >= 30);
    assert_eq!(
        runtime
            .lookup_model_info("devin/gpt-6-astra")
            .unwrap()
            .provider_type,
        "devin"
    );
    assert_eq!(
        runtime
            .lookup_model_info("gpt-6-astra")
            .unwrap()
            .context_length,
        272_000
    );
}

#[test]
fn candidate_devin_catalog_snapshots_and_rejected_updates_preserve_authority() {
    let store = DevinModelsStore::from_embedded().unwrap();
    let before = store.snapshot();
    assert!(store.load(br#"{"devin":[null]}"#, "broken").is_err());
    assert_eq!(store.snapshot(), before);
    let payload =
        br#"{"models":[{"id":"new","thinking":{"levels":["high"]},"context_length":333}]}"#;
    let loaded = store.load(payload, "remote").unwrap();
    assert!(loaded.changed);
    assert_eq!(loaded.changed_providers, ["devin"]);
    assert_eq!(loaded.revision, before.revision + 1);
    let current = store.snapshot();
    let mut copy = current.clone();
    copy.models[0].id = "mutated".into();
    copy.models[0].thinking.as_mut().unwrap().levels.clear();
    copy.data.clear();
    assert_eq!(store.snapshot(), current);
    let repeated = store.load(payload, "same").unwrap();
    assert!(!repeated.changed);
    assert_eq!(repeated.revision, current.revision);
    let reformatted = [payload.as_slice(), b"\n"].concat();
    let changed = store.load(&reformatted, "new bytes").unwrap();
    assert!(changed.changed);
    assert_eq!(changed.revision, current.revision + 1);
    assert_eq!(store.snapshot().models, current.models);
    assert_eq!(
        current.models.last().unwrap(),
        &super::devin_builtin::devin_builtin_swe16_slow()
    );
    let catalog = embedded_models_catalog().unwrap();
    assert!(store.lookup("gpt-6-astra", &catalog).is_none());
    assert_eq!(
        store
            .lookup("new-high", &catalog)
            .unwrap()
            .input_token_limit,
        333
    );
}

struct MemorySource {
    replies: HashMap<String, Result<Vec<u8>, String>>,
    calls: Mutex<Vec<(String, usize)>>,
}
impl ModelsSource for MemorySource {
    fn fetch<'a>(&'a self, source: &'a str, limit: usize) -> ModelsFetchFuture<'a> {
        self.calls.lock().unwrap().push((source.into(), limit));
        let result = self
            .replies
            .get(source)
            .cloned()
            .unwrap_or_else(|| Err("offline".into()));
        Box::pin(async move { result })
    }
}

#[tokio::test]
async fn candidate_devin_catalog_refresh_notifies_only_changes_and_keeps_valid_data() {
    let store = Arc::new(ModelCatalogStore::from_embedded().unwrap());
    let sink = Arc::new(ModelRefreshSink::default());
    let notifications = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&notifications);
    sink.set_callback(Some(Arc::new(move |providers| {
        observed.lock().unwrap().push(providers)
    })));
    let payload = br#"{"devin":[{"id":"new"}]}"#.to_vec();
    let source = Arc::new(MemorySource {
        replies: HashMap::from([
            ("good".into(), Ok(payload)),
            ("bad".into(), Ok(b"bad json".to_vec())),
            (
                "large".into(),
                Ok(vec![b' '; MAX_DEVIN_MODELS_CATALOG_SIZE + 1]),
            ),
        ]),
        calls: Mutex::new(Vec::new()),
    });
    let updater = ModelsUpdater::new(
        Arc::clone(&store),
        source.clone(),
        Vec::new(),
        Arc::clone(&sink),
    )
    .with_devin_sources(vec!["down".into(), "large".into(), "good".into()]);
    let first = updater.refresh_devin_once().await.unwrap();
    assert!(first.changed);
    assert_eq!(first.source, "good");
    assert_eq!(*notifications.lock().unwrap(), [vec!["devin".to_owned()]]);
    assert_eq!(
        source
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(|entry| entry.1)
            .collect::<Vec<_>>(),
        [MAX_DEVIN_MODELS_CATALOG_SIZE; 3]
    );
    assert!(store.lookup_model_info("devin/new").is_some());
    assert!(store.lookup_model_info("devin/gpt-6-astra").is_none());
    assert!(store.lookup_model_info("gpt-6-astra").is_some());
    assert!(!updater.refresh_devin_once().await.unwrap().changed);
    assert_eq!(notifications.lock().unwrap().len(), 1);
    let before = store.devin_store().snapshot();
    source.calls.lock().unwrap().clear();
    let rejected = ModelsUpdater::new(Arc::clone(&store), source.clone(), Vec::new(), sink)
        .with_devin_sources(vec!["bad".into(), "good".into()]);
    assert!(rejected.refresh_devin_once().await.is_err());
    assert_eq!(store.devin_store().snapshot(), before);
    assert_eq!(
        source
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(|entry| entry.0.as_str())
            .collect::<Vec<_>>(),
        ["bad"]
    );
}

struct BodyDrop(Arc<AtomicUsize>);
impl Drop for BodyDrop {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
struct PendingBodySource {
    started: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    dropped: Arc<AtomicUsize>,
}
impl ModelsSource for PendingBodySource {
    fn fetch<'a>(&'a self, source: &'a str, _: usize) -> ModelsFetchFuture<'a> {
        Box::pin(async move {
            if source != "body" {
                return Err("main unavailable".into());
            }
            let _body = BodyDrop(Arc::clone(&self.dropped));
            if let Some(started) = self.started.lock().unwrap().take() {
                let _ = started.send(());
            }
            std::future::pending::<Result<Vec<u8>, String>>().await
        })
    }
}
#[tokio::test]
async fn candidate_devin_catalog_owned_loop_cancels_inflight_body_without_mutation() {
    let store = Arc::new(ModelCatalogStore::from_embedded().unwrap());
    let before = store.devin_store().snapshot();
    let dropped = Arc::new(AtomicUsize::new(0));
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let source = Arc::new(PendingBodySource {
        started: Mutex::new(Some(started_tx)),
        dropped: Arc::clone(&dropped),
    });
    let updater = Arc::new(
        ModelsUpdater::new(
            Arc::clone(&store),
            source,
            vec!["main".into()],
            Arc::new(ModelRefreshSink::default()),
        )
        .with_devin_sources(vec!["body".into()]),
    );
    let (stop, receiver) = tokio::sync::watch::channel(false);
    let owned = Arc::clone(&updater);
    let task = tokio::spawn(async move { owned.run(receiver).await });
    tokio::time::timeout(std::time::Duration::from_secs(2), started_rx)
        .await
        .unwrap()
        .unwrap();
    tokio::task::yield_now().await;
    assert!(!task.is_finished());
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
    stop.send(true).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_eq!(store.devin_store().snapshot(), before);
}

#[cfg(feature = "codex-http-transport")]
#[tokio::test]
async fn candidate_devin_http_headers_do_not_cancel_delayed_response_body() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!(
        "http://{}/devin_models.json",
        listener.local_addr().unwrap()
    );
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        loop {
            let mut buffer = [0u8; 512];
            let n = stream.read(&mut buffer).await.unwrap();
            assert!(n > 0);
            request.extend_from_slice(&buffer[..n]);
            if request.windows(4).any(|part| part == b"\r\n\r\n") {
                break;
            }
        }
        let body = br#"{"devin":[{"id":"delayed-body"}]}"#;
        let header = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
        stream.write_all(header.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        stream.write_all(body).await.unwrap();
        stream.shutdown().await.unwrap();
    });
    let store = Arc::new(ModelCatalogStore::from_embedded().unwrap());
    let updater = ModelsUpdater::new(
        Arc::clone(&store),
        Arc::new(WreqModelsSource::new(None).unwrap()),
        Vec::new(),
        Arc::new(ModelRefreshSink::default()),
    )
    .with_devin_sources(vec![endpoint.clone()]);
    let refreshed = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        updater.refresh_devin_once(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(refreshed.source, endpoint);
    assert!(refreshed.changed);
    assert!(store.lookup_model_info("devin/delayed-body").is_some());
    assert_eq!(store.models_for_channel("devin").unwrap().len(), 2);
    tokio::time::timeout(std::time::Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
}
