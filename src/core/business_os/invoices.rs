// invoices.rs — native command handlers for the invoices module.
// Origin: CTOX (invoices module port from business-basic).
// License: AGPL-3.0-only.
//
// This module is the dispatch target for `business_commands` whose
// `module == "invoices"`. Phase 3 ships the allowlist, ACL check, idempotency
// scaffolding and failure projection. Domain logic (post, journal, numbering,
// payment allocation, dunning, recurring) is wired in later phases.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::business_os::store::BusinessCommand;
use crate::business_os::store::BusinessOsSession;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Returns true when `command_type` is an invoices-module command the native
/// handler accepts. This is the single source of truth — keep the customers
/// pattern in `store.rs::is_customers_active_command` for reference.
pub fn is_invoices_active_command(command_type: &str) -> bool {
    matches!(
        command_type,
        "invoices.invoice.create"
            | "invoices.invoice.update"
            | "invoices.invoice.delete"
            | "invoices.invoice.post"
            | "invoices.invoice.cancel"
            | "invoices.invoice.create_credit_note"
            | "invoices.invoice.assign_payment_terms"
            | "invoices.line.create"
            | "invoices.line.update"
            | "invoices.line.delete"
            | "invoices.payment.allocate"
            | "invoices.payment.unallocate"
            | "invoices.payment.match_suggestions"
            | "invoices.dunning.run"
            | "invoices.dunning.letter.send"
            | "invoices.recurring.create"
            | "invoices.recurring.update"
            | "invoices.recurring.run"
            | "invoices.recurring.pause"
            | "invoices.import.from_outbound"
            | "invoices.proposal.create"
            | "invoices.proposal.approve"
            | "invoices.proposal.reject"
    )
}

/// Dispatch a single invoices command. The skeleton returns a clear `bail!` for
/// every command type. Phase 5+ will replace these with real handlers; the
/// allowlist and ACL stay.
pub fn handle_invoices_active_command(
    root: &Path,
    _session: &BusinessOsSession,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        command.module == "invoices",
        "invoices handler requires module=invoices, got module={}",
        command.module
    );
    if !is_invoices_active_command(&command.command_type) {
        return Err(anyhow!(
            "unsupported invoices command type: {}",
            command.command_type
        ));
    }
    // ACL placeholder: the real ACL is enforced in `BusinessOsSession` upstream
    // of this call. The `force` flag (used by `invoices.invoice.post` and
    // dunning bypass) is checked per-handler, never trusted from the UI.
    invoices_idempotency_guard(root, command)?;
    match command.command_type.as_str() {
        "invoices.invoice.create" => invoices_invoice_create_stub(root, command),
        "invoices.invoice.update" => invoices_invoice_update_stub(root, command),
        "invoices.invoice.delete" => invoices_invoice_delete_stub(root, command),
        "invoices.invoice.post" => invoices_invoice_post_stub(root, command),
        "invoices.invoice.cancel" => invoices_invoice_cancel(root, command),
        "invoices.invoice.create_credit_note" => invoices_invoice_create_credit_note(root, command),
        "invoices.invoice.assign_payment_terms" => {
            invoices_invoice_assign_payment_terms_stub(command)
        }
        "invoices.line.create" => invoices_line_create(root, command),
        "invoices.line.update" => invoices_line_update(root, command),
        "invoices.line.delete" => invoices_line_delete(root, command),
        "invoices.payment.allocate" => invoices_payment_allocate_stub(root, command),
        "invoices.payment.unallocate" => invoices_payment_unallocate_stub(root, command),
        "invoices.payment.match_suggestions" => invoices_payment_match_suggestions_stub(command),
        "invoices.dunning.run" => invoices_dunning_run_stub(root, command),
        "invoices.dunning.letter.send" => invoices_dunning_letter_send_stub(root, command),
        "invoices.recurring.create" => invoices_recurring_create_stub(command),
        "invoices.recurring.update" => invoices_recurring_update_stub(command),
        "invoices.recurring.run" => invoices_recurring_run_stub(command),
        "invoices.recurring.pause" => invoices_recurring_pause_stub(command),
        "invoices.import.from_outbound" => invoices_import_from_outbound_stub(command),
        "invoices.proposal.create" => invoices_proposal_create_stub(command),
        "invoices.proposal.approve" => invoices_proposal_approve_stub(command),
        "invoices.proposal.reject" => invoices_proposal_reject_stub(command),
        other => Err(anyhow!("unsupported invoices command type: {other}")),
    }
}

/// Idempotency scaffold. Real handlers will check the existing outcome for the
/// command_id and short-circuit on duplicates (replicated commands must not
/// double-post, double-allocate, or double-reserve numbers). Phase 3 keeps the
/// hook, no real persistence yet.
fn invoices_idempotency_guard(_root: &Path, _command: &BusinessCommand) -> anyhow::Result<()> {
    Ok(())
}

fn invoice_root_from_command(_command: &BusinessCommand) -> std::path::PathBuf {
    // The dispatch site in `store.rs` passes the session root via the
    // handler signature. The stub handlers ignore this helper; the
    // concrete create/update/delete handlers use the `root` argument
    // they receive from the dispatch site directly.
    std::path::PathBuf::from(".")
}

fn load_accounting_invoice(conn: &Connection, invoice_id: &str) -> anyhow::Result<Option<Value>> {
    load_business_record(conn, "accounting_invoices", invoice_id)
}

fn load_business_record(
    conn: &Connection,
    collection: &str,
    record_id: &str,
) -> anyhow::Result<Option<Value>> {
    let payload_json: Option<String> = conn
        .query_row(
            "SELECT payload_json
             FROM business_records
             WHERE collection = ?1
               AND record_id = ?2
               AND deleted = 0",
            rusqlite::params![collection, record_id],
            |row| row.get(0),
        )
        .optional()?;
    payload_json
        .map(|raw| serde_json::from_str(&raw).map_err(Into::into))
        .transpose()
}

fn invoices_upsert_invoice(
    conn: &Connection,
    invoice_id: &str,
    payload: &Value,
    updated_at_ms: i64,
) -> anyhow::Result<()> {
    let rev_str = format!("rev_{invoice_id}_{updated_at_ms}");
    let rev_for_payload = rev_str.clone();
    let mut stored = payload.clone();
    if let Some(obj) = stored.as_object_mut() {
        obj.insert("id".to_string(), Value::String(invoice_id.to_string()));
        obj.insert("_rev".to_string(), Value::String(rev_for_payload));
        obj.insert("_deleted".to_string(), Value::Bool(false));
        obj.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
    }
    conn.execute(
        "INSERT INTO business_records
            (collection, record_id, rev, deleted, updated_at_ms, payload_json)
         VALUES (?1, ?2, ?3, 0, ?4, ?5)
         ON CONFLICT(collection, record_id) DO UPDATE SET
            rev = excluded.rev,
            deleted = excluded.deleted,
            updated_at_ms = excluded.updated_at_ms,
            payload_json = excluded.payload_json",
        rusqlite::params![
            "accounting_invoices",
            invoice_id,
            rev_str,
            updated_at_ms,
            serde_json::to_string(&stored)?,
        ],
    )?;
    Ok(())
}

fn invoices_object_from_command(command: &BusinessCommand, invoice_id: &str, now: i64) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "invoice_id".to_string(),
        Value::String(invoice_id.to_string()),
    );
    obj.insert(
        "command_id".to_string(),
        json!(command.id.clone().unwrap_or_default()),
    );
    if let Some(payload) = command.payload.as_object() {
        for (key, value) in payload {
            if key == "invoice_id" || key == "id" {
                continue;
            }
            obj.insert(key.clone(), value.clone());
        }
    }
    obj.insert("currency".to_string(), Value::String("EUR".to_string()));
    if !obj.contains_key("subtotal_cents") {
        obj.insert("subtotal_cents".to_string(), Value::from(0));
    }
    if !obj.contains_key("tax_cents") {
        obj.insert("tax_cents".to_string(), Value::from(0));
    }
    if !obj.contains_key("total_cents") {
        obj.insert("total_cents".to_string(), Value::from(0));
    }
    if !obj.contains_key("paid_cents") {
        obj.insert("paid_cents".to_string(), Value::from(0));
    }
    if !obj.contains_key("open_cents") {
        obj.insert("open_cents".to_string(), Value::from(0));
    }
    obj.insert(
        "search_text".to_string(),
        Value::String(invoices_search_text_map(&obj)),
    );
    let _ = now;
    Value::Object(obj)
}

/// Native validation gate for `invoices.*` commands. Mirrors the JS rules in
/// `modules/invoices/core/invoice-validate.js` so a draft that the UI accepts
/// cannot be silently accepted or rejected by the native handler.
///
/// The native side enforces the bare minimum a GoBD-post needs (party + line
/// shape + tax_breakdown consistency) plus the cross-field guards that the JS
/// validator covers (reverse_charge, eu_ic_supply, small_business vs
/// tax_breakdown, skonto pairing). When a command arrives over WebRTC with a
/// malformed payload — for example, from an outdated client that has not yet
/// seen the empty-party fix in the editor — the native handler refuses it
/// with a precise `anyhow::Error` and the dispatch layer projects `status:
/// failed` on the `business_commands` row.
fn validate_invoice_for_command(invoice: &Value, strict_post: bool) -> anyhow::Result<()> {
    let obj = invoice
        .as_object()
        .ok_or_else(|| anyhow!("invoice must be a JSON object"))?;

    let invoice_type = obj
        .get("invoice_type")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !matches!(
        invoice_type,
        "sale_out" | "sale_in" | "credit_note_out" | "credit_note_in" | "recurring_template"
    ) {
        anyhow::bail!(
            "invoice_type must be one of: sale_out, sale_in, credit_note_out, credit_note_in, recurring_template (got: {invoice_type:?})"
        );
    }

    let party_id = obj
        .get("party_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if party_id.is_empty() {
        anyhow::bail!("party_id is required for invoices.* commands");
    }

    let currency = obj.get("currency").and_then(Value::as_str).unwrap_or("");
    if currency.is_empty() {
        anyhow::bail!("currency is required for invoices.* commands");
    }

    if let Some(ms) = obj.get("invoice_date_ms").and_then(Value::as_i64) {
        if ms <= 0 {
            anyhow::bail!("invoice_date_ms must be a positive integer ms timestamp");
        }
    } else if strict_post {
        anyhow::bail!("invoice_date_ms is required when posting");
    }

    let small_business = obj
        .get("small_business")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let reverse_charge = obj
        .get("reverse_charge")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let eu_ic_supply = obj
        .get("eu_ic_supply")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if small_business {
        if let Some(breakdown) = obj.get("tax_breakdown").and_then(Value::as_array) {
            if !breakdown.is_empty() {
                anyhow::bail!("small_business invoices must not carry tax_breakdown entries");
            }
        }
    }

    if reverse_charge && invoice_type != "sale_out" && invoice_type != "sale_in" {
        anyhow::bail!("reverse_charge is only valid for sale_out and sale_in invoices");
    }

    if eu_ic_supply && invoice_type != "sale_out" {
        anyhow::bail!(
            "eu_ic_supply (innergemeinschaftliche Lieferung) is only valid for sale_out invoices"
        );
    }

    if (invoice_type == "credit_note_out" || invoice_type == "credit_note_in")
        && obj
            .get("credit_note_for_id")
            .and_then(Value::as_str)
            .map(str::is_empty)
            .unwrap_or(true)
    {
        anyhow::bail!(
            "credit_note_{{out,in}} invoice types require a non-empty credit_note_for_id reference"
        );
    }

    let lines_value = obj.get("lines");
    if strict_post {
        let lines = lines_value
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("at least one line item is required when posting"))?;
        if lines.is_empty() {
            anyhow::bail!("at least one line item is required when posting");
        }
        for (idx, line) in lines.iter().enumerate() {
            let line_obj = line
                .as_object()
                .ok_or_else(|| anyhow!("lines[{idx}] must be a JSON object"))?;
            let description = line_obj
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("");
            if description.is_empty() {
                anyhow::bail!("lines[{idx}].description is required when posting");
            }
            let quantity = line_obj
                .get("quantity")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            if quantity <= 0 {
                anyhow::bail!("lines[{idx}].quantity must be a positive integer (in thousandths)");
            }
            let unit_price = line_obj.get("unit_price_cents").and_then(Value::as_i64);
            if unit_price.is_none() || unit_price.unwrap_or(0) < 0 {
                anyhow::bail!("lines[{idx}].unit_price_cents must be a non-negative integer");
            }
            let tax_rate = line_obj
                .get("tax_rate")
                .and_then(Value::as_f64)
                .unwrap_or(f64::NAN);
            if !tax_rate.is_finite() || tax_rate < 0.0 || tax_rate > 1.0 {
                anyhow::bail!("lines[{idx}].tax_rate must be in [0, 1]");
            }
            let account_code = line_obj
                .get("account_code")
                .and_then(Value::as_str)
                .unwrap_or("");
            if account_code.is_empty() {
                anyhow::bail!("lines[{idx}].account_code is required when posting");
            }
        }
    }

    if let Some(skonto_percent) = obj.get("skonto_percent").and_then(Value::as_f64) {
        if skonto_percent < 0.0 || skonto_percent > 100.0 {
            anyhow::bail!("skonto_percent must be in [0, 100]");
        }
        let skonto_days = obj
            .get("skonto_days")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        if skonto_days <= 0.0 {
            anyhow::bail!("skonto_days must be positive when skonto_percent is set");
        }
    }

    if let Some(breakdown) = obj.get("tax_breakdown").and_then(Value::as_array) {
        for (idx, entry) in breakdown.iter().enumerate() {
            let entry_obj = entry
                .as_object()
                .ok_or_else(|| anyhow!("tax_breakdown[{idx}] must be a JSON object"))?;
            let rate = entry_obj
                .get("tax_rate")
                .and_then(Value::as_f64)
                .unwrap_or(f64::NAN);
            if !rate.is_finite() || rate < 0.0 || rate > 1.0 {
                anyhow::bail!("tax_breakdown[{idx}].tax_rate must be in [0, 1]");
            }
            let net = entry_obj
                .get("net_cents")
                .and_then(Value::as_i64)
                .unwrap_or(-1);
            if net < 0 {
                anyhow::bail!("tax_breakdown[{idx}].net_cents must be a non-negative integer");
            }
            let tax = entry_obj
                .get("tax_cents")
                .and_then(Value::as_i64)
                .unwrap_or(-1);
            if tax < 0 {
                anyhow::bail!("tax_breakdown[{idx}].tax_cents must be a non-negative integer");
            }
        }
    }

    Ok(())
}

