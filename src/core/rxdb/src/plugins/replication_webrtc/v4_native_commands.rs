//! E25 adapter: reuse the registered native handlers, including their guards.
//! This module opens no socket and accepts no caller credential or peer identity.
use super::super::webrtc_types::WebRTCPublicationGuard;
use super::*;
use crate::rx_error::RxResult;
use ctox_store::change_log::subscription::write_contract::routes::{
    self, Completion, NativeCommand, ResultFiles,
};
use ctox_store::change_log::subscription::write_contract::CommandError;
use rusqlite::Transaction;

/// Implemented by the Core executor binding, NOT a network-supplied grant.
/// It must verify the persisted command, current actor == admitted token user,
/// app policy and actual P9 executor fence. Retain external execution authority
/// for the entire invocation; check it again in the completion writer job.
/// No permissive implementation or cached caller `clean` boolean is provided.
/// Core-only typed dispatcher. It is never installed as a V3 auxiliary wire
/// method, and receives the committed document rather than a caller envelope.
pub type NativeCommandExecutor = Arc<
    dyn Fn(
            String,
            String,
            NativeCommand,
            Arc<dyn NativeCommandAuthority>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Value, CommandError>> + Send>,
        > + Send
        + Sync,
>;

pub trait NativeCommandAuthority: Send + Sync + 'static {
    fn with_current(
        &self,
        command: &NativeCommand,
        peer_identity: &str,
        capability: &str,
        effect: &mut dyn FnMut() -> RxResult<()>,
    ) -> RxResult<()>;
    /// The actual P9 lease/fence must be checked on THIS completion transaction,
    /// never by opening a second DB or synchronously submitting to its writer.
    fn with_completion(
        &self,
        tx: &Transaction<'_>,
        command: &NativeCommand,
        peer_identity: &str,
        capability: &str,
        effect: &mut dyn FnMut() -> RxResult<()>,
    ) -> RxResult<()>;
}
struct BoundNativeAuthority {
    core: Arc<dyn NativeCommandAuthority>,
    peer: Arc<dyn WebRTCPublicationGuard>,
}
impl NativeCommandAuthority for BoundNativeAuthority {
    fn with_current(
        &self,
        command: &NativeCommand,
        identity: &str,
        capability: &str,
        effect: &mut dyn FnMut() -> RxResult<()>,
    ) -> RxResult<()> {
        self.core
            .with_current(command, identity, capability, &mut || {
                self.peer.with_current(effect)
            })
    }
    fn with_completion(
        &self,
        tx: &Transaction<'_>,
        command: &NativeCommand,
        identity: &str,
        capability: &str,
        effect: &mut dyn FnMut() -> RxResult<()>,
    ) -> RxResult<()> {
        self.core
            .with_completion(tx, command, identity, capability, &mut || {
                self.peer.with_current(effect)
            })
    }
}
impl<P> RegisteredAuxiliaryRequest<P> {
    /// E25 has a guarded command completion rather than V3's unguarded error
    /// response. Preserve typed errors, still redact all unknown free text.
    async fn command_answer(
        &self,
        peer: P,
        identity: String,
        token: String,
        params: Vec<Value>,
    ) -> Result<AuxiliaryAnswer, CommandError> {
        match &self.handler {
            AuxiliaryHandler::Plain(handler) => handler(identity, token, params)
                .await
                .map(|result| AuxiliaryAnswer {
                    result,
                    publication: None,
                })
                .map_err(|s| routes::native_error(&s)),
            AuxiliaryHandler::Guarded(handler) => handler(peer, token, params)
                .await
                .map(|answer| AuxiliaryAnswer {
                    result: answer.result,
                    publication: Some(answer.publication),
                })
                .map_err(|s| routes::native_error(&s)),
        }
    }
}
struct ExistingPeerAuthority;
impl WebRTCPublicationGuard for ExistingPeerAuthority {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        publish()
    }
}
/// Opaque result: the native value cannot be extracted without the retained
/// publication + executor checks. Consume inside the ONE writer transaction.
pub struct PendingNativeCompletion {
    pub(super) command: NativeCommand,
    pub(super) result: Result<Value, CommandError>,
    pub(super) publication: Arc<dyn WebRTCPublicationGuard>,
    pub(super) authority: Arc<dyn NativeCommandAuthority>,
    pub(super) peer_identity: String,
    pub(super) capability: String,
    pub(super) linked: Option<Box<PendingNativeCompletion>>,
    pub(super) canonical_replay: bool,
}
struct ResultFilesBatch(std::collections::HashMap<String, Arc<dyn ResultFiles>>);
impl ResultFiles for ResultFilesBatch {
    fn put_json(
        &self,
        tx: &Transaction<'_>,
        command: &NativeCommand,
        bytes: &[u8],
    ) -> Result<routes::FileRef, CommandError> {
        use sha2::Digest;
        let hash = format!("{:x}", sha2::Sha256::digest(bytes));
        self.0
            .get(&hash)
            .ok_or_else(|| routes::error("invalid", "files_batch_digest"))?
            .put_json(tx, command, bytes)
    }
}
impl PendingNativeCompletion {
    /// A control result and the opening session update can have different DTO
    /// bytes. Stage BOTH before entering the writer; each files row still shares
    /// the completion transaction. No CAS I/O is performed by publish().
    pub async fn stage_all_files<F, Fut>(
        &self,
        mut stage: F,
    ) -> Result<Option<Arc<dyn ResultFiles>>, CommandError>
    where
        F: FnMut(NativeCommand, Vec<u8>) -> Fut,
        Fut: std::future::Future<Output = Result<Arc<dyn ResultFiles>, CommandError>>,
    {
        use sha2::Digest;
        let mut sinks = std::collections::HashMap::new();
        let mut current = Some(self);
        while let Some(pending) = current {
            if let Ok(value) = &pending.result {
                if let Some(bytes) = routes::result_file_bytes(&pending.command, value.clone())? {
                    let hash = format!("{:x}", sha2::Sha256::digest(&bytes));
                    let sink = pending
                        .stage_files(|command, bytes| stage(command, bytes))
                        .await?
                        .ok_or_else(|| routes::error("invalid", "files_batch_missing"))?;
                    sinks.insert(hash, sink);
                }
            }
            current = pending.linked.as_deref();
        }
        if sinks.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Arc::new(ResultFilesBatch(sinks))))
        }
    }
    /// Stage verified result bytes through the Core's existing Transfer-CAS
    /// service, outside the writer. The returned adapter must only attach files
    /// metadata in publish(); it must not do file/network I/O on that writer.
    pub async fn stage_files<F, Fut>(
        &self,
        stage: F,
    ) -> Result<Option<Arc<dyn ResultFiles>>, CommandError>
    where
        F: FnOnce(NativeCommand, Vec<u8>) -> Fut,
        Fut: std::future::Future<Output = Result<Arc<dyn ResultFiles>, CommandError>>,
    {
        let Ok(value) = &self.result else {
            return Ok(None);
        };
        let Some(bytes) = routes::result_file_bytes(&self.command, value.clone())? else {
            return Ok(None);
        };
        let mut stage = Some(stage);
        let mut bytes = Some(bytes);
        let mut future = None;
        self.authority
            .with_current(
                &self.command,
                &self.peer_identity,
                &self.capability,
                &mut || {
                    self.publication.with_current(&mut || {
                        let call = stage
                            .take()
                            .ok_or_else(|| new_rx_error("E25_DUPLICATE_STAGE", None))?;
                        future = Some(call(self.command.clone(), bytes.take().unwrap()));
                        Ok(())
                    })
                },
            )
            .map_err(|_| routes::error("revoked", "result_staging_authority_changed"))?;
        let future =
            future.ok_or_else(|| routes::error("forbidden", "result_staging_not_admitted"))?;
        future.await.map(Some)
    }

    /// Caller persists this completion on the admitted command document using
    /// its ordinary P9 write operation, in this same existing writer transaction.
    /// Return Err from the writer job on ANY error so files metadata rolls back.
    pub fn publish(
        self,
        tx: &Transaction<'_>,
        files: Option<&dyn ResultFiles>,
        persist: &mut dyn FnMut(&Transaction<'_>, Completion) -> RxResult<()>,
    ) -> RxResult<()> {
        tx.execute_batch("SAVEPOINT e25_completion")
            .map_err(|_| new_rx_error("E25_TRANSACTION", None))?;
        let attempted = (|| {
            if self.canonical_replay {
                if let Ok(value) = &self.result {
                    if routes::is_canonical_completion(tx, &self.command, value.clone())
                        .map_err(|_| new_rx_error("E25_CANONICAL_REPLAY", None))?
                    {
                        return Ok(());
                    }
                }
            }
            let mut called = false;
            self.authority.with_completion(
                tx,
                &self.command,
                &self.peer_identity,
                &self.capability,
                &mut || {
                    self.publication.with_current(&mut || {
                        if called {
                            return Err(new_rx_error("E25_DUPLICATE_PUBLICATION", None));
                        }
                        called = true;
                        let completion = match &self.result {
                            Ok(value) => {
                                routes::completion(tx, &self.command, value.clone(), files)
                                    .map_err(|e| {
                                        new_rx_error(
                                            "E25_RESULT",
                                            Some(serde_json::to_value(e).unwrap()),
                                        )
                                    })?
                            }
                            Err(error) => {
                                Completion::rejected(&self.command.command_id, error.clone())
                            }
                        };
                        persist(tx, completion)
                    })
                },
            )?;
            if !called {
                return Err(new_rx_error("E25_PUBLICATION_NOT_RUN", None));
            }
            if let Some(linked) = self.linked {
                linked.publish(tx, files, persist)?;
            }
            Ok(())
        })();
        let finish = if attempted.is_ok() {
            "RELEASE e25_completion"
        } else {
            "ROLLBACK TO e25_completion; RELEASE e25_completion"
        };
        tx.execute_batch(finish)
            .map_err(|_| new_rx_error("E25_TRANSACTION", None))?;
        attempted
    }
}
impl<H: WebRTCConnectionHandler + 'static> RxWebRTCReplicationPool<H> {
    pub fn register_native_command_executor(
        &self,
        method: &str,
        executor: NativeCommandExecutor,
    ) -> RxResult<()> {
        if !routes::resolve(method).is_ok_and(routes::is_command_bus) {
            return Err(new_rx_error("E25_CORE_ROUTE", None));
        }
        self.native_command_executors
            .lock()
            .insert(method.into(), executor);
        Ok(())
    }
    /// Internal Core executor adapter, NOT a requestNative wire endpoint.
    /// Peer and its capability come from the existing admitted connection. The
    /// authority binding must retain the original command actor/fence across await.
    pub async fn execute_native_command(
        self: &Arc<Self>,
        peer: H::Peer,
        command: NativeCommand,
        authority: Arc<dyn NativeCommandAuthority>,
    ) -> Result<PendingNativeCompletion, CommandError> {
        let route = command.validate()?;
        if self.canceled.load(Ordering::SeqCst)
            || !self.authenticated_peers.lock().contains(&peer)
            || !self.connection_handler.is_peer_current(&peer)
        {
            return Err(routes::error("revoked", "peer_not_current"));
        }
        let capability = self
            .connection_handler
            .peer_capability_token(&peer)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| routes::error("forbidden", "peer_not_admitted"))?;
        let peer_identity = self.connection_handler.peer_identity(&peer);
        let call = self.session_call(&peer, &capability, &command)?;
        let params = call.params.clone();
        let cached = call.cached.clone();
        let started = std::time::Instant::now();
        let budget = std::time::Duration::from_millis(routes::timeout_ms(&command));
        let core_executor = self
            .native_command_executors
            .lock()
            .get(route.method)
            .cloned();
        if routes::is_command_bus(route) {
            let executor = core_executor
                .ok_or_else(|| routes::error("unavailable", "core_native_handler_missing"))?;
            let peer_guard: Arc<dyn WebRTCPublicationGuard> = Arc::new(AuxiliaryPublicationGuard {
                pool: Arc::downgrade(self),
                peer: peer.clone(),
                capability: Some(capability.clone()),
                native: Arc::new(ExistingPeerAuthority),
            });
            let scoped: Arc<dyn NativeCommandAuthority> = Arc::new(BoundNativeAuthority {
                core: authority.clone(),
                peer: peer_guard.clone(),
            });
            let mut admitted = None;
            authority
                .with_current(&command, &peer_identity, &capability, &mut || {
                    if admitted.is_some() {
                        return Err(new_rx_error("E25_DUPLICATE_START", None));
                    }
                    admitted = Some(executor(
                        peer_identity.clone(),
                        capability.clone(),
                        command.clone(),
                        scoped.clone(),
                    ));
                    Ok(())
                })
                .map_err(|_| routes::error("forbidden", "command_authority_changed"))?;
            let native = tokio::time::timeout(
                budget,
                admitted.ok_or_else(|| routes::error("forbidden", "command_not_admitted"))?,
            )
            .await
            .map_err(|_| routes::error("expired", "native_request_timeout"))?;
            let publication = Arc::new(AuxiliaryPublicationGuard {
                pool: Arc::downgrade(self),
                peer,
                capability: Some(capability.clone()),
                native: Arc::new(ExistingPeerAuthority),
            });
            return Ok(PendingNativeCompletion {
                command,
                result: native,
                publication,
                authority,
                peer_identity,
                capability,
                linked: None,
                canonical_replay: false,
            });
        }
        let installed = self
            .auxiliary_request_handlers
            .lock()
            .get(route.method)
            .cloned()
            .ok_or_else(|| routes::error("unavailable", "native_handler_missing"))?;
        // E25 may only adapt user-scoped handlers. Public identity and native
        // control routes would substitute authority and are never eligible.
        if installed.public_identity || installed.native_control {
            return Err(routes::error("forbidden", "native_route_not_user_scoped"));
        }
        let mut future = None;
        authority
            .with_current(&command, &peer_identity, &capability, &mut || {
                if future.is_some() {
                    return Err(new_rx_error("E25_DUPLICATE_START", None));
                }
                future = Some(installed.command_answer(
                    peer.clone(),
                    peer_identity.clone(),
                    capability.clone(),
                    vec![params.clone()],
                ));
                Ok(())
            })
            .map_err(|_| routes::error("forbidden", "command_authority_changed"))?;
        let future = future.ok_or_else(|| routes::error("forbidden", "command_not_admitted"))?;
        let answer = if let Some(value) = cached {
            Ok(AuxiliaryAnswer {
                result: value,
                publication: None,
            })
        } else {
            tokio::time::timeout(budget, future)
                .await
                .map_err(|_| routes::error("expired", "native_request_timeout"))?
        };
        let (result, native) = match answer {
            Ok(answer) => (
                Ok(answer.result),
                answer
                    .publication
                    .unwrap_or_else(|| Arc::new(ExistingPeerAuthority)),
            ),
            // Existing handlers promise sanitized public errors, but may still
            // emit free text. Never forward arbitrary provider/token material.
            Err(error) => (
                Err(error),
                Arc::new(ExistingPeerAuthority) as Arc<dyn WebRTCPublicationGuard>,
            ),
        };
        let publication = Arc::new(AuxiliaryPublicationGuard {
            pool: Arc::downgrade(self),
            peer: peer.clone(),
            capability: Some(capability.clone()),
            native,
        });
        let mut pending = PendingNativeCompletion {
            command,
            result,
            publication,
            authority,
            peer_identity,
            capability,
            linked: None,
            canonical_replay: false,
        };
        self.attach_session_result(
            peer,
            call,
            &mut pending,
            budget.saturating_sub(started.elapsed()),
        )
        .await?;
        Ok(pending)
    }
}
