//! Typed local control IPC. Hosts own Unix-socket / named-pipe binding and ACLs.
//! No TCP or HTTP fallback; framing is shared by both platform transports.
use crate::{
    authority::{client::ExecutionAuthority, Command, Receipt, Request},
    contracts::{SyncIpcOperation, SyncIpcRequest, SyncIpcResponse, SyncIpcResult},
};
use std::{io, sync::Arc, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const IPC_PROTOCOL_VERSION: u32 = crate::contracts::CTOX_SYNC_IPC_PROTOCOL_VERSION;
pub const IPC_MAX_FRAME_BYTES: usize = crate::contracts::CTOX_SYNC_IPC_MAX_FRAME_BYTES as usize;
const FRAME_DEADLINE: Duration =
    Duration::from_millis(crate::contracts::CTOX_SYNC_IPC_DEADLINE_MILLIS as u64);

pub struct AuthorityIpc {
    node: Arc<dyn ExecutionAuthority>,
}
impl AuthorityIpc {
    pub fn new(node: Arc<dyn ExecutionAuthority>) -> Self {
        Self { node }
    }
    pub async fn dispatch(&self, request: SyncIpcRequest) -> SyncIpcResponse {
        let result = if request.version != IPC_PROTOCOL_VERSION {
            SyncIpcResult::Rejected {
                reason: "incompatible IPC protocol".into(),
            }
        } else if request.request_id.is_empty() || request.request_id.len() > 256 {
            SyncIpcResult::Rejected {
                reason: "invalid request ID".into(),
            }
        } else {
            self.execute(&request.request_id, request.operation).await
        };
        SyncIpcResponse {
            version: IPC_PROTOCOL_VERSION,
            request_id: request.request_id,
            result,
        }
    }
    async fn execute(&self, request_id: &str, operation: SyncIpcOperation) -> SyncIpcResult {
        let command = match operation {
            SyncIpcOperation::WorkerMembership { node_id } => {
                if node_id == 0 || node_id > 9_007_199_254_740_991 {
                    return SyncIpcResult::Rejected {
                        reason: "invalid worker node ID".into(),
                    };
                }
                return match self.node.worker_membership(node_id).await {
                    Ok(worker) => SyncIpcResult::WorkerMembership { node_id, worker },
                    Err(error) => authority_error(error),
                };
            }
            SyncIpcOperation::AdmitWorker { worker } => Command::AdmitWorker { worker },
            SyncIpcOperation::RevokeWorker { node_id } => Command::RevokeWorker { node_id },
            SyncIpcOperation::Hello {} => {
                return SyncIpcResult::Ready {
                    node_id: self.node.node_id(),
                    scope_id: self.node.scope_id().to_owned(),
                    protocol_version: IPC_PROTOCOL_VERSION,
                }
            }
            SyncIpcOperation::Validate { job_id, ownership } => {
                return match self.node.validate_ownership(&job_id, &ownership).await {
                    Ok(job) => SyncIpcResult::Authorized {
                        spec: job.spec,
                        ownership: job.ownership,
                    },
                    Err(error) => authority_error(error),
                }
            }
            SyncIpcOperation::Create { spec } => Command::Create {
                spec,
                owner: self.node.node_id(),
            },
            SyncIpcOperation::ProtectCheckpoint {
                job_id,
                ownership,
                receipts,
            } => Command::ProtectCheckpoint {
                job_id,
                ownership,
                receipts,
            },
            SyncIpcOperation::TakeOver {
                job_id,
                expected,
                checkpoint_digest,
            } => Command::TakeOver {
                job_id,
                expected,
                checkpoint_digest,
                owner: self.node.node_id(),
            },
            SyncIpcOperation::BeginEffect {
                job_id,
                ownership,
                effect_id,
            } => Command::BeginEffect {
                job_id,
                ownership,
                effect_id,
            },
            SyncIpcOperation::CompleteEffect {
                job_id,
                ownership,
                effect_id,
            } => Command::CompleteEffect {
                job_id,
                ownership,
                effect_id,
            },
            SyncIpcOperation::Stop { job_id, ownership } => Command::Stop { job_id, ownership },
        };
        match self
            .node
            .submit(Request {
                request_id: request_id.into(),
                actor: self.node.node_id(),
                command,
            })
            .await
        {
            Ok(Receipt::WorkerApplied(worker)) => SyncIpcResult::WorkerApplied { worker },
            Ok(Receipt::WorkerReplayed(worker)) => SyncIpcResult::WorkerReplayed { worker },
            Ok(Receipt::Applied(job)) => SyncIpcResult::Applied {
                spec: job.spec,
                ownership: job.ownership,
            },
            Ok(Receipt::Replayed(job)) => SyncIpcResult::Replayed {
                spec: job.spec,
                ownership: job.ownership,
            },
            Ok(Receipt::Rejected(reason)) => SyncIpcResult::Rejected {
                reason: format!("{reason:?}"),
            },
            Err(error) => authority_error(error),
        }
    }
    /// Serve an already-authenticated local stream. One frame is processed at a
    /// time, so a client cannot create unbounded work on one connection.
    pub async fn serve<S: AsyncRead + AsyncWrite + Unpin>(&self, mut stream: S) -> io::Result<()> {
        loop {
            // Idle connections hold no authority. Once a header starts, bound
            // completion of that header and the payload independently.
            let mut header = [0; 4];
            if stream.read(&mut header[..1]).await? == 0 {
                return Ok(());
            }
            timeout(stream.read_exact(&mut header[1..])).await?;
            let size = u32::from_be_bytes(header) as usize;
            if size == 0 || size > IPC_MAX_FRAME_BYTES {
                return Err(invalid("IPC frame exceeds its budget"));
            }
            let mut bytes = vec![0; size];
            timeout(stream.read_exact(&mut bytes)).await?;
            let request: SyncIpcRequest =
                serde_json::from_slice(&bytes).map_err(|e| invalid(&e.to_string()))?;
            let response = self.dispatch(request).await;
            let bytes = serde_json::to_vec(&response).map_err(io::Error::other)?;
            if bytes.len() > IPC_MAX_FRAME_BYTES {
                return Err(invalid("IPC response exceeds its budget"));
            }
            timeout(async {
                stream
                    .write_all(&(bytes.len() as u32).to_be_bytes())
                    .await?;
                stream.write_all(&bytes).await?;
                stream.flush().await
            })
            .await?;
        }
    }
}
fn authority_error(error: io::Error) -> SyncIpcResult {
    match error.kind() {
        io::ErrorKind::PermissionDenied | io::ErrorKind::InvalidInput => SyncIpcResult::Rejected {
            reason: error.to_string(),
        },
        _ => SyncIpcResult::Unavailable {
            reason: error.to_string(),
        },
    }
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
async fn timeout<T>(future: impl std::future::Future<Output = io::Result<T>>) -> io::Result<T> {
    tokio::time::timeout(FRAME_DEADLINE, future)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "incomplete IPC frame"))?
}
