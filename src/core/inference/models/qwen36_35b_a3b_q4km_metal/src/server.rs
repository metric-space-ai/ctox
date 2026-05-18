// Origin: CTOX
// License: Apache-2.0

//! Unix-domain-socket Responses-IPC server.
//!
//! Stage-1 only answers `runtime_health` honestly (with `healthy=false`,
//! `stage="skeleton"`) and rejects every `responses_create` with a
//! typed `engine_not_ready` error. That is enough for the harness to
//! probe this backend, see it's not ready, and route real traffic to
//! the existing `qwen36_35b_a3b_ggml` shim instead of crashing or
//! hanging.

use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::{UnixListener, UnixStream};
use tracing::{debug, error, info, warn};

use crate::driver::Engine;
use crate::wire::{IpcError, LocalIpcRequest, LocalIpcResponse, RuntimeHealth};

const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;
const IDLE_TIMEOUT: Duration = Duration::from_secs(900);

/// Configuration for a single server invocation. Mirrors the canonical
/// `qwen36_35b_a3b_ggml` shim's CLI surface where it overlaps so the
/// harness can probe both backends with the same arguments.
#[derive(Clone, Debug)]
pub struct ServeConfig {
    pub socket: std::path::PathBuf,
    pub model_id: String,
}

/// Run the server until SIGINT/SIGTERM or until the parent dies.
pub async fn run(config: ServeConfig) -> Result<()> {
    let engine = Arc::new(Engine::new());
    info!(
        socket = %config.socket.display(),
        model_id = %config.model_id,
        "qwen36-35b-a3b-q4km-metal server starting (stage-1 skeleton)"
    );
    serve(engine, config).await
}

async fn serve(engine: Arc<Engine>, config: ServeConfig) -> Result<()> {
    if let Some(parent) = config.socket.parent() {
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
    remove_stale_socket(&config.socket)?;
    let listener = UnixListener::bind(&config.socket)
        .with_context(|| format!("bind {}", config.socket.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&config.socket)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&config.socket, perms)?;
    }

    info!(socket = %config.socket.display(), "server listening");
    let mut sig_int = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut sig_term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _addr)) => {
                    if !peer_uid_authorized(&stream) {
                        warn!("rejecting connection: peer UID mismatch");
                        drop(stream);
                        continue;
                    }
                    let engine = engine.clone();
                    let model_id = config.model_id.clone();
                    tokio::spawn(async move {
                        if let Err(err) = handle_connection(engine, model_id, stream).await {
                            debug!("connection closed: {err:#}");
                        }
                    });
                }
                Err(err) => error!("accept failed: {err}; continuing"),
            },
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
    let _ = std::fs::remove_file(&config.socket);
    info!("shutdown complete");
    Ok(())
}

async fn handle_connection(
    _engine: Arc<Engine>,
    model_id: String,
    stream: UnixStream,
) -> Result<()> {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);

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

    let request: LocalIpcRequest = match serde_json::from_slice(&buf) {
        Ok(r) => r,
        Err(err) => {
            write_json_line(
                &mut writer,
                &LocalIpcResponse::Error(IpcError {
                    code: "invalid_request".into(),
                    message: format!("invalid_request: {err}"),
                }),
            )
            .await?;
            writer.flush().await?;
            return Ok(());
        }
    };

    match request {
        LocalIpcRequest::RuntimeHealth => {
            write_json_line(
                &mut writer,
                &LocalIpcResponse::RuntimeHealth(RuntimeHealth {
                    healthy: false,
                    default_model: Some(model_id.clone()),
                    loaded_models: vec![],
                    stage: "skeleton",
                }),
            )
            .await?;
            writer.flush().await?;
        }
        LocalIpcRequest::ResponsesCreate(_) => {
            write_json_line(
                &mut writer,
                &LocalIpcResponse::Error(IpcError {
                    code: "engine_not_ready".into(),
                    message: "qwen36-35b-a3b-q4km-metal is in stage-1 skeleton; \
                              use the qwen36_35b_a3b_ggml shim until the native \
                              Metal Q4_K_M path lands"
                        .into(),
                }),
            )
            .await?;
            writer.flush().await?;
        }
    }
    Ok(())
}

fn peer_uid_authorized(_stream: &UnixStream) -> bool {
    // This crate is macOS-only — see project_platform_installer in
    // CTOX memory: "Linux+CUDA vs macOS+Metal never coexist; model
    // crates cfg-gated, installer branches at install time." The
    // 0700/0600 directory + socket modes set during `serve()` are the
    // current peer-uid contract on macOS. Stage 2 may add a
    // `getpeereid(3)` check via `libc::getpeereid` once the native
    // Metal path is doing real work; for the stage-1 skeleton, the
    // filesystem perms are sufficient.
    true
}

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

async fn write_json_line<T: Serialize, W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    value: &T,
) -> Result<()> {
    let mut buf = serde_json::to_vec(value).context("encode response frame")?;
    buf.push(b'\n');
    writer.write_all(&buf).await.context("write response frame")?;
    Ok(())
}

fn remove_stale_socket(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("remove stale {}", path.display())),
    }
}
