//! Account fencing without starting a transport or supplying credentials.
use super::*;
use ctox_sync::{
    business_data_contract::NativeBusinessDataDeviceIdentity, native::NativeSyncOptions,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

#[tokio::test]
async fn range_retry_rechecks_original_account_before_more_traffic() {
    let (host, request) = fixture();
    let reads = AtomicUsize::new(0);
    let error = fetch_authorized_range(
        || async { current_account(&host, &request).await.map(|_| ()) },
        || async {
            reads.fetch_add(1, Ordering::SeqCst);
            host.saved.lock().unwrap().as_mut().unwrap().account_epoch += 1;
            Err(rxdb::rx_error::new_rx_error(
                "RC_WEBRTC_FILE",
                Some(serde_json::json!({"reason": "file_timeout"})),
            ))
        },
        Duration::ZERO,
    )
    .await
    .unwrap_err();
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(
        error.downcast_ref::<PeerReadFailure>(),
        Some(&PeerReadFailure::Authorization)
    );
}

#[tokio::test]
async fn range_retry_is_bounded_and_recovers_a_discarded_response() {
    let reads = AtomicUsize::new(0);
    let checks = AtomicUsize::new(0);
    let bytes = fetch_authorized_range(
        || async {
            checks.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
        || async {
            if reads.fetch_add(1, Ordering::SeqCst) < 2 {
                Err(rxdb::rx_error::new_rx_error(
                    "RC_WEBRTC_FILE",
                    Some(serde_json::json!({"reason": "chunk_sequence_gap"})),
                ))
            } else {
                Ok(FileRangeBytes {
                    offset: 17,
                    bytes: vec![1, 2],
                })
            }
        },
        Duration::ZERO,
    )
    .await
    .unwrap();
    assert_eq!(bytes.offset, 17);
    assert_eq!(bytes.bytes, vec![1, 2]);
    assert_eq!(reads.load(Ordering::SeqCst), 3);
    assert_eq!(checks.load(Ordering::SeqCst), 3);
    let attempts = AtomicUsize::new(0);
    let error = fetch_authorized_range(
        || async { Ok(()) },
        || async {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err(rxdb::rx_error::new_rx_error(
                "RC_WEBRTC_FILE",
                Some(serde_json::json!({"reason": "file_timeout"})),
            ))
        },
        Duration::ZERO,
    )
    .await
    .unwrap_err();
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    assert_eq!(error.to_string(), "PEER_FILE_TIMEOUT");
}

#[tokio::test]
async fn range_rejection_and_unknown_reasons_never_retry_or_expose_remote_material() {
    for (reason, expected) in [
        ("file_not_accepted", PeerReadFailure::Rejected),
        ("secret-url-token", PeerReadFailure::Unavailable),
    ] {
        let attempts = AtomicUsize::new(0);
        let error = fetch_authorized_range(
            || async { Ok(()) },
            || async {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(rxdb::rx_error::new_rx_error(
                    "RC_WEBRTC_FILE",
                    Some(serde_json::json!({"reason": reason, "url": "secret-url-token"})),
                ))
            },
            Duration::ZERO,
        )
        .await
        .unwrap_err();
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert_eq!(error.to_string(), expected.code());
        assert!(!format!("{error:?}").contains("secret-url-token"));
    }
}

struct Host {
    saved: Mutex<Option<SavedBusinessDataTarget>>,
    principal: Mutex<Option<NativeBusinessDataPrincipal>>,
    reads_before_switch: AtomicUsize,
    options_requests: AtomicUsize,
}

// Spell out the async-trait ABI so the daemon needs no additional dependency
// solely for these test doubles.
impl BusinessDataSessionHost for Host {
    fn saved_target<'a, 'b, 'f>(
        &'a self,
        target_id: &'b str,
    ) -> Pin<Box<dyn Future<Output = std::io::Result<Option<SavedBusinessDataTarget>>> + Send + 'f>>
    where
        'a: 'f,
        'b: 'f,
        Self: 'f,
    {
        Box::pin(async move {
            assert_eq!(target_id, "target");
            Ok(self.saved.lock().unwrap().clone())
        })
    }
    fn current_principal<'a, 'b, 'f>(
        &'a self,
        target_id: &'b str,
    ) -> Pin<
        Box<dyn Future<Output = std::io::Result<Option<NativeBusinessDataPrincipal>>> + Send + 'f>,
    >
    where
        'a: 'f,
        'b: 'f,
        Self: 'f,
    {
        Box::pin(async move {
            assert_eq!(target_id, "target");
            if self
                .reads_before_switch
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                    count.checked_sub(1)
                })
                == Ok(1)
            {
                self.saved.lock().unwrap().as_mut().unwrap().account_epoch += 1;
            }
            Ok(self.principal.lock().unwrap().clone())
        })
    }
    fn native_options<'a, 'b, 'f>(
        &'a self,
        _: &'b str,
    ) -> Pin<Box<dyn Future<Output = std::io::Result<NativeSyncOptions>> + Send + 'f>>
    where
        'a: 'f,
        'b: 'f,
        Self: 'f,
    {
        Box::pin(async move {
            self.options_requests.fetch_add(1, Ordering::SeqCst);
            Err(std::io::Error::other(
                "fixture transport options unavailable",
            ))
        })
    }
}

