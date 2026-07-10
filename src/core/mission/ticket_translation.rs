use anyhow::Result;
use serde::Serialize;
use std::path::Path;

use crate::mission::ticket_protocol::TicketSyncBatch;
use crate::mission::tickets::ensure_ticket_source_control_for_sync;
use crate::mission::tickets::load_ticket_source_control;
use crate::mission::tickets::record_ticket_sync_run;
use crate::mission::tickets::upsert_ticket_event_from_adapter;
use crate::mission::tickets::upsert_ticket_from_adapter;
use crate::mission::tickets::AdapterTicketEventRequest;
use crate::mission::tickets::AdapterTicketMirrorRequest;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct TicketSyncApplyResult {
    pub system: String,
    pub fetched_count: usize,
    pub stored_ticket_count: usize,
    pub stored_event_count: usize,
    pub source_control: serde_json::Value,
}

pub(crate) fn apply_ticket_sync_batch(
    root: &Path,
    batch: &TicketSyncBatch,
) -> Result<TicketSyncApplyResult> {
    let control = ensure_ticket_source_control_for_sync(root, batch)?;
    let mut stored_ticket_count = 0usize;
    let mut stored_event_count = 0usize;
    for ticket in &batch.tickets {
        let request = AdapterTicketMirrorRequest {
            system: &batch.system,
            remote_ticket_id: &ticket.remote_ticket_id,
            title: &ticket.title,
            body_text: &ticket.body_text,
            remote_status: &ticket.remote_status,
            priority: ticket.priority.as_deref(),
            requester: ticket.requester.as_deref(),
            metadata: ticket.metadata.clone(),
            external_created_at: &ticket.external_created_at,
            external_updated_at: &ticket.external_updated_at,
        };
        if upsert_ticket_from_adapter(root, request)?.changed {
            stored_ticket_count += 1;
        }
    }
    for event in &batch.events {
        let request = AdapterTicketEventRequest {
            system: &batch.system,
            remote_ticket_id: &event.remote_ticket_id,
            remote_event_id: &event.remote_event_id,
            direction: &event.direction,
            event_type: &event.event_type,
            summary: &event.summary,
            body_text: &event.body_text,
            metadata: event.metadata.clone(),
            external_created_at: &event.external_created_at,
        };
        if upsert_ticket_event_from_adapter(root, request)?.changed {
            stored_event_count += 1;
        }
    }
    if stored_ticket_count > 0 || stored_event_count > 0 {
        record_ticket_sync_run(
            root,
            &batch.system,
            batch.fetched_ticket_count,
            stored_ticket_count,
            stored_event_count,
        )?;
    }
    Ok(TicketSyncApplyResult {
        system: batch.system.clone(),
        fetched_count: batch.fetched_ticket_count,
        stored_ticket_count,
        stored_event_count,
        source_control: serde_json::to_value(
            load_ticket_source_control(root, &batch.system)?.unwrap_or(control),
        )?,
    })
}

#[cfg(test)]
mod tests {
    // ctox-allow-direct-state-write: this module deliberately writes legacy
    // routing states to prove sync ingestion preserves progressed fixtures.
    use super::apply_ticket_sync_batch;
    use crate::mission::ticket_protocol::TicketEventRecord;
    use crate::mission::ticket_protocol::TicketMirrorRecord;
    use crate::mission::ticket_protocol::TicketSyncBatch;
    use rusqlite::{params, Connection};
    use serde_json::json;
    use std::fs;

