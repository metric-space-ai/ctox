// Origin: CTOX
// License: AGPL-3.0-only
//
// Native server-authoritative gate primitives for the ATS data plane.
//
// These mirror the tested browser engines (credentials/core/credential.js,
// consent/core/consent.js) so the same allow/deny decisions can be enforced
// server-side. Per the Business OS data boundary, the decisions that gate a
// state transition (deploy a worker, present a candidate, run a
// consent-requiring command, purge an over-retained record) must be
// server-authoritative — a browser helper may mirror them for UX but is never
// the source of truth.
//
// Wiring note: these are the decision primitives. The command handlers that
// call them (the deployment/placement/submission command arms in `store.rs` and
// the retention service loop) are introduced as those flows land; until then
// the gates are exercised by the unit tests below.
#![allow(dead_code)]

use serde_json::Value;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn field_str<'a>(record: &'a Value, key: &str) -> Option<&'a str> {
    record.get(key).and_then(Value::as_str)
}

fn field_i64(record: &Value, key: &str) -> Option<i64> {
    record.get(key).and_then(Value::as_i64)
}

fn field_bool(record: &Value, key: &str) -> Option<bool> {
    record.get(key).and_then(Value::as_bool)
}

// ----------------------------------------------------------------------------
// Credential / deployment gate (VAULT-1 / VAULT-2)
// ----------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub enum CredentialStatus {
    Valid,
    Expiring,
    Expired,
    NotYetValid,
    Unverified,
}

pub const EXPIRY_WARN_DAYS: i64 = 30;

/// Classify a credential's validity at `now_ms`.
pub fn credential_status(credential: &Value, now_ms: i64) -> CredentialStatus {
    // Fail closed: a compliance/legal credential is deployable only when it is
    // explicitly verified. A missing `verified` flag (e.g. an imported or
    // UI-created record) must NOT count as verified.
    if field_bool(credential, "verified") != Some(true) {
        return CredentialStatus::Unverified;
    }
    if let Some(from) = field_i64(credential, "valid_from_ms") {
        if from > 0 && now_ms < from {
            return CredentialStatus::NotYetValid;
        }
    }
    match field_i64(credential, "valid_until_ms") {
        None => CredentialStatus::Valid, // no expiry set
        Some(until) if until == 0 => CredentialStatus::Valid,
        Some(until) => {
            // Compare the instant before the day math: integer division
            // truncates toward zero, so a credential that expired <24h ago would
            // otherwise classify as `Expiring` (days==0) and slip past the gate.
            if until <= now_ms {
                CredentialStatus::Expired
            } else if (until - now_ms) / DAY_MS <= EXPIRY_WARN_DAYS {
                CredentialStatus::Expiring
            } else {
                CredentialStatus::Valid
            }
        }
    }
}

/// A credential is deployable when it is currently usable: Valid or within the
/// expiry-warning window (Expiring). Expired / Unverified / NotYetValid are not.
fn is_deployable(credential: &Value, now_ms: i64) -> bool {
    matches!(
        credential_status(credential, now_ms),
        CredentialStatus::Valid | CredentialStatus::Expiring
    )
}

#[derive(Debug, PartialEq, Eq)]
pub struct DeploymentReadiness {
    pub ready: bool,
    /// (credential_type, reason)
    pub blockers: Vec<(String, String)>,
}

/// Decide whether a subject may be deployed. A type must be satisfied when it is
/// explicitly required OR when the subject carries a `deployment_blocking`
/// credential of that type. A type is satisfied when at least one credential of
/// that type is deployable (Valid/Expiring) — so a newer valid credential
/// covers an older expired one, and a required-but-expired credential is caught
/// even when it is not flagged `deployment_blocking`.
pub fn evaluate_deployment_readiness(
    credentials: &[Value],
    required_types: &[&str],
    now_ms: i64,
) -> DeploymentReadiness {
    let mut types_to_check: Vec<String> =
        required_types.iter().map(|ty| (*ty).to_string()).collect();
    for credential in credentials {
        if field_bool(credential, "deployment_blocking") == Some(true) {
            if let Some(ty) = field_str(credential, "credential_type") {
                if !types_to_check.iter().any(|t| t == ty) {
                    types_to_check.push(ty.to_string());
                }
            }
        }
    }

    let mut blockers = Vec::new();
    for ty in &types_to_check {
        let of_type: Vec<&Value> = credentials
            .iter()
            .filter(|c| field_str(c, "credential_type") == Some(ty.as_str()))
            .collect();
        if of_type.is_empty() {
            // Only required types can be "missing"; a blocking-only type is in
            // the list because a credential of it exists.
            blockers.push((ty.clone(), "missing".to_string()));
            continue;
        }
        if !of_type.iter().any(|c| is_deployable(c, now_ms)) {
            let reason = format!("{:?}", credential_status(of_type[0], now_ms)).to_lowercase();
            blockers.push((ty.clone(), reason));
        }
    }
    DeploymentReadiness {
        ready: blockers.is_empty(),
        blockers,
    }
}

