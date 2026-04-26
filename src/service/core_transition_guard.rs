// Origin: CTOX
// License: Apache-2.0

use crate::service::core_state_machine as csm;
use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreTransitionProof {
    pub proof_id: String,
    pub accepted: bool,
    pub report: csm::CoreTransitionReport,
}

pub fn ensure_core_transition_guard_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS ctox_core_transition_proofs (
            proof_id TEXT PRIMARY KEY,
            entity_type TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            lane TEXT NOT NULL,
            from_state TEXT NOT NULL,
            to_state TEXT NOT NULL,
            core_event TEXT NOT NULL,
            actor TEXT NOT NULL,
            accepted INTEGER NOT NULL,
            violation_codes_json TEXT NOT NULL DEFAULT '[]',
            request_json TEXT NOT NULL,
            report_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ctox_core_transition_proofs_entity
          ON ctox_core_transition_proofs(entity_type, entity_id, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_ctox_core_transition_proofs_accepted
          ON ctox_core_transition_proofs(accepted, updated_at DESC);
        "#,
    )?;
    Ok(())
}

pub fn evaluate_core_transition(
    conn: &Connection,
    request: &csm::CoreTransitionRequest,
) -> Result<CoreTransitionProof> {
    ensure_core_transition_guard_schema(conn)?;

    let report = csm::validate_transition(request);
    let request_json = serde_json::to_string(request)?;
    let report_json = serde_json::to_string(&report)?;
    let violation_codes = report
        .violations
        .iter()
        .map(|violation| violation.code.clone())
        .collect::<Vec<_>>();
    let violation_codes_json = serde_json::to_string(&violation_codes)?;
    let proof_id = deterministic_proof_id(&request_json);
    let now = Utc::now().to_rfc3339();

    conn.execute(
        r#"
        INSERT INTO ctox_core_transition_proofs (
            proof_id, entity_type, entity_id, lane, from_state, to_state,
            core_event, actor, accepted, violation_codes_json,
            request_json, report_json, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)
        ON CONFLICT(proof_id) DO UPDATE SET
            accepted = excluded.accepted,
            violation_codes_json = excluded.violation_codes_json,
            request_json = excluded.request_json,
            report_json = excluded.report_json,
            updated_at = excluded.updated_at
        "#,
        params![
            &proof_id,
            format!("{:?}", request.entity_type),
            &request.entity_id,
            format!("{:?}", request.lane),
            format!("{:?}", request.from_state),
            format!("{:?}", request.to_state),
            format!("{:?}", request.event),
            &request.actor,
            if report.accepted { 1 } else { 0 },
            violation_codes_json,
            request_json,
            report_json,
            now,
        ],
    )?;

    Ok(CoreTransitionProof {
        proof_id,
        accepted: report.accepted,
        report,
    })
}

pub fn enforce_core_transition(
    conn: &Connection,
    request: &csm::CoreTransitionRequest,
) -> Result<CoreTransitionProof> {
    let proof = evaluate_core_transition(conn, request)?;
    if proof.accepted {
        return Ok(proof);
    }

    anyhow::bail!("{}", agent_recovery_message(&proof.report));
}

fn deterministic_proof_id(request_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"ctox-core-transition-proof-v1");
    hasher.update(request_json.as_bytes());
    format!("ctp-{:x}", hasher.finalize())
}

