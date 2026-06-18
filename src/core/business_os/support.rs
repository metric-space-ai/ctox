// Origin: CTOX
// License: Apache-2.0

use super::policy::BusinessOsPermission;
use super::store::{self, BusinessCommand, BusinessOsSession};
use crate::mission::channels;
use anyhow::Context;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde_json::json;
use serde_json::Value;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::Path;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use uuid::Uuid;

#[derive(Debug, Clone)]
struct ProjectionRef {
    collection: &'static str,
    record_id: String,
}

#[derive(Debug, Clone)]
struct SupportConversation {
    id: String,
    inbox_id: String,
    primary_thread_key: String,
    status: String,
    priority: String,
    assignee_id: String,
    team_id: String,
    customer_account_id: String,
    customer_contact_id: String,
    ticket_case_id: String,
    last_message_key: String,
    last_activity_at_ms: i64,
    waiting_since_ms: i64,
    snoozed_until_ms: i64,
    unread_count: i64,
    label_ids: Value,
    custom_attributes: Value,
    search_text: String,
    created_at_ms: i64,
    updated_at_ms: i64,
}

pub(super) fn migrate(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS support_conversations (
            conversation_id TEXT PRIMARY KEY,
            inbox_id TEXT NOT NULL DEFAULT '',
            primary_thread_key TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'open',
            priority TEXT NOT NULL DEFAULT 'normal',
            assignee_id TEXT NOT NULL DEFAULT '',
            team_id TEXT NOT NULL DEFAULT '',
            customer_account_id TEXT NOT NULL DEFAULT '',
            customer_contact_id TEXT NOT NULL DEFAULT '',
            ticket_case_id TEXT NOT NULL DEFAULT '',
            last_message_key TEXT NOT NULL DEFAULT '',
            last_activity_at_ms INTEGER NOT NULL DEFAULT 0,
            waiting_since_ms INTEGER NOT NULL DEFAULT 0,
            snoozed_until_ms INTEGER NOT NULL DEFAULT 0,
            unread_count INTEGER NOT NULL DEFAULT 0,
            label_ids_json TEXT NOT NULL DEFAULT '[]',
            custom_attributes_json TEXT NOT NULL DEFAULT '{}',
            search_text TEXT NOT NULL DEFAULT '',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_support_conversations_status_activity
            ON support_conversations(status, last_activity_at_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_support_conversations_assignee_activity
            ON support_conversations(assignee_id, last_activity_at_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_support_conversations_customer
            ON support_conversations(customer_account_id, customer_contact_id);

        CREATE TABLE IF NOT EXISTS support_inboxes (
            inbox_id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'active',
            channel_filters_json TEXT NOT NULL DEFAULT '{}',
            team_id TEXT NOT NULL DEFAULT '',
            assignment_policy_id TEXT NOT NULL DEFAULT '',
            sla_policy_id TEXT NOT NULL DEFAULT '',
            policy_json TEXT NOT NULL DEFAULT '{}',
            is_default INTEGER NOT NULL DEFAULT 0,
            sort_key TEXT NOT NULL DEFAULT '',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_support_inboxes_status_team
            ON support_inboxes(status, team_id, is_default);

        CREATE TABLE IF NOT EXISTS support_thread_links (
            thread_link_id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL,
            thread_key TEXT NOT NULL,
            channel TEXT NOT NULL DEFAULT '',
            account_key TEXT NOT NULL DEFAULT '',
            link_role TEXT NOT NULL DEFAULT 'primary',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_support_thread_links_thread
            ON support_thread_links(thread_key, link_role);
        CREATE INDEX IF NOT EXISTS idx_support_thread_links_conversation
            ON support_thread_links(conversation_id, updated_at_ms DESC);

        CREATE TABLE IF NOT EXISTS support_identity_links (
            identity_link_id TEXT PRIMARY KEY,
            channel TEXT NOT NULL DEFAULT '',
            account_key TEXT NOT NULL DEFAULT '',
            external_identity TEXT NOT NULL DEFAULT '',
            normalized_identity TEXT NOT NULL DEFAULT '',
            customer_account_id TEXT NOT NULL DEFAULT '',
            customer_contact_id TEXT NOT NULL DEFAULT '',
            confidence REAL NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'active',
            source TEXT NOT NULL DEFAULT '',
            payload_json TEXT NOT NULL DEFAULT '{}',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_support_identity_links_identity
            ON support_identity_links(channel, normalized_identity, status);
        CREATE INDEX IF NOT EXISTS idx_support_identity_links_customer
            ON support_identity_links(customer_account_id, customer_contact_id);

        CREATE TABLE IF NOT EXISTS support_notes (
            note_id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL,
            author_id TEXT NOT NULL DEFAULT '',
            body TEXT NOT NULL,
            visibility TEXT NOT NULL DEFAULT 'internal',
            source TEXT NOT NULL DEFAULT '',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_support_notes_conversation
            ON support_notes(conversation_id, created_at_ms DESC);

        CREATE TABLE IF NOT EXISTS support_conversation_events (
            event_id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            actor_id TEXT NOT NULL DEFAULT '',
            source_command_id TEXT NOT NULL DEFAULT '',
            source_task_id TEXT NOT NULL DEFAULT '',
            summary TEXT NOT NULL DEFAULT '',
            payload_json TEXT NOT NULL DEFAULT '{}',
            occurred_at_ms INTEGER NOT NULL,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_support_conversation_events_conversation
            ON support_conversation_events(conversation_id, occurred_at_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_support_conversation_events_type
            ON support_conversation_events(event_type, occurred_at_ms DESC);

        CREATE TABLE IF NOT EXISTS support_assignment_events (
            event_id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL,
            policy_id TEXT NOT NULL DEFAULT '',
            assignee_id TEXT NOT NULL DEFAULT '',
            previous_assignee_id TEXT NOT NULL DEFAULT '',
            event_type TEXT NOT NULL,
            occurred_at_ms INTEGER NOT NULL,
            payload_json TEXT NOT NULL DEFAULT '{}',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_support_assignment_events_conversation
            ON support_assignment_events(conversation_id, occurred_at_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_support_assignment_events_assignee
            ON support_assignment_events(assignee_id, occurred_at_ms DESC);

        CREATE TABLE IF NOT EXISTS support_views (
            view_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            owner_id TEXT NOT NULL DEFAULT '',
            scope TEXT NOT NULL DEFAULT 'personal',
            position INTEGER NOT NULL DEFAULT 0,
            filters_json TEXT NOT NULL DEFAULT '{}',
            sorts_json TEXT NOT NULL DEFAULT '{}',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_support_views_owner_scope
            ON support_views(owner_id, scope, position);

        CREATE TABLE IF NOT EXISTS support_view_filters (
            filter_id TEXT PRIMARY KEY,
            view_id TEXT NOT NULL,
            field TEXT NOT NULL,
            operator TEXT NOT NULL,
            value_json TEXT NOT NULL DEFAULT '{}',
            position INTEGER NOT NULL DEFAULT 0,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_support_view_filters_view
            ON support_view_filters(view_id, position);

        CREATE TABLE IF NOT EXISTS support_assignment_policies (
            policy_id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            strategy TEXT NOT NULL DEFAULT 'manual',
            fair_distribution_limit INTEGER NOT NULL DEFAULT 0,
            fair_distribution_window_ms INTEGER NOT NULL DEFAULT 0,
            payload_json TEXT NOT NULL DEFAULT '{}',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_support_assignment_policies_strategy
            ON support_assignment_policies(strategy);

        CREATE TABLE IF NOT EXISTS support_macros (
            macro_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            visibility TEXT NOT NULL DEFAULT 'team',
            owner_id TEXT NOT NULL DEFAULT '',
            actions_json TEXT NOT NULL DEFAULT '[]',
            payload_json TEXT NOT NULL DEFAULT '{}',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_support_macros_visibility_owner
            ON support_macros(visibility, owner_id);

        CREATE TABLE IF NOT EXISTS support_automation_rules (
            rule_id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            event_name TEXT NOT NULL,
            active INTEGER NOT NULL DEFAULT 1,
            query_operator TEXT NOT NULL DEFAULT 'all',
            conditions_json TEXT NOT NULL DEFAULT '[]',
            actions_json TEXT NOT NULL DEFAULT '[]',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_support_automation_rules_event_active
            ON support_automation_rules(event_name, active);

        CREATE TABLE IF NOT EXISTS support_sla_policies (
            policy_id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            active INTEGER NOT NULL DEFAULT 1,
            first_response_target_ms INTEGER NOT NULL DEFAULT 0,
            next_response_target_ms INTEGER NOT NULL DEFAULT 0,
            resolution_target_ms INTEGER NOT NULL DEFAULT 0,
            business_hours_json TEXT NOT NULL DEFAULT '{}',
            payload_json TEXT NOT NULL DEFAULT '{}',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_support_sla_policies_active
            ON support_sla_policies(active);

        CREATE TABLE IF NOT EXISTS support_applied_slas (
            applied_sla_id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL,
            policy_id TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'active',
            first_response_due_at_ms INTEGER NOT NULL DEFAULT 0,
            next_response_due_at_ms INTEGER NOT NULL DEFAULT 0,
            resolution_due_at_ms INTEGER NOT NULL DEFAULT 0,
            breached_at_ms INTEGER NOT NULL DEFAULT 0,
            payload_json TEXT NOT NULL DEFAULT '{}',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_support_applied_slas_conversation_policy
            ON support_applied_slas(conversation_id, policy_id);
        CREATE INDEX IF NOT EXISTS idx_support_applied_slas_status_resolution
            ON support_applied_slas(status, resolution_due_at_ms);

        CREATE TABLE IF NOT EXISTS support_sla_events (
            event_id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL,
            applied_sla_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            occurred_at_ms INTEGER NOT NULL,
            payload_json TEXT NOT NULL DEFAULT '{}',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_support_sla_events_conversation
            ON support_sla_events(conversation_id, occurred_at_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_support_sla_events_applied
            ON support_sla_events(applied_sla_id, occurred_at_ms DESC);

        CREATE TABLE IF NOT EXISTS support_agent_suggestions (
            suggestion_id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL,
            source_command_id TEXT NOT NULL DEFAULT '',
            task_id TEXT NOT NULL DEFAULT '',
            suggestion_kind TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'proposed',
            confidence REAL NOT NULL DEFAULT 0,
            required_human_action TEXT NOT NULL DEFAULT 'review',
            summary TEXT NOT NULL DEFAULT '',
            payload_json TEXT NOT NULL DEFAULT '{}',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_support_agent_suggestions_conversation
            ON support_agent_suggestions(conversation_id, updated_at_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_support_agent_suggestions_source
            ON support_agent_suggestions(source_command_id, task_id);

        CREATE TABLE IF NOT EXISTS support_reporting_events (
            event_id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL DEFAULT '',
            event_name TEXT NOT NULL,
            metric_name TEXT NOT NULL DEFAULT 'count',
            value_ms INTEGER NOT NULL DEFAULT 0,
            occurred_at_ms INTEGER NOT NULL,
            payload_json TEXT NOT NULL DEFAULT '{}',
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_support_reporting_events_event
            ON support_reporting_events(event_name, occurred_at_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_support_reporting_events_conversation
            ON support_reporting_events(conversation_id, occurred_at_ms DESC);

        CREATE TABLE IF NOT EXISTS support_reporting_rollups (
            rollup_id TEXT PRIMARY KEY,
            rollup_key TEXT NOT NULL,
            bucket_start_ms INTEGER NOT NULL,
            bucket_end_ms INTEGER NOT NULL,
            metric_name TEXT NOT NULL,
            dimensions_json TEXT NOT NULL DEFAULT '{}',
            value REAL NOT NULL DEFAULT 0,
            count INTEGER NOT NULL DEFAULT 0,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_support_reporting_rollups_key
            ON support_reporting_rollups(rollup_key, bucket_start_ms, metric_name);
        ",
    )?;
    Ok(())
}

pub(super) fn is_support_command(command_type: &str) -> bool {
    matches!(
        command_type,
        "support.inbox.upsert"
            | "support.conversation.open_from_thread"
            | "support.conversation.claim"
            | "support.conversation.assign"
            | "support.conversation.status"
            | "support.conversation.priority"
            | "support.conversation.snooze"
            | "support.conversation.resolve"
            | "support.conversation.reopen"
            | "support.identity.link"
            | "support.note.create"
            | "support.ticket.link"
            | "support.ticket.create_from_conversation"
            | "support.reply.draft"
            | "support.reply.send"
            | "support.view.upsert"
            | "support.view_filter.upsert"
            | "support.bulk.assign"
            | "support.bulk.status"
            | "support.bulk.priority"
            | "support.bulk.snooze"
            | "support.bulk.resolve"
            | "support.assignment_policy.upsert"
            | "support.macro.upsert"
            | "support.macro.run"
            | "support.automation_rule.upsert"
            | "support.automation.evaluate"
            | "support.sla_policy.upsert"
            | "support.sla.apply"
            | "support.sla.recalculate"
            | "support.reporting.rebuild_rollups"
            | "support.agent.writeback"
            | "support.agent.apply_suggestion"
            | "support.agent.reject_suggestion"
    )
}

pub(super) fn command_permission(command_type: &str) -> BusinessOsPermission {
    match command_type {
        "support.inbox.upsert" => BusinessOsPermission::SupportManageInboxes,
        "support.conversation.assign" | "support.bulk.assign" => {
            BusinessOsPermission::SupportAssign
        }
        "support.conversation.resolve" | "support.bulk.resolve" => {
            BusinessOsPermission::SupportResolve
        }
        "support.reply.draft" | "support.reply.send" => BusinessOsPermission::SupportReply,
        "support.view.upsert" | "support.view_filter.upsert" => BusinessOsPermission::SupportTriage,
        "support.bulk.status" | "support.bulk.priority" | "support.bulk.snooze" => {
            BusinessOsPermission::SupportTriage
        }
        "support.assignment_policy.upsert" => BusinessOsPermission::SupportAssign,
        "support.macro.upsert" => BusinessOsPermission::SupportManageMacros,
        "support.macro.run" => BusinessOsPermission::SupportTriage,
        "support.automation_rule.upsert" => BusinessOsPermission::SupportManageInboxes,
        "support.automation.evaluate" => BusinessOsPermission::SupportTriage,
        "support.sla_policy.upsert" | "support.sla.apply" | "support.sla.recalculate" => {
            BusinessOsPermission::SupportManageSla
        }
        "support.reporting.rebuild_rollups" => BusinessOsPermission::SupportManageInboxes,
        "support.agent.writeback" => BusinessOsPermission::SupportAgentRequest,
        "support.agent.apply_suggestion" | "support.agent.reject_suggestion" => {
            BusinessOsPermission::SupportAgentApply
        }
        _ => BusinessOsPermission::SupportTriage,
    }
}

pub(super) fn handle_business_command(
    root: &Path,
    session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let conn = store::open_store(root)?;
    let now = now_ms();
    let command_id = command.id.as_deref().context("command id is required")?;
    let actor_id = actor_id(session);
    let mut projections = Vec::new();

    let mut result = match command.command_type.as_str() {
        "support.inbox.upsert" => {
            let inbox_id = first_string_field(&command.payload, &["id", "inbox_id"])
                .unwrap_or_else(|| format!("support_inbox_{}", Uuid::new_v4()));
            upsert_inbox(&conn, &inbox_id, &command.payload, now)?;
            project_inbox(&conn, &inbox_id, &mut projections)?;
            json!({ "inbox_id": inbox_id })
        }
        "support.conversation.open_from_thread" => {
            let conversation_id = command_conversation_id(command)?;
            upsert_conversation_from_payload(&conn, &conversation_id, &command.payload, now)?;
            let conversation = load_conversation_required(&conn, &conversation_id)?;
            apply_sla_for_conversation_if_available(&conn, &conversation, now, &mut projections)?;
            project_conversation(&conn, &conversation, &mut projections)?;
            if !conversation.primary_thread_key.is_empty() {
                upsert_thread_link(
                    &conn,
                    &conversation.id,
                    &conversation.primary_thread_key,
                    first_string_field(&command.payload, &["channel", "inbound_channel"])
                        .unwrap_or_default()
                        .as_str(),
                    first_string_field(&command.payload, &["account_key"])
                        .unwrap_or_default()
                        .as_str(),
                    "primary",
                    now,
                    &mut projections,
                )?;
            }
            insert_conversation_event(
                &conn,
                &conversation.id,
                "support.conversation.opened",
                actor_id.as_str(),
                command_id,
                "",
                "Support conversation opened from an inbound thread.",
                json!({ "payload": command.payload }),
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation.id, "status": conversation.status })
        }
        "support.conversation.claim" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            let next_assignee_id =
                first_string_field(&command.payload, &["assignee_id", "user_id"])
                    .unwrap_or_else(|| actor_id.clone());
            let force = bool_field(&command.payload, &["force"]).unwrap_or(false);
            let previous_assignee_id =
                claim_conversation_atomic(&conn, &conversation_id, &next_assignee_id, force, now)?;
            let conversation = load_conversation_required(&conn, &conversation_id)?;
            project_conversation(&conn, &conversation, &mut projections)?;
            insert_assignment_event(
                &conn,
                &conversation_id,
                &next_assignee_id,
                &previous_assignee_id,
                "support.assignment.claimed",
                json!({ "source_command_id": command_id, "force": force }),
                now,
                &mut projections,
            )?;
            insert_conversation_event(
                &conn,
                &conversation_id,
                "support.conversation.claimed",
                actor_id.as_str(),
                command_id,
                "",
                "Support conversation claimed.",
                json!({
                    "assignee_id": next_assignee_id,
                    "previous_assignee_id": previous_assignee_id
                }),
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "assignee_id": conversation.assignee_id })
        }
        "support.conversation.assign" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            let previous = load_conversation_required(&conn, &conversation_id)?;
            let assignee_id = first_string_field(&command.payload, &["assignee_id", "user_id"]);
            let team_id = first_string_field(&command.payload, &["team_id"]);
            anyhow::ensure!(
                assignee_id
                    .as_deref()
                    .is_some_and(|value| !value.is_empty())
                    || team_id.as_deref().is_some_and(|value| !value.is_empty()),
                "assignee_id or team_id is required"
            );
            update_conversation_fields(
                &conn,
                &conversation_id,
                ConversationPatch {
                    assignee_id: assignee_id.clone(),
                    team_id: team_id.clone(),
                    last_activity_at_ms: Some(now),
                    ..Default::default()
                },
                now,
            )?;
            let conversation = load_conversation_required(&conn, &conversation_id)?;
            project_conversation(&conn, &conversation, &mut projections)?;
            insert_assignment_event(
                &conn,
                &conversation_id,
                assignee_id
                    .as_deref()
                    .unwrap_or(conversation.assignee_id.as_str()),
                &previous.assignee_id,
                "support.assignment.assigned",
                json!({ "team_id": team_id, "source_command_id": command_id }),
                now,
                &mut projections,
            )?;
            insert_conversation_event(
                &conn,
                &conversation_id,
                "support.conversation.assigned",
                actor_id.as_str(),
                command_id,
                "",
                "Support conversation assigned.",
                json!({
                    "assignee_id": conversation.assignee_id,
                    "team_id": conversation.team_id,
                    "previous_assignee_id": previous.assignee_id
                }),
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "assignee_id": conversation.assignee_id, "team_id": conversation.team_id })
        }
        "support.conversation.status" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            let status = normalize_status(
                first_string_field(&command.payload, &["status"])
                    .context("status is required")?
                    .as_str(),
            );
            update_conversation_status(
                &conn,
                &conversation_id,
                &status,
                command_id,
                actor_id.as_str(),
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "status": status })
        }
        "support.conversation.priority" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            let priority = normalize_priority(
                first_string_field(&command.payload, &["priority"])
                    .context("priority is required")?
                    .as_str(),
            );
            update_conversation_fields(
                &conn,
                &conversation_id,
                ConversationPatch {
                    priority: Some(priority.clone()),
                    last_activity_at_ms: Some(now),
                    ..Default::default()
                },
                now,
            )?;
            let conversation = load_conversation_required(&conn, &conversation_id)?;
            project_conversation(&conn, &conversation, &mut projections)?;
            insert_conversation_event(
                &conn,
                &conversation_id,
                "support.conversation.priority_changed",
                actor_id.as_str(),
                command_id,
                "",
                "Support conversation priority changed.",
                json!({ "priority": priority }),
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "priority": priority })
        }
        "support.conversation.snooze" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            let snoozed_until_ms =
                number_field(&command.payload, &["snoozed_until_ms", "until_ms"])
                    .context("snoozed_until_ms is required")?;
            update_conversation_fields(
                &conn,
                &conversation_id,
                ConversationPatch {
                    status: Some("snoozed".to_owned()),
                    snoozed_until_ms: Some(snoozed_until_ms),
                    last_activity_at_ms: Some(now),
                    ..Default::default()
                },
                now,
            )?;
            let conversation = load_conversation_required(&conn, &conversation_id)?;
            project_conversation(&conn, &conversation, &mut projections)?;
            insert_conversation_event(
                &conn,
                &conversation_id,
                "support.conversation.snoozed",
                actor_id.as_str(),
                command_id,
                "",
                "Support conversation snoozed.",
                json!({ "snoozed_until_ms": snoozed_until_ms }),
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "snoozed_until_ms": snoozed_until_ms })
        }
        "support.conversation.resolve" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            ensure_resolve_requirements(&conn, &conversation_id, &command.payload)?;
            update_conversation_status(
                &conn,
                &conversation_id,
                "resolved",
                command_id,
                actor_id.as_str(),
                now,
                &mut projections,
            )?;
            mark_applied_slas_resolved(&conn, &conversation_id, now, &mut projections)?;
            json!({ "conversation_id": conversation_id, "status": "resolved" })
        }
        "support.conversation.reopen" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            update_conversation_status(
                &conn,
                &conversation_id,
                "open",
                command_id,
                actor_id.as_str(),
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "status": "open" })
        }
        "support.identity.link" => {
            let identity_link_id =
                first_string_field(&command.payload, &["id", "identity_link_id"])
                    .unwrap_or_else(|| format!("support_identity_{}", Uuid::new_v4()));
            let conversation_id = first_string_field(&command.payload, &["conversation_id"])
                .or_else(|| command.record_id.clone())
                .unwrap_or_default();
            upsert_identity_link(&conn, &identity_link_id, &command.payload, now)?;
            project_identity_link(&conn, &identity_link_id, &mut projections)?;
            if !conversation_id.is_empty() {
                ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
                update_conversation_fields(
                    &conn,
                    &conversation_id,
                    ConversationPatch {
                        customer_account_id: first_string_field(
                            &command.payload,
                            &["customer_account_id"],
                        ),
                        customer_contact_id: first_string_field(
                            &command.payload,
                            &["customer_contact_id"],
                        ),
                        last_activity_at_ms: Some(now),
                        ..Default::default()
                    },
                    now,
                )?;
                let conversation = load_conversation_required(&conn, &conversation_id)?;
                project_conversation(&conn, &conversation, &mut projections)?;
                insert_conversation_event(
                    &conn,
                    &conversation_id,
                    "support.identity.linked",
                    actor_id.as_str(),
                    command_id,
                    "",
                    "Support identity linked.",
                    json!({ "identity_link_id": identity_link_id }),
                    now,
                    &mut projections,
                )?;
            }
            json!({ "identity_link_id": identity_link_id, "conversation_id": conversation_id })
        }
        "support.note.create" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            let note_id = first_string_field(&command.payload, &["id", "note_id"])
                .unwrap_or_else(|| format!("support_note_{}", Uuid::new_v4()));
            let body = first_string_field(&command.payload, &["body", "text", "note"])
                .context("body is required")?;
            let visibility = normalize_visibility(
                first_string_field(&command.payload, &["visibility"])
                    .unwrap_or_else(|| "internal".to_owned())
                    .as_str(),
            );
            insert_note(
                &conn,
                &note_id,
                &conversation_id,
                actor_id.as_str(),
                body.as_str(),
                visibility.as_str(),
                first_string_field(&command.payload, &["source"])
                    .unwrap_or_else(|| "business-os.support".to_owned())
                    .as_str(),
                now,
            )?;
            update_conversation_fields(
                &conn,
                &conversation_id,
                ConversationPatch {
                    last_activity_at_ms: Some(now),
                    ..Default::default()
                },
                now,
            )?;
            project_note(&conn, &note_id, &mut projections)?;
            let conversation = load_conversation_required(&conn, &conversation_id)?;
            project_conversation(&conn, &conversation, &mut projections)?;
            insert_conversation_event(
                &conn,
                &conversation_id,
                "support.note.created",
                actor_id.as_str(),
                command_id,
                "",
                "Internal support note created.",
                json!({ "note_id": note_id, "visibility": visibility }),
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "note_id": note_id })
        }
        "support.ticket.link" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            let ticket_case_id = first_string_field(
                &command.payload,
                &["ticket_case_id", "case_id", "ticket_key"],
            )
            .context("ticket_case_id is required")?;
            update_conversation_fields(
                &conn,
                &conversation_id,
                ConversationPatch {
                    ticket_case_id: Some(ticket_case_id.clone()),
                    last_activity_at_ms: Some(now),
                    ..Default::default()
                },
                now,
            )?;
            let conversation = load_conversation_required(&conn, &conversation_id)?;
            project_conversation(&conn, &conversation, &mut projections)?;
            insert_conversation_event(
                &conn,
                &conversation_id,
                "support.ticket.linked",
                actor_id.as_str(),
                command_id,
                "",
                "Support conversation linked to a CTOX ticket.",
                json!({ "ticket_case_id": ticket_case_id }),
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "ticket_case_id": ticket_case_id })
        }
        "support.ticket.create_from_conversation" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            let conversation = load_conversation_required(&conn, &conversation_id)?;
            let title = first_string_field(&command.payload, &["title"]).unwrap_or_else(|| {
                if conversation.search_text.trim().is_empty() {
                    format!("Support conversation {}", conversation.id)
                } else {
                    trim_to_chars(conversation.search_text.as_str(), 120)
                }
            });
            let body =
                first_string_field(&command.payload, &["body", "summary"]).unwrap_or_else(|| {
                    format!(
                        "Created from Business OS Support conversation `{}`.",
                        conversation.id
                    )
                });
            let ticket_outcome = crate::mission::tickets::run_business_os_ticket_command(
                root,
                "ctox.ticket.local.create",
                &json!({
                    "title": title,
                    "body": body,
                    "status": "open",
                    "priority": conversation.priority
                }),
            )?;
            let ticket_case_id = ticket_outcome
                .get("case_id")
                .or_else(|| ticket_outcome.get("ticket_key"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            anyhow::ensure!(
                !ticket_case_id.is_empty(),
                "ticket outcome is missing case_id"
            );
            update_conversation_fields(
                &conn,
                &conversation_id,
                ConversationPatch {
                    ticket_case_id: Some(ticket_case_id.clone()),
                    last_activity_at_ms: Some(now),
                    ..Default::default()
                },
                now,
            )?;
            let conversation = load_conversation_required(&conn, &conversation_id)?;
            project_conversation(&conn, &conversation, &mut projections)?;
            insert_conversation_event(
                &conn,
                &conversation_id,
                "support.ticket.created",
                actor_id.as_str(),
                command_id,
                "",
                "CTOX ticket created from Support conversation.",
                json!({ "ticket_case_id": ticket_case_id, "ticket_outcome": ticket_outcome }),
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "ticket_case_id": ticket_case_id })
        }
        "support.reply.draft" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            let body = first_string_field(&command.payload, &["body", "text", "draft"])
                .context("draft body is required")?;
            let suggestion_id = first_string_field(&command.payload, &["id", "suggestion_id"])
                .unwrap_or_else(|| format!("support_suggestion_{}", Uuid::new_v4()));
            upsert_agent_suggestion(
                &conn,
                &suggestion_id,
                &conversation_id,
                command_id,
                "",
                "draft_reply",
                "draft",
                1.0,
                "human_send_required",
                trim_to_chars(body.as_str(), 180).as_str(),
                json!({ "body": body, "source": "support.reply.draft" }),
                now,
            )?;
            project_agent_suggestion(&conn, &suggestion_id, &mut projections)?;
            insert_conversation_event(
                &conn,
                &conversation_id,
                "support.reply.draft_created",
                actor_id.as_str(),
                command_id,
                "",
                "Support reply draft created.",
                json!({ "suggestion_id": suggestion_id }),
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "suggestion_id": suggestion_id, "status": "draft" })
        }
        "support.reply.send" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            let send_mode = first_string_field(&command.payload, &["send_mode", "mode"])
                .unwrap_or_else(|| "approval_required".to_owned());
            anyhow::ensure!(
                send_mode != "direct",
                "support.reply.send direct mode requires a configured channel send gateway"
            );
            let body = first_string_field(&command.payload, &["body", "text", "reply"])
                .context("reply body is required")?;
            let approval_id = first_string_field(&command.payload, &["approval_id"])
                .unwrap_or_else(|| format!("support_reply_approval_{}", Uuid::new_v4()));
            let attachment_refs = attachment_refs_from_payload(&command.payload);
            insert_conversation_event(
                &conn,
                &conversation_id,
                "support.reply.pending_approval",
                actor_id.as_str(),
                command_id,
                "",
                "Support reply queued for human approval.",
                json!({
                    "approval_id": approval_id,
                    "body": body,
                    "attachment_refs": attachment_refs,
                    "channel": first_string_field(&command.payload, &["channel"]).unwrap_or_default(),
                    "thread_key": first_string_field(&command.payload, &["thread_key"]).unwrap_or_default()
                }),
                now,
                &mut projections,
            )?;
            insert_reporting_event(
                &conn,
                &conversation_id,
                "support.reply.pending_approval",
                "count",
                1,
                json!({ "approval_id": approval_id }),
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "approval_id": approval_id, "status": "pending_approval" })
        }
        "support.view.upsert" => {
            let view_id = first_string_field(&command.payload, &["id", "view_id"])
                .unwrap_or_else(|| format!("support_view_{}", Uuid::new_v4()));
            upsert_view(&conn, &view_id, &command.payload, actor_id.as_str(), now)?;
            project_view(&conn, &view_id, &mut projections)?;
            json!({ "view_id": view_id })
        }
        "support.view_filter.upsert" => {
            let filter_id = first_string_field(&command.payload, &["id", "filter_id"])
                .unwrap_or_else(|| format!("support_view_filter_{}", Uuid::new_v4()));
            upsert_view_filter(&conn, &filter_id, &command.payload, now)?;
            project_view_filter(&conn, &filter_id, &mut projections)?;
            json!({ "filter_id": filter_id })
        }
        "support.bulk.assign" => {
            let conversation_ids = command_conversation_ids(command)?;
            let assignee_id = first_string_field(&command.payload, &["assignee_id", "user_id"]);
            let team_id = first_string_field(&command.payload, &["team_id"]);
            anyhow::ensure!(
                assignee_id
                    .as_deref()
                    .is_some_and(|value| !value.is_empty())
                    || team_id.as_deref().is_some_and(|value| !value.is_empty()),
                "assignee_id or team_id is required"
            );
            for conversation_id in &conversation_ids {
                ensure_conversation(&conn, conversation_id, &command.payload, now)?;
                let previous = load_conversation_required(&conn, conversation_id)?;
                update_conversation_fields(
                    &conn,
                    conversation_id,
                    ConversationPatch {
                        assignee_id: assignee_id.clone(),
                        team_id: team_id.clone(),
                        last_activity_at_ms: Some(now),
                        ..Default::default()
                    },
                    now,
                )?;
                let conversation = load_conversation_required(&conn, conversation_id)?;
                project_conversation(&conn, &conversation, &mut projections)?;
                insert_assignment_event(
                    &conn,
                    conversation_id,
                    assignee_id
                        .as_deref()
                        .unwrap_or(conversation.assignee_id.as_str()),
                    previous.assignee_id.as_str(),
                    "support.assignment.bulk_assigned",
                    json!({ "team_id": team_id, "source_command_id": command_id }),
                    now,
                    &mut projections,
                )?;
            }
            json!({ "conversation_ids": conversation_ids, "updated": conversation_ids.len() })
        }
        "support.bulk.status" => {
            let conversation_ids = command_conversation_ids(command)?;
            let status = normalize_status(
                first_string_field(&command.payload, &["status"])
                    .context("status is required")?
                    .as_str(),
            );
            for conversation_id in &conversation_ids {
                ensure_conversation(&conn, conversation_id, &command.payload, now)?;
                if status == "resolved" {
                    ensure_resolve_requirements(&conn, conversation_id, &command.payload)?;
                }
                update_conversation_status(
                    &conn,
                    conversation_id,
                    status.as_str(),
                    command_id,
                    actor_id.as_str(),
                    now,
                    &mut projections,
                )?;
                if status == "resolved" {
                    mark_applied_slas_resolved(&conn, conversation_id, now, &mut projections)?;
                }
            }
            json!({ "conversation_ids": conversation_ids, "status": status, "updated": conversation_ids.len() })
        }
        "support.bulk.priority" => {
            let conversation_ids = command_conversation_ids(command)?;
            let priority = normalize_priority(
                first_string_field(&command.payload, &["priority"])
                    .context("priority is required")?
                    .as_str(),
            );
            for conversation_id in &conversation_ids {
                ensure_conversation(&conn, conversation_id, &command.payload, now)?;
                update_conversation_fields(
                    &conn,
                    conversation_id,
                    ConversationPatch {
                        priority: Some(priority.clone()),
                        last_activity_at_ms: Some(now),
                        ..Default::default()
                    },
                    now,
                )?;
                let conversation = load_conversation_required(&conn, conversation_id)?;
                project_conversation(&conn, &conversation, &mut projections)?;
            }
            json!({ "conversation_ids": conversation_ids, "priority": priority, "updated": conversation_ids.len() })
        }
        "support.bulk.snooze" => {
            let conversation_ids = command_conversation_ids(command)?;
            let snoozed_until_ms =
                number_field(&command.payload, &["snoozed_until_ms", "until_ms"])
                    .context("snoozed_until_ms is required")?;
            for conversation_id in &conversation_ids {
                ensure_conversation(&conn, conversation_id, &command.payload, now)?;
                update_conversation_fields(
                    &conn,
                    conversation_id,
                    ConversationPatch {
                        status: Some("snoozed".to_owned()),
                        snoozed_until_ms: Some(snoozed_until_ms),
                        last_activity_at_ms: Some(now),
                        ..Default::default()
                    },
                    now,
                )?;
                let conversation = load_conversation_required(&conn, conversation_id)?;
                project_conversation(&conn, &conversation, &mut projections)?;
            }
            json!({ "conversation_ids": conversation_ids, "snoozed_until_ms": snoozed_until_ms, "updated": conversation_ids.len() })
        }
        "support.bulk.resolve" => {
            let conversation_ids = command_conversation_ids(command)?;
            for conversation_id in &conversation_ids {
                ensure_conversation(&conn, conversation_id, &command.payload, now)?;
                ensure_resolve_requirements(&conn, conversation_id, &command.payload)?;
                update_conversation_status(
                    &conn,
                    conversation_id,
                    "resolved",
                    command_id,
                    actor_id.as_str(),
                    now,
                    &mut projections,
                )?;
                mark_applied_slas_resolved(&conn, conversation_id, now, &mut projections)?;
            }
            json!({ "conversation_ids": conversation_ids, "status": "resolved", "updated": conversation_ids.len() })
        }
        "support.assignment_policy.upsert" => {
            let policy_id = first_string_field(&command.payload, &["id", "policy_id"])
                .unwrap_or_else(|| format!("support_assignment_policy_{}", Uuid::new_v4()));
            upsert_assignment_policy(&conn, &policy_id, &command.payload, now)?;
            project_assignment_policy(&conn, &policy_id, &mut projections)?;
            json!({ "policy_id": policy_id })
        }
        "support.macro.upsert" => {
            let macro_id = first_string_field(&command.payload, &["id", "macro_id"])
                .unwrap_or_else(|| format!("support_macro_{}", Uuid::new_v4()));
            upsert_macro(&conn, &macro_id, &command.payload, actor_id.as_str(), now)?;
            project_macro(&conn, &macro_id, &mut projections)?;
            json!({ "macro_id": macro_id })
        }
        "support.macro.run" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            let macro_id = first_string_field(&command.payload, &["macro_id", "id"])
                .context("macro_id is required")?;
            let actions = load_macro_actions(&conn, &macro_id)?;
            let applied = apply_support_actions(
                &conn,
                &conversation_id,
                &actions,
                ActionContext {
                    actor_id: actor_id.as_str(),
                    command_id,
                    source_label: "support.macro.run",
                    now,
                },
                &mut projections,
            )?;
            project_macro(&conn, &macro_id, &mut projections)?;
            insert_conversation_event(
                &conn,
                &conversation_id,
                "support.macro.ran",
                actor_id.as_str(),
                command_id,
                "",
                "Support macro applied.",
                json!({ "macro_id": macro_id, "applied_actions": applied }),
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "macro_id": macro_id, "applied_actions": applied })
        }
        "support.automation_rule.upsert" => {
            let rule_id = first_string_field(&command.payload, &["id", "rule_id"])
                .unwrap_or_else(|| format!("support_automation_rule_{}", Uuid::new_v4()));
            upsert_automation_rule(&conn, &rule_id, &command.payload, now)?;
            project_automation_rule(&conn, &rule_id, &mut projections)?;
            json!({ "rule_id": rule_id })
        }
        "support.automation.evaluate" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            let event_name = first_string_field(&command.payload, &["event_name"])
                .unwrap_or_else(|| "manual".to_owned());
            let matches = evaluate_automation_rules(
                &conn,
                &conversation_id,
                event_name.as_str(),
                ActionContext {
                    actor_id: actor_id.as_str(),
                    command_id,
                    source_label: "support.automation.evaluate",
                    now,
                },
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "event_name": event_name, "matched_rules": matches })
        }
        "support.sla_policy.upsert" => {
            let policy_id = first_string_field(&command.payload, &["id", "policy_id"])
                .unwrap_or_else(|| format!("support_sla_policy_{}", Uuid::new_v4()));
            upsert_sla_policy(&conn, &policy_id, &command.payload, now)?;
            project_sla_policy(&conn, &policy_id, &mut projections)?;
            json!({ "policy_id": policy_id })
        }
        "support.sla.apply" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            let conversation = load_conversation_required(&conn, &conversation_id)?;
            let policy_id = first_string_field(&command.payload, &["policy_id"])
                .or_else(|| {
                    resolve_sla_policy_for_conversation(&conn, &conversation)
                        .ok()
                        .flatten()
                })
                .context("policy_id is required")?;
            let started_at_ms =
                number_field(&command.payload, &["started_at_ms"]).unwrap_or_else(|| {
                    if conversation.waiting_since_ms > 0 {
                        conversation.waiting_since_ms
                    } else {
                        conversation.created_at_ms
                    }
                });
            let applied_sla_id = apply_sla_policy(
                &conn,
                &conversation,
                policy_id.as_str(),
                started_at_ms,
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "policy_id": policy_id, "applied_sla_id": applied_sla_id })
        }
        "support.sla.recalculate" => {
            let conversation_id = first_string_field(&command.payload, &["conversation_id"])
                .or_else(|| command.record_id.clone());
            let recalculated =
                recalculate_slas(&conn, conversation_id.as_deref(), now, &mut projections)?;
            json!({ "conversation_id": conversation_id.unwrap_or_default(), "recalculated": recalculated })
        }
        "support.reporting.rebuild_rollups" => {
            let rebuilt = rebuild_reporting_rollups(&conn, now, &mut projections)?;
            json!({ "rebuilt": rebuilt })
        }
        "support.agent.writeback" => {
            let conversation_id = command_conversation_id(command)?;
            ensure_conversation(&conn, &conversation_id, &command.payload, now)?;
            let suggestion_id = first_string_field(&command.payload, &["id", "suggestion_id"])
                .unwrap_or_else(|| format!("support_suggestion_{}", Uuid::new_v4()));
            let source_command_id =
                first_string_field(&command.payload, &["source_command_id"]).unwrap_or_default();
            let task_id = first_string_field(&command.payload, &["task_id"]).unwrap_or_default();
            let suggestion_kind =
                first_string_field(&command.payload, &["suggestion_kind", "kind"])
                    .unwrap_or_else(|| "summary".to_owned());
            let confidence = number_field(&command.payload, &["confidence"]).unwrap_or(0);
            let required_human_action =
                first_string_field(&command.payload, &["required_human_action"])
                    .unwrap_or_else(|| "review".to_owned());
            let summary = first_string_field(&command.payload, &["summary"]).unwrap_or_default();
            let payload = command.payload.get("payload").cloned().unwrap_or_else(|| {
                json!({
                    "raw_payload": command.payload
                })
            });
            upsert_agent_suggestion(
                &conn,
                &suggestion_id,
                &conversation_id,
                source_command_id.as_str(),
                task_id.as_str(),
                suggestion_kind.as_str(),
                "proposed",
                confidence as f64,
                required_human_action.as_str(),
                summary.as_str(),
                payload,
                now,
            )?;
            project_agent_suggestion(&conn, &suggestion_id, &mut projections)?;
            insert_conversation_event(
                &conn,
                &conversation_id,
                "support.agent.writeback",
                actor_id.as_str(),
                command_id,
                task_id.as_str(),
                "CTOX Agent wrote a structured Support suggestion.",
                json!({ "suggestion_id": suggestion_id, "suggestion_kind": suggestion_kind }),
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "suggestion_id": suggestion_id, "status": "proposed" })
        }
        "support.agent.apply_suggestion" | "support.agent.reject_suggestion" => {
            let suggestion_id = first_string_field(&command.payload, &["suggestion_id", "id"])
                .context("suggestion_id is required")?;
            let status = if command.command_type == "support.agent.apply_suggestion" {
                "applied"
            } else {
                "rejected"
            };
            conn.execute(
                "UPDATE support_agent_suggestions
                 SET status = ?2, updated_at_ms = ?3
                 WHERE suggestion_id = ?1",
                params![suggestion_id, status, now],
            )?;
            let conversation_id = load_agent_suggestion_conversation_id(&conn, &suggestion_id)?
                .or_else(|| first_string_field(&command.payload, &["conversation_id"]))
                .context("support agent suggestion does not exist")?;
            project_agent_suggestion(&conn, &suggestion_id, &mut projections)?;
            insert_conversation_event(
                &conn,
                &conversation_id,
                if status == "applied" {
                    "support.agent.suggestion_applied"
                } else {
                    "support.agent.suggestion_rejected"
                },
                actor_id.as_str(),
                command_id,
                first_string_field(&command.payload, &["task_id"])
                    .unwrap_or_default()
                    .as_str(),
                if status == "applied" {
                    "Support agent suggestion applied."
                } else {
                    "Support agent suggestion rejected."
                },
                json!({ "suggestion_id": suggestion_id, "status": status }),
                now,
                &mut projections,
            )?;
            json!({ "conversation_id": conversation_id, "suggestion_id": suggestion_id, "status": status })
        }
        other => anyhow::bail!("unsupported Support command `{other}`"),
    };

    if let Some(obj) = result.as_object_mut() {
        obj.insert("ok".to_owned(), Value::Bool(true));
        obj.insert(
            "command_type".to_owned(),
            Value::String(command.command_type.clone()),
        );
        obj.insert("projections".to_owned(), projection_payload(&projections));
    }
    Ok(result)
}

