// Origin: CTOX
// License: AGPL-3.0-only

use super::*;
use anyhow::{ensure, Result};
use tokio::io::DuplexStream;

const GREETING: &str = "{\"QMP\":{\"version\":{\"qemu\":{\"major\":8,\"minor\":2,\"micro\":0}},\"capabilities\":[]}}\r\n";

async fn frame<S: AsyncRead + AsyncWrite + Unpin>(
    peer: &mut BufStream<S>,
    value: Value,
) -> Result<()> {
    peer.write_all(&serde_json::to_vec(&value)?).await?;
    peer.write_all(b"\r\n").await?;
    peer.flush().await?;
    Ok(())
}

async fn request<S: AsyncRead + AsyncWrite + Unpin>(peer: &mut BufStream<S>) -> Result<Value> {
    let mut line = String::new();
    ensure!(
        peer.read_line(&mut line).await? > 0,
        "expected a QMP request"
    );
    Ok(serde_json::from_str(&line)?)
}

async fn negotiate_peer(stream: DuplexStream) -> Result<BufStream<DuplexStream>> {
    let mut peer = BufStream::new(stream);
    // Split the greeting inside JSON, proving that packet boundaries aren't frames.
    peer.write_all(&GREETING.as_bytes()[..13]).await?;
    peer.flush().await?;
    peer.write_all(&GREETING.as_bytes()[13..]).await?;
    peer.flush().await?;
    let capabilities = request(&mut peer).await?;
    ensure!(
        capabilities["execute"] == "qmp_capabilities",
        "capabilities must come first"
    );
    frame(&mut peer, json!({"return": {}, "id": capabilities["id"]})).await?;
    Ok(peer)
}

#[tokio::test]
async fn commands_correlate_responses_and_ignore_bounded_events() -> Result<()> {
    let (client_stream, peer_stream) = tokio::io::duplex(4096);
    let (client, peer) = tokio::join!(
        async move {
            let mut client = QmpClient::negotiate(client_stream, OPERATION_TIMEOUT).await?;
            ensure!(
                client.query_status().await?.status == "prelaunch",
                "initial status"
            );
            client.resume().await?;
            ensure!(client.query_status().await?.running, "running status");
            client.pause().await?;
            client.request_powerdown().await?;
            client.quit().await?;
            ensure!(
                client.pause().await == Err(QmpError::ConnectionUnusable),
                "quit retires connection"
            );
            Ok::<_, anyhow::Error>(())
        },
        async move {
            let mut peer = negotiate_peer(peer_stream).await?;
            for (index, operation) in [
                "query-status",
                "cont",
                "query-status",
                "stop",
                "system_powerdown",
                "quit",
            ]
            .iter()
            .enumerate()
            {
                let request = request(&mut peer).await?;
                ensure!(request["execute"] == *operation, "wrong operation");
                ensure!(request["id"] == (index + 2) as u64, "wrong correlation");
                frame(
                    &mut peer,
                    json!({"event":"STOP","data":{},"timestamp":{"seconds":1,"microseconds":0}}),
                )
                .await?;
                frame(&mut peer, json!({"return":{},"id":999})).await?;
                let result = if *operation == "query-status" {
                    json!({"running": index == 2, "status": if index == 2 { "running" } else { "prelaunch" }, "singlestep":false})
                } else {
                    json!({})
                };
                frame(&mut peer, json!({"return":result,"id":request["id"]})).await?;
            }
            Ok::<_, anyhow::Error>(())
        }
    );
    client?;
    peer
}

#[tokio::test]
async fn explicit_failure_is_correlated_and_does_not_log_guest_details() -> Result<()> {
    let (client_stream, peer_stream) = tokio::io::duplex(4096);
    let (client, peer) = tokio::join!(
        async move {
            let mut client = QmpClient::negotiate(client_stream, OPERATION_TIMEOUT).await?;
            let error = client.pause().await.unwrap_err();
            ensure!(error == QmpError::CommandFailed, "explicit command error");
            ensure!(
                !format!("{error:?} {error}").contains("private-path"),
                "private response leaked"
            );
            ensure!(
                !client.query_status().await?.running,
                "subsequent correlated read"
            );
            Ok::<_, anyhow::Error>(())
        },
        async move {
            let mut peer = negotiate_peer(peer_stream).await?;
            let stop = request(&mut peer).await?;
            frame(&mut peer, json!({"error":{"class":"GenericError","desc":"private-path /secret/guest"},"id":stop["id"]})).await?;
            let status = request(&mut peer).await?;
            frame(
                &mut peer,
                json!({"return":{"running":false,"status":"paused"},"id":status["id"]}),
            )
            .await?;
            Ok::<_, anyhow::Error>(())
        }
    );
    client?;
    peer
}

