use super::*;
use serde_json::json;

fn fixture(label: &str) -> (std::path::PathBuf, channels::RoutedInboundMessage) {
    let root = std::env::temp_dir().join(format!(
        "ctox-founder-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let message = channels::RoutedInboundMessage {
        message_key: "email:lena@example.test::inbox::budget".to_string(),
        channel: "email".to_string(),
        account_key: "email:lena@example.test".to_string(),
        thread_key: "email:lena@example.test::thread::budget".to_string(),
        sender_display: "Founder".to_string(),
        sender_address: "founder@example.test".to_string(),
        subject: "Example".to_string(),
        preview: "Example".to_string(),
        body_text: "Example".to_string(),
        external_created_at: "2026-10-12T10:00:00Z".to_string(),
        workspace_root: None,
        metadata: json!({}),
        preferred_reply_modality: None,
    };
    (root, message)
}

#[test]
fn blocked_founder_rework_needs_durable_attempts_before_budget_exhaustion() {
    let (root, message) = fixture("blocked-budget");
    let item = tickets::put_ticket_self_work_item(
        &root,
        tickets::TicketSelfWorkUpsertInput {
            source_system: "local".to_string(),
            kind: FOUNDER_COMMUNICATION_REWORK_KIND.to_string(),
            title: "Founder communication rework: Example".to_string(),
            body_text: "Review required.".to_string(),
            state: "blocked".to_string(),
            metadata: json!({
                "inbound_message_key": message.message_key,
                "parent_message_key": message.message_key,
                "thread_key": message.thread_key,
            }),
        },
        false,
    )
    .unwrap();
    assert!(!founder_communication_review_budget_exhausted(&root, &message).unwrap());
    for attempt in 1..=FOUNDER_REWORK_REQUEUE_BLOCK_THRESHOLD {
        channels::create_queue_task_with_metadata(
            &root,
            channels::QueueTaskCreateRequest {
                title: "Founder communication rework: Example".to_string(),
                prompt: format!("Reserved rework attempt {attempt}"),
                thread_key: message.thread_key.clone(),
                workspace_root: None,
                priority: "urgent".to_string(),
                suggested_skill: None,
                parent_message_key: Some(message.message_key.clone()),
                extra_metadata: Some(json!({
                    "ticket_self_work_id": item.work_id,
                    "ticket_self_work_kind": FOUNDER_COMMUNICATION_REWORK_KIND
                })),
            },
        )
        .unwrap();
        assert_eq!(
            founder_rework_queue_attempt_count(&root, &item.work_id).unwrap(),
            attempt
        );
        assert_eq!(
            founder_communication_review_budget_exhausted(&root, &message).unwrap(),
            attempt >= FOUNDER_REWORK_REQUEUE_BLOCK_THRESHOLD
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn policy_founder_notice_cannot_supersede_a_thread_or_subject_request() {
    let (root, message) = fixture("policy-supersession");
    let conn = channels::open_channel_db(&crate::paths::core_db(&root)).unwrap();
    let action = json!({"thread_key": message.thread_key, "subject": "Re: Example",
        "to": [message.sender_address]});
    conn.execute(
        "INSERT INTO communication_founder_reply_reviews (
            approval_key, inbound_message_key, action_digest, action_json, body_sha256,
            reviewer, review_summary, approved_at, sent_at, send_result_json)
         VALUES ('legacy-policy', ?1, 'legacy-digest', ?2, 'legacy-body', 'policy-escalation',
            'legacy notice', ?3, ?3, '{\"ok\":true,\"delivery_confirmed\":true}')",
        params![
            message.message_key,
            action.to_string(),
            channels::now_iso_string()
        ],
    )
    .unwrap();
    assert!(!founder_thread_has_later_reviewed_send(&root, &message).unwrap());
    assert!(!founder_subject_has_later_reviewed_send_to_sender(&root, &message).unwrap());
    conn.execute(
        "UPDATE communication_founder_reply_reviews SET reviewer='external-review'",
        [],
    )
    .unwrap();
    assert!(founder_thread_has_later_reviewed_send(&root, &message).unwrap());
    assert!(founder_subject_has_later_reviewed_send_to_sender(&root, &message).unwrap());
    drop(conn);
    std::fs::remove_dir_all(root).unwrap();
}