pub(super) fn project_communication_intake(root: &Path, limit: usize) -> anyhow::Result<usize> {
    let threads = channels::pull_communication_threads_for_business_os(root, Some(0), Some(limit))
        .context("pull communication thread projection for support intake")?
        .get("documents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if threads.is_empty() {
        return Ok(0);
    }
    let messages =
        channels::pull_communication_messages_for_business_os(root, Some(0), Some(limit))
            .context("pull communication message projection for support intake")?
            .get("documents")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
    let mut inbound_by_thread: HashMap<String, (String, i64, i64)> = HashMap::new();
    for message in messages {
        if message.get("direction").and_then(Value::as_str) != Some("inbound") {
            continue;
        }
        let Some(thread_key) = first_string_field(&message, &["thread_key"]) else {
            continue;
        };
        if is_support_internal_thread_key(&thread_key) {
            continue;
        }
        let message_key = first_string_field(&message, &["message_key", "id"]).unwrap_or_default();
        let updated_at_ms = number_field(&message, &["updated_at_ms"]).unwrap_or(0);
        let entry = inbound_by_thread
            .entry(thread_key)
            .or_insert_with(|| (String::new(), 0, 0));
        entry.2 += 1;
        if updated_at_ms >= entry.1 {
            entry.0 = message_key;
            entry.1 = updated_at_ms;
        }
    }
    if inbound_by_thread.is_empty() {
        return Ok(0);
    }

    let conn = store::open_store(root)?;
    let now = now_ms();
    let mut projected = 0usize;
    for thread in threads {
        let Some(thread_key) = first_string_field(&thread, &["thread_key", "id"]) else {
            continue;
        };
        if is_support_internal_thread_key(&thread_key) {
            continue;
        }
        let Some((latest_inbound_message_key, latest_inbound_at_ms, inbound_count)) =
            inbound_by_thread.get(&thread_key).cloned()
        else {
            continue;
        };
        let existing_conversation_id = linked_conversation_for_thread(&conn, &thread_key)?;
        let conversation_id = existing_conversation_id.unwrap_or_else(|| {
            format!(
                "support_conv_{}",
                Uuid::new_v5(&Uuid::NAMESPACE_URL, thread_key.as_bytes())
            )
        });
        let last_message_key = first_string_field(&thread, &["last_message_key"])
            .filter(|value| !value.is_empty())
            .unwrap_or(latest_inbound_message_key);
        let last_activity_at_ms = number_field(&thread, &["updated_at_ms", "last_message_at_ms"])
            .unwrap_or(latest_inbound_at_ms.max(now));
        let unread_count = number_field(&thread, &["unread_count"]).unwrap_or(inbound_count);
        let search_text = first_string_field(&thread, &["subject"])
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| thread_key.clone());
        upsert_conversation_from_payload(
            &conn,
            &conversation_id,
            &json!({
                "conversation_id": conversation_id,
                "thread_key": thread_key,
                "channel": first_string_field(&thread, &["channel"]).unwrap_or_default(),
                "account_key": first_string_field(&thread, &["account_key"]).unwrap_or_default(),
                "last_message_key": last_message_key,
                "last_activity_at_ms": last_activity_at_ms,
                "unread_count": unread_count,
                "search_text": search_text
            }),
            now,
        )?;
        let conversation = load_conversation_required(&conn, &conversation_id)?;
        let mut projections = Vec::new();
        project_conversation(&conn, &conversation, &mut projections)?;
        if linked_conversation_for_thread(&conn, &thread_key)?.is_none() {
            insert_conversation_event(
                &conn,
                &conversation_id,
                "support.intake.communication_thread",
                "ctox",
                "",
                "",
                "Communication thread imported into Support.",
                json!({
                    "thread_key": thread_key,
                    "last_message_key": last_message_key
                }),
                now,
                &mut projections,
            )?;
        }
        upsert_thread_link(
            &conn,
            &conversation_id,
            &thread_key,
            first_string_field(&thread, &["channel"])
                .unwrap_or_default()
                .as_str(),
            first_string_field(&thread, &["account_key"])
                .unwrap_or_default()
                .as_str(),
            "primary",
            now,
            &mut projections,
        )?;
        projected += projections.len();
    }
    Ok(projected)
}