fn agent_recovery_message(report: &csm::CoreTransitionReport) -> String {
    let mut actions = Vec::new();
    for violation in &report.violations {
        let action = match violation.code.as_str() {
            "invalid_transition" => {
                "Bleib im erlaubten Arbeitsablauf: springe nicht direkt zum Zielzustand. Fuehre die fehlenden Zwischenschritte aus, dokumentiere jeden Schritt dauerhaft und versuche danach erneut fortzufahren."
            }
            "owner_visible_completion_requires_review" => {
                "Schliesse owner- oder founder-sichtbare Arbeit noch nicht ab. Fuehre zuerst ein echtes Review durch, arbeite kritische Review-Punkte inhaltlich nach und speichere die Review-Freigabe dauerhaft."
            }
            "closure_requires_verification" => {
                "Schliesse die Aufgabe noch nicht. Verifiziere das Ergebnis zuerst mit belastbarer Evidenz, speichere diese Evidenz und schliesse erst danach."
            }
            "founder_send_requires_review_audit" => {
                "Sende diese Founder-Kommunikation noch nicht. Baue zuerst den vollstaendigen Kontext auf, lasse den finalen Entwurf durch das Review laufen und speichere die Review-Freigabe dauerhaft."
            }
            "founder_send_body_hash_mismatch" => {
                "Der zu sendende Text entspricht nicht dem freigegebenen Review-Text. Stoppe den Versand, erstelle den finalen Entwurf erneut und lasse genau diese finale Fassung freigeben."
            }
            "founder_send_recipient_hash_mismatch" => {
                "Die Empfaenger oder CC-Liste entsprechen nicht der freigegebenen Fassung. Stoppe den Versand, pruefe To/CC gegen den Mail-Thread-Kontext und lasse die finale Empfaengerliste erneut freigeben."
            }
            "commitment_requires_backing_schedule" => {
                "Lege kein Versprechen ohne Absicherung ab. Erstelle zuerst eine konkrete Termin- oder Queue-Absicherung, damit die Zusage rechtzeitig bearbeitet wird."
            }
            "commitment_delivery_requires_evidence" => {
                "Markiere die Zusage noch nicht als geliefert. Sammle zuerst belastbare Liefer-Evidenz und verknuepfe sie mit der Zusage."
            }
            "repair_requires_canonical_hot_path" => {
                "Fuehre die Reparatur ueber den kanonischen Repair-Pfad aus: Diagnose, Plan, Review, deterministische Massnahmen, Verifikation. Starte nicht mitten im Prozess."
            }
            "active_knowledge_requires_incident" => {
                "Lege Knowledge nicht direkt als aktiv ab. Halte zuerst den beobachteten Vorfall fest, formuliere die Lehre, pruefe sie und aktiviere sie erst nach Evidenz."
            }
            _ => "Die geplante Aktion passt noch nicht zum gesicherten Harness-Zustand. Pruefe den naechsten erlaubten Arbeitsschritt, halte Evidenz fest und versuche erst danach erneut fortzufahren.",
        };
        if !actions.iter().any(|existing| *existing == action) {
            actions.push(action);
        }
    }

    if actions.is_empty() {
        actions.push("Die Aktion wurde vom Harness gestoppt. Pruefe den naechsten erlaubten Arbeitsschritt, halte Evidenz fest und versuche danach erneut fortzufahren.");
    }

    format!(
        "Diese Aktion wurde noch nicht ausgefuehrt, weil der abgesicherte Arbeitsablauf unvollstaendig ist. {}\nWenn du eine kompakte Diagnose brauchst, nutze `ctox process-mining guidance --limit 50` und arbeite die dort genannten naechsten Schritte ab.",
        actions.join(" ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::core_state_machine::{
        CoreEntityType, CoreEvent, CoreEvidenceRefs, CoreState, CoreTransitionRequest, RuntimeLane,
    };
    use std::collections::BTreeMap;

    fn founder_send_request(evidence: CoreEvidenceRefs) -> CoreTransitionRequest {
        let mut metadata = BTreeMap::new();
        metadata.insert("protected_party".to_string(), "founder".to_string());
        metadata.insert("channel".to_string(), "email".to_string());

        CoreTransitionRequest {
            entity_type: CoreEntityType::FounderCommunication,
            entity_id: "thread-founder".to_string(),
            lane: RuntimeLane::P0FounderCommunication,
            from_state: CoreState::Approved,
            to_state: CoreState::Sending,
            event: CoreEvent::Send,
            actor: "CTO1".to_string(),
            evidence,
            metadata,
        }
    }

    #[test]
    fn rejected_transition_is_persisted_as_proof() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let proof =
            evaluate_core_transition(&conn, &founder_send_request(CoreEvidenceRefs::default()))?;

        assert!(!proof.accepted);
        let accepted: i64 = conn.query_row(
            "SELECT accepted FROM ctox_core_transition_proofs WHERE proof_id = ?1",
            params![proof.proof_id],
            |row| row.get(0),
        )?;
        assert_eq!(accepted, 0);
        Ok(())
    }

    #[test]
    fn accepted_transition_is_persisted_as_proof() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let evidence = CoreEvidenceRefs {
            review_audit_key: Some("review-1".to_string()),
            approved_body_sha256: Some("body".to_string()),
            outgoing_body_sha256: Some("body".to_string()),
            approved_recipient_set_sha256: Some("recipients".to_string()),
            outgoing_recipient_set_sha256: Some("recipients".to_string()),
            ..CoreEvidenceRefs::default()
        };
        let proof = evaluate_core_transition(&conn, &founder_send_request(evidence))?;

        assert!(proof.accepted);
        let accepted: i64 = conn.query_row(
            "SELECT accepted FROM ctox_core_transition_proofs WHERE proof_id = ?1",
            params![proof.proof_id],
            |row| row.get(0),
        )?;
        assert_eq!(accepted, 1);
        Ok(())
    }

    #[test]
    fn rejected_transition_error_is_agent_readable_without_internal_ids() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        let err =
            enforce_core_transition(&conn, &founder_send_request(CoreEvidenceRefs::default()))
                .expect_err("founder send without review must be rejected");
        let message = err.to_string();

        assert!(message.contains("Sende diese Founder-Kommunikation noch nicht"));
        assert!(message.contains("ctox process-mining guidance --limit 50"));
        assert!(!message.contains("ctp-"));
        assert!(!message.contains("founder_send_requires_review_audit"));
        assert!(!message.contains("core transition guard rejected"));
        Ok(())
    }
}
