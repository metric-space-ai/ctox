// Origin: CTOX
// License: AGPL-3.0-only
//! Bounded checkpoint transport and explicit native ownership controls.
use super::*;
use ctox_sync::checkpoint::{artifacts, CheckpointStore};
use ctox_sync::contracts::{ArtifactRef, CheckpointManifest};
use std::io::{Read, Seek, SeekFrom};
const CHUNK: usize = 8192;
const MANIFEST_LIMIT: u64 = 8 * 1024 * 1024;
const BLOB_LIMIT: u64 = 64 * 1024 * 1024;
#[path = "session_handoff_guest_enrollment.rs"]
mod guest_enrollment;
#[path = "session_handoff_guest_import.rs"]
mod guest_import;
#[path = "session_handoff_copy_progress.rs"]
mod progress;
#[path = "session_handoff_checkpoint_quorum.rs"]
mod quorum;
#[path = "session_handoff_reconstruction.rs"]
mod reconstruction;
#[path = "session_handoff_takeover.rs"]
mod takeover;

/// Resolve the native Core credential source in the assigned workspace. The
/// returned manager pins the actual account, not a client account label. It
/// rechecks its storage on every guarded callback; external file mutations
/// remain separate from the in-process auth fence.
fn account(
    root: &Path,
    request: &SessionHandoffGateRequest,
) -> anyhow::Result<Arc<ctox_core::AuthManager>> {
    let conn = Connection::open_with_flags(
        business_os_store_path(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let (owner, profile, project): (String, String, String) = if request.phase
        == SessionHandoffPhase::Disclose
    {
        let json: String = conn.query_row("SELECT n.source_json FROM business_native_source_handoff_bindings n
            JOIN business_session_handoff_bindings b ON b.binding_id=n.binding_id WHERE b.binding_digest=?1",
            [&request.binding_digest], |r| r.get(0))?;
        let facts: crate::business_os::guest_registry::source_handoff::SourceHandoffFacts =
            serde_json::from_str(&json)?;
        (
            facts.owner_user_id,
            facts.worker_profile_id,
            facts.project_id,
        )
    } else {
        let json: String = conn.query_row("SELECT n.target_scope_json FROM business_native_target_handoff_bindings n
            JOIN business_session_handoff_bindings b ON b.binding_id=n.binding_id WHERE b.binding_digest=?1",
            [&request.binding_digest], |r| r.get(0))?;
        let scope: serde_json::Value = serde_json::from_str(&json)?;
        (
            scope["ownerUserId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("owner absent"))?
                .into(),
            scope["workerProfileId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("profile absent"))?
                .into(),
            scope["projectId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("project absent"))?
                .into(),
        )
    };
    let cwd: String = conn.query_row(
        "SELECT native_workspace FROM business_native_guest_workspace_assignments
        WHERE owner_user_id=?1 AND worker_profile_id=?2 AND project_id=?3 AND state='active'",
        params![owner, profile, project],
        |r| r.get(0),
    )?;
    drop(conn);
    let home = ctox_core::config::find_codex_home()?;
    let cwd = ctox_utils_absolute_path::AbsolutePathBuf::from_absolute_path(PathBuf::from(cwd))?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let config = rt.block_on(ctox_core::config::load_config_as_toml_with_cli_overrides(
        &home,
        &cwd,
        vec![],
    ))?;
    anyhow::ensure!(
        config
            .chatgpt_base_url
            .as_deref()
            .is_none_or(|u| u.trim_end_matches('/') == "https://chatgpt.com/backend-api"),
        "native handoff provider endpoint changed"
    );
    let auth = ctox_core::AuthManager::from_account_bound_storage(
        home,
        config.cli_auth_credentials_store.unwrap_or_default(),
    )?;
    anyhow::ensure!(
        request.spec.model_route_id == "openai"
            && request.spec.harness == ctox_core::native_harness_name()
            && request.spec.harness_version == ctox_core::native_harness_version()
            && auth.runtime_account_binding() == Some(request.spec.gateway_account_id.as_str()),
        "native handoff account changed"
    );
    Ok(auth)
}
fn source_store(
    root: &Path,
    conn: &Connection,
    request: &SessionHandoffGateRequest,
) -> anyhow::Result<CheckpointStore> {
    let path: String = conn.query_row("SELECT j.artifact_store_path FROM business_native_source_journals j
        JOIN business_native_source_handoff_bindings n ON json_extract(n.source_json,'$.captureId')=j.capture_id
        JOIN business_session_handoff_bindings b ON b.binding_id=n.binding_id WHERE b.binding_digest=?1",
        [&request.binding_digest], |r|r.get(0))?;
    let path = PathBuf::from(path);
    anyhow::ensure!(
        path.starts_with(root)
            && path.file_name().is_some_and(|n| n == "source-journals")
            && std::fs::canonicalize(&path)? == path,
        "foreign checkpoint store"
    );
    private_dir(&path)?;
    CheckpointStore::open(path, BLOB_LIMIT).map_err(Into::into)
}
fn private_dir(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let m = std::fs::symlink_metadata(path)?;
    anyhow::ensure!(
        m.is_dir()
            && !m.file_type().is_symlink()
            && m.uid() == unsafe { libc::geteuid() }
            && m.permissions().mode() & 0o077 == 0,
        "checkpoint directory is not private"
    );
    Ok(())
}
fn create_private(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    match std::fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    private_dir(path)
}
fn read_chunk(
    store: &CheckpointStore,
    request: &SessionHandoffGateRequest,
    artifact: &Option<ArtifactRef>,
    offset: u64,
) -> anyhow::Result<(u64, String)> {
    let manifest = store.load_manifest(&request.checkpoint_digest)?;
    anyhow::ensure!(
        manifest.sequence == request.checkpoint_sequence,
        "checkpoint sequence changed"
    );
    let (size, bytes) = if let Some(a) = artifact {
        anyhow::ensure!(
            artifacts(&manifest).any(|r| r == a),
            "artifact is not in this checkpoint"
        );
        anyhow::ensure!(offset <= a.size_bytes, "checkpoint offset out of range");
        let bytes = store.read_blob_range(&request.checkpoint_digest, a, offset, CHUNK)?;
        (a.size_bytes, bytes)
    } else {
        let bytes = serde_json::to_vec(&manifest)?;
        let size = bytes.len() as u64;
        anyhow::ensure!(
            size <= MANIFEST_LIMIT && offset <= size,
            "manifest range out of bounds"
        );
        (
            size,
            bytes[offset as usize..(offset as usize + CHUNK).min(bytes.len())].to_vec(),
        )
    };
    Ok((size, bytes.iter().map(|b| format!("{b:02x}")).collect()))
}
fn verify_receive(
    verified: &VerifiedHandoffRequest,
    permit: &SessionHandoffPermit,
) -> anyhow::Result<()> {
    let r = verified.request();
    ctox_sync::authority::auth::session_handoff::verify_fresh_session_handoff_permit(
        permit,
        verified.sender(),
        &r.audience,
        &r.nonce,
        now_ms() as u64,
    )?;
    anyhow::ensure!(
        permit.phase == SessionHandoffPhase::Receive
            && permit.binding_digest == r.binding_digest
            && permit.job_id == r.spec.job_id
            && permit.session_id == r.spec.session_id
            && permit.scope_id == r.spec.scope_id
            && permit.checkpoint_digest == r.checkpoint_digest
            && permit.checkpoint_sequence == r.checkpoint_sequence
            && permit.ownership_generation == r.ownership.generation,
        "target receive decision differs"
    );
    Ok(())
}

pub(super) fn fetch<P: Clone + Eq + Hash + Send + Sync + 'static>(
    server: Arc<Server<P>>,
    peer: P,
    verified: Arc<VerifiedHandoffRequest>,
) -> Result<GuardedAuxiliaryResponse, SessionHandoffDenial> {
    let SessionHandoffWireRequest::Fetch {
        request,
        receive_permit,
        ..
    } = verified.message()
    else {
        return Err(deny("invalid_request"));
    };
    if request.phase != SessionHandoffPhase::Disclose {
        return Err(deny("wrong_phase"));
    }
    verify_receive(&verified, receive_permit).map_err(|_| deny("target_receive_unavailable"))?;
    let auth = account(&server.gate.root, request).map_err(|_| deny("provider_unavailable"))?;
    fetch_with_account(server, peer, verified, auth)
}
fn fetch_with_account<P: Clone + Eq + Hash + Send + Sync + 'static>(
    server: Arc<Server<P>>,
    peer: P,
    verified: Arc<VerifiedHandoffRequest>,
    auth: Arc<ctox_core::AuthManager>,
) -> Result<GuardedAuxiliaryResponse, SessionHandoffDenial> {
    let SessionHandoffWireRequest::Fetch {
        request,
        challenge,
        receive_permit,
        artifact,
        offset,
    } = verified.message()
    else {
        return Err(deny("invalid_request"));
    };
    if request.phase != SessionHandoffPhase::Disclose
        || auth.runtime_account_binding() != Some(request.spec.gateway_account_id.as_str())
    {
        return Err(deny("provider_unavailable"));
    }
    verify_receive(&verified, receive_permit).map_err(|_| deny("target_receive_unavailable"))?;
    let _account = auth
        .current_runtime_account_guard()
        .map_err(|_| deny("provider_unavailable"))?;
    let (bound, result) = server.gate.with_current_authority(|conn, identity| {
        let permit = server.authorize_peer(conn, identity, &verified, true)?;
        let mut ledger = server.lock_ledger()?;
        if !ledger.alive {
            return Err(deny("host_retired"));
        }
        let bound = ledger
            .pending
            .get_mut(&peer)
            .ok_or_else(|| deny("challenge_unknown"))?;
        if bound.used
            || bound.nonce != *challenge
            || bound.request != *request
            || bound.sender != verified.sender()
            || !currency_matches(&bound.permit, &permit)
        {
            return Err(deny("challenge_changed"));
        }
        bound.used = true;
        let store = source_store(&server.gate.root, conn, request)
            .map_err(|_| deny("source_store_unavailable"))?;
        let (size_bytes, hex) = read_chunk(&store, request, artifact, *offset)
            .map_err(|_| deny("checkpoint_read_failed"))?;
        let result = verified
            .reply(
                identity,
                SessionHandoffWireReply::Chunk {
                    permit,
                    artifact: artifact.clone(),
                    offset: *offset,
                    size_bytes,
                    hex,
                },
            )
            .map_err(|_| deny("reply_signing_failed"))?;
        Ok((bound.clone(), result))
    })?;
    let receive = receive_permit.clone();
    drop(_account);
    Ok(GuardedAuxiliaryResponse {
        result,
        publication: Arc::new(CheckpointPublication {
            base: Publication {
                server,
                peer,
                verified,
                challenge: bound,
                authorized: true,
            },
            auth,
            receive,
        }),
    })
}
struct CheckpointPublication<P> {
    base: Publication<P>,
    auth: Arc<ctox_core::AuthManager>,
    receive: SessionHandoffPermit,
}
impl<P: Clone + Eq + Hash + Send + Sync + 'static> WebRTCPublicationGuard
    for CheckpointPublication<P>
{
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        let _account = self
            .auth
            .current_runtime_account_guard()
            .map_err(|_| new_rx_error("RC_WEBRTC_CONTROL", None))?;
        verify_receive(&self.base.verified, &self.receive)
            .map_err(|_| new_rx_error("RC_WEBRTC_CONTROL", None))?;
        self.base.with_current(publish)
    }
}

type PeerGuard = Arc<dyn Fn(&mut dyn FnMut() -> RxResult<()>) -> RxResult<()> + Send + Sync>;
struct CopyLifetime(Arc<Mutex<bool>>);
impl Drop for CopyLifetime {
    fn drop(&mut self) {
        if let Ok(mut live) = self.0.lock() {
            *live = false;
        }
    }
}
struct Target<P> {
    server: Arc<Server<P>>,
    request: SessionHandoffGateRequest,
    source_identity: String,
    original: SessionHandoffPermit,
    auth: Arc<ctox_core::AuthManager>,
    peer_guard: Option<PeerGuard>,
    live: Arc<Mutex<bool>>,
}
impl<P: Clone + Eq + Hash + Send + Sync + 'static> Target<P> {
    fn prepare(
        server: Arc<Server<P>>,
        binding: &str,
        peer_guard: Option<PeerGuard>,
        live: Arc<Mutex<bool>>,
        phase: SessionHandoffPhase,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            peer_guard.is_some() || phase == SessionHandoffPhase::Resume,
            "checkpoint receive requires a native peer fence"
        );
        let request=server.gate.with_current_authority(|conn,identity|{
            let json:String=conn.query_row("SELECT n.source_offer_json FROM business_native_target_handoff_bindings n
                JOIN business_session_handoff_bindings b ON b.binding_id=n.binding_id WHERE b.binding_digest=?1",
                [binding],|r|r.get(0)).map_err(|_|deny("binding_unknown"))?;
            let offer:crate::business_os::session_handoff_enrollment::target::SourceOffer=
                serde_json::from_str(&json).map_err(|_|deny("binding_unknown"))?;
            let mut request=crate::business_os::session_handoff_enrollment::target::offer_request(&offer.body)
                .map_err(|_|deny("binding_unknown"))?;
            request.phase=phase;
            request.issuer_identity=identity.public_identity();
            request.nonce=fresh_nonce().map_err(|_|deny("nonce_generation_failed"))?;
            server.gate.authorize_fenced(conn,identity,&request)?;
            Ok((request,offer.body.source_identity))
        })?;
        let auth = account(&server.gate.root, &request.0)?;
        let _account = auth.current_runtime_account_guard()?;
        let original = server.gate.with_current_authority(|conn, identity| {
            server.gate.authorize_fenced(conn, identity, &request.0)
        })?;
        drop(_account);
        Ok(Self {
            server,
            request: request.0,
            source_identity: request.1,
            original,
            auth,
            peer_guard,
            live,
        })
    }
    fn current<T>(
        &self,
        request: &SessionHandoffGateRequest,
        action: impl FnOnce(&SessionHandoffPermit, &SigningIdentity) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        self.current_policy(request, |permit, identity, _| action(permit, identity))
    }
    fn current_policy<T>(
        &self,
        request: &SessionHandoffGateRequest,
        action: impl FnOnce(&SessionHandoffPermit, &SigningIdentity, &Connection) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let _account = self.auth.current_runtime_account_guard()?;
        let mut result = None;
        self.server.gate.with_current_authority(|conn, identity| {
            let permit = self.server.gate.resolve_fenced(conn, identity, request)?;
            if !operation_authority_matches(&self.original, &permit) {
                return Err(deny("target_authority_changed"));
            }
            if self.request.phase == SessionHandoffPhase::Resume {
                let mut receive = request.clone();
                receive.phase = SessionHandoffPhase::Receive;
                let permit = self.server.gate.resolve_fenced(conn, identity, &receive)?;
                if !operation_authority_matches(&self.original, &permit) {
                    return Err(deny("target_authority_changed"));
                }
            }
            let ledger = self.server.lock_ledger()?;
            if !ledger.alive {
                return Err(deny("host_retired"));
            }
            let live = self.live.lock().map_err(|_| deny("copy_retired"))?;
            if !*live {
                return Err(deny("copy_retired"));
            }
            let mut action = Some(action);
            let mut publish = || {
                result = Some(action
                    .take()
                    .ok_or_else(|| new_rx_error("RC_WEBRTC_CONTROL", None))?(
                    &permit, identity, conn,
                ));
                Ok(())
            };
            if let Some(guard) = &self.peer_guard {
                guard(&mut publish).map_err(|_| deny("peer_unavailable"))?;
            } else {
                if self.request.phase != SessionHandoffPhase::Resume {
                    return Err(deny("peer_unavailable"));
                }
                publish().map_err(|_| deny("target_publication_failed"))?;
            }
            Ok(())
        })?;
        result.ok_or_else(|| anyhow::anyhow!("target publication absent"))?
    }
    fn sign(
        &self,
        message: SessionHandoffWireRequest,
        local: &SessionHandoffGateRequest,
    ) -> anyhow::Result<ctox_sync::authority::auth::handoff_wire::SignedHandoffRequest> {
        self.current(local, |_, identity| {
            Ok(
                ctox_sync::authority::auth::handoff_wire::SignedHandoffRequest::new(
                    identity, message,
                )?,
            )
        })
    }
}
async fn exchange<H: WebRTCConnectionHandler + 'static>(
    pool: &Arc<RxWebRTCReplicationPool<H>>,
    peer: &H::Peer,
    signed: &ctox_sync::authority::auth::handoff_wire::SignedHandoffRequest,
) -> anyhow::Result<SessionHandoffWireReply> {
    use rxdb::plugins::replication_webrtc::{send_message_and_await_answer, WebRTCMessage};
    anyhow::ensure!(
        pool.is_peer_ready_for_control(peer),
        "checkpoint peer unavailable"
    );
    let reply = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        send_message_and_await_answer(
            pool.connection_handler.clone(),
            peer.clone(),
            WebRTCMessage {
                id: format!("handoff:{}", signed.nonce()),
                method: ctox_sync::contracts::CTOX_SYNC_SESSION_HANDOFF_METHOD.into(),
                params: vec![signed.envelope.clone()],
                collection: None,
            },
        ),
    )
    .await??;
    anyhow::ensure!(
        reply.error.is_none() && pool.is_peer_ready_for_control(peer),
        "checkpoint exchange failed"
    );
    Ok(signed.verify_reply(reply.result, now_ms() as u64)?)
}
enum Part {
    Complete(Vec<u8>),
    Pending(u64),
}
async fn part<H: WebRTCConnectionHandler + 'static>(
    target: &Arc<Target<H::Peer>>,
    pool: &Arc<RxWebRTCReplicationPool<H>>,
    peer: &H::Peer,
    store: &Arc<CheckpointStore>,
    staging: &Path,
    artifact: Option<ArtifactRef>,
    deadline: tokio::time::Instant,
) -> anyhow::Result<Part> {
    let t = target.clone();
    let directory = staging.to_path_buf();
    let a = artifact.clone();
    let mut file = tokio::task::spawn_blocking(move || {
        t.current(&t.request, |_, _| {
            progress::StagedPart::open(&directory, a.as_ref())
        })
    })
    .await??;
    let mut offset = file.offset;
    let mut expected_size = None;
    loop {
        if tokio::time::Instant::now() >= deadline {
            return Ok(Part::Pending(offset));
        }
        let mut local = target.request.clone();
        local.nonce = fresh_nonce()?;
        let mut remote = local.clone();
        remote.phase = SessionHandoffPhase::Disclose;
        remote.issuer_identity = target.source_identity.clone();
        let t = target.clone();
        let r = remote.clone();
        let l = local.clone();
        let probe = tokio::task::spawn_blocking(move || {
            t.sign(SessionHandoffWireRequest::Probe { request: r }, &l)
        })
        .await??;
        let reply = match tokio::time::timeout_at(deadline, exchange(pool, peer, &probe)).await {
            Ok(reply) => reply?,
            Err(_) => return Ok(Part::Pending(offset)),
        };
        let SessionHandoffWireReply::Challenge { challenge } = reply else {
            anyhow::bail!("checkpoint challenge absent");
        };
        let t = target.clone();
        let l = local.clone();
        let r = remote;
        let a = artifact.clone();
        let fetch = tokio::task::spawn_blocking(move || {
            t.current(&l, |permit, identity| {
                Ok(
                    ctox_sync::authority::auth::handoff_wire::SignedHandoffRequest::new(
                        identity,
                        SessionHandoffWireRequest::Fetch {
                            request: r,
                            challenge,
                            receive_permit: permit.clone(),
                            artifact: a,
                            offset,
                        },
                    )?,
                )
            })
        })
        .await??;
        let reply = match tokio::time::timeout_at(deadline, exchange(pool, peer, &fetch)).await {
            Ok(reply) => reply?,
            Err(_) => return Ok(Part::Pending(offset)),
        };
        let SessionHandoffWireReply::Chunk {
            size_bytes, hex, ..
        } = reply
        else {
            anyhow::bail!("checkpoint chunk absent");
        };
        anyhow::ensure!(
            size_bytes <= artifact.as_ref().map_or(MANIFEST_LIMIT, |a| a.size_bytes)
                && artifact.as_ref().is_none_or(|a| size_bytes == a.size_bytes)
                && size_bytes <= BLOB_LIMIT
                && expected_size.is_none_or(|n| n == size_bytes),
            "checkpoint length changed"
        );
        expected_size = Some(size_bytes);
        let bytes: Vec<u8> = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|s| {
                let n = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
                (n(s[0]) << 4) | n(s[1])
            })
            .collect();
        let count = bytes.len() as u64;
        anyhow::ensure!(
            count > 0 || offset == size_bytes,
            "checkpoint range made no progress"
        );
        let t = target.clone();
        file = tokio::task::spawn_blocking(move || {
            t.current(&local, |_, _| {
                file.append(&bytes, size_bytes)?;
                Ok(())
            })?;
            Ok::<_, anyhow::Error>(file)
        })
        .await??;
        offset += count;
        if offset == size_bytes {
            break;
        }
    }
    let t = target.clone();
    let store = store.clone();
    tokio::task::spawn_blocking(move || {
        t.current(&t.request, |_, _| {
            file.finish(&store, artifact.as_ref(), &t.request.checkpoint_digest)
                .map(Part::Complete)
        })
    })
    .await?
}
async fn copy<H: WebRTCConnectionHandler + 'static>(
    server: Arc<Server<H::Peer>>,
    pool: Arc<RxWebRTCReplicationPool<H>>,
    peer: H::Peer,
    binding: String,
) -> anyhow::Result<CopyResponse> {
    // Leave time for current-authority publication before the 60s operator
    // bound. Every network exchange is also bounded by this deadline.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(50);
    let lifetime = CopyLifetime(Arc::new(Mutex::new(true)));
    let live = lifetime.0.clone();
    let guard_pool = pool.clone();
    let guard_peer = peer.clone();
    let peer_guard = Arc::new(move |apply: &mut dyn FnMut() -> RxResult<()>| {
        guard_pool.with_current_native_control_peer(&guard_peer, apply)?
    });
    let target = Arc::new(
        tokio::task::spawn_blocking(move || {
            Target::prepare(
                server,
                &binding,
                Some(peer_guard),
                live,
                SessionHandoffPhase::Receive,
            )
        })
        .await??,
    );
    let t = target.clone();
    let (store, staging) = tokio::task::spawn_blocking(move || {
        t.current(&t.request, |_, _| {
            let parent = t.server.gate.root.join("runtime/ctox-sync");
            private_dir(&parent)?;
            let root = parent.join("received-checkpoints");
            create_private(&root)?;
            create_private(&root.join("blobs"))?;
            create_private(&root.join("manifests"))?;
            let progress = root.join("copy-progress");
            create_private(&progress)?;
            let binding = progress.join(&t.request.binding_digest);
            create_private(&binding)?;
            let staging = binding.join(&t.request.checkpoint_digest);
            create_private(&staging)?;
            // Pending progress must retain newly created directory entries as
            // well as the fsynced range file itself.
            for directory in [&parent, &root, &progress, &binding, &staging] {
                std::fs::File::open(directory)?.sync_all()?;
            }
            let store = Arc::new(CheckpointStore::open(root, BLOB_LIMIT)?);
            Ok((store, staging))
        })
    })
    .await??;
    let bytes = match part(&target, &pool, &peer, &store, &staging, None, deadline).await? {
        Part::Complete(bytes) => bytes,
        Part::Pending(offset) => return copy_pending(&target, 0, offset, None).await,
    };
    use sha2::{Digest, Sha256};
    anyhow::ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == target.request.checkpoint_digest,
        "checkpoint manifest hash mismatch"
    );
    let manifest: CheckpointManifest = serde_json::from_slice(&bytes)?;
    ctox_sync::checkpoint::validate_manifest(&manifest)?;
    anyhow::ensure!(
        manifest.sequence == target.request.checkpoint_sequence
            && manifest.session.session_id == target.request.spec.session_id
            && manifest.session.scope_id == target.request.spec.scope_id
            && manifest.session.harness == target.request.spec.harness
            && manifest.session.harness_version == target.request.spec.harness_version
            && manifest.session.gateway_account_id == target.request.spec.gateway_account_id
            && manifest.session.model_route_id == target.request.spec.model_route_id
            && manifest.session.model_id == target.request.spec.model_id
            && manifest.session.required_capabilities == target.request.spec.required_capabilities,
        "checkpoint manifest differs from enrollment"
    );
    let mut seen = std::collections::BTreeSet::new();
    let mut total = bytes.len() as u64;
    let mut unique = Vec::new();
    for artifact in artifacts(&manifest) {
        anyhow::ensure!(
            artifact.size_bytes <= BLOB_LIMIT,
            "checkpoint artifact too large"
        );
        if seen.insert(artifact.sha256.clone()) {
            total = total
                .checked_add(artifact.size_bytes)
                .ok_or_else(|| anyhow::anyhow!("checkpoint size overflow"))?;
            anyhow::ensure!(
                total <= 1024 * 1024 * 1024 && seen.len() <= 4096,
                "checkpoint exceeds receive budget"
            );
            unique.push(artifact.clone());
        }
    }
    let mut verified_bytes = bytes.len() as u64;
    for artifact in unique {
        let t = target.clone();
        let s = store.clone();
        let a = artifact.clone();
        let present = tokio::task::spawn_blocking(move || {
            t.current(&t.request, |_, _| progress::verified(&s, &a))
        })
        .await??;
        if !present {
            match part(
                &target,
                &pool,
                &peer,
                &store,
                &staging,
                Some(artifact.clone()),
                deadline,
            )
            .await?
            {
                Part::Complete(_) => (),
                Part::Pending(offset) => {
                    return copy_pending(&target, verified_bytes, offset, Some(total)).await
                }
            }
        }
        verified_bytes += artifact.size_bytes;
    }
    let t = target.clone();
    tokio::task::spawn_blocking(move || {
        t.current(&t.request, |_, _| {
            let digest = store.publish(&manifest)?;
            anyhow::ensure!(
                digest == t.request.checkpoint_digest,
                "checkpoint digest changed"
            );
            store.verify_durable_copy(&digest)?;
            // This confirms only this local immutable copy. It creates neither a
            // Raft DATA receipt nor clean-effect/ownership/Core resume evidence.
            Ok(CopyResponse::Copied {
                checkpoint_digest: digest,
            })
        })
    })
    .await?
}

