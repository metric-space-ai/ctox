// ref: internal/auth/claude/utls_transport_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::time::Duration;

use std::sync::Arc;

use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{HandshakeKind, ServerConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio_rustls::TlsAcceptor;

use super::anthropic_auth::{ClaudeRefreshTransport, RefreshRequest, RefreshTransportFailure};
use super::token::SecretString;
use wreq::tls::{ExtensionType, KeyShare, TlsVersion};

use super::utls_transport::{
    build_anthropic_loopback_test_client, claude_oauth_tls_options, proxy_session_cache_count,
    AnthropicHttpTransport, CLAUDE_OAUTH_INSPECT_HEADER_ORDER, CLAUDE_OAUTH_REFRESH_HEADER_ORDER,
};
use wreq::tls::session::{LruTlsSessionCache, TlsSessionCache};

#[test]
fn candidate_oauth_session_cache_is_shared_per_proxy_and_bounded() {
    let first = AnthropicHttpTransport::new(Some("http://127.0.0.1:31000")).unwrap();
    let same = AnthropicHttpTransport::new(Some("http://127.0.0.1:31000")).unwrap();
    let other = AnthropicHttpTransport::new(Some("http://127.0.0.1:31001")).unwrap();
    assert_eq!(first.session_cache_id(), same.session_cache_id());
    assert_ne!(first.session_cache_id(), other.session_cache_id());

    for port in 31_002..31_072 {
        AnthropicHttpTransport::new(Some(&format!("http://127.0.0.1:{port}"))).unwrap();
    }
    assert!(proxy_session_cache_count() <= 64);
}

// Disposition: adapted loopback port of TestUtlsRoundTripperBoundsTLSHandshake.
// Go injects a 20 ms handshake timeout through a private context key. Rust's
// wreq transport bounds connect/TLS setup and the complete operation; this gate
// stalls a real loopback TLS handshake and proves the supplied operation bound
// terminates it without waiting for the peer.
#[tokio::test]
async fn anthropic_transport_bounds_stalled_tls_handshake() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("https://{}/v1/oauth/token", listener.local_addr().unwrap());
    let (handshake_started_tx, handshake_started_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut client_hello = [0_u8; 1024];
        let read = socket.read(&mut client_hello).await.unwrap();
        assert!(read > 0, "client must begin the TLS handshake");
        let _ = handshake_started_tx.send(());
        let _ = release_rx.await;
    });

    let transport = AnthropicHttpTransport::with_endpoint(&endpoint, None).unwrap();
    let request = RefreshRequest::new(SecretString::new("tls-timeout-probe").unwrap());
    let started_at = tokio::time::Instant::now();
    let pending = tokio::spawn(async move {
        transport
            .execute(&request, Duration::from_millis(100))
            .await
    });
    handshake_started_rx.await.unwrap();
    let error = pending.await.unwrap().unwrap_err();
    let elapsed = started_at.elapsed();
    let _ = release_tx.send(());
    server.await.unwrap();

    assert_eq!(error, RefreshTransportFailure::Timeout);
    assert!(elapsed >= Duration::from_millis(50));
    assert!(elapsed < Duration::from_secs(1));
}

#[tokio::test]
async fn candidate_oauth_session_cache_completes_real_tls13_resumption() {
    let certified = generate_simple_self_signed(["localhost".to_owned()]).unwrap();
    let certificate = CertificateDer::from(certified.cert.der().to_vec());
    let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
        certified.signing_key.serialize_der(),
    ));
    let server = Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certificate], private_key)
            .unwrap(),
    );
    let acceptor = TlsAcceptor::from(server);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!(
        "https://localhost:{}/probe",
        listener.local_addr().unwrap().port()
    );
    let (kind_tx, mut kind_rx) = mpsc::channel(2);
    let server_task = tokio::spawn(async move {
        for _ in 0..2 {
            let (socket, _) = listener.accept().await.unwrap();
            let mut tls = acceptor.accept(socket).await.unwrap();
            kind_tx
                .send(tls.get_ref().1.handshake_kind().unwrap())
                .await
                .unwrap();

            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = tls.read(&mut buffer).await.unwrap();
                assert!(read > 0, "HTTP request ended before the header terminator");
                request.extend_from_slice(&buffer[..read]);
            }
            tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();
            tls.shutdown().await.unwrap();
        }
    });

    let session_cache: Arc<dyn TlsSessionCache> = Arc::new(LruTlsSessionCache::new(8));
    let client = build_anthropic_loopback_test_client(session_cache).unwrap();
    for _ in 0..2 {
        let response = client.get(&endpoint).send().await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.bytes().await.unwrap().as_ref(), b"ok");
    }
    server_task.await.unwrap();

    assert_eq!(kind_rx.recv().await, Some(HandshakeKind::Full));
    assert_eq!(kind_rx.recv().await, Some(HandshakeKind::Resumed));
}