// ----------------------------------------------------------------------------
// Consent / retention gate (CONSENT-1)
// ----------------------------------------------------------------------------

/// A consent row is valid when granted, not withdrawn, and not expired. Bases
/// other than (special-category) consent are not subject to withdrawal.
pub fn consent_valid(consent: &Value, now_ms: i64, require_evidence: bool) -> bool {
    let basis = field_str(consent, "legal_basis").unwrap_or("");
    let withdrawable =
        basis.is_empty() || basis == "consent" || basis == "special_category_consent";
    if !withdrawable {
        if !matches!(
            basis,
            "contract" | "legal_obligation" | "legitimate_interest"
        ) {
            return false;
        }
        // §9.2 DSGVO accountability: a non-consent legal basis is only valid when
        // it carries documented evidence (balancing test / notice / contract
        // reference) in `basis_evidence` — when evidence is required. Otherwise a
        // bare `legitimate_interest` label would auto-pass the gate.
        if require_evidence {
            return field_str(consent, "basis_evidence")
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
        }
        return true;
    }
    if field_i64(consent, "granted_at_ms").unwrap_or(0) <= 0 {
        return false;
    }
    if let Some(withdrawn) = field_i64(consent, "withdrawn_at_ms") {
        if withdrawn > 0 && withdrawn <= now_ms {
            return false;
        }
    }
    if let Some(expires) = field_i64(consent, "expires_at_ms") {
        if expires > 0 && expires <= now_ms {
            return false;
        }
    }
    true
}

/// Is there a valid consent for `purpose` in the subject's ledger?
pub fn has_valid_consent(
    consents: &[Value],
    purpose: &str,
    now_ms: i64,
    require_evidence: bool,
) -> bool {
    consents.iter().any(|c| {
        field_str(c, "purpose") == Some(purpose) && consent_valid(c, now_ms, require_evidence)
    })
}

/// Gate a consent-requiring command: allow when no purpose is required or a valid
/// consent exists; deny otherwise.
pub fn evaluate_consent_gate(
    purpose: Option<&str>,
    consents: &[Value],
    now_ms: i64,
    require_evidence: bool,
) -> bool {
    match purpose {
        None => true,
        Some(p) if p.is_empty() => true,
        Some(p) => has_valid_consent(consents, p, now_ms, require_evidence),
    }
}

/// Retention: a record is due for deletion when its retention window has elapsed
/// since the reference timestamp (Aufbewahrungs-/Löschfrist).
pub fn retention_due(reference_ms: i64, retention_days: i64, now_ms: i64) -> bool {
    if reference_ms <= 0 || retention_days <= 0 {
        return false;
    }
    now_ms >= reference_ms + retention_days * DAY_MS
}

// ----------------------------------------------------------------------------
// Submission guard (SHAREOUT-1) + signature status (ESIGN-1)
// ----------------------------------------------------------------------------

/// Find an existing active submission of the same candidate to the same client
/// within the protection window (ownership conflict / double submission).
/// Returns the conflicting submission id, if any.
pub fn find_double_submission(
    existing: &[Value],
    candidate_id: &str,
    client_account_id: &str,
    within_days: i64,
    now_ms: i64,
) -> Option<String> {
    let window = within_days.max(0) * DAY_MS;
    for submission in existing {
        if field_str(submission, "status") == Some("withdrawn") {
            continue;
        }
        if field_str(submission, "candidate_id") != Some(candidate_id) {
            continue;
        }
        if field_str(submission, "client_account_id") != Some(client_account_id) {
            continue;
        }
        let sent_at = field_i64(submission, "sent_at_ms");
        let within = match sent_at {
            Some(ts) => now_ms - ts <= window,
            None => true,
        };
        if within {
            return field_str(submission, "id")
                .map(str::to_owned)
                .or(Some(String::new()));
        }
    }
    None
}