fn is_support_internal_thread_key(thread_key: &str) -> bool {
    thread_key.trim().starts_with("business-os/support/")
}

fn upsert_conversation_from_payload(
    conn: &Connection,
    conversation_id: &str,
    payload: &Value,
    now: i64,
) -> anyhow::Result<()> {
    let existing = load_conversation(conn, conversation_id)?;
    let created_at_ms = existing
        .as_ref()
        .map(|row| row.created_at_ms)
        .unwrap_or(now);
    let inbox_id = first_string_field(payload, &["inbox_id"])
        .or_else(|| existing.as_ref().map(|row| row.inbox_id.clone()))
        .unwrap_or_default();
    let primary_thread_key = first_string_field(payload, &["primary_thread_key", "thread_key"])
        .or_else(|| existing.as_ref().map(|row| row.primary_thread_key.clone()))
        .unwrap_or_else(|| conversation_id.to_owned());
    let status = first_string_field(payload, &["status"])
        .map(|value| normalize_status(value.as_str()))
        .or_else(|| existing.as_ref().map(|row| row.status.clone()))
        .unwrap_or_else(|| "open".to_owned());
    let priority = first_string_field(payload, &["priority"])
        .map(|value| normalize_priority(value.as_str()))
        .or_else(|| existing.as_ref().map(|row| row.priority.clone()))
        .unwrap_or_else(|| "normal".to_owned());
    let assignee_id = first_string_field(payload, &["assignee_id"])
        .or_else(|| existing.as_ref().map(|row| row.assignee_id.clone()))
        .unwrap_or_default();
    let team_id = first_string_field(payload, &["team_id"])
        .or_else(|| existing.as_ref().map(|row| row.team_id.clone()))
        .unwrap_or_default();
    let customer_account_id = first_string_field(payload, &["customer_account_id"])
        .or_else(|| existing.as_ref().map(|row| row.customer_account_id.clone()))
        .unwrap_or_default();
    let customer_contact_id = first_string_field(payload, &["customer_contact_id"])
        .or_else(|| existing.as_ref().map(|row| row.customer_contact_id.clone()))
        .unwrap_or_default();
    let ticket_case_id = first_string_field(payload, &["ticket_case_id", "case_id", "ticket_key"])
        .or_else(|| existing.as_ref().map(|row| row.ticket_case_id.clone()))
        .unwrap_or_default();
    let last_message_key = first_string_field(payload, &["last_message_key", "message_key"])
        .or_else(|| existing.as_ref().map(|row| row.last_message_key.clone()))
        .unwrap_or_default();
    let last_activity_at_ms = number_field(payload, &["last_activity_at_ms", "occurred_at_ms"])
        .or_else(|| existing.as_ref().map(|row| row.last_activity_at_ms))
        .unwrap_or(now);
    let waiting_since_ms = number_field(payload, &["waiting_since_ms"])
        .or_else(|| existing.as_ref().map(|row| row.waiting_since_ms))
        .unwrap_or(0);
    let snoozed_until_ms = number_field(payload, &["snoozed_until_ms"])
        .or_else(|| existing.as_ref().map(|row| row.snoozed_until_ms))
        .unwrap_or(0);
    let unread_count = number_field(payload, &["unread_count"])
        .or_else(|| existing.as_ref().map(|row| row.unread_count))
        .unwrap_or(0);
    let label_ids = payload
        .get("label_ids")
        .cloned()
        .or_else(|| existing.as_ref().map(|row| row.label_ids.clone()))
        .unwrap_or_else(|| json!([]));
    let custom_attributes = payload
        .get("custom_attributes")
        .cloned()
        .or_else(|| existing.as_ref().map(|row| row.custom_attributes.clone()))
        .unwrap_or_else(|| json!({}));
    let search_text = first_string_field(payload, &["search_text", "title", "summary", "subject"])
        .or_else(|| existing.as_ref().map(|row| row.search_text.clone()))
        .unwrap_or_else(|| conversation_id.to_owned());
    conn.execute(
        "INSERT INTO support_conversations
            (conversation_id, inbox_id, primary_thread_key, status, priority, assignee_id,
             team_id, customer_account_id, customer_contact_id, ticket_case_id, last_message_key,
             last_activity_at_ms, waiting_since_ms, snoozed_until_ms, unread_count,
             label_ids_json, custom_attributes_json, search_text, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
         ON CONFLICT(conversation_id) DO UPDATE SET
            inbox_id = excluded.inbox_id,
            primary_thread_key = excluded.primary_thread_key,
            status = excluded.status,
            priority = excluded.priority,
            assignee_id = excluded.assignee_id,
            team_id = excluded.team_id,
            customer_account_id = excluded.customer_account_id,
            customer_contact_id = excluded.customer_contact_id,
            ticket_case_id = excluded.ticket_case_id,
            last_message_key = excluded.last_message_key,
            last_activity_at_ms = excluded.last_activity_at_ms,
            waiting_since_ms = excluded.waiting_since_ms,
            snoozed_until_ms = excluded.snoozed_until_ms,
            unread_count = excluded.unread_count,
            label_ids_json = excluded.label_ids_json,
            custom_attributes_json = excluded.custom_attributes_json,
            search_text = excluded.search_text,
            updated_at_ms = excluded.updated_at_ms",
        params![
            conversation_id,
            inbox_id,
            primary_thread_key,
            status,
            priority,
            assignee_id,
            team_id,
            customer_account_id,
            customer_contact_id,
            ticket_case_id,
            last_message_key,
            last_activity_at_ms,
            waiting_since_ms,
            snoozed_until_ms,
            unread_count,
            serde_json::to_string(&label_ids)?,
            serde_json::to_string(&custom_attributes)?,
            search_text,
            created_at_ms,
            now
        ],
    )?;
    Ok(())
}

fn ensure_conversation(
    conn: &Connection,
    conversation_id: &str,
    payload: &Value,
    now: i64,
) -> anyhow::Result<()> {
    if load_conversation(conn, conversation_id)?.is_some() {
        return Ok(());
    }
    upsert_conversation_from_payload(conn, conversation_id, payload, now)
}

fn upsert_inbox(
    conn: &Connection,
    inbox_id: &str,
    payload: &Value,
    now: i64,
) -> anyhow::Result<()> {
    let created_at_ms =
        current_created_at(conn, "support_inboxes", "inbox_id", inbox_id)?.unwrap_or(now);
    let name = first_string_field(payload, &["name", "title"])
        .unwrap_or_else(|| "Support Inbox".to_owned());
    let description = first_string_field(payload, &["description"]).unwrap_or_default();
    let status = first_string_field(payload, &["status"]).unwrap_or_else(|| "active".to_owned());
    let team_id = first_string_field(payload, &["team_id"]).unwrap_or_default();
    let assignment_policy_id =
        first_string_field(payload, &["assignment_policy_id"]).unwrap_or_default();
    let sla_policy_id = first_string_field(payload, &["sla_policy_id"]).unwrap_or_default();
    let channel_filters = json_field(payload, &["channel_filters_json", "channel_filters"])
        .unwrap_or_else(|| json!({}));
    let policy = json_field(payload, &["policy_json", "policy"]).unwrap_or_else(|| json!({}));
    let is_default = bool_field(payload, &["is_default"]).unwrap_or(false);
    let sort_key = first_string_field(payload, &["sort_key"]).unwrap_or_else(|| name.clone());
    conn.execute(
        "INSERT INTO support_inboxes
            (inbox_id, name, description, status, channel_filters_json, team_id,
             assignment_policy_id, sla_policy_id, policy_json, is_default, sort_key,
             created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(inbox_id) DO UPDATE SET
            name = excluded.name,
            description = excluded.description,
            status = excluded.status,
            channel_filters_json = excluded.channel_filters_json,
            team_id = excluded.team_id,
            assignment_policy_id = excluded.assignment_policy_id,
            sla_policy_id = excluded.sla_policy_id,
            policy_json = excluded.policy_json,
            is_default = excluded.is_default,
            sort_key = excluded.sort_key,
            updated_at_ms = excluded.updated_at_ms",
        params![
            inbox_id,
            name,
            description,
            status,
            serde_json::to_string(&channel_filters)?,
            team_id,
            assignment_policy_id,
            sla_policy_id,
            serde_json::to_string(&policy)?,
            if is_default { 1 } else { 0 },
            sort_key,
            created_at_ms,
            now
        ],
    )?;
    Ok(())
}

fn upsert_assignment_policy(
    conn: &Connection,
    policy_id: &str,
    payload: &Value,
    now: i64,
) -> anyhow::Result<()> {
    let created_at_ms =
        current_created_at(conn, "support_assignment_policies", "policy_id", policy_id)?
            .unwrap_or(now);
    let name = first_string_field(payload, &["name", "title"])
        .unwrap_or_else(|| "Support assignment policy".to_owned());
    let strategy =
        first_string_field(payload, &["strategy"]).unwrap_or_else(|| "manual".to_owned());
    let fair_distribution_limit = number_field(payload, &["fair_distribution_limit"]).unwrap_or(0);
    let fair_distribution_window_ms =
        number_field(payload, &["fair_distribution_window_ms"]).unwrap_or(0);
    let policy_payload = json_field(payload, &["payload"]).unwrap_or_else(|| json!({}));
    conn.execute(
        "INSERT INTO support_assignment_policies
            (policy_id, name, strategy, fair_distribution_limit, fair_distribution_window_ms,
             payload_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(policy_id) DO UPDATE SET
            name = excluded.name,
            strategy = excluded.strategy,
            fair_distribution_limit = excluded.fair_distribution_limit,
            fair_distribution_window_ms = excluded.fair_distribution_window_ms,
            payload_json = excluded.payload_json,
            updated_at_ms = excluded.updated_at_ms",
        params![
            policy_id,
            name,
            strategy,
            fair_distribution_limit,
            fair_distribution_window_ms,
            serde_json::to_string(&policy_payload)?,
            created_at_ms,
            now
        ],
    )?;
    Ok(())
}

fn upsert_macro(
    conn: &Connection,
    macro_id: &str,
    payload: &Value,
    actor_id: &str,
    now: i64,
) -> anyhow::Result<()> {
    let created_at_ms =
        current_created_at(conn, "support_macros", "macro_id", macro_id)?.unwrap_or(now);
    let title =
        first_string_field(payload, &["title", "name"]).context("macro title is required")?;
    let visibility =
        first_string_field(payload, &["visibility"]).unwrap_or_else(|| "team".to_owned());
    let owner_id =
        first_string_field(payload, &["owner_id"]).unwrap_or_else(|| actor_id.to_owned());
    let actions = json_field(payload, &["actions_json", "actions"]).unwrap_or_else(|| json!([]));
    anyhow::ensure!(actions.is_array(), "macro actions must be an array");
    let macro_payload = json_field(payload, &["payload"]).unwrap_or_else(|| json!({}));
    conn.execute(
        "INSERT INTO support_macros
            (macro_id, title, visibility, owner_id, actions_json, payload_json,
             created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(macro_id) DO UPDATE SET
            title = excluded.title,
            visibility = excluded.visibility,
            owner_id = excluded.owner_id,
            actions_json = excluded.actions_json,
            payload_json = excluded.payload_json,
            updated_at_ms = excluded.updated_at_ms",
        params![
            macro_id,
            title,
            visibility,
            owner_id,
            serde_json::to_string(&actions)?,
            serde_json::to_string(&macro_payload)?,
            created_at_ms,
            now
        ],
    )?;
    Ok(())
}

fn upsert_automation_rule(
    conn: &Connection,
    rule_id: &str,
    payload: &Value,
    now: i64,
) -> anyhow::Result<()> {
    let created_at_ms =
        current_created_at(conn, "support_automation_rules", "rule_id", rule_id)?.unwrap_or(now);
    let name = first_string_field(payload, &["name", "title"])
        .context("automation rule name is required")?;
    let event_name =
        first_string_field(payload, &["event_name"]).context("event_name is required")?;
    let active = bool_field(payload, &["active"]).unwrap_or(true);
    let query_operator =
        first_string_field(payload, &["query_operator"]).unwrap_or_else(|| "all".to_owned());
    let conditions =
        json_field(payload, &["conditions_json", "conditions"]).unwrap_or_else(|| json!([]));
    let actions = json_field(payload, &["actions_json", "actions"]).unwrap_or_else(|| json!([]));
    anyhow::ensure!(
        conditions.is_array(),
        "automation conditions must be an array"
    );
    anyhow::ensure!(actions.is_array(), "automation actions must be an array");
    conn.execute(
        "INSERT INTO support_automation_rules
            (rule_id, name, event_name, active, query_operator, conditions_json,
             actions_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(rule_id) DO UPDATE SET
            name = excluded.name,
            event_name = excluded.event_name,
            active = excluded.active,
            query_operator = excluded.query_operator,
            conditions_json = excluded.conditions_json,
            actions_json = excluded.actions_json,
            updated_at_ms = excluded.updated_at_ms",
        params![
            rule_id,
            name,
            event_name,
            if active { 1 } else { 0 },
            query_operator,
            serde_json::to_string(&conditions)?,
            serde_json::to_string(&actions)?,
            created_at_ms,
            now
        ],
    )?;
    Ok(())
}

fn upsert_sla_policy(
    conn: &Connection,
    policy_id: &str,
    payload: &Value,
    now: i64,
) -> anyhow::Result<()> {
    let created_at_ms =
        current_created_at(conn, "support_sla_policies", "policy_id", policy_id)?.unwrap_or(now);
    let name =
        first_string_field(payload, &["name", "title"]).context("SLA policy name is required")?;
    let active = bool_field(payload, &["active"]).unwrap_or(true);
    let business_hours = json_field(payload, &["business_hours_json", "business_hours"])
        .unwrap_or_else(|| json!({}));
    let policy_payload = json_field(payload, &["payload"]).unwrap_or_else(|| json!({}));
    conn.execute(
        "INSERT INTO support_sla_policies
            (policy_id, name, active, first_response_target_ms, next_response_target_ms,
             resolution_target_ms, business_hours_json, payload_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(policy_id) DO UPDATE SET
            name = excluded.name,
            active = excluded.active,
            first_response_target_ms = excluded.first_response_target_ms,
            next_response_target_ms = excluded.next_response_target_ms,
            resolution_target_ms = excluded.resolution_target_ms,
            business_hours_json = excluded.business_hours_json,
            payload_json = excluded.payload_json,
            updated_at_ms = excluded.updated_at_ms",
        params![
            policy_id,
            name,
            if active { 1 } else { 0 },
            number_field(payload, &["first_response_target_ms"]).unwrap_or(0),
            number_field(payload, &["next_response_target_ms"]).unwrap_or(0),
            number_field(payload, &["resolution_target_ms"]).unwrap_or(0),
            serde_json::to_string(&business_hours)?,
            serde_json::to_string(&policy_payload)?,
            created_at_ms,
            now
        ],
    )?;
    Ok(())
}

fn upsert_view(
    conn: &Connection,
    view_id: &str,
    payload: &Value,
    actor_id: &str,
    now: i64,
) -> anyhow::Result<()> {
    let created_at_ms =
        current_created_at(conn, "support_views", "view_id", view_id)?.unwrap_or(now);
    let title = first_string_field(payload, &["title", "name"])
        .context("support view title is required")?;
    let owner_id =
        first_string_field(payload, &["owner_id"]).unwrap_or_else(|| actor_id.to_owned());
    let scope = first_string_field(payload, &["scope"]).unwrap_or_else(|| "personal".to_owned());
    let position = number_field(payload, &["position"]).unwrap_or(0);
    let filters = json_field(payload, &["filters_json", "filters"]).unwrap_or_else(|| json!({}));
    let sorts = json_field(payload, &["sorts_json", "sorts"]).unwrap_or_else(|| json!({}));
    conn.execute(
        "INSERT INTO support_views
            (view_id, title, owner_id, scope, position, filters_json, sorts_json,
             created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(view_id) DO UPDATE SET
            title = excluded.title,
            owner_id = excluded.owner_id,
            scope = excluded.scope,
            position = excluded.position,
            filters_json = excluded.filters_json,
            sorts_json = excluded.sorts_json,
            updated_at_ms = excluded.updated_at_ms",
        params![
            view_id,
            title,
            owner_id,
            scope,
            position,
            serde_json::to_string(&filters)?,
            serde_json::to_string(&sorts)?,
            created_at_ms,
            now
        ],
    )?;
    Ok(())
}

fn upsert_view_filter(
    conn: &Connection,
    filter_id: &str,
    payload: &Value,
    now: i64,
) -> anyhow::Result<()> {
    let created_at_ms =
        current_created_at(conn, "support_view_filters", "filter_id", filter_id)?.unwrap_or(now);
    let view_id = first_string_field(payload, &["view_id"]).context("view_id is required")?;
    let field = first_string_field(payload, &["field"]).context("filter field is required")?;
    let operator = first_string_field(payload, &["operator"]).unwrap_or_else(|| "eq".to_owned());
    let value = json_field(payload, &["value"]).unwrap_or(Value::Null);
    let position = number_field(payload, &["position"]).unwrap_or(0);
    conn.execute(
        "INSERT INTO support_view_filters
            (filter_id, view_id, field, operator, value_json, position, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(filter_id) DO UPDATE SET
            view_id = excluded.view_id,
            field = excluded.field,
            operator = excluded.operator,
            value_json = excluded.value_json,
            position = excluded.position,
            updated_at_ms = excluded.updated_at_ms",
        params![
            filter_id,
            view_id,
            field,
            operator,
            serde_json::to_string(&value)?,
            position,
            created_at_ms,
            now
        ],
    )?;
    Ok(())
}

fn attachment_refs_from_payload(payload: &Value) -> Value {
    let refs = string_array_field(payload, &["attachment_file_ids", "file_ids"])
        .into_iter()
        .map(|file_id| {
            json!({
                "file_id": file_id,
                "file_collection": "desktop_files",
                "chunk_collection": "desktop_file_chunks"
            })
        })
        .collect::<Vec<_>>();
    Value::Array(refs)
}

fn rebuild_reporting_rollups(
    conn: &Connection,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<usize> {
    let mut stmt = conn.prepare(
        "SELECT event_name, metric_name, occurred_at_ms, value_ms
         FROM support_reporting_events
         ORDER BY occurred_at_ms ASC",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut buckets: HashMap<(String, String, i64), (f64, i64)> = HashMap::new();
    for (event_name, metric_name, occurred_at_ms, value_ms) in rows {
        let bucket_start_ms = day_bucket_start_ms(occurred_at_ms);
        let entry = buckets
            .entry((event_name, metric_name, bucket_start_ms))
            .or_insert((0.0, 0));
        entry.0 += value_ms as f64;
        entry.1 += 1;
    }
    for ((event_name, metric_name, bucket_start_ms), (value, count)) in &buckets {
        let bucket_end_ms = bucket_start_ms.saturating_add(86_400_000);
        let rollup_key = format!("support:{event_name}:{metric_name}");
        let rollup_id = format!(
            "support_rollup_{}",
            Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("{rollup_key}:{bucket_start_ms}").as_bytes()
            )
        );
        conn.execute(
            "INSERT INTO support_reporting_rollups
                (rollup_id, rollup_key, bucket_start_ms, bucket_end_ms, metric_name,
                 dimensions_json, value, count, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(rollup_key, bucket_start_ms, metric_name) DO UPDATE SET
                bucket_end_ms = excluded.bucket_end_ms,
                dimensions_json = excluded.dimensions_json,
                value = excluded.value,
                count = excluded.count,
                updated_at_ms = excluded.updated_at_ms",
            params![
                rollup_id,
                rollup_key,
                bucket_start_ms,
                bucket_end_ms,
                metric_name,
                serde_json::to_string(&json!({ "event_name": event_name }))?,
                value,
                count,
                now,
                now
            ],
        )?;
        project_reporting_rollup(conn, &rollup_id, projections)?;
    }
    Ok(buckets.len())
}

fn claim_conversation_atomic(
    conn: &Connection,
    conversation_id: &str,
    next_assignee_id: &str,
    force: bool,
    now: i64,
) -> anyhow::Result<String> {
    let previous = load_conversation_required(conn, conversation_id)?.assignee_id;
    let changed = conn.execute(
        "UPDATE support_conversations
         SET assignee_id = ?2,
             last_activity_at_ms = ?3,
             updated_at_ms = ?3
         WHERE conversation_id = ?1
           AND (?4 = 1 OR assignee_id = '' OR assignee_id = ?2)",
        params![
            conversation_id,
            next_assignee_id,
            now,
            if force { 1 } else { 0 }
        ],
    )?;
    if changed == 0 {
        let current = load_conversation_required(conn, conversation_id)?.assignee_id;
        anyhow::bail!(
            "support conversation `{conversation_id}` is already assigned to `{current}`"
        );
    }
    Ok(previous)
}

fn ensure_resolve_requirements(
    conn: &Connection,
    conversation_id: &str,
    payload: &Value,
) -> anyhow::Result<()> {
    let conversation = load_conversation_required(conn, conversation_id)?;
    let mut required = string_array_field(payload, &["required_fields"]);
    if let Some(custom_required) = conversation
        .custom_attributes
        .get("resolve_required_fields")
        .and_then(Value::as_array)
    {
        for field in custom_required.iter().filter_map(Value::as_str) {
            required.push(field.to_owned());
        }
    }
    if !conversation.inbox_id.is_empty() {
        if let Some(inbox_policy) = load_inbox_policy(conn, &conversation.inbox_id)? {
            if let Some(inbox_required) = inbox_policy
                .get("resolve_required_fields")
                .and_then(Value::as_array)
            {
                for field in inbox_required.iter().filter_map(Value::as_str) {
                    required.push(field.to_owned());
                }
            }
        }
    }
    required.sort();
    required.dedup();
    let missing = required
        .into_iter()
        .filter(|field| conversation_required_field_missing(&conversation, field))
        .collect::<Vec<_>>();
    anyhow::ensure!(
        missing.is_empty(),
        "cannot resolve support conversation; missing required fields: {}",
        missing.join(", ")
    );
    Ok(())
}

fn conversation_required_field_missing(conversation: &SupportConversation, field: &str) -> bool {
    match field {
        "assignee_id" => conversation.assignee_id.is_empty(),
        "customer_account_id" => conversation.customer_account_id.is_empty(),
        "customer_contact_id" => conversation.customer_contact_id.is_empty(),
        "ticket_case_id" => conversation.ticket_case_id.is_empty(),
        "primary_thread_key" => conversation.primary_thread_key.is_empty(),
        "last_message_key" => conversation.last_message_key.is_empty(),
        "label_ids" => conversation
            .label_ids
            .as_array()
            .map(|items| items.is_empty())
            .unwrap_or(true),
        other => conversation
            .custom_attributes
            .get(other)
            .map(value_is_empty)
            .unwrap_or(true),
    }
}

#[derive(Clone, Copy)]
struct ActionContext<'a> {
    actor_id: &'a str,
    command_id: &'a str,
    source_label: &'a str,
    now: i64,
}

fn load_macro_actions(conn: &Connection, macro_id: &str) -> anyhow::Result<Value> {
    let actions_json: String = conn
        .query_row(
            "SELECT actions_json FROM support_macros WHERE macro_id = ?1",
            params![macro_id],
            |row| row.get(0),
        )
        .optional()?
        .with_context(|| format!("support macro `{macro_id}` does not exist"))?;
    serde_json::from_str(&actions_json).context("parse support macro actions")
}

fn apply_support_actions(
    conn: &Connection,
    conversation_id: &str,
    actions: &Value,
    ctx: ActionContext<'_>,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<Vec<Value>> {
    let action_items = actions
        .as_array()
        .context("support actions must be an array")?;
    let mut applied = Vec::new();
    for action in action_items {
        let action_type = first_string_field(action, &["type", "command_type", "action"])
            .context("support action type is required")?;
        let payload = action
            .get("payload")
            .cloned()
            .unwrap_or_else(|| action.clone());
        match action_type.as_str() {
            "support.note.create" | "note.create" => {
                let note_id = first_string_field(&payload, &["id", "note_id"])
                    .unwrap_or_else(|| format!("support_note_{}", Uuid::new_v4()));
                let body = first_string_field(&payload, &["body", "text", "note"])
                    .context("macro note body is required")?;
                let visibility = normalize_visibility(
                    first_string_field(&payload, &["visibility"])
                        .unwrap_or_else(|| "internal".to_owned())
                        .as_str(),
                );
                insert_note(
                    conn,
                    &note_id,
                    conversation_id,
                    ctx.actor_id,
                    body.as_str(),
                    visibility.as_str(),
                    ctx.source_label,
                    ctx.now,
                )?;
                project_note(conn, &note_id, projections)?;
                insert_conversation_event(
                    conn,
                    conversation_id,
                    "support.note.created",
                    ctx.actor_id,
                    ctx.command_id,
                    "",
                    "Internal support note created by automation.",
                    json!({ "note_id": note_id, "source": ctx.source_label }),
                    ctx.now,
                    projections,
                )?;
                applied.push(json!({ "type": action_type, "note_id": note_id }));
            }
            "support.conversation.status" | "status" => {
                let status = normalize_status(
                    first_string_field(&payload, &["status"])
                        .context("status action requires status")?
                        .as_str(),
                );
                if status == "resolved" {
                    ensure_resolve_requirements(conn, conversation_id, &payload)?;
                }
                update_conversation_status(
                    conn,
                    conversation_id,
                    status.as_str(),
                    ctx.command_id,
                    ctx.actor_id,
                    ctx.now,
                    projections,
                )?;
                if status == "resolved" {
                    mark_applied_slas_resolved(conn, conversation_id, ctx.now, projections)?;
                }
                applied.push(json!({ "type": action_type, "status": status }));
            }
            "support.conversation.priority" | "priority" => {
                let priority = normalize_priority(
                    first_string_field(&payload, &["priority"])
                        .context("priority action requires priority")?
                        .as_str(),
                );
                update_conversation_fields(
                    conn,
                    conversation_id,
                    ConversationPatch {
                        priority: Some(priority.clone()),
                        last_activity_at_ms: Some(ctx.now),
                        ..Default::default()
                    },
                    ctx.now,
                )?;
                let conversation = load_conversation_required(conn, conversation_id)?;
                project_conversation(conn, &conversation, projections)?;
                insert_conversation_event(
                    conn,
                    conversation_id,
                    "support.conversation.priority_changed",
                    ctx.actor_id,
                    ctx.command_id,
                    "",
                    "Support conversation priority changed by automation.",
                    json!({ "priority": priority, "source": ctx.source_label }),
                    ctx.now,
                    projections,
                )?;
                applied.push(json!({ "type": action_type, "priority": priority }));
            }
            "support.conversation.assign" | "assign" => {
                let previous = load_conversation_required(conn, conversation_id)?;
                let assignee_id = first_string_field(&payload, &["assignee_id", "user_id"]);
                let team_id = first_string_field(&payload, &["team_id"]);
                anyhow::ensure!(
                    assignee_id
                        .as_deref()
                        .is_some_and(|value| !value.is_empty())
                        || team_id.as_deref().is_some_and(|value| !value.is_empty()),
                    "assign action requires assignee_id or team_id"
                );
                update_conversation_fields(
                    conn,
                    conversation_id,
                    ConversationPatch {
                        assignee_id: assignee_id.clone(),
                        team_id: team_id.clone(),
                        last_activity_at_ms: Some(ctx.now),
                        ..Default::default()
                    },
                    ctx.now,
                )?;
                let conversation = load_conversation_required(conn, conversation_id)?;
                project_conversation(conn, &conversation, projections)?;
                insert_assignment_event(
                    conn,
                    conversation_id,
                    assignee_id
                        .as_deref()
                        .unwrap_or(conversation.assignee_id.as_str()),
                    previous.assignee_id.as_str(),
                    "support.assignment.automated",
                    json!({ "team_id": team_id, "source_command_id": ctx.command_id }),
                    ctx.now,
                    projections,
                )?;
                applied.push(json!({ "type": action_type, "assignee_id": conversation.assignee_id, "team_id": conversation.team_id }));
            }
            "support.conversation.snooze" | "snooze" => {
                let snoozed_until_ms = number_field(&payload, &["snoozed_until_ms", "until_ms"])
                    .context("snooze action requires snoozed_until_ms")?;
                update_conversation_fields(
                    conn,
                    conversation_id,
                    ConversationPatch {
                        status: Some("snoozed".to_owned()),
                        snoozed_until_ms: Some(snoozed_until_ms),
                        last_activity_at_ms: Some(ctx.now),
                        ..Default::default()
                    },
                    ctx.now,
                )?;
                let conversation = load_conversation_required(conn, conversation_id)?;
                project_conversation(conn, &conversation, projections)?;
                insert_conversation_event(
                    conn,
                    conversation_id,
                    "support.conversation.snoozed",
                    ctx.actor_id,
                    ctx.command_id,
                    "",
                    "Support conversation snoozed by automation.",
                    json!({ "snoozed_until_ms": snoozed_until_ms, "source": ctx.source_label }),
                    ctx.now,
                    projections,
                )?;
                applied.push(json!({ "type": action_type, "snoozed_until_ms": snoozed_until_ms }));
            }
            "support.ticket.link" | "ticket.link" => {
                let ticket_case_id =
                    first_string_field(&payload, &["ticket_case_id", "case_id", "ticket_key"])
                        .context("ticket link action requires ticket_case_id")?;
                update_conversation_fields(
                    conn,
                    conversation_id,
                    ConversationPatch {
                        ticket_case_id: Some(ticket_case_id.clone()),
                        last_activity_at_ms: Some(ctx.now),
                        ..Default::default()
                    },
                    ctx.now,
                )?;
                let conversation = load_conversation_required(conn, conversation_id)?;
                project_conversation(conn, &conversation, projections)?;
                insert_conversation_event(
                    conn,
                    conversation_id,
                    "support.ticket.linked",
                    ctx.actor_id,
                    ctx.command_id,
                    "",
                    "Support conversation linked to a CTOX ticket by automation.",
                    json!({ "ticket_case_id": ticket_case_id, "source": ctx.source_label }),
                    ctx.now,
                    projections,
                )?;
                applied.push(json!({ "type": action_type, "ticket_case_id": ticket_case_id }));
            }
            "support.reply.draft" | "reply.draft" => {
                let body = first_string_field(&payload, &["body", "text", "draft"])
                    .context("reply draft action requires body")?;
                let suggestion_id = first_string_field(&payload, &["id", "suggestion_id"])
                    .unwrap_or_else(|| format!("support_suggestion_{}", Uuid::new_v4()));
                upsert_agent_suggestion(
                    conn,
                    &suggestion_id,
                    conversation_id,
                    ctx.command_id,
                    "",
                    "draft_reply",
                    "draft",
                    1.0,
                    "human_send_required",
                    trim_to_chars(body.as_str(), 180).as_str(),
                    json!({ "body": body, "source": ctx.source_label }),
                    ctx.now,
                )?;
                project_agent_suggestion(conn, &suggestion_id, projections)?;
                applied.push(json!({ "type": action_type, "suggestion_id": suggestion_id }));
            }
            "support.sla.apply" | "sla.apply" => {
                let conversation = load_conversation_required(conn, conversation_id)?;
                let policy_id = first_string_field(&payload, &["policy_id"])
                    .or_else(|| {
                        resolve_sla_policy_for_conversation(conn, &conversation)
                            .ok()
                            .flatten()
                    })
                    .context("sla action requires policy_id")?;
                let started_at_ms =
                    number_field(&payload, &["started_at_ms"]).unwrap_or_else(|| {
                        if conversation.waiting_since_ms > 0 {
                            conversation.waiting_since_ms
                        } else {
                            conversation.created_at_ms
                        }
                    });
                let applied_sla_id = apply_sla_policy(
                    conn,
                    &conversation,
                    policy_id.as_str(),
                    started_at_ms,
                    ctx.now,
                    projections,
                )?;
                applied.push(json!({ "type": action_type, "policy_id": policy_id, "applied_sla_id": applied_sla_id }));
            }
            other => anyhow::bail!("unsupported closed Support action `{other}`"),
        }
    }
    Ok(applied)
}

fn evaluate_automation_rules(
    conn: &Connection,
    conversation_id: &str,
    event_name: &str,
    ctx: ActionContext<'_>,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<Vec<Value>> {
    let conversation = load_conversation_required(conn, conversation_id)?;
    let mut stmt = conn.prepare(
        "SELECT rule_id, query_operator, conditions_json, actions_json
         FROM support_automation_rules
         WHERE event_name = ?1 AND active = 1
         ORDER BY updated_at_ms ASC",
    )?;
    let rows = stmt
        .query_map(params![event_name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut matched = Vec::new();
    for (rule_id, query_operator, conditions_json, actions_json) in rows {
        let conditions: Value =
            serde_json::from_str(&conditions_json).unwrap_or_else(|_| json!([]));
        if !automation_conditions_match(&conversation, &conditions, query_operator.as_str()) {
            continue;
        }
        let actions: Value = serde_json::from_str(&actions_json).unwrap_or_else(|_| json!([]));
        let applied_actions =
            apply_support_actions(conn, conversation_id, &actions, ctx, projections)?;
        project_automation_rule(conn, &rule_id, projections)?;
        insert_conversation_event(
            conn,
            conversation_id,
            "support.automation.rule_matched",
            ctx.actor_id,
            ctx.command_id,
            "",
            "Support automation rule matched.",
            json!({
                "rule_id": rule_id,
                "event_name": event_name,
                "applied_actions": applied_actions
            }),
            ctx.now,
            projections,
        )?;
        matched.push(json!({ "rule_id": rule_id, "applied_actions": applied_actions }));
    }
    Ok(matched)
}

fn automation_conditions_match(
    conversation: &SupportConversation,
    conditions: &Value,
    query_operator: &str,
) -> bool {
    let Some(items) = conditions.as_array() else {
        return false;
    };
    if items.is_empty() {
        return true;
    }
    let matches = items
        .iter()
        .map(|condition| automation_condition_match(conversation, condition));
    if query_operator.eq_ignore_ascii_case("any") {
        matches.into_iter().any(|item| item)
    } else {
        matches.into_iter().all(|item| item)
    }
}

fn automation_condition_match(conversation: &SupportConversation, condition: &Value) -> bool {
    let Some(field) = first_string_field(condition, &["field"]) else {
        return false;
    };
    let operator = first_string_field(condition, &["operator"]).unwrap_or_else(|| "eq".to_owned());
    let actual = conversation_field_value(conversation, field.as_str());
    let expected = condition.get("value").cloned().unwrap_or(Value::Null);
    match operator.as_str() {
        "eq" | "equals" => json_scalar_eq(&actual, &expected),
        "neq" | "not_eq" | "not_equals" => !json_scalar_eq(&actual, &expected),
        "in" => expected
            .as_array()
            .map(|items| items.iter().any(|item| json_scalar_eq(&actual, item)))
            .unwrap_or(false),
        "contains" => json_contains(&actual, &expected),
        "empty" => value_is_empty(&actual),
        "not_empty" => !value_is_empty(&actual),
        _ => false,
    }
}

fn conversation_field_value(conversation: &SupportConversation, field: &str) -> Value {
    match field {
        "id" | "conversation_id" => Value::String(conversation.id.clone()),
        "inbox_id" => Value::String(conversation.inbox_id.clone()),
        "status" => Value::String(conversation.status.clone()),
        "priority" => Value::String(conversation.priority.clone()),
        "assignee_id" => Value::String(conversation.assignee_id.clone()),
        "team_id" => Value::String(conversation.team_id.clone()),
        "customer_account_id" => Value::String(conversation.customer_account_id.clone()),
        "customer_contact_id" => Value::String(conversation.customer_contact_id.clone()),
        "ticket_case_id" => Value::String(conversation.ticket_case_id.clone()),
        "last_message_key" => Value::String(conversation.last_message_key.clone()),
        "primary_thread_key" => Value::String(conversation.primary_thread_key.clone()),
        "label_ids" => conversation.label_ids.clone(),
        other => conversation
            .custom_attributes
            .get(other)
            .cloned()
            .unwrap_or(Value::Null),
    }
}

fn resolve_sla_policy_for_conversation(
    conn: &Connection,
    conversation: &SupportConversation,
) -> anyhow::Result<Option<String>> {
    if !conversation.inbox_id.is_empty() {
        if let Some(policy_id) = conn
            .query_row(
                "SELECT sla_policy_id FROM support_inboxes WHERE inbox_id = ?1",
                params![conversation.inbox_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .filter(|value| !value.is_empty())
        {
            return Ok(Some(policy_id));
        }
    }
    conn.query_row(
        "SELECT policy_id FROM support_sla_policies
         WHERE active = 1
         ORDER BY updated_at_ms DESC
         LIMIT 1",
        [],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn apply_sla_for_conversation_if_available(
    conn: &Connection,
    conversation: &SupportConversation,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<Option<String>> {
    let Some(policy_id) = resolve_sla_policy_for_conversation(conn, conversation)? else {
        return Ok(None);
    };
    let started_at_ms = if conversation.waiting_since_ms > 0 {
        conversation.waiting_since_ms
    } else {
        conversation.created_at_ms
    };
    apply_sla_policy(
        conn,
        conversation,
        policy_id.as_str(),
        started_at_ms,
        now,
        projections,
    )
    .map(Some)
}

fn apply_sla_policy(
    conn: &Connection,
    conversation: &SupportConversation,
    policy_id: &str,
    started_at_ms: i64,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<String> {
    let policy = load_sla_policy(conn, policy_id)?
        .with_context(|| format!("support SLA policy `{policy_id}` does not exist"))?;
    anyhow::ensure!(
        policy.active,
        "support SLA policy `{policy_id}` is inactive"
    );
    let applied_sla_id = format!(
        "support_applied_sla_{}",
        Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("{}:{policy_id}", conversation.id).as_bytes()
        )
    );
    let existing_breached_at_ms = load_applied_sla_breached_at(conn, &applied_sla_id)?;
    let first_response_due_at_ms = due_at(started_at_ms, policy.first_response_target_ms);
    let next_response_due_at_ms = due_at(started_at_ms, policy.next_response_target_ms);
    let resolution_due_at_ms = due_at(started_at_ms, policy.resolution_target_ms);
    let breached = sla_is_breached(
        now,
        first_response_due_at_ms,
        next_response_due_at_ms,
        resolution_due_at_ms,
    );
    let breached_at_ms = if breached {
        existing_breached_at_ms.unwrap_or(now)
    } else {
        0
    };
    let status = if conversation.status == "resolved" {
        "resolved"
    } else if breached {
        "breached"
    } else {
        "active"
    };
    conn.execute(
        "INSERT INTO support_applied_slas
            (applied_sla_id, conversation_id, policy_id, status, first_response_due_at_ms,
             next_response_due_at_ms, resolution_due_at_ms, breached_at_ms, payload_json,
             created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(applied_sla_id) DO UPDATE SET
            status = excluded.status,
            first_response_due_at_ms = excluded.first_response_due_at_ms,
            next_response_due_at_ms = excluded.next_response_due_at_ms,
            resolution_due_at_ms = excluded.resolution_due_at_ms,
            breached_at_ms = excluded.breached_at_ms,
            payload_json = excluded.payload_json,
            updated_at_ms = excluded.updated_at_ms",
        params![
            applied_sla_id,
            conversation.id,
            policy_id,
            status,
            first_response_due_at_ms,
            next_response_due_at_ms,
            resolution_due_at_ms,
            breached_at_ms,
            serde_json::to_string(&json!({
                "started_at_ms": started_at_ms,
                "source": "support.sla.apply"
            }))?,
            now,
            now
        ],
    )?;
    project_applied_sla(conn, &applied_sla_id, projections)?;
    insert_sla_event(
        conn,
        &conversation.id,
        &applied_sla_id,
        "support.sla.applied",
        json!({ "policy_id": policy_id }),
        now,
        projections,
    )?;
    if breached && existing_breached_at_ms.is_none() {
        insert_sla_event(
            conn,
            &conversation.id,
            &applied_sla_id,
            "support.sla.breached",
            json!({
                "policy_id": policy_id,
                "first_response_due_at_ms": first_response_due_at_ms,
                "next_response_due_at_ms": next_response_due_at_ms,
                "resolution_due_at_ms": resolution_due_at_ms
            }),
            now,
            projections,
        )?;
    }
    Ok(applied_sla_id)
}

fn recalculate_slas(
    conn: &Connection,
    conversation_id: Option<&str>,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<usize> {
    let mut sql = "SELECT applied_sla_id, conversation_id, first_response_due_at_ms,
                          next_response_due_at_ms, resolution_due_at_ms, breached_at_ms, status
                   FROM support_applied_slas"
        .to_owned();
    if conversation_id.is_some() {
        sql.push_str(" WHERE conversation_id = ?1");
    }
    let mut stmt = conn.prepare(&sql)?;
    let rows = if let Some(conversation_id) = conversation_id {
        stmt.query_map(params![conversation_id], applied_sla_recalc_row)?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map([], applied_sla_recalc_row)?
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut recalculated = 0usize;
    for row in rows {
        let breached = sla_is_breached(
            now,
            row.first_response_due_at_ms,
            row.next_response_due_at_ms,
            row.resolution_due_at_ms,
        );
        let next_status = if row.status == "resolved" {
            "resolved"
        } else if breached {
            "breached"
        } else {
            "active"
        };
        let next_breached_at_ms = if breached && row.breached_at_ms == 0 {
            now
        } else {
            row.breached_at_ms
        };
        conn.execute(
            "UPDATE support_applied_slas
             SET status = ?2, breached_at_ms = ?3, updated_at_ms = ?4
             WHERE applied_sla_id = ?1",
            params![row.applied_sla_id, next_status, next_breached_at_ms, now],
        )?;
        project_applied_sla(conn, row.applied_sla_id.as_str(), projections)?;
        if breached && row.breached_at_ms == 0 {
            insert_sla_event(
                conn,
                row.conversation_id.as_str(),
                row.applied_sla_id.as_str(),
                "support.sla.breached",
                json!({ "source": "support.sla.recalculate" }),
                now,
                projections,
            )?;
        }
        recalculated += 1;
    }
    Ok(recalculated)
}

fn mark_applied_slas_resolved(
    conn: &Connection,
    conversation_id: &str,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let mut stmt =
        conn.prepare("SELECT applied_sla_id FROM support_applied_slas WHERE conversation_id = ?1")?;
    let ids = stmt
        .query_map(params![conversation_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for applied_sla_id in ids {
        conn.execute(
            "UPDATE support_applied_slas
             SET status = 'resolved', updated_at_ms = ?2
             WHERE applied_sla_id = ?1",
            params![applied_sla_id, now],
        )?;
        project_applied_sla(conn, applied_sla_id.as_str(), projections)?;
        insert_sla_event(
            conn,
            conversation_id,
            applied_sla_id.as_str(),
            "support.sla.resolved",
            json!({ "source": "support.conversation.resolve" }),
            now,
            projections,
        )?;
    }
    Ok(())
}

#[derive(Debug)]
struct SlaPolicy {
    active: bool,
    first_response_target_ms: i64,
    next_response_target_ms: i64,
    resolution_target_ms: i64,
}

fn load_sla_policy(conn: &Connection, policy_id: &str) -> anyhow::Result<Option<SlaPolicy>> {
    conn.query_row(
        "SELECT active, first_response_target_ms, next_response_target_ms, resolution_target_ms
         FROM support_sla_policies
         WHERE policy_id = ?1",
        params![policy_id],
        |row| {
            Ok(SlaPolicy {
                active: row.get::<_, i64>(0)? != 0,
                first_response_target_ms: row.get(1)?,
                next_response_target_ms: row.get(2)?,
                resolution_target_ms: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn load_applied_sla_breached_at(
    conn: &Connection,
    applied_sla_id: &str,
) -> anyhow::Result<Option<i64>> {
    conn.query_row(
        "SELECT breached_at_ms FROM support_applied_slas WHERE applied_sla_id = ?1",
        params![applied_sla_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

#[derive(Debug)]
struct AppliedSlaRecalcRow {
    applied_sla_id: String,
    conversation_id: String,
    first_response_due_at_ms: i64,
    next_response_due_at_ms: i64,
    resolution_due_at_ms: i64,
    breached_at_ms: i64,
    status: String,
}

fn applied_sla_recalc_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AppliedSlaRecalcRow> {
    Ok(AppliedSlaRecalcRow {
        applied_sla_id: row.get(0)?,
        conversation_id: row.get(1)?,
        first_response_due_at_ms: row.get(2)?,
        next_response_due_at_ms: row.get(3)?,
        resolution_due_at_ms: row.get(4)?,
        breached_at_ms: row.get(5)?,
        status: row.get(6)?,
    })
}

fn due_at(started_at_ms: i64, target_ms: i64) -> i64 {
    if target_ms <= 0 {
        0
    } else {
        started_at_ms.saturating_add(target_ms)
    }
}

fn sla_is_breached(
    now: i64,
    first_response_due_at_ms: i64,
    next_response_due_at_ms: i64,
    resolution_due_at_ms: i64,
) -> bool {
    [
        first_response_due_at_ms,
        next_response_due_at_ms,
        resolution_due_at_ms,
    ]
    .into_iter()
    .filter(|due_at| *due_at > 0)
    .any(|due_at| now > due_at)
}

fn load_inbox_policy(conn: &Connection, inbox_id: &str) -> anyhow::Result<Option<Value>> {
    conn.query_row(
        "SELECT policy_json FROM support_inboxes WHERE inbox_id = ?1",
        params![inbox_id],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .map(|raw| serde_json::from_str(&raw).context("parse support inbox policy"))
    .transpose()
}

fn current_created_at(
    conn: &Connection,
    table: &str,
    id_column: &str,
    id: &str,
) -> anyhow::Result<Option<i64>> {
    let sql = format!("SELECT created_at_ms FROM {table} WHERE {id_column} = ?1");
    conn.query_row(sql.as_str(), params![id], |row| row.get(0))
        .optional()
        .map_err(Into::into)
}

#[derive(Default)]
struct ConversationPatch {
    status: Option<String>,
    priority: Option<String>,
    assignee_id: Option<String>,
    team_id: Option<String>,
    customer_account_id: Option<String>,
    customer_contact_id: Option<String>,
    ticket_case_id: Option<String>,
    last_activity_at_ms: Option<i64>,
    snoozed_until_ms: Option<i64>,
}

fn update_conversation_fields(
    conn: &Connection,
    conversation_id: &str,
    patch: ConversationPatch,
    now: i64,
) -> anyhow::Result<()> {
    let current = load_conversation_required(conn, conversation_id)?;
    conn.execute(
        "UPDATE support_conversations
         SET status = ?2,
             priority = ?3,
             assignee_id = ?4,
             team_id = ?5,
             customer_account_id = ?6,
             customer_contact_id = ?7,
             ticket_case_id = ?8,
             last_activity_at_ms = ?9,
             snoozed_until_ms = ?10,
             updated_at_ms = ?11
         WHERE conversation_id = ?1",
        params![
            conversation_id,
            patch.status.unwrap_or(current.status),
            patch.priority.unwrap_or(current.priority),
            patch.assignee_id.unwrap_or(current.assignee_id),
            patch.team_id.unwrap_or(current.team_id),
            patch
                .customer_account_id
                .unwrap_or(current.customer_account_id),
            patch
                .customer_contact_id
                .unwrap_or(current.customer_contact_id),
            patch.ticket_case_id.unwrap_or(current.ticket_case_id),
            patch
                .last_activity_at_ms
                .unwrap_or(current.last_activity_at_ms),
            patch.snoozed_until_ms.unwrap_or(current.snoozed_until_ms),
            now
        ],
    )?;
    Ok(())
}

fn update_conversation_status(
    conn: &Connection,
    conversation_id: &str,
    status: &str,
    command_id: &str,
    actor_id: &str,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    update_conversation_fields(
        conn,
        conversation_id,
        ConversationPatch {
            status: Some(status.to_owned()),
            snoozed_until_ms: Some(if status == "open" {
                0
            } else {
                number_or_current_snooze(conn, conversation_id)?
            }),
            last_activity_at_ms: Some(now),
            ..Default::default()
        },
        now,
    )?;
    let conversation = load_conversation_required(conn, conversation_id)?;
    project_conversation(conn, &conversation, projections)?;
    insert_conversation_event(
        conn,
        conversation_id,
        "support.conversation.status_changed",
        actor_id,
        command_id,
        "",
        "Support conversation status changed.",
        json!({ "status": status }),
        now,
        projections,
    )?;
    insert_reporting_event(
        conn,
        conversation_id,
        "support.conversation.status_changed",
        "count",
        1,
        json!({ "status": status }),
        now,
        projections,
    )?;
    Ok(())
}

fn number_or_current_snooze(conn: &Connection, conversation_id: &str) -> anyhow::Result<i64> {
    Ok(load_conversation_required(conn, conversation_id)?.snoozed_until_ms)
}

fn load_conversation_required(
    conn: &Connection,
    conversation_id: &str,
) -> anyhow::Result<SupportConversation> {
    load_conversation(conn, conversation_id)?
        .with_context(|| format!("support conversation `{conversation_id}` does not exist"))
}

fn load_conversation(
    conn: &Connection,
    conversation_id: &str,
) -> anyhow::Result<Option<SupportConversation>> {
    conn.query_row(
        "SELECT conversation_id, inbox_id, primary_thread_key, status, priority, assignee_id,
                team_id, customer_account_id, customer_contact_id, ticket_case_id,
                last_message_key, last_activity_at_ms, waiting_since_ms, snoozed_until_ms,
                unread_count, label_ids_json, custom_attributes_json, search_text,
                created_at_ms, updated_at_ms
         FROM support_conversations
         WHERE conversation_id = ?1",
        params![conversation_id],
        |row| {
            let label_ids_json: String = row.get(15)?;
            let custom_attributes_json: String = row.get(16)?;
            Ok(SupportConversation {
                id: row.get(0)?,
                inbox_id: row.get(1)?,
                primary_thread_key: row.get(2)?,
                status: row.get(3)?,
                priority: row.get(4)?,
                assignee_id: row.get(5)?,
                team_id: row.get(6)?,
                customer_account_id: row.get(7)?,
                customer_contact_id: row.get(8)?,
                ticket_case_id: row.get(9)?,
                last_message_key: row.get(10)?,
                last_activity_at_ms: row.get(11)?,
                waiting_since_ms: row.get(12)?,
                snoozed_until_ms: row.get(13)?,
                unread_count: row.get(14)?,
                label_ids: serde_json::from_str(&label_ids_json).unwrap_or_else(|_| json!([])),
                custom_attributes: serde_json::from_str(&custom_attributes_json)
                    .unwrap_or_else(|_| json!({})),
                search_text: row.get(17)?,
                created_at_ms: row.get(18)?,
                updated_at_ms: row.get(19)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn linked_conversation_for_thread(
    conn: &Connection,
    thread_key: &str,
) -> anyhow::Result<Option<String>> {
    conn.query_row(
        "SELECT conversation_id
         FROM support_thread_links
         WHERE thread_key = ?1 AND link_role = 'primary'",
        params![thread_key],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn insert_note(
    conn: &Connection,
    note_id: &str,
    conversation_id: &str,
    author_id: &str,
    body: &str,
    visibility: &str,
    source: &str,
    now: i64,
) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO support_notes
            (note_id, conversation_id, author_id, body, visibility, source, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT(note_id) DO UPDATE SET
            body = excluded.body,
            visibility = excluded.visibility,
            source = excluded.source,
            updated_at_ms = excluded.updated_at_ms",
        params![note_id, conversation_id, author_id, body, visibility, source, now],
    )?;
    Ok(())
}

fn upsert_thread_link(
    conn: &Connection,
    conversation_id: &str,
    thread_key: &str,
    channel: &str,
    account_key: &str,
    link_role: &str,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let thread_link_id = format!(
        "support_thread_link_{}",
        Uuid::new_v5(&Uuid::NAMESPACE_URL, thread_key.as_bytes())
    );
    conn.execute(
        "INSERT INTO support_thread_links
            (thread_link_id, conversation_id, thread_key, channel, account_key, link_role, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT(thread_key, link_role) DO UPDATE SET
            conversation_id = excluded.conversation_id,
            channel = excluded.channel,
            account_key = excluded.account_key,
            updated_at_ms = excluded.updated_at_ms",
        params![thread_link_id, conversation_id, thread_key, channel, account_key, link_role, now],
    )?;
    project_thread_link(conn, &thread_link_id, projections)
}

fn upsert_identity_link(
    conn: &Connection,
    identity_link_id: &str,
    payload: &Value,
    now: i64,
) -> anyhow::Result<()> {
    let channel = first_string_field(payload, &["channel", "inbound_channel"]).unwrap_or_default();
    let account_key = first_string_field(payload, &["account_key"]).unwrap_or_default();
    let external_identity =
        first_string_field(payload, &["external_identity", "email", "phone"]).unwrap_or_default();
    let normalized_identity = first_string_field(payload, &["normalized_identity"])
        .unwrap_or_else(|| external_identity.trim().to_ascii_lowercase());
    let customer_account_id =
        first_string_field(payload, &["customer_account_id"]).unwrap_or_default();
    let customer_contact_id =
        first_string_field(payload, &["customer_contact_id"]).unwrap_or_default();
    let confidence = number_field(payload, &["confidence"]).unwrap_or(1) as f64;
    let status = first_string_field(payload, &["status"]).unwrap_or_else(|| "active".to_owned());
    let source = first_string_field(payload, &["source"])
        .unwrap_or_else(|| "business-os.support".to_owned());
    conn.execute(
        "INSERT INTO support_identity_links
            (identity_link_id, channel, account_key, external_identity, normalized_identity,
             customer_account_id, customer_contact_id, confidence, status, source, payload_json,
             created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)
         ON CONFLICT(identity_link_id) DO UPDATE SET
            channel = excluded.channel,
            account_key = excluded.account_key,
            external_identity = excluded.external_identity,
            normalized_identity = excluded.normalized_identity,
            customer_account_id = excluded.customer_account_id,
            customer_contact_id = excluded.customer_contact_id,
            confidence = excluded.confidence,
            status = excluded.status,
            source = excluded.source,
            payload_json = excluded.payload_json,
            updated_at_ms = excluded.updated_at_ms",
        params![
            identity_link_id,
            channel,
            account_key,
            external_identity,
            normalized_identity,
            customer_account_id,
            customer_contact_id,
            confidence,
            status,
            source,
            serde_json::to_string(payload)?,
            now
        ],
    )?;
    Ok(())
}

fn insert_conversation_event(
    conn: &Connection,
    conversation_id: &str,
    event_type: &str,
    actor_id: &str,
    source_command_id: &str,
    source_task_id: &str,
    summary: &str,
    payload: Value,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let event_id = format!("support_event_{}", Uuid::new_v4());
    conn.execute(
        "INSERT INTO support_conversation_events
            (event_id, conversation_id, event_type, actor_id, source_command_id, source_task_id,
             summary, payload_json, occurred_at_ms, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?9)",
        params![
            event_id,
            conversation_id,
            event_type,
            actor_id,
            source_command_id,
            source_task_id,
            summary,
            serde_json::to_string(&payload)?,
            now
        ],
    )?;
    project_conversation_event(conn, &event_id, projections)
}

fn insert_assignment_event(
    conn: &Connection,
    conversation_id: &str,
    assignee_id: &str,
    previous_assignee_id: &str,
    event_type: &str,
    payload: Value,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let event_id = format!("support_assignment_{}", Uuid::new_v4());
    conn.execute(
        "INSERT INTO support_assignment_events
            (event_id, conversation_id, policy_id, assignee_id, previous_assignee_id,
             event_type, occurred_at_ms, payload_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, '', ?3, ?4, ?5, ?6, ?7, ?6, ?6)",
        params![
            event_id,
            conversation_id,
            assignee_id,
            previous_assignee_id,
            event_type,
            now,
            serde_json::to_string(&payload)?
        ],
    )?;
    project_assignment_event(conn, &event_id, projections)
}

fn insert_sla_event(
    conn: &Connection,
    conversation_id: &str,
    applied_sla_id: &str,
    event_type: &str,
    payload: Value,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let event_id = format!("support_sla_event_{}", Uuid::new_v4());
    conn.execute(
        "INSERT INTO support_sla_events
            (event_id, conversation_id, applied_sla_id, event_type, occurred_at_ms,
             payload_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?5, ?5)",
        params![
            event_id,
            conversation_id,
            applied_sla_id,
            event_type,
            now,
            serde_json::to_string(&payload)?
        ],
    )?;
    project_sla_event(conn, &event_id, projections)
}

fn insert_reporting_event(
    conn: &Connection,
    conversation_id: &str,
    event_name: &str,
    metric_name: &str,
    value_ms: i64,
    payload: Value,
    now: i64,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let event_id = format!("support_reporting_event_{}", Uuid::new_v4());
    conn.execute(
        "INSERT INTO support_reporting_events
            (event_id, conversation_id, event_name, metric_name, value_ms, occurred_at_ms,
             payload_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?6, ?6)",
        params![
            event_id,
            conversation_id,
            event_name,
            metric_name,
            value_ms,
            now,
            serde_json::to_string(&payload)?
        ],
    )?;
    project_reporting_event(conn, &event_id, projections)
}

#[allow(clippy::too_many_arguments)]
fn upsert_agent_suggestion(
    conn: &Connection,
    suggestion_id: &str,
    conversation_id: &str,
    source_command_id: &str,
    task_id: &str,
    suggestion_kind: &str,
    status: &str,
    confidence: f64,
    required_human_action: &str,
    summary: &str,
    payload: Value,
    now: i64,
) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO support_agent_suggestions
            (suggestion_id, conversation_id, source_command_id, task_id, suggestion_kind,
             status, confidence, required_human_action, summary, payload_json, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
         ON CONFLICT(suggestion_id) DO UPDATE SET
            conversation_id = excluded.conversation_id,
            source_command_id = excluded.source_command_id,
            task_id = excluded.task_id,
            suggestion_kind = excluded.suggestion_kind,
            status = excluded.status,
            confidence = excluded.confidence,
            required_human_action = excluded.required_human_action,
            summary = excluded.summary,
            payload_json = excluded.payload_json,
            updated_at_ms = excluded.updated_at_ms",
        params![
            suggestion_id,
            conversation_id,
            source_command_id,
            task_id,
            suggestion_kind,
            status,
            confidence,
            required_human_action,
            summary,
            serde_json::to_string(&payload)?,
            now
        ],
    )?;
    Ok(())
}

fn load_agent_suggestion_conversation_id(
    conn: &Connection,
    suggestion_id: &str,
) -> anyhow::Result<Option<String>> {
    conn.query_row(
        "SELECT conversation_id FROM support_agent_suggestions WHERE suggestion_id = ?1",
        params![suggestion_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn project_inbox(
    conn: &Connection,
    inbox_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT inbox_id, name, description, status, channel_filters_json, team_id,
                    assignment_policy_id, sla_policy_id, policy_json, is_default, sort_key,
                    created_at_ms, updated_at_ms
             FROM support_inboxes
             WHERE inbox_id = ?1",
            params![inbox_id],
            |row| {
                let channel_filters_json: String = row.get(4)?;
                let policy_json: String = row.get(8)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "name": row.get::<_, String>(1)?,
                    "description": row.get::<_, String>(2)?,
                    "status": row.get::<_, String>(3)?,
                    "channel_filters_json": serde_json::from_str::<Value>(&channel_filters_json).unwrap_or_else(|_| json!({})),
                    "team_id": row.get::<_, String>(5)?,
                    "assignment_policy_id": row.get::<_, String>(6)?,
                    "sla_policy_id": row.get::<_, String>(7)?,
                    "policy_json": serde_json::from_str::<Value>(&policy_json).unwrap_or_else(|_| json!({})),
                    "is_default": row.get::<_, i64>(9)? != 0,
                    "sort_key": row.get::<_, String>(10)?,
                    "created_at_ms": row.get::<_, i64>(11)?,
                    "updated_at_ms": row.get::<_, i64>(12)?
                }))
            },
        )
        .optional()?;
    project_payload(conn, "support_inboxes", inbox_id, payload, projections)
}

fn project_view(
    conn: &Connection,
    view_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT view_id, title, owner_id, scope, position, filters_json, sorts_json,
                    created_at_ms, updated_at_ms
             FROM support_views
             WHERE view_id = ?1",
            params![view_id],
            |row| {
                let filters_json: String = row.get(5)?;
                let sorts_json: String = row.get(6)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "title": row.get::<_, String>(1)?,
                    "owner_id": row.get::<_, String>(2)?,
                    "scope": row.get::<_, String>(3)?,
                    "position": row.get::<_, i64>(4)?,
                    "filters_json": serde_json::from_str::<Value>(&filters_json).unwrap_or_else(|_| json!({})),
                    "sorts_json": serde_json::from_str::<Value>(&sorts_json).unwrap_or_else(|_| json!({})),
                    "created_at_ms": row.get::<_, i64>(7)?,
                    "updated_at_ms": row.get::<_, i64>(8)?
                }))
            },
        )
        .optional()?;
    project_payload(conn, "support_views", view_id, payload, projections)
}

fn project_view_filter(
    conn: &Connection,
    filter_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT filter_id, view_id, field, operator, value_json, position,
                    created_at_ms, updated_at_ms
             FROM support_view_filters
             WHERE filter_id = ?1",
            params![filter_id],
            |row| {
                let value_json: String = row.get(4)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "view_id": row.get::<_, String>(1)?,
                    "field": row.get::<_, String>(2)?,
                    "operator": row.get::<_, String>(3)?,
                    "value": serde_json::from_str::<Value>(&value_json).unwrap_or(Value::Null),
                    "position": row.get::<_, i64>(5)?,
                    "created_at_ms": row.get::<_, i64>(6)?,
                    "updated_at_ms": row.get::<_, i64>(7)?
                }))
            },
        )
        .optional()?;
    project_payload(
        conn,
        "support_view_filters",
        filter_id,
        payload,
        projections,
    )
}

fn project_conversation(
    conn: &Connection,
    conversation: &SupportConversation,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = json!({
        "id": conversation.id,
        "is_deleted": false,
        "created_at_ms": conversation.created_at_ms,
        "updated_at_ms": conversation.updated_at_ms,
        "inbox_id": conversation.inbox_id,
        "primary_thread_key": conversation.primary_thread_key,
        "status": conversation.status,
        "priority": conversation.priority,
        "assignee_id": conversation.assignee_id,
        "team_id": conversation.team_id,
        "customer_account_id": conversation.customer_account_id,
        "customer_contact_id": conversation.customer_contact_id,
        "ticket_case_id": conversation.ticket_case_id,
        "last_message_key": conversation.last_message_key,
        "last_activity_at_ms": conversation.last_activity_at_ms,
        "waiting_since_ms": conversation.waiting_since_ms,
        "snoozed_until_ms": conversation.snoozed_until_ms,
        "unread_count": conversation.unread_count,
        "label_ids": conversation.label_ids,
        "custom_attributes": conversation.custom_attributes,
        "search_text": conversation.search_text
    });
    store::upsert_business_record(
        conn,
        "support_conversations",
        conversation.id.as_str(),
        conversation.updated_at_ms,
        payload,
    )?;
    push_projection(projections, "support_conversations", &conversation.id);
    Ok(())
}

fn project_thread_link(
    conn: &Connection,
    thread_link_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT thread_link_id, conversation_id, thread_key, channel, account_key,
                    link_role, created_at_ms, updated_at_ms
             FROM support_thread_links
             WHERE thread_link_id = ?1",
            params![thread_link_id],
            |row| {
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "conversation_id": row.get::<_, String>(1)?,
                    "thread_key": row.get::<_, String>(2)?,
                    "channel": row.get::<_, String>(3)?,
                    "account_key": row.get::<_, String>(4)?,
                    "link_role": row.get::<_, String>(5)?,
                    "created_at_ms": row.get::<_, i64>(6)?,
                    "updated_at_ms": row.get::<_, i64>(7)?
                }))
            },
        )
        .optional()?;
    if let Some(payload) = payload {
        let updated_at_ms = payload
            .get("updated_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        store::upsert_business_record(
            conn,
            "support_thread_links",
            thread_link_id,
            updated_at_ms,
            payload,
        )?;
        push_projection(projections, "support_thread_links", thread_link_id);
    }
    Ok(())
}

fn project_identity_link(
    conn: &Connection,
    identity_link_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT identity_link_id, channel, account_key, external_identity, normalized_identity,
                    customer_account_id, customer_contact_id, confidence, status, source,
                    payload_json, created_at_ms, updated_at_ms
             FROM support_identity_links
             WHERE identity_link_id = ?1",
            params![identity_link_id],
            |row| {
                let payload_json: String = row.get(10)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "channel": row.get::<_, String>(1)?,
                    "account_key": row.get::<_, String>(2)?,
                    "external_identity": row.get::<_, String>(3)?,
                    "normalized_identity": row.get::<_, String>(4)?,
                    "customer_account_id": row.get::<_, String>(5)?,
                    "customer_contact_id": row.get::<_, String>(6)?,
                    "confidence": row.get::<_, f64>(7)?,
                    "status": row.get::<_, String>(8)?,
                    "source": row.get::<_, String>(9)?,
                    "payload": serde_json::from_str::<Value>(&payload_json).unwrap_or_else(|_| json!({})),
                    "created_at_ms": row.get::<_, i64>(11)?,
                    "updated_at_ms": row.get::<_, i64>(12)?
                }))
            },
        )
        .optional()?;
    if let Some(payload) = payload {
        let updated_at_ms = payload
            .get("updated_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        store::upsert_business_record(
            conn,
            "support_identity_links",
            identity_link_id,
            updated_at_ms,
            payload,
        )?;
        push_projection(projections, "support_identity_links", identity_link_id);
    }
    Ok(())
}

fn project_note(
    conn: &Connection,
    note_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT note_id, conversation_id, author_id, body, visibility, source,
                    created_at_ms, updated_at_ms
             FROM support_notes
             WHERE note_id = ?1",
            params![note_id],
            |row| {
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "conversation_id": row.get::<_, String>(1)?,
                    "author_id": row.get::<_, String>(2)?,
                    "body": row.get::<_, String>(3)?,
                    "visibility": row.get::<_, String>(4)?,
                    "source": row.get::<_, String>(5)?,
                    "created_at_ms": row.get::<_, i64>(6)?,
                    "updated_at_ms": row.get::<_, i64>(7)?
                }))
            },
        )
        .optional()?;
    if let Some(payload) = payload {
        let updated_at_ms = payload
            .get("updated_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        store::upsert_business_record(conn, "support_notes", note_id, updated_at_ms, payload)?;
        push_projection(projections, "support_notes", note_id);
    }
    Ok(())
}

fn project_conversation_event(
    conn: &Connection,
    event_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT event_id, conversation_id, event_type, actor_id, source_command_id,
                    source_task_id, summary, payload_json, occurred_at_ms, created_at_ms,
                    updated_at_ms
             FROM support_conversation_events
             WHERE event_id = ?1",
            params![event_id],
            |row| {
                let payload_json: String = row.get(7)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "conversation_id": row.get::<_, String>(1)?,
                    "event_type": row.get::<_, String>(2)?,
                    "actor_id": row.get::<_, String>(3)?,
                    "source_command_id": row.get::<_, String>(4)?,
                    "source_task_id": row.get::<_, String>(5)?,
                    "summary": row.get::<_, String>(6)?,
                    "payload": serde_json::from_str::<Value>(&payload_json).unwrap_or_else(|_| json!({})),
                    "occurred_at_ms": row.get::<_, i64>(8)?,
                    "created_at_ms": row.get::<_, i64>(9)?,
                    "updated_at_ms": row.get::<_, i64>(10)?
                }))
            },
        )
        .optional()?;
    if let Some(payload) = payload {
        let updated_at_ms = payload
            .get("updated_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        store::upsert_business_record(
            conn,
            "support_conversation_events",
            event_id,
            updated_at_ms,
            payload,
        )?;
        push_projection(projections, "support_conversation_events", event_id);
    }
    Ok(())
}

fn project_assignment_event(
    conn: &Connection,
    event_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT event_id, conversation_id, policy_id, assignee_id, previous_assignee_id,
                    event_type, occurred_at_ms, payload_json, created_at_ms, updated_at_ms
             FROM support_assignment_events
             WHERE event_id = ?1",
            params![event_id],
            |row| {
                let payload_json: String = row.get(7)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "conversation_id": row.get::<_, String>(1)?,
                    "policy_id": row.get::<_, String>(2)?,
                    "assignee_id": row.get::<_, String>(3)?,
                    "previous_assignee_id": row.get::<_, String>(4)?,
                    "event_type": row.get::<_, String>(5)?,
                    "occurred_at_ms": row.get::<_, i64>(6)?,
                    "payload": serde_json::from_str::<Value>(&payload_json).unwrap_or_else(|_| json!({})),
                    "created_at_ms": row.get::<_, i64>(8)?,
                    "updated_at_ms": row.get::<_, i64>(9)?
                }))
            },
        )
        .optional()?;
    if let Some(payload) = payload {
        let updated_at_ms = payload
            .get("updated_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        store::upsert_business_record(
            conn,
            "support_assignment_events",
            event_id,
            updated_at_ms,
            payload,
        )?;
        push_projection(projections, "support_assignment_events", event_id);
    }
    Ok(())
}

fn project_assignment_policy(
    conn: &Connection,
    policy_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT policy_id, name, strategy, fair_distribution_limit,
                    fair_distribution_window_ms, payload_json, created_at_ms, updated_at_ms
             FROM support_assignment_policies
             WHERE policy_id = ?1",
            params![policy_id],
            |row| {
                let payload_json: String = row.get(5)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "name": row.get::<_, String>(1)?,
                    "strategy": row.get::<_, String>(2)?,
                    "fair_distribution_limit": row.get::<_, i64>(3)?,
                    "fair_distribution_window_ms": row.get::<_, i64>(4)?,
                    "payload": serde_json::from_str::<Value>(&payload_json).unwrap_or_else(|_| json!({})),
                    "created_at_ms": row.get::<_, i64>(6)?,
                    "updated_at_ms": row.get::<_, i64>(7)?
                }))
            },
        )
        .optional()?;
    project_payload(
        conn,
        "support_assignment_policies",
        policy_id,
        payload,
        projections,
    )
}

fn project_macro(
    conn: &Connection,
    macro_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT macro_id, title, visibility, owner_id, actions_json, payload_json,
                    created_at_ms, updated_at_ms
             FROM support_macros
             WHERE macro_id = ?1",
            params![macro_id],
            |row| {
                let actions_json: String = row.get(4)?;
                let payload_json: String = row.get(5)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "title": row.get::<_, String>(1)?,
                    "visibility": row.get::<_, String>(2)?,
                    "owner_id": row.get::<_, String>(3)?,
                    "actions_json": serde_json::from_str::<Value>(&actions_json).unwrap_or_else(|_| json!([])),
                    "payload": serde_json::from_str::<Value>(&payload_json).unwrap_or_else(|_| json!({})),
                    "created_at_ms": row.get::<_, i64>(6)?,
                    "updated_at_ms": row.get::<_, i64>(7)?
                }))
            },
        )
        .optional()?;
    project_payload(conn, "support_macros", macro_id, payload, projections)
}