#[tokio::test]
async fn incomplete_oversized_malformed_and_uncorrelated_replies_poison_connection() -> Result<()> {
    for mode in [
        "eof",
        "oversized",
        "malformed",
        "wrong-id",
        "event-flood",
        "invalid-status",
    ] {
        let (client_stream, peer_stream) = tokio::io::duplex(4096);
        let (client, peer) = tokio::join!(
            async move {
                let mut client = QmpClient::negotiate(client_stream, OPERATION_TIMEOUT).await?;
                ensure!(
                    client.query_status().await == Err(QmpError::UnknownOutcome),
                    "{mode}"
                );
                ensure!(
                    client.pause().await == Err(QmpError::ConnectionUnusable),
                    "reused {mode}"
                );
                Ok::<_, anyhow::Error>(())
            },
            async move {
                let mut peer = negotiate_peer(peer_stream).await?;
                let pending = request(&mut peer).await?;
                match mode {
                    "eof" => {
                        peer.write_all(b"{\"return\":").await?;
                        peer.flush().await?;
                    }
                    "oversized" => {
                        // The client may close while the last bytes are being written.
                        let _ = peer.write_all(&vec![b'x'; MAX_FRAME_BYTES + 1]).await;
                        let _ = peer.flush().await;
                    }
                    "malformed" => {
                        peer.write_all(b"not-json\r\n").await?;
                        peer.flush().await?;
                    }
                    "wrong-id" => {
                        frame(&mut peer, json!({"return":{},"id":999})).await?;
                    }
                    "event-flood" => {
                        for _ in 0..=MAX_SKIPPED_FRAMES {
                            frame(&mut peer, json!({"event":"STOP"})).await?;
                        }
                    }
                    "invalid-status" => {
                        frame(&mut peer, json!({"return":{"running":"yes","status":"running"},"id":pending["id"]})).await?;
                    }
                    _ => unreachable!(),
                }
                Ok::<_, anyhow::Error>(())
            }
        );
        client?;
        peer?;
    }
    Ok(())
}

#[tokio::test]
async fn timeout_does_not_retry_or_reuse_an_uncertain_command() -> Result<()> {
    let (client_stream, peer_stream) = tokio::io::duplex(4096);
    let (client, peer) = tokio::join!(
        async move {
            let mut client = QmpClient::negotiate(client_stream, OPERATION_TIMEOUT).await?;
            client.timeout = Duration::from_millis(25);
            ensure!(
                client.pause().await == Err(QmpError::UnknownOutcome),
                "timeout must stay unknown"
            );
            ensure!(
                client.resume().await == Err(QmpError::ConnectionUnusable),
                "timeout reused connection"
            );
            Ok::<_, anyhow::Error>(())
        },
        async move {
            let mut peer = negotiate_peer(peer_stream).await?;
            ensure!(
                request(&mut peer).await?["execute"] == "stop",
                "missing command"
            );
            let mut tail = String::new();
            ensure!(
                peer.read_line(&mut tail).await? == 0,
                "command was repeated"
            );
            Ok::<_, anyhow::Error>(())
        }
    );
    client?;
    peer
}

#[tokio::test]
async fn cancellation_after_write_retires_the_connection() -> Result<()> {
    let (client_stream, peer_stream) = tokio::io::duplex(4096);
    let (written, received) = tokio::sync::oneshot::channel();
    let (client, peer) = tokio::join!(
        async move {
            let mut client = QmpClient::negotiate(client_stream, OPERATION_TIMEOUT).await?;
            {
                let pause = client.pause();
                tokio::pin!(pause);
                tokio::select! {
                    _ = &mut pause => anyhow::bail!("peer did not return a command result"),
                    result = received => { result?; }
                }
            }
            ensure!(
                client.resume().await == Err(QmpError::ConnectionUnusable),
                "cancel reused connection"
            );
            Ok::<_, anyhow::Error>(())
        },
        async move {
            let mut peer = negotiate_peer(peer_stream).await?;
            ensure!(
                request(&mut peer).await?["execute"] == "stop",
                "missing effect request"
            );
            written
                .send(())
                .map_err(|_| anyhow::anyhow!("client disappeared"))?;
            let mut tail = String::new();
            ensure!(
                peer.read_line(&mut tail).await? == 0,
                "effect repeated after cancellation"
            );
            Ok::<_, anyhow::Error>(())
        }
    );
    client?;
    peer
}