fn fixture() -> (Host, DownloadRequest) {
    let principal = NativeBusinessDataPrincipal {
        user_id: "user".into(),
        authorization_epoch: 12,
        device: Some(NativeBusinessDataDeviceIdentity {
            pairing_id: "pairing".into(),
            device_id: "device".into(),
            proof_key_thumbprint: "device-key".into(),
        }),
    };
    let host = Host {
        saved: Mutex::new(Some(SavedBusinessDataTarget {
            public_identity: "source-key".into(),
            instance_id: "source".into(),
            account_epoch: 34,
        })),
        principal: Mutex::new(Some(principal.clone())),
        reads_before_switch: AtomicUsize::new(0),
        options_requests: AtomicUsize::new(0),
    };
    let request = DownloadRequest {
        id: "bound-job".into(),
        sources: vec![],
        sha256: "a".repeat(64),
        size: 7,
        storage: None,
        peer_source: Some(ctox_transfers::PeerSource {
            instance_id: "source".into(),
            public_key: "source-key".into(),
            collection: "desktop_files".into(),
            file_id: "file".into(),
            account_binding: Some(PeerAccountBinding {
                target_id: "target".into(),
                account_epoch: 34,
                grant_id: "grant".into(),
                principal_sha256: principal_digest(&principal).unwrap(),
            }),
        }),
    };
    (host, request)
}

#[tokio::test]
async fn current_authority_rejects_logout_account_device_and_enrollment_changes() {
    let (host, request) = fixture();
    let original = request.clone();
    let saved = host.saved.lock().unwrap().clone().unwrap();
    let principal = host.principal.lock().unwrap().clone().unwrap();
    assert_eq!(current_account(&host, &request).await.unwrap(), principal);
    for field in 0..5 {
        let mut changed = principal.clone();
        match field {
            0 => changed.user_id = "other-user".into(),
            1 => changed.authorization_epoch += 1,
            2 => changed.device.as_mut().unwrap().device_id = "other-device".into(),
            3 => changed.device.as_mut().unwrap().proof_key_thumbprint = "other-key".into(),
            _ => changed.device = None,
        }
        *host.principal.lock().unwrap() = Some(changed);
        assert!(current_account(&host, &request).await.is_err());
    }
    *host.principal.lock().unwrap() = None;
    assert!(current_account(&host, &request).await.is_err());
    *host.principal.lock().unwrap() = Some(principal.clone());
    for field in 0..3 {
        let mut changed = saved.clone();
        match field {
            0 => changed.account_epoch += 1,
            1 => changed.instance_id = "other-instance".into(),
            _ => changed.public_identity = "other-source-key".into(),
        }
        *host.saved.lock().unwrap() = Some(changed);
        assert!(current_account(&host, &request).await.is_err());
    }
    *host.saved.lock().unwrap() = None;
    assert!(current_account(&host, &request).await.is_err());
    *host.saved.lock().unwrap() = Some(saved);
    assert_eq!(current_account(&host, &request).await.unwrap(), principal);
    assert_eq!(
        request, original,
        "checking authority must never rebind a job"
    );
}