fn project_automation_rule(
    conn: &Connection,
    rule_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT rule_id, name, event_name, active, query_operator, conditions_json,
                    actions_json, created_at_ms, updated_at_ms
             FROM support_automation_rules
             WHERE rule_id = ?1",
            params![rule_id],
            |row| {
                let conditions_json: String = row.get(5)?;
                let actions_json: String = row.get(6)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "name": row.get::<_, String>(1)?,
                    "event_name": row.get::<_, String>(2)?,
                    "active": row.get::<_, i64>(3)? != 0,
                    "query_operator": row.get::<_, String>(4)?,
                    "conditions_json": serde_json::from_str::<Value>(&conditions_json).unwrap_or_else(|_| json!([])),
                    "actions_json": serde_json::from_str::<Value>(&actions_json).unwrap_or_else(|_| json!([])),
                    "created_at_ms": row.get::<_, i64>(7)?,
                    "updated_at_ms": row.get::<_, i64>(8)?
                }))
            },
        )
        .optional()?;
    project_payload(
        conn,
        "support_automation_rules",
        rule_id,
        payload,
        projections,
    )
}

fn project_sla_policy(
    conn: &Connection,
    policy_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT policy_id, name, active, first_response_target_ms, next_response_target_ms,
                    resolution_target_ms, business_hours_json, payload_json,
                    created_at_ms, updated_at_ms
             FROM support_sla_policies
             WHERE policy_id = ?1",
            params![policy_id],
            |row| {
                let business_hours_json: String = row.get(6)?;
                let payload_json: String = row.get(7)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "name": row.get::<_, String>(1)?,
                    "active": row.get::<_, i64>(2)? != 0,
                    "first_response_target_ms": row.get::<_, i64>(3)?,
                    "next_response_target_ms": row.get::<_, i64>(4)?,
                    "resolution_target_ms": row.get::<_, i64>(5)?,
                    "business_hours_json": serde_json::from_str::<Value>(&business_hours_json).unwrap_or_else(|_| json!({})),
                    "payload": serde_json::from_str::<Value>(&payload_json).unwrap_or_else(|_| json!({})),
                    "created_at_ms": row.get::<_, i64>(8)?,
                    "updated_at_ms": row.get::<_, i64>(9)?
                }))
            },
        )
        .optional()?;
    project_payload(
        conn,
        "support_sla_policies",
        policy_id,
        payload,
        projections,
    )
}