async fn copy_pending<P: Clone + Eq + Hash + Send + Sync + 'static>(
    target: &Arc<Target<P>>,
    verified_bytes: u64,
    partial_bytes: u64,
    total_bytes: Option<u64>,
) -> anyhow::Result<CopyResponse> {
    let t = target.clone();
    tokio::task::spawn_blocking(move || {
        t.current(&t.request, |_, _| {
            Ok(CopyResponse::CopyPending {
                checkpoint_digest: t.request.checkpoint_digest.clone(),
                verified_bytes,
                partial_bytes,
                total_bytes,
            })
        })
    })
    .await?
}
#[cfg(test)]
pub(crate) fn assert_native_checkpoint_path(
    source_root: &Path,
    target_root: &Path,
    receive: &SessionHandoffGateRequest,
) {
    use base64::Engine;
    use ctox_core::auth::{AuthCredentialsStoreMode, AuthDotJson};
    use ctox_sync::authority::auth::handoff_wire::SignedHandoffRequest;
    let source_key = crate::sync_host::signing_identity(source_root).unwrap();
    let target_key = crate::sync_host::signing_identity(target_root).unwrap();
    let mut local = receive.clone();
    local.nonce = fresh_nonce().unwrap();
    let mut remote = local.clone();
    remote.phase = SessionHandoffPhase::Disclose;
    remote.issuer_identity = source_key.public_identity();
    let make_server = |root: &Path, key: &SigningIdentity| {
        Arc::new(Server {
            gate: NativeSessionHandoffGate {
                root: root.into(),
                issuer_identity: key.public_identity(),
                permit_ttl_ms: PERMIT_TTL_MS,
            },
            scope: remote.audience.clone(),
            ledger: Mutex::new(Ledger {
                alive: true,
                pending: HashMap::new(),
            }),
        })
    };
    let server: Arc<Server<(&str, u64)>> = make_server(source_root, &source_key);
    let native_target: Arc<Server<(&str, u64)>> = make_server(target_root, &target_key);
    let home = tempfile::tempdir().unwrap();
    let jwt=format!("e30.{}.fixture",base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&serde_json::json!({"https://api.openai.com/auth":{"chatgpt_account_id":local.spec.gateway_account_id}})).unwrap()));
    let credentials:AuthDotJson=serde_json::from_value(serde_json::json!({
        "auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{"id_token":jwt,
        "access_token":"test-placeholder","refresh_token":"test-placeholder","account_id":local.spec.gateway_account_id},
        "last_refresh":"2026-10-07T00:00:00Z"
    })).unwrap();
    ctox_core::auth::save_auth(home.path(), &credentials, AuthCredentialsStoreMode::File).unwrap();
    let auth = ctox_core::AuthManager::from_account_bound_storage(
        home.path().into(),
        AuthCredentialsStoreMode::File,
    )
    .unwrap();
    let mut original = native_target.gate.authorize(&local).unwrap();
    // Deterministically exercise a copy outliving its initial local witness.
    // Actual wire challenges/receive permits are freshly signed per chunk.
    original.expires_at_ms = 0;
    let live = Arc::new(Mutex::new(true));
    let target = Target {
        server: native_target,
        request: local.clone(),
        source_identity: source_key.public_identity(),
        original,
        auth: auth.clone(),
        peer_guard: Some(Arc::new(|apply| apply())),
        live: live.clone(),
    };
    let peer = ("exact-peer", 1);
    let make_fetch = |artifact: Option<ArtifactRef>, offset: u64| {
        let probe = SignedHandoffRequest::new(
            &target_key,
            SessionHandoffWireRequest::Probe {
                request: remote.clone(),
            },
        )
        .unwrap();
        let response = server
            .clone()
            .answer(peer, vec![probe.envelope.clone()])
            .unwrap();
        response.publication.with_current(&mut || Ok(())).unwrap();
        let SessionHandoffWireReply::Challenge { challenge } = probe
            .verify_reply(response.result, now_ms() as u64)
            .unwrap()
        else {
            panic!("probe")
        };
        let permit = target.current(&local, |p, _| Ok(p.clone())).unwrap();
        let sent = SignedHandoffRequest::new(
            &target_key,
            SessionHandoffWireRequest::Fetch {
                request: remote.clone(),
                challenge,
                receive_permit: permit,
                artifact,
                offset,
            },
        )
        .unwrap();
        let verified = Arc::new(
            verify_request(
                sent.envelope.clone(),
                &source_key.public_identity(),
                &remote.audience,
            )
            .unwrap(),
        );
        (sent, verified)
    };
    let (sent, verified) = make_fetch(None, 0);
    assert!(fetch_with_account(
        server.clone(),
        ("exact-peer", 2),
        verified.clone(),
        auth.clone()
    )
    .is_err());
    let prepared =
        fetch_with_account(server.clone(), peer, verified.clone(), auth.clone()).unwrap();
    prepared.publication.with_current(&mut || Ok(())).unwrap();
    let SessionHandoffWireReply::Chunk {
        hex, size_bytes, ..
    } = sent
        .verify_reply(prepared.result.clone(), now_ms() as u64)
        .unwrap()
    else {
        panic!("chunk")
    };
    assert!(!hex.is_empty() && hex.len() <= CHUNK * 2 && size_bytes > 0);
    assert!(
        fetch_with_account(server.clone(), peer, verified, auth.clone()).is_err(),
        "one-use request"
    );
    // Exercise every protected byte from the actual stopped Core/Git capture,
    // not only the manifest response. This is a native store-path regression;
    // the peer publication callback remains controlled, not network acceptance.
    let copied = tempfile::tempdir().unwrap();
    let received = Arc::new(CheckpointStore::open(copied.path().into(), BLOB_LIMIT).unwrap());
    let copy_started = std::time::Instant::now();
    let read_part = |artifact: Option<ArtifactRef>| {
        let part_started = std::time::Instant::now();
        let mut chunks = 0usize;
        let mut bytes = Vec::new();
        loop {
            let (sent, verified) = make_fetch(artifact.clone(), bytes.len() as u64);
            let response =
                fetch_with_account(server.clone(), peer, verified, auth.clone()).unwrap();
            response.publication.with_current(&mut || Ok(())).unwrap();
            let SessionHandoffWireReply::Chunk {
                size_bytes, hex, ..
            } = sent.verify_reply(response.result, now_ms() as u64).unwrap()
            else {
                panic!("expected signed checkpoint chunk");
            };
            let chunk: Vec<u8> = hex
                .as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect();
            chunks += 1;
            target
                .current(&local, |_, _| {
                    bytes.extend_from_slice(&chunk);
                    Ok(())
                })
                .unwrap_or_else(|error| {
                    panic!(
                        "target ingestion failed: {error}; copy_ms={}, part_ms={}, bytes={}, chunks={}, original_permit_expired={}",
                        copy_started.elapsed().as_millis(),
                        part_started.elapsed().as_millis(),
                        bytes.len(),
                        chunks,
                        target.original.expires_at_ms <= now_ms() as u64,
                    )
                });
            assert!(bytes.len() as u64 <= size_bytes);
            if bytes.len() as u64 == size_bytes {
                break;
            }
            assert!(!chunk.is_empty(), "incomplete ranges must make progress");
        }
        eprintln!(
            "checkpoint component copy: blob={}, bytes={}, chunks={}, part_ms={}, copy_ms={}",
            artifact.is_some(),
            bytes.len(),
            chunks,
            part_started.elapsed().as_millis(),
            copy_started.elapsed().as_millis(),
        );
        bytes
    };
    let manifest_bytes = read_part(None);
    let manifest: CheckpointManifest = serde_json::from_slice(&manifest_bytes).unwrap();
    let mut copied_artifacts = std::collections::BTreeSet::new();
    for artifact in artifacts(&manifest) {
        if copied_artifacts.insert(artifact.sha256.clone()) {
            let bytes = read_part(Some(artifact.clone()));
            if !bytes.is_empty() {
                assert!(
                    target
                        .current(&local, |_, _| {
                            received.ingest_blob(artifact, &bytes[..bytes.len() - 1])?;
                            Ok(())
                        })
                        .is_err(),
                    "truncated input must never be acknowledged"
                );
            }
            target
                .current(&local, |_, _| {
                    received.ingest_blob(artifact, bytes.as_slice())?;
                    Ok(())
                })
                .unwrap();
        }
    }
    assert!(!copied_artifacts.is_empty());
    target
        .current(&local, |_, _| {
            let digest = received.publish(&manifest)?;
            anyhow::ensure!(
                digest == remote.checkpoint_digest,
                "copied checkpoint changed"
            );
            received.verify_durable_copy(&digest)?;
            let loaded = received.load(&digest)?;
            anyhow::ensure!(
                serde_json::to_vec(&loaded)? == manifest_bytes,
                "manifest changed"
            );
            Ok(())
        })
        .unwrap();
    reconstruction::assert_native_reconstruction(&target, &received);
    guest_import::assert_native_import_fence(&target, &received);
    quorum::assert_dirty_copy_cannot_acknowledge(&target, &received);
    takeover::assert_dirty_copy_cannot_take_over(&target, &received);
    let (_, foreign) = make_fetch(
        Some(ArtifactRef {
            sha256: "aa".repeat(32),
            size_bytes: 1,
        }),
        0,
    );
    assert!(fetch_with_account(server.clone(), peer, foreign, auth.clone()).is_err());
    let (_, next) = make_fetch(None, 0);
    let denied = fetch_with_account(server.clone(), peer, next, auth.clone()).unwrap();
    let policy = Connection::open(business_os_store_path(source_root)).unwrap();
    policy
        .execute(
            "UPDATE business_session_handoff_bindings SET state='revoked' WHERE binding_digest=?1",
            [&remote.binding_digest],
        )
        .unwrap();
    let mut writes = 0;
    assert!(denied
        .publication
        .with_current(&mut || {
            writes += 1;
            Ok(())
        })
        .is_err());
    assert_eq!(writes, 0);
    policy
        .execute(
            "UPDATE business_session_handoff_bindings SET state='active' WHERE binding_digest=?1",
            [&remote.binding_digest],
        )
        .unwrap();
    let (_, fresh) = make_fetch(None, 0);
    let pending = fetch_with_account(server.clone(), peer, fresh, auth.clone()).unwrap();
    let progress_root = tempfile::tempdir().unwrap();
    let staged_artifact = ArtifactRef {
        sha256: "bb".repeat(32),
        size_bytes: 4,
    };
    let mut staged = target
        .current(&local, |_, _| {
            let mut part =
                progress::StagedPart::open(progress_root.path(), Some(&staged_artifact))?;
            part.append(b"ab", 4)?;
            Ok(part)
        })
        .unwrap();
    drop(staged);
    // A new operation reopens the durable prefix only under current authority.
    staged = target
        .current(&local, |_, _| {
            progress::StagedPart::open(progress_root.path(), Some(&staged_artifact))
        })
        .unwrap();
    assert_eq!(staged.offset, 2);
    std::fs::remove_file(home.path().join("auth.json")).unwrap();
    assert!(
        pending
            .publication
            .with_current(&mut || {
                writes += 1;
                Ok(())
            })
            .is_err(),
        "credential removal stops source bytes"
    );
    assert!(
        target
            .current(&local, |_, _| {
                staged.append(b"cd", 4)?;
                writes += 1;
                Ok(())
            })
            .is_err(),
        "credential removal stops target ingestion"
    );
    assert_eq!(writes, 0);
    ctox_core::auth::save_auth(home.path(), &credentials, AuthCredentialsStoreMode::File).unwrap();
    target.current(&local, |_, _| Ok(())).unwrap();
    drop(CopyLifetime(live));
    assert!(
        target
            .current(&local, |_, _| {
                staged.append(b"cd", 4)?;
                writes += 1;
                Ok(())
            })
            .is_err(),
        "cancel fences queued blocking ingestion"
    );
    assert_eq!(writes, 0);
    assert_eq!(
        std::fs::metadata(progress_root.path().join(&staged_artifact.sha256))
            .unwrap()
            .len(),
        2
    );
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CopyRequest {
    pub binding_digest: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_route: String,
    #[serde(default, skip_serializing_if = "copy_only")]
    pub reconstruct: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub guest_id: String,
    #[serde(default, skip_serializing_if = "copy_only")]
    pub acknowledge: bool,
    #[serde(default, skip_serializing_if = "copy_only")]
    pub take_over: bool,
    #[serde(default, skip_serializing_if = "copy_only")]
    pub enroll_guest: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protection_receipts: Vec<ctox_sync::contracts::CheckpointCopyReceipt>,
}
fn copy_only(reconstruct: &bool) -> bool {
    !reconstruct
}
impl CopyRequest {
    pub(crate) fn operation_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(60)
    }
    fn valid_operation(&self) -> bool {
        let controls = [
            self.acknowledge,
            self.take_over,
            self.enroll_guest,
            !self.protection_receipts.is_empty(),
        ]
        .into_iter()
        .filter(|v| *v)
        .count();
        let identifiers_only =
            !self.reconstruct && self.guest_id.is_empty() && self.source_route.is_empty();
        (controls == 0
            && ((self.reconstruct && self.guest_id.is_empty() && self.source_route.is_empty())
                || (!self.reconstruct
                    && self.source_route.is_empty()
                    && super::super::super::guest_runtime::identifier(&self.guest_id))
                || (!self.reconstruct
                    && self.guest_id.is_empty()
                    && !self.source_route.is_empty()
                    && self.source_route.len() <= 256)))
            || (identifiers_only && controls == 1 && self.protection_receipts.len() <= 8)
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum CopyResponse {
    CopyPending {
        checkpoint_digest: String,
        verified_bytes: u64,
        partial_bytes: u64,
        total_bytes: Option<u64>,
    },
    GuestEnrolled {
        checkpoint_digest: String,
        guest_id: String,
        controller_id: String,
        controller_generation: u64,
    },
    Copied {
        checkpoint_digest: String,
    },
    Reconstructed {
        checkpoint_digest: String,
        preparation_id: String,
    },
    GuestImported {
        checkpoint_digest: String,
        guest_id: String,
        controller_id: String,
        controller_generation: u64,
        effect_id: String,
    },
    OwnershipTaken {
        checkpoint_digest: String,
        job_id: String,
        session_id: String,
        ownership: ctox_sync::authority::Ownership,
    },
    CopyAcknowledged {
        receipt: ctox_sync::contracts::CheckpointCopyReceipt,
    },
    CheckpointProtected {
        checkpoint_digest: String,
        sequence: u64,
        ownership_generation: u64,
    },
    Denied,
}
pub(crate) struct CheckpointListener {
    pub(crate) endpoint: PathBuf,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for CheckpointListener {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(&self.endpoint);
    }
}
pub(super) fn listen(
    server: Arc<Server<rxdb::plugins::replication_webrtc::WebRTCRsConnection>>,
    ipc: &Path,
    pool: Arc<
        RxWebRTCReplicationPool<rxdb::plugins::replication_webrtc::WebRTCRsConnectionHandler>,
    >,
    guests: Option<Arc<super::super::super::NativeGuestRegistry>>,
) -> anyhow::Result<CheckpointListener> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    private_dir(ipc)?;
    let endpoint = ipc.join("checkpoint.sock");
    let listener = tokio::net::UnixListener::bind(&endpoint)?;
    std::fs::set_permissions(&endpoint, std::fs::Permissions::from_mode(0o600))?;
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            if stream
                .peer_cred()
                .map_or(true, |c| c.uid() != unsafe { libc::geteuid() })
            {
                continue;
            }
            let request = tokio::time::timeout(std::time::Duration::from_secs(2), async {
                let n = stream.read_u32().await? as usize;
                anyhow::ensure!(n > 0 && n <= 32 * 1024, "invalid checkpoint control frame");
                let mut bytes = vec![0; n];
                stream.read_exact(&mut bytes).await?;
                Ok::<CopyRequest, anyhow::Error>(serde_json::from_slice(&bytes)?)
            })
            .await;
            let response = match request {
                Ok(Ok(r))
                    if r.binding_digest.len() == 64
                        && r.binding_digest
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                        && r.valid_operation() =>
                {
                    let deadline = r.operation_timeout();
                    let operation = async {
                        if r.enroll_guest {
                            let registry = guests
                                .as_ref()
                                .ok_or_else(|| anyhow::anyhow!("native guest host unavailable"))?;
                            guest_enrollment::enroll(
                                server.clone(),
                                registry.clone(),
                                r.binding_digest,
                            )
                            .await
                        } else if r.take_over {
                            let registry = guests.as_ref().ok_or_else(|| {
                                anyhow::anyhow!("native authority host unavailable")
                            })?;
                            takeover::take_over(server.clone(), registry.clone(), r.binding_digest)
                                .await
                        } else if r.acknowledge || !r.protection_receipts.is_empty() {
                            let registry = guests.as_ref().ok_or_else(|| {
                                anyhow::anyhow!("native authority host unavailable")
                            })?;
                            if r.acknowledge {
                                quorum::acknowledge(
                                    server.clone(),
                                    registry.clone(),
                                    r.binding_digest,
                                )
                                .await
                            } else {
                                quorum::protect(
                                    server.clone(),
                                    registry.clone(),
                                    r.binding_digest,
                                    r.protection_receipts,
                                )
                                .await
                            }
                        } else if !r.guest_id.is_empty() {
                            let registry = guests
                                .as_ref()
                                .ok_or_else(|| anyhow::anyhow!("native guest host unavailable"))?;
                            guest_import::import(
                                server.clone(),
                                registry.clone(),
                                r.binding_digest,
                                r.guest_id,
                            )
                            .await
                        } else if r.reconstruct {
                            let (checkpoint_digest, preparation_id) =
                                reconstruction::reconstruct(server.clone(), r.binding_digest)
                                    .await?;
                            Ok::<_, anyhow::Error>(CopyResponse::Reconstructed {
                                checkpoint_digest,
                                preparation_id,
                            })
                        } else {
                            let peer = pool
                                .connection_handler
                                .connection_for_peer(&r.source_route)
                                .ok_or_else(|| anyhow::anyhow!("checkpoint source unavailable"))?;
                            copy(server.clone(), pool.clone(), peer, r.binding_digest).await
                        }
                    };
                    tokio::select! {
                        result=tokio::time::timeout(deadline,operation)=>{
                            match result {Ok(Ok(response))=>response,_=>CopyResponse::Denied}
                        },
                        _=stream.read_u8()=>CopyResponse::Denied,
                    }
                }
                _ => CopyResponse::Denied,
            };
            let bytes = serde_json::to_vec(&response).unwrap_or_default();
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), async {
                stream.write_u32(bytes.len() as u32).await?;
                stream.write_all(&bytes).await
            })
            .await;
        }
    });
    Ok(CheckpointListener { endpoint, task })
}
