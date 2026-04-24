//! Unix-domain-socket IPC server for the qwen35-27b-q4km-dflash
//! engine.
//!
//! # Security posture
//!
//! * **Unix socket only.** No TCP, no HTTP, no TLS.
//! * **Socket mode 0600**, parent dir 0700 — owner-only access at
//!   the filesystem layer.
//! * **`SO_PEERCRED` check on accept** (Linux): server rejects
//!   connections whose peer UID differs from the server's UID.
//!   On non-Linux builds the check is skipped (we don't deploy
//!   production inference on other platforms).
//! * **Frame size cap** — one request is read from the socket with
//!   a hard upper bound; malformed / oversized frames close the
//!   connection rather than allocating unbounded memory.
//!
//! # Concurrency
//!
//! The target model is 27B Q4_K_M ≈ 16 GB VRAM; running two
//! forward passes in parallel on one A6000 is not feasible. The
//! server therefore **serializes** inference: at most one
//! `run_turn` in flight at a time, guarded by a sync mutex.
//! Additional connections queue behind it.
//!
//! The inference call is **synchronous** Rust (not async) — it
//! calls into ggml + CUDA directly. The surrounding socket IO is
//! async (tokio). The adapter buffers all response events into a
//! `Vec<u8>` during inference; once inference returns we async-
//! write the whole buffer back to the socket in one shot. This
//! keeps all the non-Send raw-pointer state safely out of the
//! futures layer.
//!
//! # Lifecycle
//!
//! * Binds the socket (removing a stale leftover if the previous
//!   process died uncleanly).
//! * On `SIGINT` / `SIGTERM` the accept loop stops, the socket is
//!   unlinked, in-flight turns are allowed to finish, then the
//!   process exits.

use std::io;
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::{UnixListener, UnixStream};
use tracing::{debug, error, info, warn};

use crate::adapter::{self, AdapterCtx, StreamSink};
use crate::model::{DraftWeights, TargetCache, TargetWeights};
use crate::tokenizer::Tokenizer;
use crate::wire::{
    IpcError, LocalIpcRequest, LocalIpcResponse, ResponsesStreamEvent, RuntimeHealth,
};

/// Hard cap on the single-line request payload. 64 MiB — more than
/// enough for any Responses request (even with large tool schemas);
/// smaller values kick badly-framed clients immediately.
const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;

/// Per-connection idle timeout.
const IDLE_TIMEOUT: Duration = Duration::from_secs(900);

/// Guarded handle onto the engine — one per server, serialized.
pub struct Engine {
    pub target: TargetWeights,
    pub draft: DraftWeights,
    pub cache: TargetCache,
    pub backend: crate::ffi::ggml_backend_t,
    pub tokenizer: Tokenizer,
    pub model_id: String,
}

// SAFETY: the FFI pointers inside TargetWeights/DraftWeights/
// TargetCache/ggml_backend_t are GPU-backed objects owned by this
// process; we serialize all inference through a sync `Mutex<Engine>`
// that no futures touch, so there is no cross-thread concurrent access.
unsafe impl Send for Engine {}
unsafe impl Sync for Engine {}

pub type SharedEngine = Arc<Mutex<Engine>>;

pub struct ServeConfig {
    /// Path of the Unix domain socket to bind.
    pub socket_path: PathBuf,
}

/// Bind the socket, accept forever, dispatch per-connection until
/// a shutdown signal arrives.
pub async fn serve(engine: SharedEngine, config: ServeConfig) -> Result<()> {
    let sock = &config.socket_path;
    if let Some(parent) = sock.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create_dir_all {}", parent.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mut perms) = std::fs::metadata(parent).map(|m| m.permissions()) {
                perms.set_mode(0o700);
                let _ = std::fs::set_permissions(parent, perms);
            }
        }
    }
    remove_stale_socket(sock)?;
    let listener =
        UnixListener::bind(sock).with_context(|| format!("bind {}", sock.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(sock)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(sock, perms)?;
    }

    info!(
        socket = %sock.display(),
        model = %engine.lock().unwrap().model_id,
        "qwen35-27b-q4km-dflash server listening"
    );

    let mut sig_int =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut sig_term =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _addr)) => {
                        if !peer_uid_authorized(&stream) {
                            warn!("rejecting connection: peer UID mismatch");
                            drop(stream);
                            continue;
                        }
                        let engine = engine.clone();
                        tokio::spawn(async move {
                            if let Err(err) = handle_connection(engine, stream).await {
                                debug!("connection closed: {err:#}");
                            }
                        });
                    }
                    Err(err) => {
                        error!("accept failed: {err}; continuing");
                    }
                }
            }
            _ = sig_int.recv() => {
                info!("SIGINT received; draining");
                break;
            }
            _ = sig_term.recv() => {
                info!("SIGTERM received; draining");
                break;
            }
        }
    }

    drop(listener);
    let _ = std::fs::remove_file(sock);
    info!("shutdown complete");
    Ok(())
}

#[cfg(target_os = "linux")]
fn peer_uid_authorized(stream: &UnixStream) -> bool {
    use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
    use nix::unistd::Uid;
    let fd = stream.as_raw_fd();
    let borrowed = unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) };
    let Ok(cred) = getsockopt(&borrowed, PeerCredentials) else {
        return false;
    };
    Uid::from_raw(cred.uid()) == Uid::current()
}