#[tokio::test]
async fn account_switch_during_snapshot_rejects_admission_and_enqueue() {
    let (host, mut request) = fixture();
    host.reads_before_switch.store(1, Ordering::SeqCst);
    assert!(current_account(&host, &request).await.is_err());
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(
        temp.path().join("state.sqlite"),
        temp.path().join("objects"),
    )
    .unwrap();
    request.peer_source.as_mut().unwrap().account_binding = None;
    // Switch on the second principal read, after the original snapshot and
    // the second saved-target check. A final epoch check must reject this.
    host.reads_before_switch.store(2, Ordering::SeqCst);
    assert!(
        enqueue_enrolled_peer(&store, &host, "target", "grant", request.clone())
            .await
            .is_err()
    );
    assert!(
        store.get(&request.id).is_err(),
        "rejected admission must not persist a runnable job"
    );
    let enqueued = enqueue_enrolled_peer(&store, &host, "target", "grant", request)
        .await
        .unwrap();
    assert_eq!(
        enqueued
            .request
            .peer_source
            .unwrap()
            .account_binding
            .unwrap()
            .account_epoch,
        36
    );
}

#[tokio::test]
async fn resolver_rejects_missing_credentials_and_revoked_account_before_transport() {
    let (host, request) = fixture();
    let host = Arc::new(host);
    let resolver = NativeTransferPeerResolver::new(host.clone(), Default::default());
    let error = resolver.authorize(&request).await.unwrap_err();
    assert!(error
        .to_string()
        .contains("credential provider unavailable"));
    *host.principal.lock().unwrap() = None;
    let error = resolver.authorize(&request).await.unwrap_err();
    assert!(error.to_string().contains("account is unavailable"));
    resolver.shutdown().await.unwrap();
    resolver.shutdown().await.unwrap();
}

#[tokio::test]
async fn late_provider_becomes_visible_without_rebinding_or_daemon_restart() {
    let (host, request) = fixture();
    let host = Arc::new(host);
    let available: Arc<Mutex<Option<ctox_sync::native::NativeSessionTargetProvider>>> =
        Arc::new(Mutex::new(None));
    let lookups = Arc::new(AtomicUsize::new(0));
    let registry = available.clone();
    let calls = lookups.clone();
    let resolver = NativeTransferPeerResolver::with_provider_lookup(
        host.clone(),
        Arc::new(move |id| {
            assert_eq!(id, "target");
            calls.fetch_add(1, Ordering::SeqCst);
            let provider = registry.lock().unwrap().clone();
            Box::pin(
                async move { provider.context("native target credential provider unavailable") },
            )
        }),
    );
    assert!(resolver
        .authorize(&request)
        .await
        .unwrap_err()
        .to_string()
        .contains("credential provider unavailable"));
    assert_eq!(host.options_requests.load(Ordering::SeqCst), 0);
    *available.lock().unwrap() = Some(Arc::new(|_| {
        Box::pin(async {
            panic!("failed options must prevent transport and credential resolution")
        })
    }));
    assert!(resolver
        .authorize(&request)
        .await
        .unwrap_err()
        .to_string()
        .contains("fixture transport options unavailable"));
    assert_eq!(host.options_requests.load(Ordering::SeqCst), 1);
    assert_eq!(lookups.load(Ordering::SeqCst), 2);
    *host.principal.lock().unwrap() = None;
    assert!(resolver.authorize(&request).await.is_err());
    assert_eq!(
        lookups.load(Ordering::SeqCst),
        2,
        "revocation must precede provider lookup"
    );
    assert_eq!(host.options_requests.load(Ordering::SeqCst), 1);
    resolver.shutdown().await.unwrap();
}

#[tokio::test]
async fn credential_release_rechecks_account_after_provider_await() {
    let (host, request) = fixture();
    let host = Arc::new(host);
    let changed_host = host.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let called = calls.clone();
    let provider = resolver::fenced_credentials(
        host,
        request,
        Arc::new(move |_: (), _| {
            let host = changed_host.clone();
            let called = called.clone();
            Box::pin(async move {
                called.fetch_add(1, Ordering::SeqCst);
                tokio::task::yield_now().await;
                *host.principal.lock().unwrap() = None;
                Ok(
                    rxdb::plugins::replication_webrtc::local_session::LocalSessionCredentials {
                        capability_token: "test-only-unreleased-value".into(),
                        device_proof: None,
                    },
                )
            })
        }),
    );
    assert!(provider((), None).await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(provider((), None).await.is_err());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "logout must prevent another provider call"
    );
}

#[test]
fn credential_target_must_match_original_source_pins() {
    let (_, request) = fixture();
    resolver::validate_target(&request, "source-key", "source").unwrap();
    assert!(resolver::validate_target(&request, "other-key", "source").is_err());
    assert!(resolver::validate_target(&request, "source-key", "other-source").is_err());
}
