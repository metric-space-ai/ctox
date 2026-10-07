// Origin: CTOX
// License: AGPL-3.0-only
//! Immutable native capture policy; explicit reauthorization keeps its scope.
use super::*;

pub(super) fn revision(snapshot: &serde_json::Value) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(snapshot)?)
    ))
}

pub(super) fn persist(
    policy: &Connection,
    destination: &GuestRestoreDestination,
    capture: &str,
) -> Result<()> {
    let snapshot = policy_snapshot(policy, destination)?;
    let captured: String = policy.query_row(
        "SELECT policy_revision FROM business_native_source_journals WHERE capture_id=?1",
        [capture],
        |row| row.get(0),
    )?;
    ensure!(
        revision(&snapshot)? == captured,
        "source capture policy changed"
    );
    let json = serde_json::to_string(&snapshot)?;
    policy.execute(
        "INSERT INTO business_native_source_policy_snapshots(capture_id,snapshot_json)
         VALUES (?1,?2) ON CONFLICT(capture_id) DO NOTHING",
        rusqlite::params![capture, json],
    )?;
    let exact: bool = policy.query_row(
        "SELECT EXISTS(SELECT 1 FROM business_native_source_policy_snapshots
         WHERE capture_id=?1 AND snapshot_json=?2)",
        rusqlite::params![capture, json],
        |row| row.get(0),
    )?;
    ensure!(exact, "source policy attestation conflicts");
    Ok(())
}

/// A local operator can reauthorize the same capture after an epoch/grant
/// change, only with fresh provider/workspace assignments in the same scope.
/// No role, active state, account/model, project/chat/profile/computer,
/// working-copy identity/path/device/inode or peer change is normalized away.
pub(super) fn validate_reauthorization(
    policy: &Connection,
    capture: &str,
    captured_revision: &str,
    current: &serde_json::Value,
) -> Result<()> {
    let json: String = policy
        .query_row(
            "SELECT snapshot_json FROM business_native_source_policy_snapshots WHERE capture_id=?1",
            [capture],
            |row| row.get(0),
        )
        .optional()?
        .context("capture has no native policy attestation; recapture")?;
    let captured: serde_json::Value = serde_json::from_str(&json)?;
    validate_advance(&captured, captured_revision, current)
}

pub(super) fn validate_advance(
    captured: &serde_json::Value,
    captured_revision: &str,
    current: &serde_json::Value,
) -> Result<()> {
    ensure!(
        revision(captured)? == captured_revision,
        "capture policy attestation changed"
    );
    let normalize = |value: &serde_json::Value| -> Result<serde_json::Value> {
        let mut values = value
            .as_array()
            .context("invalid native policy snapshot")?
            .clone();
        ensure!(values.len() == 8, "invalid native policy snapshot length");
        for (index, epoch, current_epoch, transient) in [
            (
                6,
                "principal_epoch",
                "current_epoch",
                &[
                    "principal_epoch",
                    "current_epoch",
                    "revision",
                    "updated_at_ms",
                ][..],
            ),
            (
                7,
                "principalEpoch",
                "currentEpoch",
                &["principalEpoch", "currentEpoch", "revision"][..],
            ),
        ] {
            let assignment = values[index]
                .as_object_mut()
                .context("capture assignment absent")?;
            let bound = assignment
                .get(epoch)
                .and_then(serde_json::Value::as_u64)
                .context("invalid assignment epoch")?;
            let now = assignment
                .get(current_epoch)
                .and_then(serde_json::Value::as_u64)
                .context("invalid current epoch")?;
            ensure!(
                bound == now,
                "provider/workspace needs explicit current regrant"
            );
            ensure!(
                assignment
                    .get("revision")
                    .and_then(serde_json::Value::as_u64)
                    .is_some_and(|r| r > 0),
                "invalid assignment revision"
            );
            for key in transient {
                assignment.remove(*key);
            }
        }
        Ok(serde_json::Value::Array(values))
    };
    ensure!(
        normalize(captured)? == normalize(current)?,
        "native capture scope changed; recapture"
    );
    for (index, epoch) in [(6, "current_epoch"), (7, "currentEpoch")] {
        ensure!(
            current[index][epoch].as_u64() >= captured[index][epoch].as_u64()
                && current[index]["revision"].as_u64() >= captured[index]["revision"].as_u64(),
            "native assignment epoch or revision regressed"
        );
    }
    Ok(())
}
