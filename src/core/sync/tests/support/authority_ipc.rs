//! Real private Unix IPC on supported hosts; framed in-process coverage elsewhere.
//! The non-Unix branch does not claim a platform listener exists.
use ctox_sync::{
    authority::client::ExecutionAuthority,
    contracts::{SyncIpcOperation, SyncIpcRequest, SyncIpcResponse, SyncIpcResult},
    ipc::{IPC_MAX_FRAME_BYTES, IPC_PROTOCOL_VERSION},
};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub async fn call(
    node: Arc<dyn ExecutionAuthority>,
    request_id: &str,
    operation: SyncIpcOperation,
) -> SyncIpcResult {
    #[cfg(unix)]
    {
        use ctox_sync::local_host::{private_ipc_directory, LocalIpcHost};
        let directory = private_ipc_directory().unwrap();
        let host = LocalIpcHost::start_authority(directory.path().into(), node)
            .await
            .unwrap();
        let stream = tokio::net::UnixStream::connect(host.endpoint())
            .await
            .unwrap();
        let result = exchange(stream, request_id, operation).await;
        host.shutdown().await.unwrap();
        result
    }
    #[cfg(not(unix))]
    {
        let (client, server) = tokio::io::duplex(IPC_MAX_FRAME_BYTES);
        let task =
            tokio::spawn(
                async move { ctox_sync::ipc::AuthorityIpc::new(node).serve(server).await },
            );
        let result = exchange(client, request_id, operation).await;
        task.await.unwrap().unwrap();
        result
    }
}

async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    request_id: &str,
    operation: SyncIpcOperation,
) -> SyncIpcResult {
    tokio::time::timeout(Duration::from_secs(10), async {
        let bytes = serde_json::to_vec(&SyncIpcRequest {
            version: IPC_PROTOCOL_VERSION,
            request_id: request_id.into(),
            operation,
        })
        .unwrap();
        assert!(!bytes.is_empty() && bytes.len() <= IPC_MAX_FRAME_BYTES);
        stream.write_u32(bytes.len() as u32).await.unwrap();
        stream.write_all(&bytes).await.unwrap();
        stream.flush().await.unwrap();
        let size = stream.read_u32().await.unwrap() as usize;
        assert!(size > 0 && size <= IPC_MAX_FRAME_BYTES);
        let mut bytes = vec![0; size];
        stream.read_exact(&mut bytes).await.unwrap();
        let response: SyncIpcResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(response.version, IPC_PROTOCOL_VERSION);
        assert_eq!(response.request_id, request_id);
        response.result
    })
    .await
    .expect("native authority IPC exceeded its deadline")
}