    #[test]
    fn canonical_sync_batch_persists_through_translation_layer() -> anyhow::Result<()> {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "ctox-ticket-translation-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(root.join("runtime"))?;

        let batch = TicketSyncBatch {
            system: "example".to_string(),
            fetched_ticket_count: 1,
            tickets: vec![TicketMirrorRecord {
                remote_ticket_id: "T-100".to_string(),
                title: "VPN outage".to_string(),
                body_text: "User cannot connect".to_string(),
                remote_status: "open".to_string(),
                priority: Some("high".to_string()),
                requester: Some("alice".to_string()),
                metadata: json!({"group":"support"}),
                external_created_at: "2026-04-09T10:00:00Z".to_string(),
                external_updated_at: "2026-04-09T10:05:00Z".to_string(),
            }],
            events: vec![TicketEventRecord {
                remote_ticket_id: "T-100".to_string(),
                remote_event_id: "E-100".to_string(),
                direction: "inbound".to_string(),
                event_type: "comment".to_string(),
                summary: "Initial request".to_string(),
                body_text: "VPN still down".to_string(),
                metadata: json!({"channel":"email"}),
                external_created_at: "2026-04-09T10:01:00Z".to_string(),
            }],
            metadata: json!({"adapter":"example"}),
        };

        let result = apply_ticket_sync_batch(&root, &batch)?;
        assert_eq!(result.system, "example");
        assert_eq!(result.fetched_count, 1);
        assert_eq!(result.stored_ticket_count, 1);
        assert_eq!(result.stored_event_count, 1);
        assert_eq!(
            result
                .source_control
                .get("adoption_mode")
                .and_then(serde_json::Value::as_str),
            Some("baseline_observe_only")
        );

        let conn = Connection::open(root.join("runtime/ctox.sqlite3"))?;
        let ticket_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM ticket_items", [], |row| row.get(0))?;
        let event_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM ticket_events", [], |row| row.get(0))?;
        let sync_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM ticket_sync_runs", [], |row| {
                row.get(0)
            })?;
        assert_eq!(ticket_count, 1);
        assert_eq!(event_count, 1);
        assert_eq!(sync_count, 1);
        drop(conn);

        let second = apply_ticket_sync_batch(&root, &batch)?;
        assert_eq!(second.fetched_count, 1);
        assert_eq!(second.stored_ticket_count, 0);
        assert_eq!(second.stored_event_count, 0);

        let conn = Connection::open(root.join("runtime/ctox.sqlite3"))?;
        let sync_count_after_noop: i64 =
            conn.query_row("SELECT COUNT(*) FROM ticket_sync_runs", [], |row| {
                row.get(0)
            })?;
        assert_eq!(
            sync_count_after_noop, 1,
            "unchanged ticket sync batches must not dirty the DB with sync-run heartbeat rows"
        );

        drop(conn);

        let event_key = "example:E-100";
        for (idx, progressed_status) in ["leased", "handled", "blocked", "failed"]
            .into_iter()
            .enumerate()
        {
            let conn = Connection::open(root.join("runtime/ctox.sqlite3"))?;
            let marker = format!("2026-04-09T11:00:0{idx}Z");
            let lease_owner = (progressed_status == "leased").then_some("ticket-test");
            let leased_at = (progressed_status == "leased").then_some(marker.as_str());
            let acked_at = (progressed_status == "handled").then_some(marker.as_str());
            conn.execute(
                r#"
                UPDATE ticket_event_routing_state
                SET route_status = ?1,
                    lease_owner = ?2,
                    leased_at = ?3,
                    acked_at = ?4,
                    updated_at = ?5
                WHERE event_key = ?6
                "#,
                params![
                    progressed_status,
                    lease_owner,
                    leased_at,
                    acked_at,
                    marker,
                    event_key
                ],
            )?;
            drop(conn);

            let progressed_noop = apply_ticket_sync_batch(&root, &batch)?;
            assert_eq!(progressed_noop.stored_ticket_count, 0);
            assert_eq!(
                progressed_noop.stored_event_count, 0,
                "unchanged event payload must not reset progressed routing state {progressed_status}"
            );

            let conn = Connection::open(root.join("runtime/ctox.sqlite3"))?;
            let sync_count_after_progressed_noop: i64 =
                conn.query_row("SELECT COUNT(*) FROM ticket_sync_runs", [], |row| {
                    row.get(0)
                })?;
            assert_eq!(
                sync_count_after_progressed_noop, 1,
                "unchanged progressed events must not append sync-run rows"
            );
            let (route_status, updated_at): (String, String) = conn.query_row(
                "SELECT route_status, updated_at FROM ticket_event_routing_state WHERE event_key = ?1",
                [event_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            assert_eq!(route_status, progressed_status);
            assert_eq!(
                updated_at, marker,
                "unchanged progressed events must not update routing timestamps"
            );
        }

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }
}
