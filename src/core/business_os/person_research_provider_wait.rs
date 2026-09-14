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
    use serde_json::json;

    const RECOVERY_ID: &str = "provider-recovery-process-fixture";

    fn canonical(root: &Path) -> Value {
        crate::channels::business_command_projection(root, RECOVERY_ID).unwrap()
    }

    fn child_phase(root: &Path, phase: &str) {
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};
        let name = format!(
            "{}::provider_recovery_process_fixture",
            module_path!().split_once("::").unwrap().1
        );
        let log_path = root.join(format!("{phase}.log"));
        let log = std::fs::File::create(&log_path).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &name,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("CTOX_PROVIDER_RECOVERY_TEST_ROOT", root)
            .env("CTOX_PROVIDER_RECOVERY_TEST_PHASE", phase)
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(40);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "fixture {phase} failed: {}",
                    std::fs::read_to_string(&log_path).unwrap_or_default()
                );
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("fixture {phase} exceeded its 40-second bound");
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    #[test]
    fn provider_command_recovers_across_process_restart_without_duplicate_sources(
    ) -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        std::fs::write(root.join("provider-recovery-owned"), b"isolated test root")?;
        child_phase(root, "first");
        let first = canonical(root);
        assert_eq!(first["execution_phase"], "running");
        assert_eq!(first["terminal_status"], "none");
        assert_eq!(first["result"]["status"], "awaiting_provider");
        let xing = first["result"]["scrape_runs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|run| run["source_id"] == "xing.com")
            .unwrap()
            .clone();
        assert_eq!(xing["classification"], "succeeded");
        let receipts = first["result"]["provider_wait"]["receipts"].clone();
        let paths: Value =
            serde_json::from_slice(&std::fs::read(root.join("fixture-counters.json"))?)?;
        let counter = |source: &str| -> Value {
            let path = Path::new(paths[source].as_str().unwrap());
            assert!(path.starts_with(root));
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
        };
        assert_eq!(counter("xing")["calls"], 1);
        assert_eq!(counter("linkedin")["submissions"], 1);
        let expected_root = std::fs::canonicalize(root)?.to_string_lossy().into_owned();
        assert_eq!(counter("xing")["instance_roots"], json!([expected_root]));
        assert_eq!(
            counter("linkedin")["instance_roots"],
            json!([expected_root])
        );
        child_phase(root, "early");
        assert_eq!(canonical(root)["result"], first["result"]);
        assert_eq!(counter("xing")["calls"], 1);
        assert_eq!(counter("linkedin")["calls"], 1);

        // Move only the persisted wake time into the past instead of sleeping
        // 30s. Operation, receipts, poll count and original deadline stay real.
        let conn = rusqlite::Connection::open(crate::paths::core_db(root))?;
        assert_eq!(
            conn.execute(
                "UPDATE business_command_aggregates
            SET result_json=json_set(result_json,'$.provider_wait.next_poll_at_ms',0)
            WHERE command_id=?1 AND execution_phase='running'",
                [RECOVERY_ID]
            )?,
            1
        );
        drop(conn);
        child_phase(root, "resume");
        let terminal = canonical(root);
        assert_eq!(terminal["terminal_status"], "completed");
        assert_eq!(terminal["attempt"], 1);
        assert_eq!(terminal["result"]["ok"], true);
        assert!(terminal["result"]["scrape_runs"]
            .as_array()
            .unwrap()
            .contains(&xing));
        let linkedin = counter("linkedin");
        assert_eq!(linkedin["calls"], 2);
        assert_eq!(linkedin["submissions"], 1);
        assert_eq!(linkedin["operations"][0], receipts[0]["operation_id"]);
        assert_eq!(linkedin["operations"][1], receipts[0]["operation_id"]);
        assert_eq!(
            linkedin["instance_roots"],
            json!([expected_root, expected_root])
        );
        assert_eq!(
            counter("xing")["calls"],
            1,
            "completed native adapter reran after restart"
        );
        child_phase(root, "terminal");
        assert_eq!(canonical(root)["result"], terminal["result"]);
        assert_eq!(counter("linkedin")["calls"], 2);
        assert_eq!(counter("xing")["calls"], 1);
        Ok(())
    }

    #[test]
    #[ignore = "bounded subprocess fixture; invoked by provider_command_recovers_across_process_restart_without_duplicate_sources"]
    fn provider_recovery_process_fixture() -> anyhow::Result<()> {
        let root = PathBuf::from(
            std::env::var_os("CTOX_PROVIDER_RECOVERY_TEST_ROOT")
                .context("fixture root required")?,
        );
        anyhow::ensure!(
            std::fs::read(root.join("provider-recovery-owned"))? == b"isolated test root",
            "not a fixture root"
        );
        let phase = std::env::var("CTOX_PROVIDER_RECOVERY_TEST_PHASE")?;
        if phase == "first" {
            store::tests::seed_business_user(&root, "researcher", "chef")?;
            // This isolated fixture has no native/browser peer to create its
            // collection; optional projection writers otherwise skip the row.
            crate::business_os::person_research_gap_closure::seed_rxdb_collection_table_for_tests(
                &root,
                "business_commands",
            )?;
            crate::inference::runtime_env::set_runtime_env_value(
                &root,
                "CTOX_WEB_SEARCH_PROVIDER",
                "mock",
            )?;
            let script = include_str!("fixtures/person-research-provider-recovery.cjs");
            let linkedin = crate::capabilities::scrape::register_provider_recovery_fixture(
                &root,
                "linkedin-com",
                "linkedin.com",
                script,
            )?;
            let xing = crate::capabilities::scrape::register_provider_recovery_fixture(
                &root, "xing-com", "xing.com", script,
            )?;
            std::fs::write(
                root.join("fixture-counters.json"),
                serde_json::to_vec(&serde_json::json!({"linkedin":linkedin,"xing":xing}))?,
            )?;
            let (token, _) =
                store::issue_business_os_capability_token(&root, "researcher", now_ms())?;
            let command = BusinessCommand {
                origin: store::CommandOrigin::TrustedLocal,
                id: Some(RECOVERY_ID.into()),
                module: "research".into(),
                command_type: "web_stack.person_research".into(),
                record_id: Some("fixture-company".into()),
                payload: serde_json::json!({"company":"Fixture GmbH","country":"DE","mode":"new_record",
                "fields":["person_linkedin","person_xing"],"include_private":["linkedin.com","xing.com"],
                "source_policy":{"sources":[
                    {"id":"linkedin.com","url":"https://www.linkedin.com/","target_key":"linkedin-com"},
                    {"id":"xing.com","url":"https://www.xing.com/","target_key":"xing-com"}
                ]}}),
                client_context: serde_json::json!({"actor":{"id":"researcher"},"capability_token":token}),
            };
            crate::channels::claim_business_control_command(
                &root,
                store::business_command_core_claim(RECOVERY_ID, &command)?,
            )?;
            let conn = store::open_store(&root)?;
            conn.execute("INSERT INTO business_commands
                (command_id,module,command_type,record_id,status,payload_json,client_context_json,observed_at_ms)
                VALUES (?1,'research','web_stack.person_research','fixture-company','accepted',?2,?3,1)",
                rusqlite::params![RECOVERY_ID,serde_json::to_string(&command.payload)?,serde_json::to_string(&command.client_context)?])?;
        }
        let started = super::super::recover_once(&root)?;
        if matches!(phase.as_str(), "early" | "terminal") {
            assert_eq!(started, 0);
            return Ok(());
        }
        anyhow::ensure!(
            matches!(phase.as_str(), "first" | "resume"),
            "unknown fixture phase"
        );
        assert_eq!(started, 1);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut terminal_state = Value::Null;
        loop {
            let current = canonical(&root);
            assert_ne!(
                current["terminal_status"],
                "failed",
                "native recovery failed: {:?}",
                current.pointer("/result/error_code")
            );
            let done = if phase == "first" {
                current["result"]["status"] == "awaiting_provider"
            } else if current["terminal_status"] == "completed" {
                // Core completion precedes the final lifecycle mirrors. Let
                // this successful phase finish both writes before exiting its
                // process; the later terminal phase must remain a true no-op.
                let conn = store::open_store(&root)?;
                let stored = store::stored_rxdb_business_command_outcome(&conn, RECOVERY_ID)?;
                let replicated =
                    store::load_rxdb_collection_record(&root, "business_commands", RECOVERY_ID)?;
                let state = |projection: &Option<Value>| {
                    json!({
                        "present": projection.is_some(),
                        "terminal_status": projection.as_ref()
                            .and_then(|value| value["terminal_status"].as_str()),
                        "execution_phase": projection.as_ref()
                            .and_then(|value| value["execution_phase"].as_str()),
                        "attempt": projection.as_ref()
                            .and_then(|value| value["attempt"].as_u64()),
                        "result_matches": projection.as_ref()
                            .is_some_and(|value| value["result"] == current["result"]),
                    })
                };
                terminal_state = json!({"local": state(&stored), "rxdb": state(&replicated)});
                [stored, replicated].iter().all(|projection| {
                    projection.as_ref().is_some_and(|projection| {
                        projection["terminal_status"] == current["terminal_status"]
                            && projection["execution_phase"] == current["execution_phase"]
                            && projection["attempt"] == current["attempt"]
                            && projection["result"] == current["result"]
                    })
                })
            } else {
                false
            };
            if done {
                return Ok(());
            }
            anyhow::ensure!(
                std::time::Instant::now() < deadline,
                "native recovery worker exceeded fixture deadline; terminal projections: {terminal_state}"
            );
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }

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
