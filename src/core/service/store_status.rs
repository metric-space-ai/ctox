//! V4 lifecycle compatibility socket. Core custody is process readiness;
//! signaling readiness belongs exclusively to the launch context.
use super::{
    service_listen_addr, service_socket_path, AcceptedResponse, ServiceIpcRequest,
    ServiceIpcResponse, ServiceStatus,
};
use crate::service::store_inputs::StoreInputs;
use ctox_store::writer::Lifecycle;
use std::os::unix::{
    fs::{FileTypeExt, MetadataExt, PermissionsExt},
    net::UnixListener,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{watch, Notify, Semaphore},
    task::JoinHandle,
};

struct SocketFile {
    path: PathBuf,
    dev: u64,
    ino: u64,
}
impl SocketFile {
    fn remove(&self) {
        if std::fs::symlink_metadata(&self.path)
            .is_ok_and(|m| m.dev() == self.dev && m.ino() == self.ino && m.file_type().is_socket())
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
struct Shared {
    inputs: Weak<StoreInputs>,
    stop_requested: AtomicBool,
    requested: Notify,
}
pub(crate) struct StatusIngress {
    shared: Arc<Shared>,
    shutdown: watch::Sender<bool>,
    task: Mutex<Option<JoinHandle<()>>>,
}
impl StatusIngress {
    pub(crate) async fn start(inputs: &Arc<StoreInputs>) -> anyhow::Result<Arc<Self>> {
        let path = service_socket_path(&inputs.root);
        let check_path = path.clone();
        let existing =
            tokio::task::spawn_blocking(move || match std::fs::symlink_metadata(check_path) {
                Ok(m) => Ok(Some(m)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e),
            })
            .await??;
        if let Some(metadata) = existing {
            anyhow::ensure!(
                metadata.file_type().is_socket() && metadata.uid() == unsafe { libc::geteuid() },
                "lifecycle socket is not an owned Unix socket"
            );
            match tokio::time::timeout(
                Duration::from_millis(200),
                tokio::net::UnixStream::connect(&path),
            )
            .await
            {
                Ok(Err(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                    ) =>
                {
                    ()
                }
                _ => anyhow::bail!("lifecycle socket is already owned or busy"),
            }
            let stale = SocketFile {
                path: path.clone(),
                dev: metadata.dev(),
                ino: metadata.ino(),
            };
            tokio::task::spawn_blocking(move || stale.remove()).await?;
        }
        let bind_path = path.clone();
        let (listener, socket) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            if let Some(parent) = bind_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let listener = UnixListener::bind(&bind_path)?;
            std::fs::set_permissions(&bind_path, std::fs::Permissions::from_mode(0o600))?;
            listener.set_nonblocking(true)?;
            let meta = std::fs::symlink_metadata(&bind_path)?;
            Ok((
                listener,
                SocketFile {
                    path: bind_path,
                    dev: meta.dev(),
                    ino: meta.ino(),
                },
            ))
        })
        .await??;
        let listener = tokio::net::UnixListener::from_std(listener)?;
        let shared = Arc::new(Shared {
            inputs: Arc::downgrade(inputs),
            stop_requested: AtomicBool::new(false),
            requested: Notify::new(),
        });
        let (shutdown, mut stopped) = watch::channel(false);
        let state = shared.clone();
        let task = tokio::spawn(async move {
            let permits = Arc::new(Semaphore::new(64));
            let mut peers = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    biased;
                    _=stopped.changed()=>break,
                    accepted=listener.accept()=>{
                        let Ok((stream,_))=accepted else{break;};
                        let Ok(permit)=permits.clone().try_acquire_owned() else{drop(stream);continue;};
                        let state=state.clone();
                        peers.spawn(async move{
                            let _permit=permit;
                            // Same-user kernel identity, bounded frame/lifetime/concurrency.
                            if stream.peer_cred().map(|c|c.uid()).ok()!=Some(unsafe{libc::geteuid()}){return;}
                            let _=tokio::time::timeout(Duration::from_secs(2),state.respond(stream)).await;
                        });
                    }
                    _=peers.join_next(),if !peers.is_empty()=>(),
                }
            }
            peers.shutdown().await;
            drop(listener);
            let _ = tokio::task::spawn_blocking(move || socket.remove()).await;
        });
        Ok(Arc::new(Self {
            shared,
            shutdown,
            task: Mutex::new(Some(task)),
        }))
    }
    pub(crate) async fn wait_stop(&self) {
        loop {
            let notified = self.shared.requested.notified();
            if self.shared.stop_requested.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }
    pub(crate) async fn shutdown(&self) -> anyhow::Result<()> {
        self.shutdown.send_replace(true);
        let task = self.task.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(task) = task {
            task.await?;
        }
        Ok(())
    }
}
impl Shared {
    async fn snapshot(&self) -> anyhow::Result<ServiceStatus> {
        let inputs = self
            .inputs
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!("Core owner retired"))?;
        anyhow::ensure!(
            inputs.core.writer.lifecycle() == Lifecycle::Ready
                && inputs.core.writer.generation().0 == inputs.core.readers.generation(),
            "Core owner is not ready"
        );
        // Never wait for browser_ingress_ready here. A running Core must be
        // observable even during DNS failure or signaling protocol rejection.
        let browser_ready = inputs.browser_ingress_ready().is_ok();
        Ok(ServiceStatus {
            running: true,
            busy: false,
            pid: Some(std::process::id()),
            listen_addr: service_listen_addr(&inputs.root),
            autostart_enabled: false,
            manager: "process".into(),
            pending_count: 0,
            pending_previews: vec![],
            blocked_count: 0,
            blocked_previews: vec![],
            current_goal_preview: None,
            active_source_label: None,
            recent_events: vec![],
            last_error: None,
            last_completed_at: None,
            last_reply_chars: None,
            monitor_last_check_at: None,
            monitor_alerts: vec![],
            monitor_last_error: None,
            last_agent_outcome: None,
            worker_active_count: 0,
            worker_phase: None,
            business_os: None,
            work_hours: Default::default(),
            performance: serde_json::json!({"status_only":true,"queue_state":"unknown","core_generation":inputs.core.writer.generation().0,"browser_ingress_ready":browser_ready}),
            // Queue/worker fields are explicitly unknown, not a fresh empty queue.
            degraded_probe: true,
        })
    }
    async fn respond(&self, mut stream: tokio::net::UnixStream) -> anyhow::Result<()> {
        let mut frame = Vec::with_capacity(128);
        loop {
            let byte = stream.read_u8().await?;
            if byte == b'\n' {
                break;
            }
            anyhow::ensure!(frame.len() < 4096, "lifecycle request exceeds 4 KiB");
            frame.push(byte);
        }
        let request = serde_json::from_slice::<ServiceIpcRequest>(&frame)?;
        let stopping = matches!(&request, ServiceIpcRequest::Stop);
        let response=match request {
            ServiceIpcRequest::Status=>ServiceIpcResponse::Status(self.snapshot().await?),
            ServiceIpcRequest::Stop=>{
                ServiceIpcResponse::Accepted(AcceptedResponse{accepted:true,status:"stopping".into()})
            },
            _=>ServiceIpcResponse::Error{message:"V4 lifecycle IPC supports status/stop; data operations require the typed owner/app ingress".into()},
        };
        let mut bytes = serde_json::to_vec(&response)?;
        bytes.push(b'\n');
        stream.write_all(&bytes).await?;
        stream.shutdown().await?;
        if stopping {
            self.stop_requested.store(true, Ordering::Release);
            self.requested.notify_waiters();
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "store_status_tests.rs"]
mod tests;