fn invoices_search_text_map(obj: &serde_json::Map<String, Value>) -> String {
    let mut parts: Vec<String> = Vec::new();
    for key in ["party_id", "invoice_number", "currency"] {
        if let Some(Value::String(s)) = obj.get(key) {
            parts.push(s.to_lowercase());
        }
    }
    parts.join(" ")
}

fn invoices_search_text(value: &Value) -> Option<String> {
    value.as_object().map(invoices_search_text_map)
}

fn invoices_record_requirement(
    command: &BusinessCommand,
    candidates: &[&str],
) -> anyhow::Result<String> {
    let payload = command.payload.as_object();
    for key in candidates {
        if let Some(value) = payload.and_then(|p| p.get(*key)) {
            if let Some(text) = value.as_str() {
                if !text.is_empty() {
                    return Ok(text.to_string());
                }
            }
        }
    }
    if let Some(record_id) = command.record_id.as_deref() {
        if !record_id.is_empty() {
            return Ok(record_id.to_string());
        }
    }
    Err(anyhow!(
        "missing required field for invoices command {} (expected one of {:?})",
        command.command_type,
        candidates
    ))
}

fn invoices_invoice_create_stub(root: &Path, command: &BusinessCommand) -> anyhow::Result<Value> {
    let invoice_id = invoices_record_requirement(command, &["invoice_id", "id"])?;
    let conn = open_business_os_store(root)?;
    if let Some(existing) = load_accounting_invoice(&conn, &invoice_id)? {
        return Ok(json!({
            "ok": true,
            "idempotent": true,
            "invoice": existing,
        }));
    }
    let now = now_ms() as i64;
    let mut invoice = invoices_object_from_command(command, &invoice_id, now);
    invoice["id"] = json!(invoice_id);
    invoice["state"] = json!("draft");
    invoice["is_deleted"] = json!(false);
    invoice["created_at_ms"] = json!(now);
    invoice["updated_at_ms"] = json!(now);
    // Validate the candidate invoice before persisting: a draft can still be
    // empty (no lines, no skonto), but it must at least have a party_id and a
    // recognised invoice_type so a stale client cannot poison the store with
    // unanchored records.
    validate_invoice_for_command(&invoice, false)?;
    invoices_upsert_invoice(&conn, &invoice_id, &invoice, now)?;
    Ok(json!({
        "ok": true,
        "invoice": invoice,
    }))
}

fn invoices_invoice_update_stub(root: &Path, command: &BusinessCommand) -> anyhow::Result<Value> {
    let invoice_id = invoices_record_requirement(command, &["invoice_id", "id"])?;
    let conn = open_business_os_store(root)?;
    let mut existing = load_accounting_invoice(&conn, &invoice_id)?
        .ok_or_else(|| anyhow!("invoice {invoice_id} not found"))?;
    let state = existing
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("draft");
    anyhow::ensure!(
        state == "draft",
        "invoices.invoice.update is only allowed in state=draft (current: {state})"
    );
    if let Some(payload) = command.payload.as_object() {
        for (key, value) in payload {
            if key == "invoice_id" || key == "id" {
                continue;
            }
            existing[key] = value.clone();
        }
    }
    // Re-validate the merged record with the same gate as create: a draft
    // can still be partial (no lines, no skonto), but party_id /
    // invoice_type / tax_breakdown consistency must hold.
    validate_invoice_for_command(&existing, false)?;
    let now = now_ms() as i64;
    existing["updated_at_ms"] = json!(now);
    if let Some(search_text) = invoices_search_text(&existing) {
        existing["search_text"] = json!(search_text);
    }
    invoices_upsert_invoice(&conn, &invoice_id, &existing, now)?;
    Ok(json!({
        "ok": true,
        "invoice": existing,
    }))
}

fn invoices_invoice_delete_stub(root: &Path, command: &BusinessCommand) -> anyhow::Result<Value> {
    let invoice_id = invoices_record_requirement(command, &["invoice_id", "id"])?;
    let conn = open_business_os_store(root)?;
    let mut existing = load_accounting_invoice(&conn, &invoice_id)?
        .ok_or_else(|| anyhow!("invoice {invoice_id} not found"))?;
    let state = existing
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("draft");
    anyhow::ensure!(
        state == "draft",
        "invoices.invoice.delete is only allowed in state=draft (current: {state})"
    );
    let now = now_ms() as i64;
    existing["is_deleted"] = json!(true);
    existing["deleted_at_ms"] = json!(now);
    existing["updated_at_ms"] = json!(now);
    // First upsert the payload with the soft-delete fields, then flip the
    // SQL `deleted` column to 1 so subsequent loads don't surface this row.
    // The order matters: `invoices_upsert_invoice` always sets `_deleted=false`
    // on the SQL column, so the tombstone must come after.
    invoices_upsert_invoice(&conn, &invoice_id, &existing, now)?;
    conn.execute(
        "UPDATE business_records SET deleted = 1, updated_at_ms = ?1
         WHERE collection = ?2 AND record_id = ?3",
        rusqlite::params![now, "accounting_invoices", &invoice_id],
    )?;
    Ok(json!({
        "ok": true,
        "invoice": existing,
    }))
}

fn invoices_invoice_post_stub(root: &Path, command: &BusinessCommand) -> anyhow::Result<Value> {
    let invoice_id = invoices_record_requirement(command, &["invoice_id", "id"])?;
    let conn = open_business_os_store(root)?;
    let now = now_ms() as i64;
    let command_id = command.id.clone().unwrap_or_default();
    post_invoice_in_conn(&conn, &invoice_id, &command_id, now)
}