fn project_applied_sla(
    conn: &Connection,
    applied_sla_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT applied_sla_id, conversation_id, policy_id, status,
                    first_response_due_at_ms, next_response_due_at_ms, resolution_due_at_ms,
                    breached_at_ms, payload_json, created_at_ms, updated_at_ms
             FROM support_applied_slas
             WHERE applied_sla_id = ?1",
            params![applied_sla_id],
            |row| {
                let payload_json: String = row.get(8)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "conversation_id": row.get::<_, String>(1)?,
                    "policy_id": row.get::<_, String>(2)?,
                    "status": row.get::<_, String>(3)?,
                    "first_response_due_at_ms": row.get::<_, i64>(4)?,
                    "next_response_due_at_ms": row.get::<_, i64>(5)?,
                    "resolution_due_at_ms": row.get::<_, i64>(6)?,
                    "breached_at_ms": row.get::<_, i64>(7)?,
                    "payload": serde_json::from_str::<Value>(&payload_json).unwrap_or_else(|_| json!({})),
                    "created_at_ms": row.get::<_, i64>(9)?,
                    "updated_at_ms": row.get::<_, i64>(10)?
                }))
            },
        )
        .optional()?;
    project_payload(
        conn,
        "support_applied_slas",
        applied_sla_id,
        payload,
        projections,
    )
}

