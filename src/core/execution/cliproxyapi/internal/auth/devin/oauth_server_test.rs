// ref: internal/auth/devin/devin_auth_test.go:220-260
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn request(addr: SocketAddr, method: &str, target: &str) -> String {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let head =
            format!("{method} {target} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
        stream.write_all(head.as_bytes()).await.unwrap();
        let mut body = Vec::new();
        stream.read_to_end(&mut body).await.unwrap();
        String::from_utf8(body).unwrap()
    })
    .await
    .expect("bounded local callback request")
}
async fn assert_listener_closed(addr: SocketAddr) {
    assert!(
        TcpStream::connect(addr).await.is_err(),
        "consumed listener must be closed"
    );
    let rebound = TcpListener::bind(addr)
        .await
        .expect("same port can be reused");
    drop(rebound);
}
