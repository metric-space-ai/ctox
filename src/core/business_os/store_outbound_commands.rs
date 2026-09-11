// Origin: CTOX
// License: Apache-2.0

use super::backup_restore::file_sha256;
use super::store::{
    find_rxdb_collection_record_by_string_field, find_rxdb_collection_records_by_string_field,
    find_rxdb_collection_records_by_string_field_contains, insert_business_event,
    is_safe_rxdb_collection_name, load_rxdb_collection_record, now_ms, open_store,
    outbound_first_string, outbound_id_from_command, outbound_load_record,
    outbound_load_records_by_string_field, outbound_load_required, outbound_merge_fields,
    outbound_object_payload, outbound_put_default_i64, outbound_put_default_object,
    outbound_put_default_string, outbound_put_i64, outbound_put_string,
    outbound_required_from_payload_or_record, outbound_required_string, outbound_session_actor_id,
    outbound_string, runtime_app_starter_collection_name, session_audit_actor_context,
    upsert_rxdb_collection_record, BusinessCommand, BusinessOsSession,
};
use super::store_outbound_delivery_policy::*;
use super::store_projections::upsert_business_record;
use crate::capabilities::scrape;
use crate::mission::channels;
use anyhow::Context;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use url::Url;
use uuid::Uuid;

pub(super) fn handle_outbound_active_command(
    root: &Path,
    session: &BusinessOsSession,
    _command_id: &str,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    // The outbound-lead-generation app publishes its research policy under its
    // own module id; every other active outbound command stays on `outbound`.
    anyhow::ensure!(
        command.module == "outbound"
            || (command.command_type == "outbound.research_policy.publish"
                && command.module == "outbound-lead-generation"),
        "active outbound commands require module=outbound"
    );
    let now = now_ms() as i64;
    let conn = open_store(root)?;
    match command.command_type.as_str() {
        // Writeback target for the LLM-generated outreach draft. The CTOX agent
        // (running the `outbound.pipeline.outreach_draft` mission-queue task)
        // calls this command to persist the generated subject/body/follow-ups
        // back into the pipeline item's contact. The whole loop stays on the
        // RxDB command bus — there is no external email gateway.
        "outbound.pipeline.write_outreach_draft" => {
            let pipeline_id = outbound_required_from_payload_or_record(
                command,
                &["pipeline_id", "id"],
                "pipeline_id is required",
            )?;
            let contact_index = command
                .payload
                .get("contact_index")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            anyhow::ensure!(contact_index >= 0, "contact_index must be >= 0");
            let messages = command
                .payload
                .get("messages")
                .filter(|value| value.is_object())
                .cloned()
                .context("messages object is required")?;
            let mut item = outbound_load_required(
                &conn,
                "outbound_pipeline_items",
                &pipeline_id,
                "pipeline item not found",
            )?;
            let idx = contact_index as usize;
            {
                let contacts = item
                    .get_mut("contacts")
                    .and_then(Value::as_array_mut)
                    .context("pipeline item has no contacts array")?;
                anyhow::ensure!(idx < contacts.len(), "contact_index out of range");
                let contact = &mut contacts[idx];
                if !contact.is_object() {
                    *contact = serde_json::json!({});
                }
                let contact_obj = contact
                    .as_object_mut()
                    .context("pipeline contact is not an object")?;
                let target = contact_obj
                    .entry("messages")
                    .or_insert_with(|| serde_json::json!({}));
                if !target.is_object() {
                    *target = serde_json::json!({});
                }
                let target_obj = target
                    .as_object_mut()
                    .context("contact messages is not an object")?;
                for key in [
                    "message_mail_subject",
                    "message_mail_body",
                    "message_followup_1",
                    "message_followup_2",
                ] {
                    if let Some(value) = messages.get(key).and_then(Value::as_str) {
                        target_obj.insert(key.to_string(), Value::String(value.to_string()));
                    }
                }
                // Clear the generating flag so the UI spinner resolves via sync.
                contact_obj.insert("outreach_generating".to_string(), Value::Bool(false));
                contact_obj.insert(
                    "outreach_status".to_string(),
                    Value::String("drafted".to_string()),
                );
            }
            outbound_put_i64(&mut item, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_pipeline_items",
                &pipeline_id,
                now,
                item.clone(),
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "collection": "outbound_pipeline_items",
                "pipeline_id": pipeline_id,
                "contact_index": contact_index,
                "messages": messages
            }))
        }
        "outbound.engagement.create" => {
            let engagement_id = outbound_id_from_command(command, &["engagement_id", "id"], "eng")?;
            let campaign_id = outbound_required_string(&command.payload, &["campaign_id"])?;
            let mut record = outbound_object_payload(&command.payload);
            outbound_put_string(&mut record, "id", engagement_id.clone());
            outbound_put_string(&mut record, "campaign_id", campaign_id);
            outbound_put_default_string(&mut record, "status", "ready_for_assignment");
            outbound_put_default_object(&mut record, "payload");
            outbound_put_default_i64(&mut record, "created_at_ms", now);
            outbound_put_i64(&mut record, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_engagements",
                &engagement_id,
                now,
                record.clone(),
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "collection": "outbound_engagements",
                "engagement": record
            }))
        }
        "outbound.engagement.assign_sender" => {
            let engagement_id = outbound_required_from_payload_or_record(
                command,
                &["engagement_id", "id"],
                "engagement_id is required",
            )?;
            let sender_account_id =
                outbound_required_string(&command.payload, &["sender_account_id"])?;
            let mut engagement = outbound_load_required(
                &conn,
                "outbound_engagements",
                &engagement_id,
                "engagement not found",
            )?;
            outbound_put_string(
                &mut engagement,
                "sender_account_id",
                sender_account_id.clone(),
            );
            outbound_put_string(&mut engagement, "status", "assigned".to_string());
            outbound_put_i64(&mut engagement, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_engagements",
                &engagement_id,
                now,
                engagement.clone(),
            )?;

            let assignment_id = outbound_id_from_command(command, &["assignment_id"], "assign")
                .unwrap_or_else(|_| format!("assign_{engagement_id}_{sender_account_id}"));
            let mut assignment = outbound_object_payload(&command.payload);
            outbound_put_string(&mut assignment, "id", assignment_id.clone());
            outbound_put_string(&mut assignment, "engagement_id", engagement_id.clone());
            outbound_put_string(&mut assignment, "sender_account_id", sender_account_id);
            outbound_put_default_string(&mut assignment, "status", "active");
            outbound_put_default_i64(&mut assignment, "created_at_ms", now);
            outbound_put_i64(&mut assignment, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_sender_assignments",
                &assignment_id,
                now,
                assignment.clone(),
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "engagement": engagement,
                "assignment": assignment
            }))
        }
        "outbound.sequence.save" => {
            let sequence_id = outbound_id_from_command(command, &["sequence_id", "id"], "seq")?;
            let campaign_id = outbound_required_string(&command.payload, &["campaign_id"])?;
            let mut sequence = outbound_object_payload(&command.payload);
            outbound_put_string(&mut sequence, "id", sequence_id.clone());
            outbound_put_string(&mut sequence, "campaign_id", campaign_id);
            outbound_put_default_string(&mut sequence, "name", "Outbound Sequence");
            outbound_put_default_string(&mut sequence, "strategy_text", "");
            outbound_put_default_string(&mut sequence, "sequence_policy_text", "");
            outbound_put_default_object(&mut sequence, "approval_policy");
            outbound_put_default_object(&mut sequence, "payload");
            outbound_put_default_i64(&mut sequence, "created_at_ms", now);
            outbound_put_i64(&mut sequence, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_sequences",
                &sequence_id,
                now,
                sequence.clone(),
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "collection": "outbound_sequences",
                "sequence": sequence
            }))
        }
        "outbound.draft.prepare" => {
            let message_id = outbound_id_from_command(command, &["message_id", "id"], "msg")?;
            let engagement_id = outbound_required_string(&command.payload, &["engagement_id"])?;
            let engagement = outbound_load_required(
                &conn,
                "outbound_engagements",
                &engagement_id,
                "engagement not found",
            )?;
            anyhow::ensure!(
                !matches!(
                    outbound_string(&engagement, &["status"]).as_deref(),
                    Some("closed" | "cancelled" | "meeting_booked")
                ),
                "closed outbound engagements cannot prepare new drafts"
            );
            let draft_kind = outbound_string(&command.payload, &["draft_kind"])
                .unwrap_or_else(|| "followup".to_string());
            let campaign_id = outbound_first_string(&[
                outbound_string(&command.payload, &["campaign_id"]),
                outbound_string(&engagement, &["campaign_id"]),
            ])
            .context("campaign_id is required")?;
            // Resolve the channel up front so the rest of the draft prep can branch on it.
            let channel = outbound_first_string(&[
                outbound_string(&command.payload, &["channel"]),
                outbound_string(&engagement, &["payload", "channel"]),
                outbound_string(
                    &engagement,
                    &["payload", "active_outreach", "default_channel"],
                ),
                outbound_string(&engagement, &["channel"]),
            ])
            .unwrap_or_else(|| "email".to_string());

            let (sender_account_id, recipient_email, recipient_address_text) =
                if channel == "physical_letter" {
                    // Physical letters do not need a sender mailbox or an email address.
                    let address = outbound_first_string(&[
                        outbound_string(&command.payload, &["recipient_address_text"]),
                        outbound_string(&command.payload, &["recipient_address"]),
                        outbound_string(&engagement, &["payload", "contact_address_text"]),
                        outbound_string(&engagement, &["payload", "recipient_address_text"]),
                    ])
                    .context("recipient_address_text is required for physical_letter drafts")?;
                    let sender = outbound_first_string(&[
                        outbound_string(&command.payload, &["sender_account_id"]),
                        outbound_string(&engagement, &["sender_account_id"]),
                    ])
                    .unwrap_or_default();
                    let email = outbound_first_string(&[
                        outbound_string(&command.payload, &["recipient_email"]),
                        outbound_string(&engagement, &["payload", "contact_email"]),
                    ])
                    .unwrap_or_default();
                    (sender, email, address)
                } else {
                    let sender = outbound_first_string(&[
                        outbound_string(&command.payload, &["sender_account_id"]),
                        outbound_string(&engagement, &["sender_account_id"]),
                    ])
                    .context("sender_account_id is required")?;
                    let email = outbound_first_string(&[
                        outbound_string(&command.payload, &["recipient_email"]),
                        outbound_string(&engagement, &["payload", "contact_email"]),
                    ])
                    .context("recipient_email is required")?;
                    anyhow::ensure!(
                        !outbound_recipient_suppressed(&conn, &email)?,
                        "recipient is suppressed for outbound communication"
                    );
                    (sender, email, String::new())
                };
            let previous_messages = outbound_load_records_by_string_field(
                &conn,
                "outbound_messages",
                "engagement_id",
                &engagement_id,
            )?;
            let latest_message = outbound_latest_message(&previous_messages);
            let resolved_skillbook_id = outbound_first_string(&[
                outbound_string(&command.payload, &["skillbook_id"]),
                outbound_string(&command.payload, &["payload", "skillbook_id"]),
                outbound_string(&engagement, &["payload", "skillbook_id"]),
            ])
            .unwrap_or_else(|| "business-os.outbound.message_drafting.v1".to_string());
            let skillbook_guidance = outbound_skillbook_guidance(&conn, &resolved_skillbook_id)?;
            let generated = outbound_generate_automated_draft(
                &engagement,
                latest_message.as_ref(),
                &command.payload,
                &draft_kind,
                skillbook_guidance.as_deref(),
            );
            let subject = outbound_first_string(&[
                outbound_string(&command.payload, &["subject"]),
                outbound_string(&generated, &["subject"]),
            ])
            .context("generated subject is required")?;
            let body_text = outbound_first_string(&[
                outbound_string(&command.payload, &["body_text"]),
                outbound_string(&generated, &["body_text"]),
            ])
            .context("generated body_text is required")?;
            let mut message = outbound_object_payload(&command.payload);
            outbound_put_string(&mut message, "id", message_id.clone());
            outbound_put_string(&mut message, "engagement_id", engagement_id.clone());
            outbound_put_string(&mut message, "campaign_id", campaign_id);
            outbound_put_string(&mut message, "message_type", draft_kind.clone());
            outbound_put_string(&mut message, "channel", channel.clone());
            outbound_put_string(&mut message, "direction", "outbound");
            outbound_put_string(&mut message, "sender_account_id", sender_account_id);
            outbound_put_string(&mut message, "recipient_email", recipient_email);
            if !recipient_address_text.is_empty() {
                outbound_put_string(
                    &mut message,
                    "recipient_address_text",
                    recipient_address_text,
                );
            }
            outbound_put_string(&mut message, "subject", subject);
            outbound_put_string(&mut message, "body_text", body_text);
            outbound_put_string(&mut message, "draft_status", "ready_for_review");
            outbound_put_string(&mut message, "approval_status", "awaiting_approval");
            outbound_put_string(&mut message, "send_status", "awaiting_approval");
            outbound_put_default_object(&mut message, "payload");
            outbound_payload_insert(
                &mut message,
                "draft_engine",
                Value::String("business-os.outbound.draft_automation.v1".to_string()),
            );
            outbound_payload_insert(&mut message, "generated_draft", generated.clone());
            outbound_payload_insert(
                &mut message,
                "skillbook_id",
                Value::String(resolved_skillbook_id.clone()),
            );
            outbound_payload_insert(
                &mut message,
                "runbook_id",
                Value::String(
                    outbound_first_string(&[
                        outbound_string(&command.payload, &["runbook_id"]),
                        outbound_string(&command.payload, &["payload", "runbook_id"]),
                        outbound_string(&engagement, &["payload", "runbook_id"]),
                    ])
                    .unwrap_or_default(),
                ),
            );
            if let Some(previous) = latest_message
                .as_ref()
                .and_then(|message| outbound_string(message, &["id"]))
            {
                outbound_put_string(&mut message, "reply_to_message_id", previous);
            }
            if draft_kind == "scheduling" {
                let request_id = outbound_string(&command.payload, &["meeting_request_id"])
                    .unwrap_or_else(|| format!("meeting_{message_id}"));
                outbound_payload_insert(
                    &mut message,
                    "meeting_request_id",
                    Value::String(request_id),
                );
                outbound_payload_insert(
                    &mut message,
                    "proposed_slots",
                    command
                        .payload
                        .get("proposed_slots")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([])),
                );
            }
            let revision_id = outbound_message_revision(&message);
            outbound_put_string(&mut message, "revision_id", revision_id);
            outbound_put_default_i64(&mut message, "created_at_ms", now);
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            let mut meeting_request_result = Value::Null;
            if draft_kind == "scheduling" {
                let request_id = outbound_string(&command.payload, &["meeting_request_id"])
                    .unwrap_or_else(|| format!("meeting_{message_id}"));
                let mut request = serde_json::json!({
                    "id": request_id,
                    "engagement_id": engagement_id,
                    "message_id": message_id,
                    "duration_minutes": command.payload.get("duration_minutes").and_then(Value::as_i64).unwrap_or(30),
                    "slot_strategy": outbound_string(&command.payload, &["slot_strategy"]).unwrap_or_else(|| "campaign_default".to_string()),
                    "proposed_slots": command.payload.get("proposed_slots").cloned().unwrap_or_else(|| serde_json::json!([])),
                    "status": "prepared",
                    "payload": {
                        "source": "outbound.draft.prepare"
                    },
                    "created_at_ms": now,
                    "updated_at_ms": now
                });
                if let Some(calendar) = outbound_string(&command.payload, &["calendar_account_id"])
                {
                    outbound_put_string(&mut request, "calendar_account_id", calendar);
                }
                upsert_business_record(
                    &conn,
                    "outbound_meeting_requests",
                    &request_id,
                    now,
                    request.clone(),
                )?;
                meeting_request_result = request;
            }
            outbound_update_engagement_status(&conn, &engagement_id, "awaiting_approval", now)?;
            Ok(serde_json::json!({
                "ok": true,
                "collection": "outbound_messages",
                "message": message,
                "meeting_request": meeting_request_result,
                "approval_required": true,
                "provider_send_executed": false
            }))
        }
        "outbound.message.prepare" => {
            let message_id = outbound_id_from_command(command, &["message_id", "id"], "msg")?;
            let engagement_id = outbound_required_string(&command.payload, &["engagement_id"])?;
            let engagement = outbound_load_record(&conn, "outbound_engagements", &engagement_id)?;
            let campaign_id = outbound_first_string(&[
                outbound_string(&command.payload, &["campaign_id"]),
                engagement
                    .as_ref()
                    .and_then(|value| outbound_string(value, &["campaign_id"])),
            ])
            .context("campaign_id is required")?;
            let mut message = outbound_object_payload(&command.payload);
            outbound_put_string(&mut message, "id", message_id.clone());
            outbound_put_string(&mut message, "engagement_id", engagement_id.clone());
            outbound_put_string(&mut message, "campaign_id", campaign_id);
            outbound_put_default_string(&mut message, "message_type", "initial");
            outbound_put_default_string(&mut message, "direction", "outbound");
            outbound_put_default_string(&mut message, "draft_status", "prepared");
            outbound_put_default_string(&mut message, "approval_status", "draft");
            outbound_put_default_string(&mut message, "send_status", "not_scheduled");
            outbound_put_default_object(&mut message, "payload");
            let revision_id = outbound_message_revision(&message);
            outbound_put_string(&mut message, "revision_id", revision_id);
            outbound_put_default_i64(&mut message, "created_at_ms", now);
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            outbound_update_engagement_status(&conn, &engagement_id, "draft_prepared", now)?;
            Ok(serde_json::json!({
                "ok": true,
                "collection": "outbound_messages",
                "message": message
            }))
        }
        "outbound.message.update_draft" => {
            let message_id = outbound_required_from_payload_or_record(
                command,
                &["message_id", "id"],
                "message_id is required",
            )?;
            let mut message = outbound_load_required(
                &conn,
                "outbound_messages",
                &message_id,
                "message not found",
            )?;
            anyhow::ensure!(
                !matches!(
                    outbound_string(&message, &["send_status"]).as_deref(),
                    Some("queued_for_provider" | "sent" | "accepted")
                ),
                "sent or queued outbound message drafts cannot be edited"
            );
            outbound_merge_fields(
                &mut message,
                &command.payload,
                &[
                    "recipient_email",
                    "recipient_address_text",
                    "recipient_address",
                    "channel",
                    "subject",
                    "body_text",
                    "body_html",
                    "sender_account_id",
                    "scheduled_send_at_ms",
                    "payload",
                ],
            );
            outbound_put_string(&mut message, "draft_status", "prepared".to_string());
            outbound_put_string(&mut message, "approval_status", "draft".to_string());
            outbound_put_string(&mut message, "send_status", "not_scheduled".to_string());
            let revision_id = outbound_message_revision(&message);
            outbound_put_string(&mut message, "revision_id", revision_id);
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            Ok(serde_json::json!({ "ok": true, "message": message }))
        }
        "outbound.message.request_approval" => {
            let message_id = outbound_required_from_payload_or_record(
                command,
                &["message_id", "id"],
                "message_id is required",
            )?;
            let mut message = outbound_load_required(
                &conn,
                "outbound_messages",
                &message_id,
                "message not found",
            )?;
            outbound_require_message_content(&message)?;
            let revision_id = outbound_message_revision(&message);
            outbound_put_string(&mut message, "revision_id", revision_id);
            outbound_put_string(&mut message, "draft_status", "ready_for_review".to_string());
            outbound_put_string(
                &mut message,
                "approval_status",
                "awaiting_approval".to_string(),
            );
            outbound_put_string(&mut message, "send_status", "awaiting_approval".to_string());
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            if let Some(engagement_id) = outbound_string(&message, &["engagement_id"]) {
                outbound_update_engagement_status(&conn, &engagement_id, "awaiting_approval", now)?;
            }
            Ok(serde_json::json!({ "ok": true, "message": message }))
        }
        "outbound.message.approve" => {
            let message_id = outbound_required_from_payload_or_record(
                command,
                &["message_id", "id"],
                "message_id is required",
            )?;
            let mut message = outbound_load_required(
                &conn,
                "outbound_messages",
                &message_id,
                "message not found",
            )?;
            anyhow::ensure!(
                outbound_string(&message, &["approval_status"]).as_deref()
                    == Some("awaiting_approval"),
                "only messages awaiting approval can be approved"
            );
            outbound_require_message_content(&message)?;
            let revision_id = outbound_message_revision(&message);
            let approval_id = outbound_id_from_command(command, &["approval_id"], "approval")
                .unwrap_or_else(|_| format!("approval_{message_id}_{revision_id}"));
            let engagement_id = outbound_string(&message, &["engagement_id"]).unwrap_or_default();
            let mut approval = outbound_object_payload(&command.payload);
            outbound_put_string(&mut approval, "id", approval_id.clone());
            outbound_put_string(&mut approval, "message_id", message_id.clone());
            outbound_put_string(&mut approval, "engagement_id", engagement_id.clone());
            outbound_put_string(&mut approval, "revision_id", revision_id.clone());
            outbound_put_string(
                &mut approval,
                "actor_user_id",
                outbound_session_actor_id(session),
            );
            outbound_put_string(&mut approval, "decision", "approved");
            outbound_put_default_object(&mut approval, "payload");
            outbound_put_default_i64(&mut approval, "created_at_ms", now);
            outbound_put_i64(&mut approval, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_approvals",
                &approval_id,
                now,
                approval.clone(),
            )?;
            outbound_put_string(&mut message, "revision_id", revision_id);
            outbound_put_string(&mut message, "approval_status", "approved");
            outbound_put_string(&mut message, "draft_status", "approved");
            outbound_put_string(&mut message, "send_status", "approved_not_sent");
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            if !engagement_id.is_empty() {
                outbound_update_engagement_status(&conn, &engagement_id, "approved_for_send", now)?;
            }
            record_outbound_approval_decision_event(
                &conn, session, command, &approval, &message, now,
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "message": message,
                "approval": approval
            }))
        }
        "outbound.message.reject" => outbound_record_rejection(&conn, session, command, now),
        "outbound.message.request_changes" => {
            outbound_record_change_request(&conn, session, command, now)
        }
        "outbound.message.send_approved" => {
            let message_id = outbound_required_from_payload_or_record(
                command,
                &["message_id", "id"],
                "message_id is required",
            )?;
            let mut message = outbound_load_required(
                &conn,
                "outbound_messages",
                &message_id,
                "message not found",
            )?;
            if let Err(err) = outbound_enforce_send_gate(&conn, &message) {
                let reason = err.to_string();
                outbound_record_send_failure(&conn, &message_id, &mut message, &reason, now)?;
                return Err(err);
            }
            let channel =
                outbound_string(&message, &["channel"]).unwrap_or_else(|| "email".to_string());

            // Physical letter path: no provider queueing, mark as manually dispatched.
            if channel == "physical_letter" {
                let existing_dispatch =
                    outbound_string(&message, &["payload", "provider_dispatch_status"])
                        .unwrap_or_default();
                if existing_dispatch == "manual_physical_letter_marked_sent" {
                    return Ok(serde_json::json!({
                        "ok": true,
                        "message": message,
                        "channel": "physical_letter",
                        "provider_dispatch_status": "manual_physical_letter_marked_sent",
                        "idempotent": true,
                    }));
                }
                outbound_put_string(&mut message, "send_status", "sent");
                outbound_payload_insert(
                    &mut message,
                    "provider_dispatch_status",
                    Value::String("manual_physical_letter_marked_sent".to_string()),
                );
                outbound_payload_insert(&mut message, "provider_send_executed", Value::Bool(true));
                outbound_payload_insert(
                    &mut message,
                    "physical_sent_at_ms",
                    Value::Number(serde_json::Number::from(now)),
                );
                outbound_put_i64(&mut message, "sent_at_ms", now);
                outbound_put_i64(&mut message, "updated_at_ms", now);
                upsert_business_record(
                    &conn,
                    "outbound_messages",
                    &message_id,
                    now,
                    message.clone(),
                )?;
                if let Some(engagement_id) = outbound_string(&message, &["engagement_id"]) {
                    outbound_update_engagement_status(&conn, &engagement_id, "sent", now)?;
                }
                return Ok(serde_json::json!({
                    "ok": true,
                    "message": message,
                    "channel": "physical_letter",
                    "provider_dispatch_status": "manual_physical_letter_marked_sent",
                    "provider_send_executed": true,
                    "physical_sent_at_ms": now,
                }));
            }

            let existing_provider_queue_id = outbound_first_string(&[
                outbound_string(&message, &["provider_message_id"]),
                outbound_string(&message, &["payload", "provider_queue_id"]),
                outbound_string(&message, &["payload", "provider_message_id"]),
            ]);
            let existing_send_status =
                outbound_string(&message, &["send_status"]).unwrap_or_default();
            let already_queued = matches!(
                existing_send_status.as_str(),
                "queued_for_provider" | "sent" | "accepted"
            ) && existing_provider_queue_id.is_some()
                && message
                    .get("payload")
                    .and_then(|payload| payload.get("provider_send_executed"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            if already_queued {
                outbound_sync_email_message_to_communication(
                    root,
                    &mut message,
                    &existing_send_status,
                )?;
                return Ok(serde_json::json!({
                    "ok": true,
                    "message": message,
                    "provider_dispatch_status": "queued_in_mailserver",
                    "provider_queue_id": existing_provider_queue_id,
                    "provider_send_executed": true,
                    "idempotent": true
                }));
            }
            // Atomically reserve a daily send slot BEFORE handing the message to
            // the provider. The reservation enforces the per-account daily cap
            // under parallel commands (the check+increment is serialized by a
            // BEGIN IMMEDIATE transaction), so two concurrent sends cannot both
            // pass when only one slot remains.
            let sender_account_id = outbound_required_string(&message, &["sender_account_id"])?;
            if let Err(err) = outbound_reserve_account_send_slot(&conn, &sender_account_id, now) {
                let reason = err.to_string();
                outbound_record_send_failure(&conn, &message_id, &mut message, &reason, now)?;
                return Err(err);
            }
            let provider_queue_id = match outbound_queue_email_delivery(root, &mut message)
                .context("failed to queue approved outbound email")
            {
                Ok(id) => id,
                Err(err) => {
                    // The send never reached the provider; release the reserved
                    // slot so the daily counter stays accurate for retries.
                    let _ = outbound_release_account_send_slot(&conn, &sender_account_id, now);
                    let reason = err.to_string();
                    outbound_record_send_failure(&conn, &message_id, &mut message, &reason, now)?;
                    return Err(err);
                }
            };
            outbound_put_string(
                &mut message,
                "provider_message_id",
                provider_queue_id.clone(),
            );
            outbound_payload_insert(
                &mut message,
                "provider_queue_id",
                Value::String(provider_queue_id.clone()),
            );
            outbound_payload_insert(
                &mut message,
                "provider_message_id",
                Value::String(provider_queue_id.clone()),
            );
            outbound_payload_insert(
                &mut message,
                "provider_dispatch_status",
                Value::String("queued_in_mailserver".to_string()),
            );
            outbound_payload_insert(&mut message, "provider_send_executed", Value::Bool(true));
            outbound_payload_insert(
                &mut message,
                "provider_queued_at_ms",
                Value::Number(serde_json::Number::from(now)),
            );
            outbound_put_string(&mut message, "send_status", "queued_for_provider");
            // A successful (re)send clears any prior failure markers so the
            // message no longer looks blocked after a retry.
            outbound_payload_insert(&mut message, "send_block_reason", Value::Null);
            outbound_payload_insert(&mut message, "last_send_error", Value::Null);
            outbound_payload_insert(&mut message, "retryable", Value::Bool(false));
            outbound_sync_email_message_to_communication(
                root,
                &mut message,
                "queued_for_provider",
            )?;
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            if let Some(engagement_id) = outbound_string(&message, &["engagement_id"]) {
                outbound_update_engagement_status(&conn, &engagement_id, "scheduled_to_send", now)?;
            }
            Ok(serde_json::json!({
                "ok": true,
                "message": message.clone(),
                "provider_dispatch_status": "queued_in_mailserver",
                "provider_queue_id": outbound_string(&message, &["payload", "provider_queue_id"]),
                "provider_send_executed": true
            }))
        }
        "outbound.message.pause" | "outbound.message.cancel" => {
            let message_id = outbound_required_from_payload_or_record(
                command,
                &["message_id", "id"],
                "message_id is required",
            )?;
            let mut message = outbound_load_required(
                &conn,
                "outbound_messages",
                &message_id,
                "message not found",
            )?;
            let status = if command.command_type == "outbound.message.pause" {
                "paused"
            } else {
                "cancelled"
            };
            let reason = outbound_string(&command.payload, &["reason"]);
            outbound_put_string(&mut message, "send_status", status);
            if let Some(reason) = reason.as_ref() {
                let payload_key = if status == "paused" {
                    "pause_reason"
                } else {
                    "cancel_reason"
                };
                outbound_payload_insert(&mut message, payload_key, Value::String(reason.clone()));
            }
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            if let Some(engagement_id) = outbound_string(&message, &["engagement_id"]) {
                outbound_update_engagement_terminal_status(
                    &conn,
                    &engagement_id,
                    status,
                    reason.as_deref(),
                    now,
                )?;
            }
            Ok(serde_json::json!({ "ok": true, "message": message }))
        }
        "outbound.message.resume" => {
            let message_id = outbound_required_from_payload_or_record(
                command,
                &["message_id", "id"],
                "message_id is required",
            )?;
            let mut message = outbound_load_required(
                &conn,
                "outbound_messages",
                &message_id,
                "message not found",
            )?;
            anyhow::ensure!(
                outbound_string(&message, &["send_status"]).as_deref() == Some("paused"),
                "only paused outbound messages can be resumed"
            );
            let send_status = outbound_send_status_for_resume(&message);
            outbound_put_string(&mut message, "send_status", send_status);
            outbound_payload_insert(
                &mut message,
                "resume_reason",
                Value::String(
                    outbound_string(&command.payload, &["reason"])
                        .unwrap_or_else(|| "manual_resume".to_string()),
                ),
            );
            outbound_put_i64(&mut message, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_messages",
                &message_id,
                now,
                message.clone(),
            )?;
            if let Some(engagement_id) = outbound_string(&message, &["engagement_id"]) {
                let engagement_status = outbound_engagement_status_for_message_state(&message);
                outbound_update_engagement_status(&conn, &engagement_id, engagement_status, now)?;
            }
            Ok(serde_json::json!({ "ok": true, "message": message }))
        }
        "outbound.engagement.resume" => {
            let engagement_id = outbound_required_from_payload_or_record(
                command,
                &["engagement_id", "id"],
                "engagement_id is required",
            )?;
            let mut engagement = outbound_load_required(
                &conn,
                "outbound_engagements",
                &engagement_id,
                "engagement not found",
            )?;
            anyhow::ensure!(
                outbound_string(&engagement, &["status"]).as_deref() == Some("paused"),
                "only paused engagements can be resumed"
            );
            let messages = outbound_load_records_by_string_field(
                &conn,
                "outbound_messages",
                "engagement_id",
                &engagement_id,
            )?;
            let next_status = messages
                .iter()
                .find(|message| {
                    outbound_string(message, &["send_status"]).as_deref() == Some("paused")
                })
                .map(outbound_engagement_status_for_message_state)
                .unwrap_or("assigned");
            outbound_put_string(&mut engagement, "status", next_status);
            outbound_put_string(&mut engagement, "paused_reason", "");
            outbound_payload_insert(
                &mut engagement,
                "resume_reason",
                Value::String(
                    outbound_string(&command.payload, &["reason"])
                        .unwrap_or_else(|| "manual_resume".to_string()),
                ),
            );
            outbound_put_i64(&mut engagement, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_engagements",
                &engagement_id,
                now,
                engagement.clone(),
            )?;
            Ok(serde_json::json!({ "ok": true, "engagement": engagement }))
        }
        "outbound.engagement.close" => {
            let engagement_id = outbound_required_from_payload_or_record(
                command,
                &["engagement_id", "id"],
                "engagement_id is required",
            )?;
            let reason = outbound_string(&command.payload, &["reason"]);
            let mut engagement = outbound_load_required(
                &conn,
                "outbound_engagements",
                &engagement_id,
                "engagement not found",
            )?;
            outbound_put_string(&mut engagement, "status", "closed");
            if let Some(reason) = reason.as_ref().filter(|value| !value.trim().is_empty()) {
                outbound_put_string(&mut engagement, "closed_reason", reason.clone());
            }
            outbound_put_i64(&mut engagement, "closed_at_ms", now);
            outbound_put_i64(&mut engagement, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_engagements",
                &engagement_id,
                now,
                engagement.clone(),
            )?;

            let mut closed_messages = Vec::new();
            for mut message in outbound_load_records_by_string_field(
                &conn,
                "outbound_messages",
                "engagement_id",
                &engagement_id,
            )? {
                let send_status = outbound_string(&message, &["send_status"]).unwrap_or_default();
                if matches!(
                    send_status.as_str(),
                    "sent" | "accepted" | "queued_for_provider" | "cancelled"
                ) {
                    continue;
                }
                outbound_put_string(&mut message, "send_status", "cancelled");
                if let Some(reason) = reason.as_ref() {
                    outbound_payload_insert(
                        &mut message,
                        "close_reason",
                        Value::String(reason.clone()),
                    );
                }
                outbound_put_i64(&mut message, "updated_at_ms", now);
                if let Some(message_id) = outbound_string(&message, &["id"]) {
                    upsert_business_record(
                        &conn,
                        "outbound_messages",
                        &message_id,
                        now,
                        message.clone(),
                    )?;
                    closed_messages.push(message_id);
                }
            }
            Ok(serde_json::json!({
                "ok": true,
                "engagement": engagement,
                "closed_message_ids": closed_messages
            }))
        }
        "outbound.reply.classify" => {
            let engagement_id = outbound_required_string(&command.payload, &["engagement_id"])?;
            let classification = outbound_required_string(&command.payload, &["classification"])?;
            let mut engagement = outbound_load_required(
                &conn,
                "outbound_engagements",
                &engagement_id,
                "engagement not found",
            )?;
            outbound_put_string(&mut engagement, "status", "reply_received".to_string());
            outbound_payload_insert(
                &mut engagement,
                "reply_classification",
                Value::String(classification.clone()),
            );
            outbound_merge_fields(&mut engagement, &command.payload, &["reply_message_id"]);
            if classification == "out_of_office" {
                outbound_apply_out_of_office_wait(&mut engagement, &command.payload, now);
            }
            outbound_put_i64(&mut engagement, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_engagements",
                &engagement_id,
                now,
                engagement.clone(),
            )?;
            let suppression_id = outbound_apply_reply_suppression(
                &conn,
                &engagement,
                &engagement_id,
                &classification,
                now,
            )?;
            Ok(serde_json::json!({
                "ok": true,
                "engagement": engagement,
                "suppression_id": suppression_id,
            }))
        }
        "outbound.scheduling.prepare" => {
            let request_id =
                outbound_id_from_command(command, &["meeting_request_id", "id"], "meeting")?;
            let engagement_id = outbound_required_string(&command.payload, &["engagement_id"])?;
            let mut request = outbound_object_payload(&command.payload);
            outbound_put_string(&mut request, "id", request_id.clone());
            outbound_put_string(&mut request, "engagement_id", engagement_id.clone());
            outbound_put_default_string(&mut request, "status", "prepared");
            outbound_put_default_i64(&mut request, "created_at_ms", now);
            outbound_put_i64(&mut request, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_meeting_requests",
                &request_id,
                now,
                request.clone(),
            )?;
            outbound_update_engagement_status(&conn, &engagement_id, "scheduling", now)?;
            Ok(serde_json::json!({ "ok": true, "meeting_request": request }))
        }
        "outbound.scheduling.mark_booked" => {
            let request_id = outbound_required_from_payload_or_record(
                command,
                &["meeting_request_id", "id"],
                "meeting_request_id is required",
            )?;
            let mut request = outbound_load_required(
                &conn,
                "outbound_meeting_requests",
                &request_id,
                "meeting request not found",
            )?;
            outbound_put_string(&mut request, "status", "booked");
            outbound_merge_fields(
                &mut request,
                &command.payload,
                &["meeting_url", "booked_at_ms", "payload"],
            );
            outbound_put_i64(&mut request, "updated_at_ms", now);
            upsert_business_record(
                &conn,
                "outbound_meeting_requests",
                &request_id,
                now,
                request.clone(),
            )?;
            let mut engagement_result = Value::Null;
            if let Some(engagement_id) = outbound_string(&request, &["engagement_id"]) {
                outbound_update_engagement_status(&conn, &engagement_id, "meeting_booked", now)?;
                engagement_result = outbound_load_required(
                    &conn,
                    "outbound_engagements",
                    &engagement_id,
                    "engagement not found",
                )?;
            }
            Ok(serde_json::json!({
                "ok": true,
                "meeting_request": request,
                "engagement": engagement_result
            }))
        }
        "outbound.campaign.mailbox.link" => {
            outbound_handle_campaign_mailbox_link(root, &conn, command, now)
        }
        "outbound.campaign.status.set" => outbound_handle_campaign_status_set(&conn, command, now),
        "outbound.campaign.briefing.update" => {
            outbound_handle_campaign_briefing_update(root, &conn, command, now)
        }
        "outbound.campaign.apply_setup" => {
            outbound_handle_campaign_apply_setup(root, &conn, command, now)
        }
        "outbound.reply.match" => outbound_handle_reply_match(root, &conn, command, now),
        "outbound.provider.reconcile" => {
            outbound_handle_provider_reconcile(root, &conn, command, now)
        }
        "outbound.research_policy.publish" => {
            outbound_handle_research_policy_publish(root, &conn, command, now)
        }
        "outbound.skillbook.save" => outbound_handle_skillbook_save(&conn, command, now),
        "outbound.skillbook.seed_defaults" => outbound_handle_skillbook_seed_defaults(&conn, now),
        "outbound.letter_template.save" => {
            outbound_handle_letter_template_save(&conn, command, now)
        }
        "outbound.audit.export" => outbound_handle_audit_export(&conn, command),
        "outbound.scheduler.tick" => outbound_handle_scheduler_tick(root, &conn, command, now),
        "outbound.dev.seed_test_data" => outbound_handle_dev_seed_test_data(&conn, command, now),
        "outbound.engagement.reapply_sequence" => {
            outbound_handle_engagement_reapply_sequence(&conn, command, now)
        }
        "outbound.scheduling.update_slots" => {
            outbound_handle_scheduling_update_slots(&conn, command, now)
        }
        "outbound.research_source.upsert" => {
            outbound_handle_research_source_adapter(root, &conn, command, now, "active")
        }
        "outbound.research_source.generate_adapter" => {
            outbound_handle_research_source_adapter(root, &conn, command, now, "adapter_requested")
        }
        "outbound.research_source.test" => {
            outbound_handle_research_source_adapter(root, &conn, command, now, "test_requested")
        }
        "outbound.research_source.auth_assist" => {
            outbound_handle_research_source_adapter(root, &conn, command, now, "auth_requested")
        }
        "outbound.sellify.lookup" => outbound_handle_sellify_lookup(root, command),
        "outbound.research_source.registry_read" => {
            outbound_handle_research_source_registry_read(root, command)
        }
        other => anyhow::bail!("unsupported active outbound command: {other}"),
    }
}

fn outbound_handle_sellify_lookup(root: &Path, command: &BusinessCommand) -> anyhow::Result<Value> {
    outbound_sellify_lookup(root, &command.payload)
}

/// Liest die WAHRHEIT der Scrape-Registry, damit die Quellenliste der App nicht
/// laenger ihre eigenen, veralteten Datensaetze anzeigt.
///
/// Owner-Befund 03.09.2026: "diese ganze adapter liste steht ueberall nur
/// status geht nicht, status funktioniert nicht". Gemessen: die Registry fuehrt
/// 21 Ziele, alle `active`, waehrend die App Pruefergebnisse vom 31.08. und
/// "noch nie geprueft" anzeigte - sie las nur ihre eigenen Adapterdatensaetze.
///
/// Reiner Lesebefehl: keine Ausfuehrung, keine Aenderung an einem Ziel.
fn outbound_handle_research_source_registry_read(
    root: &Path,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let antwort = scrape::dispatch_capturing(root, &["list-targets".to_string()])
        .context("scrape registry could not be read")?;
    let ziele = antwort
        .as_array()
        .cloned()
        .or_else(|| {
            antwort
                .as_object()
                .and_then(|map| map.values().find_map(|v| v.as_array().cloned()))
        })
        .unwrap_or_default();
    let gesucht = command
        .payload
        .get("target_keys")
        .and_then(Value::as_array)
        .map(|werte| {
            werte
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<std::collections::BTreeSet<_>>()
        });
    let mut eintraege = Vec::new();
    for ziel in ziele {
        let key = ziel
            .get("target_key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if key.is_empty() {
            continue;
        }
        if let Some(gesucht) = &gesucht {
            if !gesucht.contains(&key) {
                continue;
            }
        }
        eintraege.push(serde_json::json!({
            "target_key": key,
            "target_id": ziel.get("target_id").and_then(Value::as_str).unwrap_or_default(),
            "display_name": ziel.get("display_name").and_then(Value::as_str).unwrap_or_default(),
            "status": ziel.get("status").and_then(Value::as_str).unwrap_or_default(),
            "target_kind": ziel.get("target_kind").and_then(Value::as_str).unwrap_or_default(),
            "start_url": ziel.get("start_url").and_then(Value::as_str).unwrap_or_default(),
            "latest_script_revision_no": ziel.get("latest_script_revision_no").cloned().unwrap_or(Value::Null),
        }));
    }
    let script = match command
        .payload
        .get("include_script_target")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        Some(key) => outbound_registry_latest_script(root, key)?,
        None => Value::Null,
    };
    Ok(serde_json::json!({
        "ok": true,
        "schema": "ctox.outbound.research_source_registry.v1",
        "targets": eintraege,
        "script": script,
    }))
}

/// Latest registered script of one scrape target, for the Outbound app's
/// "Adapter-Skript ansehen" dialog. The dialog could never show a script: the
/// app looked for it in its own adapter records, while scripts live only in
/// the scrape registry (Klicktest-Befund P4 V14, 11.09.2026). Bounded so the
/// command result stays inside the peer wire budget.
fn outbound_registry_latest_script(root: &Path, target_key: &str) -> anyhow::Result<Value> {
    const MAX_SCRIPT_BYTES: usize = 150_000;
    let antwort = scrape::dispatch_capturing(
        root,
        &[
            "show-target".to_string(),
            "--target-key".to_string(),
            target_key.to_string(),
        ],
    )
    .with_context(|| format!("scrape target {target_key} could not be read"))?;
    let Some(revision) = antwort
        .pointer("/target/revisions")
        .and_then(Value::as_array)
        .and_then(|revisions| {
            revisions.iter().max_by_key(|revision| {
                revision
                    .get("revision_no")
                    .and_then(Value::as_i64)
                    .unwrap_or_default()
            })
        })
    else {
        return Ok(serde_json::json!({ "target_key": target_key, "available": false }));
    };
    let script_path = revision
        .get("script_path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let path = if Path::new(script_path).is_absolute() {
        std::path::PathBuf::from(script_path)
    } else {
        root.join(script_path)
    };
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Ok(serde_json::json!({
                "target_key": target_key,
                "available": false,
                "revision_no": revision.get("revision_no").cloned().unwrap_or(Value::Null),
                "error": format!("script file not readable: {error}"),
            }))
        }
    };
    let truncated = bytes.len() > MAX_SCRIPT_BYTES;
    let mut text =
        String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_SCRIPT_BYTES)]).into_owned();
    if truncated {
        text.push_str("\n/* … gekürzt: Skript größer als 150 KB … */\n");
    }
    Ok(serde_json::json!({
        "target_key": target_key,
        "available": true,
        "revision_no": revision.get("revision_no").cloned().unwrap_or(Value::Null),
        "language": revision.get("language").cloned().unwrap_or(Value::Null),
        "script_sha256": revision.get("script_sha256").cloned().unwrap_or(Value::Null),
        "created_at": revision.get("created_at").cloned().unwrap_or(Value::Null),
        "truncated": truncated,
        "text": text,
    }))
}

pub(super) fn outbound_sellify_lookup(root: &Path, payload: &Value) -> anyhow::Result<Value> {
    let entity = outbound_required_string(payload, &["entity"])?;
    let (collection, allowed_fields): (&str, &[&str]) = match entity.as_str() {
        "company" => (
            "sellify_companies",
            &[
                "contact_id",
                "name",
                "company_name",
                "website",
                // The projection stores the URL as `website_url`; `website`
                // stays allowed for callers written against the older shape.
                "website_url",
                "email",
                "phone",
            ],
        ),
        // Campaign membership rows (one row per campaign x contact). Enables
        // importing an old Sellify campaign as the seed of a follow-up
        // research run.
        "campaign" => (
            "sellify_campaigns",
            &[
                "name",
                "title",
                "contact_id",
                "company_id",
                "person_id",
                "selection_id",
            ],
        ),
        "person" => (
            "sellify_people",
            &[
                "person_id",
                "contact_id",
                "email",
                "phone",
                "name",
                "display_name",
                "first_name",
                "last_name",
            ],
        ),
        _ => anyhow::bail!("entity must be company or person"),
    };
    // Campaign imports must see the FULL membership of one campaign; the
    // usual dedupe/lookup cap of 100 would silently truncate it.
    // A name search groups rows server-side (see below), so it may scan many
    // more rows than it returns.
    let group_by_name =
        entity == "campaign" && payload.get("group_by").and_then(Value::as_str) == Some("name");
    let limit_cap = if group_by_name {
        50_000
    } else if entity == "campaign" {
        2000
    } else {
        100
    };
    let limit = payload
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(25)
        .clamp(1, limit_cap) as usize;
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    if let Some(ids) = payload.get("ids").and_then(Value::as_array) {
        for id in ids
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            if records.len() >= limit {
                break;
            }
            if let Some(record) = load_rxdb_collection_record(root, collection, id)? {
                if seen.insert(id.to_string()) {
                    records.push(record);
                }
            }
        }
    }
    if let Some(selectors) = payload.get("selectors").and_then(Value::as_array) {
        for selector in selectors {
            if records.len() >= limit {
                break;
            }
            let field = selector
                .get("field")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let expected = selector
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            anyhow::ensure!(
                allowed_fields.contains(&field),
                "unsupported {entity} lookup field `{field}`"
            );
            if expected.is_empty() {
                continue;
            }
            for (id, record) in find_rxdb_collection_records_by_string_field(
                root,
                collection,
                field,
                expected,
                limit.saturating_sub(records.len()),
            )? {
                if seen.insert(id) {
                    records.push(record);
                }
            }
        }
    }
    // Containment probes for CRM matching: company names drift (legal-form
    // suffixes, renames, umlauts), so callers pass normalized fragments and
    // the scan runs natively instead of in the browser.
    if let Some(selectors) = payload.get("fuzzy_selectors").and_then(Value::as_array) {
        for selector in selectors {
            if records.len() >= limit {
                break;
            }
            let field = selector
                .get("field")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let needle = selector
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            anyhow::ensure!(
                allowed_fields.contains(&field),
                "unsupported {entity} lookup field `{field}`"
            );
            if needle.is_empty() {
                continue;
            }
            for (id, record) in find_rxdb_collection_records_by_string_field_contains(
                root,
                collection,
                field,
                needle,
                limit.saturating_sub(records.len()),
            )? {
                if seen.insert(id) {
                    records.push(record);
                }
            }
        }
    }
    // Beauty, 11.09.2026: a campaign search returned 2000 full rows (1.1 MB);
    // the peer dropped the result ("exceeds peer wire budget", 256 KB) and the
    // browser reported "no Sellify campaign found". A search needs names and
    // counts, an import only a few columns.
    if group_by_name {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for record in &records {
            let name = record
                .get("name")
                .or_else(|| record.get("title"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            if !name.is_empty() && record.get("is_deleted").and_then(Value::as_bool) != Some(true) {
                *counts.entry(name.to_string()).or_default() += 1;
            }
        }
        let mut groups = counts.into_iter().collect::<Vec<_>>();
        groups.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let truncated = records.len() >= limit;
        let groups = groups
            .into_iter()
            .take(200)
            .map(|(name, count)| serde_json::json!({ "name": name, "count": count }))
            .collect::<Vec<_>>();
        return Ok(serde_json::json!({
            "ok": true,
            "entity": entity,
            "groups": groups,
            "scanned": records.len(),
            "truncated": truncated,
            "records": [],
        }));
    }
    let fields = payload
        .get("fields")
        .and_then(Value::as_array)
        .map(|fields| {
            fields
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|fields| !fields.is_empty());
    if let Some(fields) = fields {
        for record in records.iter_mut() {
            if let Some(object) = record.as_object_mut() {
                object.retain(|key, _| key == "id" || fields.iter().any(|field| field == key));
            }
        }
    }
    Ok(serde_json::json!({
        "ok": true,
        "entity": entity,
        "records": records,
    }))
}

pub(super) fn is_outbound_adapter_reconciliation_command(command: &BusinessCommand) -> bool {
    command.command_type == "outbound.research.adapters.reconcile"
}

/// Apply the typed result of the single campaign-wide adapter reconciliation
/// task.  The queue worker may create or repair multiple Playwright targets in
/// one bounded turn; only this native writeback path is allowed to project the
/// result into the tenant module's RxDB collections.
pub(super) fn apply_outbound_adapter_reconciliation_reply(
    root: &Path,
    conn: &Connection,
    command_id: &str,
    command: &BusinessCommand,
    task_id: &str,
    reply_text: &str,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        is_outbound_adapter_reconciliation_command(command),
        "not an outbound adapter reconciliation command"
    );
    let reply_text = reply_text.trim();
    anyhow::ensure!(
        !reply_text.is_empty(),
        "adapter reconciliation reply was empty"
    );
    let result: Value = serde_json::from_str(reply_text)
        .context("adapter reconciliation reply must be one strict JSON object")?;
    anyhow::ensure!(
        result.get("schema").and_then(Value::as_str)
            == Some("ctox.outbound.adapter_reconciliation.v1"),
        "adapter reconciliation reply has the wrong schema"
    );
    anyhow::ensure!(
        !outbound_reconciliation_contains_secret_key(&result),
        "adapter reconciliation reply must not contain credential values"
    );

    let expected_digest = outbound_required_string(&command.payload, &["configuration_digest"])?;
    anyhow::ensure!(
        result
            .get("configuration_digest")
            .and_then(Value::as_str)
            .is_some_and(|value| value == expected_digest),
        "adapter reconciliation result does not match the requested configuration"
    );
    let writeback = command
        .payload
        .get("writeback_contract")
        .context("adapter reconciliation writeback contract is required")?;
    let source_collection = outbound_required_string(writeback, &["source_collection"])?;
    let adapter_collection = outbound_required_string(writeback, &["adapter_collection"])?;
    anyhow::ensure!(
        is_safe_rxdb_collection_name(&source_collection)
            && is_safe_rxdb_collection_name(&adapter_collection),
        "invalid adapter reconciliation writeback collection"
    );
    let source_module = outbound_first_string(&[
        outbound_string(&command.client_context, &["source_module"]),
        Some(command.module.clone()),
    ])
    .context("adapter reconciliation source module is required")?;
    let module_prefix = runtime_app_starter_collection_name(&source_module)
        .trim_end_matches("_records")
        .to_string();
    anyhow::ensure!(
        !module_prefix.is_empty()
            && source_collection.starts_with(&format!("{module_prefix}_"))
            && adapter_collection.starts_with(&format!("{module_prefix}_")),
        "adapter reconciliation writeback must stay inside the source module"
    );

    let requested_sources = command
        .payload
        .get("sources")
        .and_then(Value::as_array)
        .context("adapter reconciliation sources are required")?;
    let mut source_ids = BTreeSet::new();
    let mut requested_by_id = std::collections::BTreeMap::new();
    for source in requested_sources {
        let source_id = outbound_required_string(source, &["id"])?;
        anyhow::ensure!(
            outbound_safe_reconciliation_identifier(&source_id, 160),
            "invalid source id in adapter reconciliation request"
        );
        source_ids.insert(source_id.clone());
        requested_by_id.insert(source_id, source.clone());
    }

    let result_status = outbound_required_string(&result, &["status"])?;
    anyhow::ensure!(
        matches!(result_status.as_str(), "completed" | "needs_attention"),
        "adapter reconciliation result has an unsupported status"
    );

    // Validate the complete agent result before the first projection. The
    // RxDB mirror and the native store cannot be rolled back as one database,
    // so content validation must be all-or-nothing ahead of any write.
    let discovered_sources = result
        .get("discovered_sources")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut validated_source_ids = source_ids.clone();
    for discovered in &discovered_sources {
        let source_id = outbound_required_string(discovered, &["id"])?;
        let url = outbound_required_string(discovered, &["url"])?;
        anyhow::ensure!(
            outbound_safe_reconciliation_identifier(&source_id, 160),
            "invalid discovered source id"
        );
        anyhow::ensure!(
            validated_source_ids.insert(source_id.clone()),
            "adapter reconciliation returned a source more than once"
        );
        let parsed = Url::parse(&url).context("discovered source URL is invalid")?;
        anyhow::ensure!(
            matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some(),
            "discovered source URL must be HTTP(S)"
        );
        if let Some(target_key) = outbound_string(discovered, &["target_key"]) {
            anyhow::ensure!(
                outbound_safe_reconciliation_identifier(&target_key, 128),
                "invalid discovered target key"
            );
        }
    }

    let adapter_results = result
        .get("adapters")
        .and_then(Value::as_array)
        .context("adapter reconciliation result requires adapters")?;
    let mut validated_adapter_sources = BTreeSet::new();
    let mut has_attention = false;
    for adapter in adapter_results {
        let source_id = outbound_required_string(adapter, &["source_id"])?;
        anyhow::ensure!(
            validated_source_ids.contains(&source_id),
            "adapter result references an undeclared source"
        );
        anyhow::ensure!(
            validated_adapter_sources.insert(source_id),
            "adapter reconciliation returned a source more than once"
        );
        let status = outbound_required_string(adapter, &["status"])?;
        anyhow::ensure!(
            matches!(
                status.as_str(),
                "ready" | "auth_required" | "failed" | "disabled" | "needs_attention"
            ),
            "unsupported adapter reconciliation status"
        );
        has_attention |= matches!(
            status.as_str(),
            "auth_required" | "failed" | "needs_attention"
        );
        let scrape_status = outbound_required_string(adapter, &["scrape_status"])?;
        anyhow::ensure!(
            outbound_safe_reconciliation_identifier(&scrape_status, 80),
            "invalid adapter scrape status"
        );
        if let Some(revision) = outbound_string(adapter, &["adapter_revision"]) {
            anyhow::ensure!(
                outbound_safe_reconciliation_revision(&revision),
                "invalid adapter revision"
            );
        }
        if let Some(script_path) = outbound_string(adapter, &["script_path"]) {
            anyhow::ensure!(
                outbound_safe_relative_script_path(&script_path),
                "invalid adapter script path"
            );
        }
        if let Some(test) = adapter.get("test") {
            anyhow::ensure!(test.is_object(), "adapter test result must be an object");
        }
        if status == "ready" {
            anyhow::ensure!(
                outbound_string(adapter, &["adapter_revision"]).is_some(),
                "ready adapter requires adapter_revision"
            );
            anyhow::ensure!(
                outbound_string(adapter, &["script_path"]).is_some(),
                "ready adapter requires script_path"
            );
            anyhow::ensure!(
                adapter.pointer("/test/ok").and_then(Value::as_bool) == Some(true),
                "ready adapter requires a passing test result"
            );
        }
    }
    let missing_sources = validated_source_ids
        .difference(&validated_adapter_sources)
        .cloned()
        .collect::<Vec<_>>();
    anyhow::ensure!(
        missing_sources.is_empty(),
        "adapter reconciliation omitted sources: {}",
        missing_sources.join(", ")
    );
    anyhow::ensure!(
        (result_status == "needs_attention") == has_attention,
        "adapter reconciliation status is inconsistent with adapter results"
    );

    let policy_collection = if outbound_string(&command.payload, &["policy_id"]).is_some() {
        let collection = outbound_required_string(writeback, &["policy_collection"])?;
        anyhow::ensure!(
            is_safe_rxdb_collection_name(&collection)
                && collection.starts_with(&format!("{module_prefix}_")),
            "adapter reconciliation policy writeback must stay inside the source module"
        );
        Some(collection)
    } else {
        None
    };

    let now = now_ms() as i64;
    let mut projected_sources = Vec::new();
    for discovered in discovered_sources {
        let source_id = outbound_required_string(&discovered, &["id"])?;
        let url = outbound_required_string(&discovered, &["url"])?;
        anyhow::ensure!(
            outbound_safe_reconciliation_identifier(&source_id, 160),
            "invalid discovered source id"
        );
        let parsed = Url::parse(&url).context("discovered source URL is invalid")?;
        anyhow::ensure!(
            matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some(),
            "discovered source URL must be HTTP(S)"
        );
        source_ids.insert(source_id.clone());
        let existing =
            load_rxdb_collection_record(root, &source_collection, &source_id)?.or_else(|| {
                outbound_load_record(conn, &source_collection, &source_id)
                    .ok()
                    .flatten()
            });
        let created_at = existing
            .as_ref()
            .and_then(|value| value.get("created_at_ms"))
            .and_then(Value::as_i64)
            .unwrap_or(now);
        let target_key = outbound_string(&discovered, &["target_key"])
            .unwrap_or_else(|| outbound_reconciliation_target_key(&source_id));
        anyhow::ensure!(
            outbound_safe_reconciliation_identifier(&target_key, 128),
            "invalid discovered target key"
        );
        let record = serde_json::json!({
            "id": source_id.clone(),
            "label": outbound_string(&discovered, &["label"]).unwrap_or_else(|| source_id.clone()),
            "url": url,
            "countries": discovered.get("countries").cloned().unwrap_or_else(|| serde_json::json!(["DE", "AT", "CH"])),
            "field_keys": discovered.get("field_keys").cloned().unwrap_or_else(|| command.payload.get("field_keys").cloned().unwrap_or_else(|| serde_json::json!([]))),
            "enabled": discovered.get("enabled").and_then(Value::as_bool).unwrap_or(true),
            "requires_credential": discovered.get("requires_credential").and_then(Value::as_bool).unwrap_or(false),
            "credential_secret_name": "",
            "target_key": target_key,
            "adapter_status": "reconciliation_completed",
            "scrape_status": "target_available",
            "auth_status": if discovered.get("requires_credential").and_then(Value::as_bool).unwrap_or(false) { "required" } else { "not_required" },
            "payload": {
                "builtin": false,
                "discovered_by_command_id": command_id,
                "discovery_reason": outbound_string(&discovered, &["reason"]).unwrap_or_default(),
                "secret_value_in_payload": false
            },
            "created_at_ms": created_at,
            "updated_at_ms": now
        });
        upsert_business_record(conn, &source_collection, &source_id, now, record.clone())?;
        upsert_rxdb_collection_record(root, &source_collection, &source_id, now, record.clone())?;
        requested_by_id.insert(source_id.clone(), record.clone());
        projected_sources.push(record);
    }

    let mut completed_source_ids = BTreeSet::new();
    let mut projected_adapters = Vec::new();
    for adapter_result in adapter_results {
        let source_id = outbound_required_string(adapter_result, &["source_id"])?;
        anyhow::ensure!(
            source_ids.contains(&source_id),
            "adapter result references an undeclared source"
        );
        anyhow::ensure!(
            completed_source_ids.insert(source_id.clone()),
            "adapter reconciliation returned a source more than once"
        );
        let requested = requested_by_id
            .get(&source_id)
            .context("adapter source definition is unavailable")?;
        let target_key = outbound_first_string(&[
            outbound_string(adapter_result, &["target_key"]),
            outbound_string(requested, &["target_key"]),
        ])
        .unwrap_or_else(|| outbound_reconciliation_target_key(&source_id));
        anyhow::ensure!(
            outbound_safe_reconciliation_identifier(&target_key, 128),
            "invalid adapter target key"
        );
        let adapter_id = outbound_first_string(&[
            outbound_string(requested, &["adapter_id"]),
            outbound_string(adapter_result, &["id"]),
        ])
        .unwrap_or_else(|| format!("adapter_leadgen_{target_key}"));
        anyhow::ensure!(
            outbound_safe_reconciliation_identifier(&adapter_id, 180),
            "invalid adapter id"
        );
        let existing = load_rxdb_collection_record(root, &adapter_collection, &adapter_id)?
            .or_else(|| {
                outbound_load_record(conn, &adapter_collection, &adapter_id)
                    .ok()
                    .flatten()
            });
        let created_at = existing
            .as_ref()
            .and_then(|value| value.get("created_at_ms"))
            .and_then(Value::as_i64)
            .unwrap_or(now);
        let status = outbound_required_string(adapter_result, &["status"])?;
        anyhow::ensure!(
            matches!(
                status.as_str(),
                "ready" | "auth_required" | "failed" | "disabled" | "needs_attention"
            ),
            "unsupported adapter reconciliation status"
        );
        let scrape_status = outbound_required_string(adapter_result, &["scrape_status"])?;
        let auth_status = outbound_string(adapter_result, &["auth_status"]).unwrap_or_else(|| {
            if status == "auth_required" {
                "auth_required".to_string()
            } else {
                "not_required".to_string()
            }
        });
        let last_error = outbound_string(adapter_result, &["last_error"]).unwrap_or_default();
        let mut payload = existing
            .as_ref()
            .and_then(|value| value.get("payload"))
            .filter(|value| value.is_object())
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        outbound_payload_insert(
            &mut payload,
            "configuration_digest",
            Value::String(expected_digest.clone()),
        );
        outbound_payload_insert(
            &mut payload,
            "reconciliation_command_id",
            Value::String(command_id.to_string()),
        );
        outbound_payload_insert(
            &mut payload,
            "reconciliation_task_id",
            Value::String(task_id.to_string()),
        );
        outbound_payload_insert(
            &mut payload,
            "reconciled_command_id",
            Value::String(command_id.to_string()),
        );
        outbound_payload_insert(
            &mut payload,
            "reconciled_command_status",
            Value::String(result_status.clone()),
        );
        outbound_payload_insert(
            &mut payload,
            "adapter_revision",
            adapter_result
                .get("adapter_revision")
                .cloned()
                .unwrap_or(Value::Null),
        );
        outbound_payload_insert(
            &mut payload,
            "script_path",
            adapter_result
                .get("script_path")
                .cloned()
                .unwrap_or(Value::Null),
        );
        outbound_payload_insert(
            &mut payload,
            "test",
            adapter_result.get("test").cloned().unwrap_or(Value::Null),
        );
        outbound_payload_insert(&mut payload, "secret_value_in_payload", Value::Bool(false));
        let record = serde_json::json!({
            "id": adapter_id.clone(),
            "source_id": source_id.clone(),
            "status": status.clone(),
            "scrape_status": scrape_status.clone(),
            "auth_status": auth_status.clone(),
            "last_command_id": command_id,
            "last_task_id": task_id,
            "last_error": last_error,
            "payload": payload,
            "created_at_ms": created_at,
            "updated_at_ms": now
        });
        upsert_business_record(conn, &adapter_collection, &adapter_id, now, record.clone())?;
        upsert_rxdb_collection_record(root, &adapter_collection, &adapter_id, now, record.clone())?;

        if let Some(mut source_record) =
            load_rxdb_collection_record(root, &source_collection, &source_id)?.or_else(|| {
                outbound_load_record(conn, &source_collection, &source_id)
                    .ok()
                    .flatten()
            })
        {
            outbound_put_string(&mut source_record, "adapter_status", status);
            outbound_put_string(&mut source_record, "scrape_status", scrape_status);
            outbound_put_string(&mut source_record, "auth_status", auth_status);
            outbound_put_i64(&mut source_record, "updated_at_ms", now);
            upsert_business_record(
                conn,
                &source_collection,
                &source_id,
                now,
                source_record.clone(),
            )?;
            upsert_rxdb_collection_record(
                root,
                &source_collection,
                &source_id,
                now,
                source_record,
            )?;
        }
        projected_adapters.push(record);
    }

    let missing_sources = source_ids
        .difference(&completed_source_ids)
        .cloned()
        .collect::<Vec<_>>();
    anyhow::ensure!(
        missing_sources.is_empty(),
        "adapter reconciliation omitted sources: {}",
        missing_sources.join(", ")
    );

    if let (Some(policy_id), Some(policy_collection)) = (
        outbound_string(&command.payload, &["policy_id"]),
        policy_collection,
    ) {
        if let Some(mut policy) = load_rxdb_collection_record(root, &policy_collection, &policy_id)?
        {
            outbound_put_string(&mut policy, "reconciliation_status", result_status.clone());
            outbound_put_string(&mut policy, "reconciliation_command_id", command_id);
            outbound_put_string(&mut policy, "reconciliation_task_id", task_id);
            outbound_put_string(&mut policy, "configuration_digest", expected_digest.clone());
            outbound_put_string(
                &mut policy,
                "reconciliation_error",
                if result_status == "needs_attention" {
                    "one or more adapters require operator attention"
                } else {
                    ""
                },
            );
            outbound_put_i64(&mut policy, "updated_at_ms", now);
            upsert_business_record(conn, &policy_collection, &policy_id, now, policy.clone())?;
            upsert_rxdb_collection_record(root, &policy_collection, &policy_id, now, policy)?;
        }
    }

    Ok(serde_json::json!({
        "ok": true,
        "schema": "ctox.outbound.adapter_reconciliation.v1",
        "configuration_digest": expected_digest,
        "status": result.get("status").cloned().unwrap_or_else(|| Value::String("completed".to_string())),
        "command_id": command_id,
        "execution_task_id": task_id,
        "task_id": task_id,
        "sources": projected_sources,
        "adapters": projected_adapters,
        "secret_value_in_payload": false
    }))
}

/// Project a reconciliation failure onto the same source/adapter/policy
/// records the app already renders. This keeps harness, schema-validation and
/// timeout failures visible and retryable instead of leaving an eternal
/// `reconciliation_queued` spinner.
pub(super) fn mark_outbound_adapter_reconciliation_failed(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    command_id: &str,
    task_id: &str,
    error: &str,
) -> anyhow::Result<()> {
    if !is_outbound_adapter_reconciliation_command(command) {
        return Ok(());
    }
    let writeback = command
        .payload
        .get("writeback_contract")
        .context("adapter reconciliation writeback contract is required")?;
    let source_collection = outbound_required_string(writeback, &["source_collection"])?;
    let adapter_collection = outbound_required_string(writeback, &["adapter_collection"])?;
    anyhow::ensure!(
        is_safe_rxdb_collection_name(&source_collection)
            && is_safe_rxdb_collection_name(&adapter_collection),
        "invalid adapter reconciliation writeback collection"
    );
    let source_module = outbound_first_string(&[
        outbound_string(&command.client_context, &["source_module"]),
        Some(command.module.clone()),
    ])
    .context("adapter reconciliation source module is required")?;
    let module_prefix = runtime_app_starter_collection_name(&source_module)
        .trim_end_matches("_records")
        .to_string();
    anyhow::ensure!(
        !module_prefix.is_empty()
            && source_collection.starts_with(&format!("{module_prefix}_"))
            && adapter_collection.starts_with(&format!("{module_prefix}_")),
        "adapter reconciliation failure writeback must stay inside the source module"
    );
    let now = now_ms() as i64;
    for requested in command
        .payload
        .get("sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let source_id = outbound_required_string(requested, &["id"])?;
        if let Some(mut source) = load_rxdb_collection_record(root, &source_collection, &source_id)?
            .or_else(|| {
                outbound_load_record(conn, &source_collection, &source_id)
                    .ok()
                    .flatten()
            })
        {
            outbound_put_string(&mut source, "adapter_status", "needs_attention");
            outbound_put_string(&mut source, "scrape_status", "failed");
            outbound_put_string(&mut source, "last_error", error);
            outbound_put_i64(&mut source, "updated_at_ms", now);
            upsert_business_record(conn, &source_collection, &source_id, now, source.clone())?;
            upsert_rxdb_collection_record(root, &source_collection, &source_id, now, source)?;
        }
        let target_key = outbound_string(requested, &["target_key"])
            .unwrap_or_else(|| outbound_reconciliation_target_key(&source_id));
        let adapter_id = outbound_string(requested, &["adapter_id"])
            .unwrap_or_else(|| format!("adapter_leadgen_{target_key}"));
        if let Some(mut adapter) =
            load_rxdb_collection_record(root, &adapter_collection, &adapter_id)?.or_else(|| {
                outbound_load_record(conn, &adapter_collection, &adapter_id)
                    .ok()
                    .flatten()
            })
        {
            outbound_put_string(&mut adapter, "status", "failed");
            outbound_put_string(&mut adapter, "scrape_status", "failed");
            outbound_put_string(&mut adapter, "last_command_id", command_id);
            outbound_put_string(&mut adapter, "last_task_id", task_id);
            outbound_put_string(&mut adapter, "last_error", error);
            outbound_put_i64(&mut adapter, "updated_at_ms", now);
            upsert_business_record(conn, &adapter_collection, &adapter_id, now, adapter.clone())?;
            upsert_rxdb_collection_record(root, &adapter_collection, &adapter_id, now, adapter)?;
        }
    }
    if let (Some(policy_id), Ok(policy_collection)) = (
        outbound_string(&command.payload, &["policy_id"]),
        outbound_required_string(writeback, &["policy_collection"]),
    ) {
        if is_safe_rxdb_collection_name(&policy_collection)
            && policy_collection.starts_with(&format!("{module_prefix}_"))
        {
            if let Some(mut policy) =
                load_rxdb_collection_record(root, &policy_collection, &policy_id)?
            {
                outbound_put_string(&mut policy, "reconciliation_status", "failed");
                outbound_put_string(&mut policy, "reconciliation_command_id", command_id);
                outbound_put_string(&mut policy, "reconciliation_task_id", task_id);
                outbound_put_string(&mut policy, "reconciliation_error", error);
                outbound_put_i64(&mut policy, "updated_at_ms", now);
                upsert_business_record(conn, &policy_collection, &policy_id, now, policy.clone())?;
                upsert_rxdb_collection_record(root, &policy_collection, &policy_id, now, policy)?;
            }
        }
    }
    Ok(())
}

fn outbound_safe_reconciliation_identifier(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn outbound_safe_relative_script_path(value: &str) -> bool {
    let path = Path::new(value);
    !value.is_empty()
        && value.len() <= 512
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
        && matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("js" | "mjs")
        )
}

fn outbound_safe_reconciliation_revision(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 180
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
}

fn outbound_reconciliation_target_key(source_id: &str) -> String {
    source_id
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() {
                (byte as char).to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn outbound_reconciliation_contains_secret_key(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, child)| {
            let key = key.trim().to_ascii_lowercase().replace('-', "_");
            matches!(
                key.as_str(),
                "password"
                    | "passphrase"
                    | "secret"
                    | "credential_value"
                    | "api_key"
                    | "access_token"
                    | "refresh_token"
                    | "private_key"
            ) || key.ends_with("_password")
                || key.ends_with("_secret")
                || key.ends_with("_token")
                || outbound_reconciliation_contains_secret_key(child)
        }),
        Value::Array(items) => items
            .iter()
            .any(outbound_reconciliation_contains_secret_key),
        _ => false,
    }
}

fn outbound_handle_research_source_adapter(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
    next_status: &str,
) -> anyhow::Result<Value> {
    let adapter_payload = command.payload.get("adapter").unwrap_or(&command.payload);
    let adapter_id = outbound_first_string(&[
        outbound_string(&command.payload, &["adapter_id"]),
        outbound_string(adapter_payload, &["id"]),
        command
            .record_id
            .as_ref()
            .map(|value| value.trim().to_string()),
    ])
    .context("adapter_id is required")?;
    let source_id = outbound_first_string(&[
        outbound_string(adapter_payload, &["source_id"]),
        outbound_string(adapter_payload, &["id"]),
        outbound_string(&command.payload, &["source_id"]),
    ])
    .context("source_id is required")?;
    let url = outbound_first_string(&[
        outbound_string(adapter_payload, &["url"]),
        outbound_string(&command.payload, &["url"]),
    ])
    .unwrap_or_else(|| format!("https://{source_id}/"));

    let mut record =
        outbound_load_record_or_rxdb(root, conn, "outbound_research_adapters", &adapter_id)?
            .unwrap_or_else(|| outbound_object_payload(adapter_payload));
    outbound_put_string(&mut record, "id", adapter_id.clone());
    outbound_put_string(
        &mut record,
        "campaign_id",
        outbound_first_string(&[
            outbound_string(adapter_payload, &["campaign_id"]),
            outbound_string(&command.payload, &["campaign_id"]),
            outbound_string(&command.client_context, &["campaign_id"]),
        ])
        .unwrap_or_default(),
    );
    outbound_put_string(&mut record, "source_id", source_id.clone());
    outbound_put_string(
        &mut record,
        "label",
        outbound_first_string(&[
            outbound_string(adapter_payload, &["label"]),
            outbound_string(&command.payload, &["label"]),
            Some(source_id.clone()),
        ])
        .unwrap_or_else(|| source_id.clone()),
    );
    outbound_put_string(&mut record, "url", url.clone());
    outbound_put_string(
        &mut record,
        "adapter_kind",
        outbound_first_string(&[
            outbound_string(adapter_payload, &["adapter_kind"]),
            outbound_string(&command.payload, &["adapter_kind"]),
        ])
        .unwrap_or_else(|| "custom_url".to_string()),
    );
    outbound_put_string(
        &mut record,
        "target_key",
        outbound_first_string(&[
            outbound_string(adapter_payload, &["target_key"]),
            outbound_string(&command.payload, &["target_key"]),
        ])
        .unwrap_or_default(),
    );
    outbound_put_string(&mut record, "status", next_status);
    if let Some(object) = record.as_object_mut() {
        let enabled = adapter_payload
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        object.insert("enabled".to_string(), Value::Bool(enabled));
        object.insert(
            "countries".to_string(),
            adapter_payload
                .get("countries")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
        );
        object.insert(
            "field_keys".to_string(),
            adapter_payload
                .get("field_keys")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
        );
        object.insert(
            "requires_credential".to_string(),
            Value::Bool(
                adapter_payload
                    .get("requires_credential")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
        );
    }
    outbound_merge_fields(
        &mut record,
        adapter_payload,
        &[
            "tier",
            "credential_secret_name",
            "auth_mode",
            "auth_status",
            "scrape_status",
            "last_run_id",
            "last_success_at_ms",
            "last_error",
        ],
    );
    // Customer app adapter schemas require a complete state tuple even when
    // the current command only changes authentication. Keep writeback records
    // valid for direct RxDB projection instead of relying on the browser's
    // optimistic patch to supply a missing scrape state.
    outbound_put_default_string(&mut record, "scrape_status", "target_available");
    if next_status == "adapter_requested" {
        outbound_put_string(&mut record, "scrape_status", "registration_requested");
    } else if next_status == "test_requested" {
        outbound_put_string(&mut record, "scrape_status", "test_requested");
    } else if next_status == "auth_requested" {
        outbound_put_string(&mut record, "auth_status", "auth_requested");
    }
    outbound_put_string(&mut record, "last_error", "");
    outbound_put_default_object(&mut record, "payload");
    outbound_payload_insert(
        &mut record,
        "last_command_id",
        Value::String(command.id.clone().unwrap_or_default()),
    );
    outbound_payload_insert(
        &mut record,
        "last_command_type",
        Value::String(command.command_type.clone()),
    );
    if let Some(contract) = command.payload.get("scrape_contract") {
        outbound_payload_insert(&mut record, "scrape_contract", contract.clone());
    }
    if let Some(manifest) = command.payload.get("target_manifest") {
        outbound_payload_insert(&mut record, "target_manifest", manifest.clone());
    }
    outbound_payload_insert(&mut record, "secret_value_in_payload", Value::Bool(false));
    if let Some(scrape_effect) = outbound_apply_research_adapter_scrape_effect(
        root,
        command,
        adapter_payload,
        &adapter_id,
        &source_id,
        &mut record,
    ) {
        outbound_payload_insert(&mut record, "scrape_registry_effect", scrape_effect);
    }
    let credential_effect = outbound_apply_research_adapter_credential_status(
        root,
        adapter_payload,
        &mut record,
        next_status,
    );
    if outbound_string(&record, &["status"]).as_deref() == Some("test_auth_required") {
        outbound_put_string(&mut record, "auth_status", "auth_required");
    }
    outbound_payload_insert(&mut record, "credential_ref", credential_effect);
    outbound_put_default_i64(&mut record, "created_at_ms", now);
    outbound_put_i64(&mut record, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_research_adapters",
        &adapter_id,
        now,
        record.clone(),
    )?;
    // Ohne diese Projektion bleibt der Adapter im nativen `business_records`
    // stehen und erreicht den Browser nie: der Befehl meldet dann `completed`
    // mit `ok: true`, waehrend die Oberflaeche keinen Adapter kennt. Auf der
    // Produktivinstanz war der neu erzeugte Adapter der einzige von 19
    // nativen Datensaetzen, der in der Replikationsdatenbank fehlte. Der
    // writeback-Pfad weiter unten schreibt aus demselben Grund bereits durch.
    upsert_rxdb_collection_record(
        root,
        "outbound_research_adapters",
        &adapter_id,
        now,
        record.clone(),
    )?;
    let auth_assist = if next_status == "auth_requested" {
        let secret_name = outbound_string(&record, &["credential_secret_name"]).unwrap_or_default();
        let credential_ref = (!secret_name.is_empty()).then(|| {
            format!(
                "ctox-secret://{}/{secret_name}",
                crate::secrets::credential_scope()
            )
        });
        let requesting_task_id = command
            .id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&adapter_id);
        let owner_user_id = command
            .client_context
            .pointer("/actor/id")
            .or_else(|| command.client_context.get("user_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let effect = crate::service::business_os::enqueue_web_stack_auth_assist_request(
            root,
            &source_id,
            Some(&url),
            credential_ref.as_deref(),
            None,
            command
                .payload
                .pointer("/record_snapshot/auth_assist")
                .or_else(|| command.payload.get("auth_assist")),
            requesting_task_id,
            &command.module,
            &command.command_type,
            owner_user_id,
            true,
            true,
        )?;
        outbound_put_string(&mut record, "auth_status", "browser_session_requested");
        outbound_payload_insert(&mut record, "auth_assist", effect.clone());
        outbound_put_i64(&mut record, "updated_at_ms", now);
        upsert_business_record(
            conn,
            "outbound_research_adapters",
            &adapter_id,
            now,
            record.clone(),
        )?;
        upsert_rxdb_collection_record(
            root,
            "outbound_research_adapters",
            &adapter_id,
            now,
            record.clone(),
        )?;
        Some(effect)
    } else {
        None
    };
    outbound_apply_research_adapter_writeback(root, conn, command, &adapter_id, &record, now)?;
    Ok(serde_json::json!({
        "ok": true,
        "collection": "outbound_research_adapters",
        "adapter": record,
        "adapter_id": adapter_id,
        "command_effect": next_status,
        "auth_assist": auth_assist,
        "secret_value_in_payload": false
    }))
}

fn outbound_apply_research_adapter_writeback(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    adapter_id: &str,
    record: &Value,
    now: i64,
) -> anyhow::Result<()> {
    let Some(writeback) = command.payload.get("writeback") else {
        return Ok(());
    };
    let collection = writeback
        .get("collection")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("adapter writeback collection is required")?;
    anyhow::ensure!(
        is_safe_rxdb_collection_name(collection),
        "invalid adapter writeback collection"
    );
    let source_module = outbound_first_string(&[
        outbound_string(&command.client_context, &["source_module"]),
        outbound_string(&command.client_context, &["module_id"]),
        outbound_string(&command.client_context, &["app_id"]),
    ])
    .context("adapter writeback requires a source module")?;
    let module_prefix = runtime_app_starter_collection_name(&source_module)
        .trim_end_matches("_records")
        .to_string();
    anyhow::ensure!(
        !module_prefix.is_empty() && collection.starts_with(&format!("{module_prefix}_")),
        "adapter writeback collection must be scoped to the source module"
    );
    let record_id = writeback
        .get("record_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(adapter_id);
    anyhow::ensure!(
        record_id == adapter_id,
        "adapter writeback record_id must match adapter_id"
    );
    let mut projection = record.clone();
    outbound_put_string(&mut projection, "id", record_id.to_string());
    outbound_put_string(
        &mut projection,
        "last_command_id",
        command.id.clone().unwrap_or_default(),
    );
    outbound_put_i64(&mut projection, "updated_at_ms", now);
    upsert_business_record(conn, collection, record_id, now, projection.clone())?;
    // Tenant modules read their adapter state from RxDB/WebRTC. Persisting the
    // writeback only in `business_records` leaves the browser collection empty
    // (and the Browser app's Scraping view therefore blank), especially for
    // on-demand local-module collections that the generic projector does not
    // eagerly scan. Write through the canonical native RxDB writer as part of
    // the same command outcome; this is the same server-authoritative data
    // path used by MCP projection upserts.
    upsert_rxdb_collection_record(root, collection, record_id, now, projection)?;
    Ok(())
}

fn outbound_apply_research_adapter_scrape_effect(
    root: &Path,
    command: &BusinessCommand,
    adapter_payload: &Value,
    adapter_id: &str,
    source_id: &str,
    record: &mut Value,
) -> Option<Value> {
    let command_type = command.command_type.as_str();
    if !matches!(
        command_type,
        "outbound.research_source.generate_adapter" | "outbound.research_source.test"
    ) {
        return None;
    }
    let scrape_effect_started = Instant::now();
    if let Some(object) = record.as_object_mut() {
        object.insert("test_ok".to_string(), Value::Bool(false));
    }
    let adapter_kind = outbound_string(record, &["adapter_kind"])
        .or_else(|| outbound_string(adapter_payload, &["adapter_kind"]))
        .unwrap_or_else(|| "custom_url".to_string());
    if !matches!(adapter_kind.as_str(), "scrape_target" | "custom_url") {
        return Some(serde_json::json!({
            "ok": true,
            "skipped": true,
            "reason": "source_is_not_scrape_target",
            "adapter_kind": adapter_kind,
        }));
    }
    let Some(target_key) = outbound_first_string(&[
        outbound_string(record, &["target_key"]),
        outbound_string(adapter_payload, &["target_key"]),
        outbound_string(&command.payload, &["target_key"]),
    ]) else {
        let message = "target_key is required for scrape adapter";
        let mut effect = serde_json::json!({
            "ok": false,
            "error": message,
        });
        if command_type == "outbound.research_source.test" {
            let test_effect = outbound_persist_scrape_test_preflight_failure(
                record,
                "test_failed",
                "target_key_missing",
                message,
                scrape_effect_started.elapsed(),
            );
            effect["test"] = test_effect;
        } else {
            outbound_put_string(record, "status", "adapter_failed");
            outbound_put_string(record, "scrape_status", "target_key_missing");
            outbound_put_string(record, "last_error", message);
        }
        return Some(effect);
    };

    let registration = outbound_register_research_scrape_target(
        root,
        adapter_payload,
        record,
        adapter_id,
        source_id,
        &target_key,
    );
    let mut effect = match registration {
        Ok(effect) => effect,
        Err(err) => {
            let message = err.to_string();
            let mut effect = serde_json::json!({
                "ok": false,
                "phase": "register",
                "target_key": target_key,
                "error": message.clone(),
            });
            if command_type == "outbound.research_source.test" {
                let test_effect = outbound_persist_scrape_test_preflight_failure(
                    record,
                    "test_failed",
                    "registration_failed",
                    &message,
                    scrape_effect_started.elapsed(),
                );
                effect["test"] = test_effect;
            } else {
                outbound_put_string(record, "status", "adapter_failed");
                outbound_put_string(record, "scrape_status", "registration_failed");
                outbound_put_string(record, "last_error", message);
            }
            return Some(effect);
        }
    };

    let has_script = effect
        .get("script_registered")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if command_type == "outbound.research_source.generate_adapter" {
        if !has_script {
            match outbound_queue_research_scraper_generation(
                root,
                command,
                adapter_payload,
                source_id,
                &target_key,
            ) {
                Ok(task_effect) => {
                    if let Some(object) = effect.as_object_mut() {
                        object.insert("generation_task".to_string(), task_effect);
                    }
                    outbound_put_string(record, "status", "generation_queued");
                    outbound_put_string(record, "scrape_status", "generation_queued");
                    return Some(effect);
                }
                Err(err) => {
                    let message = err.to_string();
                    if let Some(object) = effect.as_object_mut() {
                        object.insert(
                            "generation_task".to_string(),
                            serde_json::json!({ "ok": false, "error": message }),
                        );
                    }
                    outbound_put_string(record, "status", "adapter_failed");
                    outbound_put_string(record, "scrape_status", "generation_failed");
                    outbound_put_string(record, "last_error", message);
                    return Some(effect);
                }
            }
        }
        outbound_put_string(record, "status", "adapter_ready");
        outbound_put_string(
            record,
            "scrape_status",
            if has_script {
                "registered"
            } else {
                "script_required"
            },
        );
        return Some(effect);
    }

    if !has_script {
        let message = "scrape target has no registered script yet";
        let test_effect = outbound_persist_scrape_test_preflight_failure(
            record,
            "test_blocked",
            "script_required",
            message,
            scrape_effect_started.elapsed(),
        );
        if let Some(object) = effect.as_object_mut() {
            object.insert("test_skipped".to_string(), Value::Bool(true));
            object.insert(
                "test_skip_reason".to_string(),
                Value::String(message.to_string()),
            );
            object.insert("test".to_string(), test_effect);
        }
        return Some(effect);
    }

    let test_started = Instant::now();
    match outbound_execute_research_scrape_target(
        root,
        command,
        adapter_payload,
        source_id,
        &target_key,
    ) {
        Ok(test_outcome) => {
            let expected_fields = record
                .get("field_keys")
                .or_else(|| adapter_payload.get("field_keys"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|field| !field.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            let extracted_fields_lower = test_outcome
                .fields_extracted
                .iter()
                .map(|field| field.to_ascii_lowercase())
                .collect::<BTreeSet<_>>();
            let missing_fields = expected_fields
                .iter()
                .filter(|field| !extracted_fields_lower.contains(&field.to_ascii_lowercase()))
                .cloned()
                .collect::<Vec<_>>();
            let has_expected_fields =
                !test_outcome.fields_extracted.is_empty() && missing_fields.is_empty();
            let mut test_effect = serde_json::to_value(&test_outcome).unwrap_or_else(|err| {
                serde_json::json!({
                    "ok": false,
                    "run_id": test_outcome.run_id,
                    "status": test_outcome.status.as_str(),
                    "error": format!("failed to serialize scrape outcome: {err}"),
                })
            });
            if let Some(object) = test_effect.as_object_mut() {
                object.insert(
                    "expected_fields".to_string(),
                    serde_json::json!(expected_fields),
                );
                object.insert(
                    "missing_fields".to_string(),
                    serde_json::json!(missing_fields),
                );
            }
            let evidence = outbound_scrape_test_evidence(&test_effect);
            let evidence_valid = evidence.get("valid").and_then(Value::as_bool) == Some(true);
            let test_ok = outbound_scrape_test_passed(
                test_outcome.status,
                test_outcome.records_found,
                has_expected_fields,
                evidence_valid,
            );
            if let Some(object) = test_effect.as_object_mut() {
                object.insert("ok".to_string(), Value::Bool(test_ok));
                object.insert("test_ok".to_string(), Value::Bool(test_ok));
                object.insert("evidence".to_string(), evidence.clone());
                object.insert(
                    "tested_at_ms".to_string(),
                    Value::from(now_ms().min(i64::MAX as u128) as i64),
                );
            }
            outbound_put_string(record, "last_run_id", test_outcome.run_id.clone());

            if test_ok {
                outbound_put_string(record, "status", "test_ok");
                outbound_put_string(record, "scrape_status", "test_executed");
                outbound_put_string(record, "last_error", "");
                outbound_put_i64(
                    record,
                    "last_success_at_ms",
                    now_ms().min(i64::MAX as u128) as i64,
                );
            } else {
                let (status, scrape_status) = if test_outcome.reason
                    == "explicit_failure_mode_auth_required"
                {
                    ("test_auth_required", "test_auth_required")
                } else if test_outcome.reason.starts_with("command_failed_exit_") {
                    ("test_failed", "test_error")
                } else if test_outcome.records_found == 0
                    && test_outcome.reason == "empty_record_set_on_reachable_portal"
                {
                    ("test_zero_records", "test_zero_records")
                } else {
                    match test_outcome.status {
                        scrape::ScrapeRunStatus::AuthorizationRequired => {
                            ("test_auth_required", "test_auth_required")
                        }
                        scrape::ScrapeRunStatus::Blocked => ("test_blocked", "test_blocked"),
                        scrape::ScrapeRunStatus::PortalDrift => {
                            ("test_portal_drift", "test_portal_drift")
                        }
                        scrape::ScrapeRunStatus::TemporaryUnreachable => {
                            ("test_temporary_unreachable", "test_temporary_unreachable")
                        }
                        scrape::ScrapeRunStatus::PartialOutput => {
                            ("test_partial_output", "test_partial_output")
                        }
                        scrape::ScrapeRunStatus::Succeeded if test_outcome.records_found == 0 => {
                            ("test_zero_records", "test_zero_records")
                        }
                        scrape::ScrapeRunStatus::Succeeded if !evidence_valid => {
                            ("test_evidence_invalid", "test_evidence_invalid")
                        }
                        scrape::ScrapeRunStatus::Succeeded => {
                            ("test_fields_missing", "test_fields_missing")
                        }
                    }
                };
                let diagnosis_prefix = test_outcome.error.clone().unwrap_or_else(|| {
                    format!(
                        "scrape test validation failed: status={}",
                        test_outcome.status.as_str()
                    )
                });
                let diagnosis = format!(
                    "{diagnosis_prefix}; records_found={}; expected_fields={:?}; extracted_fields={:?}; missing_fields={:?}; latency_ms={}",
                    test_outcome.records_found,
                    expected_fields,
                    test_outcome.fields_extracted,
                    missing_fields,
                    test_outcome.latency_ms,
                );
                outbound_put_string(record, "status", status);
                outbound_put_string(record, "scrape_status", scrape_status);
                outbound_put_string(record, "last_error", diagnosis.clone());
                if status == "test_auth_required" {
                    outbound_put_string(record, "auth_status", "auth_required");
                }
                if let Some(object) = test_effect.as_object_mut() {
                    object.insert("error".to_string(), Value::String(diagnosis));
                }
            }
            outbound_persist_scrape_test_observation(record, &test_effect, &evidence);
            if let Some(object) = effect.as_object_mut() {
                object.insert("test".to_string(), test_effect);
            }
            Some(effect)
        }
        Err(err) => {
            let message = err.to_string();
            let latency_ms = test_started.elapsed().as_millis().min(i64::MAX as u128) as i64;
            outbound_put_string(record, "status", "test_failed");
            outbound_put_string(record, "scrape_status", "test_error");
            outbound_put_string(record, "last_error", message.clone());
            let test_effect = serde_json::json!({
                "ok": false,
                "test_ok": false,
                "status": "execution_error",
                "records_found": 0,
                "latency_ms": latency_ms,
                "error": message,
                "evidence": {
                    "valid": false,
                    "reason": "scrape execution failed before a durable run outcome was returned"
                },
                "tested_at_ms": now_ms().min(i64::MAX as u128) as i64,
            });
            let evidence = test_effect
                .get("evidence")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({ "valid": false }));
            outbound_persist_scrape_test_observation(record, &test_effect, &evidence);
            if let Some(object) = effect.as_object_mut() {
                object.insert("test".to_string(), test_effect);
            }
            Some(effect)
        }
    }
}

fn outbound_scrape_test_passed(
    status: scrape::ScrapeRunStatus,
    records_found: i64,
    has_expected_fields: bool,
    evidence_valid: bool,
) -> bool {
    status == scrape::ScrapeRunStatus::Succeeded
        && records_found > 0
        && has_expected_fields
        && evidence_valid
}

fn outbound_scrape_test_evidence(test_effect: &Value) -> Value {
    let run_id = test_effect
        .get("run_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let target_key = test_effect
        .get("target_key")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let status = test_effect
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let records_found = test_effect
        .get("records_found")
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    let run_manifest_path = test_effect
        .get("run_manifest_path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let manifest_path = PathBuf::from(run_manifest_path);
    let manifest = fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let run_id_matches = manifest
        .as_ref()
        .and_then(|value| value.get("run_id"))
        .and_then(Value::as_str)
        == Some(run_id)
        && !run_id.is_empty();
    let target_key_matches = manifest
        .as_ref()
        .and_then(|value| value.get("target_key"))
        .and_then(Value::as_str)
        == Some(target_key)
        && !target_key.is_empty();
    let status_matches = manifest
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
        == Some(status)
        && !status.is_empty();
    let record_count_matches = manifest
        .as_ref()
        .and_then(|value| value.pointer("/result/records_found"))
        .and_then(Value::as_i64)
        == Some(records_found)
        && records_found >= 0;
    let result_artifact = outbound_scrape_artifact_evidence(manifest.as_ref(), "result_json", None);
    let records_artifact =
        outbound_scrape_artifact_evidence(manifest.as_ref(), "records_json", Some(records_found));
    let valid = manifest_path.is_file()
        && manifest.is_some()
        && run_id_matches
        && target_key_matches
        && status_matches
        && record_count_matches
        && result_artifact.get("valid").and_then(Value::as_bool) == Some(true)
        && records_artifact.get("valid").and_then(Value::as_bool) == Some(true);
    serde_json::json!({
        "valid": valid,
        "run_id": run_id,
        "run_manifest_path": run_manifest_path,
        "run_manifest_exists": manifest_path.is_file(),
        "run_manifest_parsed": manifest.is_some(),
        "run_id_matches": run_id_matches,
        "target_key_matches": target_key_matches,
        "status_matches": status_matches,
        "record_count_matches": record_count_matches,
        "result_artifact": result_artifact,
        "records_artifact": records_artifact,
    })
}

fn outbound_scrape_artifact_evidence(
    manifest: Option<&Value>,
    artifact_kind: &str,
    expected_record_count: Option<i64>,
) -> Value {
    let artifact = manifest
        .and_then(|value| value.get("artifacts"))
        .and_then(Value::as_array)
        .and_then(|artifacts| {
            artifacts.iter().find(|artifact| {
                artifact.get("artifact_kind").and_then(Value::as_str) == Some(artifact_kind)
            })
        });
    let path = artifact
        .and_then(|value| value.get("path"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let declared_sha256 = artifact
        .and_then(|value| value.get("content_sha256"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let record_count = artifact
        .and_then(|value| value.get("record_count"))
        .and_then(Value::as_i64);
    let artifact_path = PathBuf::from(path);
    let actual_sha256 = file_sha256(&artifact_path)
        .ok()
        .map(|(_, sha256)| sha256)
        .unwrap_or_default();
    let record_count_matches = expected_record_count
        .map(|expected| record_count == Some(expected))
        .unwrap_or(true);
    let valid = artifact_path.is_file()
        && !declared_sha256.is_empty()
        && declared_sha256 == actual_sha256
        && record_count_matches;
    serde_json::json!({
        "valid": valid,
        "artifact_kind": artifact_kind,
        "path": path,
        "exists": artifact_path.is_file(),
        "declared_sha256": declared_sha256,
        "actual_sha256": actual_sha256,
        "sha256_matches": !declared_sha256.is_empty() && declared_sha256 == actual_sha256,
        "record_count": record_count,
        "record_count_matches": record_count_matches,
    })
}

fn outbound_persist_scrape_test_observation(
    record: &mut Value,
    test_effect: &Value,
    evidence: &Value,
) {
    let test_ok = test_effect.get("test_ok").and_then(Value::as_bool) == Some(true);
    let latency_ms = test_effect
        .get("latency_ms")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        .min(i64::MAX as u64) as i64;
    if let Some(object) = record.as_object_mut() {
        object.insert("test_ok".to_string(), Value::Bool(test_ok));
        object.insert("last_test".to_string(), test_effect.clone());
        object.insert("latency_ms".to_string(), Value::from(latency_ms));
        object.insert("evidence".to_string(), evidence.clone());
    }
    outbound_payload_insert(record, "scrape_test_diagnostics", test_effect.clone());
    outbound_payload_insert(record, "last_test", test_effect.clone());
    outbound_payload_insert(record, "test_latency_ms", Value::from(latency_ms));
    outbound_payload_insert(record, "test_evidence", evidence.clone());
}

fn outbound_persist_scrape_test_preflight_failure(
    record: &mut Value,
    status: &str,
    scrape_status: &str,
    error: &str,
    latency: Duration,
) -> Value {
    let latency_ms = latency.as_millis().min(i64::MAX as u128) as i64;
    outbound_put_string(record, "status", status);
    outbound_put_string(record, "scrape_status", scrape_status);
    outbound_put_string(record, "last_error", error);
    let evidence = serde_json::json!({
        "valid": false,
        "reason": "scrape test did not produce a durable run outcome",
    });
    let test_effect = serde_json::json!({
        "ok": false,
        "test_ok": false,
        "status": scrape_status,
        "records_found": 0,
        "latency_ms": latency_ms,
        "error": error,
        "evidence": evidence.clone(),
        "tested_at_ms": now_ms().min(i64::MAX as u128) as i64,
    });
    outbound_persist_scrape_test_observation(record, &test_effect, &evidence);
    test_effect
}

fn outbound_queue_research_scraper_generation(
    root: &Path,
    command: &BusinessCommand,
    adapter_payload: &Value,
    source_id: &str,
    target_key: &str,
) -> anyhow::Result<Value> {
    let label = outbound_first_string(&[
        outbound_string(adapter_payload, &["label"]),
        Some(source_id.to_string()),
    ])
    .unwrap_or_else(|| source_id.to_string());
    let url = outbound_string(adapter_payload, &["url"]).unwrap_or_default();
    let manifest = adapter_payload
        .get("target_manifest")
        .or_else(|| {
            adapter_payload
                .get("payload")
                .and_then(|payload| payload.get("target_manifest"))
        })
        .or_else(|| command.payload.get("target_manifest"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let contract = adapter_payload
        .get("scrape_contract")
        .or_else(|| {
            adapter_payload
                .get("payload")
                .and_then(|payload| payload.get("scrape_contract"))
        })
        .or_else(|| command.payload.get("scrape_contract"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let command_id = command.id.clone().unwrap_or_default();
    let task_idempotency_key = if command_id.is_empty() {
        format!("outbound-research-adapter:{target_key}:legacy:{}", now_ms())
    } else {
        format!("outbound-research-adapter:{target_key}:{command_id}")
    };
    let prompt = format!(
        "Erzeuge oder repariere einen CTOX Universal-Scraping Adapter fuer die Outbound Research Quelle.\n\
         Ziel:\n\
         - source_id: {source_id}\n\
         - label: {label}\n\
         - url: {url}\n\
         - target_key: {target_key}\n\n\
         Anforderungen:\n\
         - Nutze den universal-scraping Skill.\n\
         - Lege den ausfuehrbaren Scraper als JavaScript unter runtime/scraping/targets/{target_key}/scripts/ ab.\n\
         - Registriere das Target mit `ctox scrape upsert-target` und den Script-Stand mit `ctox scrape register-script`.\n\
         - Der Scraper muss `prospect.v1` Records ausgeben: field, value, confidence, source_url, note.\n\
         - Verwende keine Credential-Werte im Prompt, Code oder Log. Nur Secret-Referenzen sind erlaubt.\n\
         - Fuehre danach `ctox scrape execute --target-key {target_key} --allow-heal` mit einem kleinen Testinput aus und dokumentiere Run-ID, Quellen und Felder.\n\n\
         target_manifest:\n{manifest}\n\n\
         scrape_contract:\n{contract}\n"
    );
    let task = channels::create_queue_task(
        root,
        channels::QueueTaskCreateRequest {
            title: format!("Outbound Scraper Adapter erzeugen: {label}"),
            prompt,
            thread_key: format!("business-os/outbound/research-adapter/{target_key}"),
            workspace_root: Some(root.display().to_string()),
            // Adapter generation is background maintenance; it must not
            // outrank the research that needs the adapter (thesen 07.09.2026).
            priority: "low".to_string(),
            suggested_skill: Some("universal-scraping".to_string()),
            parent_message_key: None,
            extra_metadata: Some(serde_json::json!({
                "business_os_command_id": command_id,
                "source": "outbound.research_source.generate_adapter",
                "adapter_source_id": source_id,
                "target_key": target_key,
                "idempotency_key": task_idempotency_key,
            })),
        },
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "task_id": task.message_key,
        "task_status": task.route_status,
        "target_key": target_key,
        "suggested_skill": "universal-scraping",
        "secret_value_in_payload": false,
    }))
}

fn outbound_apply_research_adapter_credential_status(
    root: &Path,
    adapter_payload: &Value,
    record: &mut Value,
    next_status: &str,
) -> Value {
    let requires_credential = record
        .get("requires_credential")
        .and_then(Value::as_bool)
        .or_else(|| {
            adapter_payload
                .get("requires_credential")
                .and_then(Value::as_bool)
        })
        .unwrap_or(false);
    if !requires_credential {
        outbound_put_string(record, "auth_status", "not_required");
        return serde_json::json!({
            "required": false,
            "scope": crate::secrets::credential_scope(),
            "secret_value_in_payload": false,
        });
    }

    let secret_name = outbound_first_string(&[
        outbound_string(record, &["credential_secret_name"]),
        outbound_string(adapter_payload, &["credential_secret_name"]),
    ]);
    let Some(secret_name) = secret_name else {
        outbound_put_string(record, "auth_status", "credential_name_missing");
        return serde_json::json!({
            "required": true,
            "scope": crate::secrets::credential_scope(),
            "name": "",
            "exists": false,
            "secret_value_in_payload": false,
        });
    };

    let exists =
        crate::secrets::secret_exists(root, crate::secrets::credential_scope(), &secret_name)
            .unwrap_or(false);
    let status = if exists {
        "credential_available"
    } else if next_status == "auth_requested" {
        "auth_requested"
    } else {
        "credential_missing"
    };
    outbound_put_string(record, "auth_status", status);
    serde_json::json!({
        "required": true,
        "scope": crate::secrets::credential_scope(),
        "name": secret_name,
        "exists": exists,
        "status": status,
        "secret_value_in_payload": false,
    })
}

fn outbound_register_research_scrape_target(
    root: &Path,
    adapter_payload: &Value,
    record: &Value,
    adapter_id: &str,
    source_id: &str,
    target_key: &str,
) -> anyhow::Result<Value> {
    let url = outbound_first_string(&[
        outbound_string(record, &["url"]),
        outbound_string(adapter_payload, &["url"]),
    ]);
    if let Some((target_dir, script_path)) =
        outbound_find_bundled_scrape_target_dir(root, source_id, target_key, url.as_deref())
    {
        let manifest_path = target_dir.join("target.json");
        scrape::handle_scrape_command(
            root,
            &[
                "upsert-target".to_string(),
                "--input".to_string(),
                manifest_path.to_string_lossy().to_string(),
            ],
        )?;
        scrape::handle_scrape_command(
            root,
            &[
                "register-script".to_string(),
                "--target-key".to_string(),
                target_key.to_string(),
                "--script-file".to_string(),
                script_path.to_string_lossy().to_string(),
                "--language".to_string(),
                "javascript".to_string(),
                "--change-reason".to_string(),
                "outbound_adapter_registration".to_string(),
                "--notes".to_string(),
                format!("Registered from Outbound adapter {adapter_id} for {source_id}"),
            ],
        )?;
        return Ok(serde_json::json!({
            "ok": true,
            "target_key": target_key,
            "registered_from": "source_tree",
            "target_manifest": manifest_path,
            "script_file": script_path,
            "script_registered": true,
        }));
    }

    let manifest = adapter_payload
        .get("target_manifest")
        .or_else(|| {
            record
                .get("payload")
                .and_then(|payload| payload.get("target_manifest"))
        })
        .cloned()
        .unwrap_or_else(|| {
            let display_name = outbound_first_string(&[
                outbound_string(record, &["label"]),
                outbound_string(adapter_payload, &["label"]),
                Some(source_id.to_string()),
            ])
            .unwrap_or_else(|| source_id.to_string());
            let start_url = url
                .clone()
                .unwrap_or_else(|| format!("https://{source_id}/"));
            let country_hints = adapter_payload
                .get("countries")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([]));
            serde_json::json!({
                "target_key": target_key,
                "display_name": display_name,
                "start_url": start_url,
                "target_kind": "prospect-research",
                "status": "active",
                "config": {
                    "expected_provider": source_id,
                    "country_hints": country_hints,
                    "record_key_fields": ["field", "source_url"]
                },
                "output_schema": {
                    "schema_key": "prospect.v1",
                    "primary_artifact_kind": "field_records_json",
                    "record_key_fields": ["field", "source_url"]
                }
            })
        });
    let manifest_path = outbound_write_research_scrape_manifest(root, target_key, &manifest)?;
    scrape::handle_scrape_command(
        root,
        &[
            "upsert-target".to_string(),
            "--input".to_string(),
            manifest_path.to_string_lossy().to_string(),
        ],
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "target_key": target_key,
        "registered_from": "adapter_manifest",
        "target_manifest": manifest_path,
        "script_registered": false,
        "next_step": "generate or register scripts/v1.js with universal-scraping",
    }))
}

fn outbound_execute_research_scrape_target(
    root: &Path,
    command: &BusinessCommand,
    adapter_payload: &Value,
    source_id: &str,
    target_key: &str,
) -> anyhow::Result<scrape::ScrapeExecutionOutcome> {
    let company = outbound_first_string(&[
        outbound_string(&command.payload, &["test_input", "company"]),
        outbound_string(&command.payload, &["company"]),
        outbound_string(adapter_payload, &["label"]),
        Some(source_id.to_string()),
    ])
    .unwrap_or_else(|| source_id.to_string());
    let country = outbound_first_string(&[
        outbound_string(&command.payload, &["test_input", "country"]),
        outbound_string(&command.payload, &["country"]),
        adapter_payload
            .get("countries")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
    ])
    .unwrap_or_else(|| "DE".to_string());
    let input = serde_json::json!({
        "company": company,
        "country": country,
        "source_id": source_id,
        "adapter_test": true,
    });
    scrape::execute_scrape_with_outcome(
        root,
        &[
            "execute".to_string(),
            "--target-key".to_string(),
            target_key.to_string(),
            "--trigger-kind".to_string(),
            "manual".to_string(),
            "--timeout-seconds".to_string(),
            "45".to_string(),
            "--allow-heal".to_string(),
            "--input-json".to_string(),
            input.to_string(),
        ],
    )
}

fn outbound_find_bundled_scrape_target_dir(
    root: &Path,
    source_id: &str,
    target_key: &str,
    url: Option<&str>,
) -> Option<(PathBuf, PathBuf)> {
    let mut host_candidates = BTreeSet::new();
    for candidate in [
        Some(source_id.to_string()),
        outbound_host_from_url(url),
        Some(target_key.replace('-', ".")),
        Some(target_key.to_string()),
    ]
    .into_iter()
    .flatten()
    {
        let normalized = candidate
            .trim()
            .trim_start_matches("www.")
            .trim_start_matches("app.")
            .trim_start_matches("api.")
            .to_ascii_lowercase();
        if !normalized.is_empty() {
            host_candidates.insert(normalized);
        }
    }

    for base in outbound_scrape_target_roots(root) {
        let shared_script = base.join("_shared").join("generic-prospect-v1.js");
        for host in &host_candidates {
            let dir = base.join(host);
            if !dir.join("target.json").is_file() {
                continue;
            }
            let specialized_script = dir.join("scripts").join("v1.js");
            if specialized_script.is_file() {
                return Some((dir, specialized_script));
            }
            if shared_script.is_file() {
                return Some((dir, shared_script.clone()));
            }
        }
    }
    None
}

fn outbound_scrape_target_roots(root: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push_root = |candidate: PathBuf| {
        let key = candidate.to_string_lossy().to_string();
        if seen.insert(key) {
            roots.push(candidate);
        }
    };
    for ancestor in root.ancestors() {
        push_root(ancestor.join("src/tools/web-stack/scrape-targets"));
    }
    if let Ok(current_dir) = env::current_dir() {
        for ancestor in current_dir.ancestors() {
            push_root(ancestor.join("src/tools/web-stack/scrape-targets"));
        }
    }
    if let Ok(current_exe) = env::current_exe() {
        for ancestor in current_exe.ancestors() {
            push_root(ancestor.join("src/tools/web-stack/scrape-targets"));
        }
    }
    roots
}

fn outbound_host_from_url(url: Option<&str>) -> Option<String> {
    let raw = url?.trim();
    if raw.is_empty() {
        return None;
    }
    Url::parse(raw)
        .ok()
        .and_then(|parsed| parsed.host_str().map(ToOwned::to_owned))
        .or_else(|| raw.split('/').next().map(ToOwned::to_owned))
}

fn outbound_write_research_scrape_manifest(
    root: &Path,
    target_key: &str,
    manifest: &Value,
) -> anyhow::Result<PathBuf> {
    let dir = root
        .join("runtime")
        .join("scraping")
        .join("outbound-adapters")
        .join(outbound_safe_path_component(target_key));
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create scrape adapter dir {}", dir.display()))?;
    let manifest_path = dir.join("target.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(manifest)?).with_context(|| {
        format!(
            "failed to write scrape target manifest {}",
            manifest_path.display()
        )
    })?;
    Ok(manifest_path)
}

fn outbound_safe_path_component(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    if out.is_empty() {
        "adapter".to_string()
    } else {
        out
    }
}

fn outbound_handle_campaign_mailbox_link(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let campaign_id = outbound_required_string(&command.payload, &["campaign_id"])?;
    let mailbox_address = outbound_required_string(&command.payload, &["mailbox_address"])
        .or_else(|_| {
            outbound_required_string(&command.payload, &["communication_account_address"])
        })?;
    let mailbox_address = mailbox_address.trim().to_string();
    anyhow::ensure!(
        mailbox_address.contains('@'),
        "mailbox_address must be a valid email address"
    );
    let account_key = outbound_first_string(&[
        outbound_string(&command.payload, &["communication_account_key"]),
        outbound_string(&command.payload, &["account_key"]),
        Some(format!("email:{}", mailbox_address.to_ascii_lowercase())),
    ])
    .unwrap_or_else(|| format!("email:{}", mailbox_address.to_ascii_lowercase()));
    let channel =
        outbound_string(&command.payload, &["channel"]).unwrap_or_else(|| "email".to_string());
    let provider = outbound_string(&command.payload, &["provider"])
        .unwrap_or_else(|| "business-os.outbound".to_string());
    let mailbox_status = outbound_string(&command.payload, &["mailbox_status"])
        .unwrap_or_else(|| "ready".to_string());
    let display_name = outbound_string(&command.payload, &["display_name"]).unwrap_or_default();
    let reply_to =
        outbound_string(&command.payload, &["reply_to"]).unwrap_or_else(|| mailbox_address.clone());

    let profile = serde_json::json!({
        "address": mailbox_address,
        "campaign_id": campaign_id,
        "outbound_campaign_id": campaign_id,
        "display_name": display_name,
        "reply_to": reply_to,
        "mailbox_status": mailbox_status,
        "source": "business-os.outbound.campaign.mailbox.link",
    });

    let mut channel_conn = channels::open_channel_db(&crate::paths::core_db(root))?;
    channels::upsert_communication_account(
        &mut channel_conn,
        &account_key,
        &channel,
        &mailbox_address,
        &provider,
        profile.clone(),
    )?;

    let mut campaign = outbound_load_record(conn, "outbound_campaigns", &campaign_id)?
        .unwrap_or_else(|| serde_json::json!({ "id": campaign_id.clone() }));
    outbound_put_string(&mut campaign, "id", campaign_id.clone());
    outbound_put_string(
        &mut campaign,
        "communication_account_key",
        account_key.clone(),
    );
    outbound_put_string(
        &mut campaign,
        "communication_account_address",
        mailbox_address.clone(),
    );
    outbound_put_string(&mut campaign, "mailbox_status", mailbox_status.clone());
    outbound_put_default_object(&mut campaign, "payload");
    outbound_payload_insert(
        &mut campaign,
        "communication_account_key",
        Value::String(account_key.clone()),
    );
    outbound_payload_insert(
        &mut campaign,
        "communication_account_address",
        Value::String(mailbox_address.clone()),
    );
    outbound_payload_insert(
        &mut campaign,
        "mailbox_status",
        Value::String(mailbox_status.clone()),
    );
    outbound_put_default_i64(&mut campaign, "created_at_ms", now);
    outbound_put_i64(&mut campaign, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_campaigns",
        &campaign_id,
        now,
        campaign.clone(),
    )?;

    let mut limit = outbound_load_record(conn, "outbound_account_limits", &account_key)?
        .unwrap_or_else(|| {
            serde_json::json!({
                "id": account_key,
                "sender_account_id": account_key,
                "daily_sent_count": 0,
                "daily_limit": 0,
                "status": "active",
                "blocked": false,
            })
        });
    outbound_put_string(&mut limit, "id", account_key.clone());
    outbound_put_string(&mut limit, "sender_account_id", account_key.clone());
    outbound_put_string(&mut limit, "campaign_id", campaign_id.clone());
    outbound_put_default_i64(&mut limit, "daily_sent_count", 0);
    outbound_put_default_i64(&mut limit, "daily_limit", 0);
    outbound_put_default_string(&mut limit, "status", "active");
    if !limit
        .get("blocked")
        .map(|value| value.is_boolean())
        .unwrap_or(false)
    {
        if let Some(object) = limit.as_object_mut() {
            object.insert("blocked".to_string(), Value::Bool(false));
        }
    }
    outbound_put_default_i64(&mut limit, "created_at_ms", now);
    outbound_put_i64(&mut limit, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_account_limits",
        &account_key,
        now,
        limit.clone(),
    )?;

    Ok(serde_json::json!({
        "ok": true,
        "campaign": campaign,
        "communication_account_key": account_key,
        "communication_account_address": mailbox_address,
        "mailbox_status": mailbox_status,
        "account_limit": limit,
    }))
}

fn outbound_handle_campaign_status_set(
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let campaign_id = outbound_required_string(&command.payload, &["campaign_id"])?;
    let requested_status = outbound_required_string(&command.payload, &["status"])?;
    let allowed = matches!(
        requested_status.as_str(),
        "setup_required" | "active" | "paused" | "closed"
    );
    anyhow::ensure!(
        allowed,
        "unsupported campaign status: {requested_status}; allowed: setup_required, active, paused, closed"
    );
    let mut campaign = outbound_load_required(
        conn,
        "outbound_campaigns",
        &campaign_id,
        "campaign not found",
    )?;
    let default_channel = outbound_first_string(&[
        outbound_string(&command.payload, &["channel"]),
        outbound_string(
            &campaign,
            &["payload", "active_outreach", "default_channel"],
        ),
        outbound_string(&campaign, &["channel"]),
        Some("email".to_string()),
    ])
    .unwrap_or_else(|| "email".to_string());

    if requested_status == "active" {
        match default_channel.as_str() {
            "physical_letter" => {
                // manually-handled channel; no mailbox required.
            }
            _ => {
                let account_key = outbound_first_string(&[
                    outbound_string(&campaign, &["communication_account_key"]),
                    outbound_string(&campaign, &["payload", "communication_account_key"]),
                ])
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "campaign cannot activate email channel without a linked mailbox"
                    )
                })?;
                let limit = outbound_load_record(conn, "outbound_account_limits", &account_key)?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "campaign cannot activate email channel without outbound_account_limits"
                        )
                    })?;
                let limit_status =
                    outbound_string(&limit, &["status"]).unwrap_or_else(|| "active".to_string());
                anyhow::ensure!(
                    !matches!(
                        limit_status.as_str(),
                        "blocked" | "locked" | "suspended" | "disabled"
                    ),
                    "campaign mailbox is not ready (status: {limit_status})"
                );
                let blocked = limit
                    .get("blocked")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                anyhow::ensure!(
                    !blocked,
                    "campaign mailbox is blocked for outbound communication"
                );
            }
        }
    }

    outbound_put_string(&mut campaign, "status", requested_status.clone());
    outbound_put_default_object(&mut campaign, "payload");
    outbound_payload_insert(
        &mut campaign,
        "status_set_at_ms",
        Value::Number(serde_json::Number::from(now)),
    );
    outbound_payload_insert(
        &mut campaign,
        "active_channel",
        Value::String(default_channel.clone()),
    );
    outbound_put_i64(&mut campaign, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_campaigns",
        &campaign_id,
        now,
        campaign.clone(),
    )?;

    Ok(serde_json::json!({
        "ok": true,
        "campaign": campaign,
        "status": requested_status,
        "channel": default_channel,
    }))
}

/// Persist (or overwrite) an outbound skillbook record. Returned `version_number`
/// is monotonically incremented per record so operators can audit edits.
/// Publish the app's maintained research procedure and settings into the
/// scraping area of the CTOX SQLite store, as the scrape target
/// `<app>-policy` (`target_kind = "app-policy"`, the policy in `config`).
///
/// The research worker discovers it with `ctox scrape list-targets` /
/// `ctox scrape show-target --target-key <app>-policy`. That keeps a long
/// procedure out of the chat prompt and gives every later turn (continuation
/// after a login, a repair task) the same binding instructions even when it
/// never saw the original command payload.
fn outbound_handle_research_policy_publish(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let app = outbound_first_string(&[
        outbound_string(&command.payload, &["app"]),
        outbound_string(&command.payload, &["module"]),
        Some(command.module.clone()).filter(|value| !value.trim().is_empty()),
    ])
    .unwrap_or_else(|| "outbound-lead-generation".to_string());
    let app_slug = app
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    anyhow::ensure!(!app_slug.is_empty(), "app id is required");
    let target_key = format!("{app_slug}-policy");
    let mut policy = outbound_object_payload(&command.payload);
    if let Some(object) = policy.as_object_mut() {
        object.insert("app".to_string(), Value::String(app.clone()));
        object.insert("published_at_ms".to_string(), Value::from(now));
        object.insert(
            "policy_contract".to_string(),
            Value::String("ctox.outbound.research_policy.v1".to_string()),
        );
    }
    let manifest = serde_json::json!({
        "target_key": target_key,
        "display_name": format!("{app} · Rechercheablauf und Einstellungen"),
        // The policy is data, not a scraped site; the start URL is only the
        // manifest's required anchor and is never fetched for a policy target.
        "start_url": "https://ctox.local/app-policy",
        "target_kind": "app-policy",
        "status": "active",
        "config": policy,
        "output_schema": { "type": "object" },
    });
    let manifest_path = crate::paths::runtime_dir(root)
        .join("scraping")
        .join("app-policy")
        .join(format!("{target_key}.json"));
    if let Some(parent) = manifest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    scrape::handle_scrape_command(
        root,
        &[
            "upsert-target".to_string(),
            "--input".to_string(),
            manifest_path.to_string_lossy().to_string(),
        ],
    )?;
    let record_id = format!("policy:{app_slug}");
    let mut record = manifest["config"].clone();
    outbound_put_string(&mut record, "id", record_id.clone());
    upsert_business_record(conn, "outbound_research_policies", &record_id, now, record)?;
    Ok(serde_json::json!({
        "ok": true,
        "app": app,
        "target_key": target_key,
        "manifest_path": manifest_path.to_string_lossy(),
        "record_id": record_id,
    }))
}

fn outbound_handle_skillbook_save(
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let skillbook_id = outbound_required_string(&command.payload, &["skillbook_id", "id"])
        .or_else(|_| {
            command
                .record_id
                .as_deref()
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("skillbook_id is required"))
        })?;
    let prior = outbound_load_record(conn, "outbound_skillbooks", &skillbook_id)?;
    let prior_version = prior
        .as_ref()
        .and_then(|v| v.get("version_number").and_then(Value::as_i64))
        .unwrap_or(0);
    let mut record = prior.clone().unwrap_or_else(|| serde_json::json!({}));
    let incoming = outbound_object_payload(&command.payload);
    if let (Some(target), Some(source)) = (record.as_object_mut(), incoming.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    outbound_put_string(&mut record, "id", skillbook_id.clone());
    outbound_put_string(&mut record, "skillbook_id", skillbook_id.clone());
    outbound_put_default_string(&mut record, "title", "");
    outbound_put_default_string(&mut record, "mission", "");
    for key in [
        "non_negotiable_rules",
        "workflow_backbone",
        "routing_taxonomy",
        "stop_rules",
    ] {
        if !record.get(key).map(Value::is_array).unwrap_or(false) {
            if let Some(obj) = record.as_object_mut() {
                obj.insert(key.to_string(), Value::Array(Vec::new()));
            }
        }
    }
    outbound_put_default_i64(&mut record, "created_at_ms", now);
    outbound_put_i64(&mut record, "updated_at_ms", now);
    outbound_put_i64(&mut record, "version_number", prior_version + 1);
    upsert_business_record(
        conn,
        "outbound_skillbooks",
        &skillbook_id,
        now,
        record.clone(),
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "skillbook": record,
        "version_number": prior_version + 1,
    }))
}

/// Insert the three default outbound skillbook records (message_drafting,
/// reply_handling, scheduling) if they do not yet exist. Idempotent: returns
/// the list of newly seeded ids, empty if everything is already present.
fn outbound_handle_skillbook_seed_defaults(conn: &Connection, now: i64) -> anyhow::Result<Value> {
    let defaults: [Value; 3] = [
        outbound_default_message_drafting_skillbook(),
        outbound_default_reply_handling_skillbook(),
        outbound_default_scheduling_skillbook(),
    ];
    let mut seeded = Vec::new();
    for mut record in defaults {
        let id = record
            .get("skillbook_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .context("default skillbook is missing skillbook_id")?;
        if outbound_load_record(conn, "outbound_skillbooks", &id)?.is_some() {
            continue;
        }
        outbound_put_i64(&mut record, "created_at_ms", now);
        outbound_put_i64(&mut record, "updated_at_ms", now);
        upsert_business_record(conn, "outbound_skillbooks", &id, now, record)?;
        seeded.push(id);
    }
    Ok(serde_json::json!({ "ok": true, "seeded": seeded }))
}

/// Canonical message-drafting skillbook. Drives both the agent-backed
/// `outbound.pipeline.outreach_draft` loop (via the
/// `business-os-outbound-message-drafting` skill) and the deterministic
/// `outbound.draft.prepare` fallback. The whole loop stays on the RxDB command
/// bus and never sends without an explicit approval.
fn outbound_default_message_drafting_skillbook() -> Value {
    serde_json::json!({
        "id": "business-os.outbound.message_drafting.v1",
        "skillbook_id": "business-os.outbound.message_drafting.v1",
        "title": "Initial- und Follow-up-Drafts vorbereiten",
        "mission": "Personalisierte Erst- und Follow-up-Anschreiben fuer eine Outbound-Campaign entwerfen, die an die ICP-Beschreibung, das konkrete Unternehmen und die Zielperson anknuepfen und ausschliesslich als freigabepflichtige Drafts entstehen.",
        "non_negotiable_rules": [
            "Keine Nachricht ohne explizite Freigabe versenden",
            "Keinen externen Dienst und keine HTTP-Schnittstelle aufrufen; ausschliesslich ueber den CTOX-Command-Bus arbeiten",
            "Suppression-, Bounce- und Opt-out-Listen jederzeit respektieren",
            "Keine Fakten erfinden; nur belegte Rechercheergebnisse und vom Operator gepflegte Angaben verwenden",
            "Sprache und Anrede an die Zielperson anpassen (Standard: Deutsch, Sie-Form)",
        ],
        "workflow_backbone": [
            {
                "step": "context_intake",
                "title": "Kontext aufnehmen",
                "detail": "drafting_request lesen: ICP-/Produktbeschreibung, CTA, Signatur, Landingpage-Checkliste, Prompt-Vorlagen sowie Unternehmens- und Personendaten.",
            },
            {
                "step": "anchor_selection",
                "title": "Anknuepfungspunkt waehlen",
                "detail": "Aus Homepage-Summary und Personendaten den staerksten, belegbaren Anknuepfungspunkt zwischen Angebot und Empfaenger ableiten.",
            },
            {
                "step": "initial_draft",
                "title": "Erstanschreiben entwerfen",
                "detail": "Betreff plus knappen Body mit einem klaren CTA schreiben; Signatur anhaengen; keine erfundenen Aussagen.",
            },
            {
                "step": "followups",
                "title": "Zwei Follow-ups entwerfen",
                "detail": "Zwei kurze, eskalationsarme Follow-ups verfassen, die ohne Antwort hoeflich nachfassen und sich auf das Erstanschreiben beziehen.",
            },
            {
                "step": "writeback",
                "title": "Ergebnis zurueckschreiben",
                "detail": "message_mail_subject, message_mail_body, message_followup_1 und message_followup_2 ausschliesslich ueber den Command outbound.pipeline.write_outreach_draft persistieren.",
            },
        ],
        "routing_taxonomy": [
            { "intent": "initial", "route": "Erstanschreiben entwerfen", "stop": "Draft als awaiting_approval ablegen" },
            { "intent": "followup", "route": "Naechstes Follow-up aus der Sequenz entwerfen", "stop": "stop on reply" },
            { "intent": "reply_received", "route": "An reply_handling.v1 uebergeben", "stop": "Keine weitere Sequenznachricht senden" },
        ],
        "stop_rules": [
            "stop on reply",
            "stop on bounce",
            "stop on opt-out",
            "stop on suppression match",
        ],
        "version_number": 1,
    })
}

/// Canonical reply-handling skillbook: classify inbound replies and stage a
/// reply draft instead of letting an automated sequence keep firing.
fn outbound_default_reply_handling_skillbook() -> Value {
    serde_json::json!({
        "id": "business-os.outbound.reply_handling.v1",
        "skillbook_id": "business-os.outbound.reply_handling.v1",
        "title": "Antworten klassifizieren und Reply-Drafts vorbereiten",
        "mission": "Eingehende Antworten klassifizieren, die laufende Sequenz anhalten und einen passenden, freigabepflichtigen Reply-Draft vorbereiten.",
        "non_negotiable_rules": [
            "Bei jeder Antwort die automatische Sequenz anhalten, bevor weiter entworfen wird",
            "Opt-out und Unsubscribe sofort in die Suppression-Liste ueberfuehren",
            "Keine Antwort ohne explizite Freigabe versenden",
            "Keinen externen Dienst aufrufen; nur ueber den CTOX-Command-Bus arbeiten",
        ],
        "workflow_backbone": [
            {
                "step": "classify",
                "title": "Antwort klassifizieren",
                "detail": "Antwort in positive_reply, question, objection, not_interested, out_of_office oder unsubscribe einsortieren.",
            },
            {
                "step": "halt_sequence",
                "title": "Sequenz anhalten",
                "detail": "Bei jeder echten Antwort die laufende Follow-up-Sequenz pausieren, damit keine widerspruechliche Nachricht hinausgeht.",
            },
            {
                "step": "route",
                "title": "Folgeaktion routen",
                "detail": "Auf Basis der Klassifikation Reply-Draft, Terminfindung, Suppression oder manuelle Eskalation auswaehlen.",
            },
            {
                "step": "draft_reply",
                "title": "Reply-Draft vorbereiten",
                "detail": "Knappe, kontextbezogene Antwort als freigabepflichtigen Draft ablegen.",
            },
        ],
        "routing_taxonomy": [
            { "intent": "positive_reply", "route": "Reply-Draft oder Terminfindung vorbereiten", "stop": "Sequenz beendet" },
            { "intent": "question", "route": "Antwort mit Klaerung entwerfen", "stop": "Sequenz pausiert" },
            { "intent": "objection", "route": "Einwand-Antwort entwerfen", "stop": "Sequenz pausiert" },
            { "intent": "not_interested", "route": "Hoeflich schliessen", "stop": "Engagement schliessen" },
            { "intent": "out_of_office", "route": "Follow-up nach OOO-Datum neu planen", "stop": "stop until ooo_until" },
            { "intent": "unsubscribe", "route": "In Suppression-Liste ueberfuehren", "stop": "stop on opt-out" },
        ],
        "stop_rules": [
            "stop on opt-out",
            "stop on unsubscribe",
            "stop sequence on any human reply",
        ],
        "version_number": 1,
    })
}

/// Canonical scheduling skillbook: propose meeting slots, check them against
/// the calendar, and book only after an explicit approval.
fn outbound_default_scheduling_skillbook() -> Value {
    serde_json::json!({
        "id": "business-os.outbound.scheduling.v1",
        "skillbook_id": "business-os.outbound.scheduling.v1",
        "title": "Terminfindung vorbereiten",
        "mission": "Auf Terminwunsch passende Slots vorschlagen, gegen Kalenderkonflikte und Arbeitszeiten pruefen und das Meeting erst nach Freigabe buchen.",
        "non_negotiable_rules": [
            "Slots nur innerhalb der konfigurierten Arbeitszeiten und Limits vorschlagen",
            "Kein Meeting ohne explizite Freigabe buchen",
            "Bei Kalenderkonflikt einen Alternativslot anbieten statt zu ueberbuchen",
        ],
        "workflow_backbone": [
            {
                "step": "collect_constraints",
                "title": "Rahmen sammeln",
                "detail": "Dauer, Zeitzone, bevorzugte Fenster und Arbeitszeiten der Campaign ermitteln.",
            },
            {
                "step": "propose_slots",
                "title": "Slots vorschlagen",
                "detail": "Zwei bis drei konkrete Slots vorschlagen, die innerhalb der Limits liegen.",
            },
            {
                "step": "check_conflicts",
                "title": "Konflikte pruefen",
                "detail": "Vorgeschlagene Slots gegen Kalender und bestehende Buchungen pruefen und Konflikte aussortieren.",
            },
            {
                "step": "book_on_approval",
                "title": "Nach Freigabe buchen",
                "detail": "Erst nach Operator-Freigabe per outbound.scheduling.mark_booked buchen.",
            },
        ],
        "routing_taxonomy": [
            { "intent": "slot_request", "route": "Slots vorschlagen", "stop": "Auf Empfaengerwahl warten" },
            { "intent": "slot_confirmed", "route": "Buchung nach Freigabe vorbereiten", "stop": "stop until approved" },
            { "intent": "conflict", "route": "Alternativslot vorschlagen", "stop": "Nicht ueberbuchen" },
        ],
        "stop_rules": [
            "stop on reply",
            "stop until approved before booking",
            "stop on calendar conflict",
        ],
        "version_number": 1,
    })
}

/// Persist a per-campaign letter template (salutation, body, closing).
fn outbound_handle_letter_template_save(
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let template_id =
        outbound_required_string(&command.payload, &["template_id", "id"]).or_else(|_| {
            command
                .record_id
                .as_deref()
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("template_id is required"))
        })?;
    let prior = outbound_load_record(conn, "outbound_letter_templates", &template_id)?;
    let prior_version = prior
        .as_ref()
        .and_then(|v| v.get("version_number").and_then(Value::as_i64))
        .unwrap_or(0);
    let mut record = outbound_object_payload(&command.payload);
    outbound_put_string(&mut record, "id", template_id.clone());
    outbound_put_string(&mut record, "template_id", template_id.clone());
    outbound_put_default_string(&mut record, "title", "");
    outbound_put_default_string(&mut record, "salutation", "");
    outbound_put_default_string(&mut record, "body_template", "");
    outbound_put_default_string(&mut record, "closing", "");
    outbound_put_default_i64(&mut record, "created_at_ms", now);
    outbound_put_i64(&mut record, "updated_at_ms", now);
    outbound_put_i64(&mut record, "version_number", prior_version + 1);
    upsert_business_record(
        conn,
        "outbound_letter_templates",
        &template_id,
        now,
        record.clone(),
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "template": record,
        "version_number": prior_version + 1,
    }))
}

/// Audit export: dump every outbound record linked to a campaign (or all
/// campaigns if campaign_id is empty) so operators can produce GDPR / SLA proof.
fn outbound_handle_audit_export(
    conn: &Connection,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let campaign_filter = outbound_string(&command.payload, &["campaign_id"]).unwrap_or_default();
    let collections = [
        "outbound_campaigns",
        "outbound_engagements",
        "outbound_messages",
        "outbound_approvals",
        "outbound_sequences",
        "outbound_sender_assignments",
        "outbound_meeting_requests",
        "outbound_suppression_entries",
        "outbound_account_limits",
        "outbound_skillbooks",
        "outbound_letter_templates",
    ];
    let mut export = serde_json::Map::new();
    for collection in collections {
        let mut stmt = conn.prepare(
            "SELECT payload_json FROM business_records
             WHERE collection = ?1 AND deleted = 0",
        )?;
        let rows = stmt.query_map(params![collection], |row| row.get::<_, String>(0))?;
        let mut records: Vec<Value> = Vec::new();
        for row in rows {
            let raw = row?;
            let value: Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let matches_filter = if campaign_filter.is_empty() {
                true
            } else {
                let id = outbound_string(&value, &["campaign_id"]).unwrap_or_default();
                let payload_id =
                    outbound_string(&value, &["payload", "campaign_id"]).unwrap_or_default();
                id == campaign_filter
                    || payload_id == campaign_filter
                    || outbound_string(&value, &["id"]).as_deref() == Some(campaign_filter.as_str())
            };
            if matches_filter {
                records.push(value);
            }
        }
        export.insert(collection.to_string(), Value::Array(records));
    }
    Ok(serde_json::json!({
        "ok": true,
        "campaign_id": campaign_filter,
        "export": export,
        "exported_at_ms": now_ms() as i64,
    }))
}

/// Scheduler tick: walks every active engagement, prepares overdue follow-up
/// drafts, and reconciles any pending SMTP delivery outcomes. Honors
/// `payload.dry_run == true` by reporting what would have happened without
/// touching state. Always pulls the reconciler so delivered emails get
/// promoted to `send_status = sent` automatically.
fn outbound_handle_scheduler_tick(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let dry_run = command
        .payload
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut actions: Vec<Value> = Vec::new();

    // 1. Reconcile SMTP delivery log → outbound_messages.send_status.
    let reconcile = if dry_run {
        serde_json::json!({ "ok": true, "checked": 0, "updated": [], "dry_run": true })
    } else {
        outbound_handle_provider_reconcile(root, conn, command, now)?
    };
    if let Some(updated) = reconcile
        .get("updated")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
    {
        actions.push(serde_json::json!({
            "kind": "provider_reconcile",
            "count": updated.len(),
        }));
    }

    // 2. Prepare overdue follow-up drafts: engagements with next_action_at_ms <= now
    //    and status in waiting_for_reply / scheduled_to_send / draft_prepared.
    let mut stmt = conn.prepare(
        "SELECT payload_json FROM business_records
         WHERE collection = 'outbound_engagements' AND deleted = 0",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut engagements = Vec::new();
    for row in rows {
        let Ok(raw) = row else { continue };
        if let Ok(engagement) = serde_json::from_str::<Value>(&raw) {
            engagements.push(engagement);
        }
    }
    drop(stmt);

    // Campaign-level pause: an engagement whose campaign is paused/closed/not yet
    // active must not get an automated follow-up. Cache campaign statuses once so
    // a tick over many engagements stays a single sweep.
    let mut campaign_status: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT payload_json FROM business_records
             WHERE collection = 'outbound_campaigns' AND deleted = 0",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows.flatten() {
            if let Ok(campaign) = serde_json::from_str::<Value>(&row) {
                if let Some(id) = outbound_string(&campaign, &["id"]) {
                    let status =
                        outbound_string(&campaign, &["status"]).unwrap_or_else(|| "active".into());
                    campaign_status.insert(id, status);
                }
            }
        }
    }

    for engagement in engagements {
        let status = outbound_string(&engagement, &["status"]).unwrap_or_else(|| "".to_string());
        // Out-of-office is the documented exception to the reply halt: the
        // engagement stays `reply_received` for the UI but the scheduler resumes
        // the follow-up after its OOO hold (the produced draft is approval-gated).
        let is_out_of_office_wait = status == "reply_received"
            && outbound_string(&engagement, &["payload", "reply_classification"]).as_deref()
                == Some("out_of_office");
        if !is_out_of_office_wait
            && matches!(
                status.as_str(),
                "closed"
                    | "cancelled"
                    | "meeting_booked"
                    | "paused"
                    | "reply_received"
                    | "bounced"
                    | "unsubscribed"
                    | "suppressed"
            )
        {
            continue;
        }
        let next_action = engagement
            .get("next_action_at_ms")
            .and_then(Value::as_i64)
            .or_else(|| {
                engagement
                    .pointer("/payload/next_action_at_ms")
                    .and_then(Value::as_i64)
            });
        let due = match next_action {
            Some(ts) => ts <= now,
            None => false,
        };
        if !due {
            continue;
        }
        let Some(engagement_id) = outbound_string(&engagement, &["id"]) else {
            continue;
        };
        if let Some(campaign_id) = outbound_string(&engagement, &["campaign_id"]) {
            if let Some(camp_status) = campaign_status.get(&campaign_id) {
                if matches!(camp_status.as_str(), "paused" | "closed" | "setup_required") {
                    actions.push(serde_json::json!({
                        "kind": "followup_skipped_campaign_paused",
                        "engagement_id": engagement_id,
                        "campaign_id": campaign_id,
                        "campaign_status": camp_status,
                    }));
                    continue;
                }
            }
        }
        if dry_run {
            actions.push(serde_json::json!({
                "kind": "followup_due",
                "engagement_id": engagement_id,
                "due_at_ms": next_action,
            }));
            continue;
        }
        match outbound_prepare_due_followup_draft(conn, engagement, command, now) {
            Ok(action) => actions.push(action),
            Err(error) => actions.push(serde_json::json!({
                "kind": "followup_prepare_failed",
                "engagement_id": engagement_id,
                "error": error.to_string(),
            })),
        }
    }

    Ok(serde_json::json!({
        "ok": true,
        "now_ms": now,
        "actions": actions,
        "dry_run": dry_run,
        "reconciled": reconcile.get("updated").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
    }))
}

fn outbound_prepare_due_followup_draft(
    conn: &Connection,
    mut engagement: Value,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let engagement_id = outbound_required_string(&engagement, &["id"])?;
    let campaign_id = outbound_required_string(&engagement, &["campaign_id"])?;
    let draft_kind = outbound_first_string(&[
        outbound_string(&command.payload, &["draft_kind"]),
        outbound_string(&engagement, &["payload", "next_draft_kind"]),
    ])
    .unwrap_or_else(|| "followup".to_string());
    let channel = outbound_first_string(&[
        outbound_string(&command.payload, &["channel"]),
        outbound_string(&engagement, &["payload", "channel"]),
        outbound_string(&engagement, &["channel"]),
    ])
    .unwrap_or_else(|| "email".to_string());

    let previous_messages = outbound_load_records_by_string_field(
        conn,
        "outbound_messages",
        "engagement_id",
        &engagement_id,
    )?;
    if previous_messages.iter().any(|message| {
        let approval = outbound_string(message, &["approval_status"]).unwrap_or_default();
        let send = outbound_string(message, &["send_status"]).unwrap_or_default();
        let message_type = outbound_string(message, &["message_type"]).unwrap_or_default();
        approval == "awaiting_approval"
            && !matches!(send.as_str(), "cancelled" | "blocked")
            && (message_type == draft_kind || message_type.starts_with("followup"))
    }) {
        outbound_put_i64(&mut engagement, "next_action_at_ms", 0);
        outbound_payload_insert(
            &mut engagement,
            "scheduler_last_skip_reason",
            Value::String("draft_already_awaiting_approval".to_string()),
        );
        outbound_put_i64(&mut engagement, "updated_at_ms", now);
        upsert_business_record(
            conn,
            "outbound_engagements",
            &engagement_id,
            now,
            engagement,
        )?;
        return Ok(serde_json::json!({
            "kind": "followup_skipped_existing_draft",
            "engagement_id": engagement_id,
        }));
    }

    let (sender_account_id, recipient_email, recipient_address_text) =
        if channel == "physical_letter" {
            let address = outbound_first_string(&[
                outbound_string(&command.payload, &["recipient_address_text"]),
                outbound_string(&engagement, &["payload", "contact_address_text"]),
                outbound_string(&engagement, &["payload", "recipient_address_text"]),
            ])
            .context("recipient_address_text is required for physical_letter scheduler drafts")?;
            let sender = outbound_first_string(&[
                outbound_string(&command.payload, &["sender_account_id"]),
                outbound_string(&engagement, &["sender_account_id"]),
            ])
            .unwrap_or_default();
            let email = outbound_first_string(&[
                outbound_string(&command.payload, &["recipient_email"]),
                outbound_string(&engagement, &["payload", "contact_email"]),
            ])
            .unwrap_or_default();
            (sender, email, address)
        } else {
            let sender = outbound_first_string(&[
                outbound_string(&command.payload, &["sender_account_id"]),
                outbound_string(&engagement, &["sender_account_id"]),
            ])
            .context("sender_account_id is required for scheduler draft")?;
            let email = outbound_first_string(&[
                outbound_string(&command.payload, &["recipient_email"]),
                outbound_string(&engagement, &["payload", "contact_email"]),
            ])
            .context("recipient_email is required for scheduler draft")?;
            anyhow::ensure!(
                !outbound_recipient_suppressed(conn, &email)?,
                "recipient is suppressed for outbound communication"
            );
            (sender, email, String::new())
        };

    // Respect the sender account's health and daily cap: do not pile up scheduler
    // drafts that would only bounce off the send gate. When the account is blocked
    // or its daily cap is already exhausted, defer the follow-up — leave the due
    // marker in place so a later tick (after a daily reset) retries — and record a
    // skip reason instead of generating an unsendable draft.
    if channel != "physical_letter" && !sender_account_id.is_empty() {
        if let Err(limit_err) = outbound_enforce_account_limit(conn, &sender_account_id) {
            let detail = limit_err.to_string();
            outbound_payload_insert(
                &mut engagement,
                "scheduler_last_skip_reason",
                Value::String("account_limit".to_string()),
            );
            outbound_payload_insert(
                &mut engagement,
                "scheduler_last_skip_detail",
                Value::String(detail.clone()),
            );
            outbound_payload_insert(
                &mut engagement,
                "scheduler_last_skip_at_ms",
                Value::Number(serde_json::Number::from(now)),
            );
            outbound_put_i64(&mut engagement, "updated_at_ms", now);
            upsert_business_record(
                conn,
                "outbound_engagements",
                &engagement_id,
                now,
                engagement,
            )?;
            return Ok(serde_json::json!({
                "kind": "followup_skipped_account_limit",
                "engagement_id": engagement_id,
                "sender_account_id": sender_account_id,
                "reason": detail,
            }));
        }
    }

    let latest_message = outbound_latest_message(&previous_messages);
    let scheduler_skillbook_id = outbound_first_string(&[
        outbound_string(&command.payload, &["skillbook_id"]),
        outbound_string(&engagement, &["payload", "skillbook_id"]),
    ])
    .unwrap_or_else(|| "business-os.outbound.message_drafting.v1".to_string());
    let scheduler_skillbook_guidance = outbound_skillbook_guidance(conn, &scheduler_skillbook_id)?;
    let generated = outbound_generate_automated_draft(
        &engagement,
        latest_message.as_ref(),
        &command.payload,
        &draft_kind,
        scheduler_skillbook_guidance.as_deref(),
    );
    let subject = outbound_first_string(&[
        outbound_string(&command.payload, &["subject"]),
        outbound_string(&generated, &["subject"]),
    ])
    .context("generated subject is required")?;
    let body_text = outbound_first_string(&[
        outbound_string(&command.payload, &["body_text"]),
        outbound_string(&generated, &["body_text"]),
    ])
    .context("generated body_text is required")?;
    let message_id = format!("msg_{}", Uuid::new_v4().simple());
    let mut message = outbound_object_payload(&command.payload);
    outbound_put_string(&mut message, "id", message_id.clone());
    outbound_put_string(&mut message, "engagement_id", engagement_id.clone());
    outbound_put_string(&mut message, "campaign_id", campaign_id);
    outbound_put_string(&mut message, "message_type", draft_kind.clone());
    outbound_put_string(&mut message, "channel", channel);
    outbound_put_string(&mut message, "direction", "outbound");
    outbound_put_string(&mut message, "sender_account_id", sender_account_id);
    outbound_put_string(&mut message, "recipient_email", recipient_email);
    if !recipient_address_text.is_empty() {
        outbound_put_string(
            &mut message,
            "recipient_address_text",
            recipient_address_text,
        );
    }
    outbound_put_string(&mut message, "subject", subject);
    outbound_put_string(&mut message, "body_text", body_text);
    outbound_put_string(&mut message, "draft_status", "ready_for_review");
    outbound_put_string(&mut message, "approval_status", "awaiting_approval");
    outbound_put_string(&mut message, "send_status", "awaiting_approval");
    outbound_put_default_object(&mut message, "payload");
    outbound_payload_insert(
        &mut message,
        "draft_engine",
        Value::String("business-os.outbound.scheduler.v1".to_string()),
    );
    outbound_payload_insert(&mut message, "generated_draft", generated);
    // Stamp the sequence revision the draft was produced from so each scheduler
    // draft is auditable back to its sequence version.
    let (sequence_id, sequence_version) = outbound_engagement_sequence_context(&engagement);
    if let Some(seq) = sequence_id {
        outbound_payload_insert(&mut message, "sequence_id", Value::String(seq));
    }
    outbound_payload_insert(
        &mut message,
        "sequence_version",
        Value::Number(serde_json::Number::from(sequence_version)),
    );
    if let Some(previous) = latest_message
        .as_ref()
        .and_then(|message| outbound_string(message, &["id"]))
    {
        outbound_put_string(&mut message, "reply_to_message_id", previous);
    }
    let revision_id = outbound_message_revision(&message);
    outbound_put_string(&mut message, "revision_id", revision_id);
    outbound_put_default_i64(&mut message, "created_at_ms", now);
    outbound_put_i64(&mut message, "updated_at_ms", now);
    upsert_business_record(conn, "outbound_messages", &message_id, now, message.clone())?;

    outbound_put_string(&mut engagement, "status", "awaiting_approval");
    outbound_put_i64(&mut engagement, "next_action_at_ms", 0);
    outbound_payload_insert(
        &mut engagement,
        "scheduler_last_message_id",
        Value::String(message_id.clone()),
    );
    outbound_payload_insert(
        &mut engagement,
        "scheduler_last_run_at_ms",
        Value::Number(serde_json::Number::from(now)),
    );
    outbound_put_i64(&mut engagement, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_engagements",
        &engagement_id,
        now,
        engagement,
    )?;

    Ok(serde_json::json!({
        "kind": "followup_draft_prepared",
        "engagement_id": engagement_id,
        "message_id": message_id,
        "draft_kind": draft_kind,
    }))
}

/// Developer-only helper: seed N approval-gated demo engagements and drafts so
/// operators can verify the UI shell against realistic data. Idempotent on the
/// given (campaign_id, count) tuple; existing records are preserved.
fn outbound_handle_dev_seed_test_data(
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let campaign_id = outbound_required_string(&command.payload, &["campaign_id"])?;
    let count = command
        .payload
        .get("count")
        .and_then(Value::as_i64)
        .unwrap_or(3)
        .clamp(1, 25);
    let mut created = Vec::new();
    for idx in 0..count {
        let eng_id = format!("dev_eng_{campaign_id}_{idx}");
        if outbound_load_record(conn, "outbound_engagements", &eng_id)?.is_some() {
            continue;
        }
        let engagement = serde_json::json!({
            "id": eng_id,
            "campaign_id": campaign_id,
            "company_id": format!("dev_co_{idx}"),
            "contact_id": format!("dev_ct_{idx}"),
            "status": "ready_for_assignment",
            "payload": {
                "contact_email": format!("lead{idx}@example.com"),
                "source": "outbound.dev.seed_test_data",
            },
            "created_at_ms": now,
            "updated_at_ms": now,
        });
        upsert_business_record(conn, "outbound_engagements", &eng_id, now, engagement)?;
        created.push(eng_id);
    }
    Ok(serde_json::json!({
        "ok": true,
        "campaign_id": campaign_id,
        "count": created.len(),
        "engagement_ids": created,
    }))
}

/// Re-apply the campaign sequence to an existing engagement: re-projects the
/// stored sequence policy into the engagement payload so newly prepared drafts
/// pick up the latest settings. Requires the engagement to reference a
/// known sequence_id (either inline or via campaign default).
fn outbound_handle_engagement_reapply_sequence(
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let engagement_id = outbound_required_string(&command.payload, &["engagement_id"])?;
    let mut engagement = outbound_load_required(
        conn,
        "outbound_engagements",
        &engagement_id,
        "engagement not found",
    )?;
    let sequence_id = outbound_first_string(&[
        outbound_string(&command.payload, &["sequence_id"]),
        outbound_string(&engagement, &["sequence_id"]),
        outbound_string(&engagement, &["payload", "sequence_id"]),
    ])
    .ok_or_else(|| anyhow::anyhow!("sequence_id is required to reapply"))?;
    let sequence = outbound_load_required(
        conn,
        "outbound_sequences",
        &sequence_id,
        "sequence not found",
    )?;
    outbound_payload_insert(
        &mut engagement,
        "sequence_id",
        Value::String(sequence_id.clone()),
    );
    outbound_payload_insert(&mut engagement, "sequence_snapshot", sequence.clone());
    outbound_payload_insert(
        &mut engagement,
        "sequence_reapplied_at_ms",
        Value::Number(serde_json::Number::from(now)),
    );
    outbound_put_i64(&mut engagement, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_engagements",
        &engagement_id,
        now,
        engagement.clone(),
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "engagement": engagement,
        "sequence_id": sequence_id,
    }))
}

/// Extract the sequence-version context recorded on an engagement so a generated
/// draft can be traced back to the exact sequence revision it was produced from.
/// The version is the snapshot timestamp captured by
/// `outbound.engagement.reapply_sequence` (falling back to an explicit `version`
/// field), defaulting to 0 when no sequence has been projected yet.
fn outbound_engagement_sequence_context(engagement: &Value) -> (Option<String>, i64) {
    let sequence_id = outbound_first_string(&[
        outbound_string(engagement, &["payload", "sequence_id"]),
        outbound_string(engagement, &["sequence_id"]),
    ]);
    let version = engagement
        .pointer("/payload/sequence_snapshot/updated_at_ms")
        .and_then(Value::as_i64)
        .or_else(|| {
            engagement
                .pointer("/payload/sequence_snapshot/version")
                .and_then(Value::as_i64)
        })
        .unwrap_or(0);
    (sequence_id, version)
}

/// Persist updated proposed_slots (or other slot metadata) into an existing
/// meeting request. Empty proposed_slots is allowed and signals the next
/// draft.prepare(scheduling) call to regenerate from scratch.
fn outbound_handle_scheduling_update_slots(
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let request_id = outbound_required_string(&command.payload, &["meeting_request_id"])?;
    let mut request = outbound_load_required(
        conn,
        "outbound_meeting_requests",
        &request_id,
        "meeting_request not found",
    )?;
    if let Some(slots) = command.payload.get("proposed_slots") {
        if let Some(obj) = request.as_object_mut() {
            obj.insert("proposed_slots".to_string(), slots.clone());
        }
    }
    for key in ["duration_minutes", "slot_strategy", "calendar_account_id"] {
        if let Some(value) = command.payload.get(key) {
            if let Some(obj) = request.as_object_mut() {
                obj.insert(key.to_string(), value.clone());
            }
        }
    }
    outbound_put_i64(&mut request, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_meeting_requests",
        &request_id,
        now,
        request.clone(),
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "meeting_request": request,
    }))
}

/// Reconcile outbound_messages.send_status with terminal SMTP delivery outcomes
/// recorded by the mailserver runner in `stalwart_smtp_delivery_log`.
/// For every outbound_message with `send_status = queued_for_provider` and a
/// known `provider_message_id`, this looks up the latest delivery log row and
/// promotes the message to `sent` (delivered) or `failed` accordingly. Runs are
/// idempotent: already-final messages are skipped.
fn outbound_handle_provider_reconcile(
    root: &Path,
    conn: &Connection,
    _command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let core_db = crate::paths::core_db(root);
    let mut updated = Vec::new();
    let mut checked: i64 = 0;
    let messages = {
        let mut stmt = conn.prepare(
            "SELECT payload_json FROM business_records
             WHERE collection = 'outbound_messages' AND deleted = 0",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out: Vec<Value> = Vec::new();
        for row in rows {
            let raw = row?;
            if let Ok(value) = serde_json::from_str::<Value>(&raw) {
                out.push(value);
            }
        }
        out
    };

    let log_conn = if core_db.exists() {
        Some(Connection::open(&core_db)?)
    } else {
        None
    };

    for mut message in messages {
        let send_status =
            outbound_string(&message, &["send_status"]).unwrap_or_else(|| "draft".to_string());
        if !matches!(
            send_status.as_str(),
            "queued_for_provider" | "queued" | "approved_not_sent"
        ) {
            continue;
        }
        let provider_id = outbound_first_string(&[
            outbound_string(&message, &["provider_message_id"]),
            outbound_string(&message, &["payload", "provider_queue_id"]),
            outbound_string(&message, &["payload", "provider_message_id"]),
        ]);
        let Some(provider_id) = provider_id else {
            continue;
        };
        let Some(message_id) = outbound_string(&message, &["id"]) else {
            continue;
        };
        checked += 1;
        let Some(ref log_conn) = log_conn else {
            continue;
        };
        let outcome: Option<(String, Option<String>, i64)> = log_conn
            .query_row(
                "SELECT outcome, error_text, completed_at
                 FROM stalwart_smtp_delivery_log
                 WHERE id = ?1
                 ORDER BY completed_at DESC LIMIT 1",
                rusqlite::params![provider_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .unwrap_or(None);
        let Some((outcome, error_text, completed_at)) = outcome else {
            continue;
        };
        let new_status = match outcome.as_str() {
            "delivered" => "sent",
            "failed" => "failed",
            other => other,
        };
        outbound_put_string(&mut message, "send_status", new_status);
        outbound_payload_insert(
            &mut message,
            "provider_dispatch_status",
            Value::String(outcome.clone()),
        );
        outbound_payload_insert(
            &mut message,
            "provider_completed_at_ms",
            Value::Number(serde_json::Number::from(completed_at)),
        );
        if let Some(text) = error_text.as_ref() {
            outbound_payload_insert(
                &mut message,
                "provider_error_text",
                Value::String(text.clone()),
            );
        }
        if new_status == "sent" {
            outbound_put_i64(&mut message, "sent_at_ms", completed_at);
            outbound_payload_insert(
                &mut message,
                "delivered_at_ms",
                Value::Number(serde_json::Number::from(completed_at)),
            );
        }
        outbound_put_i64(&mut message, "updated_at_ms", now);
        upsert_business_record(conn, "outbound_messages", &message_id, now, message.clone())?;
        if new_status == "sent" {
            if let Some(engagement_id) = outbound_string(&message, &["engagement_id"]) {
                outbound_update_engagement_status(conn, &engagement_id, "sent", now)?;
            }
        }
        updated.push(serde_json::json!({
            "message_id": message_id,
            "outcome": outcome,
            "send_status": new_status,
        }));
    }

    Ok(serde_json::json!({
        "ok": true,
        "checked": checked,
        "updated": updated,
        "log_available": log_conn.is_some(),
    }))
}

fn outbound_handle_campaign_apply_setup(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let campaign_id = outbound_required_string(&command.payload, &["campaign_id"])?;
    let patch = command
        .payload
        .get("campaign_payload_patch")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("campaign_payload_patch object is required"))?;
    let mut campaign = outbound_load_required_or_rxdb(
        root,
        conn,
        "outbound_campaigns",
        &campaign_id,
        "campaign not found",
    )?;
    outbound_put_string(&mut campaign, "id", campaign_id.clone());
    outbound_put_default_object(&mut campaign, "payload");
    if let Some(status) = outbound_string(&command.payload, &["status"]) {
        if matches!(
            status.as_str(),
            "setup_required" | "active" | "paused" | "closed"
        ) {
            outbound_put_string(&mut campaign, "status", status);
        }
    }
    {
        let payload = campaign
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| anyhow::anyhow!("campaign payload object is required"))?;
        for (key, value) in patch {
            payload.insert(key.clone(), value.clone());
        }
        let apply_command_id = command.id.clone().unwrap_or_default();
        let source_command_id =
            outbound_string(&command.payload, &["source_command_id"]).unwrap_or_default();
        payload.insert(
            "campaign_setup_task".to_string(),
            serde_json::json!({
                "command_id": if source_command_id.is_empty() { apply_command_id.clone() } else { source_command_id.clone() },
                "apply_command_id": apply_command_id,
                "source_command_id": source_command_id,
                "status": "completed",
                "skill": outbound_string(&command.payload, &["skill"]).unwrap_or_else(|| "business-os-outbound-campaign-setup".to_string()),
                "applied_at_ms": now,
            }),
        );
    }
    outbound_put_i64(&mut campaign, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_campaigns",
        &campaign_id,
        now,
        campaign.clone(),
    )?;
    upsert_rxdb_collection_record(
        root,
        "outbound_campaigns",
        &campaign_id,
        now,
        campaign.clone(),
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "campaign": campaign,
        "campaign_id": campaign_id,
    }))
}

fn outbound_handle_campaign_briefing_update(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let campaign_id = outbound_required_string(&command.payload, &["campaign_id"])?;
    let mut campaign = outbound_load_required_or_rxdb(
        root,
        conn,
        "outbound_campaigns",
        &campaign_id,
        "campaign not found",
    )?;
    outbound_put_string(&mut campaign, "id", campaign_id.clone());
    if let Some(name) = outbound_string(&command.payload, &["name"]) {
        anyhow::ensure!(!name.trim().is_empty(), "campaign name is required");
        outbound_put_string(&mut campaign, "name", name.trim().to_string());
    }
    if let Some(objective) = outbound_string(&command.payload, &["objective"]) {
        outbound_put_string(&mut campaign, "objective", objective.trim().to_string());
    }
    outbound_put_default_object(&mut campaign, "payload");
    if let Some(payload_patch) = command
        .payload
        .get("payload_patch")
        .and_then(Value::as_object)
    {
        let payload = campaign
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| anyhow::anyhow!("campaign payload object is required"))?;
        for key in [
            "subtitle",
            "scope",
            "briefing",
            "briefing_template_id",
            "briefing_language",
            "campaign_setup_task",
        ] {
            if let Some(value) = payload_patch.get(key) {
                payload.insert(key.to_string(), value.clone());
            }
        }
    }
    outbound_put_i64(&mut campaign, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_campaigns",
        &campaign_id,
        now,
        campaign.clone(),
    )?;
    upsert_rxdb_collection_record(
        root,
        "outbound_campaigns",
        &campaign_id,
        now,
        campaign.clone(),
    )?;
    Ok(serde_json::json!({
        "ok": true,
        "campaign": campaign,
        "campaign_id": campaign_id,
    }))
}

fn outbound_handle_reply_match(
    root: &Path,
    conn: &Connection,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let engagement_id = outbound_required_string(&command.payload, &["engagement_id"])?;
    let reply_message_key = outbound_required_string(&command.payload, &["reply_message_id"])
        .or_else(|_| outbound_required_string(&command.payload, &["communication_message_key"]))?;
    let classification = outbound_string(&command.payload, &["classification"])
        .unwrap_or_else(|| "unclear".to_string());
    let outbound_message_id =
        outbound_string(&command.payload, &["outbound_message_id"]).unwrap_or_default();

    let mut engagement = outbound_load_required(
        conn,
        "outbound_engagements",
        &engagement_id,
        "engagement not found",
    )?;
    outbound_put_string(&mut engagement, "status", "reply_received".to_string());
    outbound_payload_insert(
        &mut engagement,
        "reply_classification",
        Value::String(classification.clone()),
    );
    outbound_payload_insert(
        &mut engagement,
        "reply_message_id",
        Value::String(reply_message_key.clone()),
    );
    outbound_payload_insert(
        &mut engagement,
        "reply_matched_at_ms",
        Value::Number(serde_json::Number::from(now)),
    );
    outbound_put_i64(&mut engagement, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_engagements",
        &engagement_id,
        now,
        engagement.clone(),
    )?;

    let suppression_id =
        outbound_apply_reply_suppression(conn, &engagement, &engagement_id, &classification, now)?;

    let pending_messages = outbound_load_records_by_string_field(
        conn,
        "outbound_messages",
        "engagement_id",
        &engagement_id,
    )?;
    let mut cancelled = Vec::new();
    for mut message in pending_messages {
        let send_status =
            outbound_string(&message, &["send_status"]).unwrap_or_else(|| "draft".to_string());
        let direction =
            outbound_string(&message, &["direction"]).unwrap_or_else(|| "outbound".to_string());
        if direction != "outbound" {
            continue;
        }
        if matches!(
            send_status.as_str(),
            "sent" | "delivered" | "queued_for_provider"
        ) {
            continue;
        }
        if matches!(send_status.as_str(), "cancelled" | "paused") {
            continue;
        }
        let Some(message_id) = outbound_string(&message, &["id"]) else {
            continue;
        };
        outbound_put_string(&mut message, "send_status", "cancelled");
        outbound_payload_insert(
            &mut message,
            "cancelled_reason",
            Value::String("reply_received".to_string()),
        );
        outbound_payload_insert(
            &mut message,
            "cancelled_at_ms",
            Value::Number(serde_json::Number::from(now)),
        );
        outbound_put_i64(&mut message, "updated_at_ms", now);
        upsert_business_record(conn, "outbound_messages", &message_id, now, message)?;
        cancelled.push(message_id);
    }

    // Best-effort: annotate the matched communication_message with outbound metadata.
    let channel_path = crate::paths::core_db(root);
    if channel_path.exists() {
        if let Ok(mut channel_conn) = channels::open_channel_db(&channel_path) {
            let _ = annotate_communication_message_with_outbound(
                &mut channel_conn,
                &reply_message_key,
                &engagement_id,
                &outbound_message_id,
                &classification,
            );
        }
    }

    Ok(serde_json::json!({
        "ok": true,
        "engagement": engagement,
        "classification": classification,
        "reply_message_key": reply_message_key,
        "cancelled_message_ids": cancelled,
        "suppression_id": suppression_id,
    }))
}

fn annotate_communication_message_with_outbound(
    conn: &mut Connection,
    message_key: &str,
    engagement_id: &str,
    outbound_message_id: &str,
    classification: &str,
) -> anyhow::Result<()> {
    let row: Option<String> = conn
        .query_row(
            "SELECT metadata_json FROM communication_messages WHERE message_key = ?1",
            rusqlite::params![message_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(metadata_text) = row else {
        return Ok(());
    };
    let mut metadata: Value =
        serde_json::from_str(&metadata_text).unwrap_or_else(|_| serde_json::json!({}));
    let object = metadata.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!("communication_messages.metadata_json is not an object for {message_key}")
    })?;
    object.insert(
        "outbound_engagement_id".to_string(),
        Value::String(engagement_id.to_string()),
    );
    if !outbound_message_id.is_empty() {
        object.insert(
            "outbound_message_id".to_string(),
            Value::String(outbound_message_id.to_string()),
        );
    }
    object.insert(
        "outbound_reply_classification".to_string(),
        Value::String(classification.to_string()),
    );
    let updated = serde_json::to_string(&metadata)?;
    conn.execute(
        "UPDATE communication_messages SET metadata_json = ?1 WHERE message_key = ?2",
        rusqlite::params![updated, message_key],
    )?;
    Ok(())
}

pub(super) fn outbound_payload_insert(record: &mut Value, key: &str, value: Value) {
    if !matches!(record.get("payload"), Some(Value::Object(_))) {
        outbound_put_default_object(record, "payload");
    }
    if let Some(payload) = record.get_mut("payload").and_then(Value::as_object_mut) {
        payload.insert(key.to_string(), value);
    }
}

pub(super) fn outbound_email_account_key_from(value: Option<String>) -> Option<String> {
    let raw = value?.trim().to_ascii_lowercase();
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with("email:") {
        return Some(raw);
    }
    if raw.contains('@') {
        return Some(format!("email:{raw}"));
    }
    Some(raw)
}

fn outbound_email_address_from_account_key(account_key: &str) -> String {
    account_key
        .trim()
        .strip_prefix("email:")
        .unwrap_or(account_key.trim())
        .to_ascii_lowercase()
}

fn outbound_sync_email_message_to_communication(
    root: &Path,
    message: &mut Value,
    status: &str,
) -> anyhow::Result<()> {
    let Some(account_key) = outbound_email_account_key_from(outbound_first_string(&[
        outbound_string(message, &["communication_account_key"]),
        outbound_string(message, &["payload", "communication_account_key"]),
        outbound_string(message, &["sender_account_id"]),
    ])) else {
        return Ok(());
    };
    let account_address = outbound_email_address_from_account_key(&account_key);
    if !account_key.starts_with("email:") || !account_address.contains('@') {
        return Ok(());
    }
    let Some(recipient_email) = outbound_string(message, &["recipient_email"]) else {
        return Ok(());
    };
    let Some(message_id) = outbound_string(message, &["id"]) else {
        return Ok(());
    };
    let engagement_id = outbound_string(message, &["engagement_id"]).unwrap_or_default();
    let campaign_id = outbound_string(message, &["campaign_id"]).unwrap_or_default();
    let subject = outbound_string(message, &["subject"]).unwrap_or_default();
    let body_text = outbound_string(message, &["body_text"]).unwrap_or_default();
    let body_html = outbound_string(message, &["body_html"]).unwrap_or_default();
    let message_key = outbound_first_string(&[
        outbound_string(message, &["communication_message_key"]),
        outbound_string(message, &["payload", "communication_message_key"]),
    ])
    .unwrap_or_else(|| {
        format!(
            "email:{}:outbound:{}",
            account_address,
            channels::stable_digest(&message_id)
        )
    });
    let thread_key = outbound_first_string(&[
        outbound_string(message, &["thread_key"]),
        outbound_string(message, &["payload", "thread_key"]),
    ])
    .unwrap_or_else(|| {
        let material = format!("{account_key}|{campaign_id}|{engagement_id}|{recipient_email}");
        format!(
            "email:{}:outbound-thread:{}",
            account_address,
            channels::stable_digest(&material)
        )
    });
    let now_iso = channels::now_iso_string();
    let recipient_addresses_json = serde_json::to_string(&vec![recipient_email.clone()])?;
    let preview = channels::preview_text(&body_text, &subject);
    let remote_id = outbound_first_string(&[
        outbound_string(message, &["provider_message_id"]),
        outbound_string(message, &["payload", "provider_message_id"]),
        outbound_string(message, &["payload", "provider_queue_id"]),
    ])
    .unwrap_or_else(|| message_id.clone());
    let provider_send_executed = message
        .get("payload")
        .and_then(|payload| payload.get("provider_send_executed"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let provider_dispatch_status =
        outbound_string(message, &["payload", "provider_dispatch_status"])
            .unwrap_or_else(|| "not_dispatched".to_string());
    let provider_queue_id = outbound_first_string(&[
        outbound_string(message, &["payload", "provider_queue_id"]),
        outbound_string(message, &["provider_message_id"]),
    ])
    .unwrap_or_default();
    let metadata = serde_json::json!({
        "source": "business-os.outbound",
        "campaign_id": campaign_id,
        "outbound_campaign_id": campaign_id,
        "engagement_id": engagement_id,
        "outbound_engagement_id": engagement_id,
        "outbound_message_id": message_id,
        "communication_account_key": account_key,
        "communication_thread_key": thread_key,
        "communication_message_key": message_key,
        "approval_status": outbound_string(message, &["approval_status"]).unwrap_or_default(),
        "send_status": outbound_string(message, &["send_status"]).unwrap_or_default(),
        "provider_dispatch_status": provider_dispatch_status,
        "provider_queue_id": provider_queue_id,
        "provider_send_executed": provider_send_executed,
    });
    let metadata_json = serde_json::to_string(&metadata)?;
    let mut channel_conn = channels::open_channel_db(&crate::paths::core_db(root))?;
    channels::upsert_communication_message(
        &mut channel_conn,
        channels::UpsertMessage {
            message_key: &message_key,
            channel: "email",
            account_key: &account_key,
            thread_key: &thread_key,
            remote_id: &remote_id,
            direction: "outbound",
            folder_hint: "outbound",
            sender_display: "",
            sender_address: &account_address,
            recipient_addresses_json: &recipient_addresses_json,
            cc_addresses_json: "[]",
            bcc_addresses_json: "[]",
            subject: &subject,
            preview: &preview,
            body_text: &body_text,
            body_html: &body_html,
            raw_payload_ref: "",
            trust_level: "business-os",
            status,
            seen: true,
            has_attachments: false,
            external_created_at: &now_iso,
            observed_at: &now_iso,
            metadata_json: &metadata_json,
        },
    )?;
    channels::refresh_thread(&mut channel_conn, &thread_key)?;
    outbound_put_string(message, "sender_account_id", account_key.clone());
    outbound_put_string(message, "communication_account_key", account_key.clone());
    outbound_put_string(message, "communication_message_key", message_key.clone());
    outbound_put_string(message, "thread_key", thread_key.clone());
    outbound_payload_insert(
        message,
        "communication_account_key",
        Value::String(account_key),
    );
    outbound_payload_insert(
        message,
        "communication_message_key",
        Value::String(message_key),
    );
    outbound_payload_insert(message, "thread_key", Value::String(thread_key));
    Ok(())
}

fn outbound_queue_email_delivery(root: &Path, message: &mut Value) -> anyhow::Result<String> {
    let sender_account_id = outbound_required_string(message, &["sender_account_id"])?;
    let from = outbound_email_address_from_account_key(&sender_account_id);
    let to = outbound_required_string(message, &["recipient_email"])?;
    let subject = outbound_string(message, &["subject"]).unwrap_or_default();
    let body_text = outbound_string(message, &["body_text"]).unwrap_or_default();
    let mut body_html = outbound_string(message, &["body_html"]).unwrap_or_default();
    anyhow::ensure!(
        !body_text.trim().is_empty() || !body_html.trim().is_empty(),
        "outbound email body is empty (body_text and body_html both blank)"
    );
    let db_path = root
        .join("runtime/ctox.sqlite3")
        .to_string_lossy()
        .into_owned();
    let store = ctox_mailserver::store::sqlite::SqliteStore::new(&db_path);
    store.init()?;
    if !body_html.trim().is_empty() {
        body_html = outbound_prepare_tracked_html(&store, message, &body_html)?;
    }
    let sender_domain = from.split('@').nth(1).unwrap_or("ctox.local");
    let msg_id = format!("<{}@{}>", Uuid::new_v4(), sender_domain);
    let date = chrono::Utc::now().to_rfc2822();

    let header = format!(
        "From: {from}\r\n\
         To: {to}\r\n\
         Subject: {subject}\r\n\
         Message-ID: {msg_id}\r\n\
         Date: {date}\r\n\
         MIME-Version: 1.0\r\n",
        from = outbound_header_value(&from),
        to = outbound_header_value(&to),
        subject = outbound_header_value(&subject),
        msg_id = outbound_header_value(&msg_id),
        date = outbound_header_value(&date),
    );

    let normalize = |body: &str| body.replace("\r\n", "\n").replace('\r', "\n");

    let rfc822_body = if !body_html.trim().is_empty() && !body_text.trim().is_empty() {
        // Send a proper multipart/alternative with both representations.
        let boundary = format!("ctox-{}", Uuid::new_v4().simple());
        format!(
            "{header}Content-Type: multipart/alternative; boundary=\"{boundary}\"\r\n\r\n\
             --{boundary}\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\
             Content-Transfer-Encoding: 8bit\r\n\r\n\
             {text}\r\n\
             --{boundary}\r\n\
             Content-Type: text/html; charset=utf-8\r\n\
             Content-Transfer-Encoding: 8bit\r\n\r\n\
             {html}\r\n\
             --{boundary}--\r\n",
            text = normalize(&body_text),
            html = normalize(&body_html),
        )
    } else if !body_html.trim().is_empty() {
        // HTML-only body — also include a plain-text fallback derived from the HTML.
        let fallback_text = outbound_html_to_plain_text(&body_html);
        let boundary = format!("ctox-{}", Uuid::new_v4().simple());
        format!(
            "{header}Content-Type: multipart/alternative; boundary=\"{boundary}\"\r\n\r\n\
             --{boundary}\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\
             Content-Transfer-Encoding: 8bit\r\n\r\n\
             {text}\r\n\
             --{boundary}\r\n\
             Content-Type: text/html; charset=utf-8\r\n\
             Content-Transfer-Encoding: 8bit\r\n\r\n\
             {html}\r\n\
             --{boundary}--\r\n",
            text = fallback_text,
            html = normalize(&body_html),
        )
    } else {
        format!(
            "{header}Content-Type: text/plain; charset=utf-8\r\n\
             Content-Transfer-Encoding: 8bit\r\n\r\n\
             {body}\r\n",
            body = normalize(&body_text),
        )
    };

    store
        .queue_email(&from, &to, &rfc822_body)
        .map_err(Into::into)
}

fn outbound_prepare_tracked_html(
    store: &ctox_mailserver::store::sqlite::SqliteStore,
    message: &mut Value,
    html: &str,
) -> anyhow::Result<String> {
    let settings = store.load_runtime_settings()?;
    let explicitly_disabled = message
        .pointer("/payload/tracking_enabled")
        .and_then(Value::as_bool)
        == Some(false);
    if settings.tracking_base_url.is_empty() || explicitly_disabled {
        outbound_payload_insert(message, "tracking_enabled", Value::Bool(false));
        return Ok(html.to_string());
    }

    let message_id = outbound_required_string(message, &["id"])?;
    let campaign_id = outbound_string(message, &["campaign_id"]);
    let base = settings.tracking_base_url.trim_end_matches('/');
    let link_pattern = regex::Regex::new(r#"(?i)href\s*=\s*[\"'](https?://[^\"']+)[\"']"#)
        .context("compile outbound link tracking pattern")?;
    let mut tracked = String::with_capacity(html.len() + 256);
    let mut cursor = 0;
    let mut click_links = 0_i64;
    for captures in link_pattern.captures_iter(html) {
        let Some(whole) = captures.get(0) else {
            continue;
        };
        let Some(target) = captures.get(1) else {
            continue;
        };
        tracked.push_str(&html[cursor..whole.start()]);
        let token = Uuid::new_v4().simple().to_string();
        store.save_tracking_token(
            &token,
            &message_id,
            campaign_id.as_deref(),
            "clicked",
            Some(target.as_str()),
        )?;
        tracked.push_str(&format!("href=\"{base}/mail/track/c/{token}\""));
        cursor = whole.end();
        click_links += 1;
    }
    tracked.push_str(&html[cursor..]);

    let open_token = Uuid::new_v4().simple().to_string();
    store.save_tracking_token(
        &open_token,
        &message_id,
        campaign_id.as_deref(),
        "opened",
        None,
    )?;
    let pixel = format!(
        r#"<img src="{base}/mail/track/o/{open_token}.gif" width="1" height="1" alt="" style="display:block;width:1px;height:1px;border:0" />"#
    );
    let lower = tracked.to_ascii_lowercase();
    if let Some(index) = lower.rfind("</body>") {
        tracked.insert_str(index, &pixel);
    } else {
        tracked.push_str(&pixel);
    }
    outbound_payload_insert(message, "tracking_enabled", Value::Bool(true));
    outbound_payload_insert(
        message,
        "tracking_status",
        Value::String("armed".to_string()),
    );
    outbound_payload_insert(message, "tracking_open_token", Value::String(open_token));
    outbound_payload_insert(
        message,
        "tracking_click_links",
        Value::Number(click_links.into()),
    );
    Ok(tracked)
}

pub(super) fn record_mail_tracking_event(
    root: &Path,
    token: &str,
    expected_event_type: &str,
    user_agent: Option<&str>,
) -> anyhow::Result<Option<Value>> {
    let db_path = crate::paths::core_db(root).to_string_lossy().into_owned();
    let mail_store = ctox_mailserver::store::sqlite::SqliteStore::new(&db_path);
    mail_store.init()?;
    let Some(tracking) = mail_store.tracking_token(token)? else {
        return Ok(None);
    };
    if tracking.event_type != expected_event_type {
        return Ok(None);
    }
    let now = now_ms() as i64;
    mail_store.record_tracking_event(
        token,
        &tracking.message_id,
        &tracking.event_type,
        now,
        user_agent,
    )?;

    let conn = open_store(root)?;
    if let Ok(mut message) = outbound_load_required(
        &conn,
        "outbound_messages",
        &tracking.message_id,
        "outbound message not found",
    ) {
        let (count_key, first_key, last_key) = if tracking.event_type == "clicked" {
            ("click_count", "first_clicked_at_ms", "last_clicked_at_ms")
        } else {
            ("open_count", "first_opened_at_ms", "last_opened_at_ms")
        };
        let count = message
            .pointer(&format!("/payload/{count_key}"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
            + 1;
        outbound_payload_insert(&mut message, count_key, Value::Number(count.into()));
        if message
            .pointer(&format!("/payload/{first_key}"))
            .and_then(Value::as_i64)
            .is_none()
        {
            outbound_payload_insert(&mut message, first_key, Value::Number(now.into()));
        }
        outbound_payload_insert(&mut message, last_key, Value::Number(now.into()));
        outbound_payload_insert(
            &mut message,
            "tracking_status",
            Value::String(tracking.event_type.clone()),
        );
        outbound_put_i64(&mut message, "updated_at_ms", now);
        upsert_business_record(
            &conn,
            "outbound_messages",
            &tracking.message_id,
            now,
            message,
        )?;
    }

    Ok(Some(serde_json::json!({
        "message_id": tracking.message_id,
        "campaign_id": tracking.campaign_id,
        "event_type": tracking.event_type,
        "target_url": tracking.target_url,
        "occurred_at_ms": now,
    })))
}

/// Minimal HTML → plain-text fallback for outbound mails that only carry body_html.
/// Strips tags and decodes a handful of common entities; intentionally simple — for
/// rich rendering, operators should also fill body_text in the draft pipeline.
fn outbound_html_to_plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut last_was_space = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            _ if in_tag => {}
            '\r' | '\n' | '\t' => {
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            ' ' => {
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            }
            other => {
                out.push(other);
                last_was_space = false;
            }
        }
    }
    let collapsed = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    collapsed.trim().to_string()
}

fn outbound_header_value(value: &str) -> String {
    value
        .replace(['\r', '\n'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Atomically reserve one daily send slot for the sender account, enforcing the
/// per-account daily cap under concurrent commands. The check-and-increment runs
/// inside a `BEGIN IMMEDIATE` transaction so two parallel `send_approved`
/// commands cannot both pass when only one slot remains: the second writer blocks
/// on the write lock (WAL + busy_timeout), then reads the already-incremented
/// count. Bails — leaving the counter untouched, since the transaction rolls back
/// on the early return — when the account is blocked, ineligible, or already at
/// its cap. Mirrors the read-only checks in `outbound_enforce_account_limit` so
/// the cheap early gate and the authoritative reservation agree.
fn outbound_reserve_account_send_slot(
    conn: &Connection,
    sender_account_id: &str,
    now: i64,
) -> anyhow::Result<()> {
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    let canonical = outbound_email_account_key_from(Some(sender_account_id.to_string()))
        .unwrap_or_else(|| sender_account_id.to_string());
    let existing = outbound_load_record(&tx, "outbound_account_limits", &canonical)?.or(
        outbound_load_record(&tx, "outbound_account_limits", sender_account_id)?,
    );
    let Some(mut limit) = existing else {
        // No limit record: no cap configured, nothing to reserve.
        tx.commit()?;
        return Ok(());
    };
    // Re-validate eligibility under the lock so a block applied by a concurrent
    // command takes effect on the very next send.
    let blocked = limit
        .get("blocked")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    anyhow::ensure!(
        !blocked,
        "sender account is blocked for outbound communication"
    );
    let status = outbound_string(&limit, &["status"]).unwrap_or_else(|| "active".to_string());
    anyhow::ensure!(
        !matches!(
            status.as_str(),
            "blocked" | "locked" | "suspended" | "disabled"
        ),
        "sender account status `{status}` is not eligible to send"
    );
    let current = limit
        .get("sent_today")
        .and_then(Value::as_i64)
        .or_else(|| limit.get("daily_sent_count").and_then(Value::as_i64))
        .unwrap_or(0);
    let next = current + 1;
    if let Some(limit_value) = limit.get("daily_limit").and_then(Value::as_i64) {
        // daily_limit semantics: <= 0 means "no daily cap configured".
        if limit_value > 0 {
            anyhow::ensure!(next <= limit_value, "sender account daily limit exhausted");
        }
    }
    outbound_put_i64(&mut limit, "sent_today", next);
    outbound_put_i64(&mut limit, "daily_sent_count", next);
    if let Some(limit_value) = limit.get("daily_limit").and_then(Value::as_i64) {
        if limit_value > 0 {
            outbound_put_i64(
                &mut limit,
                "remaining_today",
                limit_value.saturating_sub(next).max(0),
            );
        }
    }
    outbound_put_i64(&mut limit, "updated_at_ms", now);
    upsert_business_record(&tx, "outbound_account_limits", &canonical, now, limit)?;
    tx.commit()?;
    Ok(())
}

/// Release a previously reserved daily send slot, decrementing the counter under
/// a `BEGIN IMMEDIATE` transaction. Used when a send was reserved but never
/// reached the provider (queue failure), so the daily counter stays accurate for
/// a later retry. Never drops below zero.
fn outbound_release_account_send_slot(
    conn: &Connection,
    sender_account_id: &str,
    now: i64,
) -> anyhow::Result<()> {
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    let canonical = outbound_email_account_key_from(Some(sender_account_id.to_string()))
        .unwrap_or_else(|| sender_account_id.to_string());
    let existing = outbound_load_record(&tx, "outbound_account_limits", &canonical)?.or(
        outbound_load_record(&tx, "outbound_account_limits", sender_account_id)?,
    );
    let Some(mut limit) = existing else {
        tx.commit()?;
        return Ok(());
    };
    let current = limit
        .get("sent_today")
        .and_then(Value::as_i64)
        .or_else(|| limit.get("daily_sent_count").and_then(Value::as_i64))
        .unwrap_or(0);
    let next = (current - 1).max(0);
    outbound_put_i64(&mut limit, "sent_today", next);
    outbound_put_i64(&mut limit, "daily_sent_count", next);
    if let Some(limit_value) = limit.get("daily_limit").and_then(Value::as_i64) {
        if limit_value > 0 {
            outbound_put_i64(
                &mut limit,
                "remaining_today",
                limit_value.saturating_sub(next).max(0),
            );
        }
    }
    outbound_put_i64(&mut limit, "updated_at_ms", now);
    upsert_business_record(&tx, "outbound_account_limits", &canonical, now, limit)?;
    tx.commit()?;
    Ok(())
}

fn outbound_load_record_or_rxdb(
    root: &Path,
    conn: &Connection,
    collection: &str,
    record_id: &str,
) -> anyhow::Result<Option<Value>> {
    if let Some(record) = outbound_load_record(conn, collection, record_id)? {
        return Ok(Some(record));
    }
    let Some(record) = load_rxdb_collection_record(root, collection, record_id)? else {
        return Ok(None);
    };
    let updated_at_ms = record
        .get("updated_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| now_ms() as i64);
    upsert_business_record(conn, collection, record_id, updated_at_ms, record.clone())?;
    Ok(Some(record))
}

fn outbound_load_required_or_rxdb(
    root: &Path,
    conn: &Connection,
    collection: &str,
    record_id: &str,
    message: &str,
) -> anyhow::Result<Value> {
    outbound_load_record_or_rxdb(root, conn, collection, record_id)?
        .with_context(|| message.to_string())
}

fn outbound_latest_message(messages: &[Value]) -> Option<Value> {
    messages
        .iter()
        .max_by_key(|message| {
            message
                .get("updated_at_ms")
                .and_then(Value::as_i64)
                .or_else(|| message.get("created_at_ms").and_then(Value::as_i64))
                .unwrap_or(0)
        })
        .cloned()
}

/// Resolve a persisted outbound skillbook into a one-line strategy hint for the
/// deterministic draft fallback: its mission plus the first non-negotiable rule.
/// Returns `None` when the skillbook is absent or carries no usable text, so the
/// caller falls back to the generic strategy line.
fn outbound_skillbook_guidance(
    conn: &Connection,
    skillbook_id: &str,
) -> anyhow::Result<Option<String>> {
    let Some(skillbook) = outbound_load_record(conn, "outbound_skillbooks", skillbook_id)? else {
        return Ok(None);
    };
    let mission = outbound_string(&skillbook, &["mission"]).unwrap_or_default();
    let first_rule = skillbook
        .get("non_negotiable_rules")
        .and_then(Value::as_array)
        .and_then(|rules| rules.first())
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let guidance = match (mission.trim().is_empty(), first_rule.trim().is_empty()) {
        (true, true) => return Ok(None),
        (false, true) => mission,
        (true, false) => first_rule,
        (false, false) => format!("{mission} Leitplanke: {first_rule}."),
    };
    Ok(Some(guidance))
}

fn outbound_generate_automated_draft(
    engagement: &Value,
    latest_message: Option<&Value>,
    request: &Value,
    draft_kind: &str,
    skillbook_guidance: Option<&str>,
) -> Value {
    let contact_name = outbound_first_string(&[
        outbound_string(engagement, &["payload", "contact_name"]),
        outbound_string(request, &["contact_name"]),
    ])
    .unwrap_or_else(|| "Hallo".to_string());
    let company_name = outbound_first_string(&[
        outbound_string(engagement, &["payload", "company_name"]),
        outbound_string(request, &["company_name"]),
    ])
    .unwrap_or_else(|| "Ihr Unternehmen".to_string());
    let previous_subject =
        latest_message.and_then(|message| outbound_string(message, &["subject"]));
    let subject = match draft_kind {
        "initial" => format!("Austausch zu {company_name}"),
        "reply" => previous_subject
            .map(|subject| format!("Re: {subject}"))
            .unwrap_or_else(|| format!("Re: Austausch zu {company_name}")),
        "scheduling" => previous_subject
            .map(|subject| format!("Re: {subject}"))
            .unwrap_or_else(|| "Terminvorschlag".to_string()),
        kind if kind.starts_with("followup") => previous_subject
            .map(|subject| format!("Re: {subject}"))
            .unwrap_or_else(|| format!("Kurzer Nachtrag zu {company_name}")),
        _ => previous_subject.unwrap_or_else(|| format!("Austausch zu {company_name}")),
    };
    let strategy = outbound_first_string(&[
        outbound_string(request, &["strategy_text"]),
        outbound_string(request, &["payload", "strategy_text"]),
        outbound_string(engagement, &["payload", "strategy_text"]),
        skillbook_guidance.map(str::to_string),
    ])
    .unwrap_or_else(|| "Kontextbezogen, knapp und ohne erfundene Aussagen schreiben.".to_string());
    let body_text = match draft_kind {
        "initial" => format!(
            "{contact_name},\n\nich habe mir {company_name} im Kontext der aktuellen Outbound-Campaign angesehen und einen moeglichen Anknuepfungspunkt identifiziert.\n\n{strategy}\n\nWenn das grundsaetzlich relevant ist, schlage ich einen kurzen Austausch vor.\n\nBeste Gruesse"
        ),
        "reply" => {
            let reply_text = outbound_first_string(&[
                outbound_string(request, &["reply_text"]),
                outbound_string(engagement, &["payload", "reply_text"]),
                outbound_string(engagement, &["payload", "reply_classification"]),
            ])
            .unwrap_or_else(|| "die Rueckmeldung wurde als relevant klassifiziert".to_string());
            format!(
                "{contact_name},\n\nvielen Dank fuer die Rueckmeldung. Ich habe den Kontext so verstanden: {reply_text}.\n\nGerne konkretisiere ich den naechsten Schritt und halte es knapp: {strategy}\n\nBeste Gruesse"
            )
        }
        "scheduling" => {
            let duration = request
                .get("duration_minutes")
                .and_then(Value::as_i64)
                .unwrap_or(30);
            let slot_hint = outbound_string(request, &["slot_hint"])
                .unwrap_or_else(|| "zwei bis drei passende Zeitfenster".to_string());
            format!(
                "{contact_name},\n\nsehr gerne. Fuer einen kurzen Austausch wuerde ich {duration} Minuten einplanen.\n\nIch kann {slot_hint} vorschlagen; bitte geben Sie kurz Bescheid, was bei Ihnen passt.\n\nBeste Gruesse"
            )
        }
        kind if kind.starts_with("followup") => format!(
            "{contact_name},\n\nich wollte meine vorherige Nachricht kurz nachfassen, weil der Anknuepfungspunkt zu {company_name} weiterhin relevant sein koennte.\n\n{strategy}\n\nFalls es aktuell nicht passt, reicht eine kurze Rueckmeldung.\n\nBeste Gruesse"
        ),
        _ => format!(
            "{contact_name},\n\nich bereite den naechsten Outbound-Schritt zu {company_name} vor.\n\n{strategy}\n\nBeste Gruesse"
        ),
    };
    serde_json::json!({
        "subject": subject,
        "body_text": body_text,
        "draft_kind": draft_kind,
        "requires_approval": true
    })
}

fn outbound_update_engagement_status(
    conn: &Connection,
    engagement_id: &str,
    status: &str,
    now: i64,
) -> anyhow::Result<()> {
    if engagement_id.trim().is_empty() {
        return Ok(());
    }
    let Some(mut engagement) = outbound_load_record(conn, "outbound_engagements", engagement_id)?
    else {
        return Ok(());
    };
    outbound_put_string(&mut engagement, "status", status.to_string());
    outbound_put_i64(&mut engagement, "updated_at_ms", now);
    upsert_business_record(conn, "outbound_engagements", engagement_id, now, engagement)
}

fn outbound_send_status_for_resume(message: &Value) -> &'static str {
    match outbound_string(message, &["approval_status"]).as_deref() {
        Some("approved") => "approved_not_sent",
        Some("awaiting_approval") => "awaiting_approval",
        Some("rejected") => "rejected",
        _ => "not_scheduled",
    }
}

fn outbound_engagement_status_for_message_state(message: &Value) -> &'static str {
    match outbound_string(message, &["approval_status"]).as_deref() {
        Some("approved") => "approved_for_send",
        Some("awaiting_approval") => "awaiting_approval",
        Some("rejected") => "rejected",
        _ => "draft_prepared",
    }
}

fn outbound_update_engagement_terminal_status(
    conn: &Connection,
    engagement_id: &str,
    status: &str,
    reason: Option<&str>,
    now: i64,
) -> anyhow::Result<()> {
    if engagement_id.trim().is_empty() {
        return Ok(());
    }
    let Some(mut engagement) = outbound_load_record(conn, "outbound_engagements", engagement_id)?
    else {
        return Ok(());
    };
    outbound_put_string(&mut engagement, "status", status.to_string());
    if let Some(reason) = reason.map(str::trim).filter(|value| !value.is_empty()) {
        let key = if status == "paused" {
            "paused_reason"
        } else {
            "closed_reason"
        };
        outbound_put_string(&mut engagement, key, reason.to_string());
    }
    outbound_put_i64(&mut engagement, "updated_at_ms", now);
    upsert_business_record(conn, "outbound_engagements", engagement_id, now, engagement)
}

fn outbound_message_revision(message: &Value) -> String {
    let material = serde_json::json!({
        "sender_account_id": outbound_string(message, &["sender_account_id"]).unwrap_or_default(),
        "recipient_email": outbound_string(message, &["recipient_email"]).unwrap_or_default(),
        "subject": outbound_string(message, &["subject"]).unwrap_or_default(),
        "body_text": outbound_string(message, &["body_text"]).unwrap_or_default(),
        "body_html": outbound_string(message, &["body_html"]).unwrap_or_default(),
    });
    format!("rev_{}", channels::stable_digest(&material.to_string()))
}

fn outbound_require_message_content(message: &Value) -> anyhow::Result<()> {
    let has_body = outbound_string(message, &["body_text"]).is_some()
        || outbound_string(message, &["body_html"]).is_some();
    anyhow::ensure!(has_body, "message body is required before approval");
    Ok(())
}

fn outbound_approval_audit_snapshot(approval: &Value) -> Value {
    serde_json::json!({
        "id": outbound_string(approval, &["id"]),
        "message_id": outbound_string(approval, &["message_id"]),
        "engagement_id": outbound_string(approval, &["engagement_id"]),
        "revision_id": outbound_string(approval, &["revision_id"]),
        "actor_user_id": outbound_string(approval, &["actor_user_id"]),
        "decision": outbound_string(approval, &["decision"]),
        "updated_at_ms": approval.get("updated_at_ms").and_then(Value::as_i64)
    })
}

fn outbound_message_audit_snapshot(message: &Value) -> Value {
    serde_json::json!({
        "id": outbound_string(message, &["id"]),
        "engagement_id": outbound_string(message, &["engagement_id"]),
        "campaign_id": outbound_string(message, &["campaign_id"]),
        "channel": outbound_string(message, &["channel"]),
        "draft_status": outbound_string(message, &["draft_status"]),
        "approval_status": outbound_string(message, &["approval_status"]),
        "send_status": outbound_string(message, &["send_status"]),
        "updated_at_ms": message.get("updated_at_ms").and_then(Value::as_i64)
    })
}

fn record_outbound_approval_decision_event(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    approval: &Value,
    message: &Value,
    observed_at_ms: i64,
) -> anyhow::Result<()> {
    let approval_id = outbound_required_string(approval, &["id"])?;
    let message_id = outbound_string(approval, &["message_id"])
        .or_else(|| outbound_string(message, &["id"]))
        .unwrap_or_default();
    let engagement_id = outbound_string(approval, &["engagement_id"])
        .or_else(|| outbound_string(message, &["engagement_id"]))
        .unwrap_or_default();
    let decision =
        outbound_string(approval, &["decision"]).unwrap_or_else(|| "updated".to_string());
    insert_business_event(
        conn,
        "outbound_approvals",
        &approval_id,
        "business_os.external_approval.decided",
        serde_json::json!({
            "event_type": "business_os.external_approval.decided",
            "approval_id": approval_id,
            "message_id": message_id,
            "engagement_id": engagement_id,
            "decision": decision,
            "command_id": command.id.as_deref(),
            "command_type": command.command_type.as_str(),
            "actor": session_audit_actor_context(session),
            "approval": outbound_approval_audit_snapshot(approval),
            "message": outbound_message_audit_snapshot(message),
            "observed_at_ms": observed_at_ms
        }),
        observed_at_ms,
    )
}

fn outbound_record_rejection(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let message_id = outbound_required_from_payload_or_record(
        command,
        &["message_id", "id"],
        "message_id is required",
    )?;
    let mut message =
        outbound_load_required(conn, "outbound_messages", &message_id, "message not found")?;
    let revision_id = outbound_string(&message, &["revision_id"])
        .unwrap_or_else(|| outbound_message_revision(&message));
    let approval_id = outbound_id_from_command(command, &["approval_id"], "approval")
        .unwrap_or_else(|_| format!("rejection_{message_id}_{revision_id}"));
    let engagement_id = outbound_string(&message, &["engagement_id"]).unwrap_or_default();
    let mut approval = outbound_object_payload(&command.payload);
    outbound_put_string(&mut approval, "id", approval_id.clone());
    outbound_put_string(&mut approval, "message_id", message_id.clone());
    outbound_put_string(&mut approval, "engagement_id", engagement_id.clone());
    outbound_put_string(&mut approval, "revision_id", revision_id);
    outbound_put_string(
        &mut approval,
        "actor_user_id",
        outbound_session_actor_id(session),
    );
    outbound_put_string(&mut approval, "decision", "rejected");
    outbound_put_default_i64(&mut approval, "created_at_ms", now);
    outbound_put_i64(&mut approval, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_approvals",
        &approval_id,
        now,
        approval.clone(),
    )?;
    outbound_put_string(&mut message, "approval_status", "rejected");
    outbound_put_string(&mut message, "send_status", "blocked");
    outbound_put_i64(&mut message, "updated_at_ms", now);
    upsert_business_record(conn, "outbound_messages", &message_id, now, message.clone())?;
    if !engagement_id.is_empty() {
        outbound_update_engagement_status(conn, &engagement_id, "draft_rejected", now)?;
    }
    record_outbound_approval_decision_event(conn, session, command, &approval, &message, now)?;
    Ok(serde_json::json!({
        "ok": true,
        "message": message,
        "approval": approval
    }))
}

fn outbound_record_change_request(
    conn: &Connection,
    session: &BusinessOsSession,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let message_id = outbound_required_from_payload_or_record(
        command,
        &["message_id", "id"],
        "message_id is required",
    )?;
    let mut message =
        outbound_load_required(conn, "outbound_messages", &message_id, "message not found")?;
    let revision_id = outbound_string(&message, &["revision_id"])
        .unwrap_or_else(|| outbound_message_revision(&message));
    let approval_id = outbound_id_from_command(command, &["approval_id"], "change_request")
        .unwrap_or_else(|_| format!("change_request_{message_id}_{revision_id}"));
    let engagement_id = outbound_string(&message, &["engagement_id"]).unwrap_or_default();
    let mut approval = outbound_object_payload(&command.payload);
    outbound_put_string(&mut approval, "id", approval_id.clone());
    outbound_put_string(&mut approval, "message_id", message_id.clone());
    outbound_put_string(&mut approval, "engagement_id", engagement_id.clone());
    outbound_put_string(&mut approval, "revision_id", revision_id);
    outbound_put_string(
        &mut approval,
        "actor_user_id",
        outbound_session_actor_id(session),
    );
    outbound_put_string(&mut approval, "decision", "changes_requested");
    outbound_put_default_i64(&mut approval, "created_at_ms", now);
    outbound_put_i64(&mut approval, "updated_at_ms", now);
    upsert_business_record(
        conn,
        "outbound_approvals",
        &approval_id,
        now,
        approval.clone(),
    )?;
    outbound_put_string(&mut message, "approval_status", "changes_requested");
    outbound_put_string(&mut message, "draft_status", "changes_requested");
    outbound_put_string(&mut message, "send_status", "blocked");
    outbound_put_i64(&mut message, "updated_at_ms", now);
    upsert_business_record(conn, "outbound_messages", &message_id, now, message.clone())?;
    if !engagement_id.is_empty() {
        outbound_update_engagement_status(conn, &engagement_id, "draft_changes_requested", now)?;
    }
    record_outbound_approval_decision_event(conn, session, command, &approval, &message, now)?;
    Ok(serde_json::json!({
        "ok": true,
        "message": message,
        "approval": approval
    }))
}

fn outbound_enforce_send_gate(conn: &Connection, message: &Value) -> anyhow::Result<()> {
    let message_id = outbound_required_string(message, &["id"])?;
    anyhow::ensure!(
        outbound_string(message, &["approval_status"]).as_deref() == Some("approved"),
        "outbound message must be approved before send"
    );
    let revision_id = outbound_string(message, &["revision_id"])
        .unwrap_or_else(|| outbound_message_revision(message));
    anyhow::ensure!(
        outbound_has_matching_approval(conn, &message_id, &revision_id)?,
        "approved outbound message has no matching approval for current revision"
    );
    let channel = outbound_string(message, &["channel"]).unwrap_or_else(|| "email".to_string());
    outbound_require_message_content(message)?;
    match channel.as_str() {
        "physical_letter" => {
            // Physical letters need a postal address, NOT a sender_account or email.
            let address =
                outbound_required_string(message, &["recipient_address_text"]).map_err(|_| {
                    anyhow::anyhow!(
                        "physical_letter messages require recipient_address_text before send"
                    )
                })?;
            anyhow::ensure!(
                !address.trim().is_empty(),
                "physical_letter recipient_address_text must not be blank"
            );
        }
        _ => {
            let sender_account_id = outbound_required_string(message, &["sender_account_id"])?;
            let recipient_email = outbound_required_string(message, &["recipient_email"])?;
            if let Some(reason) = outbound_recipient_suppression_reason(conn, &recipient_email)? {
                anyhow::bail!(
                    "recipient is suppressed for outbound communication (reason: {reason})"
                );
            }
            outbound_enforce_account_limit(conn, &sender_account_id)?;
        }
    }
    Ok(())
}

/// Map a send-gate / provider-queue error into a stable, replicable block-reason
/// code so the UI and downstream automation can branch on the cause instead of
/// parsing free-form text.
fn outbound_classify_send_block(error: &str) -> &'static str {
    let lowered = error.to_ascii_lowercase();
    if lowered.contains("suppress") {
        "recipient_suppressed"
    } else if lowered.contains("blocked") || lowered.contains("not eligible") {
        "sender_blocked"
    } else if lowered.contains("limit") {
        "sender_limit_exhausted"
    } else if lowered.contains("approv") {
        "approval_required"
    } else if lowered.contains("queue") || lowered.contains("provider") {
        "provider_queue_failed"
    } else if lowered.contains("recipient_address") || lowered.contains("recipient_email") {
        "missing_recipient"
    } else if lowered.contains("sender_account") {
        "missing_sender"
    } else {
        "send_blocked"
    }
}

/// Persist a failed send attempt onto the message without destroying the draft.
/// The draft body, subject, and `approval_status = approved` are untouched, so a
/// later `outbound.message.send_approved` can retry once the blocking condition
/// clears. Records the reason code, last error, attempt count, and timestamp in
/// replicable payload fields, and reflects a non-final `send_status`.
fn outbound_record_send_failure(
    conn: &Connection,
    message_id: &str,
    message: &mut Value,
    error: &str,
    now: i64,
) -> anyhow::Result<()> {
    let reason = outbound_classify_send_block(error);
    let attempts = message
        .get("payload")
        .and_then(|payload| payload.get("send_attempts"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        + 1;
    outbound_put_string(message, "send_status", "send_blocked");
    outbound_payload_insert(
        message,
        "send_block_reason",
        Value::String(reason.to_string()),
    );
    outbound_payload_insert(message, "last_send_error", Value::String(error.to_string()));
    outbound_payload_insert(
        message,
        "send_attempts",
        Value::Number(serde_json::Number::from(attempts)),
    );
    outbound_payload_insert(
        message,
        "last_send_attempt_at_ms",
        Value::Number(serde_json::Number::from(now)),
    );
    // Stays retry-able: the message is not marked final and remains approved.
    outbound_payload_insert(message, "retryable", Value::Bool(true));
    outbound_put_i64(message, "updated_at_ms", now);
    upsert_business_record(conn, "outbound_messages", message_id, now, message.clone())?;
    // Reflect the blocking condition back onto the owning engagement so the
    // pipeline/timeline UI and downstream automation can see why the send did
    // not go out, with the same reason code persisted on the message.
    if let Some(engagement_id) = outbound_string(message, &["engagement_id"]) {
        if !engagement_id.trim().is_empty() {
            if let Some(mut engagement) =
                outbound_load_record(conn, "outbound_engagements", &engagement_id)?
            {
                outbound_put_string(&mut engagement, "status", "send_blocked");
                outbound_put_string(
                    &mut engagement,
                    "last_send_block_reason",
                    reason.to_string(),
                );
                outbound_put_string(&mut engagement, "last_send_error", error.to_string());
                outbound_put_i64(&mut engagement, "last_send_block_at_ms", now);
                outbound_put_i64(&mut engagement, "updated_at_ms", now);
                upsert_business_record(
                    conn,
                    "outbound_engagements",
                    &engagement_id,
                    now,
                    engagement,
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::store::tests::{business_event_payloads, seed_business_user};
    use super::super::store::{
        accept_rxdb_business_command, load_business_record_payload, CommandOrigin,
    };
    use super::*;
    use tempfile::tempdir;

    // Klicktest P4 V14 (11.09.2026): "Adapter-Skript ansehen" could never show
    // a script; the registry read now carries the latest script of one target.
    #[test]
    fn the_registry_read_carries_the_latest_script_of_the_asked_target() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        fs::create_dir_all(root.join("runtime"))?;
        let target_file = root.join("uitest-target.json");
        fs::write(
            &target_file,
            serde_json::to_vec(&serde_json::json!({
                "target_key": "uitest-script",
                "display_name": "UITEST Skript",
                "start_url": "https://uitest-script.ctox.dev/",
                "target_kind": "prospect-research",
            }))?,
        )?;
        scrape::dispatch_capturing(
            root,
            &[
                "upsert-target".to_string(),
                "--input".to_string(),
                target_file.display().to_string(),
            ],
        )?;
        let script_file = root.join("uitest-script.js");
        fs::write(
            &script_file,
            "// UITEST_SCRIPT_MARKER\nexport default async () => [];\n",
        )?;
        scrape::dispatch_capturing(
            root,
            &[
                "register-script".to_string(),
                "--target-key".to_string(),
                "uitest-script".to_string(),
                "--script-file".to_string(),
                script_file.display().to_string(),
            ],
        )?;
        let command = BusinessCommand {
            id: Some("cmd_registry_script".to_string()),
            module: "outbound".to_string(),
            command_type: "outbound.research_source.registry_read".to_string(),
            record_id: Some("registry".to_string()),
            payload: serde_json::json!({
                "target_keys": ["uitest-script"],
                "include_script_target": "uitest-script",
            }),
            client_context: serde_json::json!({}),
            origin: CommandOrigin::TrustedLocal,
        };
        let result = outbound_handle_research_source_registry_read(root, &command)?;
        assert_eq!(
            result.pointer("/script/available").and_then(Value::as_bool),
            Some(true),
            "{result:#}"
        );
        assert!(result
            .pointer("/script/text")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains("UITEST_SCRIPT_MARKER")));
        assert_eq!(
            result
                .pointer("/script/revision_no")
                .and_then(Value::as_i64),
            Some(1)
        );
        // Without the flag the answer stays metadata only.
        let plain = outbound_handle_research_source_registry_read(
            root,
            &BusinessCommand {
                payload: serde_json::json!({ "target_keys": ["uitest-script"] }),
                ..command
            },
        )?;
        assert!(plain.get("script").is_some_and(Value::is_null));
        Ok(())
    }

    #[test]
    fn a_campaign_search_returns_names_and_an_import_only_the_asked_columns() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let root = temp.path();
        drop(super::super::store::open_store(root)?);
        super::super::person_research_gap_closure::seed_rxdb_collection_table_for_tests(
            root,
            "sellify_campaigns",
        )?;
        for (id, name, contact) in [
            ("c1", "Beauty - Welle 5 – 10.09.2026", 3181),
            ("c2", "Beauty - Welle 5 – 10.09.2026", 3182),
            ("c3", "Beauty - 2023 - Welle 1 - 07.06.2023", 3183),
            ("c4", "Chemie D – 01.07.2026", 3184),
        ] {
            super::super::store::upsert_rxdb_collection_record(
                root,
                "sellify_campaigns",
                id,
                1,
                serde_json::json!({"id": id, "name": name, "contact_id": contact,
                    "company_id": format!("sellify-company-{contact}"), "note_text": "x".repeat(500)}),
            )?;
        }
        let search = outbound_sellify_lookup(
            root,
            &serde_json::json!({"entity": "campaign", "group_by": "name", "limit": 50000,
                "fuzzy_selectors": [{"field": "name", "value": "Beauty"}]}),
        )?;
        assert_eq!(
            search["groups"],
            serde_json::json!([
                {"name": "Beauty - Welle 5 – 10.09.2026", "count": 2},
                {"name": "Beauty - 2023 - Welle 1 - 07.06.2023", "count": 1}
            ])
        );
        assert_eq!(search["records"], serde_json::json!([]));
        let import = outbound_sellify_lookup(
            root,
            &serde_json::json!({"entity": "campaign", "limit": 2000,
                "fields": ["contact_id", "company_id", "name"],
                "selectors": [{"field": "name", "value": "Beauty - Welle 5 – 10.09.2026"}]}),
        )?;
        let rows = import["records"].as_array().context("records")?;
        assert_eq!(rows.len(), 2);
        assert!(rows
            .iter()
            .all(|row| row.get("note_text").is_none() && row["contact_id"].is_number()));
        Ok(())
    }

    #[test]
    fn outbound_adapter_reconciliation_projects_typed_result_without_secrets() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        let now = 1_000;
        let source = serde_json::json!({
            "id": "example.com",
            "label": "Example",
            "url": "https://example.com/",
            "countries": ["DE"],
            "field_keys": ["firma_name"],
            "enabled": true,
            "requires_credential": false,
            "credential_secret_name": "",
            "target_key": "example-com",
            "adapter_id": "adapter_leadgen_example-com",
            "adapter_status": "reconciliation_queued",
            "scrape_status": "generation_queued",
            "auth_status": "not_required",
            "payload": { "builtin": false, "secret_value_in_payload": false },
            "created_at_ms": now,
            "updated_at_ms": now
        });
        upsert_business_record(
            &conn,
            "outbound_lead_generation_sources",
            "example.com",
            now,
            source.clone(),
        )?;
        upsert_rxdb_collection_record(
            root,
            "outbound_lead_generation_sources",
            "example.com",
            now,
            source.clone(),
        )?;
        let policy = serde_json::json!({
            "id": "research_policy",
            "title": "Research",
            "version_number": 2,
            "status": "active",
            "skill_name": "outbound-lead-generation-research",
            "skill_version": "1.0.0",
            "min_independent_sources": 2,
            "rules": [],
            "instructions": "Find legal name",
            "created_at_ms": now,
            "updated_at_ms": now
        });
        upsert_rxdb_collection_record(
            root,
            "outbound_lead_generation_research_policies",
            "research_policy",
            now,
            policy,
        )?;
        let command = BusinessCommand {
            id: Some("cmd_reconcile".to_string()),
            module: "outbound-lead-generation".to_string(),
            command_type: "outbound.research.adapters.reconcile".to_string(),
            record_id: Some("research_policy".to_string()),
            payload: serde_json::json!({
                "configuration_digest": "sha256:test",
                "policy_id": "research_policy",
                "field_keys": ["firma_name"],
                "sources": [source],
                "writeback_contract": {
                    "source_collection": "outbound_lead_generation_sources",
                    "adapter_collection": "outbound_lead_generation_adapters",
                    "policy_collection": "outbound_lead_generation_research_policies"
                }
            }),
            client_context: serde_json::json!({
                "source_module": "outbound-lead-generation"
            }),
            origin: CommandOrigin::TrustedLocal,
        };
        let reply = serde_json::json!({
            "schema": "ctox.outbound.adapter_reconciliation.v1",
            "configuration_digest": "sha256:test",
            "status": "completed",
            "discovered_sources": [],
            "adapters": [{
                "source_id": "example.com",
                "target_key": "example-com",
                "status": "ready",
                "scrape_status": "test_passed",
                "auth_status": "not_required",
                "adapter_revision": "sha256:adapter",
                "script_path": "runtime/scraping/targets/example-com/scripts/capture.js",
                "last_error": "",
                "test": { "ok": true, "records_found": 1 }
            }]
        })
        .to_string();
        let outcome = apply_outbound_adapter_reconciliation_reply(
            root,
            &conn,
            "cmd_reconcile",
            &command,
            "task_reconcile",
            &reply,
        )?;
        assert_eq!(outcome.get("ok").and_then(Value::as_bool), Some(true));
        let adapter = load_rxdb_collection_record(
            root,
            "outbound_lead_generation_adapters",
            "adapter_leadgen_example-com",
        )?
        .context("adapter writeback")?;
        assert_eq!(adapter.get("status").and_then(Value::as_str), Some("ready"));
        assert_eq!(
            adapter
                .pointer("/payload/test/records_found")
                .and_then(Value::as_i64),
            Some(1)
        );
        let policy = load_rxdb_collection_record(
            root,
            "outbound_lead_generation_research_policies",
            "research_policy",
        )?
        .context("policy writeback")?;
        assert_eq!(
            policy.get("reconciliation_status").and_then(Value::as_str),
            Some("completed")
        );
        assert!(!serde_json::to_string(&outcome)?.contains("password"));
        Ok(())
    }

    #[test]
    fn outbound_adapter_reconciliation_rejects_invalid_batch_before_any_write() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        let now = 1_000;
        let source = |id: &str| {
            let target_key = id.replace('.', "-");
            serde_json::json!({
                "id": id,
                "label": id,
                "url": format!("https://{id}/"),
                "countries": ["DE"],
                "field_keys": ["firma_name"],
                "enabled": true,
                "requires_credential": false,
                "credential_secret_name": "",
                "target_key": target_key,
                "adapter_id": format!("adapter_leadgen_{target_key}"),
                "adapter_status": "reconciliation_queued",
                "scrape_status": "generation_queued",
                "auth_status": "not_required",
                "payload": { "builtin": false, "secret_value_in_payload": false },
                "created_at_ms": now,
                "updated_at_ms": now
            })
        };
        let first = source("first.example");
        let second = source("second.example");
        for record in [&first, &second] {
            let id = record["id"].as_str().unwrap();
            upsert_rxdb_collection_record(
                root,
                "outbound_lead_generation_sources",
                id,
                now,
                record.clone(),
            )?;
        }
        let command = BusinessCommand {
            id: Some("cmd_atomic".to_string()),
            module: "outbound-lead-generation".to_string(),
            command_type: "outbound.research.adapters.reconcile".to_string(),
            record_id: None,
            payload: serde_json::json!({
                "configuration_digest": "sha256:atomic",
                "field_keys": ["firma_name"],
                "sources": [first, second],
                "writeback_contract": {
                    "source_collection": "outbound_lead_generation_sources",
                    "adapter_collection": "outbound_lead_generation_adapters",
                    "policy_collection": "outbound_lead_generation_research_policies"
                }
            }),
            client_context: serde_json::json!({"source_module": "outbound-lead-generation"}),
            origin: CommandOrigin::TrustedLocal,
        };
        let reply = serde_json::json!({
            "schema": "ctox.outbound.adapter_reconciliation.v1",
            "configuration_digest": "sha256:atomic",
            "status": "completed",
            "discovered_sources": [],
            "adapters": [
                {
                    "source_id": "first.example",
                    "target_key": "first-example",
                    "status": "ready",
                    "scrape_status": "test_passed",
                    "adapter_revision": "sha256:first",
                    "script_path": "runtime/scraping/targets/first-example/scripts/capture.js",
                    "test": {"ok": true, "records_found": 1}
                },
                {
                    "source_id": "second.example",
                    "target_key": "second-example",
                    "status": "ready",
                    "scrape_status": "test_passed",
                    "adapter_revision": "sha256:second",
                    "script_path": "../outside.js",
                    "test": {"ok": true, "records_found": 1}
                }
            ]
        })
        .to_string();

        assert!(apply_outbound_adapter_reconciliation_reply(
            root,
            &conn,
            "cmd_atomic",
            &command,
            "task_atomic",
            &reply,
        )
        .is_err());
        assert!(load_rxdb_collection_record(
            root,
            "outbound_lead_generation_adapters",
            "adapter_leadgen_first-example",
        )?
        .is_none());
        let unchanged =
            load_rxdb_collection_record(root, "outbound_lead_generation_sources", "first.example")?
                .context("first source")?;
        assert_eq!(
            unchanged.get("adapter_status").and_then(Value::as_str),
            Some("reconciliation_queued")
        );
        Ok(())
    }

    #[test]
    fn outbound_approval_decisions_write_business_event_audit() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        seed_business_user(root, "ops_admin", "admin")?;
        let actor = serde_json::json!({
            "actor": {
                "id": "ops_admin",
                "display_name": "Ops Admin"
            }
        });
        let command = |id: &str, command_type: &str, record_id: &str, payload: Value| {
            serde_json::json!({
                "id": id,
                "command_id": id,
                "module": "outbound",
                "command_type": command_type,
                "record_id": record_id,
                "status": "pending_sync",
                "payload": payload,
                "client_context": actor.clone()
            })
        };
        let prepare_message = |suffix: &str| -> anyhow::Result<()> {
            let message_id = format!("msg_{suffix}");
            accept_rxdb_business_command(
                root,
                command(
                    &format!("cmd_prepare_{suffix}"),
                    "outbound.message.prepare",
                    &message_id,
                    serde_json::json!({
                        "engagement_id": format!("eng_{suffix}"),
                        "campaign_id": "camp_audit",
                        "sender_account_id": "sender@example.com",
                        "recipient_email": format!("{suffix}@example.com"),
                        "subject": "Intro",
                        "body_text": "Hello"
                    }),
                ),
            )?;
            accept_rxdb_business_command(
                root,
                command(
                    &format!("cmd_request_{suffix}"),
                    "outbound.message.request_approval",
                    &message_id,
                    serde_json::json!({
                        "message_id": message_id
                    }),
                ),
            )?;
            Ok(())
        };

        prepare_message("approved")?;
        prepare_message("rejected")?;
        prepare_message("changes")?;

        accept_rxdb_business_command(
            root,
            command(
                "cmd_approve_audit",
                "outbound.message.approve",
                "msg_approved",
                serde_json::json!({
                    "message_id": "msg_approved",
                    "approval_id": "approval_audit_approved"
                }),
            ),
        )?;
        accept_rxdb_business_command(
            root,
            command(
                "cmd_reject_audit",
                "outbound.message.reject",
                "msg_rejected",
                serde_json::json!({
                    "message_id": "msg_rejected",
                    "approval_id": "approval_audit_rejected"
                }),
            ),
        )?;
        accept_rxdb_business_command(
            root,
            command(
                "cmd_changes_audit",
                "outbound.message.request_changes",
                "msg_changes",
                serde_json::json!({
                    "message_id": "msg_changes",
                    "approval_id": "approval_audit_changes"
                }),
            ),
        )?;

        let conn = open_store(root)?;
        let approved_payloads = business_event_payloads(
            &conn,
            "outbound_approvals",
            "approval_audit_approved",
            "business_os.external_approval.decided",
        )?;
        assert_eq!(approved_payloads.len(), 1);
        let approved = &approved_payloads[0];
        assert_eq!(
            approved.pointer("/event_type").and_then(Value::as_str),
            Some("business_os.external_approval.decided")
        );
        assert_eq!(
            approved.pointer("/decision").and_then(Value::as_str),
            Some("approved")
        );
        assert_eq!(
            approved.pointer("/actor/id").and_then(Value::as_str),
            Some("ops_admin")
        );
        assert_eq!(
            approved.pointer("/message/id").and_then(Value::as_str),
            Some("msg_approved")
        );
        assert_eq!(
            approved
                .pointer("/message/approval_status")
                .and_then(Value::as_str),
            Some("approved")
        );
        assert!(
            approved.pointer("/message/body_text").is_none(),
            "approval activity must not duplicate message body text"
        );
        let rejected_payloads = business_event_payloads(
            &conn,
            "outbound_approvals",
            "approval_audit_rejected",
            "business_os.external_approval.decided",
        )?;
        assert_eq!(
            rejected_payloads[0]
                .pointer("/decision")
                .and_then(Value::as_str),
            Some("rejected")
        );
        let changes_payloads = business_event_payloads(
            &conn,
            "outbound_approvals",
            "approval_audit_changes",
            "business_os.external_approval.decided",
        )?;
        assert_eq!(
            changes_payloads[0]
                .pointer("/decision")
                .and_then(Value::as_str),
            Some("changes_requested")
        );
        drop(conn);

        let activity = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_activity_list_approvals",
                "command_id": "cmd_activity_list_approvals",
                "module": "ctox",
                "command_type": "ctox.business_os.audit.list",
                "record_id": "business-activity",
                "status": "pending_sync",
                "payload": {
                    "limit": 10
                },
                "client_context": actor
            }),
        )?;
        let events = activity
            .pointer("/result/events")
            .and_then(Value::as_array)
            .context("expected activity events")?;
        assert!(events.iter().any(|event| {
            event.get("type").and_then(Value::as_str)
                == Some("business_os.external_approval.decided")
                && event.pointer("/payload/decision").and_then(Value::as_str) == Some("approved")
        }));

        Ok(())
    }

    #[test]
    fn outbound_write_outreach_draft_persists_messages_into_pipeline_contact() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        let conn = open_store(root)?;
        upsert_business_record(
            &conn,
            "outbound_pipeline_items",
            "pipe_one",
            1,
            serde_json::json!({
                "id": "pipe_one",
                "campaign_id": "camp",
                "company_id": "company_one",
                "company_name": "Beispiel GmbH",
                "stage": "contact_research",
                "contacts": [
                    { "name": "Erika Muster", "email": "erika@example.test" }
                ],
                "updated_at_ms": 1
            }),
        )?;
        drop(conn);

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_outreach_writeback",
                "command_id": "cmd_outreach_writeback",
                "module": "outbound",
                "command_type": "outbound.pipeline.write_outreach_draft",
                "record_id": "pipe_one",
                "status": "pending_sync",
                "payload": {
                    "pipeline_id": "pipe_one",
                    "contact_index": 0,
                    "messages": {
                        "message_mail_subject": "Kurzer Austausch zu Beispiel GmbH",
                        "message_mail_body": "Hallo Erika, ...",
                        "message_followup_1": "Kurzer Nachtrag ...",
                        "message_followup_2": "Letztes Follow-up ..."
                    }
                },
                "client_context": actor
            }),
        )?;

        let conn = open_store(root)?;
        let item = outbound_load_required(
            &conn,
            "outbound_pipeline_items",
            "pipe_one",
            "pipeline item",
        )?;
        let contact = item
            .pointer("/contacts/0")
            .cloned()
            .expect("contact present");
        assert_eq!(
            outbound_string(&contact, &["messages", "message_mail_subject"]).as_deref(),
            Some("Kurzer Austausch zu Beispiel GmbH")
        );
        assert_eq!(
            outbound_string(&contact, &["messages", "message_followup_2"]).as_deref(),
            Some("Letztes Follow-up ...")
        );
        assert_eq!(
            contact
                .pointer("/outreach_generating")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            outbound_string(&contact, &["outreach_status"]).as_deref(),
            Some("drafted")
        );
        Ok(())
    }

    #[test]
    fn outbound_research_adapter_records_credential_reference_without_secret_value(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        crate::secrets::set_credential(root, "LEADFEEDER_API_KEY", "DO_NOT_LEAK_LEADFEEDER")?;
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        let outcome = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_outbound_adapter_credential_ref",
                "command_id": "cmd_outbound_adapter_credential_ref",
                "module": "outbound",
                "command_type": "outbound.research_source.upsert",
                "record_id": "adapter_leadfeeder",
                "status": "pending_sync",
                "payload": {
                    "adapter_id": "adapter_leadfeeder",
                    "campaign_id": "camp",
                    "source_id": "leadfeeder.com",
                    "adapter": {
                        "id": "adapter_leadfeeder",
                        "campaign_id": "camp",
                        "source_id": "leadfeeder.com",
                        "label": "Leadfeeder",
                        "url": "https://www.leadfeeder.com/",
                        "adapter_kind": "api",
                        "requires_credential": true,
                        "credential_secret_name": "LEADFEEDER_API_KEY",
                        "auth_mode": "api_key"
                    },
                    "secret_value_in_payload": false
                },
                "client_context": actor
            }),
        )?;

        assert_eq!(
            outcome.get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            outcome
                .pointer("/result/adapter/auth_status")
                .and_then(Value::as_str),
            Some("credential_available")
        );
        assert_eq!(
            outcome
                .pointer("/result/adapter/payload/credential_ref/exists")
                .and_then(Value::as_bool),
            Some(true)
        );
        let serialized = serde_json::to_string(&outcome)?;
        assert!(
            !serialized.contains("DO_NOT_LEAK_LEADFEEDER"),
            "outbound adapter outcome leaked a credential value"
        );

        let conn = open_store(root)?;
        let adapter = outbound_load_required(
            &conn,
            "outbound_research_adapters",
            "adapter_leadfeeder",
            "adapter",
        )?;
        assert_eq!(
            outbound_string(&adapter, &["auth_status"]).as_deref(),
            Some("credential_available")
        );
        assert_eq!(
            adapter
                .pointer("/payload/credential_ref/secret_value_in_payload")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert!(
            !serde_json::to_string(&adapter)?.contains("DO_NOT_LEAK_LEADFEEDER"),
            "persisted outbound adapter leaked a credential value"
        );
        Ok(())
    }

    #[test]
    fn outbound_research_adapter_auth_assist_queues_browser_session_with_explicit_secret(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        crate::secrets::set_credential(
            root,
            "LEADFEEDER_BROWSER_LOGIN",
            r#"{"username":"researcher@example.test","password":"DO_NOT_LEAK_BROWSER_PASSWORD"}"#,
        )?;
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" },
            "source_module": "outbound"
        });

        let outcome = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_outbound_adapter_auth_assist",
                "command_id": "cmd_outbound_adapter_auth_assist",
                "module": "outbound",
                "command_type": "outbound.research_source.auth_assist",
                "record_id": "adapter_leadfeeder_browser",
                "status": "pending_sync",
                "payload": {
                    "adapter_id": "adapter_leadfeeder_browser",
                    "campaign_id": "camp",
                    "source_id": "leadfeeder.com",
                    "adapter": {
                        "id": "adapter_leadfeeder_browser",
                        "campaign_id": "camp",
                        "source_id": "leadfeeder.com",
                        "label": "Leadfeeder",
                        "url": "https://app.leadfeeder.com/dashboard",
                        "adapter_kind": "scrape_target",
                        "requires_credential": true,
                        "credential_secret_name": "LEADFEEDER_BROWSER_LOGIN",
                        "auth_mode": "browser_session"
                    },
                    "writeback": {
                        "collection": "outbound_adapters",
                        "record_id": "adapter_leadfeeder_browser"
                    },
                    "record_snapshot": {
                        "auth_assist": {
                            "session_id": "browser_session_web_stack_auth_leadfeeder_com_existing",
                            "tab_id": "browser_tab_browser_session_web_stack_auth_leadfeeder_com_existing"
                        }
                    },
                    "secret_value_in_payload": false
                },
                "client_context": actor
            }),
        )?;

        assert_eq!(
            outcome
                .pointer("/result/adapter/auth_status")
                .and_then(Value::as_str),
            Some("browser_session_requested")
        );
        assert_eq!(
            outcome
                .pointer("/result/auth_assist/required_secret_name")
                .and_then(Value::as_str),
            Some("LEADFEEDER_BROWSER_LOGIN")
        );
        assert_eq!(
            outcome
                .pointer("/result/auth_assist/status")
                .and_then(Value::as_str),
            Some("accepted")
        );
        assert_eq!(
            outcome
                .pointer("/result/auth_assist/session_id")
                .and_then(Value::as_str),
            Some("browser_session_web_stack_auth_leadfeeder_com_existing")
        );
        assert_eq!(
            outcome
                .pointer("/result/auth_assist/tab_id")
                .and_then(Value::as_str),
            Some("browser_tab_browser_session_web_stack_auth_leadfeeder_com_existing")
        );
        let queued_command_id = outcome
            .pointer("/result/auth_assist/command_id")
            .and_then(Value::as_str)
            .context("auth assist command id")?;
        let queued = load_rxdb_collection_record(root, "business_commands", queued_command_id)?
            .context("auth assist command queued")?;
        assert_eq!(
            queued.get("command_type").and_then(Value::as_str),
            Some("web_stack.auth_assist.request")
        );
        assert_eq!(
            queued.get("status").and_then(Value::as_str),
            Some("accepted")
        );
        assert!(
            outcome
                .pointer("/result/auth_assist/task_id")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty()),
            "auth assist must become a durable CTOX task"
        );
        let conn = open_store(root)?;
        let private_projection =
            load_business_record_payload(&conn, "outbound_adapters", "adapter_leadfeeder_browser")?
                .context("private adapter writeback")?;
        assert_eq!(
            private_projection.get("status").and_then(Value::as_str),
            Some("auth_requested")
        );
        assert_eq!(
            private_projection
                .get("auth_status")
                .and_then(Value::as_str),
            Some("browser_session_requested")
        );
        assert_eq!(
            private_projection
                .get("scrape_status")
                .and_then(Value::as_str),
            Some("target_available")
        );
        assert_eq!(
            private_projection.get("last_error").and_then(Value::as_str),
            Some("")
        );
        assert_eq!(
            queued
                .pointer("/payload/secret_name")
                .and_then(Value::as_str),
            Some("LEADFEEDER_BROWSER_LOGIN")
        );
        let serialized = serde_json::to_string(&(outcome, queued))?;
        assert!(
            !serialized.contains("DO_NOT_LEAK_BROWSER_PASSWORD"),
            "browser auth assist leaked a credential value"
        );
        Ok(())
    }

    #[test]
    fn outbound_research_adapter_writeback_reaches_tenant_rxdb_collection() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let adapter_id = "adapter_outbound_example-com";
        let rxdb_path = super::super::store::rxdb_store_path(root);
        std::fs::create_dir_all(rxdb_path.parent().context("RxDB parent")?)?;
        let rxdb = Connection::open(rxdb_path)?;
        rxdb.execute_batch(
            "CREATE TABLE ctox_business_os__outbound_lead_generation_adapters__v1 (
                id TEXT PRIMARY KEY NOT NULL,
                revision TEXT,
                deleted INTEGER NOT NULL,
                lastWriteTime REAL NOT NULL,
                data TEXT NOT NULL
            );",
        )?;
        drop(rxdb);
        let outcome = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_outbound_adapter_writeback",
                "command_id": "cmd_outbound_adapter_writeback",
                "module": "outbound",
                "command_type": "outbound.research_source.upsert",
                "record_id": adapter_id,
                "status": "pending_sync",
                "payload": {
                    "adapter_id": adapter_id,
                    "source_id": "example.com",
                    "adapter": {
                        "id": adapter_id,
                        "source_id": "example.com",
                        "label": "Example",
                        "url": "https://example.com/",
                        "adapter_kind": "scrape_target",
                        "target_key": "example-com",
                        "countries": ["DE"],
                        "field_keys": ["firma_name"],
                        "enabled": true,
                        "requires_credential": false,
                        "auth_mode": "none",
                        "auth_status": "not_required"
                    },
                    "writeback": {
                        "collection": "outbound_lead_generation_adapters",
                        "record_id": adapter_id
                    },
                    "secret_value_in_payload": false
                },
                "client_context": {
                    "actor": { "id": "tester", "role": "admin", "display_name": "Tester" },
                    "source_module": "outbound-lead-generation"
                }
            }),
        )?;

        assert_eq!(
            outcome.get("status").and_then(Value::as_str),
            Some("completed")
        );
        let projected =
            load_rxdb_collection_record(root, "outbound_lead_generation_adapters", adapter_id)?
                .context("tenant adapter writeback must be projected into RxDB")?;
        assert_eq!(
            projected.get("source_id").and_then(Value::as_str),
            Some("example.com")
        );
        assert_eq!(
            projected.get("status").and_then(Value::as_str),
            Some("active")
        );
        Ok(())
    }

    #[test]
    fn outbound_custom_research_adapter_queues_universal_scraping_generation() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        let outcome = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_outbound_adapter_generate_custom",
                "command_id": "cmd_outbound_adapter_generate_custom",
                "module": "outbound",
                "command_type": "outbound.research_source.generate_adapter",
                "record_id": "adapter_partner",
                "status": "pending_sync",
                "payload": {
                    "adapter_id": "adapter_partner",
                    "campaign_id": "camp",
                    "source_id": "research.partner.example",
                    "adapter": {
                        "id": "adapter_partner",
                        "campaign_id": "camp",
                        "source_id": "research.partner.example",
                        "label": "Partner Research",
                        "url": "https://research.partner.example/",
                        "adapter_kind": "custom_url",
                        "target_key": "research-partner-example",
                        "requires_credential": false
                    },
                    "scrape_contract": {
                        "skill": "universal-scraping",
                        "target_key": "research-partner-example",
                        "output_schema": "prospect.v1"
                    },
                    "secret_value_in_payload": false
                },
                "client_context": actor
            }),
        )?;

        assert_eq!(
            outcome.get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            outcome
                .pointer("/result/adapter/status")
                .and_then(Value::as_str),
            Some("generation_queued")
        );
        assert_eq!(
            outcome
                .pointer("/result/adapter/scrape_status")
                .and_then(Value::as_str),
            Some("generation_queued")
        );
        let task_id = outcome
            .pointer("/result/adapter/payload/scrape_registry_effect/generation_task/task_id")
            .and_then(Value::as_str)
            .context("generation task id")?;
        let task = channels::load_queue_task(root, task_id)?.context("generation task exists")?;
        assert_eq!(task.suggested_skill.as_deref(), Some("universal-scraping"));
        assert!(task.prompt.contains("research-partner-example"));
        assert!(task.prompt.contains("https://research.partner.example/"));
        let queue_conn = channels::open_channel_db(&crate::paths::core_db(root))?;
        let generation_idempotency_key: String = queue_conn.query_row(
            "SELECT json_extract(metadata_json, '$.idempotency_key')
             FROM communication_messages WHERE message_key = ?1",
            params![task_id],
            |row| row.get(0),
        )?;
        assert_eq!(
            generation_idempotency_key,
            "outbound-research-adapter:research-partner-example:cmd_outbound_adapter_generate_custom"
        );
        let generated_manifest =
            root.join("runtime/scraping/outbound-adapters/research-partner-example/target.json");
        assert!(generated_manifest.is_file());
        let generated_manifest: Value = serde_json::from_slice(&fs::read(generated_manifest)?)?;
        assert_eq!(
            generated_manifest
                .pointer("/output_schema/schema_key")
                .and_then(Value::as_str),
            Some("prospect.v1")
        );
        assert!(
            !serde_json::to_string(&outcome)?.contains("credential_value"),
            "generation command must not introduce credential values"
        );
        Ok(())
    }

    #[test]
    fn outbound_bundled_scrape_target_prefers_specialized_script_and_supports_shared_fallback(
    ) -> anyhow::Result<()> {
        let temp = tempdir()?;
        let base = temp.path().join("src/tools/web-stack/scrape-targets");
        let shared_script = base.join("_shared/generic-prospect-v1.js");
        let fallback_dir = base.join("google.de");
        let specialized_dir = base.join("northdata.de");
        let specialized_script = specialized_dir.join("scripts/v1.js");
        fs::create_dir_all(shared_script.parent().context("shared script parent")?)?;
        fs::create_dir_all(
            specialized_script
                .parent()
                .context("specialized script parent")?,
        )?;
        fs::create_dir_all(&fallback_dir)?;
        fs::write(&shared_script, "console.log('{}');")?;
        fs::write(&specialized_script, "console.log('{}');")?;
        fs::write(fallback_dir.join("target.json"), "{}")?;
        fs::write(specialized_dir.join("target.json"), "{}")?;

        let (resolved_dir, resolved_script) = outbound_find_bundled_scrape_target_dir(
            temp.path(),
            "google.de",
            "google-de",
            Some("https://www.google.de/"),
        )
        .context("shared scrape target")?;
        assert_eq!(resolved_dir, fallback_dir);
        assert_eq!(resolved_script, shared_script);

        let (resolved_dir, resolved_script) = outbound_find_bundled_scrape_target_dir(
            temp.path(),
            "northdata.de",
            "northdata-de",
            Some("https://www.northdata.de/"),
        )
        .context("specialized scrape target")?;
        assert_eq!(resolved_dir, specialized_dir);
        assert_eq!(resolved_script, specialized_script);
        Ok(())
    }

    fn write_outbound_scrape_test_fixture(root: &Path, script_body: &str) -> anyhow::Result<()> {
        let target_dir = root.join("src/tools/web-stack/scrape-targets/fixture.example");
        let script_path = target_dir.join("scripts/v1.js");
        fs::create_dir_all(script_path.parent().context("fixture script parent")?)?;
        fs::write(
            target_dir.join("target.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "target_key": "fixture-example",
                "display_name": "Fixture Research",
                "start_url": "https://fixture.example/",
                "target_kind": "prospect-research",
                "status": "active",
                "config": {
                    "skip_probe": true,
                    "expected_min_records": 0,
                    "record_key_fields": ["field", "source_url"]
                },
                "output_schema": {
                    "schema_key": "prospect.v1",
                    "record_key_fields": ["field", "source_url"]
                }
            }))?,
        )?;
        fs::write(&script_path, script_body)?;
        Ok(())
    }

    fn run_outbound_scrape_test_fixture(script_body: &str) -> anyhow::Result<(Value, Value)> {
        let temp = tempdir()?;
        let root = temp.path();
        write_outbound_scrape_test_fixture(root, script_body)?;

        let adapter_payload = serde_json::json!({
            "source_id": "fixture.example",
            "label": "Fixture Research",
            "url": "https://fixture.example/",
            "adapter_kind": "scrape_target",
            "target_key": "fixture-example",
            "field_keys": ["company_name"],
            "countries": ["DE"]
        });
        let command = BusinessCommand {
            id: Some("cmd_fixture_scrape_test".to_string()),
            module: "outbound".to_string(),
            command_type: "outbound.research_source.test".to_string(),
            record_id: Some("adapter_fixture".to_string()),
            payload: serde_json::json!({
                "test_input": {"company": "Fixture GmbH", "country": "DE"}
            }),
            client_context: serde_json::json!({}),
            origin: CommandOrigin::TrustedLocal,
        };
        let mut record = serde_json::json!({
            "id": "adapter_fixture",
            "source_id": "fixture.example",
            "url": "https://fixture.example/",
            "adapter_kind": "scrape_target",
            "target_key": "fixture-example",
            "field_keys": ["company_name"],
            "payload": {}
        });
        let effect = outbound_apply_research_adapter_scrape_effect(
            root,
            &command,
            &adapter_payload,
            "adapter_fixture",
            "fixture.example",
            &mut record,
        )
        .context("fixture scrape effect")?;
        Ok((record, effect))
    }

    #[test]
    fn outbound_scrape_test_requires_records_fields_and_durable_evidence() {
        assert!(!outbound_scrape_test_passed(
            scrape::ScrapeRunStatus::Succeeded,
            1,
            true,
            false,
        ));
        assert!(!outbound_scrape_test_passed(
            scrape::ScrapeRunStatus::Succeeded,
            0,
            true,
            true,
        ));
        assert!(!outbound_scrape_test_passed(
            scrape::ScrapeRunStatus::PortalDrift,
            1,
            true,
            true,
        ));
        assert!(outbound_scrape_test_passed(
            scrape::ScrapeRunStatus::Succeeded,
            1,
            true,
            true,
        ));
    }

    #[test]
    fn outbound_completed_test_command_persists_negative_scrape_evidence() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        write_outbound_scrape_test_fixture(
            root,
            r#"process.stdout.write(JSON.stringify({records: [], failure_mode: "portal_drift", detail: "selector changed"}));"#,
        )?;
        let outcome = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_outbound_adapter_test_drift",
                "command_id": "cmd_outbound_adapter_test_drift",
                "module": "outbound",
                "command_type": "outbound.research_source.test",
                "record_id": "adapter_fixture",
                "status": "pending_sync",
                "payload": {
                    "adapter_id": "adapter_fixture",
                    "campaign_id": "camp",
                    "source_id": "fixture.example",
                    "adapter": {
                        "id": "adapter_fixture",
                        "campaign_id": "camp",
                        "source_id": "fixture.example",
                        "label": "Fixture Research",
                        "url": "https://fixture.example/",
                        "adapter_kind": "scrape_target",
                        "target_key": "fixture-example",
                        "field_keys": ["company_name"],
                        "countries": ["DE"]
                    },
                    "test_input": {"company": "Fixture GmbH", "country": "DE"}
                },
                "client_context": {
                    "actor": {"id": "tester", "role": "admin", "display_name": "Tester"}
                }
            }),
        )?;
        assert_eq!(
            outcome.get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            outcome
                .pointer("/result/adapter/status")
                .and_then(Value::as_str),
            Some("test_portal_drift")
        );

        let conn = open_store(root)?;
        let adapter = outbound_load_required(
            &conn,
            "outbound_research_adapters",
            "adapter_fixture",
            "adapter",
        )?;
        assert_eq!(
            adapter.get("status").and_then(Value::as_str),
            Some("test_portal_drift")
        );
        assert_eq!(adapter.get("test_ok").and_then(Value::as_bool), Some(false));
        assert_eq!(
            adapter.pointer("/last_test/status").and_then(Value::as_str),
            Some("portal_drift")
        );
        assert_eq!(
            adapter
                .pointer("/payload/test_evidence/valid")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert!(adapter
            .get("last_error")
            .and_then(Value::as_str)
            .is_some_and(|error| error.contains("selector changed")));
        Ok(())
    }

    #[test]
    fn outbound_scrape_test_blocked_never_turns_green() -> anyhow::Result<()> {
        let (record, effect) = run_outbound_scrape_test_fixture(
            r#"process.stdout.write(JSON.stringify({records: [], failure_mode: "blocked", detail: "login wall"}));"#,
        )?;
        assert_eq!(
            record.get("status").and_then(Value::as_str),
            Some("test_blocked")
        );
        assert_eq!(
            record.get("scrape_status").and_then(Value::as_str),
            Some("test_blocked")
        );
        assert_eq!(
            effect.pointer("/test/status").and_then(Value::as_str),
            Some("blocked")
        );
        assert_eq!(
            effect.pointer("/test/ok").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            effect.pointer("/test/test_ok").and_then(Value::as_bool),
            Some(false)
        );
        assert!(record
            .get("last_error")
            .and_then(Value::as_str)
            .is_some_and(|error| error.contains("status=blocked") && error.contains("login wall")));
        assert!(record
            .get("last_run_id")
            .and_then(Value::as_str)
            .is_some_and(|run_id| run_id.starts_with("scrape_run-")));
        assert_eq!(record.get("test_ok").and_then(Value::as_bool), Some(false));
        assert_eq!(
            record.pointer("/evidence/valid").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            record
                .pointer("/last_test/latency_ms")
                .and_then(Value::as_u64),
            effect.pointer("/test/latency_ms").and_then(Value::as_u64)
        );
        Ok(())
    }

    #[test]
    fn outbound_scrape_test_auth_required_is_distinct_and_never_green() -> anyhow::Result<()> {
        let (record, effect) = run_outbound_scrape_test_fixture(
            r#"process.stdout.write(JSON.stringify({records: [], failure_mode: "auth_required", detail: "login required"}));"#,
        )?;
        assert_eq!(
            record.get("status").and_then(Value::as_str),
            Some("test_auth_required")
        );
        assert_eq!(
            record.get("scrape_status").and_then(Value::as_str),
            Some("test_auth_required")
        );
        assert_eq!(
            record.get("auth_status").and_then(Value::as_str),
            Some("auth_required")
        );
        assert_eq!(
            effect.pointer("/test/test_ok").and_then(Value::as_bool),
            Some(false)
        );
        assert!(record
            .get("last_error")
            .and_then(Value::as_str)
            .is_some_and(|error| error.contains("login required")));
        assert_eq!(
            record
                .pointer("/payload/test_evidence/valid")
                .and_then(Value::as_bool),
            Some(true)
        );
        Ok(())
    }

    #[test]
    fn outbound_scrape_test_portal_drift_never_turns_green() -> anyhow::Result<()> {
        let (record, effect) = run_outbound_scrape_test_fixture(
            r#"process.stdout.write(JSON.stringify({records: [], failure_mode: "portal_drift", detail: "selector missing"}));"#,
        )?;
        assert_eq!(
            record.get("status").and_then(Value::as_str),
            Some("test_portal_drift")
        );
        assert_eq!(
            record.get("scrape_status").and_then(Value::as_str),
            Some("test_portal_drift")
        );
        assert_eq!(
            effect.pointer("/test/status").and_then(Value::as_str),
            Some("portal_drift")
        );
        assert_eq!(
            effect.pointer("/test/test_ok").and_then(Value::as_bool),
            Some(false)
        );
        assert!(record
            .pointer("/payload/scrape_test_diagnostics/error")
            .and_then(Value::as_str)
            .is_some_and(|error| error.contains("selector missing")));
        assert_eq!(
            record
                .pointer("/last_test/evidence/valid")
                .and_then(Value::as_bool),
            Some(true)
        );
        Ok(())
    }

    #[test]
    fn outbound_scrape_test_temporary_unreachable_never_turns_green() -> anyhow::Result<()> {
        let (record, effect) = run_outbound_scrape_test_fixture(
            r#"process.stdout.write(JSON.stringify({records: [], failure_mode: "temporary_unreachable", detail: "upstream timeout"}));"#,
        )?;
        assert_eq!(
            record.get("status").and_then(Value::as_str),
            Some("test_temporary_unreachable")
        );
        assert_eq!(
            record.get("scrape_status").and_then(Value::as_str),
            Some("test_temporary_unreachable")
        );
        assert_eq!(
            effect.pointer("/test/status").and_then(Value::as_str),
            Some("temporary_unreachable")
        );
        assert_eq!(
            effect.pointer("/test/test_ok").and_then(Value::as_bool),
            Some(false)
        );
        assert!(record
            .get("last_error")
            .and_then(Value::as_str)
            .is_some_and(|error| error.contains("upstream timeout")));
        assert!(record
            .get("latency_ms")
            .and_then(Value::as_i64)
            .is_some_and(|latency| latency >= 0));
        Ok(())
    }

    #[test]
    fn outbound_scrape_test_zero_records_has_its_own_non_green_state() -> anyhow::Result<()> {
        let (record, effect) = run_outbound_scrape_test_fixture(
            r#"process.stdout.write(JSON.stringify({records: []}));"#,
        )?;
        assert_eq!(
            effect.pointer("/test/status").and_then(Value::as_str),
            Some("portal_drift")
        );
        assert_eq!(
            effect
                .pointer("/test/records_found")
                .and_then(Value::as_i64),
            Some(0)
        );
        assert_eq!(
            record.get("status").and_then(Value::as_str),
            Some("test_zero_records")
        );
        assert_eq!(
            record.get("scrape_status").and_then(Value::as_str),
            Some("test_zero_records")
        );
        assert_eq!(record.get("test_ok").and_then(Value::as_bool), Some(false));
        assert!(record
            .get("last_error")
            .and_then(Value::as_str)
            .is_some_and(|error| error.contains("records_found=0")));
        Ok(())
    }

    #[test]
    fn outbound_scrape_test_command_error_is_not_reported_as_portal_drift() -> anyhow::Result<()> {
        let (record, effect) = run_outbound_scrape_test_fixture(
            r#"process.stderr.write("extractor crashed"); process.exit(7);"#,
        )?;
        assert_eq!(
            record.get("status").and_then(Value::as_str),
            Some("test_failed")
        );
        assert_eq!(
            record.get("scrape_status").and_then(Value::as_str),
            Some("test_error")
        );
        assert_eq!(
            effect.pointer("/test/test_ok").and_then(Value::as_bool),
            Some(false)
        );
        assert!(record
            .get("last_error")
            .and_then(Value::as_str)
            .is_some_and(|error| error.contains("extractor crashed")));
        assert_eq!(
            record.pointer("/evidence/valid").and_then(Value::as_bool),
            Some(false)
        );
        Ok(())
    }

    #[test]
    fn outbound_scrape_test_empty_expected_field_never_turns_green() -> anyhow::Result<()> {
        let (record, effect) = run_outbound_scrape_test_fixture(
            r#"process.stdout.write(JSON.stringify({records: [{field: "company_name", value: "", source_url: "https://fixture.example/"}]}));"#,
        )?;
        assert_eq!(
            effect.pointer("/test/status").and_then(Value::as_str),
            Some("succeeded")
        );
        assert_eq!(
            effect
                .pointer("/test/records_found")
                .and_then(Value::as_i64),
            Some(1)
        );
        assert_eq!(
            effect.pointer("/test/test_ok").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            record.get("status").and_then(Value::as_str),
            Some("test_fields_missing")
        );
        assert_eq!(
            record.get("scrape_status").and_then(Value::as_str),
            Some("test_fields_missing")
        );
        assert_eq!(
            effect
                .pointer("/test/missing_fields/0")
                .and_then(Value::as_str),
            Some("company_name")
        );
        Ok(())
    }

    #[test]
    fn outbound_scrape_test_succeeds_with_extracted_expected_fields() -> anyhow::Result<()> {
        let (record, effect) = run_outbound_scrape_test_fixture(
            r#"process.stdout.write(JSON.stringify({records: [{field: "company_name", value: "Fixture GmbH", confidence: 0.9, source_url: "https://fixture.example/company"}]}));"#,
        )?;
        assert_eq!(
            record.get("status").and_then(Value::as_str),
            Some("test_ok")
        );
        assert_eq!(
            record.get("scrape_status").and_then(Value::as_str),
            Some("test_executed")
        );
        assert_eq!(record.get("last_error").and_then(Value::as_str), Some(""));
        assert!(record
            .get("last_success_at_ms")
            .and_then(Value::as_i64)
            .is_some_and(|value| value > 0));
        assert_eq!(
            effect.pointer("/test/ok").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            effect.pointer("/test/test_ok").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            effect
                .pointer("/test/records_found")
                .and_then(Value::as_i64),
            Some(1)
        );
        assert_eq!(
            effect
                .pointer("/test/fields_extracted/0")
                .and_then(Value::as_str),
            Some("company_name")
        );
        assert!(effect
            .pointer("/test/latency_ms")
            .and_then(Value::as_u64)
            .is_some());
        assert_eq!(record.get("test_ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            record.pointer("/evidence/valid").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            record
                .pointer("/evidence/result_artifact/sha256_matches")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            record
                .pointer("/evidence/records_artifact/record_count")
                .and_then(Value::as_i64),
            Some(1)
        );
        assert!(record
            .pointer("/last_test/tested_at_ms")
            .and_then(Value::as_i64)
            .is_some_and(|value| value > 0));
        Ok(())
    }

    #[test]
    fn outbound_message_send_approved_requires_matching_current_revision() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_engagement",
                "command_id": "cmd_engagement",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_test",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_test",
                    "company_id": "company_test",
                    "contact_id": "contact_test"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_prepare",
                "command_id": "cmd_prepare",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_test",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_test",
                    "campaign_id": "camp_test",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Intro",
                    "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;

        let before_approval = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_before_approval",
                "command_id": "cmd_send_before_approval",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_test",
                "status": "pending_sync",
                "payload": { "message_id": "msg_test" },
                "client_context": actor.clone()
            }),
        )
        .expect_err("send must be blocked before approval");
        assert!(
            before_approval.to_string().contains("must be approved"),
            "{before_approval}"
        );

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_request_approval",
                "command_id": "cmd_request_approval",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_test",
                "status": "pending_sync",
                "payload": { "message_id": "msg_test" },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_approve",
                "command_id": "cmd_approve",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_test",
                "status": "pending_sync",
                "payload": { "message_id": "msg_test" },
                "client_context": actor.clone()
            }),
        )?;
        let send = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_after_approval",
                "command_id": "cmd_send_after_approval",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_test",
                "status": "pending_sync",
                "payload": { "message_id": "msg_test" },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            send.pointer("/result/provider_send_executed")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            send.pointer("/result/provider_dispatch_status")
                .and_then(Value::as_str),
            Some("queued_in_mailserver")
        );
        assert_eq!(
            send.pointer("/result/message/send_status")
                .and_then(Value::as_str),
            Some("queued_for_provider")
        );
        let provider_queue_id = send
            .pointer("/result/provider_queue_id")
            .and_then(Value::as_str)
            .context("provider_queue_id should be returned")?;
        let queue_conn = Connection::open(crate::paths::core_db(root))?;
        let queued_count: i64 = queue_conn.query_row(
            "SELECT COUNT(*) FROM stalwart_smtp_queue WHERE id = ?1 AND from_addr = 'sender@example.com' AND to_addr = 'lead@example.com' AND status = 'pending'",
            params![provider_queue_id],
            |row| row.get(0),
        )?;
        assert_eq!(queued_count, 1);
        drop(queue_conn);

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_prepare_second",
                "command_id": "cmd_prepare_second",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_revision",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_test",
                    "campaign_id": "camp_test",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Intro",
                    "body_text": "Old body"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_request_second",
                "command_id": "cmd_request_second",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_revision",
                "status": "pending_sync",
                "payload": { "message_id": "msg_revision" },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_approve_second",
                "command_id": "cmd_approve_second",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_revision",
                "status": "pending_sync",
                "payload": { "message_id": "msg_revision" },
                "client_context": actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let mut stale_message =
            outbound_load_required(&conn, "outbound_messages", "msg_revision", "message")?;
        outbound_put_string(&mut stale_message, "body_text", "Changed body");
        outbound_put_string(&mut stale_message, "approval_status", "approved");
        outbound_put_string(&mut stale_message, "send_status", "approved_not_sent");
        let changed_revision = outbound_message_revision(&stale_message);
        outbound_put_string(&mut stale_message, "revision_id", changed_revision);
        upsert_business_record(
            &conn,
            "outbound_messages",
            "msg_revision",
            now_ms() as i64,
            stale_message,
        )?;
        drop(conn);

        let changed_revision_send = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_changed_revision",
                "command_id": "cmd_send_changed_revision",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_revision",
                "status": "pending_sync",
                "payload": { "message_id": "msg_revision" },
                "client_context": actor.clone()
            }),
        )
        .expect_err("changed message body must invalidate approval");
        assert!(
            changed_revision_send
                .to_string()
                .contains("no matching approval for current revision"),
            "{changed_revision_send}"
        );

        Ok(())
    }

    #[test]
    fn outbound_send_approved_blocked_after_rejection_or_change_request() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        let prepare = |message_id: &str| {
            serde_json::json!({
                "id": format!("cmd_prepare_{message_id}"),
                "command_id": format!("cmd_prepare_{message_id}"),
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": message_id,
                "status": "pending_sync",
                "payload": {
                    "engagement_id": format!("eng_{message_id}"),
                    "campaign_id": "camp_test",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Intro",
                    "body_text": "Hello"
                },
                "client_context": actor.clone()
            })
        };
        let decide = |message_id: &str, command_type: &str| {
            serde_json::json!({
                "id": format!("cmd_{command_type}_{message_id}"),
                "command_id": format!("cmd_{command_type}_{message_id}"),
                "module": "outbound",
                "command_type": format!("outbound.message.{command_type}"),
                "record_id": message_id,
                "status": "pending_sync",
                "payload": { "message_id": message_id },
                "client_context": actor.clone()
            })
        };
        let send = |message_id: &str| {
            serde_json::json!({
                "id": format!("cmd_send_{message_id}"),
                "command_id": format!("cmd_send_{message_id}"),
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": message_id,
                "status": "pending_sync",
                "payload": { "message_id": message_id },
                "client_context": actor.clone()
            })
        };

        // Rejected message: request approval, reject, then sending must be blocked.
        accept_rxdb_business_command(root, prepare("msg_reject"))?;
        accept_rxdb_business_command(root, decide("msg_reject", "request_approval"))?;
        accept_rxdb_business_command(root, decide("msg_reject", "reject"))?;
        let rejected_send = accept_rxdb_business_command(root, send("msg_reject"))
            .expect_err("send must be blocked after rejection");
        assert!(
            rejected_send.to_string().contains("must be approved"),
            "{rejected_send}"
        );

        // Change-requested message: request approval, request changes, send blocked.
        accept_rxdb_business_command(root, prepare("msg_changes"))?;
        accept_rxdb_business_command(root, decide("msg_changes", "request_approval"))?;
        accept_rxdb_business_command(root, decide("msg_changes", "request_changes"))?;
        let changes_send = accept_rxdb_business_command(root, send("msg_changes"))
            .expect_err("send must be blocked after change request");
        assert!(
            changes_send.to_string().contains("must be approved"),
            "{changes_send}"
        );

        Ok(())
    }

    #[test]
    fn outbound_sequence_save_persists_strategy_policy_and_touchpoints() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        let saved = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_sequence_save",
                "command_id": "cmd_sequence_save",
                "module": "outbound",
                "command_type": "outbound.sequence.save",
                "record_id": "seq_campaign",
                "status": "pending_sync",
                "payload": {
                    "sequence_id": "seq_campaign",
                    "campaign_id": "camp_sequence",
                    "name": "CTOX Active Outbound",
                    "strategy_text": "Bereite jede Nachricht vor und warte auf Freigabe.",
                    "sequence_policy_text": "Initiale Nachricht, 5 Werktage warten, Follow-up 1.",
                    "send_window": { "text": "Werktags 09:00-16:00" },
                    "touchpoints": [
                        { "type": "initial", "wait_days_after_previous": 0, "requires_approval": true },
                        { "type": "followup_1", "wait_days_after_previous": 5, "requires_approval": true }
                    ],
                    "stop_rules": [
                        { "type": "hard_stop", "text": "Stoppe bei Antwort, Opt-out oder Termin." }
                    ],
                    "approval_policy": {
                        "require_all_messages": true,
                        "policy_text": "Jede ausgehende Nachricht braucht Freigabe."
                    },
                    "scheduling_policy": {
                        "strategy_text": "Bei Interesse Terminantwort vorbereiten.",
                        "duration_minutes": 30,
                        "slot_proposal_count": 3
                    },
                    "compliance_policy": {
                        "policy_text": "Keine Nachricht nach Opt-out.",
                        "suppression_policy_text": "Suppressions hart beachten."
                    },
                    "payload": {
                        "sender_account_id": "sender@example.com",
                        "skillbook_id": "business-os-outbound-active",
                        "runbook_id": "runbook-active-outbound"
                    }
                },
                "client_context": actor
            }),
        )?;

        assert_eq!(
            saved.pointer("/result/collection").and_then(Value::as_str),
            Some("outbound_sequences")
        );
        assert_eq!(
            saved
                .pointer("/result/sequence/strategy_text")
                .and_then(Value::as_str),
            Some("Bereite jede Nachricht vor und warte auf Freigabe.")
        );

        let conn = open_store(root)?;
        let sequence =
            outbound_load_required(&conn, "outbound_sequences", "seq_campaign", "sequence")?;
        assert_eq!(
            outbound_string(&sequence, &["campaign_id"]).as_deref(),
            Some("camp_sequence")
        );
        assert_eq!(
            sequence
                .pointer("/approval_policy/require_all_messages")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            sequence
                .pointer("/touchpoints/1/wait_days_after_previous")
                .and_then(Value::as_i64),
            Some(5)
        );
        assert_eq!(
            sequence
                .pointer("/stop_rules/0/type")
                .and_then(Value::as_str),
            Some("hard_stop")
        );
        assert_eq!(
            sequence
                .pointer("/scheduling_policy/duration_minutes")
                .and_then(Value::as_i64),
            Some(30)
        );
        assert_eq!(
            sequence
                .pointer("/compliance_policy/policy_text")
                .and_then(Value::as_str),
            Some("Keine Nachricht nach Opt-out.")
        );
        assert_eq!(
            sequence
                .pointer("/payload/skillbook_id")
                .and_then(Value::as_str),
            Some("business-os-outbound-active")
        );

        Ok(())
    }

    #[test]
    fn outbound_draft_prepare_creates_approval_gated_automated_messages() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_engagement_auto",
                "command_id": "cmd_engagement_auto",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_auto",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_auto",
                    "company_id": "company_auto",
                    "contact_id": "contact_auto",
                    "payload": {
                        "company_name": "ACME GmbH",
                        "contact_name": "Frau Beispiel",
                        "contact_email": "lead@example.com",
                        "strategy_text": "Kurz, belegt und respektvoll nachfassen.",
                        "skillbook_id": "business-os.outbound.message_drafting.v1",
                        "runbook_id": "runbook-active-outbound"
                    }
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_assign_auto",
                "command_id": "cmd_assign_auto",
                "module": "outbound",
                "command_type": "outbound.engagement.assign_sender",
                "record_id": "eng_auto",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_auto",
                    "sender_account_id": "sender@example.com"
                },
                "client_context": actor.clone()
            }),
        )?;

        let followup = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_auto_followup",
                "command_id": "cmd_auto_followup",
                "module": "outbound",
                "command_type": "outbound.draft.prepare",
                "record_id": "msg_auto_followup",
                "status": "pending_sync",
                "payload": {
                    "message_id": "msg_auto_followup",
                    "engagement_id": "eng_auto",
                    "draft_kind": "followup_1"
                },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            followup
                .pointer("/result/message/approval_status")
                .and_then(Value::as_str),
            Some("awaiting_approval")
        );
        assert_eq!(
            followup
                .pointer("/result/provider_send_executed")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            followup
                .pointer("/result/message/payload/skillbook_id")
                .and_then(Value::as_str),
            Some("business-os.outbound.message_drafting.v1")
        );

        let scheduling = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_auto_scheduling",
                "command_id": "cmd_auto_scheduling",
                "module": "outbound",
                "command_type": "outbound.draft.prepare",
                "record_id": "msg_auto_scheduling",
                "status": "pending_sync",
                "payload": {
                    "message_id": "msg_auto_scheduling",
                    "engagement_id": "eng_auto",
                    "draft_kind": "scheduling",
                    "duration_minutes": 45,
                    "slot_hint": "drei Slots in der kommenden Woche",
                    "proposed_slots": [
                        {"start_iso":"2026-06-02T10:00:00Z","end_iso":"2026-06-02T10:45:00Z"}
                    ]
                },
                "client_context": actor
            }),
        )?;
        assert_eq!(
            scheduling
                .pointer("/result/message/message_type")
                .and_then(Value::as_str),
            Some("scheduling")
        );
        assert_eq!(
            scheduling
                .pointer("/result/message/send_status")
                .and_then(Value::as_str),
            Some("awaiting_approval")
        );
        assert_eq!(
            scheduling
                .pointer("/result/message/payload/meeting_request_id")
                .and_then(Value::as_str),
            Some("meeting_msg_auto_scheduling")
        );
        assert_eq!(
            scheduling
                .pointer("/result/message/payload/proposed_slots")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            scheduling
                .pointer("/result/meeting_request/id")
                .and_then(Value::as_str),
            Some("meeting_msg_auto_scheduling")
        );

        let conn = open_store(root)?;
        let meeting = outbound_load_required(
            &conn,
            "outbound_meeting_requests",
            "meeting_msg_auto_scheduling",
            "meeting request",
        )?;
        assert_eq!(
            outbound_string(&meeting, &["status"]).as_deref(),
            Some("prepared")
        );
        assert_eq!(
            meeting.get("duration_minutes").and_then(Value::as_i64),
            Some(45)
        );
        assert_eq!(
            meeting
                .get("proposed_slots")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );

        Ok(())
    }

    #[test]
    fn outbound_scheduling_message_can_be_edited_and_approved() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        let cmd = |id: &str, command_type: &str, record_id: &str, payload: Value| {
            serde_json::json!({
                "id": id, "command_id": id, "module": "outbound",
                "command_type": command_type, "record_id": record_id,
                "status": "pending_sync", "payload": payload,
                "client_context": actor.clone()
            })
        };

        accept_rxdb_business_command(
            root,
            cmd(
                "sch_eng",
                "outbound.engagement.create",
                "eng_sch",
                serde_json::json!({
                    "campaign_id": "camp_sch",
                    "company_id": "co_sch",
                    "contact_id": "ct_sch",
                    "payload": { "contact_email": "lead@example.com" }
                }),
            ),
        )?;
        accept_rxdb_business_command(
            root,
            cmd(
                "sch_assign",
                "outbound.engagement.assign_sender",
                "eng_sch",
                serde_json::json!({
                    "engagement_id": "eng_sch",
                    "sender_account_id": "sender@example.com"
                }),
            ),
        )?;
        let prepared = accept_rxdb_business_command(
            root,
            cmd(
                "sch_prepare",
                "outbound.draft.prepare",
                "msg_sch",
                serde_json::json!({
                    "message_id": "msg_sch",
                    "engagement_id": "eng_sch",
                    "draft_kind": "scheduling",
                    "duration_minutes": 30
                }),
            ),
        )?;
        assert_eq!(
            prepared
                .pointer("/result/message/message_type")
                .and_then(Value::as_str),
            Some("scheduling")
        );

        // The user edits the auto-generated scheduling proposal before approving.
        accept_rxdb_business_command(
            root,
            cmd(
                "sch_edit",
                "outbound.message.update_draft",
                "msg_sch",
                serde_json::json!({
                    "message_id": "msg_sch",
                    "subject": "Terminvorschlag (angepasst)",
                    "body_text": "Passt Ihnen Dienstag 10:00 oder Mittwoch 14:00?"
                }),
            ),
        )?;
        {
            let conn = open_store(root)?;
            let edited = outbound_load_required(&conn, "outbound_messages", "msg_sch", "message")?;
            // Editing resets the approval so the change cannot bypass the gate.
            assert_eq!(
                outbound_string(&edited, &["approval_status"]).as_deref(),
                Some("draft")
            );
            assert_eq!(
                outbound_string(&edited, &["subject"]).as_deref(),
                Some("Terminvorschlag (angepasst)")
            );
            assert_eq!(
                outbound_string(&edited, &["body_text"]).as_deref(),
                Some("Passt Ihnen Dienstag 10:00 oder Mittwoch 14:00?")
            );
        }

        accept_rxdb_business_command(
            root,
            cmd(
                "sch_request",
                "outbound.message.request_approval",
                "msg_sch",
                serde_json::json!({ "message_id": "msg_sch" }),
            ),
        )?;
        accept_rxdb_business_command(
            root,
            cmd(
                "sch_approve",
                "outbound.message.approve",
                "msg_sch",
                serde_json::json!({ "message_id": "msg_sch" }),
            ),
        )?;

        let conn = open_store(root)?;
        let approved = outbound_load_required(&conn, "outbound_messages", "msg_sch", "message")?;
        assert_eq!(
            outbound_string(&approved, &["approval_status"]).as_deref(),
            Some("approved"),
            "the edited scheduling message must be approvable"
        );
        assert_eq!(
            outbound_string(&approved, &["subject"]).as_deref(),
            Some("Terminvorschlag (angepasst)"),
            "the edit must survive the approval"
        );
        // The approval must match the edited revision.
        let revision_id = outbound_string(&approved, &["revision_id"])
            .unwrap_or_else(|| outbound_message_revision(&approved));
        assert!(
            outbound_has_matching_approval(&conn, "msg_sch", &revision_id)?,
            "approval must bind to the edited revision"
        );

        Ok(())
    }

    #[test]
    fn outbound_send_gate_blocks_bounce_and_unhealthy_sender() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        let cmd = |id: &str, command_type: &str, record_id: &str, payload: Value| {
            serde_json::json!({
                "id": id, "command_id": id, "module": "outbound",
                "command_type": command_type, "record_id": record_id,
                "status": "pending_sync", "payload": payload,
                "client_context": actor.clone()
            })
        };
        let approve_message = |eng: &str,
                               msg: &str,
                               sender: &str,
                               recipient: &str|
         -> anyhow::Result<()> {
            accept_rxdb_business_command(
                root,
                cmd(
                    &format!("c_eng_{msg}"),
                    "outbound.engagement.create",
                    eng,
                    serde_json::json!({"campaign_id":"camp_217","company_id":"co","contact_id":"ct"}),
                ),
            )?;
            accept_rxdb_business_command(
                root,
                cmd(
                    &format!("c_prep_{msg}"),
                    "outbound.message.prepare",
                    msg,
                    serde_json::json!({
                        "engagement_id": eng, "campaign_id": "camp_217",
                        "sender_account_id": sender, "recipient_email": recipient,
                        "subject": "Intro", "body_text": "Hello"
                    }),
                ),
            )?;
            accept_rxdb_business_command(
                root,
                cmd(
                    &format!("c_req_{msg}"),
                    "outbound.message.request_approval",
                    msg,
                    serde_json::json!({"message_id": msg}),
                ),
            )?;
            accept_rxdb_business_command(
                root,
                cmd(
                    &format!("c_apv_{msg}"),
                    "outbound.message.approve",
                    msg,
                    serde_json::json!({"message_id": msg}),
                ),
            )?;
            Ok(())
        };

        // (1) A hard bounce recorded as a suppression entry blocks the send.
        approve_message(
            "eng_bounce",
            "msg_bounce",
            "sender@example.com",
            "bounced@example.com",
        )?;
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_suppression_entries",
                "supp_bounce_217",
                1000,
                serde_json::json!({
                    "id": "supp_bounce_217",
                    "email": "bounced@example.com",
                    "reason": "bounce",
                    "status": "active",
                    "created_at_ms": 1000,
                    "updated_at_ms": 1000
                }),
            )?;
        }
        let bounce_blocked = accept_rxdb_business_command(
            root,
            cmd(
                "c_send_bounce",
                "outbound.message.send_approved",
                "msg_bounce",
                serde_json::json!({"message_id": "msg_bounce"}),
            ),
        )
        .expect_err("bounce suppression must block the send");
        assert!(
            bounce_blocked.to_string().contains("suppressed"),
            "{bounce_blocked}"
        );

        // (2) A sender account that is not provider-eligible (suspended) is rejected
        //     even for a perfectly clean recipient.
        approve_message(
            "eng_health",
            "msg_health",
            "email:sick@example.com",
            "clean@example.com",
        )?;
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_account_limits",
                "email:sick@example.com",
                1000,
                serde_json::json!({
                    "id": "email:sick@example.com",
                    "sender_account_id": "email:sick@example.com",
                    "status": "suspended",
                    "blocked": false,
                    "created_at_ms": 1000,
                    "updated_at_ms": 1000
                }),
            )?;
        }
        let health_blocked = accept_rxdb_business_command(
            root,
            cmd(
                "c_send_health",
                "outbound.message.send_approved",
                "msg_health",
                serde_json::json!({"message_id": "msg_health"}),
            ),
        )
        .expect_err("unhealthy sender account must block the send");
        assert!(
            health_blocked.to_string().contains("not eligible"),
            "{health_blocked}"
        );

        Ok(())
    }

    #[test]
    fn outbound_pause_resume_and_close_updates_engagement_state() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_engagement_lifecycle",
                "command_id": "cmd_engagement_lifecycle",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_lifecycle",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_lifecycle",
                    "company_id": "company_lifecycle",
                    "contact_id": "contact_lifecycle"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_prepare_lifecycle",
                "command_id": "cmd_prepare_lifecycle",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_lifecycle",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_lifecycle",
                    "campaign_id": "camp_lifecycle",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Intro",
                    "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_pause_lifecycle",
                "command_id": "cmd_pause_lifecycle",
                "module": "outbound",
                "command_type": "outbound.message.pause",
                "record_id": "msg_lifecycle",
                "status": "pending_sync",
                "payload": { "message_id": "msg_lifecycle", "reason": "manual review" },
                "client_context": actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let paused_message =
            outbound_load_required(&conn, "outbound_messages", "msg_lifecycle", "message")?;
        let paused_engagement =
            outbound_load_required(&conn, "outbound_engagements", "eng_lifecycle", "engagement")?;
        assert_eq!(
            outbound_string(&paused_message, &["send_status"]).as_deref(),
            Some("paused")
        );
        assert_eq!(
            outbound_string(&paused_engagement, &["status"]).as_deref(),
            Some("paused")
        );
        assert_eq!(
            outbound_string(&paused_engagement, &["paused_reason"]).as_deref(),
            Some("manual review")
        );
        drop(conn);

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_resume_lifecycle",
                "command_id": "cmd_resume_lifecycle",
                "module": "outbound",
                "command_type": "outbound.message.resume",
                "record_id": "msg_lifecycle",
                "status": "pending_sync",
                "payload": { "message_id": "msg_lifecycle", "reason": "ready" },
                "client_context": actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let resumed_message =
            outbound_load_required(&conn, "outbound_messages", "msg_lifecycle", "message")?;
        let resumed_engagement =
            outbound_load_required(&conn, "outbound_engagements", "eng_lifecycle", "engagement")?;
        assert_eq!(
            outbound_string(&resumed_message, &["send_status"]).as_deref(),
            Some("not_scheduled")
        );
        assert_eq!(
            outbound_string(&resumed_engagement, &["status"]).as_deref(),
            Some("draft_prepared")
        );
        drop(conn);

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_close_lifecycle",
                "command_id": "cmd_close_lifecycle",
                "module": "outbound",
                "command_type": "outbound.engagement.close",
                "record_id": "eng_lifecycle",
                "status": "pending_sync",
                "payload": { "engagement_id": "eng_lifecycle", "reason": "not a fit" },
                "client_context": actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let closed_message =
            outbound_load_required(&conn, "outbound_messages", "msg_lifecycle", "message")?;
        let closed_engagement =
            outbound_load_required(&conn, "outbound_engagements", "eng_lifecycle", "engagement")?;
        assert_eq!(
            outbound_string(&closed_message, &["send_status"]).as_deref(),
            Some("cancelled")
        );
        assert_eq!(
            outbound_string(&closed_engagement, &["status"]).as_deref(),
            Some("closed")
        );
        assert_eq!(
            outbound_string(&closed_engagement, &["closed_reason"]).as_deref(),
            Some("not a fit")
        );

        Ok(())
    }

    #[test]
    fn outbound_engagement_status_advances_through_approval_and_send() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        let cmd = |id: &str, command_type: &str, record_id: &str, payload: Value| {
            serde_json::json!({
                "id": id, "command_id": id, "module": "outbound",
                "command_type": command_type, "record_id": record_id,
                "status": "pending_sync", "payload": payload,
                "client_context": actor.clone()
            })
        };
        let engagement_status = |id: &str| -> anyhow::Result<String> {
            let conn = open_store(root)?;
            let eng = outbound_load_required(&conn, "outbound_engagements", id, "engagement")?;
            Ok(outbound_string(&eng, &["status"]).unwrap_or_default())
        };

        accept_rxdb_business_command(
            root,
            cmd(
                "fwd_create",
                "outbound.engagement.create",
                "eng_fwd",
                serde_json::json!({
                    "campaign_id": "camp_fwd",
                    "company_id": "co_fwd",
                    "contact_id": "ct_fwd"
                }),
            ),
        )?;

        accept_rxdb_business_command(
            root,
            cmd(
                "fwd_prepare",
                "outbound.message.prepare",
                "msg_fwd",
                serde_json::json!({
                    "engagement_id": "eng_fwd",
                    "campaign_id": "camp_fwd",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Intro",
                    "body_text": "Hello"
                }),
            ),
        )?;
        assert_eq!(engagement_status("eng_fwd")?, "draft_prepared");

        accept_rxdb_business_command(
            root,
            cmd(
                "fwd_request",
                "outbound.message.request_approval",
                "msg_fwd",
                serde_json::json!({ "message_id": "msg_fwd" }),
            ),
        )?;
        assert_eq!(engagement_status("eng_fwd")?, "awaiting_approval");

        accept_rxdb_business_command(
            root,
            cmd(
                "fwd_approve",
                "outbound.message.approve",
                "msg_fwd",
                serde_json::json!({ "message_id": "msg_fwd" }),
            ),
        )?;
        assert_eq!(engagement_status("eng_fwd")?, "approved_for_send");

        accept_rxdb_business_command(
            root,
            cmd(
                "fwd_send",
                "outbound.message.send_approved",
                "msg_fwd",
                serde_json::json!({ "message_id": "msg_fwd" }),
            ),
        )?;
        // Email goes through the mailserver queue, so the engagement lands on
        // `scheduled_to_send` (queued for provider), not the physical-letter
        // immediate `sent`.
        assert_eq!(engagement_status("eng_fwd")?, "scheduled_to_send");

        Ok(())
    }

    #[test]
    fn outbound_empty_collections_load_without_errors() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        for collection in [
            "outbound_campaigns",
            "outbound_engagements",
            "outbound_messages",
            "outbound_approvals",
            "outbound_sequences",
            "outbound_sender_assignments",
            "outbound_meeting_requests",
            "outbound_suppression_entries",
            "outbound_account_limits",
        ] {
            let records =
                outbound_load_records_by_string_field(&conn, collection, "campaign_id", "missing")?;
            assert!(
                records.is_empty(),
                "expected {collection} to be empty but got {} rows",
                records.len()
            );
            let single = outbound_load_record(&conn, collection, "missing-id")?;
            assert!(
                single.is_none(),
                "expected {collection} missing-id lookup to be None"
            );
        }
        Ok(())
    }

    #[test]
    fn outbound_tombstone_marks_record_as_deleted() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        upsert_business_record(
            &conn,
            "outbound_messages",
            "msg_tombstone",
            1000,
            serde_json::json!({
                "id": "msg_tombstone",
                "engagement_id": "eng_t",
                "send_status": "draft",
                "created_at_ms": 1000,
                "updated_at_ms": 1000,
            }),
        )?;
        // Soft-delete: set deleted = 1
        conn.execute(
            "UPDATE business_records SET deleted = 1 WHERE collection = 'outbound_messages' AND record_id = 'msg_tombstone'",
            [],
        )?;
        // After tombstone, outbound_load_record (which filters deleted = 0) must return None
        let loaded = outbound_load_record(&conn, "outbound_messages", "msg_tombstone")?;
        assert!(
            loaded.is_none(),
            "tombstoned outbound_messages record must not be loadable"
        );
        // Re-upserting must re-activate (deleted = 0)
        upsert_business_record(
            &conn,
            "outbound_messages",
            "msg_tombstone",
            2000,
            serde_json::json!({
                "id": "msg_tombstone",
                "engagement_id": "eng_t",
                "send_status": "draft",
                "updated_at_ms": 2000,
            }),
        )?;
        let reloaded = outbound_load_record(&conn, "outbound_messages", "msg_tombstone")?;
        assert!(
            reloaded.is_some(),
            "re-upserted record must replace the tombstone"
        );
        Ok(())
    }

    #[test]
    fn outbound_tombstone_and_conflict_strategy_for_approvals() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;

        // Initial approval decision.
        upsert_business_record(
            &conn,
            "outbound_approvals",
            "apv_conflict",
            1000,
            serde_json::json!({
                "id": "apv_conflict",
                "message_id": "msg_x",
                "revision_id": "rev_1",
                "decision": "approved",
                "created_at_ms": 1000,
                "updated_at_ms": 1000,
            }),
        )?;

        // A conflicting write (no tombstone) is last-write-wins: the later payload
        // overwrites the earlier one for the same (collection, record_id).
        upsert_business_record(
            &conn,
            "outbound_approvals",
            "apv_conflict",
            1500,
            serde_json::json!({
                "id": "apv_conflict",
                "message_id": "msg_x",
                "revision_id": "rev_1",
                "decision": "changes_requested",
                "created_at_ms": 1000,
                "updated_at_ms": 1500,
            }),
        )?;
        let after_conflict =
            outbound_load_required(&conn, "outbound_approvals", "apv_conflict", "approval")?;
        assert_eq!(
            outbound_string(&after_conflict, &["decision"]).as_deref(),
            Some("changes_requested"),
            "the latest write must win for a conflicting approval upsert"
        );

        // Tombstone the approval: a deleted approval must not be loadable, so a
        // resolved/withdrawn approval cannot silently re-gate a send.
        conn.execute(
            "UPDATE business_records SET deleted = 1 WHERE collection = 'outbound_approvals' AND record_id = 'apv_conflict'",
            [],
        )?;
        assert!(
            outbound_load_record(&conn, "outbound_approvals", "apv_conflict")?.is_none(),
            "a tombstoned approval must not be loadable"
        );

        // Re-upsert resurrects the tombstone (deleted reset to 0) with the new
        // payload — the state machine resolves a tombstone-vs-write conflict in
        // favor of the live write so a re-issued approval is not lost.
        upsert_business_record(
            &conn,
            "outbound_approvals",
            "apv_conflict",
            2000,
            serde_json::json!({
                "id": "apv_conflict",
                "message_id": "msg_x",
                "revision_id": "rev_2",
                "decision": "approved",
                "created_at_ms": 1000,
                "updated_at_ms": 2000,
            }),
        )?;
        let resurrected =
            outbound_load_required(&conn, "outbound_approvals", "apv_conflict", "approval")?;
        assert_eq!(
            outbound_string(&resurrected, &["decision"]).as_deref(),
            Some("approved")
        );
        assert_eq!(
            outbound_string(&resurrected, &["revision_id"]).as_deref(),
            Some("rev_2"),
            "the resurrected approval carries the new revision binding"
        );

        Ok(())
    }

    #[test]
    fn outbound_email_html_only_body_uses_multipart_alternative() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_html",
                "command_id": "cmd_eng_html",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_html",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_html","company_id":"co_html","contact_id":"ct_html"},
                "client_context": actor.clone()
            }),
        )?;
        // HTML-only message — body_text is empty, body_html has content.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_html",
                "command_id": "cmd_msg_html",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_html",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_html",
                    "campaign_id": "camp_html",
                    "sender_account_id": "email:sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "HTML only",
                    "body_html": "<p>Hello <strong>world</strong></p>"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_html",
                "command_id": "cmd_req_html",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_html",
                "status": "pending_sync",
                "payload": {"message_id":"msg_html"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_html",
                "command_id": "cmd_apv_html",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_html",
                "status": "pending_sync",
                "payload": {"message_id":"msg_html"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_html",
                "command_id": "cmd_send_html",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_html",
                "status": "pending_sync",
                "payload": {"message_id":"msg_html"},
                "client_context": actor.clone()
            }),
        )?;
        // Inspect the queued SMTP body and confirm it has the HTML part.
        let queue_conn = Connection::open(crate::paths::core_db(root))?;
        let body: String = queue_conn.query_row(
            "SELECT msg_body FROM stalwart_smtp_queue WHERE to_addr = 'lead@example.com' LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        assert!(
            body.contains("multipart/alternative"),
            "expected multipart/alternative, got: {body}"
        );
        assert!(
            body.contains("text/html"),
            "expected text/html part, got: {body}"
        );
        assert!(
            body.contains("<p>Hello <strong>world</strong></p>"),
            "expected raw HTML in body, got: {body}"
        );
        // And the text/plain fallback must not be empty.
        assert!(
            body.contains("Hello")
                && !body.contains(
                    "text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n\r\n"
                ),
            "expected non-empty text/plain fallback, got: {body}"
        );
        Ok(())
    }

    #[test]
    fn outbound_email_send_blocked_when_body_completely_empty() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_empty",
                "command_id": "cmd_eng_empty",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_empty",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_empty","company_id":"co_e","contact_id":"ct_e"},
                "client_context": actor.clone()
            }),
        )?;
        // Both body fields empty
        let prep = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_empty",
                "command_id": "cmd_msg_empty",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_empty",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_empty",
                    "campaign_id": "camp_empty",
                    "sender_account_id": "email:sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Empty",
                    "body_text": "",
                    "body_html": ""
                },
                "client_context": actor.clone()
            }),
        )?;
        // request_approval should block because content is empty.
        let req = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_empty",
                "command_id": "cmd_req_empty",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_empty",
                "status": "pending_sync",
                "payload": {"message_id":"msg_empty"},
                "client_context": actor.clone()
            }),
        );
        // Either request_approval already errors or send_approved later does.
        if req.is_ok() {
            let _ = accept_rxdb_business_command(
                root,
                serde_json::json!({
                    "id": "cmd_apv_empty",
                    "command_id": "cmd_apv_empty",
                    "module": "outbound",
                    "command_type": "outbound.message.approve",
                    "record_id": "msg_empty",
                    "status": "pending_sync",
                    "payload": {"message_id":"msg_empty"},
                    "client_context": actor.clone()
                }),
            );
            let send = accept_rxdb_business_command(
                root,
                serde_json::json!({
                    "id": "cmd_send_empty",
                    "command_id": "cmd_send_empty",
                    "module": "outbound",
                    "command_type": "outbound.message.send_approved",
                    "record_id": "msg_empty",
                    "status": "pending_sync",
                    "payload": {"message_id":"msg_empty"},
                    "client_context": actor.clone()
                }),
            )
            .expect_err("empty body must block send");
            assert!(
                send.to_string().contains("content")
                    || send.to_string().contains("body")
                    || send.to_string().contains("empty"),
                "{send}"
            );
        }
        let _ = prep;
        Ok(())
    }

    #[test]
    fn outbound_daily_limit_zero_is_treated_as_unlimited() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        // Use mailbox.link to create the campaign+account_limits with daily_limit=0
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_mbx_dlz",
                "command_id": "cmd_mbx_dlz",
                "module": "outbound",
                "command_type": "outbound.campaign.mailbox.link",
                "record_id": "camp_dlz",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_dlz","mailbox_address":"dlz@example.com","mailbox_status":"ready"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_dlz",
                "command_id": "cmd_eng_dlz",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_dlz",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_dlz","company_id":"co_d","contact_id":"ct_d"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_dlz",
                "command_id": "cmd_msg_dlz",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_dlz",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_dlz",
                    "campaign_id": "camp_dlz",
                    "sender_account_id": "email:dlz@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "DLZ",
                    "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_dlz",
                "command_id": "cmd_req_dlz",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_dlz",
                "status": "pending_sync",
                "payload": {"message_id":"msg_dlz"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_dlz",
                "command_id": "cmd_apv_dlz",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_dlz",
                "status": "pending_sync",
                "payload": {"message_id":"msg_dlz"},
                "client_context": actor.clone()
            }),
        )?;
        // daily_limit=0 must be treated as unlimited — send must succeed.
        let send = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_dlz",
                "command_id": "cmd_send_dlz",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_dlz",
                "status": "pending_sync",
                "payload": {"message_id":"msg_dlz"},
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            send.pointer("/result/provider_dispatch_status")
                .and_then(Value::as_str),
            Some("queued_in_mailserver")
        );
        Ok(())
    }

    #[test]
    fn outbound_bare_email_sender_normalizes_to_email_prefix_for_limit_lookup() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let root = temp.path();
        // Pre-seed an account_limits row with canonical key.
        let conn = open_store(root)?;
        upsert_business_record(
            &conn,
            "outbound_account_limits",
            "email:s@example.com",
            1000,
            serde_json::json!({
                "id": "email:s@example.com",
                "sender_account_id": "email:s@example.com",
                "status": "blocked",
                "blocked": true,
                "daily_limit": 100,
                "daily_sent_count": 0,
                "created_at_ms": 1000,
                "updated_at_ms": 1000,
            }),
        )?;
        // Now run the limit check against a bare email — should hit the canonical row.
        let result = outbound_enforce_account_limit(&conn, "s@example.com");
        assert!(
            result.is_err(),
            "bare email must resolve to canonical limit row"
        );
        assert!(result.unwrap_err().to_string().contains("blocked"));
        Ok(())
    }

    #[test]
    fn outbound_skillbook_seed_defaults_is_idempotent() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        let res1 = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"sk_seed_1","command_id":"sk_seed_1","module":"outbound",
                "command_type":"outbound.skillbook.seed_defaults","record_id":"",
                "status":"pending_sync","payload":{},"client_context":actor.clone()
            }),
        )?;
        let first_seeded = res1
            .pointer("/result/seeded")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0);
        assert_eq!(first_seeded, 3);
        let res2 = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"sk_seed_2","command_id":"sk_seed_2","module":"outbound",
                "command_type":"outbound.skillbook.seed_defaults","record_id":"",
                "status":"pending_sync","payload":{},"client_context":actor.clone()
            }),
        )?;
        let second_seeded = res2
            .pointer("/result/seeded")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0);
        assert_eq!(second_seeded, 0, "second seed must be no-op");
        Ok(())
    }

    #[test]
    fn outbound_skillbook_seed_defaults_carry_real_backbone_and_guidance() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let conn = open_store(root)?;
        let now = now_ms() as i64;
        outbound_handle_skillbook_seed_defaults(&conn, now)?;

        let drafting = outbound_load_required(
            &conn,
            "outbound_skillbooks",
            "business-os.outbound.message_drafting.v1",
            "message drafting skillbook",
        )?;
        let backbone = drafting
            .get("workflow_backbone")
            .and_then(Value::as_array)
            .expect("workflow_backbone array");
        assert!(
            !backbone.is_empty(),
            "message drafting skillbook must have a real workflow backbone"
        );
        assert!(backbone
            .iter()
            .any(|step| { outbound_string(step, &["step"]).as_deref() == Some("writeback") }));
        let routing = drafting
            .get("routing_taxonomy")
            .and_then(Value::as_array)
            .expect("routing_taxonomy array");
        assert!(!routing.is_empty(), "routing taxonomy must be populated");

        let guidance =
            outbound_skillbook_guidance(&conn, "business-os.outbound.message_drafting.v1")?
                .expect("guidance present");
        assert!(guidance.contains("Leitplanke:"));

        assert!(
            outbound_skillbook_guidance(&conn, "does-not-exist")?.is_none(),
            "missing skillbook yields no guidance"
        );
        Ok(())
    }

    #[test]
    fn outbound_skillbook_save_bumps_version_number() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        let r1 = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"sk_v_1","command_id":"sk_v_1","module":"outbound",
                "command_type":"outbound.skillbook.save","record_id":"business-os.outbound.message_drafting.v1",
                "status":"pending_sync","payload":{"skillbook_id":"business-os.outbound.message_drafting.v1","mission":"M1"},
                "client_context":actor.clone()
            }),
        )?;
        assert_eq!(
            r1.pointer("/result/version_number").and_then(Value::as_i64),
            Some(1)
        );
        let r2 = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"sk_v_2","command_id":"sk_v_2","module":"outbound",
                "command_type":"outbound.skillbook.save","record_id":"business-os.outbound.message_drafting.v1",
                "status":"pending_sync","payload":{"skillbook_id":"business-os.outbound.message_drafting.v1","mission":"M2"},
                "client_context":actor.clone()
            }),
        )?;
        assert_eq!(
            r2.pointer("/result/version_number").and_then(Value::as_i64),
            Some(2)
        );
        Ok(())
    }

    #[test]
    fn outbound_skillbook_save_preserves_unsent_fields() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"sk_preserve_seed","command_id":"sk_preserve_seed","module":"outbound",
                "command_type":"outbound.skillbook.seed_defaults","record_id":"",
                "status":"pending_sync","payload":{},"client_context":actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"sk_preserve_save","command_id":"sk_preserve_save","module":"outbound",
                "command_type":"outbound.skillbook.save","record_id":"business-os.outbound.message_drafting.v1",
                "status":"pending_sync",
                "payload":{
                    "skillbook_id":"business-os.outbound.message_drafting.v1",
                    "mission":"Updated mission only"
                },
                "client_context":actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let stored = outbound_load_required(
            &conn,
            "outbound_skillbooks",
            "business-os.outbound.message_drafting.v1",
            "skillbook",
        )?;
        assert_eq!(
            outbound_string(&stored, &["title"]).as_deref(),
            Some("Initial- und Follow-up-Drafts vorbereiten")
        );
        assert!(
            stored
                .get("stop_rules")
                .and_then(Value::as_array)
                .map(|items| !items.is_empty())
                .unwrap_or(false),
            "stop_rules must survive partial saves"
        );
        assert_eq!(
            outbound_string(&stored, &["mission"]).as_deref(),
            Some("Updated mission only")
        );
        Ok(())
    }

    #[test]
    fn outbound_letter_template_save_persists_record() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tpl_save","command_id":"tpl_save","module":"outbound",
                "command_type":"outbound.letter_template.save","record_id":"tpl_x",
                "status":"pending_sync","payload":{"template_id":"tpl_x","title":"T","salutation":"Hi","closing":"Bye"},
                "client_context":actor.clone()
            }),
        )?;
        assert_eq!(
            r.pointer("/result/template/title").and_then(Value::as_str),
            Some("T")
        );
        let conn = open_store(root)?;
        let stored = outbound_load_required(&conn, "outbound_letter_templates", "tpl_x", "t")?;
        assert_eq!(
            outbound_string(&stored, &["salutation"]).as_deref(),
            Some("Hi")
        );
        Ok(())
    }

    #[test]
    fn outbound_audit_export_returns_collections_filtered_by_campaign() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        // Seed two engagements on different campaigns.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"e1","command_id":"e1","module":"outbound",
                "command_type":"outbound.engagement.create","record_id":"e1",
                "status":"pending_sync",
                "payload":{"campaign_id":"camp_audit","company_id":"co","contact_id":"ct"},
                "client_context":actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"e2","command_id":"e2","module":"outbound",
                "command_type":"outbound.engagement.create","record_id":"e2",
                "status":"pending_sync",
                "payload":{"campaign_id":"other","company_id":"co","contact_id":"ct"},
                "client_context":actor.clone()
            }),
        )?;
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"audit_x","command_id":"audit_x","module":"outbound",
                "command_type":"outbound.audit.export","record_id":"",
                "status":"pending_sync","payload":{"campaign_id":"camp_audit"},
                "client_context":actor.clone()
            }),
        )?;
        let engagements = r
            .pointer("/result/export/outbound_engagements")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert_eq!(engagements.len(), 1, "filter must include only camp_audit");
        assert_eq!(
            engagements[0].pointer("/id").and_then(Value::as_str),
            Some("e1")
        );
        assert!(
            r.pointer("/result/export/outbound_skillbooks").is_some(),
            "audit export must include skillbook configuration"
        );
        assert!(
            r.pointer("/result/export/outbound_letter_templates")
                .is_some(),
            "audit export must include letter template configuration"
        );
        Ok(())
    }

    #[test]
    fn outbound_scheduler_tick_runs_dry_then_reconciles() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        let r_dry = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_dry","command_id":"tick_dry","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{"dry_run":true},
                "client_context":actor.clone()
            }),
        )?;
        assert_eq!(
            r_dry.pointer("/result/dry_run").and_then(Value::as_bool),
            Some(true)
        );
        // Non-dry run also succeeds even on an empty DB.
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_real","command_id":"tick_real","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{},
                "client_context":actor.clone()
            }),
        )?;
        assert_eq!(r.pointer("/result/ok").and_then(Value::as_bool), Some(true));
        Ok(())
    }

    #[test]
    fn outbound_scheduler_tick_prepares_due_followup_draft() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_engagements",
                "eng_due",
                1000,
                serde_json::json!({
                    "id":"eng_due",
                    "campaign_id":"camp_due",
                    "sender_account_id":"email:scheduler@example.com",
                    "status":"sent",
                    "next_action_at_ms":1,
                    "payload":{
                        "contact_name":"Lead",
                        "company_name":"Scheduler GmbH",
                        "contact_email":"lead@example.com"
                    },
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
        }
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_due","command_id":"tick_due","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{},
                "client_context":actor.clone()
            }),
        )?;
        let actions = r
            .pointer("/result/actions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            actions.iter().any(|action| {
                action.get("kind").and_then(Value::as_str) == Some("followup_draft_prepared")
            }),
            "scheduler must create a follow-up draft, actions={actions:?}"
        );
        let conn = open_store(root)?;
        let messages = outbound_load_records_by_string_field(
            &conn,
            "outbound_messages",
            "engagement_id",
            "eng_due",
        )?;
        assert_eq!(messages.len(), 1);
        assert_eq!(
            outbound_string(&messages[0], &["approval_status"]).as_deref(),
            Some("awaiting_approval")
        );
        assert_ne!(
            outbound_string(&messages[0], &["id"]).as_deref(),
            Some("eng_due"),
            "scheduler-created message must not reuse the engagement id"
        );
        let engagement =
            outbound_load_required(&conn, "outbound_engagements", "eng_due", "engagement")?;
        assert_eq!(
            engagement.get("next_action_at_ms").and_then(Value::as_i64),
            Some(0)
        );
        Ok(())
    }

    #[test]
    fn outbound_out_of_office_reply_schedules_gated_retry_without_send() -> anyhow::Result<()> {
        // Welle 7 (554): an out-of-office reply must not stop the sequence like a
        // hard reply — it schedules a wait/retry, and the resumed follow-up is
        // still approval-gated (no send without a fresh approval).
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_engagements",
                "eng_ooo",
                1000,
                serde_json::json!({
                    "id":"eng_ooo",
                    "campaign_id":"camp_ooo",
                    "sender_account_id":"email:scheduler@example.com",
                    "status":"sent",
                    "payload":{
                        "contact_name":"Lead",
                        "company_name":"OOO GmbH",
                        "contact_email":"lead@example.com"
                    },
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
        }

        // Classify the reply as out-of-office. Unlike unsubscribe/bounce this is
        // not a hard stop: the engagement stays reply_received for the UI but a
        // future wait/retry plan is recorded.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"cls_ooo","command_id":"cls_ooo","module":"outbound",
                "command_type":"outbound.reply.classify","record_id":"eng_ooo",
                "status":"pending_sync",
                "payload":{"engagement_id":"eng_ooo","classification":"out_of_office","reply_message_id":"reply_1"},
                "client_context":actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let after_classify =
            outbound_load_required(&conn, "outbound_engagements", "eng_ooo", "engagement")?;
        assert_eq!(
            outbound_string(&after_classify, &["status"]).as_deref(),
            Some("reply_received"),
            "OOO keeps reply_received for the UI"
        );
        assert_eq!(
            outbound_string(&after_classify, &["payload", "reply_wait_reason"]).as_deref(),
            Some("out_of_office"),
            "a wait/retry plan is recorded"
        );
        let planned = after_classify
            .get("next_action_at_ms")
            .and_then(Value::as_i64)
            .expect("OOO must schedule a future retry");
        assert!(planned > 1000, "retry is scheduled into the future");

        // Force the hold to be due, then run a scheduler tick.
        let mut due_engagement = after_classify.clone();
        outbound_put_i64(&mut due_engagement, "next_action_at_ms", 1);
        upsert_business_record(
            &conn,
            "outbound_engagements",
            "eng_ooo",
            2000,
            due_engagement,
        )?;
        drop(conn);

        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_ooo","command_id":"tick_ooo","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{},
                "client_context":actor.clone()
            }),
        )?;
        let actions = r
            .pointer("/result/actions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            actions.iter().any(|action| {
                action.get("kind").and_then(Value::as_str) == Some("followup_draft_prepared")
                    && action.get("engagement_id").and_then(Value::as_str) == Some("eng_ooo")
            }),
            "OOO retry must resume the follow-up, actions={actions:?}"
        );

        // The resumed follow-up is approval-gated: not sent, not queued.
        let conn = open_store(root)?;
        let messages = outbound_load_records_by_string_field(
            &conn,
            "outbound_messages",
            "engagement_id",
            "eng_ooo",
        )?;
        assert_eq!(messages.len(), 1, "exactly one resumed draft");
        assert_eq!(
            outbound_string(&messages[0], &["approval_status"]).as_deref(),
            Some("awaiting_approval"),
            "resumed OOO follow-up requires approval"
        );
        assert_eq!(
            outbound_string(&messages[0], &["send_status"]).as_deref(),
            Some("awaiting_approval"),
            "no send happens without approval"
        );
        Ok(())
    }

    #[test]
    fn outbound_scheduler_tick_skips_when_account_limit_exhausted() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_engagements",
                "eng_cap_due",
                1000,
                serde_json::json!({
                    "id":"eng_cap_due",
                    "campaign_id":"camp_cap_due",
                    "sender_account_id":"email:capped@example.com",
                    "status":"sent",
                    "next_action_at_ms":1,
                    "payload":{
                        "contact_name":"Lead",
                        "company_name":"Capped GmbH",
                        "contact_email":"lead@example.com"
                    },
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
            // The sender account has already exhausted its daily cap.
            upsert_business_record(
                &conn,
                "outbound_account_limits",
                "email:capped@example.com",
                1000,
                serde_json::json!({
                    "id":"email:capped@example.com",
                    "daily_limit":2,
                    "daily_sent_count":2,
                    "remaining_today":0,
                    "status":"active",
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
        }
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_cap","command_id":"tick_cap","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{},
                "client_context":actor.clone()
            }),
        )?;
        let actions = r
            .pointer("/result/actions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            actions.iter().any(|action| {
                action.get("kind").and_then(Value::as_str) == Some("followup_skipped_account_limit")
            }),
            "scheduler must skip when the sender cap is exhausted, actions={actions:?}"
        );
        let conn = open_store(root)?;
        // No unsendable draft was created.
        let messages = outbound_load_records_by_string_field(
            &conn,
            "outbound_messages",
            "engagement_id",
            "eng_cap_due",
        )?;
        assert!(
            messages.is_empty(),
            "no follow-up draft may be created when the account cap is exhausted"
        );
        // The engagement records the skip reason and stays due for a later retry
        // (the daily cap resets), so next_action_at_ms is not zeroed out.
        let engagement =
            outbound_load_required(&conn, "outbound_engagements", "eng_cap_due", "engagement")?;
        assert_eq!(
            outbound_string(&engagement, &["payload", "scheduler_last_skip_reason"]).as_deref(),
            Some("account_limit")
        );
        assert_eq!(
            engagement.get("next_action_at_ms").and_then(Value::as_i64),
            Some(1),
            "a capped follow-up must remain due for a later retry"
        );
        Ok(())
    }

    #[test]
    fn outbound_scheduler_draft_carries_sequence_version() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_engagements",
                "eng_seq",
                1000,
                serde_json::json!({
                    "id":"eng_seq",
                    "campaign_id":"camp_seq",
                    "sender_account_id":"email:scheduler@example.com",
                    "status":"sent",
                    "next_action_at_ms":1,
                    "payload":{
                        "contact_name":"Lead",
                        "company_name":"Sequence GmbH",
                        "contact_email":"lead@example.com",
                        "sequence_id":"seq_v3",
                        "sequence_snapshot":{ "id":"seq_v3", "updated_at_ms": 424242 }
                    },
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
        }
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_seq","command_id":"tick_seq","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{},
                "client_context":actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let messages = outbound_load_records_by_string_field(
            &conn,
            "outbound_messages",
            "engagement_id",
            "eng_seq",
        )?;
        assert_eq!(messages.len(), 1);
        // The draft must be traceable to the exact sequence revision.
        assert_eq!(
            outbound_string(&messages[0], &["payload", "sequence_id"]).as_deref(),
            Some("seq_v3")
        );
        assert_eq!(
            messages[0]
                .pointer("/payload/sequence_version")
                .and_then(Value::as_i64),
            Some(424242)
        );
        Ok(())
    }

    #[test]
    fn outbound_scheduler_tick_skips_paused_campaign() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_campaigns",
                "camp_paused",
                1000,
                serde_json::json!({
                    "id":"camp_paused",
                    "status":"paused",
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
            upsert_business_record(
                &conn,
                "outbound_engagements",
                "eng_paused_camp",
                1000,
                serde_json::json!({
                    "id":"eng_paused_camp",
                    "campaign_id":"camp_paused",
                    "sender_account_id":"email:scheduler@example.com",
                    "status":"sent",
                    "next_action_at_ms":1,
                    "payload":{
                        "contact_name":"Lead",
                        "company_name":"Paused GmbH",
                        "contact_email":"lead@example.com"
                    },
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
        }
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_paused","command_id":"tick_paused","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{},
                "client_context":actor.clone()
            }),
        )?;
        let actions = r
            .pointer("/result/actions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            actions.iter().any(|action| {
                action.get("kind").and_then(Value::as_str)
                    == Some("followup_skipped_campaign_paused")
            }),
            "scheduler must skip engagements of a paused campaign, actions={actions:?}"
        );
        let conn = open_store(root)?;
        let messages = outbound_load_records_by_string_field(
            &conn,
            "outbound_messages",
            "engagement_id",
            "eng_paused_camp",
        )?;
        assert!(
            messages.is_empty(),
            "a paused campaign must not produce follow-up drafts"
        );
        Ok(())
    }

    #[test]
    fn outbound_scheduler_tick_does_not_follow_up_after_reply() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_engagements",
                "eng_reply_stop",
                1000,
                serde_json::json!({
                    "id":"eng_reply_stop",
                    "campaign_id":"camp_due",
                    "sender_account_id":"email:scheduler@example.com",
                    "status":"reply_received",
                    "next_action_at_ms":1,
                    "payload":{
                        "contact_name":"Lead",
                        "company_name":"Scheduler GmbH",
                        "contact_email":"lead@example.com",
                        "reply_classification":"positive"
                    },
                    "created_at_ms":1000,
                    "updated_at_ms":1000
                }),
            )?;
        }
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"tick_reply_stop","command_id":"tick_reply_stop","module":"outbound",
                "command_type":"outbound.scheduler.tick","record_id":"",
                "status":"pending_sync","payload":{},
                "client_context":actor.clone()
            }),
        )?;
        let actions = r
            .pointer("/result/actions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            !actions.iter().any(|action| {
                action.get("kind").and_then(Value::as_str) == Some("followup_draft_prepared")
            }),
            "scheduler must not create follow-ups after reply, actions={actions:?}"
        );
        let conn = open_store(root)?;
        let messages = outbound_load_records_by_string_field(
            &conn,
            "outbound_messages",
            "engagement_id",
            "eng_reply_stop",
        )?;
        assert!(messages.is_empty());
        Ok(())
    }

    #[test]
    fn outbound_dev_seed_test_data_creates_engagements() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"seed_td","command_id":"seed_td","module":"outbound",
                "command_type":"outbound.dev.seed_test_data","record_id":"",
                "status":"pending_sync","payload":{"campaign_id":"camp_dev","count":4},
                "client_context":actor.clone()
            }),
        )?;
        assert_eq!(r.pointer("/result/count").and_then(Value::as_i64), Some(4));
        Ok(())
    }

    #[test]
    fn outbound_engagement_reapply_sequence_requires_sequence_id() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"er_e","command_id":"er_e","module":"outbound",
                "command_type":"outbound.engagement.create","record_id":"er_e",
                "status":"pending_sync",
                "payload":{"campaign_id":"camp_re","company_id":"c","contact_id":"x"},
                "client_context":actor.clone()
            }),
        )?;
        let err = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"er_rs","command_id":"er_rs","module":"outbound",
                "command_type":"outbound.engagement.reapply_sequence","record_id":"er_e",
                "status":"pending_sync","payload":{"engagement_id":"er_e"},
                "client_context":actor.clone()
            }),
        )
        .expect_err("missing sequence_id must error");
        assert!(err.to_string().contains("sequence_id"), "{err}");
        Ok(())
    }

    #[test]
    fn outbound_active_engagement_keeps_sequence_version_until_explicit_reapply(
    ) -> anyhow::Result<()> {
        // Welle 4 (367): a live campaign sequence change must not silently
        // re-version active engagements. Each engagement stays pinned to the
        // sequence snapshot it captured until an explicit reapply flow runs.
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});

        // Sequence revision v1.
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_sequences",
                "seq_367",
                100,
                serde_json::json!({
                    "id":"seq_367","campaign_id":"camp_367",
                    "updated_at_ms":100,"touchpoints":[{"day":0}]
                }),
            )?;
        }
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"e367","command_id":"e367","module":"outbound",
                "command_type":"outbound.engagement.create","record_id":"e367",
                "status":"pending_sync",
                "payload":{"campaign_id":"camp_367","company_id":"c","contact_id":"x"},
                "client_context":actor.clone()
            }),
        )?;
        // Pin the engagement to v1 via the explicit reapply flow.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"rs1","command_id":"rs1","module":"outbound",
                "command_type":"outbound.engagement.reapply_sequence","record_id":"e367",
                "status":"pending_sync",
                "payload":{"engagement_id":"e367","sequence_id":"seq_367"},
                "client_context":actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let engagement =
            outbound_load_required(&conn, "outbound_engagements", "e367", "engagement")?;
        let (_, pinned_version) = outbound_engagement_sequence_context(&engagement);
        assert_eq!(pinned_version, 100, "engagement pinned to sequence v1");

        // Campaign edits the sequence (new revision v2). This re-writes the
        // shared sequence record but must not touch existing engagements.
        upsert_business_record(
            &conn,
            "outbound_sequences",
            "seq_367",
            200,
            serde_json::json!({
                "id":"seq_367","campaign_id":"camp_367",
                "updated_at_ms":200,"touchpoints":[{"day":0},{"day":3}]
            }),
        )?;
        let engagement_after_edit =
            outbound_load_required(&conn, "outbound_engagements", "e367", "engagement")?;
        let (_, still_pinned) = outbound_engagement_sequence_context(&engagement_after_edit);
        assert_eq!(
            still_pinned, 100,
            "a live sequence edit must not silently re-version an active engagement"
        );
        drop(conn);

        // Explicit reapply rolls the engagement forward to v2.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"rs2","command_id":"rs2","module":"outbound",
                "command_type":"outbound.engagement.reapply_sequence","record_id":"e367",
                "status":"pending_sync",
                "payload":{"engagement_id":"e367","sequence_id":"seq_367"},
                "client_context":actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let reapplied =
            outbound_load_required(&conn, "outbound_engagements", "e367", "engagement")?;
        let (_, new_version) = outbound_engagement_sequence_context(&reapplied);
        assert_eq!(new_version, 200, "explicit reapply rolls forward to v2");
        assert!(
            reapplied
                .pointer("/payload/sequence_reapplied_at_ms")
                .and_then(Value::as_i64)
                .is_some(),
            "reapply stamps a traceable timestamp"
        );
        Ok(())
    }

    #[test]
    fn outbound_scheduling_update_slots_replaces_proposed_slots() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        // Pre-seed a meeting request with one slot.
        let conn = open_store(root)?;
        upsert_business_record(
            &conn,
            "outbound_meeting_requests",
            "mreq_1",
            1000,
            serde_json::json!({
                "id":"mreq_1","engagement_id":"e","status":"prepared",
                "proposed_slots":[{"start_iso":"2026-06-01T10:00:00Z"}],
                "created_at_ms":1000,"updated_at_ms":1000
            }),
        )?;
        drop(conn);
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"upd_slots","command_id":"upd_slots","module":"outbound",
                "command_type":"outbound.scheduling.update_slots","record_id":"mreq_1",
                "status":"pending_sync","payload":{"meeting_request_id":"mreq_1","proposed_slots":[]},
                "client_context":actor.clone()
            }),
        )?;
        let slots = r
            .pointer("/result/meeting_request/proposed_slots")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert_eq!(slots.len(), 0);
        Ok(())
    }

    #[test]
    fn outbound_draft_prepare_for_physical_letter_does_not_require_email() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"pl_eng","command_id":"pl_eng","module":"outbound",
                "command_type":"outbound.engagement.create","record_id":"pl_eng",
                "status":"pending_sync",
                "payload":{"campaign_id":"camp_pl","company_id":"co","contact_id":"ct"},
                "client_context":actor.clone()
            }),
        )?;
        let r = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"pl_draft","command_id":"pl_draft","module":"outbound",
                "command_type":"outbound.draft.prepare","record_id":"pl_msg",
                "status":"pending_sync",
                "payload":{
                    "engagement_id":"pl_eng",
                    "draft_kind":"initial",
                    "campaign_id":"camp_pl",
                    "channel":"physical_letter",
                    "recipient_address_text":"Tester Inc.\nStr. 1\n10115 Berlin"
                },
                "client_context":actor.clone()
            }),
        )?;
        assert_eq!(
            r.pointer("/result/message/channel").and_then(Value::as_str),
            Some("physical_letter")
        );
        assert_eq!(
            r.pointer("/result/message/recipient_address_text")
                .and_then(Value::as_str),
            Some("Tester Inc.\nStr. 1\n10115 Berlin")
        );
        Ok(())
    }

    #[test]
    fn outbound_update_draft_persists_recipient_address_text() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({"actor":{"id":"t","role":"admin"}});
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"ud_eng","command_id":"ud_eng","module":"outbound",
                "command_type":"outbound.engagement.create","record_id":"ud_eng",
                "status":"pending_sync",
                "payload":{"campaign_id":"camp_ud","company_id":"co","contact_id":"ct"},
                "client_context":actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"ud_prep","command_id":"ud_prep","module":"outbound",
                "command_type":"outbound.message.prepare","record_id":"ud_msg",
                "status":"pending_sync",
                "payload":{
                    "engagement_id":"ud_eng","campaign_id":"camp_ud",
                    "channel":"physical_letter",
                    "recipient_address_text":"Old Addr",
                    "subject":"Hi","body_text":"x"
                },
                "client_context":actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id":"ud_upd","command_id":"ud_upd","module":"outbound",
                "command_type":"outbound.message.update_draft","record_id":"ud_msg",
                "status":"pending_sync",
                "payload":{"message_id":"ud_msg","recipient_address_text":"New Addr 42\n12345 Berlin"},
                "client_context":actor.clone()
            }),
        )?;
        let conn = open_store(root)?;
        let msg = outbound_load_required(&conn, "outbound_messages", "ud_msg", "msg")?;
        assert_eq!(
            outbound_string(&msg, &["recipient_address_text"]).as_deref(),
            Some("New Addr 42\n12345 Berlin")
        );
        Ok(())
    }

    #[test]
    fn outbound_physical_letter_marks_manual_send_without_mail_account() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_letter",
                "command_id": "cmd_eng_letter",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_letter",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_letter",
                    "company_id": "co_letter",
                    "contact_id": "ct_letter"
                },
                "client_context": actor.clone()
            }),
        )?;
        // Prepare a physical_letter message — NO sender_account_id, NO recipient_email.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter_prepare",
                "command_id": "cmd_letter_prepare",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_letter",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_letter",
                    "campaign_id": "camp_letter",
                    "channel": "physical_letter",
                    "recipient_address_text": "Tester Inc.\nMusterstrasse 1\n10115 Berlin",
                    "subject": "Letter Intro",
                    "body_text": "Sehr geehrter Herr Tester,\n\nbitte beachten Sie unser Angebot.\n\nFreundliche Gruesse"
                },
                "client_context": actor.clone()
            }),
        )?;
        // The send_gate should refuse before approval.
        let before_apv = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter_send_pre",
                "command_id": "cmd_letter_send_pre",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_letter",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter" },
                "client_context": actor.clone()
            }),
        )
        .expect_err("send must be blocked before approval even for letters");
        assert!(
            before_apv.to_string().contains("must be approved"),
            "{before_apv}"
        );
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter_req",
                "command_id": "cmd_letter_req",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_letter",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter" },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter_apv",
                "command_id": "cmd_letter_apv",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_letter",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter" },
                "client_context": actor.clone()
            }),
        )?;
        let send = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter_send",
                "command_id": "cmd_letter_send",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_letter",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter" },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            send.pointer("/result/channel").and_then(Value::as_str),
            Some("physical_letter")
        );
        assert_eq!(
            send.pointer("/result/provider_dispatch_status")
                .and_then(Value::as_str),
            Some("manual_physical_letter_marked_sent")
        );
        assert!(send
            .pointer("/result/physical_sent_at_ms")
            .and_then(Value::as_i64)
            .is_some());
        // Idempotency: replaying send_approved must not re-mark.
        let send_again = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter_send2",
                "command_id": "cmd_letter_send2",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_letter",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter" },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            send_again
                .pointer("/result/idempotent")
                .and_then(Value::as_bool),
            Some(true)
        );

        // Negative: a physical_letter without recipient_address_text must be blocked.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter2_prepare",
                "command_id": "cmd_letter2_prepare",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_letter2",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_letter",
                    "campaign_id": "camp_letter",
                    "channel": "physical_letter",
                    "subject": "Letter2",
                    "body_text": "No address provided"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter2_req",
                "command_id": "cmd_letter2_req",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_letter2",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter2" },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter2_apv",
                "command_id": "cmd_letter2_apv",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_letter2",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter2" },
                "client_context": actor.clone()
            }),
        )?;
        let blocked = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_letter2_send",
                "command_id": "cmd_letter2_send",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_letter2",
                "status": "pending_sync",
                "payload": { "message_id": "msg_letter2" },
                "client_context": actor.clone()
            }),
        )
        .expect_err("missing recipient_address_text must block letter send");
        assert!(
            blocked.to_string().contains("recipient_address_text"),
            "{blocked}"
        );
        Ok(())
    }

    #[test]
    fn outbound_message_send_blocked_when_recipient_is_suppressed() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_supp",
                "command_id": "cmd_eng_supp",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_supp",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_supp",
                    "company_id": "co_supp",
                    "contact_id": "ct_supp"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_supp",
                "command_id": "cmd_msg_supp",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_supp",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_supp",
                    "campaign_id": "camp_supp",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "blocked@example.com",
                    "subject": "Intro",
                    "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_supp_entry",
                "command_id": "cmd_supp_entry",
                "module": "outbound",
                "command_type": "outbound.suppression.add",
                "record_id": "supp_blocked",
                "status": "pending_sync",
                "payload": {
                    "id": "supp_blocked",
                    "email": "blocked@example.com",
                    "reason": "unsubscribe",
                    "status": "active"
                },
                "client_context": actor.clone()
            }),
        )
        .ok();
        // The suppression collection is generic; insert directly to bypass any module guard.
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_suppression_entries",
                "supp_blocked",
                1000,
                serde_json::json!({
                    "id": "supp_blocked",
                    "email": "blocked@example.com",
                    "reason": "unsubscribe",
                    "status": "active",
                    "created_at_ms": 1000,
                    "updated_at_ms": 1000
                }),
            )?;
        }
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_supp",
                "command_id": "cmd_req_supp",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_supp",
                "status": "pending_sync",
                "payload": { "message_id": "msg_supp" },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_supp",
                "command_id": "cmd_apv_supp",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_supp",
                "status": "pending_sync",
                "payload": { "message_id": "msg_supp" },
                "client_context": actor.clone()
            }),
        )?;
        let blocked = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_supp",
                "command_id": "cmd_send_supp",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_supp",
                "status": "pending_sync",
                "payload": { "message_id": "msg_supp" },
                "client_context": actor.clone()
            }),
        )
        .expect_err("send must be blocked by suppression");
        assert!(blocked.to_string().contains("suppressed"), "{blocked}");

        // The failed send must persist a structured, replicable block reason
        // onto the message WITHOUT destroying the approved draft, so it stays
        // retry-able.
        let conn = open_store(root)?;
        let msg = outbound_load_required(&conn, "outbound_messages", "msg_supp", "message")?;
        assert_eq!(
            outbound_string(&msg, &["send_status"]).as_deref(),
            Some("send_blocked")
        );
        assert_eq!(
            outbound_string(&msg, &["payload", "send_block_reason"]).as_deref(),
            Some("recipient_suppressed")
        );
        assert_eq!(
            outbound_string(&msg, &["approval_status"]).as_deref(),
            Some("approved"),
            "approval and draft must survive a blocked send"
        );
        assert_eq!(
            outbound_string(&msg, &["body_text"]).as_deref(),
            Some("Hello")
        );
        assert_eq!(
            msg.pointer("/payload/retryable").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            msg.pointer("/payload/send_attempts")
                .and_then(Value::as_i64),
            Some(1)
        );
        // Lift the suppression and retry: the same approved draft must now queue.
        upsert_business_record(
            &conn,
            "outbound_suppression_entries",
            "supp_blocked",
            2000,
            serde_json::json!({
                "id": "supp_blocked",
                "email": "blocked@example.com",
                "reason": "unsubscribe",
                "status": "inactive",
                "created_at_ms": 1000,
                "updated_at_ms": 2000
            }),
        )?;
        drop(conn);
        let retried = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_supp_retry",
                "command_id": "cmd_send_supp_retry",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_supp",
                "status": "pending_sync",
                "payload": { "message_id": "msg_supp" },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            retried.pointer("/result/ok").and_then(Value::as_bool),
            Some(true)
        );
        let conn = open_store(root)?;
        let msg = outbound_load_required(&conn, "outbound_messages", "msg_supp", "message")?;
        assert_eq!(
            outbound_string(&msg, &["send_status"]).as_deref(),
            Some("queued_for_provider")
        );
        assert_eq!(
            msg.pointer("/payload/retryable").and_then(Value::as_bool),
            Some(false),
            "successful retry clears the retryable flag"
        );
        assert!(
            msg.pointer("/payload/send_block_reason").is_none()
                || msg.pointer("/payload/send_block_reason") == Some(&Value::Null),
            "successful retry clears the block reason"
        );
        Ok(())
    }

    #[test]
    fn outbound_send_approved_is_idempotent_for_already_queued_message() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_mbx_idem", "command_id": "cmd_mbx_idem", "module": "outbound",
                "command_type": "outbound.campaign.mailbox.link", "record_id": "camp_idem",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_idem","mailbox_address":"idem@example.com","mailbox_status":"ready"},
                "client_context": actor.clone()
            }),
        )?;
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_account_limits",
                "email:idem@example.com",
                1000,
                serde_json::json!({
                    "id": "email:idem@example.com",
                    "sender_account_id": "email:idem@example.com",
                    "status": "active",
                    "blocked": false,
                    "daily_limit": 5,
                    "daily_sent_count": 0,
                    "sent_today": 0,
                    "created_at_ms": 1000,
                    "updated_at_ms": 1000
                }),
            )?;
        }
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_idem", "command_id": "cmd_eng_idem", "module": "outbound",
                "command_type": "outbound.engagement.create", "record_id": "eng_idem",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_idem","company_id":"co_idem","contact_id":"ct_idem"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_prep_idem", "command_id": "cmd_prep_idem", "module": "outbound",
                "command_type": "outbound.message.prepare", "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_idem", "campaign_id": "camp_idem",
                    "sender_account_id": "email:idem@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Hi", "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_idem", "command_id": "cmd_req_idem", "module": "outbound",
                "command_type": "outbound.message.request_approval", "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": {"message_id":"msg_idem"}, "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_idem", "command_id": "cmd_apv_idem", "module": "outbound",
                "command_type": "outbound.message.approve", "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": {"message_id":"msg_idem"}, "client_context": actor.clone()
            }),
        )?;

        // Two distinct command envelopes (different command_id, so the command
        // bus does not dedupe at the command level) targeting the same message.
        let send_cmd = |cmd_id: &str| {
            serde_json::json!({
                "id": cmd_id, "command_id": cmd_id, "module": "outbound",
                "command_type": "outbound.message.send_approved", "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": {"message_id":"msg_idem"}, "client_context": actor.clone()
            })
        };
        let first = accept_rxdb_business_command(root, send_cmd("cmd_send_idem_1"))?;
        let queue_id = first
            .pointer("/result/provider_queue_id")
            .and_then(Value::as_str)
            .context("first send must return provider_queue_id")?
            .to_string();
        assert_eq!(
            first.pointer("/result/idempotent").and_then(Value::as_bool),
            None,
            "the first send is not an idempotent replay"
        );

        // Re-dispatch the very same approved+queued message. It must be a no-op
        // replay: same queue id, no second mailserver row, no double-count.
        let second = accept_rxdb_business_command(root, send_cmd("cmd_send_idem_2"))?;
        assert_eq!(
            second
                .pointer("/result/idempotent")
                .and_then(Value::as_bool),
            Some(true),
            "a re-send of a queued message must be flagged idempotent"
        );
        assert_eq!(
            second
                .pointer("/result/provider_queue_id")
                .and_then(Value::as_str),
            Some(queue_id.as_str()),
            "idempotent replay keeps the original queue id"
        );

        let queue_conn = Connection::open(crate::paths::core_db(root))?;
        let queued_count: i64 = queue_conn.query_row(
            "SELECT COUNT(*) FROM stalwart_smtp_queue WHERE to_addr = 'lead@example.com'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            queued_count, 1,
            "no duplicate mailserver queue row on replay"
        );
        drop(queue_conn);

        let conn = open_store(root)?;
        let limit = outbound_load_required(
            &conn,
            "outbound_account_limits",
            "email:idem@example.com",
            "limit",
        )?;
        assert_eq!(
            limit.get("daily_sent_count").and_then(Value::as_i64),
            Some(1),
            "idempotent replay must not increment the daily counter"
        );
        Ok(())
    }

    #[test]
    fn outbound_send_links_message_to_communication_thread_bidirectionally() -> anyhow::Result<()> {
        // Welle 10 (637/638): after an approved email is sent, the outbound
        // message and the communication thread must reference each other so the
        // link is traceable from either side (debug/status surfaces read it).
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_mbx_link", "command_id": "cmd_mbx_link", "module": "outbound",
                "command_type": "outbound.campaign.mailbox.link", "record_id": "camp_link",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_link","mailbox_address":"link@example.com","mailbox_status":"ready"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_link", "command_id": "cmd_eng_link", "module": "outbound",
                "command_type": "outbound.engagement.create", "record_id": "eng_link",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_link","company_id":"co_link","contact_id":"ct_link"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_prep_link", "command_id": "cmd_prep_link", "module": "outbound",
                "command_type": "outbound.message.prepare", "record_id": "msg_link",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_link", "campaign_id": "camp_link",
                    "sender_account_id": "email:link@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Hi", "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_link", "command_id": "cmd_req_link", "module": "outbound",
                "command_type": "outbound.message.request_approval", "record_id": "msg_link",
                "status": "pending_sync",
                "payload": {"message_id":"msg_link"}, "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_link", "command_id": "cmd_apv_link", "module": "outbound",
                "command_type": "outbound.message.approve", "record_id": "msg_link",
                "status": "pending_sync",
                "payload": {"message_id":"msg_link"}, "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_link", "command_id": "cmd_send_link", "module": "outbound",
                "command_type": "outbound.message.send_approved", "record_id": "msg_link",
                "status": "pending_sync",
                "payload": {"message_id":"msg_link"}, "client_context": actor.clone()
            }),
        )?;

        // Outbound side carries the communication keys written back by the sync.
        let conn = open_store(root)?;
        let message = outbound_load_required(&conn, "outbound_messages", "msg_link", "message")?;
        let message_key = message
            .get("communication_message_key")
            .and_then(Value::as_str)
            .context("outbound message must carry communication_message_key after send")?
            .to_string();
        let thread_key = message
            .get("thread_key")
            .and_then(Value::as_str)
            .context("outbound message must carry thread_key after send")?
            .to_string();
        assert!(
            !message_key.is_empty() && !thread_key.is_empty(),
            "communication keys must be non-empty"
        );
        drop(conn);

        // Communication side carries the outbound identifiers in its metadata.
        let channel_conn = Connection::open(crate::paths::core_db(root))?;
        let metadata_json: String = channel_conn.query_row(
            "SELECT metadata_json FROM communication_messages WHERE message_key = ?1",
            [&message_key],
            |row| row.get(0),
        )?;
        let metadata: Value = serde_json::from_str(&metadata_json)?;
        assert_eq!(
            metadata
                .get("outbound_engagement_id")
                .and_then(Value::as_str),
            Some("eng_link"),
            "communication message metadata must back-reference the engagement"
        );
        assert_eq!(
            metadata.get("outbound_message_id").and_then(Value::as_str),
            Some("msg_link"),
            "communication message metadata must back-reference the outbound message"
        );
        assert_eq!(
            metadata
                .get("communication_thread_key")
                .and_then(Value::as_str),
            Some(thread_key.as_str()),
            "thread key must match on both sides of the link"
        );
        Ok(())
    }

    #[test]
    fn outbound_daily_limit_enforced_under_parallel_commands() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        let n: usize = 6;
        let cap: i64 = 3;

        // Establish the campaign + account_limits row, then pin a hard daily cap.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_mbx_cap", "command_id": "cmd_mbx_cap", "module": "outbound",
                "command_type": "outbound.campaign.mailbox.link", "record_id": "camp_cap",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_cap","mailbox_address":"cap@example.com","mailbox_status":"ready"},
                "client_context": actor.clone()
            }),
        )?;
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_account_limits",
                "email:cap@example.com",
                1000,
                serde_json::json!({
                    "id": "email:cap@example.com",
                    "sender_account_id": "email:cap@example.com",
                    "status": "active",
                    "blocked": false,
                    "daily_limit": cap,
                    "daily_sent_count": 0,
                    "sent_today": 0,
                    "created_at_ms": 1000,
                    "updated_at_ms": 1000
                }),
            )?;
        }

        // Build N independent approved messages, all sharing the one sender.
        for i in 0..n {
            let eng = format!("eng_cap_{i}");
            let msg = format!("msg_cap_{i}");
            accept_rxdb_business_command(
                root,
                serde_json::json!({
                    "id": format!("c_eng_{i}"), "command_id": format!("c_eng_{i}"),
                    "module": "outbound", "command_type": "outbound.engagement.create",
                    "record_id": eng, "status": "pending_sync",
                    "payload": {"campaign_id":"camp_cap","company_id":format!("co_{i}"),"contact_id":format!("ct_{i}")},
                    "client_context": actor.clone()
                }),
            )?;
            accept_rxdb_business_command(
                root,
                serde_json::json!({
                    "id": format!("c_prep_{i}"), "command_id": format!("c_prep_{i}"),
                    "module": "outbound", "command_type": "outbound.message.prepare",
                    "record_id": msg, "status": "pending_sync",
                    "payload": {
                        "engagement_id": eng, "campaign_id": "camp_cap",
                        "sender_account_id": "email:cap@example.com",
                        "recipient_email": format!("lead{i}@example.com"),
                        "subject": "Hi", "body_text": "Hello"
                    },
                    "client_context": actor.clone()
                }),
            )?;
            accept_rxdb_business_command(
                root,
                serde_json::json!({
                    "id": format!("c_req_{i}"), "command_id": format!("c_req_{i}"),
                    "module": "outbound", "command_type": "outbound.message.request_approval",
                    "record_id": msg, "status": "pending_sync",
                    "payload": {"message_id": msg}, "client_context": actor.clone()
                }),
            )?;
            accept_rxdb_business_command(
                root,
                serde_json::json!({
                    "id": format!("c_apv_{i}"), "command_id": format!("c_apv_{i}"),
                    "module": "outbound", "command_type": "outbound.message.approve",
                    "record_id": msg, "status": "pending_sync",
                    "payload": {"message_id": msg}, "client_context": actor.clone()
                }),
            )?;
        }

        // Fire all N sends concurrently — each opens its own connection, exactly
        // the parallel-command scenario the atomic reservation must survive.
        let root_buf = root.to_path_buf();
        let handles: Vec<_> = (0..n)
            .map(|i| {
                let r = root_buf.clone();
                let actor = actor.clone();
                std::thread::spawn(move || {
                    accept_rxdb_business_command(
                        &r,
                        serde_json::json!({
                            "id": format!("c_send_{i}"), "command_id": format!("c_send_{i}"),
                            "module": "outbound", "command_type": "outbound.message.send_approved",
                            "record_id": format!("msg_cap_{i}"), "status": "pending_sync",
                            "payload": {"message_id": format!("msg_cap_{i}")},
                            "client_context": actor
                        }),
                    )
                })
            })
            .collect();

        let mut ok = 0usize;
        let mut limit_blocked = 0usize;
        let mut transient = 0usize;
        for handle in handles {
            match handle.join().expect("send thread panicked") {
                Ok(_) => ok += 1,
                Err(err) => {
                    let msg = err.to_string();
                    if msg.contains("daily limit") {
                        limit_blocked += 1;
                    } else if msg.contains("locked") {
                        // SQLite write-lock contention under heavy parallel load is a
                        // transient failure. It may strike before the slot reservation
                        // (no slot consumed) or after it commits but before the message
                        // upsert (a leaked-but-counted slot). Either way the slot is
                        // never double-counted and the counter cannot exceed the cap, so
                        // the no-overshoot guarantee holds; only the realized send count
                        // drops. The approved draft stays retryable.
                        transient += 1;
                    } else {
                        panic!("unexpected send error: {err}");
                    }
                }
            }
        }
        assert_eq!(
            ok + limit_blocked + transient,
            n,
            "every attempt is accounted for"
        );
        // The core safety guarantee: parallel sends may never exceed the cap.
        assert!(
            ok <= cap as usize,
            "parallel sends overshot the cap: {ok} > {cap}"
        );
        let conn = open_store(root)?;
        let limit = outbound_load_required(
            &conn,
            "outbound_account_limits",
            "email:cap@example.com",
            "limit",
        )?;
        let counter = limit
            .get("daily_sent_count")
            .and_then(Value::as_i64)
            .expect("daily_sent_count present");
        // The no-overshoot guarantee: the reservation never lets the counter exceed
        // the cap, and every realized send is reflected in it.
        assert!(
            counter <= cap,
            "parallel sends overshot the daily cap: {counter} > {cap}"
        );
        assert!(
            counter >= ok as i64,
            "every successful send must be counted: counter {counter} < ok {ok}"
        );
        assert_eq!(
            limit.get("remaining_today").and_then(Value::as_i64),
            Some(cap - counter)
        );
        // Absent transient contention the cap is fully reached, the rest hard-blocked,
        // and the counter lands exactly on the cap with no leaked slots.
        if transient == 0 {
            assert_eq!(ok, cap as usize, "exactly the cap may pass");
            assert_eq!(limit_blocked, n - cap as usize, "the rest must be blocked");
            assert_eq!(counter, cap, "counter must land exactly on the cap");
            assert_eq!(counter, ok as i64, "no leaked slots without contention");
        }
        Ok(())
    }

    #[test]
    fn outbound_send_failure_reflects_block_onto_engagement() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_ef", "command_id": "cmd_eng_ef", "module": "outbound",
                "command_type": "outbound.engagement.create", "record_id": "eng_ef",
                "status": "pending_sync",
                "payload": {"campaign_id":"camp_ef","company_id":"co_ef","contact_id":"ct_ef"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_ef", "command_id": "cmd_msg_ef", "module": "outbound",
                "command_type": "outbound.message.prepare", "record_id": "msg_ef",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_ef", "campaign_id": "camp_ef",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "blocked@example.com",
                    "subject": "Intro", "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_suppression_entries",
                "supp_ef",
                1000,
                serde_json::json!({
                    "id": "supp_ef", "email": "blocked@example.com",
                    "reason": "bounce", "status": "active",
                    "created_at_ms": 1000, "updated_at_ms": 1000
                }),
            )?;
        }
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_ef", "command_id": "cmd_req_ef", "module": "outbound",
                "command_type": "outbound.message.request_approval", "record_id": "msg_ef",
                "status": "pending_sync", "payload": {"message_id":"msg_ef"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_ef", "command_id": "cmd_apv_ef", "module": "outbound",
                "command_type": "outbound.message.approve", "record_id": "msg_ef",
                "status": "pending_sync", "payload": {"message_id":"msg_ef"},
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_ef", "command_id": "cmd_send_ef", "module": "outbound",
                "command_type": "outbound.message.send_approved", "record_id": "msg_ef",
                "status": "pending_sync", "payload": {"message_id":"msg_ef"},
                "client_context": actor.clone()
            }),
        )
        .expect_err("send must be blocked");

        // The engagement must carry the structured block reason so the timeline UI
        // can show why the send did not go out.
        let conn = open_store(root)?;
        let eng = outbound_load_required(&conn, "outbound_engagements", "eng_ef", "engagement")?;
        assert_eq!(
            outbound_string(&eng, &["status"]).as_deref(),
            Some("send_blocked")
        );
        assert_eq!(
            outbound_string(&eng, &["last_send_block_reason"]).as_deref(),
            Some("recipient_suppressed")
        );
        assert!(
            outbound_string(&eng, &["last_send_error"]).is_some(),
            "engagement records the underlying error text"
        );
        Ok(())
    }

    #[test]
    fn outbound_message_send_is_idempotent_after_queueing() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_idem",
                "command_id": "cmd_eng_idem",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_idem",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_idem",
                    "company_id": "co_idem",
                    "contact_id": "ct_idem"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_idem",
                "command_id": "cmd_msg_idem",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_idem",
                    "campaign_id": "camp_idem",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Intro",
                    "body_text": "Hello"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_idem",
                "command_id": "cmd_req_idem",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": { "message_id": "msg_idem" },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_idem",
                "command_id": "cmd_apv_idem",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": { "message_id": "msg_idem" },
                "client_context": actor.clone()
            }),
        )?;
        let first = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_idem_1",
                "command_id": "cmd_send_idem_1",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": { "message_id": "msg_idem" },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            first
                .pointer("/result/provider_dispatch_status")
                .and_then(Value::as_str),
            Some("queued_in_mailserver")
        );
        let first_queue_id = first
            .pointer("/result/provider_queue_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        assert!(first_queue_id.is_some(), "expected provider_queue_id");

        let second = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_idem_2",
                "command_id": "cmd_send_idem_2",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_idem",
                "status": "pending_sync",
                "payload": { "message_id": "msg_idem" },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            second
                .pointer("/result/idempotent")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            second
                .pointer("/result/provider_queue_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            first_queue_id
        );

        // Ensure stalwart_smtp_queue contains exactly one queued row.
        let queue_conn = Connection::open(crate::paths::core_db(root))?;
        let count: i64 = queue_conn.query_row(
            "SELECT COUNT(*) FROM stalwart_smtp_queue WHERE to_addr = 'lead@example.com'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 1, "idempotent re-send must not double-queue");
        Ok(())
    }

    #[test]
    fn outbound_campaign_mailbox_link_projects_to_communication_accounts() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        let res = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_mailbox_link",
                "command_id": "cmd_mailbox_link",
                "module": "outbound",
                "command_type": "outbound.campaign.mailbox.link",
                "record_id": "camp_mbx",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_mbx",
                    "mailbox_address": "outreach@example.com",
                    "mailbox_status": "ready",
                    "display_name": "Outreach"
                },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            res.pointer("/result/communication_account_key")
                .and_then(Value::as_str),
            Some("email:outreach@example.com")
        );
        let conn = open_store(root)?;
        let campaign = outbound_load_required(&conn, "outbound_campaigns", "camp_mbx", "campaign")?;
        assert_eq!(
            outbound_string(&campaign, &["mailbox_status"]).as_deref(),
            Some("ready")
        );
        assert_eq!(
            outbound_string(&campaign, &["communication_account_address"]).as_deref(),
            Some("outreach@example.com")
        );
        let limit = outbound_load_required(
            &conn,
            "outbound_account_limits",
            "email:outreach@example.com",
            "account_limits",
        )?;
        assert_eq!(
            outbound_string(&limit, &["campaign_id"]).as_deref(),
            Some("camp_mbx")
        );
        drop(conn);

        // verify communication_accounts row exists in channels db
        let channel_conn = channels::open_channel_db(&crate::paths::core_db(root))?;
        let exists: Option<String> = channel_conn
            .query_row(
                "SELECT address FROM communication_accounts WHERE account_key = ?1",
                rusqlite::params!["email:outreach@example.com"],
                |row| row.get(0),
            )
            .optional()?;
        assert_eq!(exists.as_deref(), Some("outreach@example.com"));

        Ok(())
    }

    #[test]
    fn outbound_campaign_activation_requires_ready_channel() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        // Create campaign without any mailbox link
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_campaigns",
                "camp_act",
                1000,
                serde_json::json!({
                    "id": "camp_act",
                    "status": "setup_required",
                    "payload": { "active_outreach": { "default_channel": "email" } },
                    "created_at_ms": 1000,
                    "updated_at_ms": 1000
                }),
            )?;
        }

        let blocked = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_status_blocked",
                "command_id": "cmd_status_blocked",
                "module": "outbound",
                "command_type": "outbound.campaign.status.set",
                "record_id": "camp_act",
                "status": "pending_sync",
                "payload": { "campaign_id": "camp_act", "status": "active", "channel": "email" },
                "client_context": actor.clone()
            }),
        )
        .expect_err("activation must require a linked mailbox");
        assert!(blocked.to_string().contains("linked mailbox"), "{blocked}");

        // Link mailbox + activate must succeed
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_mbx_link_act",
                "command_id": "cmd_mbx_link_act",
                "module": "outbound",
                "command_type": "outbound.campaign.mailbox.link",
                "record_id": "camp_act",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_act",
                    "mailbox_address": "ops@example.com"
                },
                "client_context": actor.clone()
            }),
        )?;
        let ok = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_status_ok",
                "command_id": "cmd_status_ok",
                "module": "outbound",
                "command_type": "outbound.campaign.status.set",
                "record_id": "camp_act",
                "status": "pending_sync",
                "payload": { "campaign_id": "camp_act", "status": "active", "channel": "email" },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            ok.pointer("/result/status").and_then(Value::as_str),
            Some("active")
        );

        // physical_letter activation must work without mailbox
        {
            let conn = open_store(root)?;
            upsert_business_record(
                &conn,
                "outbound_campaigns",
                "camp_phys",
                1100,
                serde_json::json!({
                    "id": "camp_phys",
                    "status": "setup_required",
                    "payload": { "active_outreach": { "default_channel": "physical_letter" } },
                    "created_at_ms": 1100,
                    "updated_at_ms": 1100
                }),
            )?;
        }
        let phys = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_status_phys",
                "command_id": "cmd_status_phys",
                "module": "outbound",
                "command_type": "outbound.campaign.status.set",
                "record_id": "camp_phys",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_phys",
                    "status": "active",
                    "channel": "physical_letter"
                },
                "client_context": actor.clone()
            }),
        )?;
        assert_eq!(
            phys.pointer("/result/status").and_then(Value::as_str),
            Some("active")
        );

        Ok(())
    }

    #[test]
    fn outbound_reply_match_sets_engagement_and_stops_pending_followups() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_reply",
                "command_id": "cmd_eng_reply",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_reply",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_reply",
                    "company_id": "co_reply",
                    "contact_id": "ct_reply"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_followup",
                "command_id": "cmd_msg_followup",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_followup",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_reply",
                    "campaign_id": "camp_reply",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Follow-up",
                    "body_text": "Just checking in",
                    "message_type": "followup"
                },
                "client_context": actor.clone()
            }),
        )?;

        let res = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_reply_match",
                "command_id": "cmd_reply_match",
                "module": "outbound",
                "command_type": "outbound.reply.match",
                "record_id": "eng_reply",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_reply",
                    "reply_message_id": "email:inbox/lead-reply-1",
                    "classification": "positive"
                },
                "client_context": actor.clone()
            }),
        )?;
        let cancelled_ids = res
            .pointer("/result/cancelled_message_ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert_eq!(cancelled_ids.len(), 1);
        assert_eq!(cancelled_ids[0].as_str(), Some("msg_followup"));

        let conn = open_store(root)?;
        let engagement =
            outbound_load_required(&conn, "outbound_engagements", "eng_reply", "engagement")?;
        assert_eq!(
            outbound_string(&engagement, &["status"]).as_deref(),
            Some("reply_received")
        );
        assert_eq!(
            outbound_string(&engagement, &["payload", "reply_classification"]).as_deref(),
            Some("positive")
        );
        let message =
            outbound_load_required(&conn, "outbound_messages", "msg_followup", "message")?;
        assert_eq!(
            outbound_string(&message, &["send_status"]).as_deref(),
            Some("cancelled")
        );
        assert_eq!(
            outbound_string(&message, &["payload", "cancelled_reason"]).as_deref(),
            Some("reply_received")
        );

        Ok(())
    }

    #[test]
    fn outbound_reply_match_preserves_already_sent_messages() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_keep",
                "command_id": "cmd_eng_keep",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_keep",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_keep",
                    "company_id": "co_keep",
                    "contact_id": "ct_keep"
                },
                "client_context": actor.clone()
            }),
        )?;
        // An already-queued initial message and a not-yet-sent follow-up draft.
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_sent",
                "command_id": "cmd_msg_sent",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_sent",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_keep",
                    "campaign_id": "camp_keep",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Intro",
                    "body_text": "Hello",
                    "message_type": "initial"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_draft",
                "command_id": "cmd_msg_draft",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_draft",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_keep",
                    "campaign_id": "camp_keep",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "lead@example.com",
                    "subject": "Follow-up",
                    "body_text": "Checking in",
                    "message_type": "followup"
                },
                "client_context": actor.clone()
            }),
        )?;

        {
            let conn = open_store(root)?;
            let mut sent =
                outbound_load_required(&conn, "outbound_messages", "msg_sent", "message")?;
            outbound_put_string(&mut sent, "send_status", "queued_for_provider");
            upsert_business_record(
                &conn,
                "outbound_messages",
                "msg_sent",
                now_ms() as i64,
                sent,
            )?;
        }

        let res = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_reply_keep",
                "command_id": "cmd_reply_keep",
                "module": "outbound",
                "command_type": "outbound.reply.match",
                "record_id": "eng_keep",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_keep",
                    "reply_message_id": "email:inbox/lead-reply-keep",
                    "classification": "positive"
                },
                "client_context": actor.clone()
            }),
        )?;
        let cancelled_ids = res
            .pointer("/result/cancelled_message_ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            cancelled_ids.len(),
            1,
            "only the un-sent draft may be cancelled"
        );
        assert_eq!(cancelled_ids[0].as_str(), Some("msg_draft"));

        let conn = open_store(root)?;
        let sent = outbound_load_required(&conn, "outbound_messages", "msg_sent", "message")?;
        assert_eq!(
            outbound_string(&sent, &["send_status"]).as_deref(),
            Some("queued_for_provider"),
            "already-queued message must be preserved"
        );
        let draft = outbound_load_required(&conn, "outbound_messages", "msg_draft", "message")?;
        assert_eq!(
            outbound_string(&draft, &["send_status"]).as_deref(),
            Some("cancelled")
        );

        Ok(())
    }

    #[test]
    fn outbound_unsubscribe_reply_creates_suppression_and_blocks_send() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path();
        let actor = serde_json::json!({
            "actor": { "id": "tester", "role": "admin", "display_name": "Tester" }
        });

        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_eng_unsub",
                "command_id": "cmd_eng_unsub",
                "module": "outbound",
                "command_type": "outbound.engagement.create",
                "record_id": "eng_unsub",
                "status": "pending_sync",
                "payload": {
                    "campaign_id": "camp_unsub",
                    "company_id": "co_unsub",
                    "contact_id": "ct_unsub",
                    "payload": { "contact_email": "stop@example.com" }
                },
                "client_context": actor.clone()
            }),
        )?;

        // The recipient replies asking to be removed. The reply must register an
        // active suppression entry so any later send to that address is refused.
        let res = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_reply_unsub",
                "command_id": "cmd_reply_unsub",
                "module": "outbound",
                "command_type": "outbound.reply.match",
                "record_id": "eng_unsub",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_unsub",
                    "reply_message_id": "email:inbox/unsub-1",
                    "classification": "unsubscribe"
                },
                "client_context": actor.clone()
            }),
        )?;
        let suppression_id = res
            .pointer("/result/suppression_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        assert!(
            suppression_id.is_some(),
            "unsubscribe reply must create a suppression entry, got {res:?}"
        );

        let conn = open_store(root)?;
        let reason = outbound_recipient_suppression_reason(&conn, "stop@example.com")?;
        assert_eq!(reason.as_deref(), Some("unsubscribe"));
        // The engagement must be hard-stopped, not merely marked reply_received.
        let engagement =
            outbound_load_required(&conn, "outbound_engagements", "eng_unsub", "engagement")?;
        assert_eq!(
            outbound_string(&engagement, &["status"]).as_deref(),
            Some("stopped")
        );
        assert_eq!(
            outbound_string(&engagement, &["payload", "stop_reason"]).as_deref(),
            Some("unsubscribe")
        );
        drop(conn);

        // A fresh approved draft to the now-suppressed recipient must be refused
        // by the send gate (the suppression was created purely by the reply).
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_msg_after_unsub",
                "command_id": "cmd_msg_after_unsub",
                "module": "outbound",
                "command_type": "outbound.message.prepare",
                "record_id": "msg_after_unsub",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_unsub",
                    "campaign_id": "camp_unsub",
                    "sender_account_id": "sender@example.com",
                    "recipient_email": "stop@example.com",
                    "subject": "One more thing",
                    "body_text": "Following up again",
                    "message_type": "followup"
                },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_req_after_unsub",
                "command_id": "cmd_req_after_unsub",
                "module": "outbound",
                "command_type": "outbound.message.request_approval",
                "record_id": "msg_after_unsub",
                "status": "pending_sync",
                "payload": { "message_id": "msg_after_unsub" },
                "client_context": actor.clone()
            }),
        )?;
        accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_apv_after_unsub",
                "command_id": "cmd_apv_after_unsub",
                "module": "outbound",
                "command_type": "outbound.message.approve",
                "record_id": "msg_after_unsub",
                "status": "pending_sync",
                "payload": { "message_id": "msg_after_unsub" },
                "client_context": actor.clone()
            }),
        )?;
        let blocked = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_send_after_unsub",
                "command_id": "cmd_send_after_unsub",
                "module": "outbound",
                "command_type": "outbound.message.send_approved",
                "record_id": "msg_after_unsub",
                "status": "pending_sync",
                "payload": { "message_id": "msg_after_unsub" },
                "client_context": actor.clone()
            }),
        )
        .expect_err("send to a suppressed recipient must be blocked");
        assert!(
            blocked.to_string().contains("suppressed"),
            "expected suppression block, got: {blocked}"
        );

        // Idempotent: a second unsubscribe reply must not create a duplicate entry.
        let res2 = accept_rxdb_business_command(
            root,
            serde_json::json!({
                "id": "cmd_reply_unsub_2",
                "command_id": "cmd_reply_unsub_2",
                "module": "outbound",
                "command_type": "outbound.reply.match",
                "record_id": "eng_unsub",
                "status": "pending_sync",
                "payload": {
                    "engagement_id": "eng_unsub",
                    "reply_message_id": "email:inbox/unsub-2",
                    "classification": "unsubscribe"
                },
                "client_context": actor.clone()
            }),
        )?;
        assert!(
            res2.pointer("/result/suppression_id")
                .map(Value::is_null)
                .unwrap_or(true),
            "second unsubscribe must be a no-op, got {res2:?}"
        );

        Ok(())
    }
}

/// Markiert alle Quellen-/Adapter-Records dieser Quelle als angemeldet.
///
/// Der Login-Rueckweg: nach "Ich bin angemeldet" bekommt jeder business_record,
/// der diese `source_id` beschreibt, `auth_status = session_authenticated` --
/// in der generischen Sammlung `outbound_research_adapters` UND im Modul-
/// Zwilling `outbound_lead_generation_adapters`, den die Oberflaeche wirklich liest.
/// Damit schliesst sich zugleich die Zwei-Wahrheiten-Luecke: der Server, der
/// bisher nur die generische Sammlung schrieb, spiegelt die Wahrheit hier in
/// beide. Gibt die Zahl der geaenderten Records zurueck.
pub(super) fn outbound_mark_source_authenticated(
    root: &Path,
    source_id: &str,
) -> anyhow::Result<usize> {
    let source = source_id.trim();
    anyhow::ensure!(!source.is_empty(), "source_id is required");
    let conn = open_store(root)?;
    let now = now_ms() as i64;
    let mut touched = 0usize;
    for collection in [
        "outbound_research_adapters",
        "outbound_lead_generation_adapters",
    ] {
        let mut rows = conn.prepare(
            "SELECT record_id, payload_json FROM business_records
             WHERE collection = ?1 AND deleted = 0",
        )?;
        let matches: Vec<(String, Value)> = rows
            .query_map([collection], |row| {
                let id: String = row.get(0)?;
                let raw: String = row.get(1)?;
                Ok((id, raw))
            })?
            .filter_map(Result::ok)
            .filter_map(|(id, raw)| {
                serde_json::from_str::<Value>(&raw)
                    .ok()
                    .map(|doc| (id, doc))
            })
            .filter(|(_, doc)| {
                let sid = doc
                    .get("source_id")
                    .or_else(|| doc.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                sid == source
            })
            .collect();
        drop(rows);
        for (record_id, mut doc) in matches {
            if let Some(object) = doc.as_object_mut() {
                object.insert(
                    "auth_status".to_string(),
                    Value::String("session_authenticated".to_string()),
                );
                object.insert("auth_authenticated_at_ms".to_string(), Value::from(now));
            }
            upsert_business_record(&conn, collection, &record_id, now, doc)?;
            touched += 1;
        }
    }
    Ok(touched)
}
