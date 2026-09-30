//! Daemon-owned, single-session resolver for persisted native transfer jobs.
use super::*;
use ctox_sync::native::NativeSessionTargetProvider;
use std::{collections::BTreeMap, time::Duration};
use tokio::{sync::Mutex, task::JoinHandle};

const START_TIMEOUT: Duration = Duration::from_secs(20);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Credential providers are installed by the native account host, keyed by its
/// saved target IDs. They must remain live lookups, never copied UI tokens.
/// This resolver does not enroll targets, issue grants, or refresh job bindings.
pub(crate) struct NativeTransferPeerResolver {
    host: Arc<dyn BusinessDataSessionHost>,
    providers: BTreeMap<String, NativeSessionTargetProvider>,
    active: Mutex<Option<ActiveSession>>,
}

struct ActiveSession {
    request: DownloadRequest,
    starting: Option<JoinHandle<Result<Arc<NativeSyncSession>>>>,
    session: Option<Arc<NativeSyncSession>>,
    source: Option<Arc<NativePeerRangeSource>>,
}

impl Drop for ActiveSession {
    fn drop(&mut self) {
        if let Some(starting) = &self.starting {
            starting.abort();
        }
    }
}

impl ActiveSession {
    async fn finish_start(&mut self) -> Result<()> {
        if let Some(starting) = &mut self.starting {
            // Keep the handle in the slot across await: pause/cancel of the
            // worker's authorization future must not detach native startup.
            let result = starting.await;
            self.starting = None;
            self.session = Some(result.context("native transfer startup task failed")??);
        }
        ensure!(self.session.is_some(), "native transfer startup failed");
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        // Startup has its own deadline. Even when authorization is cancelled,
        // shutdown retrieves the resulting session and drains it explicitly.
        let _ = self.finish_start().await;
        if let Some(session) = &self.session {
            tokio::time::timeout(CLOSE_TIMEOUT, session.shutdown())
                .await
                .context("native transfer session cleanup timed out")?;
        }
        self.source = None;
        self.session = None;
        Ok(())
    }
}

impl NativeTransferPeerResolver {
    pub(crate) fn new(
        host: Arc<dyn BusinessDataSessionHost>,
        providers: BTreeMap<String, NativeSessionTargetProvider>,
    ) -> Self {
        Self {
            host,
            providers,
            active: Mutex::new(None),
        }
    }

