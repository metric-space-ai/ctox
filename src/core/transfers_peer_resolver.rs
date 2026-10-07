//! Daemon-owned, single-session resolver for persisted native transfer jobs.
use super::*;
use ctox_sync::native::NativeSessionTargetProvider;
use rxdb::plugins::replication_webrtc::WebRTCConnectionHandler;
use std::{collections::BTreeMap, time::Duration};
use tokio::{sync::Mutex, task::JoinHandle};

const START_TIMEOUT: Duration = Duration::from_secs(20);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) type NativeTransferProviderLookup = Arc<
    dyn Fn(String) -> futures_util::future::BoxFuture<'static, Result<NativeSessionTargetProvider>>
        + Send
        + Sync,
>;

/// Credential providers are installed by the native account host, keyed by its
/// saved target IDs. They must remain live lookups, never copied UI tokens.
/// This resolver does not enroll targets, issue grants, or refresh job bindings.
pub(crate) struct NativeTransferPeerResolver {
    host: Arc<dyn BusinessDataSessionHost>,
    account_host: Option<Arc<crate::native_transfer_accounts::NativeTransferAccountHost>>,
    providers: NativeTransferProviderLookup,
    active: Mutex<Option<ActiveSession>>,
}

struct ActiveSession {
    request: DownloadRequest,
    starting: Option<JoinHandle<Result<Arc<NativeSyncSession>>>>,
    session: Option<Arc<NativeSyncSession>>,
    source: Option<Arc<NativePeerRangeSource>>,
    deadline: Option<crate::native_transfer_accounts::NativeTransferSessionDeadline>,
    bootstrap: bool,
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
        self.finish_start_with_timeout(START_TIMEOUT).await
    }

    async fn finish_start_with_timeout(&mut self, timeout: Duration) -> Result<()> {
        if let Some(starting) = &mut self.starting {
            // Keep the handle in the slot across await: pause/cancel of the
            // worker's authorization future must not detach native startup.
            let result = tokio::time::timeout(timeout, starting)
                .await
                .context("native transfer startup cleanup is still pending")?;
            self.starting = None;
            self.session = Some(result.context("native transfer startup task failed")??);
        }
        ensure!(self.session.is_some(), "native transfer startup failed");
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        self.close_with_timeout(START_TIMEOUT).await
    }

    async fn close_with_timeout(&mut self, timeout: Duration) -> Result<()> {
        // Startup has its own deadline. Even when authorization is cancelled,
        // shutdown retrieves the resulting session and drains it explicitly.
        if let Err(error) = self.finish_start_with_timeout(timeout).await {
            // Terminal bring-up errors have already drained native resources.
            // A timed-out waiter still owns a running startup/cleanup task.
            if self.starting.is_some() {
                return Err(error);
            }
        }
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
        Self::with_provider_lookup(
            host,
            Arc::new(move |target_id| {
                let provider = providers.get(&target_id).cloned();
                Box::pin(async move {
                    provider.context("native target credential provider unavailable")
                })
            }),
        )
    }

    /// Resolve the current provider when opening each session. Enrollment after
    /// daemon boot must not require rebuilding a cached target-ID map.
    pub(crate) fn with_provider_lookup(
        host: Arc<dyn BusinessDataSessionHost>,
        providers: NativeTransferProviderLookup,
    ) -> Self {
        Self {
            host,
            account_host: None,
            providers,
            active: Mutex::new(None),
        }
    }

    pub(crate) fn with_account_host(
        host: Arc<crate::native_transfer_accounts::NativeTransferAccountHost>,
    ) -> Self {
        let provider_host = host.clone();
        let mut resolver = Self::with_provider_lookup(
            host.clone(),
            Arc::new(move |target_id| {
                let host = provider_host.clone();
                Box::pin(async move { Ok(host.provider(target_id)) })
            }),
        );
        resolver.account_host = Some(host);
        resolver
    }

    async fn renew_if_due(&self, active: &ActiveSession) -> Result<bool> {
        let Some(deadline) = active.deadline else {
            return Ok(false);
        };
        let now = chrono::Utc::now().timestamp_millis();
        if now >= deadline.expires_at_ms {
            // Another admitted client may have refreshed storage while this
            // worker was idle. Retire this transport, then load the live route.
            return Ok(true);
        }
        if now < deadline.refresh_after_ms {
            return Ok(false);
        }
        // Pending startup has no admitted channel for renewal yet. It remains
        // bounded, and the deadline is checked again before returning data.
        let (Some(session), Some(source)) = (&active.session, &active.source) else {
            return Ok(false);
        };
        if !source
            .pool
            .connection_handler
            .is_peer_current(&source.connection)
            || !source.pool.is_peer_ready_for_control(&source.connection)
        {
            return Ok(false);
        }
        let host = self
            .account_host
            .as_ref()
            .context("native renewal host missing")?;
        let original = active
            .request
            .peer_source
            .as_ref()
            .context("original peer source required")?;
        let binding = original
            .account_binding
            .as_ref()
            .context("original native account binding required")?;
        host.provision_from_session(
            crate::native_data_device::NativeDeviceKeyScope {
                target_id: binding.target_id.clone(),
                source_instance_id: original.instance_id.clone(),
                source_public_identity: original.public_key.clone(),
                account_epoch: binding.account_epoch,
            },
            session,
            &source.connection,
        )
        .await?;
        current_account(self.host.as_ref(), &active.request).await?;
        // Rebuild immediately with the new source-confirmed ICE/credentials;
        // never stretch the old transport's deadline after refreshing storage.
        Ok(true)
    }

    async fn resolve(
        &self,
        active: &mut Option<ActiveSession>,
        request: &DownloadRequest,
    ) -> Result<Arc<NativePeerRangeSource>> {
        // At most one control-only bootstrap followed by one payload session.
        for attempt in 0..2 {
            if let Some(source) = self.resolve_once(active, request, attempt == 0).await? {
                return Ok(source);
            }
        }
        anyhow::bail!("native routing recovery did not produce a current route")
    }

    async fn resolve_once(
        &self,
        active: &mut Option<ActiveSession>,
        request: &DownloadRequest,
        allow_bootstrap: bool,
    ) -> Result<Option<Arc<NativePeerRangeSource>>> {
        current_account(self.host.as_ref(), request).await?;
        if let Some(previous) = active.as_ref() {
            if previous.request == *request && self.renew_if_due(previous).await? {
                active.as_mut().unwrap().close().await?;
                *active = None;
            }
        }
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
            let provider = (self.providers)(binding.target_id.clone()).await?;
            let (mut options, deadline, bootstrap) = if let Some(host) = &self.account_host {
                if let Some(options) = host.recovery_options(&binding.target_id).await? {
                    ensure!(
                        allow_bootstrap,
                        "native routing recovery did not produce a current route"
                    );
                    (options, None, true)
                } else {
                    let (options, deadline) = host
                        .native_options_with_deadline(&binding.target_id)
                        .await?;
                    (options, Some(deadline), false)
                }
            } else {
                (
                    self.host.native_options(&binding.target_id).await?,
                    None,
                    false,
                )
            };
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
                // Native bring-up enforces options.bringup_timeout and drains
                // resources on failure. Do not cancel that cleanup with a
                // second timer around the whole native future.
                let session = NativeSyncSession::start_data_client(options).await?;
                Ok(Arc::new(session))
            });
            *active = Some(ActiveSession {
                request: request.clone(),
                starting: Some(starting),
                session: None,
                source: None,
                deadline,
                bootstrap,
            });
        }
        let slot = active;
        let active = slot.as_mut().context("native transfer session missing")?;
        active.finish_start().await?;
        if let Some(source) = &active.source {
            ensure!(
                !active.bootstrap,
                "bootstrap cannot expose a payload source"
            );
            return Ok(Some(source.clone()));
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
        if active.bootstrap {
            let original = request
                .peer_source
                .as_ref()
                .context("peer source required")?;
            let binding = original
                .account_binding
                .as_ref()
                .context("account binding required")?;
            self.account_host
                .as_ref()
                .context("native recovery host required")?
                .provision_from_session(
                    crate::native_data_device::NativeDeviceKeyScope {
                        target_id: binding.target_id.clone(),
                        source_instance_id: original.instance_id.clone(),
                        source_public_identity: original.public_key.clone(),
                        account_epoch: binding.account_epoch,
                    },
                    session,
                    &connection,
                )
                .await?;
            current_account(self.host.as_ref(), request).await?;
            active.close().await?;
            *slot = None;
            return Ok(None);
        }
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
        Ok(Some(source))
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

/// An operation cannot return bytes or authorization from an expired transport,
/// even when its network future crosses the deadline or the wall clock moves.
async fn before_deadline<T>(
    deadline: Option<crate::native_transfer_accounts::NativeTransferSessionDeadline>,
    operation: impl Future<Output = Result<T>>,
) -> Result<T> {
    let Some(deadline) = deadline else {
        return operation.await;
    };
    let remaining = deadline
        .expires_at_ms
        .saturating_sub(chrono::Utc::now().timestamp_millis());
    ensure!(remaining > 0, "native transfer route expired");
    let result = tokio::time::timeout(Duration::from_millis(remaining as u64), operation)
        .await
        .context("native transfer route expired during operation")??;
    ensure!(
        chrono::Utc::now().timestamp_millis() < deadline.expires_at_ms,
        "native transfer route expired during operation"
    );
    Ok(result)
}

impl PeerRangeSource for NativeTransferPeerResolver {
    fn authorize<'a>(
        &'a self,
        request: &'a DownloadRequest,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let mut active = self.active.lock().await;
            let result = async {
                let source = self.resolve(&mut active, request).await?;
                before_deadline(
                    active.as_ref().and_then(|active| active.deadline),
                    source.authorize(request),
                )
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
                let source = self.resolve(&mut active, request).await?;
                before_deadline(
                    active.as_ref().and_then(|active| active.deadline),
                    source.read_range(request, offset, length),
                )
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
    async fn expired_route_does_not_poll_payload_or_authorization() {
        let polled = std::sync::atomic::AtomicBool::new(false);
        let deadline = crate::native_transfer_accounts::NativeTransferSessionDeadline {
            refresh_after_ms: 0,
            expires_at_ms: chrono::Utc::now().timestamp_millis(),
        };
        assert!(before_deadline(Some(deadline), async {
            polled.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(vec![1_u8])
        })
        .await
        .is_err());
        assert!(!polled.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn route_deadline_cancels_an_in_flight_range() {
        struct PendingRange(Arc<std::sync::atomic::AtomicBool>);
        impl Drop for PendingRange {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let pending = PendingRange(dropped.clone());
        let deadline = crate::native_transfer_accounts::NativeTransferSessionDeadline {
            refresh_after_ms: 0,
            expires_at_ms: chrono::Utc::now().timestamp_millis() + 20,
        };
        assert!(before_deadline(Some(deadline), async move {
            let _pending = pending;
            std::future::pending::<Result<Vec<u8>>>().await
        })
        .await
        .is_err());
        assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
    }

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
                storage: None,
                peer_source: None,
                sha256: "a".repeat(64),
                size: 1,
            },
            starting: Some(starting),
            session: None,
            source: None,
            deadline: None,
            bootstrap: false,
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
        // A shutdown wait expiring cannot report successful cleanup and allow
        // the caller to close the host database beneath native startup.
        assert!(active
            .close_with_timeout(Duration::from_millis(1))
            .await
            .is_err());
        assert!(active.starting.is_some());
        release.send(()).unwrap();
        active.close().await.unwrap();
        completion.await.unwrap();
        assert!(active.starting.is_none());
        active.close().await.unwrap();
    }
}
