//! Native-owned bounded continuation of a person-research command. No browser
//! checkpoint or adapter-selected path may grant authority to resume work.
use super::*;
use sha2::{Digest, Sha256};

const MAX_POLLS: u64 = 120;
const MAX_WAIT_MS: u64 = 6 * 60 * 60 * 1000;
const MIN_POLL_MS: u64 = 30_000;

// Outer None: not due (or already terminal). Inner None: first execution.
pub(super) fn claim_if_due(
    root: &Path,
    command: &BusinessCommand,
    now: u64,
) -> anyhow::Result<Option<Option<Value>>> {
    let id = command
        .id
        .as_deref()
        .context("research command has no id")?;
    let projection = crate::channels::business_command_projection(root, id)?;
    if projection["execution_phase"] == "terminal" {
        return Ok(None);
    }
    let previous = projection.get("result").cloned().unwrap_or(Value::Null);
    if previous["status"] != "awaiting_provider" {
        return Ok(Some(None));
    }
    let Some(next) = next_checkpoint(&previous, now)? else {
        return Ok(None);
    };
    // Claim the bounded poll BEFORE invoking any external provider. If the
    // process exits, recovery retains its count, delay and operation identity.
    store::write_rxdb_control_command_progress(root, command, "running", next.clone())?;
    Ok(Some(Some(next)))
}

fn next_checkpoint(previous: &Value, now: u64) -> anyhow::Result<Option<Value>> {
    let wait = &previous["provider_wait"];
    anyhow::ensure!(
        wait["schema"] == "ctox.research.provider_wait.v1",
        "invalid native provider wait checkpoint"
    );
    let attempt = wait["poll_attempt"]
        .as_u64()
        .context("missing provider poll count")?;
    let deadline = wait["deadline_at_ms"]
        .as_u64()
        .context("missing provider deadline")?;
    let next_poll = wait["next_poll_at_ms"]
        .as_u64()
        .context("missing provider retry time")?;
    let delay = wait["poll_delay_ms"]
        .as_u64()
        .context("missing provider retry delay")?;
    anyhow::ensure!(
        (MIN_POLL_MS..=300_000).contains(&delay) && attempt > 0,
        "invalid provider poll budget"
    );
    anyhow::ensure!(
        now < deadline,
        "provider_wait_deadline_exceeded: provider did not finish within six hours"
    );
    anyhow::ensure!(
        attempt < MAX_POLLS,
        "provider_wait_poll_budget_exhausted: provider did not finish within 120 attempts"
    );
    if now < next_poll {
        return Ok(None);
    }
    let mut next = previous.clone();
    next["provider_wait"]["poll_attempt"] = Value::from(attempt + 1);
    next["provider_wait"]["next_poll_at_ms"] = Value::from(now.saturating_add(delay));
    Ok(Some(next))
}

