//! Bounded local operator admission; payload writes remain daemon-owned.
use super::*;
use crate::{
    native_transfer_accounts::{NativeTransferAccount, NativeTransferAccountHost},
    transfers_grant::{NativeTransferGrantAdmission, TransferGrantScope},
    transfers_peer::{current_account, principal_digest, NativePeerRangeSource},
};
use ctox_sync::native::NativeSyncSession;
use ctox_transfers::{PeerAccountBinding, PeerSource, Store, Transfer};
use rxdb::plugins::replication_webrtc::{WebRTCConnectionHandler, WebRTCRsConnection};

pub(crate) struct PeerDownload {
    pub id: String,
    pub target_id: String,
    pub sha256: String,
    pub size: u64,
    pub file_id: String,
}

impl PeerDownload {
    fn request(&self, account: &NativeTransferAccount) -> Result<DownloadRequest> {
        ensure!(account.target_id == self.target_id, "native target changed");
        let request = DownloadRequest {
            id: self.id.clone(),
            sha256: self.sha256.clone(),
            size: self.size,
            sources: Vec::new(),
            peer_source: Some(PeerSource {
                instance_id: account.instance_id.clone(),
                public_key: account.public_identity.clone(),
                collection: "desktop_files".into(),
                file_id: self.file_id.clone(),
                account_binding: None,
            }),
        };
        TransferGrantScope::from_request(&request)?;
        Ok(request)
    }
}

async fn same_account(
    host: &NativeTransferAccountHost,
    account: &NativeTransferAccount,
) -> Result<()> {
    ensure!(
        host.account(&account.target_id).await?.as_ref() == Some(account),
        "native account changed during transfer admission"
    );
    Ok(())
}