fn project_sla_event(
    conn: &Connection,
    event_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT event_id, conversation_id, applied_sla_id, event_type, occurred_at_ms,
                    payload_json, created_at_ms, updated_at_ms
             FROM support_sla_events
             WHERE event_id = ?1",
            params![event_id],
            |row| {
                let payload_json: String = row.get(5)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "conversation_id": row.get::<_, String>(1)?,
                    "applied_sla_id": row.get::<_, String>(2)?,
                    "event_type": row.get::<_, String>(3)?,
                    "occurred_at_ms": row.get::<_, i64>(4)?,
                    "payload": serde_json::from_str::<Value>(&payload_json).unwrap_or_else(|_| json!({})),
                    "created_at_ms": row.get::<_, i64>(6)?,
                    "updated_at_ms": row.get::<_, i64>(7)?
                }))
            },
        )
        .optional()?;
    project_payload(conn, "support_sla_events", event_id, payload, projections)
}

fn project_reporting_event(
    conn: &Connection,
    event_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT event_id, conversation_id, event_name, metric_name, value_ms,
                    occurred_at_ms, payload_json, created_at_ms, updated_at_ms
             FROM support_reporting_events
             WHERE event_id = ?1",
            params![event_id],
            |row| {
                let payload_json: String = row.get(6)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "conversation_id": row.get::<_, String>(1)?,
                    "event_name": row.get::<_, String>(2)?,
                    "metric_name": row.get::<_, String>(3)?,
                    "value_ms": row.get::<_, i64>(4)?,
                    "occurred_at_ms": row.get::<_, i64>(5)?,
                    "payload": serde_json::from_str::<Value>(&payload_json).unwrap_or_else(|_| json!({})),
                    "created_at_ms": row.get::<_, i64>(7)?,
                    "updated_at_ms": row.get::<_, i64>(8)?
                }))
            },
        )
        .optional()?;
    project_payload(
        conn,
        "support_reporting_events",
        event_id,
        payload,
        projections,
    )
}