pub(super) fn prepare_outcome(
    root: &Path,
    command: &BusinessCommand,
    outcome: &mut Value,
    previous: Option<&Value>,
    now: u64,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        outcome["status"] == "awaiting_provider" && outcome["ok"] == false,
        "provider checkpoint cannot be a completed research result"
    );
    anyhow::ensure!(
        outcome["provider_resume"]["schema"] == "ctox.research.provider_resume.v1",
        "research runner did not preserve completed sources for continuation"
    );
    let id = command
        .id
        .as_deref()
        .context("research command has no id")?;
    let company = command.payload["company"]
        .as_str()
        .context("missing research company")?;
    let country = command.payload["country"]
        .as_str()
        .context("missing research country")?;
    let country = Country::from_iso(country)
        .context("unsupported research country")?
        .as_iso();
    let workspace = root
        .join("runtime/research/person")
        .join(safe_workspace_segment(id));
    let mut receipts = Vec::new();
    let mut delay_ms = MIN_POLL_MS;
    for run in outcome["scrape_runs"]
        .as_array()
        .context("research has no scrape receipts")?
    {
        if run["classification"] != "awaiting_provider" {
            continue;
        }
        let source = run["source_id"]
            .as_str()
            .context("provider receipt has no source")?;
        let target = run["target_key"]
            .as_str()
            .context("provider receipt has no target")?;
        anyhow::ensure!(
            source == "linkedin.com" && target == "linkedin-com",
            "unsupported asynchronous research source"
        );
        let run_id = run["run_id"]
            .as_str()
            .context("provider receipt has no native run id")?;
        let mut digest = Sha256::new();
        digest.update(b"ctox-research-operation-v1");
        for part in [
            workspace.as_os_str().as_encoded_bytes(),
            source.as_bytes(),
            target.as_bytes(),
        ] {
            digest.update((part.len() as u64).to_be_bytes());
            digest.update(part);
        }
        let operation = format!("research-v1-{:x}", digest.finalize());
        let receipt = crate::capabilities::scrape::load_provider_wait_receipt(
            root, run_id, target, &operation, company, country,
        )?;
        delay_ms = delay_ms.max(
            receipt["retry_after_seconds"]
                .as_u64()
                .context("provider has no retry hint")?
                * 1000,
        );
        receipts.push(receipt);
    }
    anyhow::ensure!(
        !receipts.is_empty(),
        "waiting research has no verified native provider jobs"
    );
    let (attempt, deadline) = if let Some(previous) = previous {
        (
            previous["provider_wait"]["poll_attempt"]
                .as_u64()
                .context("missing claimed poll count")?,
            previous["provider_wait"]["deadline_at_ms"]
                .as_u64()
                .context("missing original deadline")?,
        )
    } else {
        (1, now.saturating_add(MAX_WAIT_MS))
    };
    anyhow::ensure!(
        attempt <= MAX_POLLS && now < deadline,
        "provider_wait_budget_exhausted"
    );
    outcome["provider_wait"] = serde_json::json!({
        "schema": "ctox.research.provider_wait.v1", "poll_attempt": attempt,
        "deadline_at_ms": deadline, "next_poll_at_ms": now.saturating_add(delay_ms),
        "poll_delay_ms": delay_ms, "receipts": receipts,
    });
    outcome["summary"] = Value::String("Recherche wartet auf die bereits gestartete Anbieterabfrage; abgeschlossene Quellen bleiben erhalten.".into());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkpoint() -> Value {
        serde_json::json!({
            "status":"awaiting_provider", "fields":{"firma_name":{"value":"ACME"}},
            "provider_wait": {"schema":"ctox.research.provider_wait.v1", "poll_attempt":1,
                "next_poll_at_ms":40_000, "deadline_at_ms":1_000_000, "poll_delay_ms":30_000,
                "receipts":[{"operation_id":"same-operation","snapshot_id":"sd_same"}]}
        })
    }

    #[test]
    fn provider_poll_claim_preserves_evidence_and_operation_across_restart() {
        let original = checkpoint();
        assert!(next_checkpoint(&original, 39_999).unwrap().is_none());
        let claimed = next_checkpoint(&original, 40_000).unwrap().unwrap();
        assert_eq!(claimed["provider_wait"]["poll_attempt"], 2);
        assert_eq!(claimed["provider_wait"]["next_poll_at_ms"], 70_000);
        assert_eq!(
            claimed["provider_wait"]["deadline_at_ms"],
            original["provider_wait"]["deadline_at_ms"]
        );
        assert_eq!(
            claimed["provider_wait"]["receipts"],
            original["provider_wait"]["receipts"]
        );
        assert_eq!(claimed["fields"], original["fields"]);
        let reopened: Value =
            serde_json::from_slice(&serde_json::to_vec(&claimed).unwrap()).unwrap();
        assert!(next_checkpoint(&reopened, 69_999).unwrap().is_none());
        assert_eq!(
            next_checkpoint(&reopened, 70_000).unwrap().unwrap()["provider_wait"]["poll_attempt"],
            3
        );
    }

    #[test]
    fn provider_poll_budget_and_deadline_fail_visibly() {
        let mut previous = checkpoint();
        previous["provider_wait"]["poll_attempt"] = Value::from(MAX_POLLS);
        assert!(next_checkpoint(&previous, 40_000)
            .unwrap_err()
            .to_string()
            .contains("poll_budget_exhausted"));
        assert!(next_checkpoint(&checkpoint(), 1_000_000)
            .unwrap_err()
            .to_string()
            .contains("deadline_exceeded"));
        previous["provider_wait"]["poll_delay_ms"] = Value::from(1);
        assert!(next_checkpoint(&previous, 40_000).is_err());
    }
}
