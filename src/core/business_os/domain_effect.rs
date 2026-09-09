// Origin: CTOX
// License: AGPL-3.0-only

//! Domain application evidence, committed in the same SQLite transaction as
//! the effect. This is not a command lifecycle, claim store, or retry queue.
//! Core alone owns those decisions. Receipts retain results and references,
//! never historical projection payloads.

use anyhow::{ensure, Context};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS business_command_domain_effects (
    command_id TEXT PRIMARY KEY,
    payload_hash TEXT NOT NULL,
    actor_user_id TEXT NOT NULL,
    receipt_json TEXT NOT NULL
);";

/// Only native handlers select references; these are not browser input.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DomainRecordRef {
    pub collection: String,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AppliedDomainEffect {
    pub result: Value,
    pub projections: Vec<DomainRecordRef>,
}

/// Constructed by the command plane only after a NEW Core claim and central
/// authorization. An uncertain or terminal replay never receives this token.
pub(super) struct DomainEffectAdmission {
    command_id: String,
    payload_hash: String,
    actor_user_id: String,
}

pub(super) fn supports_command(command_type: &str) -> bool {
    matches!(
        command_type,
        "ctox.workjet.project.upsert"
            | "ctox.workjet.project.chat.ensure"
            | "ctox.workjet.project.worker.add"
            | "ctox.workjet.project.worker.remove"
            | "ctox.workjet.project.chat.create"
            | "ctox.workjet.worker_profile.bind"
            | "ctox.workjet.worker_profile.unbind"
    )
}

impl DomainEffectAdmission {
    pub(super) fn newly_claimed(
        command_id: &str,
        payload_hash: &str,
        actor_user_id: &str,
    ) -> anyhow::Result<Self> {
        ensure!(
            !command_id.trim().is_empty()
                && !payload_hash.trim().is_empty()
                && !actor_user_id.trim().is_empty(),
            "domain effect admission requires command, intent and authenticated actor"
        );
        Ok(Self {
            command_id: command_id.to_owned(),
            payload_hash: payload_hash.to_owned(),
            actor_user_id: actor_user_id.to_owned(),
        })
    }

    /// The closure may only mutate this transaction's local domain records.
    /// External effects, Core writes and RxDB publication are not atomic here.
    /// Returning an error rolls back BOTH the domain mutation and its receipt.
    pub(super) fn apply(
        &self,
        conn: &mut Connection,
        mutate: impl FnOnce(&Transaction<'_>) -> anyhow::Result<AppliedDomainEffect>,
    ) -> anyhow::Result<AppliedDomainEffect> {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(applied) = load(
            &tx,
            &self.command_id,
            &self.payload_hash,
            &self.actor_user_id,
        )? {
            tx.commit()?;
            return Ok(applied);
        }
        let applied = mutate(&tx)?;
        for reference in &applied.projections {
            ensure!(
                !reference.collection.trim().is_empty() && !reference.id.trim().is_empty(),
                "domain effect projection reference is empty"
            );
        }
        // Plain INSERT: an applied intent/result is immutable, never replaced.
        tx.execute(
            "INSERT INTO business_command_domain_effects
             (command_id, payload_hash, actor_user_id, receipt_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                self.command_id,
                self.payload_hash,
                self.actor_user_id,
                serde_json::to_string(&applied)?
            ],
        )?;
        tx.commit()?;
        Ok(applied)
    }
}

pub(super) fn contains(conn: &Connection, command_id: &str) -> anyhow::Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_command_domain_effects WHERE command_id = ?1)",
        [command_id],
        |row| row.get(0),
    )?)
}

pub(super) fn load(
    conn: &Connection,
    command_id: &str,
    payload_hash: &str,
    actor_user_id: &str,
) -> anyhow::Result<Option<AppliedDomainEffect>> {
    let row: Option<(String, String, String)> = conn
        .query_row(
            "SELECT payload_hash, actor_user_id, receipt_json
             FROM business_command_domain_effects WHERE command_id = ?1",
            [command_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((stored_hash, stored_actor, raw)) = row else {
        return Ok(None);
    };
    ensure!(stored_hash == payload_hash, "domain effect intent mismatch");
    ensure!(
        stored_actor == actor_user_id,
        "domain effect actor mismatch"
    );
    Ok(Some(
        serde_json::from_str(&raw).context("invalid durable domain effect receipt")?,
    ))
}

#[cfg(test)]
#[path = "domain_effect_tests.rs"]
mod tests;