/// Post a draft invoice (or credit note): reserve its number, build the balanced
/// journal entry (credit-note-aware — a credit_note_* posts a reversing entry),
/// persist it, and mark the invoice posted/GoBD-immutable. Idempotent by the
/// invoice's journal entry. Shared by invoices.invoice.post and the Storno path
/// in invoices.invoice.cancel.
fn post_invoice_in_conn(
    conn: &Connection,
    invoice_id: &str,
    command_id: &str,
    now: i64,
) -> anyhow::Result<Value> {
    // Idempotency: if a journal entry already references this invoice, return it.
    if let Some(existing_je) = find_journal_entry_for_invoice(conn, invoice_id)? {
        return Ok(json!({
            "ok": true,
            "idempotent": true,
            "journal_entry": existing_je,
        }));
    }

    // Atomic post: number reservation, invoice mutation, journal header + lines
    // and the final posted state commit together (or roll back together) so a
    // crash can never leave a half-posted ledger or a reserved-but-unused
    // number. The native peer is the single writer, so the savepoint also
    // serialises the number-series reservation.
    conn.execute_batch("SAVEPOINT post_invoice")?;
    let posted = (|| -> anyhow::Result<Value> {
    let mut invoice = load_accounting_invoice(conn, invoice_id)?
        .ok_or_else(|| anyhow!("invoice {invoice_id} not found"))?;
    let state = invoice
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("draft");
    anyhow::ensure!(
        state == "draft",
        "invoices.invoice.post requires state=draft (current: {state})"
    );

    // Strict post gate: a posted invoice must have a real customer, at least
    // one balanced line item, and consistent tax_breakdown / reverse_charge /
    // small_business flags. The JS validator in modules/invoices/core runs
    // the same checks at the editor boundary; this is the safety net for
    // paths that bypass the UI (ReST callers, App-Creator hardeners, future
    // recurring hooks).
    validate_invoice_for_command(&invoice, true)?;

    let fiscal_year = iso_year_from_ms(invoice_invoice_date_ms(&invoice));
    let party = resolve_party_account(conn, invoice_id)?;

    // 1. Reserve the next invoice number from accounting_number_series.
    let invoice_number = reserve_next_invoice_number(conn, &invoice, fiscal_year, now)?;
    invoice["invoice_number"] = json!(invoice_number);
    invoice["updated_at_ms"] = json!(now);

    // 1b. Compute the gross total from the line items (matches the JS
    // poster's balance check). Set subtotal/tax/total/paid/open on the
    // invoice so downstream commands (allocate, dunning) have the totals.
    // `quantity` is stored in thousandths (XRechnung/UBL convention):
    // quantity=1000 means 1.000 natural units, quantity=1500 means 1.500.
    // Net is therefore (gross_unit_cents * quantity) / 1000.
    let mut net_total: i64 = 0;
    let mut tax_total: i64 = 0;
    if let Some(lines) = invoice.get("lines").and_then(Value::as_array) {
        for line in lines {
            let quantity = line.get("quantity").and_then(Value::as_i64).unwrap_or(0);
            let unit_price = line
                .get("unit_price_cents")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let discount = line
                .get("discount_percent")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                .clamp(0.0, 100.0)
                / 100.0;
            let tax_rate = line.get("tax_rate").and_then(Value::as_f64).unwrap_or(0.0);
            let gross_unit = ((unit_price as f64) * (1.0 - discount)).round() as i64;
            let net = ((gross_unit as f64) * (quantity as f64) / 1000.0).round() as i64;
            let tax = ((net as f64) * tax_rate).round() as i64;
            net_total += net;
            tax_total += tax;
        }
    }
    let small_business = invoice
        .get("small_business")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let reverse_charge = invoice
        .get("reverse_charge")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let effective_tax = if small_business || reverse_charge {
        0
    } else {
        tax_total
    };
    let total_cents = net_total + effective_tax;
    invoice["subtotal_cents"] = json!(net_total);
    invoice["tax_cents"] = json!(effective_tax);
    invoice["total_cents"] = json!(total_cents);
    invoice["paid_cents"] = json!(0);
    invoice["open_cents"] = json!(total_cents);

    // 2. Build the journal entry payload (pure helper, see core/invoice-poster.js).
    //    We do not import the JS module here; instead we replicate the
    //    balanced journal shape via a Rust-side helper. JS-side validation
    //    is enforced at the command boundary; the native handler
    //    re-validates with the same rules below.
    let journal = invoices_build_journal_entry(&invoice, &party, &invoice_number, now)?;

    // 3. Persist the journal entry and lines.
    let journal_id = format!("je_{invoice_id}_post");
    invoices_upsert_invoice(conn, invoice_id, &invoice, now)?;
    upsert_business_record_helper(
        conn,
        "accounting_journal_entries",
        &journal_id,
        now,
        journal.header.clone(),
    )?;
    for (idx, line) in journal.lines.iter().enumerate() {
        let line_id = format!("{journal_id}_l{}", idx + 1);
        let line_id_for_record = line_id.clone();
        let mut line_doc = line.clone();
        if let Some(obj) = line_doc.as_object_mut() {
            obj.insert("id".to_string(), Value::String(line_id));
            obj.insert(
                "journal_entry_id".to_string(),
                Value::String(journal_id.clone()),
            );
            obj.insert("line_no".to_string(), Value::from(idx as i64 + 1));
            obj.insert("updated_at_ms".to_string(), Value::from(now));
        }
        upsert_business_record_helper(
            conn,
            "accounting_journal_entry_lines",
            &line_id_for_record,
            now,
            line_doc,
        )?;
    }

    // 4. Mark the invoice as posted and GoBD-immutable.
    invoice["state"] = json!("posted");
    invoice["post_journal_entry_id"] = json!(journal_id);
    invoice["posted_at"] = json!(now);
    invoice["state_changed_at_ms"] = json!(now);
    invoice["state_changed_by_command_id"] = json!(command_id);
    invoice["updated_at_ms"] = json!(now);
    invoices_upsert_invoice(conn, invoice_id, &invoice, now)?;

    Ok(json!({
        "ok": true,
        "invoice": invoice,
        "journal_entry": journal.header,
    }))
    })();
    match &posted {
        Ok(_) => conn.execute_batch("RELEASE SAVEPOINT post_invoice")?,
        Err(_) => {
            let _ = conn.execute_batch(
                "ROLLBACK TO SAVEPOINT post_invoice; RELEASE SAVEPOINT post_invoice",
            );
        }
    }
    posted
}

fn iso_year_from_ms(ms: i64) -> i64 {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    // Civil-date-from-days algorithm, Howard Hinnant's date.h. `yoe` counts years
    // from a March-1 era start, so January and February belong to the *next*
    // calendar year and need the `+ (m <= 2)` adjustment — omitting it put e.g.
    // 2025-01-01 in fiscal year 2024 and broke the RE-/GS- number series.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    y + i64::from(month <= 2)
}

fn invoice_invoice_date_ms(invoice: &Value) -> i64 {
    invoice
        .get("invoice_date_ms")
        .and_then(Value::as_i64)
        .unwrap_or_else(now_ms)
}

fn find_journal_entry_for_invoice(
    conn: &Connection,
    invoice_id: &str,
) -> anyhow::Result<Option<Value>> {
    // Exact JSON match: a LIKE pattern would treat `_`/`%` in the invoice id as
    // SQL wildcards and could match an unrelated journal entry, making a post
    // falsely idempotent and silently skipping a real post.
    let payload_json: Option<String> = conn
        .query_row(
            "SELECT payload_json
             FROM business_records
             WHERE collection = ?1
               AND deleted = 0
               AND json_extract(payload_json, '$.ref_id') = ?2
             LIMIT 1",
            rusqlite::params!["accounting_journal_entries", invoice_id],
            |row| row.get(0),
        )
        .optional()?;
    payload_json
        .map(|raw| serde_json::from_str(&raw).map_err(Into::into))
        .transpose()
}