#[tokio::test]
async fn invalid_greeting_never_sends_capabilities_or_effects() -> Result<()> {
    let (client_stream, peer_stream) = tokio::io::duplex(4096);
    let (client, peer) = tokio::join!(
        async move {
            let result = QmpClient::negotiate(client_stream, OPERATION_TIMEOUT).await;
            ensure!(
                matches!(result, Err(QmpError::InvalidGreeting)),
                "invalid greeting accepted"
            );
            Ok::<_, anyhow::Error>(())
        },
        async move {
            let mut peer = BufStream::new(peer_stream);
            frame(&mut peer, json!({"return":{}})).await?;
            let mut tail = String::new();
            ensure!(
                peer.read_line(&mut tail).await? == 0,
                "sent on unnegotiated connection"
            );
            Ok::<_, anyhow::Error>(())
        }
    );
    client?;
    peer
}

#[cfg(unix)]
#[tokio::test]
async fn private_local_socket_handshake_uses_no_network_endpoint() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("qmp.sock");
    let listener = tokio::net::UnixListener::bind(&path)?;
    let (client, peer) = tokio::join!(
        async {
            let mut client = QmpClient::connect_local(&path).await?;
            ensure!(!client.query_status().await?.running, "wrong local status");
            ensure!(
                matches!(
                    QmpClient::connect_local(std::path::Path::new("tcp://localhost:123")).await,
                    Err(QmpError::Unavailable)
                ),
                "accepted non-local endpoint"
            );
            Ok::<_, anyhow::Error>(())
        },
        async {
            let (stream, _) = listener.accept().await?;
            let mut peer = BufStream::new(stream);
            peer.write_all(GREETING.as_bytes()).await?;
            peer.flush().await?;
            let capabilities = request(&mut peer).await?;
            frame(&mut peer, json!({"return":{},"id":capabilities["id"]})).await?;
            let status = request(&mut peer).await?;
            frame(
                &mut peer,
                json!({"return":{"running":false,"status":"paused"},"id":status["id"]}),
            )
            .await?;
            Ok::<_, anyhow::Error>(())
        }
    );
    client?;
    peer
}

/// Real VMM/QMP/process-lifecycle evidence only. It boots no guest operating
/// system and proves neither hardware acceleration nor a worker desktop.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn real_qemu_process_pauses_resumes_and_exits_under_its_owner() -> Result<()> {
    use std::process::Stdio;
    use tokio::process::Command;

    let root = tempfile::tempdir()?;
    let socket = root.path().join("control.sock");
    let listener = tokio::net::UnixListener::bind(&socket)?;
    let stderr_path = root.path().join("qemu.stderr");
    let stderr = std::fs::File::create(&stderr_path)?;
    let mut child = Command::new("/usr/bin/qemu-system-x86_64")
        .args([
            "-machine",
            "pc",
            "-accel",
            "tcg,thread=single",
            "-m",
            "32",
            "-smp",
            "1",
            "-nodefaults",
            "-no-user-config",
            "-display",
            "none",
            "-serial",
            "none",
            "-monitor",
            "none",
            "-nic",
            "none",
            "-S",
        ])
        .arg("-chardev")
        .arg(format!(
            "socket,id=control,path={},server=off",
            socket.display()
        ))
        .args(["-mon", "chardev=control,mode=control"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .kill_on_drop(true)
        .spawn()?;
    eprintln!(
        "owned QEMU test: pid={:?}, output={}, stop=quit or test cleanup",
        child.id(),
        stderr_path.display()
    );
    let result = tokio::time::timeout(Duration::from_secs(15), async {
        let (stream, _) = listener.accept().await?;
        let mut qmp = QmpClient::negotiate(stream, OPERATION_TIMEOUT).await?;
        ensure!(
            !qmp.query_status().await?.running,
            "QEMU did not start paused"
        );
        qmp.resume().await?;
        ensure!(qmp.query_status().await?.running, "QEMU did not resume");
        qmp.pause().await?;
        ensure!(!qmp.query_status().await?.running, "QEMU did not pause");
        qmp.request_powerdown().await?;
        ensure!(
            child.try_wait()?.is_none(),
            "powerdown request unexpectedly reported process exit"
        );
        qmp.quit().await?;
        let status = child.wait().await?;
        ensure!(status.success(), "QEMU exited unsuccessfully");
        Ok::<_, anyhow::Error>(())
    })
    .await;
    // Always reap only this captured child, including timeout/error paths.
    if child.try_wait()?.is_none() {
        child.kill().await?;
    }
    child.wait().await?;
    result.map_err(|_| anyhow::anyhow!("owned QEMU test exceeded its deadline"))??;
    Ok(())
}
