use super::*;
#[test]
fn founder_escalation_chat_receipts_keep_their_existing_send_semantics() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE communication_founder_reply_reviews (
            inbound_message_key TEXT, sent_at TEXT, reviewer TEXT, send_result_json TEXT);
         CREATE TABLE communication_routing_state (
            message_key TEXT, route_status TEXT, updated_at TEXT);
         INSERT INTO communication_founder_reply_reviews VALUES (
            'slack:workspace::thread::1', '2026-10-13T11:40:00Z',
            'policy-escalation', '{\"ok\":true,\"status\":\"sent\"}');",
    )
    .unwrap();
    assert!(founder_reply_sent_after_review(&conn, "slack:workspace::thread::1").unwrap());
}

#[test]
fn founder_escalation_acceptance_waits_for_its_own_sent_copy() {
    let root = std::env::temp_dir().join(format!(
        "ctox-escalation-confirmation-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let db_path = resolve_db_path(&root, None);
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let conn = open_channel_db(&db_path).unwrap();
    let inbound_key = "email:lena@example.test::inbox::1";
    let settings = std::collections::BTreeMap::from([(
        "CTOX_OWNER_EMAIL_ADDRESS".to_string(),
        "founder@example.test".to_string(),
    )]);
    crate::inference::runtime_env::save_runtime_env_map(&root, &settings).unwrap();
    conn.execute(
        "INSERT INTO communication_messages (
            message_key, channel, account_key, thread_key, remote_id, direction, folder_hint,
            sender_display, sender_address, recipient_addresses_json, cc_addresses_json,
            bcc_addresses_json, subject, preview, body_text, body_html, raw_payload_ref,
            trust_level, status, seen, has_attachments, external_created_at, observed_at, metadata_json
         ) VALUES (?1, 'email', 'email:lena@example.test', 'email:lena@example.test::thread::1',
            'inbound-1', 'inbound', 'inbox', 'Founder', 'founder@example.test', '[]', '[]',
            '[]', 'Example', 'Example', 'Example', '', '', 'normal', 'received', 0, 0,
            '2026-10-13T11:39:00Z', '2026-10-13T11:39:00Z', '{}')",
        [inbound_key]
    ).unwrap();
    conn.execute(
        "INSERT INTO communication_routing_state (
            message_key, route_status, lease_owner, leased_at, updated_at)
         VALUES (?1, 'leased', 'ctox-service', '2026-10-13T11:39:00Z', '2026-10-13T11:39:00Z')",
        [inbound_key],
    )
    .unwrap();
    let request = ChannelSendRequest {
        channel: "email".to_string(),
        account_key: "email:lena@example.test".to_string(),
        thread_key: "email:lena@example.test::thread::1".to_string(),
        body: "The notice awaits confirmation.".to_string(),
        subject: "Re: Example".to_string(),
        to: vec!["founder@example.test".to_string()],
        cc: Vec::new(),
        attachments: Vec::new(),
        sender_display: None,
        sender_address: None,
        send_voice: false,
        reviewed_founder_send: true,
    };
    let reservation =
        record_outbound_pending_send(&conn, &request, "approval", "body-digest").unwrap();
    let message_key = reservation.message_key;
    let adapter = json!({"ok": true, "status": "accepted", "delivery": {"confirmed": false}});
    mark_outbound_send_accepted(&conn, &message_key, "accepted", &adapter).unwrap();
    conn.execute(
        "INSERT INTO communication_founder_reply_reviews (
            approval_key, inbound_message_key, action_digest, action_json, body_sha256,
            reviewer, review_summary, approved_at
        ) VALUES ('approval', ?1, 'digest', '{}', 'body-digest',
            'policy-escalation', 'notice submitted', '2026-10-13T11:40:00Z')",
        [inbound_key],
    )
    .unwrap();
    let receipt = json!({
        "ok": true, "status": "accepted", "message_key": message_key,
        "delivery_confirmed": false
    });
    mark_founder_reply_review_sent(&conn, "approval", &receipt).unwrap();

    // Repeated sweeps retain exactly one consumed approval and send reservation.
    for _ in 0..3 {
        assert!(!founder_reply_sent_after_review(&conn, inbound_key).unwrap());
        let reused = consumed_founder_escalation_result(&conn, "approval")
            .unwrap()
            .unwrap();
        assert_eq!(reused, receipt);
        assert!(
            super::super::ack_leased_messages(&root, &[inbound_key.to_string()], "handled")
                .is_err()
        );
        assert!(
            super::super::handle_inbound_after_reviewed_founder_reply(&root, inbound_key).is_err()
        );
        let route: String = conn
            .query_row(
                "SELECT route_status FROM communication_routing_state WHERE message_key=?1",
                [inbound_key],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(route, "leased");
        assert!(
            record_outbound_pending_send(&conn, &request, "approval", "body-digest")
                .unwrap()
                .existing_result
                .is_some()
        );
    }
    let counts: (i64, i64) = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM communication_messages),
                (SELECT COUNT(*) FROM communication_founder_reply_reviews WHERE sent_at IS NOT NULL)",
        [], |row| Ok((row.get(0)?, row.get(1)?))
    ).unwrap();
    assert_eq!(counts, (2, 1));

    // An unrelated Sent copy cannot prove this notice was delivered.
    let other = ChannelSendRequest {
        body: "A different notice.".to_string(),
        ..request
    };
    let other_key = record_outbound_pending_send(&conn, &other, "other-approval", "other-body")
        .unwrap()
        .message_key;
    mark_outbound_send_accepted(
        &conn,
        &other_key,
        "confirmed",
        &json!({"ok": true, "delivery": {"confirmed": true}}),
    )
    .unwrap();
    assert!(!founder_reply_sent_after_review(&conn, inbound_key).unwrap());

    // The existing native Sent-copy reconciliation updates the exact outbox
    // artifact, not the consumed review receipt.
    conn.execute(
        r#"UPDATE communication_messages SET status='confirmed',
         metadata_json=json_set(metadata_json, '$.sentCopyConfirmation',
             json('{"remoteId":"provider-sent-1"}')) WHERE message_key=?1"#,
        [&message_key],
    )
    .unwrap();
    assert!(founder_reply_sent_after_review(&conn, inbound_key).unwrap());
    let confirmed = consumed_founder_escalation_result(&conn, "approval")
        .unwrap()
        .unwrap();
    assert_eq!(confirmed["message_key"], receipt["message_key"]);
    assert_eq!(confirmed["delivery_confirmed"], true);
    assert_eq!(
        super::super::handle_inbound_after_reviewed_founder_reply(&root, inbound_key).unwrap(),
        1
    );
    assert_eq!(
        super::super::handle_inbound_after_reviewed_founder_reply(&root, inbound_key).unwrap(),
        0
    );
    let route: String = conn
        .query_row(
            "SELECT route_status FROM communication_routing_state WHERE message_key=?1",
            [inbound_key],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(route, "handled");
    let stored: String = conn.query_row(
        "SELECT send_result_json FROM communication_founder_reply_reviews WHERE approval_key='approval'",
        [], |row| row.get(0)
    ).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&stored).unwrap(), receipt);
    drop(conn);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn founder_escalation_requires_explicit_delivery_evidence() {
    let conn = Connection::open_in_memory().unwrap();
    assert!(!founder_escalation_delivery_confirmed(
        &conn,
        &json!({"ok": true, "status": "accepted"})
    )
    .unwrap());
    assert!(founder_escalation_delivery_confirmed(
        &conn,
        &json!({"ok": true, "status": "accepted", "delivery_confirmed": true})
    )
    .unwrap());
    assert!(!founder_escalation_delivery_confirmed(
        &conn,
        &json!({"ok": false, "status": "failed", "delivery_confirmed": true})
    )
    .unwrap());
}