/// Derive an e-signature request's overall status from its signers + expiry.
pub fn signature_request_status(
    signers: &[Value],
    expires_at_ms: Option<i64>,
    sent_at_ms: Option<i64>,
    now_ms: i64,
) -> &'static str {
    if signers
        .iter()
        .any(|s| field_str(s, "state") == Some("declined"))
    {
        return "declined";
    }
    let all_signed = !signers.is_empty()
        && signers
            .iter()
            .all(|s| field_str(s, "state") == Some("signed"));
    if all_signed {
        return "completed";
    }
    if let Some(expires) = expires_at_ms {
        if expires > 0 && now_ms >= expires {
            return "expired";
        }
    }
    if signers
        .iter()
        .any(|s| field_str(s, "state") == Some("signed"))
    {
        return "partially_signed";
    }
    if sent_at_ms.unwrap_or(0) > 0 {
        return "sent";
    }
    "created"
}

// ----------------------------------------------------------------------------
// Leistungsnachweis billing (DISPATCH-2 / §5.9)
// ----------------------------------------------------------------------------

/// Hour categories carried by a Leistungsnachweis entry. Mirrors
/// `shiftflow/core/leistungsnachweis.js` HOUR_TYPES so the native invoice math
/// and the browser tally agree.
pub const HOUR_TYPES: [&str; 5] = ["regular", "nacht", "sonntag", "feiertag", "mehrarbeit"];

#[derive(Debug, PartialEq)]
pub struct NachweisLine {
    pub category: String,
    pub hours: f64,
    pub rate: f64,
    pub amount: f64,
}

#[derive(Debug, PartialEq)]
pub struct NachweisBilling {
    pub total_hours: f64,
    pub net_total: f64,
    pub lines: Vec<NachweisLine>,
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn entry_hours(entry: &Value) -> f64 {
    entry.get("hours").and_then(Value::as_f64).unwrap_or(0.0)
}

fn entry_type(entry: &Value) -> &str {
    match entry.get("type").and_then(Value::as_str) {
        Some(ty) if HOUR_TYPES.contains(&ty) => ty,
        _ => "regular",
    }
}

/// Tally Leistungsnachweis entries into a billable invoice at `charge_rate` (the
/// Verrechnungssatz billed to the Entleiher), applying per-category surcharge
/// percentages. Mirrors computeNachweisTotals + computeNachweisPay but bills at
/// the charge rate rather than the worker's base pay. One invoice line per
/// non-empty category; lines are rounded and `net_total` is the sum of lines.
pub fn compute_nachweis_billing(
    entries: &[Value],
    charge_rate: f64,
    surcharge_pct: &Value,
) -> NachweisBilling {
    let mut lines = Vec::new();
    let mut net_total = 0.0;
    let mut total_hours = 0.0;
    for ty in HOUR_TYPES {
        let hours: f64 = entries
            .iter()
            .filter(|e| entry_type(e) == ty)
            .map(entry_hours)
            .sum();
        if hours == 0.0 {
            continue;
        }
        total_hours += hours;
        let pct = if ty == "regular" {
            0.0
        } else {
            surcharge_pct.get(ty).and_then(Value::as_f64).unwrap_or(0.0)
        };
        let rate = round2(charge_rate * (1.0 + pct / 100.0));
        let amount = round2(hours * rate);
        net_total += amount;
        lines.push(NachweisLine {
            category: ty.to_string(),
            hours,
            rate,
            amount,
        });
    }
    NachweisBilling {
        total_hours,
        net_total: round2(net_total),
        lines,
    }
}

/// Billing gate: a Leistungsnachweis may be invoiced only once the Entleiher has
/// signed it and it carries time entries. Mirrors evaluateBillingGate. Returns
/// the blocking reasons; empty means billing may be released.
pub fn evaluate_billing_gate(nachweis: &Value) -> Vec<&'static str> {
    let mut blockers = Vec::new();
    let signed = field_bool(nachweis, "entleiher_signed") == Some(true)
        && field_i64(nachweis, "signed_at_ms").unwrap_or(0) > 0;
    if !signed {
        blockers.push("entleiher_signature_missing");
    }
    match nachweis
        .get("entries")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
    {
        None | Some([]) => blockers.push("no_time_entries"),
        Some(entries) => {
            // A non-empty array is not enough. Any single non-finite or negative
            // entry would become a negative invoice line (which the poster
            // rejects), and a zero total bills nothing — so reject both.
            let hours: Vec<f64> = entries.iter().map(entry_hours).collect();
            if hours.iter().any(|h| !h.is_finite() || *h < 0.0) {
                blockers.push("invalid_hours");
            } else if hours.iter().sum::<f64>() <= 0.0 {
                blockers.push("no_billable_hours");
            }
        }
    }
    blockers
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_750_000_000_000;