#[cfg(not(target_os = "linux"))]
fn peer_uid_authorized(_stream: &UnixStream) -> bool {
    true
}

fn remove_stale_socket(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("remove stale {}", path.display())),
    }
}

async fn handle_connection(engine: SharedEngine, stream: UnixStream) -> Result<()> {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);

    // 1. Read one request line, size-capped.
    let mut buf = Vec::with_capacity(4096);
    let bytes = tokio::time::timeout(
        IDLE_TIMEOUT,
        read_line_capped(&mut reader, &mut buf, MAX_REQUEST_BYTES),
    )
    .await
    .map_err(|_| anyhow!("idle timeout waiting for request"))??;
    if bytes == 0 {
        return Ok(());
    }

    // 2. Parse.
    let request: LocalIpcRequest = match serde_json::from_slice(&buf) {
        Ok(r) => r,
        Err(err) => {
            let msg = format!("invalid_request: {err}");
            let _ = write_json_line(
                &mut writer,
                &LocalIpcResponse::Error(IpcError {
                    code: "invalid_request".into(),
                    message: msg.clone(),
                }),
            )
            .await;
            let _ = writer.flush().await;
            return Err(anyhow!(msg));
        }
    };

    // 3. Dispatch.
    match request {
        LocalIpcRequest::RuntimeHealth => {
            let (healthy, model_id) = {
                let g = engine.lock().unwrap();
                (true, g.model_id.clone())
            };
            let health = LocalIpcResponse::RuntimeHealth(RuntimeHealth {
                healthy,
                default_model: Some(model_id.clone()),
                loaded_models: vec![model_id],
            });
            write_json_line(&mut writer, &health).await?;
            writer.flush().await?;
        }
        LocalIpcRequest::ResponsesCreate(req) => {
            // Inference runs synchronously off the async scheduler
            // via spawn_blocking; it buffers events into a Vec and
            // returns the buffer, which we then async-write back.
            let engine_for_task = engine.clone();
            let frames = tokio::task::spawn_blocking(move || {
                run_responses_turn_sync(engine_for_task, req)
            })
            .await
            .context("inference task panicked")??;

            writer.write_all(&frames).await?;
            writer.flush().await?;
        }
    }

    Ok(())
}

/// Synchronous entrypoint: locks the engine, buffers the whole
/// Responses stream into `Vec<u8>`, returns it. Safe to call from a
/// blocking thread pool; never blocks the async executor.
fn run_responses_turn_sync(
    engine: SharedEngine,
    req: crate::wire::ResponsesCreateRequest,
) -> Result<Vec<u8>> {
    let mut guard = engine
        .lock()
        .map_err(|e| anyhow!("engine mutex poisoned: {e}"))?;
    let engine_ref: &mut Engine = &mut *guard;

    let mut sink = BufferSink::default();
    let model_id = engine_ref.model_id.clone();
    // Build the adapter ctx — raw references into the guarded engine
    // never escape this sync function (we hand out no future that
    // captures `&mut TargetWeights`).
    let tokenizer = &engine_ref.tokenizer as *const Tokenizer;
    let mut ctx = AdapterCtx {
        target_weights: &mut engine_ref.target,
        draft_weights: &mut engine_ref.draft,
        target_cache: &mut engine_ref.cache,
        backend: engine_ref.backend,
        // SAFETY: tokenizer lives in the same Engine struct and is
        // pinned under the same mutex guard; borrow checker can't see
        // that we hold `&mut` to disjoint fields via raw ptr here.
        tokenizer: unsafe { &*tokenizer },
        model_id: &model_id,
        sink: &mut sink,
    };

    if let Err(err) = adapter::run_turn(&mut ctx, &req) {
        error!("run_turn failed: {err:#}");
        let _ = adapter::emit_failed(
            &mut sink,
            &model_id,
            "inference_error",
            &err.to_string(),
        );
    }
    Ok(sink.buf)
}

/// Byte-level sink the adapter writes JSON events into. No locking,
/// no async — just append JSON lines.
#[derive(Default)]
struct BufferSink {
    buf: Vec<u8>,
}

impl StreamSink for BufferSink {
    fn send(&mut self, event: ResponsesStreamEvent) -> Result<()> {
        serde_json::to_writer(&mut self.buf, &event)?;
        self.buf.push(b'\n');
        Ok(())
    }
}

/// Read bytes into `buf` until `\n` or EOF, capped at `limit`.
/// Returns the number of bytes read (excluding the newline).
async fn read_line_capped<R: AsyncBufReadExt + Unpin>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    limit: usize,
) -> Result<usize> {
    buf.clear();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(buf.len());
        }
        if let Some(pos) = available.iter().position(|b| *b == b'\n') {
            buf.extend_from_slice(&available[..pos]);
            reader.consume(pos + 1);
            return Ok(buf.len());
        }
        if buf.len() + available.len() > limit {
            return Err(anyhow!("request exceeds {limit}-byte cap"));
        }
        buf.extend_from_slice(available);
        let n = available.len();
        reader.consume(n);
    }
}

async fn write_json_line<T: serde::Serialize, W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    value: &T,
) -> Result<()> {
    let mut buf = serde_json::to_vec(value).context("encode response frame")?;
    buf.push(b'\n');
    writer.write_all(&buf).await.context("write response frame")?;
    Ok(())
}
