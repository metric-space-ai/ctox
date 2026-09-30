//! Account fencing without starting a transport or supplying credentials.
use super::*;
use ctox_sync::{
    business_data_contract::NativeBusinessDataDeviceIdentity, native::NativeSyncOptions,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

struct Host {
    saved: Mutex<Option<SavedBusinessDataTarget>>,
    principal: Mutex<Option<NativeBusinessDataPrincipal>>,
    switch_during_read: AtomicBool,
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
            if self.switch_during_read.swap(false, Ordering::SeqCst) {
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
        Box::pin(async { panic!("account checks must not start a transport") })
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
        switch_during_read: AtomicBool::new(false),
    };
    let request = DownloadRequest {
        id: "bound-job".into(),
        sources: vec![],
        sha256: "a".repeat(64),
        size: 7,
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
    host.switch_during_read.store(true, Ordering::SeqCst);
    assert!(current_account(&host, &request).await.is_err());
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(
        temp.path().join("state.sqlite"),
        temp.path().join("objects"),
    )
    .unwrap();
    request.peer_source.as_mut().unwrap().account_binding = None;
    host.switch_during_read.store(true, Ordering::SeqCst);
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