    #[test]
    fn credential_status_classifies_window() {
        assert_eq!(
            credential_status(
                &json!({"verified": true, "valid_until_ms": NOW + 200 * DAY_MS}),
                NOW
            ),
            CredentialStatus::Valid
        );
        assert_eq!(
            credential_status(
                &json!({"verified": true, "valid_until_ms": NOW + 10 * DAY_MS}),
                NOW
            ),
            CredentialStatus::Expiring
        );
        assert_eq!(
            credential_status(
                &json!({"verified": true, "valid_until_ms": NOW - DAY_MS}),
                NOW
            ),
            CredentialStatus::Expired
        );
        // Regression: expired <24h ago must be Expired, not Expiring (int trunc).
        assert_eq!(
            credential_status(
                &json!({"verified": true, "valid_until_ms": NOW - DAY_MS / 2}),
                NOW
            ),
            CredentialStatus::Expired
        );
        assert_eq!(
            credential_status(&json!({"verified": false}), NOW),
            CredentialStatus::Unverified
        );
    }

    #[test]
    fn deployment_gate_flags_missing_and_expired() {
        let creds = vec![
            json!({"credential_type": "staplerschein", "deployment_blocking": true, "verified": true, "valid_until_ms": NOW + 100 * DAY_MS}),
            json!({"credential_type": "aufenthaltstitel", "deployment_blocking": true, "verified": true, "valid_until_ms": NOW - DAY_MS}),
        ];
        let r = evaluate_deployment_readiness(
            &creds,
            &["staplerschein", "fuehrerschein", "aufenthaltstitel"],
            NOW,
        );
        assert!(!r.ready);
        assert!(r
            .blockers
            .iter()
            .any(|(t, why)| t == "fuehrerschein" && why == "missing"));
        assert!(r
            .blockers
            .iter()
            .any(|(t, why)| t == "aufenthaltstitel" && why == "expired"));

        let ok = evaluate_deployment_readiness(
            &[
                json!({"credential_type": "staplerschein", "deployment_blocking": true, "verified": true, "valid_until_ms": NOW + 100 * DAY_MS}),
            ],
            &["staplerschein"],
            NOW,
        );
        assert!(ok.ready);

        // Regression (#5): a newer valid credential covers an older expired one
        // of the same type — the candidate is deployable.
        let dup = evaluate_deployment_readiness(
            &[
                json!({"credential_type": "staplerschein", "deployment_blocking": true, "verified": true, "valid_until_ms": NOW - DAY_MS}),
                json!({"credential_type": "staplerschein", "deployment_blocking": true, "verified": true, "valid_until_ms": NOW + 100 * DAY_MS}),
            ],
            &["staplerschein"],
            NOW,
        );
        assert!(dup.ready, "a valid credential should cover an expired duplicate");

        // Regression (#5): a required type that is present but expired and NOT
        // flagged deployment_blocking must still be caught.
        let stale = evaluate_deployment_readiness(
            &[json!({"credential_type": "g25", "verified": true, "valid_until_ms": NOW - DAY_MS})],
            &["g25"],
            NOW,
        );
        assert!(!stale.ready);
        assert!(stale.blockers.iter().any(|(t, why)| t == "g25" && why == "expired"));
    }

    #[test]
    fn consent_validity_and_gate() {
        assert!(consent_valid(
            &json!({"legal_basis": "consent", "granted_at_ms": NOW - DAY_MS}),
            NOW,
            false
        ));
        assert!(!consent_valid(
            &json!({"legal_basis": "consent", "granted_at_ms": NOW - 10 * DAY_MS, "withdrawn_at_ms": NOW - DAY_MS}),
            NOW,
            false
        ));
        assert!(consent_valid(&json!({"legal_basis": "contract"}), NOW, false));

        // §9.2: with evidence required, a bare non-consent basis is rejected, but
        // one carrying basis_evidence passes.
        assert!(!consent_valid(&json!({"legal_basis": "legitimate_interest"}), NOW, true));
        assert!(consent_valid(
            &json!({"legal_basis": "legitimate_interest", "basis_evidence": "LIA documented 2026-06"}),
            NOW,
            true
        ));
        // consent basis is unaffected by the evidence flag.
        assert!(consent_valid(
            &json!({"legal_basis": "consent", "granted_at_ms": NOW - DAY_MS}),
            NOW,
            true
        ));

        let ledger = vec![
            json!({"purpose": "present_to_client", "legal_basis": "consent", "granted_at_ms": NOW - DAY_MS}),
        ];
        assert!(evaluate_consent_gate(
            Some("present_to_client"),
            &ledger,
            NOW,
            false
        ));
        assert!(!evaluate_consent_gate(Some("talent_pool"), &ledger, NOW, false));
        assert!(evaluate_consent_gate(None, &[], NOW, false));
    }

