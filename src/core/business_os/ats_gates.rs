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
    if field_bool(credential, "verified") == Some(false) {
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
            let days = (until - now_ms) / DAY_MS;
            if days < 0 {
                CredentialStatus::Expired
            } else if days <= EXPIRY_WARN_DAYS {
                CredentialStatus::Expiring
            } else {
                CredentialStatus::Valid
            }
        }
    }
}

fn is_blocking(credential: &Value, now_ms: i64) -> bool {
    if field_bool(credential, "deployment_blocking") != Some(true) {
        return false;
    }
    matches!(
        credential_status(credential, now_ms),
        CredentialStatus::Expired | CredentialStatus::Unverified | CredentialStatus::NotYetValid
    )
}

#[derive(Debug, PartialEq, Eq)]
pub struct DeploymentReadiness {
    pub ready: bool,
    /// (credential_type, reason)
    pub blockers: Vec<(String, String)>,
}

/// Decide whether a subject may be deployed: every required credential type must
/// be present and not blocking. `credentials` is the subject's credential list;
/// each may carry `deployment_blocking: true`.
pub fn evaluate_deployment_readiness(
    credentials: &[Value],
    required_types: &[&str],
    now_ms: i64,
) -> DeploymentReadiness {
    let mut blockers = Vec::new();
    for ty in required_types {
        let present = credentials
            .iter()
            .any(|c| field_str(c, "credential_type") == Some(ty));
        if !present {
            blockers.push((ty.to_string(), "missing".to_string()));
        }
    }
    for credential in credentials {
        if is_blocking(credential, now_ms) {
            let ty = field_str(credential, "credential_type")
                .unwrap_or_default()
                .to_string();
            let reason = format!("{:?}", credential_status(credential, now_ms)).to_lowercase();
            blockers.push((ty, reason));
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
pub fn consent_valid(consent: &Value, now_ms: i64) -> bool {
    let basis = field_str(consent, "legal_basis").unwrap_or("");
    let withdrawable =
        basis.is_empty() || basis == "consent" || basis == "special_category_consent";
    if !withdrawable {
        return matches!(
            basis,
            "contract" | "legal_obligation" | "legitimate_interest"
        );
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
pub fn has_valid_consent(consents: &[Value], purpose: &str, now_ms: i64) -> bool {
    consents
        .iter()
        .any(|c| field_str(c, "purpose") == Some(purpose) && consent_valid(c, now_ms))
}

/// Gate a consent-requiring command: allow when no purpose is required or a valid
/// consent exists; deny otherwise.
pub fn evaluate_consent_gate(purpose: Option<&str>, consents: &[Value], now_ms: i64) -> bool {
    match purpose {
        None => true,
        Some(p) if p.is_empty() => true,
        Some(p) => has_valid_consent(consents, p, now_ms),
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
    }

    #[test]
    fn consent_validity_and_gate() {
        assert!(consent_valid(
            &json!({"legal_basis": "consent", "granted_at_ms": NOW - DAY_MS}),
            NOW
        ));
        assert!(!consent_valid(
            &json!({"legal_basis": "consent", "granted_at_ms": NOW - 10 * DAY_MS, "withdrawn_at_ms": NOW - DAY_MS}),
            NOW
        ));
        assert!(consent_valid(&json!({"legal_basis": "contract"}), NOW));

        let ledger = vec![
            json!({"purpose": "present_to_client", "legal_basis": "consent", "granted_at_ms": NOW - DAY_MS}),
        ];
        assert!(evaluate_consent_gate(
            Some("present_to_client"),
            &ledger,
            NOW
        ));
        assert!(!evaluate_consent_gate(Some("talent_pool"), &ledger, NOW));
        assert!(evaluate_consent_gate(None, &[], NOW));
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
}