fn resolve_party_account(conn: &Connection, invoice_id: &str) -> anyhow::Result<Value> {
    // For Phase 6 the native handler reads the party snapshot stored in
    // accounting_invoices.payload.party_snapshot; if absent it returns a
    // minimal party object with just the party_id. Customers module updates
    // the party_snapshot on update; the test invoice in Phase 5 already
    // stores party_id on the invoice.
    let invoice = load_accounting_invoice(conn, invoice_id)?
        .ok_or_else(|| anyhow!("invoice {invoice_id} not found"))?;
    let party_id = invoice
        .get("party_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Ok(json!({ "party_id": party_id }))
}

fn reserve_next_invoice_number(
    conn: &Connection,
    invoice: &Value,
    fiscal_year: i64,
    now: i64,
) -> anyhow::Result<String> {
    let series_key = invoice
        .get("invoice_type")
        .and_then(Value::as_str)
        .unwrap_or("sale_out");
    let series_id = format!("{series_key}_{fiscal_year}");
    // Look up the existing series, or create one with next_value=1.
    let payload_json: Option<String> = conn
        .query_row(
            "SELECT payload_json
             FROM business_records
             WHERE collection = ?1
               AND record_id = ?2
               AND deleted = 0",
            rusqlite::params!["accounting_number_series", &series_id],
            |row| row.get(0),
        )
        .optional()?;
    let next_value: i64 = if let Some(raw) = payload_json {
        let stored: Value = serde_json::from_str(&raw)?;
        stored
            .get("next_value")
            .and_then(Value::as_i64)
            .unwrap_or(1)
    } else {
        1
    };
    let prefix = match series_key {
        "sale_out" => "RE-",
        "sale_in" => "ER-",
        "credit_note_out" => "GS-",
        "credit_note_in" => "EG-",
        "recurring_template" => "RT-",
        _ => "RE-",
    };
    let invoice_number = format!("{prefix}{fiscal_year}-{:04}", next_value);
    let series = json!({
        "id": series_id,
        "series_key": series_key,
        "fiscal_year": fiscal_year,
        "prefix": prefix,
        "next_value": next_value + 1,
        "last_issued_number": invoice_number,
        "gap_policy": "strict_no_gaps",
        "updated_at_ms": now,
        "payload": {}
    });
    upsert_business_record_helper(conn, "accounting_number_series", &series_id, now, series)?;
    Ok(invoice_number)
}

#[derive(Debug)]
struct JournalEntryBuild {
    header: Value,
    lines: Vec<Value>,
}

fn invoices_build_journal_entry(
    invoice: &Value,
    party: &Value,
    invoice_number: &str,
    now: i64,
) -> anyhow::Result<JournalEntryBuild> {
    let invoice_type = invoice
        .get("invoice_type")
        .and_then(Value::as_str)
        .unwrap_or("sale_out");
    let is_credit_note = invoice_type == "credit_note_out" || invoice_type == "credit_note_in";
    let is_input = invoice_type == "sale_in" || invoice_type == "credit_note_in";
    let small_business = invoice
        .get("small_business")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let reverse_charge = invoice
        .get("reverse_charge")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let party_id = party
        .get("party_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let party_account_id = match invoice_type {
        "sale_out" | "credit_note_out" => "1400",
        "sale_in" | "credit_note_in" => "1600",
        _ => "1400",
    };
    let revenue_is_debit = is_input != is_credit_note;
    let tax_is_debit = is_input != is_credit_note;
    let party_is_credit = is_input != is_credit_note;

    let lines_arr = invoice
        .get("lines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut revenue_lines: Vec<Value> = Vec::new();
    let mut tax_lines: Vec<Value> = Vec::new();
    let mut net_total: i64 = 0;
    let mut tax_total: i64 = 0;
    let mut tax_by_rate: std::collections::BTreeMap<String, (f64, i64)> =
        std::collections::BTreeMap::new();
    for line in &lines_arr {
        let quantity = line.get("quantity").and_then(Value::as_i64).unwrap_or(0);
        let unit_price = line
            .get("unit_price_cents")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let discount = line
            .get("discount_percent")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .clamp(0.0, 100.0)
            / 100.0;
        let tax_rate = line.get("tax_rate").and_then(Value::as_f64).unwrap_or(0.0);
        let gross_unit = ((unit_price as f64) * (1.0 - discount)).round() as i64;
        let net = ((gross_unit as f64) * (quantity as f64) / 1000.0).round() as i64;
        let tax = ((net as f64) * tax_rate).round() as i64;
        net_total += net;
        tax_total += tax;
        let description = line
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let account_code = line
            .get("account_code")
            .and_then(Value::as_str)
            .unwrap_or(if is_input { "3400" } else { "8400" })
            .to_string();
        revenue_lines.push(json!({
            "account_id": account_code,
            "debit": revenue_is_debit.then_some(net).unwrap_or(0),
            "credit": (!revenue_is_debit).then_some(net).unwrap_or(0),
            "description": description,
        }));
        let key = format!("{:.4}", tax_rate);
        let entry = tax_by_rate.entry(key).or_insert((tax_rate, 0));
        entry.1 += tax;
    }

    if !small_business && !reverse_charge {
        for (_key, (tax_rate, tax_cents)) in &tax_by_rate {
            let account_id = if (tax_rate - 0.19).abs() < 1e-9 {
                if is_input {
                    "1406"
                } else {
                    "3806"
                }
            } else if (tax_rate - 0.07).abs() < 1e-9 {
                if is_input {
                    "1407"
                } else {
                    "3801"
                }
            } else {
                ""
            };
            if account_id.is_empty() {
                continue;
            }
            tax_lines.push(json!({
                "account_id": account_id,
                "debit": tax_is_debit.then_some(*tax_cents).unwrap_or(0),
                "credit": (!tax_is_debit).then_some(*tax_cents).unwrap_or(0),
                "tax_rate_id": format!("tax_{tax_rate}"),
            }));
        }
    }

    let party_base = if tax_lines.is_empty() {
        net_total
    } else {
        net_total + tax_total
    };
    let party_line = json!({
        "account_id": party_account_id,
        "debit": (!party_is_credit).then_some(party_base).unwrap_or(0),
        "credit": party_is_credit.then_some(party_base).unwrap_or(0),
        "party_id": party_id,
    });

    let mut all_lines: Vec<Value> = Vec::new();
    all_lines.extend(revenue_lines);
    all_lines.extend(tax_lines);
    all_lines.push(party_line);

    let total_debit: i64 = all_lines
        .iter()
        .map(|l| l.get("debit").and_then(Value::as_i64).unwrap_or(0))
        .sum();
    let total_credit: i64 = all_lines
        .iter()
        .map(|l| l.get("credit").and_then(Value::as_i64).unwrap_or(0))
        .sum();
    anyhow::ensure!(
        total_debit == total_credit,
        "journal entry unbalanced: debit={} credit={}",
        total_debit,
        total_credit
    );

    let posting_date = invoice
        .get("invoice_date_ms")
        .and_then(Value::as_i64)
        .map(iso_date_from_ms)
        .unwrap_or_else(|| iso_date_from_ms(now));
    let narration = if is_credit_note {
        format!("Credit note for {invoice_number}")
    } else {
        format!("Invoice {invoice_number}")
    };
    let invoice_id = invoice
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let header = json!({
        "id": format!("je_{invoice_id}_post"),
        "posting_date": posting_date,
        "type": "invoice",
        "ref_type": "invoice",
        "ref_id": invoice_id,
        "number": invoice_number,
        "narration": narration,
        "total_debit_cents": total_debit,
        "total_credit_cents": total_credit,
        "balanced": true,
        "posted_at": now,
        "updated_at_ms": now,
    });
    Ok(JournalEntryBuild {
        header,
        lines: all_lines,
    })
}

fn iso_date_from_ms(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    // Howard Hinnant's date.h — civil_from_days.
    let z = days + 719_468;
    let era = if z >= 0 {
        z / 146_097
    } else {
        (z - 146_096) / 146_097
    };
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", year, m, d)
}

fn upsert_business_record_helper(
    conn: &Connection,
    collection: &str,
    record_id: &str,
    updated_at_ms: i64,
    mut payload: Value,
) -> anyhow::Result<()> {
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("id".to_string(), Value::String(record_id.to_string()));
        obj.insert(
            "_rev".to_string(),
            Value::String(format!("rev_{record_id}_{updated_at_ms}")),
        );
        obj.insert("_deleted".to_string(), Value::Bool(false));
        obj.insert("updated_at_ms".to_string(), Value::from(updated_at_ms));
    }
    let payload_json = serde_json::to_string(&payload)?;
    conn.execute(
        "INSERT INTO business_records
            (collection, record_id, rev, deleted, updated_at_ms, payload_json)
         VALUES (?1, ?2, ?3, 0, ?4, ?5)
         ON CONFLICT(collection, record_id) DO UPDATE SET
            rev = excluded.rev,
            deleted = excluded.deleted,
            updated_at_ms = excluded.updated_at_ms,
            payload_json = excluded.payload_json",
        rusqlite::params![
            collection,
            record_id,
            format!("rev_{record_id}_{updated_at_ms}"),
            updated_at_ms,
            payload_json,
        ],
    )?;
    Ok(())
}

/// Build a draft credit note (Gegenrechnung) that reverses `original`. §17 UStG:
/// a posted invoice is corrected by a credit note, never edited in place. The
/// credit note copies the original's party/currency and its lines (or a
/// caller-supplied partial `lines`) and references it via `credit_note_for_id`.
/// Returned in `state=draft` so it goes through the normal post path (which
/// builds the reversing journal entry and reserves a credit-note number).
fn build_credit_note_draft(
    original: &Value,
    new_id: &str,
    command: &BusinessCommand,
    now: i64,
) -> anyhow::Result<Value> {
    let original_type = original
        .get("invoice_type")
        .and_then(Value::as_str)
        .unwrap_or("");
    let cn_type = match original_type {
        "sale_out" => "credit_note_out",
        "sale_in" => "credit_note_in",
        other => anyhow::bail!(
            "cannot credit an invoice of type {other:?} (only sale_out / sale_in can be credited)"
        ),
    };
    let credit_note_for_id = original
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let party_id = original
        .get("party_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let currency = original
        .get("currency")
        .and_then(Value::as_str)
        .unwrap_or("EUR")
        .to_string();
    let original_reverse_charge = original
        .get("reverse_charge")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let original_small_business = original
        .get("small_business")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let raw_lines = command
        .payload
        .get("lines")
        .cloned()
        .or_else(|| original.get("lines").cloned())
        .unwrap_or_else(|| json!([]));
    // Under reverse charge OR small-business (§19 UStG) the supplier shows no
    // output tax, so the credit note must stay tax-free too. We cannot carry the
    // `reverse_charge` flag onto a credit_note_* type (validate_invoice_for_command
    // rejects it), and even with `small_business` carried the copied line
    // `tax_rate` would read as taxable in exports — so neutralise per-line tax.
    let tax_free_origin = original_reverse_charge || original_small_business;
    let lines = if tax_free_origin {
        Value::Array(
            raw_lines
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|mut line| {
                    if let Some(obj) = line.as_object_mut() {
                        obj.insert("tax_rate".to_string(), json!(0.0));
                    }
                    line
                })
                .collect(),
        )
    } else {
        raw_lines
    };
    let invoice_number = original
        .get("invoice_number")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let search_text = format!("{} {}", party_id.to_lowercase(), currency.to_lowercase());
    let cn = json!({
        "id": new_id,
        "invoice_type": cn_type,
        "credit_note_for_id": credit_note_for_id,
        "credit_note_for_number": invoice_number,
        "party_id": party_id,
        "currency": currency,
        "invoice_date_ms": command.payload.get("invoice_date_ms").and_then(Value::as_i64).unwrap_or(now),
        "lines": lines,
        "reason": command.payload.get("reason").and_then(Value::as_str).unwrap_or("§17 UStG Korrektur"),
        "small_business": original.get("small_business").cloned().unwrap_or(json!(false)),
        "reverse_charge": false,
        "reverse_charge_origin": original_reverse_charge,
        "state": "draft",
        "is_deleted": false,
        "search_text": search_text,
        "subtotal_cents": 0,
        "tax_cents": 0,
        "total_cents": 0,
        "paid_cents": 0,
        "open_cents": 0,
        "created_at_ms": now,
        "updated_at_ms": now,
    });
    validate_invoice_for_command(&cn, false)?;
    Ok(cn)
}

/// §5.11 Storno: cancel an invoice. A draft is voided in place; a posted invoice
/// is reversed by a credit note (GoBD: posted records are immutable) and marked
/// `state=cancelled` with a link to the Storno credit note.
fn invoices_invoice_cancel(root: &Path, command: &BusinessCommand) -> anyhow::Result<Value> {
    let invoice_id = invoices_record_requirement(command, &["invoice_id", "id"])?;
    let conn = open_business_os_store(root)?;
    let mut invoice = load_accounting_invoice(&conn, &invoice_id)?
        .ok_or_else(|| anyhow!("invoice {invoice_id} not found"))?;
    let state = invoice
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("draft")
        .to_string();
    let now = now_ms() as i64;
    match state.as_str() {
        "cancelled" => Ok(json!({
            "ok": true,
            "idempotent": true,
            "invoice_id": invoice_id,
            "state": "cancelled",
        })),
        "draft" => {
            invoice["state"] = json!("cancelled");
            invoice["is_deleted"] = json!(true);
            invoice["cancelled_at_ms"] = json!(now);
            invoice["updated_at_ms"] = json!(now);
            invoices_upsert_invoice(&conn, &invoice_id, &invoice, now)?;
            conn.execute(
                "UPDATE business_records SET deleted = 1, updated_at_ms = ?1
                 WHERE collection = ?2 AND record_id = ?3",
                rusqlite::params![now, "accounting_invoices", &invoice_id],
            )?;
            Ok(json!({ "ok": true, "invoice_id": invoice_id, "state": "cancelled" }))
        }
        "posted" => {
            // GoBD Storno: a posted invoice is reversed by a credit note that is
            // itself POSTED (writing the reversing journal entry), never by a
            // dangling draft. Create the Storno draft once, post it so the ledger
            // is actually reversed, then mark the original cancelled. Posting
            // first means there is no "cancelled but unreversed" window; the post
            // step is idempotent by the credit note's journal entry.
            let cn_id = format!("cn_{invoice_id}_storno");
            // Rebuild the Storno draft from the authoritative original every time
            // (overwriting any pre-existing/planted draft under that id) unless it
            // is already posted — never trust a stored draft to be a correct
            // reversal of THIS invoice.
            let already_posted = load_accounting_invoice(&conn, &cn_id)?
                .as_ref()
                .and_then(|c| c.get("state"))
                .and_then(Value::as_str)
                == Some("posted");
            if !already_posted {
                let cn = build_credit_note_draft(&invoice, &cn_id, command, now)?;
                invoices_upsert_invoice(&conn, &cn_id, &cn, now)?;
            }
            let command_id = command.id.clone().unwrap_or_default();
            let storno_posted = post_invoice_in_conn(&conn, &cn_id, &command_id, now)?;
            invoice["state"] = json!("cancelled");
            invoice["cancelled_at_ms"] = json!(now);
            invoice["storno_credit_note_id"] = json!(cn_id);
            invoice["updated_at_ms"] = json!(now);
            invoices_upsert_invoice(&conn, &invoice_id, &invoice, now)?;
            Ok(json!({
                "ok": true,
                "invoice_id": invoice_id,
                "state": "cancelled",
                "storno_credit_note_id": cn_id,
                "storno_posted": storno_posted,
            }))
        }
        other => anyhow::bail!("cannot cancel invoice in state {other} (only draft or posted)"),
    }
}

/// §5.11 §17 UStG credit note: create a draft credit note that references a
/// posted invoice via `credit_note_for_id`. The draft is then posted through the
/// normal post path.
fn invoices_invoice_create_credit_note(
    root: &Path,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let credit_note_for_id =
        invoices_record_requirement(command, &["credit_note_for_id", "invoice_id", "id"])?;
    let now = now_ms() as i64;
    // Idempotency: default to a deterministic id per original so a replayed
    // command returns the existing credit note instead of minting a new
    // postable cn_{now} draft each time. A caller wanting multiple/partial
    // credit notes for one invoice must supply an explicit credit_note_id.
    let new_id = command
        .payload
        .get("credit_note_id")
        .or_else(|| command.payload.get("new_invoice_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("cn_{credit_note_for_id}"));
    let conn = open_business_os_store(root)?;
    if let Some(existing) = load_accounting_invoice(&conn, &new_id)? {
        return Ok(json!({ "ok": true, "idempotent": true, "invoice": existing }));
    }
    let original = load_accounting_invoice(&conn, &credit_note_for_id)?
        .ok_or_else(|| anyhow!("invoice {credit_note_for_id} not found"))?;
    let state = original
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("draft");
    anyhow::ensure!(
        state == "posted",
        "credit note requires a posted invoice to correct (state: {state})"
    );
    let cn = build_credit_note_draft(&original, &new_id, command, now)?;
    // Cumulative cap: the sum of all non-deleted credit notes for this invoice
    // (existing + the new one) must not exceed its net, otherwise repeated
    // explicit-id credit notes would over-credit the original.
    let line_net = |inv: &Value| -> i64 {
        inv.get("lines")
            .and_then(Value::as_array)
            .map(|lines| {
                lines
                    .iter()
                    .map(|line| {
                        let q = line.get("quantity").and_then(Value::as_i64).unwrap_or(0);
                        let up = line
                            .get("unit_price_cents")
                            .and_then(Value::as_i64)
                            .unwrap_or(0);
                        let d = line
                            .get("discount_percent")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0)
                            .clamp(0.0, 100.0)
                            / 100.0;
                        let gross_unit = ((up as f64) * (1.0 - d)).round() as i64;
                        ((gross_unit as f64) * (q as f64) / 1000.0).round() as i64
                    })
                    .sum()
            })
            .unwrap_or(0)
    };
    let original_net = original
        .get("subtotal_cents")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| line_net(&original));
    let mut existing_net = 0i64;
    {
        let mut stmt = conn.prepare(
            "SELECT record_id, payload_json FROM business_records
             WHERE collection = 'accounting_invoices' AND deleted = 0
               AND json_extract(payload_json, '$.credit_note_for_id') = ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![credit_note_for_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (rid, raw) = row?;
            if rid == new_id {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(&raw) {
                existing_net += line_net(&value);
            }
        }
    }
    let new_net = line_net(&cn);
    anyhow::ensure!(
        existing_net + new_net <= original_net,
        "credit notes for {credit_note_for_id} would exceed the invoice net ({existing_net} + {new_net} > {original_net})"
    );
    invoices_upsert_invoice(&conn, &new_id, &cn, now)?;
    Ok(json!({ "ok": true, "invoice": cn }))
}

fn invoices_invoice_assign_payment_terms_stub(_command: &BusinessCommand) -> anyhow::Result<Value> {
    anyhow::bail!(
        "invoices.invoice.assign_payment_terms is not yet implemented (planned for Phase 7+); \
         the payment terms are set in the invoice draft and cannot be reassigned post-post"
    )
}

/// Load a draft invoice and its current lines array. Line edits are only valid
/// while the invoice is a draft (a posted invoice is GoBD-immutable).
fn load_draft_invoice_lines(
    conn: &Connection,
    invoice_id: &str,
) -> anyhow::Result<(Value, Vec<Value>)> {
    let invoice = load_accounting_invoice(conn, invoice_id)?
        .ok_or_else(|| anyhow!("invoice {invoice_id} not found"))?;
    let state = invoice
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("draft");
    anyhow::ensure!(
        state == "draft",
        "invoices.line.* is only allowed in state=draft (current: {state})"
    );
    let lines = invoice
        .get("lines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok((invoice, lines))
}

fn save_draft_invoice_lines(
    conn: &Connection,
    invoice_id: &str,
    mut invoice: Value,
    lines: Vec<Value>,
    now: i64,
) -> anyhow::Result<Value> {
    invoice["lines"] = json!(lines);
    invoice["updated_at_ms"] = json!(now);
    validate_invoice_for_command(&invoice, false)?;
    invoices_upsert_invoice(conn, invoice_id, &invoice, now)?;
    Ok(invoice)
}

fn invoices_line_create(root: &Path, command: &BusinessCommand) -> anyhow::Result<Value> {
    let invoice_id = invoices_record_requirement(command, &["invoice_id"])?;
    let line = command
        .payload
        .get("line")
        .cloned()
        .ok_or_else(|| anyhow!("invoices.line.create requires a 'line' object"))?;
    anyhow::ensure!(line.is_object(), "invoices.line.create 'line' must be an object");
    let conn = open_business_os_store(root)?;
    let now = now_ms() as i64;
    let (invoice, mut lines) = load_draft_invoice_lines(&conn, &invoice_id)?;
    lines.push(line);
    let invoice = save_draft_invoice_lines(&conn, &invoice_id, invoice, lines, now)?;
    Ok(json!({ "ok": true, "invoice": invoice }))
}

fn invoices_line_update(root: &Path, command: &BusinessCommand) -> anyhow::Result<Value> {
    let invoice_id = invoices_record_requirement(command, &["invoice_id"])?;
    let index = command
        .payload
        .get("line_index")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("invoices.line.update requires a 'line_index'"))?
        as usize;
    let patch = command
        .payload
        .get("line")
        .cloned()
        .ok_or_else(|| anyhow!("invoices.line.update requires a 'line' object"))?;
    let conn = open_business_os_store(root)?;
    let now = now_ms() as i64;
    let (invoice, mut lines) = load_draft_invoice_lines(&conn, &invoice_id)?;
    let line_count = lines.len();
    let target = lines
        .get_mut(index)
        .ok_or_else(|| anyhow!("line_index {index} out of range ({line_count} lines)"))?;
    if let (Some(target_obj), Some(patch_obj)) = (target.as_object_mut(), patch.as_object()) {
        for (key, value) in patch_obj {
            target_obj.insert(key.clone(), value.clone());
        }
    } else {
        *target = patch;
    }
    let invoice = save_draft_invoice_lines(&conn, &invoice_id, invoice, lines, now)?;
    Ok(json!({ "ok": true, "invoice": invoice }))
}

fn invoices_line_delete(root: &Path, command: &BusinessCommand) -> anyhow::Result<Value> {
    let invoice_id = invoices_record_requirement(command, &["invoice_id"])?;
    let index = command
        .payload
        .get("line_index")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("invoices.line.delete requires a 'line_index'"))?
        as usize;
    let conn = open_business_os_store(root)?;
    let now = now_ms() as i64;
    let (invoice, mut lines) = load_draft_invoice_lines(&conn, &invoice_id)?;
    anyhow::ensure!(
        index < lines.len(),
        "line_index {index} out of range ({} lines)",
        lines.len()
    );
    lines.remove(index);
    let invoice = save_draft_invoice_lines(&conn, &invoice_id, invoice, lines, now)?;
    Ok(json!({ "ok": true, "invoice": invoice }))
}

fn invoices_payment_allocate_stub(root: &Path, command: &BusinessCommand) -> anyhow::Result<Value> {
    let invoice_id = invoices_record_requirement(command, &["invoice_id", "id"])?;
    let payment_id = command
        .payload
        .get("payment_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let amount_cents = command
        .payload
        .get("amount_cents")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let skonto_cents = command
        .payload
        .get("skonto_cents")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    anyhow::ensure!(
        amount_cents > 0 || skonto_cents > 0,
        "invoices.payment.allocate requires amount_cents or skonto_cents > 0"
    );
    let conn = open_business_os_store(root)?;

    // Idempotency: a payment can only be allocated to an invoice once.
    // We use the deterministic (payment_id, invoice_id) tuple as the
    // allocation primary key. Note that `accept_rxdb_business_command`
    // already short-circuits on a duplicate command_id, so this guard
    // additionally covers the case where two distinct commands try to
    // allocate the same payment twice.
    let command_id = command
        .id
        .clone()
        .unwrap_or_else(|| format!("alloc_{invoice_id}_{}", now_ms()));
    let payment_id_for_id = payment_id
        .clone()
        .unwrap_or_else(|| format!("no_payment_{command_id}"));
    let allocation_id = format!("alloc_{payment_id_for_id}_{invoice_id}");
    if let Some(existing) =
        load_business_record(&conn, "accounting_payment_allocations", &allocation_id)?
    {
        let current_invoice = load_accounting_invoice(&conn, &invoice_id)?;
        return Ok(json!({
            "ok": true,
            "idempotent": true,
            "allocation": existing,
            "invoice": current_invoice.unwrap_or(Value::Null),
        }));
    }

    let mut invoice = load_accounting_invoice(&conn, &invoice_id)?
        .ok_or_else(|| anyhow!("invoice {invoice_id} not found"))?;
    let state = invoice
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("draft");
    anyhow::ensure!(
        matches!(state, "posted" | "partially_paid" | "paid" | "overdue"),
        "invoices.payment.allocate requires posted/overdue invoice (current: {state})"
    );
    let total_cents = invoice
        .get("total_cents")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let prev_paid = invoice
        .get("paid_cents")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let effective = amount_cents + skonto_cents;
    let new_paid = prev_paid + effective;
    anyhow::ensure!(
        new_paid <= total_cents,
        "allocation exceeds open invoice balance: total={total_cents} paid={prev_paid} applied={effective}"
    );
    let now = now_ms() as i64;
    let open_cents = total_cents - new_paid;
    let new_state = if new_paid >= total_cents {
        "paid"
    } else {
        "partially_paid"
    };
    invoice["paid_cents"] = json!(new_paid);
    invoice["open_cents"] = json!(open_cents);
    invoice["state"] = json!(new_state);
    invoice["state_changed_at_ms"] = json!(now);
    invoice["state_changed_by_command_id"] = json!(command_id.clone());
    invoice["updated_at_ms"] = json!(now);
    invoices_upsert_invoice(&conn, &invoice_id, &invoice, now)?;

    let allocation = json!({
        "id": allocation_id,
        "payment_id": payment_id,
        "invoice_id": invoice_id,
        "allocated_cents": amount_cents,
        "skonto_cents": skonto_cents,
        "note": null,
        "allocated_at_ms": now,
        "allocated_by_command_id": command_id,
        "updated_at_ms": now,
    });
    upsert_business_record_helper(
        &conn,
        "accounting_payment_allocations",
        &allocation_id,
        now,
        allocation.clone(),
    )?;
    Ok(json!({
        "ok": true,
        "invoice": invoice,
        "allocation": allocation,
    }))
}

fn invoices_payment_unallocate_stub(
    root: &Path,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let allocation_id = invoices_record_requirement(command, &["allocation_id", "id"])?;
    let conn = open_business_os_store(root)?;
    let mut allocation =
        load_business_record(&conn, "accounting_payment_allocations", &allocation_id)?
            .ok_or_else(|| anyhow!("allocation {allocation_id} not found"))?;
    let invoice_id = allocation
        .get("invoice_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("allocation missing invoice_id"))?
        .to_string();
    let allocated = allocation
        .get("allocated_cents")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let skonto = allocation
        .get("skonto_cents")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let mut invoice = load_accounting_invoice(&conn, &invoice_id)?
        .ok_or_else(|| anyhow!("invoice {invoice_id} not found"))?;
    let now = now_ms() as i64;
    let prev_paid = invoice
        .get("paid_cents")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let new_paid = (prev_paid - allocated - skonto).max(0);
    let total_cents = invoice
        .get("total_cents")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    invoice["paid_cents"] = json!(new_paid);
    invoice["open_cents"] = json!(total_cents - new_paid);
    invoice["state"] = json!(if new_paid <= 0 {
        "posted"
    } else if new_paid < total_cents {
        "partially_paid"
    } else {
        "paid"
    });
    invoice["state_changed_at_ms"] = json!(now);
    invoice["state_changed_by_command_id"] = json!(command.id.clone().unwrap_or_default());
    invoice["updated_at_ms"] = json!(now);
    invoices_upsert_invoice(&conn, &invoice_id, &invoice, now)?;
    allocation["is_deleted"] = json!(true);
    allocation["deleted_at_ms"] = json!(now);
    conn.execute(
        "UPDATE business_records SET deleted = 1, updated_at_ms = ?1
         WHERE collection = ?2 AND record_id = ?3",
        rusqlite::params![now, "accounting_payment_allocations", &allocation_id],
    )?;
    Ok(json!({
        "ok": true,
        "invoice": invoice,
    }))
}

fn invoices_payment_match_suggestions_stub(_command: &BusinessCommand) -> anyhow::Result<Value> {
    Ok(json!({
        "ok": true,
        "stub": true,
        "command": "invoices.payment.match_suggestions",
        "phase": 3,
        "suggestions": []
    }))
}

fn invoices_dunning_run_stub(root: &Path, command: &BusinessCommand) -> anyhow::Result<Value> {
    let run_id = invoices_record_requirement(command, &["run_id", "id"])?;
    let conn = open_business_os_store(root)?;
    if let Some(existing_run) = load_business_record(&conn, "accounting_dunning_runs", &run_id)? {
        return Ok(json!({
            "ok": true,
            "idempotent": true,
            "run": existing_run,
        }));
    }
    let now = now_ms() as i64;
    let filter = command
        .payload
        .get("filter")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let invoices_total = invoices_list_due_invoices(&conn, &filter, now)?;
    let run = json!({
        "id": run_id,
        "run_date_ms": now,
        "run_by": command
            .client_context
            .get("actor")
            .and_then(|a| a.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("system"),
        "filter": filter,
        "invoices_total": invoices_total,
        "letters_sent": 0,
        "state": "draft",
        "created_at_ms": now,
        "updated_at_ms": now,
    });
    upsert_business_record_helper(&conn, "accounting_dunning_runs", &run_id, now, run.clone())?;

    // Build one letter per eligible invoice. Letters start at level 1
    // (Erinnerung). Re-running the dunning run for the same run_id does
    // not double-issue (idempotency above).
    let mut letters: Vec<Value> = Vec::new();
    for invoice_id in &invoices_total {
        let letter_id = format!("let_{run_id}_{invoice_id}");
        let letter = json!({
            "id": letter_id,
            "dunning_run_id": run_id,
            "invoice_id": invoice_id,
            "level": 1,
            "letter_date_ms": now,
            "fee_cents": 0,
            "interest_cents": 0,
            "total_cents": 0,
            "pdf_attachment_id": null,
            "sent_via": null,
            "sent_at_ms": null,
            "status": "draft",
            "created_at_ms": now,
            "updated_at_ms": now,
        });
        upsert_business_record_helper(
            &conn,
            "accounting_dunning_letters",
            &letter_id,
            now,
            letter.clone(),
        )?;
        letters.push(letter);
    }
    Ok(json!({
        "ok": true,
        "run": run,
        "letters": letters,
    }))
}

fn invoices_dunning_letter_send_stub(
    root: &Path,
    command: &BusinessCommand,
) -> anyhow::Result<Value> {
    let letter_id = invoices_record_requirement(command, &["letter_id", "id"])?;
    let conn = open_business_os_store(root)?;
    let mut letter = load_business_record(&conn, "accounting_dunning_letters", &letter_id)?
        .ok_or_else(|| anyhow!("dunning letter {letter_id} not found"))?;
    let now = now_ms() as i64;
    letter["sent_via"] = json!("email");
    letter["sent_at_ms"] = json!(now);
    letter["status"] = json!("sent");
    letter["updated_at_ms"] = json!(now);
    conn.execute(
        "UPDATE business_records SET updated_at_ms = ?1
         WHERE collection = ?2 AND record_id = ?3",
        rusqlite::params![now, "accounting_dunning_letters", &letter_id],
    )?;
    upsert_business_record_helper(
        &conn,
        "accounting_dunning_letters",
        &letter_id,
        now,
        letter.clone(),
    )?;
    // Bump the parent run's `letters_sent` counter.
    if let Some(run_id) = letter.get("dunning_run_id").and_then(Value::as_str) {
        if let Some(mut run) = load_business_record(&conn, "accounting_dunning_runs", run_id)? {
            let sent = run.get("letters_sent").and_then(Value::as_i64).unwrap_or(0);
            run["letters_sent"] = json!(sent + 1);
            run["state"] = json!("executed");
            run["updated_at_ms"] = json!(now);
            upsert_business_record_helper(&conn, "accounting_dunning_runs", run_id, now, run)?;
        }
    }
    Ok(json!({
        "ok": true,
        "letter": letter,
    }))
}

fn invoices_list_due_invoices(
    conn: &Connection,
    filter: &Value,
    now_ms_value: i64,
) -> anyhow::Result<Vec<String>> {
    let min_open_cents = filter
        .get("min_open_cents")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let target_state = filter
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("overdue");
    let mut stmt = conn.prepare(
        "SELECT record_id, payload_json
         FROM business_records
         WHERE collection = ?1
           AND deleted = 0
           AND payload_json LIKE ?2",
    )?;
    let pattern = format!("%\"state\":\"{target_state}\"%");
    let rows = stmt.query_map(
        rusqlite::params!["accounting_invoices", pattern.as_str()],
        |row| {
            let record_id: String = row.get(0)?;
            let payload_json: String = row.get(1)?;
            Ok((record_id, payload_json))
        },
    )?;
    let mut result: Vec<String> = Vec::new();
    for row in rows {
        let (record_id, payload_json) = row?;
        let payload: Value = serde_json::from_str(&payload_json)?;
        let open_cents = payload
            .get("open_cents")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let due_date_ms = payload
            .get("due_date_ms")
            .and_then(Value::as_i64)
            .unwrap_or(now_ms_value);
        if open_cents > min_open_cents && due_date_ms < now_ms_value {
            result.push(record_id);
        }
    }
    Ok(result)
}

fn invoices_recurring_create_stub(_command: &BusinessCommand) -> anyhow::Result<Value> {
    anyhow::bail!(
        "invoices.recurring.create is not yet implemented (planned for Phase 7+); \
         recurring templates live in the invoice draft and are instantiated by an \
         external scheduler that posts invoices via the normal create+post path"
    )
}

fn invoices_recurring_update_stub(_command: &BusinessCommand) -> anyhow::Result<Value> {
    anyhow::bail!("invoices.recurring.update is not yet implemented (planned for Phase 7+)")
}

fn invoices_recurring_run_stub(_command: &BusinessCommand) -> anyhow::Result<Value> {
    anyhow::bail!(
        "invoices.recurring.run is not yet implemented (planned for Phase 7+); \
         the external scheduler drives periodic invoice generation"
    )
}

fn invoices_recurring_pause_stub(_command: &BusinessCommand) -> anyhow::Result<Value> {
    anyhow::bail!("invoices.recurring.pause is not yet implemented (planned for Phase 7+)")
}

fn invoices_import_from_outbound_stub(_command: &BusinessCommand) -> anyhow::Result<Value> {
    anyhow::bail!(
        "invoices.import.from_outbound is not yet implemented (planned for Phase 7+); \
         conversion from outbound_campaigns to an invoice draft goes through the \
         normal customers.invoice.create_from_opportunity path"
    )
}

fn invoices_proposal_create_stub(_command: &BusinessCommand) -> anyhow::Result<Value> {
    anyhow::bail!(
        "invoices.proposal.create is not yet implemented (planned for Phase 7+); \
         AI proposals currently surface in the inspector without a persisted command"
    )
}

fn invoices_proposal_approve_stub(_command: &BusinessCommand) -> anyhow::Result<Value> {
    anyhow::bail!("invoices.proposal.approve is not yet implemented (planned for Phase 7+)")
}

fn invoices_proposal_reject_stub(_command: &BusinessCommand) -> anyhow::Result<Value> {
    anyhow::bail!("invoices.proposal.reject is not yet implemented (planned for Phase 7+)")
}

/// Open the business_os SQLite store. The real implementations in
/// `store.rs` keep this connection. We re-export the symbol here so the
/// invoices module never opens a separate database. Phase 5+ will use this in
/// every handler that writes to `accounting_*` or invoice-owned collections.
pub fn open_business_os_store(root: &Path) -> anyhow::Result<Connection> {
    crate::business_os::store::open_store(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_command(module: &str, command_type: &str, payload: Value) -> BusinessCommand {
        BusinessCommand {
            origin: crate::business_os::store::CommandOrigin::TrustedLocal,
            id: Some("cmd_test_1".to_string()),
            module: module.to_string(),
            command_type: command_type.to_string(),
            record_id: None,
            payload,
            client_context: json!({}),
        }
    }

    fn make_session() -> BusinessOsSession {
        BusinessOsSession {
            ok: true,
            authenticated: true,
            auth_required: true,
            user: None,
            login_url: None,
            reason: None,
        }
    }

    #[test]
    fn allowlist_matches_all_documented_invoices_commands() {
        let allowed = [
            "invoices.invoice.create",
            "invoices.invoice.update",
            "invoices.invoice.delete",
            "invoices.invoice.post",
            "invoices.invoice.cancel",
            "invoices.invoice.create_credit_note",
            "invoices.invoice.assign_payment_terms",
            "invoices.line.create",
            "invoices.line.update",
            "invoices.line.delete",
            "invoices.payment.allocate",
            "invoices.payment.unallocate",
            "invoices.payment.match_suggestions",
            "invoices.dunning.run",
            "invoices.dunning.letter.send",
            "invoices.recurring.create",
            "invoices.recurring.update",
            "invoices.recurring.run",
            "invoices.recurring.pause",
            "invoices.import.from_outbound",
            "invoices.proposal.create",
            "invoices.proposal.approve",
            "invoices.proposal.reject",
        ];
        for command_type in allowed {
            assert!(
                is_invoices_active_command(command_type),
                "expected allowlist to include {command_type}"
            );
        }
        assert!(!is_invoices_active_command("customers.account.create"));
        assert!(!is_invoices_active_command("outbound.message.send"));
        assert!(!is_invoices_active_command("invoices.invoice.rogue"));
    }

    #[test]
    fn handle_rejects_wrong_module() {
        let cmd = make_command("customers", "customers.account.create", json!({}));
        let result =
            handle_invoices_active_command(std::path::Path::new("/tmp"), &make_session(), &cmd);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invoices handler requires module=invoices"));
    }

    #[test]
    fn handle_rejects_unknown_command_type() {
        let cmd = make_command("invoices", "invoices.invoice.rogue", json!({}));
        let result =
            handle_invoices_active_command(std::path::Path::new("/tmp"), &make_session(), &cmd);
        assert!(result.is_err());
    }

    #[test]
    fn post_writes_journal_entry_and_marks_invoice_posted() {
        let root = tempfile::tempdir().expect("tempdir");
        // Create a draft first.
        let create = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_post_create",
                "command_id": "cmd_post_create",
                "module": "invoices",
                "command_type": "invoices.invoice.create",
                "record_id": "inv_post",
                "status": "pending_sync",
                "payload": {
                    "invoice_id": "inv_post",
                    "invoice_type": "sale_out",
                    "party_id": "cust_p",
                    "invoice_date_ms": 1_700_000_000_000i64,
                    "currency": "EUR",
                    "lines": [
                        {
                            "id": "l1",
                            "position": 1,
                            "description": "Beratung",
                            "quantity": 1000,
                            "unit": "h",
                            "unit_price_cents": 12000,
                            "tax_rate": 0.19,
                            "account_code": "8400"
                        }
                    ]
                },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("create ok");
        assert_eq!(create["result"]["invoice"]["state"], json!("draft"));

        // Post the draft.
        let post = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_post",
                "command_id": "cmd_post",
                "module": "invoices",
                "command_type": "invoices.invoice.post",
                "record_id": "inv_post",
                "status": "pending_sync",
                "payload": { "invoice_id": "inv_post" },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("post ok");
        let invoice = &post["result"]["invoice"];
        let journal = &post["result"]["journal_entry"];

        assert_eq!(invoice["state"], json!("posted"));
        assert!(
            invoice["invoice_number"]
                .as_str()
                .unwrap()
                .starts_with("RE-"),
            "invoice_number must be RE-prefixed for sale_out, got {:?}",
            invoice["invoice_number"]
        );
        assert_eq!(journal["type"], json!("invoice"));
        assert_eq!(journal["ref_id"], json!("inv_post"));
        assert_eq!(journal["balanced"], json!(true));
        assert_eq!(journal["total_debit_cents"], journal["total_credit_cents"]);
        assert!(journal["posted_at"].as_i64().unwrap_or(0) > 0);

        // Second post returns the idempotent outcome.
        let post2 = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_post_2",
                "command_id": "cmd_post_2",
                "module": "invoices",
                "command_type": "invoices.invoice.post",
                "record_id": "inv_post",
                "status": "pending_sync",
                "payload": { "invoice_id": "inv_post" },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("post 2 ok");
        assert_eq!(post2["result"]["idempotent"], json!(true));
        assert_eq!(
            post2["result"]["journal_entry"]["ref_id"],
            json!("inv_post")
        );

        // Update on a posted invoice is rejected.
        let upd = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_post_update",
                "command_id": "cmd_post_update",
                "module": "invoices",
                "command_type": "invoices.invoice.update",
                "record_id": "inv_post",
                "status": "pending_sync",
                "payload": { "invoice_id": "inv_post", "currency": "USD" },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        );
        assert!(upd.is_err());
        let err = upd.unwrap_err().to_string();
        assert!(err.contains("state=draft"), "err was: {err}");

        // A second post via a different invoice consumes the next number.
        let create_b = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_post_b",
                "command_id": "cmd_post_b",
                "module": "invoices",
                "command_type": "invoices.invoice.create",
                "record_id": "inv_post_b",
                "status": "pending_sync",
                "payload": {
                    "invoice_id": "inv_post_b",
                    "invoice_type": "sale_out",
                    "party_id": "cust_p",
                    "invoice_date_ms": 1_700_000_000_000i64,
                    "currency": "EUR",
                    "lines": [
                        {
                            "id": "l1",
                            "position": 1,
                            "description": "x",
                            "quantity": 1000,
                            "unit": "h",
                            "unit_price_cents": 12000,
                            "tax_rate": 0.19,
                            "account_code": "8400"
                        }
                    ]
                },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("create b ok");
        let post_b = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_post_b_run",
                "command_id": "cmd_post_b_run",
                "module": "invoices",
                "command_type": "invoices.invoice.post",
                "record_id": "inv_post_b",
                "status": "pending_sync",
                "payload": { "invoice_id": "inv_post_b" },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("post b ok");
        let inv_b = &post_b["result"]["invoice"];
        let inv_a = invoice;
        assert_ne!(inv_a["invoice_number"], inv_b["invoice_number"]);
    }

    #[test]
    fn allocate_payment_reduces_open_and_marks_partially_paid_then_paid() {
        let root = tempfile::tempdir().expect("tempdir");
        // Create + post the invoice. quantity=1000 (thousandths) =
        // 1.000 natural unit, unit_price_cents=12_000 = 120.00 EUR. Net
        // therefore 12_000 cent, tax 19% = 2_280 cent, total 14_280 cent
        // = 142.80 EUR.
        crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_alloc_create",
                "command_id": "cmd_alloc_create",
                "module": "invoices",
                "command_type": "invoices.invoice.create",
                "record_id": "inv_alloc",
                "status": "pending_sync",
                "payload": {
                    "invoice_id": "inv_alloc",
                    "invoice_type": "sale_out",
                    "party_id": "cust_a",
                    "invoice_date_ms": 1_700_000_000_000i64,
                    "currency": "EUR",
                    "lines": [
                        {
                            "id": "l1",
                            "position": 1,
                            "description": "x",
                            "quantity": 1000,
                            "unit": "h",
                            "unit_price_cents": 12_000,
                            "tax_rate": 0.19,
                            "account_code": "8400"
                        }
                    ]
                },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("create ok");
        crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_alloc_post",
                "command_id": "cmd_alloc_post",
                "module": "invoices",
                "command_type": "invoices.invoice.post",
                "record_id": "inv_alloc",
                "status": "pending_sync",
                "payload": { "invoice_id": "inv_alloc" },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("post ok");

        // Partial allocation: 5_000 cent.
        let partial = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_alloc_partial",
                "command_id": "cmd_alloc_partial",
                "module": "invoices",
                "command_type": "invoices.payment.allocate",
                "record_id": "inv_alloc",
                "status": "pending_sync",
                "payload": {
                    "invoice_id": "inv_alloc",
                    "payment_id": "pay_1",
                    "amount_cents": 5_000i64
                },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("partial alloc ok");
        assert_eq!(
            partial["result"]["invoice"]["state"],
            json!("partially_paid")
        );
        assert_eq!(partial["result"]["invoice"]["paid_cents"], json!(5_000i64));
        assert_eq!(partial["result"]["invoice"]["open_cents"], json!(9_280i64));

        // Idempotent re-apply via a *different* command_id but the same
        // (payment_id, invoice_id) tuple. The dispatch layer's command_id
        // dedup catches the trivial case (same command_id), so we exercise
        // the handler-level payment-tuple guard here.
        let dup = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_alloc_partial_dup",
                "command_id": "cmd_alloc_partial_dup",
                "module": "invoices",
                "command_type": "invoices.payment.allocate",
                "record_id": "inv_alloc",
                "status": "pending_sync",
                "payload": {
                    "invoice_id": "inv_alloc",
                    "payment_id": "pay_1",
                    "amount_cents": 5_000i64
                },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("dup ok");
        assert_eq!(dup["result"]["idempotent"], json!(true));
        assert_eq!(dup["result"]["invoice"]["paid_cents"], json!(5_000i64));

        // Second allocation: skonto + remaining -> state=paid.
        let settle = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_alloc_settle",
                "command_id": "cmd_alloc_settle",
                "module": "invoices",
                "command_type": "invoices.payment.allocate",
                "record_id": "inv_alloc",
                "status": "pending_sync",
                "payload": {
                    "invoice_id": "inv_alloc",
                    "payment_id": "pay_2",
                    "amount_cents": 9_200i64,
                    "skonto_cents": 80i64
                },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("settle ok");
        assert_eq!(settle["result"]["invoice"]["state"], json!("paid"));
        assert_eq!(settle["result"]["invoice"]["paid_cents"], json!(14_280i64));
        assert_eq!(settle["result"]["invoice"]["open_cents"], json!(0i64));

        // Over-allocation is rejected.
        let over = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_alloc_over",
                "command_id": "cmd_alloc_over",
                "module": "invoices",
                "command_type": "invoices.payment.allocate",
                "record_id": "inv_alloc",
                "status": "pending_sync",
                "payload": {
                    "invoice_id": "inv_alloc",
                    "payment_id": "pay_3",
                    "amount_cents": 100i64
                },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        );
        assert!(over.is_err());
        let err = over.unwrap_err().to_string();
        assert!(
            err.contains("exceeds open invoice balance"),
            "err was: {err}"
        );
    }

    #[test]
    fn dunning_run_creates_letters_for_overdue_invoices() {
        let root = tempfile::tempdir().expect("tempdir");
        // Seed an overdue invoice directly into business_records with
        // state=overdue, open_cents>0 and a past due_date_ms.
        let conn = open_business_os_store(root.path()).expect("store");
        let now = now_ms() as i64;
        let past = now - 30 * 86_400_000;
        let overdue = json!({
            "id": "inv_overdue_1",
            "invoice_number": "RE-2025-0099",
            "invoice_type": "sale_out",
            "party_id": "cust_d",
            "invoice_date_ms": past,
            "due_date_ms": past,
            "currency": "EUR",
            "subtotal_cents": 12_000i64,
            "tax_cents": 2_280i64,
            "total_cents": 14_280i64,
            "paid_cents": 0,
            "open_cents": 14_280i64,
            "state": "overdue",
            "is_deleted": false,
            "updated_at_ms": now,
        });
        let overdue_id = overdue["id"].as_str().unwrap().to_string();
        upsert_business_record_helper(&conn, "accounting_invoices", &overdue_id, now, overdue)
            .expect("seed overdue");

        // Run dunning.
        let run = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_dunning_run",
                "command_id": "cmd_dunning_run",
                "module": "invoices",
                "command_type": "invoices.dunning.run",
                "record_id": "dunning_1",
                "status": "pending_sync",
                "payload": { "run_id": "dunning_1" },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("dunning run ok");
        assert_eq!(run["result"]["ok"], json!(true));
        assert_eq!(
            run["result"]["run"]["state"],
            json!("draft"),
            "run stays draft until first letter is sent"
        );
        let letters = run["result"]["letters"].as_array().expect("letters array");
        assert_eq!(letters.len(), 1);
        assert_eq!(letters[0]["invoice_id"], json!("inv_overdue_1"));
        assert_eq!(letters[0]["level"], json!(1));
        assert_eq!(letters[0]["status"], json!("draft"));

        // Idempotent: re-running with the same run_id returns the same run.
        // The dispatch layer short-circuits on the duplicate command_id and
        // returns the cached outcome from the first run, so the assertion
        // checks the run record is the same and no new letters are issued.
        let dup = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_dunning_run",
                "command_id": "cmd_dunning_run",
                "module": "invoices",
                "command_type": "invoices.dunning.run",
                "record_id": "dunning_1",
                "status": "pending_sync",
                "payload": { "run_id": "dunning_1" },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("dup ok");
        assert_eq!(dup["status"], json!("completed"));
        // The cached outcome carries the run record from the first call.
        assert_eq!(dup["result"]["run"]["id"], json!("dunning_1"));
    }

    #[test]
    fn dunning_letter_send_marks_letter_sent_and_run_executed() {
        let root = tempfile::tempdir().expect("tempdir");
        let now = now_ms() as i64;
        let past = now - 30 * 86_400_000;
        let conn = open_business_os_store(root.path()).expect("store");
        upsert_business_record_helper(
            &conn,
            "accounting_invoices",
            "inv_overdue_2",
            now,
            json!({
                "id": "inv_overdue_2",
                "state": "overdue",
                "open_cents": 5_000i64,
                "due_date_ms": past,
                "is_deleted": false,
                "updated_at_ms": now,
            }),
        )
        .expect("seed");

        crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_dunning_2",
                "command_id": "cmd_dunning_2",
                "module": "invoices",
                "command_type": "invoices.dunning.run",
                "record_id": "dunning_2",
                "status": "pending_sync",
                "payload": { "run_id": "dunning_2" },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("dunning 2 ok");

        // Find the letter id and send it.
        let letter_id = format!("let_dunning_2_inv_overdue_2");
        let send = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_dunning_send",
                "command_id": "cmd_dunning_send",
                "module": "invoices",
                "command_type": "invoices.dunning.letter.send",
                "record_id": letter_id,
                "status": "pending_sync",
                "payload": { "letter_id": letter_id },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("send ok");
        assert_eq!(send["result"]["letter"]["status"], json!("sent"));
        assert!(send["result"]["letter"]["sent_at_ms"].as_i64().unwrap_or(0) > 0);
    }

    #[test]
    fn allocate_on_draft_invoice_is_rejected() {
        let root = tempfile::tempdir().expect("tempdir");
        crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_alloc_draft_create",
                "command_id": "cmd_alloc_draft_create",
                "module": "invoices",
                "command_type": "invoices.invoice.create",
                "record_id": "inv_draft_alloc",
                "status": "pending_sync",
                "payload": {
                    "invoice_id": "inv_draft_alloc",
                    "invoice_type": "sale_out",
                    "party_id": "cust_a",
                    "invoice_date_ms": 1_700_000_000_000i64,
                    "currency": "EUR",
                    "lines": [
                        {
                            "id": "l1",
                            "position": 1,
                            "description": "x",
                            "quantity": 1000,
                            "unit": "h",
                            "unit_price_cents": 12000,
                            "tax_rate": 0.19,
                            "account_code": "8400"
                        }
                    ]
                },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("create ok");
        let result = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_alloc_draft",
                "command_id": "cmd_alloc_draft",
                "module": "invoices",
                "command_type": "invoices.payment.allocate",
                "record_id": "inv_draft_alloc",
                "status": "pending_sync",
                "payload": {
                    "invoice_id": "inv_draft_alloc",
                    "payment_id": "pay_d",
                    "amount_cents": 1_000i64
                },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        );
        assert!(result.is_err());
    }

    #[test]
    fn missing_invoice_id_is_rejected_for_create() {
        let cmd = make_command("invoices", "invoices.invoice.create", json!({}));
        let result =
            handle_invoices_active_command(std::path::Path::new("/tmp"), &make_session(), &cmd);
        assert!(result.is_err());
    }

    #[test]
    fn accept_rxdb_business_command_marks_failed_when_handler_errors() {
        let root = tempfile::tempdir().expect("tempdir");
        let result = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_invoice_create_no_id",
                "command_id": "cmd_invoice_create_no_id",
                "module": "invoices",
                "command_type": "invoices.invoice.create",
                "status": "pending_sync",
                "payload": {},
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        );
        assert!(result.is_err());
        let conn = open_business_os_store(root.path()).expect("open store");
        let status: Option<String> = conn
            .query_row(
                "SELECT status FROM business_commands WHERE command_id = ?1",
                rusqlite::params!["cmd_invoice_create_no_id"],
                |row| row.get(0),
            )
            .expect("query row");
        assert_eq!(status.as_deref(), Some("failed"));
    }

    #[test]
    fn create_then_update_then_delete_draft_lifecycle_persists() {
        let root = tempfile::tempdir().expect("tempdir");
        // Create
        let create = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_inv_create",
                "command_id": "cmd_inv_create",
                "module": "invoices",
                "command_type": "invoices.invoice.create",
                "record_id": "inv_lc",
                "status": "pending_sync",
                "payload": {
                    "invoice_id": "inv_lc",
                    "invoice_type": "sale_out",
                    "party_id": "cust_42",
                    "invoice_date_ms": 1_700_000_000_000i64,
                    "lines": [
                        {
                            "id": "l1",
                            "position": 1,
                            "description": "Stundensatz",
                            "quantity": 1000,
                            "unit": "h",
                            "unit_price_cents": 12000,
                            "tax_rate": 0.19,
                            "account_code": "8400"
                        }
                    ]
                },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("create ok");
        let result = &create["result"];
        assert_eq!(result["ok"], json!(true));
        assert_eq!(result["invoice"]["state"], json!("draft"));
        assert_eq!(result["invoice"]["party_id"], json!("cust_42"));

        // Update
        let update = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_inv_update",
                "command_id": "cmd_inv_update",
                "module": "invoices",
                "command_type": "invoices.invoice.update",
                "record_id": "inv_lc",
                "status": "pending_sync",
                "payload": { "invoice_id": "inv_lc", "currency": "USD" },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("update ok");
        let updated = &update["result"]["invoice"];
        assert_eq!(updated["currency"], json!("USD"));
        assert_eq!(
            updated["party_id"],
            json!("cust_42"),
            "untouched fields stay"
        );
        assert_eq!(updated["state"], json!("draft"), "state must stay draft");

        // Idempotent re-create returns the existing record
        let dup = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_inv_dup",
                "command_id": "cmd_inv_dup",
                "module": "invoices",
                "command_type": "invoices.invoice.create",
                "record_id": "inv_lc",
                "status": "pending_sync",
                "payload": { "invoice_id": "inv_lc" },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("dup ok");
        assert_eq!(dup["result"]["idempotent"], json!(true));
        assert_eq!(dup["result"]["invoice"]["currency"], json!("USD"));

        // Soft delete
        let del = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_inv_delete",
                "command_id": "cmd_inv_delete",
                "module": "invoices",
                "command_type": "invoices.invoice.delete",
                "record_id": "inv_lc",
                "status": "pending_sync",
                "payload": { "invoice_id": "inv_lc" },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("delete ok");
        assert_eq!(del["result"]["invoice"]["is_deleted"], json!(true));
        assert!(
            del["result"]["invoice"]["deleted_at_ms"]
                .as_i64()
                .unwrap_or(0)
                > 0
        );

        // After soft-delete, the load function excludes the row, so a
        // subsequent create with the same id re-inserts as a fresh draft.
        // This is the documented "undo" behaviour: deletion tombstones, but
        // re-creating the same id starts a new draft. We assert the new
        // invoice has is_deleted=false.
        let recreate = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_inv_recreate",
                "command_id": "cmd_inv_recreate",
                "module": "invoices",
                "command_type": "invoices.invoice.create",
                "record_id": "inv_lc",
                "status": "pending_sync",
                "payload": {
                    "invoice_id": "inv_lc",
                    "invoice_type": "sale_out",
                    "party_id": "cust_lc",
                    "invoice_date_ms": 1_700_000_000_000i64,
                    "currency": "EUR",
                    "lines": [
                        {
                            "id": "l1",
                            "position": 1,
                            "description": "Beratung",
                            "quantity": 1000,
                            "unit": "h",
                            "unit_price_cents": 12_000,
                            "tax_rate": 0.19,
                            "account_code": "8400"
                        }
                    ]
                },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
        .expect("recreate ok");
        assert_eq!(recreate["result"]["invoice"]["is_deleted"], json!(false));
        assert_eq!(recreate["result"]["invoice"]["state"], json!("draft"));
        // The new record is a fresh draft, not idempotent against the
        // deleted one.
        assert!(
            recreate["result"].get("idempotent").is_none()
                || recreate["result"]["idempotent"] == json!(false)
        );
    }

    #[test]
    fn accept_rxdb_business_command_rejects_unknown_invoices_command() {
        // The plan forbids generic queue-only handling for invoices.*
        // commands. Unknown invoices.* types must fail hard, not fall
        // through to `record_command`. The dispatch arm added in
        // `store.rs` rejects these with status="failed" and a clear error.
        let root = tempfile::tempdir().expect("tempdir");
        let result = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_invoice_rogue",
                "command_id": "cmd_invoice_rogue",
                "module": "invoices",
                "command_type": "invoices.invoice.rogue",
                "record_id": "inv_rogue",
                "status": "pending_sync",
                "payload": {},
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        );
        assert!(
            result.is_err(),
            "rogue invoices command must fail hard, got: {result:?}"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unsupported invoices command type"),
            "err was: {err}"
        );
        let conn = open_business_os_store(root.path()).expect("open store");
        let status: Option<String> = conn
            .query_row(
                "SELECT status FROM business_commands WHERE command_id = ?1",
                rusqlite::params!["cmd_invoice_rogue"],
                |row| row.get(0),
            )
            .expect("query status");
        assert_eq!(status.as_deref(), Some("failed"));
    }

    // P1#3 — native validation gate mirrors the JS rules in
    // modules/invoices/core/invoice-validate.js. The four tests below cover
    // the P1 finding "Native Validierung ist noch zu schwach" from the v5
    // user review: a draft that the UI rejects must also fail in Rust, and a
    // post that lands with an empty party or zero lines must bail before it
    // touches accounting_journal_entries.

    fn make_valid_invoice_payload(invoice_id: &str) -> Value {
        json!({
            "invoice_id": invoice_id,
            "invoice_type": "sale_out",
            "party_id": "cust_valid",
            "invoice_date_ms": 1_700_000_000_000i64,
            "currency": "EUR",
            "lines": [
                {
                    "id": "l1",
                    "position": 1,
                    "description": "Beratung",
                    "quantity": 1000,
                    "unit": "h",
                    "unit_price_cents": 12_000,
                    "tax_rate": 0.19,
                    "account_code": "8400"
                }
            ]
        })
    }

    fn dispatch_create(
        root: &std::path::Path,
        invoice_id: &str,
        payload: Value,
    ) -> anyhow::Result<Value> {
        crate::business_os::store::accept_rxdb_business_command(
            root,
            json!({
                "id": format!("cmd_{invoice_id}"),
                "command_id": format!("cmd_{invoice_id}"),
                "module": "invoices",
                "command_type": "invoices.invoice.create",
                "record_id": invoice_id,
                "status": "pending_sync",
                "payload": payload,
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
    }

    fn dispatch_post(root: &std::path::Path, invoice_id: &str) -> anyhow::Result<Value> {
        crate::business_os::store::accept_rxdb_business_command(
            root,
            json!({
                "id": format!("cmd_post_{invoice_id}"),
                "command_id": format!("cmd_post_{invoice_id}"),
                "module": "invoices",
                "command_type": "invoices.invoice.post",
                "record_id": invoice_id,
                "status": "pending_sync",
                "payload": { "invoice_id": invoice_id },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        )
    }

    #[test]
    fn validate_invoice_accepts_a_well_formed_draft() {
        let invoice = make_valid_invoice_payload("inv_v_ok");
        validate_invoice_for_command(&invoice, false).expect("draft must validate");
        validate_invoice_for_command(&invoice, true).expect("post must validate");
    }

    #[test]
    fn create_with_empty_party_id_is_rejected() {
        let root = tempfile::tempdir().expect("tempdir");
        let mut payload = make_valid_invoice_payload("inv_no_party");
        payload["party_id"] = json!("");
        let result = dispatch_create(root.path(), "inv_no_party", payload);
        assert!(result.is_err(), "create must reject empty party_id");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("party_id is required"), "err was: {err}");
    }

    #[test]
    fn post_with_zero_lines_is_rejected() {
        let root = tempfile::tempdir().expect("tempdir");
        // Create a draft with one line so the post gate has a record to read.
        let mut payload = make_valid_invoice_payload("inv_zero_lines");
        // First persist a valid draft...
        dispatch_create(root.path(), "inv_zero_lines", payload.clone())
            .expect("create with one line must succeed");
        // ...then strip the lines on a follow-up update and try to post.
        payload["lines"] = json!([]);
        let update = crate::business_os::store::accept_rxdb_business_command(
            root.path(),
            json!({
                "id": "cmd_update_inv_zero_lines",
                "command_id": "cmd_update_inv_zero_lines",
                "module": "invoices",
                "command_type": "invoices.invoice.update",
                "record_id": "inv_zero_lines",
                "status": "pending_sync",
                "payload": { "invoice_id": "inv_zero_lines", "lines": [] },
                "client_context": { "actor": { "id": "tester", "role": "admin" } }
            }),
        );
        // Update is allowed to drop lines (the post gate is what catches it).
        update.expect("update with empty lines is allowed for a draft");
        let post = dispatch_post(root.path(), "inv_zero_lines");
        assert!(post.is_err(), "post with zero lines must fail");
        let err = post.unwrap_err().to_string();
        assert!(err.contains("at least one line item"), "err was: {err}");
    }

    #[test]
    fn post_with_skonto_percent_but_no_skonto_days_is_rejected() {
        let root = tempfile::tempdir().expect("tempdir");
        let mut payload = make_valid_invoice_payload("inv_skonto_bad");
        payload["skonto_percent"] = json!(2.0);
        // skonto_days intentionally missing — the JS validator rejects this
        // at any state (draft or post) and so does the native validator.
        let create = dispatch_create(root.path(), "inv_skonto_bad", payload.clone());
        assert!(
            create.is_err(),
            "create with skonto_percent but no skonto_days must be rejected at any state"
        );
        let err = create.unwrap_err().to_string();
        assert!(
            err.contains("skonto_days must be positive"),
            "err was: {err}"
        );
    }

    #[test]
    fn post_with_paired_skonto_percent_and_days_is_accepted() {
        let root = tempfile::tempdir().expect("tempdir");
        let mut payload = make_valid_invoice_payload("inv_skonto_ok");
        payload["skonto_percent"] = json!(2.0);
        payload["skonto_days"] = json!(7);
        dispatch_create(root.path(), "inv_skonto_ok", payload.clone())
            .expect("create with paired skonto must succeed");
        let post = dispatch_post(root.path(), "inv_skonto_ok");
        assert!(
            post.is_ok(),
            "post with paired skonto must succeed: {post:?}"
        );
    }

    #[test]
    fn post_with_invalid_invoice_type_is_rejected() {
        let root = tempfile::tempdir().expect("tempdir");
        let mut payload = make_valid_invoice_payload("inv_bad_type");
        // Sneak an invalid invoice_type past create (validator catches it).
        payload["invoice_type"] = json!("rogue_type");
        let create = dispatch_create(root.path(), "inv_bad_type", payload);
        assert!(create.is_err(), "create must reject unknown invoice_type");
        let err = create.unwrap_err().to_string();
        assert!(
            err.contains("invoice_type must be one of"),
            "err was: {err}"
        );
    }
}
