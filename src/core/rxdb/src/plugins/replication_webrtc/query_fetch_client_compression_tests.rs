// Included in query_fetch_client_tests; drive the consumer through actual frames.
#[tokio::test]
async fn query_page_compression_rejects_truncation_trailing_bytes_and_expansion_overflow() {
    for scenario in ["valid", "truncated", "trailing", "overflow"] {
        let (handler, pool, peer, task, id) = start().await;
        let text = if scenario == "overflow" {
            "x".repeat(300_000)
        } else {
            "x".repeat(5000)
        };
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(&serde_json::to_vec(&json!([{"id":"document", "text":text}])).unwrap())
            .unwrap();
        let mut compressed = encoder.finish().unwrap();
        if scenario == "truncated" {
            compressed.truncate(compressed.len() / 2);
        }
        if scenario == "trailing" {
            compressed.extend_from_slice(b"trailing");
        }
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(compressed);
        ack(&handler, &peer, &id);
        handler.message.next(PeerWithMessage {
            peer,
            message: WebRTCMessage {
                id: format!("{id}-chunk"),
                method: "rxdb.query.chunk".into(),
                collection: None,
                params: vec![json!({"requestId":id,"sequence":0,"complete":true,
                "compressed":"deflate", "compressedBase64":encoded})],
            },
        });
        let result = tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap();
        match scenario {
            "valid" => assert_eq!(
                result.unwrap().documents[0]["text"].as_str().unwrap().len(),
                5000
            ),
            "overflow" => assert_eq!(
                result.unwrap_err().parameters()["reason"],
                "chunk_too_large"
            ),
            _ => assert_eq!(
                result.unwrap_err().parameters()["reason"],
                "invalid_compression"
            ),
        }
        pool.cancel().await;
    }
}
