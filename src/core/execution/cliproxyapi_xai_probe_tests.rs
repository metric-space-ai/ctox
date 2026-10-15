// Origin: CTOX
// License: AGPL-3.0-only
//! Real HTTP framing and the normal native router; mocked upstream, no live health claim.
use super::*;

async fn probe_wire(catalog_status: u16, status: u16, body: Vec<u8>) -> String {
    let root = tempfile::tempdir().unwrap();
    save_bundle(root.path(), &bundle()).unwrap();
    // Reuse existing registry fixture data without registering or inventing an ID.
    let registry: serde_json::Value = serde_json::from_slice(include_bytes!(
        "cliproxyapi/internal/registry/models/models.json"
    ))
    .unwrap();
    let model = registry["xai"][0]["id"].as_str().unwrap().to_owned();
    let catalog = serde_json::json!({"data":[{"id":model}]}).to_string();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    test_endpoint(
        root.path(),
        Some(format!("http://{}", listener.local_addr().unwrap())),
    );
    let upstream = tokio::spawn(async move {
        let mut replies = vec![("GET /models", catalog_status, catalog.into_bytes())];
        if catalog_status == 200 {
            replies.push(("POST /responses", status, body));
        }
        for (path, status, body) in replies {
            let (mut socket, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept())
                .await
                .unwrap()
                .unwrap();
            let mut request = vec![0; 16384];
            let n = socket.read(&mut request).await.unwrap();
            assert!(request[..n].starts_with(path.as_bytes()));
            socket.write_all(format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await.unwrap();
            socket.write_all(&body).await.unwrap();
        }
    });
    let router =
        crate::execution::cliproxyapi_host::build_instance_codex_responses_router(root.path())
            .unwrap()
            .unwrap();
    let (mut client, mut server) = tokio::io::duplex(131072);
    let dispatcher = tokio::spawn(async move {
        ctox_cliproxyapi::internal::api::server::serve_responses_connection(
            &mut server,
            router.as_ref(),
        )
        .await
        .unwrap();
    });
    let body = serde_json::json!({"model":model,"input":"Hi","stream":false}).to_string();
    client.write_all(format!("POST /v1/responses HTTP/1.1\r\nHost: localhost\r\nX-CTOX-Account: {ACCOUNT_ID}\r\nX-CTOX-Provider: xai\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    client.shutdown().await.unwrap();
    let mut response = vec![];
    tokio::time::timeout(Duration::from_secs(15), client.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    dispatcher.await.unwrap();
    upstream.await.unwrap();
    test_endpoint(root.path(), None);
    String::from_utf8(response).unwrap()
}
#[tokio::test]
async fn inference_status_and_closed_class_survive_native_grok_wrapper() {
    for (status, class, code) in [
        (401, "auth", "denied"),
        (403, "auth", "denied"),
        (429, "quota-rate-limit", "limited"),
        (404, "unknown-model", "model_not_found"),
        (503, "network-provider", "unavailable"),
    ] {
        let body =
            serde_json::json!({"error":{"code":code,"message":"private-upstream-body-fixture"}})
                .to_string()
                .into_bytes();
        let reply = probe_wire(200, status, body).await;
        assert!(reply.starts_with("HTTP/1.1 502"), "{reply}");
        assert!(
            reply.contains(&format!("X-CTOX-Upstream-Status: {status}\r\n")),
            "{reply}"
        );
        assert!(
            reply.contains(&format!("X-CTOX-Error-Class: {class}\r\n")),
            "{reply}"
        );
        assert!(reply.contains(&format!("X-CTOX-Account-Selected: {ACCOUNT_ID}\r\n")));
        assert!(
            !reply.contains("private-upstream-body-fixture") && !reply.contains("fixture-access")
        );
    }
}
#[tokio::test]
async fn catalog_failure_never_becomes_an_inference_witness() {
    let reply = probe_wire(401, 200, vec![]).await;
    assert!(reply.starts_with("HTTP/1.1 502"));
    assert!(!reply.contains("X-CTOX-Upstream-Status") && !reply.contains("X-CTOX-Error-Class"));
}
#[tokio::test]
async fn actual_success_and_incomplete_success_keep_distinct_gateway_results() {
    let event=b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"fixture-response\",\"output\":[]}}\n\n".to_vec();
    let reply = probe_wire(200, 200, event).await;
    assert!(reply.starts_with("HTTP/1.1 200"));
    assert!(reply.contains("X-CTOX-Upstream-Status: 200\r\n"));
    let reply = probe_wire(200, 200, b"incomplete-private-upstream-fixture".to_vec()).await;
    assert!(reply.starts_with("HTTP/1.1 502"));
    assert!(reply.contains("X-CTOX-Upstream-Status: 200\r\n"));
    assert!(!reply.contains("incomplete-private-upstream-fixture"));
}