    async fn resolve(
        &self,
        active: &mut Option<ActiveSession>,
        request: &DownloadRequest,
    ) -> Result<Arc<NativePeerRangeSource>> {
        current_account(self.host.as_ref(), request).await?;
        let reusable = active.as_ref().is_some_and(|active| {
            active.request == *request
                && (active.starting.is_some() || active.session.is_some())
                && active.source.as_ref().is_none_or(|source| {
                    source
                        .pool
                        .connection_handler
                        .is_peer_current(&source.connection)
                        && source.pool.is_peer_ready_for_control(&source.connection)
                })
        });
        if !reusable {
            if let Some(previous) = active.as_mut() {
                previous.close().await?;
            }
            *active = None;
            let binding = request
                .peer_source
                .as_ref()
                .and_then(|source| source.account_binding.as_ref())
                .context("original native account binding required")?;
            let provider = self
                .providers
                .get(&binding.target_id)
                .context("native target credential provider unavailable")?
                .clone();
            let mut options = self.host.native_options(&binding.target_id).await?;
            ensure!(
                options.local_session_provider.is_none(),
                "native transfer options already contain a credential provider"
            );
            current_account(self.host.as_ref(), request).await?;
            options.local_session_provider = Some(fenced_provider(
                self.host.clone(),
                request.clone(),
                provider,
            ));
            options.bringup_timeout = options.bringup_timeout.min(START_TIMEOUT);
            let starting = tokio::spawn(async move {
                let session = tokio::time::timeout(
                    START_TIMEOUT,
                    NativeSyncSession::start_data_client(options),
                )
                .await
                .context("native transfer startup timed out")??;
                Ok(Arc::new(session))
            });
            *active = Some(ActiveSession {
                request: request.clone(),
                starting: Some(starting),
                session: None,
                source: None,
            });
        }
        let active = active.as_mut().context("native transfer session missing")?;
        active.finish_start().await?;
        if let Some(source) = &active.source {
            return Ok(source.clone());
        }
        let session = active
            .session
            .as_ref()
            .context("native transfer session missing")?;
        let connection = tokio::time::timeout(START_TIMEOUT, async {
            loop {
                current_account(self.host.as_ref(), request).await?;
                let pool = session.pool();
                ensure!(
                    !pool.canceled.load(std::sync::atomic::Ordering::SeqCst),
                    "native transfer session stopped"
                );
                let connections = pool.connection_handler.current_connections();
                ensure!(connections.len() <= 1, "ambiguous native transfer source");
                if let Some(connection) = connections.into_iter().next() {
                    if pool.is_peer_ready_for_control(&connection) {
                        break Ok::<_, anyhow::Error>(connection);
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .context("native transfer source readiness timed out")??;
        let admission = Arc::new(crate::transfers_grant::NativeTransferGrantAdmission::new(
            session.clone(),
        ));
        let source = Arc::new(
            NativePeerRangeSource::bind_enrolled(
                session.clone(),
                connection,
                request.clone(),
                self.host.clone(),
                admission,
            )
            .await?,
        );
        active.source = Some(source.clone());
        Ok(source)
    }
}

/// NativeSyncSession proves these pins before calling credentials. Check the
/// original account both around target resolution and around credential release
/// so an account switch during an await cannot rebind a persisted transfer.
fn fenced_provider(
    host: Arc<dyn BusinessDataSessionHost>,
    request: DownloadRequest,
    provider: NativeSessionTargetProvider,
) -> NativeSessionTargetProvider {
    Arc::new(move |connection| {
        let host = host.clone();
        let request = request.clone();
        let provider = provider.clone();
        Box::pin(async move {
            current_account(host.as_ref(), &request)
                .await
                .map_err(|_| stale_account())?;
            let mut target = provider(connection).await?;
            validate_target(&request, &target.public_identity, &target.instance_id)
                .map_err(|_| stale_account())?;
            current_account(host.as_ref(), &request)
                .await
                .map_err(|_| stale_account())?;
            target.credentials = fenced_credentials(host, request, target.credentials);

            Ok(target)
        })
    })
}

pub(super) fn fenced_credentials<P: Send + 'static>(
    host: Arc<dyn BusinessDataSessionHost>,
    request: DownloadRequest,
    credentials: rxdb::plugins::replication_webrtc::local_session::LocalSessionProvider<P>,
) -> rxdb::plugins::replication_webrtc::local_session::LocalSessionProvider<P> {
    Arc::new(move |connection, nonce| {
        let host = host.clone();
        let request = request.clone();
        let credentials = credentials.clone();
        Box::pin(async move {
            current_account(host.as_ref(), &request)
                .await
                .map_err(|_| stale_account())?;
            let result = credentials(connection, nonce).await?;
            current_account(host.as_ref(), &request)
                .await
                .map_err(|_| stale_account())?;
            Ok(result)
        })
    })
}

pub(super) fn validate_target(
    request: &DownloadRequest,
    public_identity: &str,
    instance_id: &str,
) -> Result<()> {
    let source = request
        .peer_source
        .as_ref()
        .context("peer source required")?;
    ensure!(
        source.public_key == public_identity && source.instance_id == instance_id,
        "credential provider returned a different native target"
    );
    Ok(())
}

fn stale_account() -> rxdb::rx_error::RxError {
    rxdb::rx_error::new_rx_error(
        "RC_WEBRTC_PEER",
        Some(serde_json::json!({
            "code": "native_transfer_account_stale",
            "message": "original native transfer account or target is unavailable"
        })),
    )
}

impl PeerRangeSource for NativeTransferPeerResolver {
    fn authorize<'a>(
        &'a self,
        request: &'a DownloadRequest,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let mut active = self.active.lock().await;
            let result = async {
                self.resolve(&mut active, request)
                    .await?
                    .authorize(request)
                    .await
            }
            .await;
            if result.is_err() {
                if let Some(session) = active.as_mut() {
                    session.close().await?;
                }
                *active = None;
            }
            result
        })
    }

    fn read_range<'a>(
        &'a self,
        request: &'a DownloadRequest,
        offset: u64,
        length: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + 'a>> {
        Box::pin(async move {
            let mut active = self.active.lock().await;
            let result = async {
                self.resolve(&mut active, request)
                    .await?
                    .read_range(request, offset, length)
                    .await
            }
            .await;
            if result.is_err() {
                if let Some(session) = active.as_mut() {
                    session.close().await?;
                }
                *active = None;
            }
            result
        })
    }

    fn shutdown(&self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            let mut active = self.active.lock().await;
            if let Some(session) = active.as_mut() {
                session.close().await?;
            }
            *active = None;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelled_wait_keeps_startup_owned_until_shutdown() {
        let (release, finish) = tokio::sync::oneshot::channel();
        let (completed, completion) = tokio::sync::oneshot::channel();
        let starting = tokio::spawn(async move {
            finish.await.unwrap();
            completed.send(()).unwrap();
            anyhow::bail!("simulated startup failure")
        });
        let mut active = ActiveSession {
            request: DownloadRequest {
                id: "lifecycle".into(),
                sources: vec![],
                peer_source: None,
                sha256: "a".repeat(64),
                size: 1,
            },
            starting: Some(starting),
            session: None,
            source: None,
        };
        assert!(
            tokio::time::timeout(Duration::from_millis(10), active.finish_start())
                .await
                .is_err()
        );
        assert!(
            active.starting.is_some(),
            "cancelled waiter must retain startup ownership"
        );
        release.send(()).unwrap();
        active.close().await.unwrap();
        completion.await.unwrap();
        assert!(active.starting.is_none());
        active.close().await.unwrap();
    }
}