fn project_reporting_rollup(
    conn: &Connection,
    rollup_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT rollup_id, rollup_key, bucket_start_ms, bucket_end_ms, metric_name,
                    dimensions_json, value, count, created_at_ms, updated_at_ms
             FROM support_reporting_rollups
             WHERE rollup_id = ?1",
            params![rollup_id],
            |row| {
                let dimensions_json: String = row.get(5)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "rollup_key": row.get::<_, String>(1)?,
                    "bucket_start_ms": row.get::<_, i64>(2)?,
                    "bucket_end_ms": row.get::<_, i64>(3)?,
                    "metric_name": row.get::<_, String>(4)?,
                    "dimensions": serde_json::from_str::<Value>(&dimensions_json).unwrap_or_else(|_| json!({})),
                    "value": row.get::<_, f64>(6)?,
                    "count": row.get::<_, i64>(7)?,
                    "created_at_ms": row.get::<_, i64>(8)?,
                    "updated_at_ms": row.get::<_, i64>(9)?
                }))
            },
        )
        .optional()?;
    project_payload(
        conn,
        "support_reporting_rollups",
        rollup_id,
        payload,
        projections,
    )
}

fn project_agent_suggestion(
    conn: &Connection,
    suggestion_id: &str,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    let payload = conn
        .query_row(
            "SELECT suggestion_id, conversation_id, source_command_id, task_id, suggestion_kind,
                    status, confidence, required_human_action, summary, payload_json,
                    created_at_ms, updated_at_ms
             FROM support_agent_suggestions
             WHERE suggestion_id = ?1",
            params![suggestion_id],
            |row| {
                let payload_json: String = row.get(9)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "is_deleted": false,
                    "conversation_id": row.get::<_, String>(1)?,
                    "source_command_id": row.get::<_, String>(2)?,
                    "task_id": row.get::<_, String>(3)?,
                    "suggestion_kind": row.get::<_, String>(4)?,
                    "status": row.get::<_, String>(5)?,
                    "confidence": row.get::<_, f64>(6)?,
                    "required_human_action": row.get::<_, String>(7)?,
                    "summary": row.get::<_, String>(8)?,
                    "payload": serde_json::from_str::<Value>(&payload_json).unwrap_or_else(|_| json!({})),
                    "created_at_ms": row.get::<_, i64>(10)?,
                    "updated_at_ms": row.get::<_, i64>(11)?
                }))
            },
        )
        .optional()?;
    if let Some(payload) = payload {
        let updated_at_ms = payload
            .get("updated_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        store::upsert_business_record(
            conn,
            "support_agent_suggestions",
            suggestion_id,
            updated_at_ms,
            payload,
        )?;
        push_projection(projections, "support_agent_suggestions", suggestion_id);
    }
    Ok(())
}

fn project_payload(
    conn: &Connection,
    collection: &'static str,
    record_id: &str,
    payload: Option<Value>,
    projections: &mut Vec<ProjectionRef>,
) -> anyhow::Result<()> {
    if let Some(payload) = payload {
        let updated_at_ms = payload
            .get("updated_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        store::upsert_business_record(conn, collection, record_id, updated_at_ms, payload)?;
        push_projection(projections, collection, record_id);
    }
    Ok(())
}

fn push_projection(
    projections: &mut Vec<ProjectionRef>,
    collection: &'static str,
    record_id: &str,
) {
    if record_id.trim().is_empty() {
        return;
    }
    if projections
        .iter()
        .any(|projection| projection.collection == collection && projection.record_id == record_id)
    {
        return;
    }
    projections.push(ProjectionRef {
        collection,
        record_id: record_id.to_owned(),
    });
}

fn projection_payload(projections: &[ProjectionRef]) -> Value {
    let mut seen = BTreeSet::new();
    let values = projections
        .iter()
        .filter(|projection| seen.insert((projection.collection, projection.record_id.as_str())))
        .map(|projection| {
            json!({
                "collection": projection.collection,
                "record_id": projection.record_id
            })
        })
        .collect::<Vec<_>>();
    Value::Array(values)
}

fn command_conversation_id(command: &BusinessCommand) -> anyhow::Result<String> {
    first_string_field(&command.payload, &["conversation_id", "id"])
        .or_else(|| command.record_id.clone())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .context("conversation_id is required")
}

fn command_conversation_ids(command: &BusinessCommand) -> anyhow::Result<Vec<String>> {
    let mut ids = string_array_field(&command.payload, &["conversation_ids", "ids"]);
    if ids.is_empty() {
        if let Some(record_id) = command
            .record_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            ids.push(record_id.to_owned());
        }
    }
    ids.sort();
    ids.dedup();
    anyhow::ensure!(!ids.is_empty(), "conversation_ids is required");
    Ok(ids)
}

fn actor_id(session: &BusinessOsSession) -> String {
    session
        .user
        .as_ref()
        .map(|user| user.id.clone())
        .unwrap_or_else(|| "business-os".to_owned())
}

fn first_string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn number_field(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().map(|raw| raw as i64))
        })
    })
}