async fn ready_connection(
    host: &NativeTransferAccountHost,
    account: &NativeTransferAccount,
    session: &NativeSyncSession,
) -> Result<WebRTCRsConnection> {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            same_account(host, account).await?;
            let pool = session.pool();
            ensure!(
                !pool.canceled.load(std::sync::atomic::Ordering::SeqCst),
                "native admission session stopped"
            );
            let connections = pool.connection_handler.current_connections();
            ensure!(connections.len() <= 1, "ambiguous native transfer source");
            if let Some(connection) = connections.into_iter().next() {
                if pool.is_peer_ready_for_control(&connection) {
                    return Ok(connection);
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .context("native transfer admission readiness timed out")?
}

async fn prepare(
    host: Arc<NativeTransferAccountHost>,
    download: PeerDownload,
    session_slot: &mut Option<Arc<NativeSyncSession>>,
) -> Result<DownloadRequest> {
    let account = host
        .account(&download.target_id)
        .await?
        .context("target has no enrolled native account")?;
    let mut request = download.request(&account)?;
    let (mut options, deadline) = host
        .native_options_with_deadline(&download.target_id)
        .await?;
    ensure!(
        options.local_session_provider.is_none(),
        "native options already contain credentials"
    );
    same_account(&host, &account).await?;
    options.local_session_provider = Some(host.provider_for_account(account.clone()));
    options.bringup_timeout = options.bringup_timeout.min(Duration::from_secs(20));
    let session = Arc::new(
        tokio::time::timeout(
            Duration::from_secs(20),
            NativeSyncSession::start_data_client(options),
        )
        .await
        .context("native transfer admission startup timed out")??,
    );
    *session_slot = Some(session.clone());
    let connection = ready_connection(&host, &account, &session).await?;
    let proof = session
        .peer_identity_proof(
            connection.clone(),
            &account.public_identity,
            &account.instance_id,
        )
        .await?;
    ensure!(
        proof.principal.as_ref() == Some(&account.principal),
        "native transfer admission principal mismatch"
    );
    same_account(&host, &account).await?;
    ensure!(
        chrono::Utc::now().timestamp_millis() < deadline.expires_at_ms,
        "native transfer admission route expired"
    );
    let admission = Arc::new(NativeTransferGrantAdmission::new(session.clone()));
    let grant = admission.issue(&connection, &request).await?;
    request
        .peer_source
        .as_mut()
        .context("peer source required")?
        .account_binding = Some(PeerAccountBinding {
        target_id: account.target_id.clone(),
        account_epoch: account.account_epoch,
        principal_sha256: principal_digest(&account.principal)?,
        grant_id: grant.grant_id,
    });
    // Grant issuance alone is insufficient. Exercise the same original-account,
    // source-grant and current file-policy checks used by the durable worker.
    let _authorized = NativePeerRangeSource::bind_enrolled(
        session,
        connection,
        request.clone(),
        host.clone(),
        admission,
    )
    .await?;
    same_account(&host, &account).await?;
    ensure!(
        chrono::Utc::now().timestamp_millis() < deadline.expires_at_ms,
        "native transfer admission route expired"
    );
    Ok(request)
}

/// CLI work is bounded to authentication/grant admission. It cannot run payload
/// downloads or open the daemon's database. The daemon revalidates after enqueue.
pub(crate) fn enqueue_peer(root: &Path, store: &Store, download: PeerDownload) -> Result<Transfer> {
    let directory = root.join("runtime/transfers/admission");
    std::fs::create_dir_all(&directory)?;
    let temporary = tempfile::Builder::new()
        .prefix("native-")
        .tempdir_in(directory)?;
    let database = QueryDatabase::new(temporary.path());
    let host = account_host(root, database.clone());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let mut session = None;
        let result = tokio::time::timeout(
            Duration::from_secs(60),
            prepare(host.clone(), download, &mut session),
        )
        .await
        .context("native transfer admission deadline")
        .and_then(|result| result);
        let transport_closed = if let Some(session) = &session {
            tokio::time::timeout(Duration::from_secs(5), session.shutdown())
                .await
                .context("native transfer admission cleanup timed out")
        } else {
            Ok(())
        };
        // As in the daemon, transport shutdown must finish before host storage
        // closes. Failed admission still closes storage when transport drains.
        transport_closed?;
        database.close().await?;
        let request = result?;
        current_account(host.as_ref(), &request).await?;
        store.enqueue(request)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_sync::business_data_contract::NativeBusinessDataPrincipal;

    fn account() -> NativeTransferAccount {
        NativeTransferAccount {
            version: 1,
            target_id: "saved-target".into(),
            public_identity: format!("ed25519:{}", "a".repeat(64)),
            instance_id: "source-instance".into(),
            account_epoch: 7,
            principal: NativeBusinessDataPrincipal {
                user_id: "native-user".into(),
                authorization_epoch: 9,
                device: None,
            },
            active: true,
        }
    }
    fn download() -> PeerDownload {
        PeerDownload {
            id: "download-1".into(),
            target_id: "saved-target".into(),
            sha256: "b".repeat(64),
            size: 4,
            file_id: "file-1".into(),
        }
    }
    #[test]
    fn request_derives_source_from_enrollment_and_requires_real_grant_later() {
        let request = download().request(&account()).unwrap();
        let source = request.peer_source.unwrap();
        assert_eq!(source.public_key, account().public_identity);
        assert_eq!(source.instance_id, account().instance_id);
        assert_eq!(source.collection, "desktop_files");
        assert!(source.account_binding.is_none());
        assert!(request.sources.is_empty());
        let mut other = account();
        other.target_id = "other-target".into();
        assert!(download().request(&other).is_err());
        let mut invalid = download();
        invalid.sha256 = "unverified".into();
        assert!(invalid.request(&account()).is_err());
    }
    #[test]
    fn missing_enrollment_creates_no_job_and_removes_admission_database() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(
            root.path().join("core.sqlite3"),
            root.path().join("transfers"),
        )
        .unwrap();
        assert!(enqueue_peer(root.path(), &store, download()).is_err());
        assert!(store.get("download-1").is_err());
        assert_eq!(
            std::fs::read_dir(root.path().join("runtime/transfers/admission"))
                .unwrap()
                .count(),
            0
        );
    }
}