    #[test]
    fn retention_due_after_window() {
        assert!(retention_due(NOW - 400 * DAY_MS, 365, NOW));
        assert!(!retention_due(NOW - 10 * DAY_MS, 365, NOW));
        assert!(!retention_due(0, 365, NOW));
    }

    #[test]
    fn double_submission_detected_in_window() {
        let existing = vec![
            json!({"id": "s1", "candidate_id": "c1", "client_account_id": "a1", "sent_at_ms": NOW - 10 * DAY_MS, "status": "sent"}),
            json!({"id": "s2", "candidate_id": "c1", "client_account_id": "a2", "sent_at_ms": NOW - 10 * DAY_MS, "status": "withdrawn"}),
        ];
        assert_eq!(
            find_double_submission(&existing, "c1", "a1", 180, NOW).as_deref(),
            Some("s1")
        );
        assert_eq!(
            find_double_submission(&existing, "c1", "a3", 180, NOW),
            None
        );
        assert_eq!(
            find_double_submission(&existing, "c1", "a2", 180, NOW),
            None
        ); // withdrawn
        assert_eq!(find_double_submission(&existing, "c1", "a1", 5, NOW), None);
        // outside window
    }

    #[test]
    fn signature_status_derives() {
        let pending = vec![json!({"state": "signed"}), json!({"state": "pending"})];
        assert_eq!(
            signature_request_status(&pending, None, Some(NOW - DAY_MS), NOW),
            "partially_signed"
        );
        let all = vec![json!({"state": "signed"}), json!({"state": "signed"})];
        assert_eq!(
            signature_request_status(&all, None, Some(NOW), NOW),
            "completed"
        );
        let declined = vec![json!({"state": "declined"})];
        assert_eq!(
            signature_request_status(&declined, None, None, NOW),
            "declined"
        );
        let expired = vec![json!({"state": "pending"})];
        assert_eq!(
            signature_request_status(&expired, Some(NOW - DAY_MS), Some(NOW - 2 * DAY_MS), NOW),
            "expired"
        );
        assert_eq!(
            signature_request_status(&[json!({"state": "pending"})], None, None, NOW),
            "created"
        );
    }

    #[test]
    fn nachweis_billing_applies_surcharges_per_category() {
        let entries = vec![
            json!({"type": "regular", "hours": 40.0}),
            json!({"type": "nacht", "hours": 8.0}),
            json!({"type": "sonntag", "hours": 4.0}),
            json!({"type": "unknown", "hours": 2.0}), // falls back to regular
        ];
        let surcharge = json!({"nacht": 25.0, "sonntag": 50.0, "feiertag": 100.0});
        let billing = compute_nachweis_billing(&entries, 30.0, &surcharge);
        // regular: (40 + 2) * 30 = 1260; nacht: 8 * 37.5 = 300; sonntag: 4 * 45 = 180
        assert_eq!(billing.total_hours, 54.0);
        assert_eq!(billing.net_total, 1740.0);
        let nacht = billing
            .lines
            .iter()
            .find(|l| l.category == "nacht")
            .unwrap();
        assert_eq!(nacht.rate, 37.5);
        assert_eq!(nacht.amount, 300.0);
        assert!(!billing.lines.iter().any(|l| l.category == "feiertag")); // zero hours omitted
    }

    #[test]
    fn billing_gate_requires_signature_and_entries() {
        assert!(evaluate_billing_gate(&json!({
            "entleiher_signed": true,
            "signed_at_ms": NOW,
            "entries": [{"type": "regular", "hours": 8.0}]
        }))
        .is_empty());
        assert!(evaluate_billing_gate(&json!({"entries": [{"hours": 8.0}]}))
            .contains(&"entleiher_signature_missing"));
        assert!(evaluate_billing_gate(
            &json!({"entleiher_signed": true, "signed_at_ms": NOW, "entries": []})
        )
        .contains(&"no_time_entries"));
        // Regression (#8): a non-empty array with a negative hour entry is
        // invalid (would become a negative invoice line).
        assert!(evaluate_billing_gate(&json!({
            "entleiher_signed": true,
            "signed_at_ms": NOW,
            "entries": [{"type": "regular", "hours": 0.0}, {"type": "nacht", "hours": -4.0}]
        }))
        .contains(&"invalid_hours"));
        // All-zero hours → nothing to bill.
        assert!(evaluate_billing_gate(&json!({
            "entleiher_signed": true,
            "signed_at_ms": NOW,
            "entries": [{"type": "regular", "hours": 0.0}]
        }))
        .contains(&"no_billable_hours"));
    }
}