fn bool_field(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|value| {
            value.as_bool().or_else(|| {
                value
                    .as_str()
                    .and_then(|raw| match raw.trim().to_ascii_lowercase().as_str() {
                        "true" | "1" | "yes" | "on" => Some(true),
                        "false" | "0" | "no" | "off" => Some(false),
                        _ => None,
                    })
            })
        })
    })
}

fn json_field(value: &Value, keys: &[&str]) -> Option<Value> {
    keys.iter()
        .find_map(|key| value.get(*key).filter(|item| !item.is_null()).cloned())
}

fn string_array_field(value: &Value, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn json_scalar_eq(left: &Value, right: &Value) -> bool {
    if left == right {
        return true;
    }
    match (left, right) {
        (Value::String(left), Value::String(right)) => left.eq_ignore_ascii_case(right),
        (Value::String(left), other) => left == &json_scalar_string(other),
        (other, Value::String(right)) => &json_scalar_string(other) == right,
        _ => false,
    }
}

fn json_scalar_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}

fn json_contains(left: &Value, right: &Value) -> bool {
    match left {
        Value::Array(items) => items.iter().any(|item| json_scalar_eq(item, right)),
        Value::String(left) => left.contains(json_scalar_string(right).as_str()),
        _ => false,
    }
}

fn value_is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(value) => value.trim().is_empty(),
        Value::Array(values) => values.is_empty(),
        Value::Object(values) => values.is_empty(),
        _ => false,
    }
}

fn normalize_status(status: &str) -> String {
    match status.trim().to_ascii_lowercase().as_str() {
        "resolved" | "closed" | "done" => "resolved".to_owned(),
        "snoozed" | "sleeping" => "snoozed".to_owned(),
        "waiting" | "pending" => "waiting".to_owned(),
        _ => "open".to_owned(),
    }
}

fn normalize_priority(priority: &str) -> String {
    match priority.trim().to_ascii_lowercase().as_str() {
        "urgent" | "critical" => "urgent".to_owned(),
        "high" => "high".to_owned(),
        "low" => "low".to_owned(),
        _ => "normal".to_owned(),
    }
}

fn normalize_visibility(visibility: &str) -> String {
    match visibility.trim().to_ascii_lowercase().as_str() {
        "public" | "customer" => "public".to_owned(),
        _ => "internal".to_owned(),
    }
}

fn trim_to_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect::<String>()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn day_bucket_start_ms(value: i64) -> i64 {
    if value <= 0 {
        0
    } else {
        value - (value % 86_400_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admin_session() -> BusinessOsSession {
        session_for("support-admin")
    }

    fn session_for(user_id: &str) -> BusinessOsSession {
        BusinessOsSession {
            ok: true,
            authenticated: true,
            auth_required: false,
            user: Some(super::store::BusinessOsSessionUser {
                id: user_id.to_owned(),
                display_name: user_id.to_owned(),
                role: "admin".to_owned(),
                is_admin: true,
            }),
            login_url: None,
            reason: None,
        }
    }

    fn command(command_type: &str, conversation_id: &str, payload: Value) -> BusinessCommand {
        BusinessCommand {
            id: Some(format!(
                "cmd_{}_{}",
                command_type.replace('.', "_"),
                Uuid::new_v4()
            )),
            module: "support".to_owned(),
            command_type: command_type.to_owned(),
            record_id: Some(conversation_id.to_owned()),
            payload,
            client_context: json!({
                "actor": {
                    "id": "support-admin",
                    "display_name": "Support Admin",
                    "role": "admin"
                }
            }),
        }
    }

    #[test]
    fn support_commands_project_core_workflow_records() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let session = admin_session();

        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.conversation.open_from_thread",
                "conv_support_1",
                json!({
                    "conversation_id": "conv_support_1",
                    "thread_key": "mail:thread-1",
                    "channel": "mail",
                    "status": "open",
                    "priority": "normal",
                    "search_text": "Printer is offline"
                }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command("support.conversation.claim", "conv_support_1", json!({})),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.conversation.assign",
                "conv_support_1",
                json!({ "assignee_id": "support-admin", "team_id": "support-team" }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.conversation.status",
                "conv_support_1",
                json!({ "status": "waiting" }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.note.create",
                "conv_support_1",
                json!({ "body": "Asked customer for logs.", "visibility": "internal" }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.identity.link",
                "conv_support_1",
                json!({
                    "conversation_id": "conv_support_1",
                    "channel": "mail",
                    "external_identity": "customer@example.com",
                    "customer_account_id": "acct_1",
                    "customer_contact_id": "contact_1"
                }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command("support.conversation.resolve", "conv_support_1", json!({})),
        )?;

        let conn = store::open_store(root.path())?;
        let conversation: String = conn.query_row(
            "SELECT payload_json FROM business_records
             WHERE collection = 'support_conversations' AND record_id = 'conv_support_1'",
            [],
            |row| row.get(0),
        )?;
        let conversation: Value = serde_json::from_str(&conversation)?;
        assert_eq!(
            conversation.get("status").and_then(Value::as_str),
            Some("resolved")
        );
        assert_eq!(
            conversation.get("assignee_id").and_then(Value::as_str),
            Some("support-admin")
        );
        assert_eq!(
            conversation
                .get("customer_contact_id")
                .and_then(Value::as_str),
            Some("contact_1")
        );

        let notes: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection = 'support_notes'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(notes, 1);
        let identity_links: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection = 'support_identity_links'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(identity_links, 1);
        let assignment_events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection = 'support_assignment_events'",
            [],
            |row| row.get(0),
        )?;
        assert!(assignment_events >= 2);
        Ok(())
    }

    #[test]
    fn support_intake_projects_inbound_communication_threads() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let db_path = crate::paths::core_db(root.path());
        let mut channel_conn = channels::open_channel_db(&db_path)?;
        let observed_at = "2026-06-17T12:00:00Z";
        channels::upsert_communication_message(
            &mut channel_conn,
            channels::UpsertMessage {
                message_key: "mail-msg-1",
                channel: "mail",
                account_key: "mail:inbox",
                thread_key: "mail:thread-intake",
                remote_id: "remote-mail-msg-1",
                direction: "inbound",
                folder_hint: "inbox",
                sender_display: "Customer",
                sender_address: "customer@example.com",
                recipient_addresses_json: r#"["support@example.com"]"#,
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Printengine is stuck",
                preview: "Printer has stopped again",
                body_text: "Printer has stopped again",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "medium",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at: observed_at,
                observed_at,
                metadata_json: "{}",
            },
        )?;
        channels::refresh_thread(&mut channel_conn, "mail:thread-intake")?;
        drop(channel_conn);

        let projected = project_communication_intake(root.path(), 100)?;
        assert!(projected >= 2);

        let conn = store::open_store(root.path())?;
        let conversation: String = conn.query_row(
            "SELECT payload_json FROM business_records
             WHERE collection = 'support_conversations'",
            [],
            |row| row.get(0),
        )?;
        let conversation: Value = serde_json::from_str(&conversation)?;
        assert_eq!(
            conversation
                .get("primary_thread_key")
                .and_then(Value::as_str),
            Some("mail:thread-intake")
        );
        assert_eq!(
            conversation.get("search_text").and_then(Value::as_str),
            Some("Printengine is stuck")
        );
        let thread_links: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records
             WHERE collection = 'support_thread_links'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(thread_links, 1);
        Ok(())
    }

    #[test]
    fn support_intake_ignores_internal_business_chat_threads() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let db_path = crate::paths::core_db(root.path());
        let mut channel_conn = channels::open_channel_db(&db_path)?;
        let observed_at = "2026-06-17T12:00:00Z";
        channels::upsert_communication_message(
            &mut channel_conn,
            channels::UpsertMessage {
                message_key: "queue-msg-1",
                channel: "queue",
                account_key: "business-os",
                thread_key: "business-os/support/conv_existing",
                remote_id: "remote-queue-msg-1",
                direction: "inbound",
                folder_hint: "queue",
                sender_display: "CTOX",
                sender_address: "ctox",
                recipient_addresses_json: "[]",
                cc_addresses_json: "[]",
                bcc_addresses_json: "[]",
                subject: "Support: existing conversation",
                preview: "Agent task preview",
                body_text: "Agent task preview",
                body_html: "",
                raw_payload_ref: "",
                trust_level: "high",
                status: "received",
                seen: false,
                has_attachments: false,
                external_created_at: observed_at,
                observed_at,
                metadata_json: "{}",
            },
        )?;
        channels::refresh_thread(&mut channel_conn, "business-os/support/conv_existing")?;
        drop(channel_conn);

        let projected = project_communication_intake(root.path(), 100)?;
        assert_eq!(projected, 0);

        let conn = store::open_store(root.path())?;
        let conversations: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records
             WHERE collection = 'support_conversations'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(conversations, 0);
        Ok(())
    }

    #[test]
    fn support_claim_rejects_competing_assignee_without_force() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let first = session_for("support-agent-1");
        let second = session_for("support-agent-2");

        handle_business_command(
            root.path(),
            &first,
            &command(
                "support.conversation.open_from_thread",
                "conv_claim_1",
                json!({
                    "conversation_id": "conv_claim_1",
                    "thread_key": "mail:claim-1",
                    "search_text": "Claim race"
                }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &first,
            &command("support.conversation.claim", "conv_claim_1", json!({})),
        )?;
        let rejected = handle_business_command(
            root.path(),
            &second,
            &command("support.conversation.claim", "conv_claim_1", json!({})),
        );
        assert!(rejected.is_err());

        let conn = store::open_store(root.path())?;
        let assignee_id: String = conn.query_row(
            "SELECT assignee_id FROM support_conversations WHERE conversation_id = 'conv_claim_1'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(assignee_id, "support-agent-1");
        Ok(())
    }

    #[test]
    fn support_claim_preserves_existing_waiting_status() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let session = admin_session();

        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.conversation.open_from_thread",
                "conv_waiting_claim_1",
                json!({
                    "conversation_id": "conv_waiting_claim_1",
                    "thread_key": "mail:waiting-claim-1",
                    "status": "waiting",
                    "search_text": "Waiting customer case"
                }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.conversation.claim",
                "conv_waiting_claim_1",
                json!({}),
            ),
        )?;

        let conn = store::open_store(root.path())?;
        let (status, assignee_id): (String, String) = conn.query_row(
            "SELECT status, assignee_id
             FROM support_conversations
             WHERE conversation_id = 'conv_waiting_claim_1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(status, "waiting");
        assert_eq!(assignee_id, "support-admin");
        Ok(())
    }

    #[test]
    fn support_macro_and_automation_use_closed_actions() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let session = admin_session();

        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.conversation.open_from_thread",
                "conv_macro_1",
                json!({
                    "conversation_id": "conv_macro_1",
                    "thread_key": "mail:macro-1",
                    "priority": "normal",
                    "search_text": "Macro case"
                }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.macro.upsert",
                "macro_1",
                json!({
                    "id": "macro_1",
                    "title": "Escalate and draft",
                    "actions": [
                        { "type": "support.note.create", "payload": { "body": "Macro note" } },
                        { "type": "support.conversation.priority", "payload": { "priority": "high" } },
                        { "type": "support.reply.draft", "payload": { "body": "We are checking this now." } }
                    ]
                }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.macro.run",
                "conv_macro_1",
                json!({ "conversation_id": "conv_macro_1", "macro_id": "macro_1" }),
            ),
        )?;

        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.automation_rule.upsert",
                "rule_1",
                json!({
                    "id": "rule_1",
                    "name": "High priority waits",
                    "event_name": "manual",
                    "active": true,
                    "conditions": [
                        { "field": "priority", "operator": "eq", "value": "high" }
                    ],
                    "actions": [
                        { "type": "support.conversation.status", "payload": { "status": "waiting" } }
                    ]
                }),
            ),
        )?;
        let automation = handle_business_command(
            root.path(),
            &session,
            &command(
                "support.automation.evaluate",
                "conv_macro_1",
                json!({ "conversation_id": "conv_macro_1", "event_name": "manual" }),
            ),
        )?;
        assert_eq!(
            automation
                .get("matched_rules")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );

        let conn = store::open_store(root.path())?;
        let conversation: String = conn.query_row(
            "SELECT payload_json FROM business_records
             WHERE collection = 'support_conversations' AND record_id = 'conv_macro_1'",
            [],
            |row| row.get(0),
        )?;
        let conversation: Value = serde_json::from_str(&conversation)?;
        assert_eq!(
            conversation.get("priority").and_then(Value::as_str),
            Some("high")
        );
        assert_eq!(
            conversation.get("status").and_then(Value::as_str),
            Some("waiting")
        );
        let notes: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection = 'support_notes'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(notes, 1);
        let drafts: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records
             WHERE collection = 'support_agent_suggestions'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(drafts, 1);
        Ok(())
    }

    #[test]
    fn support_sla_breach_and_resolve_gate_are_projected() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let session = admin_session();
        let started_at_ms = now_ms().saturating_sub(10_000);

        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.conversation.open_from_thread",
                "conv_sla_1",
                json!({
                    "conversation_id": "conv_sla_1",
                    "thread_key": "mail:sla-1",
                    "waiting_since_ms": started_at_ms,
                    "search_text": "SLA case"
                }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.sla_policy.upsert",
                "sla_1",
                json!({
                    "id": "sla_1",
                    "name": "Fast resolution",
                    "active": true,
                    "resolution_target_ms": 1
                }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.sla.apply",
                "conv_sla_1",
                json!({
                    "conversation_id": "conv_sla_1",
                    "policy_id": "sla_1",
                    "started_at_ms": started_at_ms
                }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.sla.recalculate",
                "conv_sla_1",
                json!({ "conversation_id": "conv_sla_1" }),
            ),
        )?;

        let blocked = handle_business_command(
            root.path(),
            &session,
            &command(
                "support.conversation.resolve",
                "conv_sla_1",
                json!({ "required_fields": ["ticket_case_id"] }),
            ),
        );
        assert!(blocked.is_err());
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.ticket.link",
                "conv_sla_1",
                json!({ "ticket_case_id": "ticket_1" }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.conversation.resolve",
                "conv_sla_1",
                json!({ "required_fields": ["ticket_case_id"] }),
            ),
        )?;

        let conn = store::open_store(root.path())?;
        let applied_sla: String = conn.query_row(
            "SELECT payload_json FROM business_records
             WHERE collection = 'support_applied_slas'",
            [],
            |row| row.get(0),
        )?;
        let applied_sla: Value = serde_json::from_str(&applied_sla)?;
        assert_eq!(
            applied_sla.get("status").and_then(Value::as_str),
            Some("resolved")
        );
        assert!(
            applied_sla
                .get("breached_at_ms")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                > 0
        );
        let sla_events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection = 'support_sla_events'",
            [],
            |row| row.get(0),
        )?;
        assert!(sla_events >= 2);
        Ok(())
    }

    #[test]
    fn support_reply_send_creates_approval_handoff_and_reporting_rollup() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let session = admin_session();

        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.conversation.open_from_thread",
                "conv_reply_1",
                json!({
                    "conversation_id": "conv_reply_1",
                    "thread_key": "mail:reply-1",
                    "search_text": "Reply case"
                }),
            ),
        )?;
        let direct = handle_business_command(
            root.path(),
            &session,
            &command(
                "support.reply.send",
                "conv_reply_1",
                json!({
                    "conversation_id": "conv_reply_1",
                    "body": "Direct send should be blocked.",
                    "send_mode": "direct"
                }),
            ),
        );
        assert!(direct.is_err());

        let outcome = handle_business_command(
            root.path(),
            &session,
            &command(
                "support.reply.send",
                "conv_reply_1",
                json!({
                    "conversation_id": "conv_reply_1",
                    "body": "We will follow up after checking the logs.",
                    "attachment_file_ids": ["desktop_file_1"]
                }),
            ),
        )?;
        assert_eq!(
            outcome.get("status").and_then(Value::as_str),
            Some("pending_approval")
        );
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.reporting.rebuild_rollups",
                "",
                json!({ "id": "support-reporting-rebuild" }),
            ),
        )?;

        let conn = store::open_store(root.path())?;
        let event_payload: String = conn.query_row(
            "SELECT payload_json FROM business_records
             WHERE collection = 'support_conversation_events'
               AND payload_json LIKE '%desktop_file_1%'
             LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        let event_payload: Value = serde_json::from_str(&event_payload)?;
        assert_eq!(
            event_payload
                .pointer("/payload/attachment_refs/0/file_collection")
                .and_then(Value::as_str),
            Some("desktop_files")
        );
        let reporting_events: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records
             WHERE collection = 'support_reporting_events'",
            [],
            |row| row.get(0),
        )?;
        assert!(reporting_events >= 1);
        let rollups: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records
             WHERE collection = 'support_reporting_rollups'",
            [],
            |row| row.get(0),
        )?;
        assert!(rollups >= 1);
        Ok(())
    }

    #[test]
    fn support_saved_views_and_bulk_commands_project() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let session = admin_session();

        for conversation_id in ["conv_bulk_1", "conv_bulk_2"] {
            handle_business_command(
                root.path(),
                &session,
                &command(
                    "support.conversation.open_from_thread",
                    conversation_id,
                    json!({
                        "conversation_id": conversation_id,
                        "thread_key": format!("mail:{conversation_id}"),
                        "search_text": conversation_id
                    }),
                ),
            )?;
        }
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.view.upsert",
                "view_urgent",
                json!({
                    "id": "view_urgent",
                    "title": "Urgent open",
                    "scope": "team",
                    "filters": { "priority": "urgent", "status": "open" }
                }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.view_filter.upsert",
                "view_filter_urgent",
                json!({
                    "id": "view_filter_urgent",
                    "view_id": "view_urgent",
                    "field": "priority",
                    "operator": "eq",
                    "value": "urgent"
                }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.bulk.priority",
                "",
                json!({
                    "conversation_ids": ["conv_bulk_1", "conv_bulk_2"],
                    "priority": "urgent"
                }),
            ),
        )?;
        handle_business_command(
            root.path(),
            &session,
            &command(
                "support.bulk.resolve",
                "",
                json!({ "conversation_ids": ["conv_bulk_1", "conv_bulk_2"] }),
            ),
        )?;

        let conn = store::open_store(root.path())?;
        let views: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection = 'support_views'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(views, 1);
        let filters: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_records WHERE collection = 'support_view_filters'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(filters, 1);
        let resolved: i64 = conn.query_row(
            "SELECT COUNT(*) FROM support_conversations
             WHERE conversation_id IN ('conv_bulk_1', 'conv_bulk_2')
               AND status = 'resolved'
               AND priority = 'urgent'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(resolved, 2);
        Ok(())
    }
}
