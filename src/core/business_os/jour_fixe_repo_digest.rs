// Origin: CTOX
// License: AGPL-3.0-only
//! Operator-supplied repository facts, without credentials or a sync projection.
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{io::Read, path::Path};

pub(crate) const MAX_BYTES: usize = 256 * 1024;
pub(crate) const MAX_AGE_MS: i64 = 36 * 60 * 60 * 1000;
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_jour_fixe_repo_digests (
 project_id TEXT PRIMARY KEY, repository TEXT NOT NULL, digest_json TEXT NOT NULL,
 collected_at TEXT NOT NULL, collector TEXT NOT NULL);";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepoDigest {
    pub repository: String,
    pub since: String,
    pub collected_at: String,
    pub commits: Vec<Commit>,
    pub pulls: Vec<Pull>,
    pub issues: Vec<Issue>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Commit {
    pub sha: String,
    pub author: String,
    pub date: String,
    pub message: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Pull {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub merged_at: Option<String>,
    pub author: String,
    pub updated_at: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Issue {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub updated_at: String,
}
pub(crate) fn timestamp(value: &str) -> Result<i64> {
    Ok(chrono::DateTime::parse_from_rfc3339(value)
        .context("expected RFC3339 timestamp")?
        .timestamp_millis())
}
fn valid_repository(value: &str) -> bool {
    let parts: Vec<_> = value.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 100
                && *p != "."
                && *p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
}
pub(crate) fn parse(raw: &[u8]) -> Result<RepoDigest> {
    anyhow::ensure!(
        raw.len() <= MAX_BYTES,
        "repo digest exceeds 256 KiB raw budget"
    );
    let value: Value = serde_json::from_slice(raw).context("repo digest must be JSON")?;
    // merged_at is a required nullable field, rather than an omitted claim.
    if let Some(pulls) = value["pulls"].as_array() {
        for (i, pull) in pulls.iter().enumerate() {
            anyhow::ensure!(
                pull.get("merged_at").is_some(),
                "repo digest /pulls/{i}/merged_at: required field missing"
            );
        }
    }
    let digest: RepoDigest = serde_json::from_value(value).context("invalid repo digest fields")?;
    anyhow::ensure!(
        valid_repository(&digest.repository),
        "repo digest /repository: expected owner/name"
    );
    let since = timestamp(&digest.since).context("repo digest /since")?;
    let collected = timestamp(&digest.collected_at).context("repo digest /collected_at")?;
    anyhow::ensure!(since <= collected, "repo digest /since: after collected_at");
    for (i, commit) in digest.commits.iter().enumerate() {
        anyhow::ensure!(
            matches!(commit.sha.len(), 40 | 64)
                && commit.sha.bytes().all(|b| b.is_ascii_hexdigit()),
            "repo digest /commits/{i}/sha: expected full hexadecimal SHA"
        );
        timestamp(&commit.date).with_context(|| format!("repo digest /commits/{i}/date"))?;
    }
    for (i, pull) in digest.pulls.iter().enumerate() {
        anyhow::ensure!(
            pull.number > 0 && matches!(pull.state.as_str(), "open" | "closed"),
            "repo digest /pulls/{i}: invalid number or state"
        );
        timestamp(&pull.updated_at)
            .with_context(|| format!("repo digest /pulls/{i}/updated_at"))?;
        if let Some(at) = &pull.merged_at {
            timestamp(at).with_context(|| format!("repo digest /pulls/{i}/merged_at"))?;
            anyhow::ensure!(
                pull.state == "closed",
                "repo digest /pulls/{i}/state: merged pull must be closed"
            );
        }
    }
    for (i, issue) in digest.issues.iter().enumerate() {
        anyhow::ensure!(
            issue.number > 0 && matches!(issue.state.as_str(), "open" | "closed"),
            "repo digest /issues/{i}: invalid number or state"
        );
        timestamp(&issue.updated_at)
            .with_context(|| format!("repo digest /issues/{i}/updated_at"))?;
    }
    Ok(digest)
}
fn table_exists(conn: &Connection) -> Result<bool> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_jour_fixe_repo_digests')",[],|r|r.get(0))?)
}
pub(crate) fn load(conn: &Connection, project: &str) -> Result<Option<RepoDigest>> {
    if !table_exists(conn)? {
        return Ok(None);
    }
    let row: Option<(String,String,String)> = conn.query_row(
        "SELECT repository,digest_json,collected_at FROM workjet_jour_fixe_repo_digests WHERE project_id=?1",
        [project], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    row.map(|(repo, raw, at)| {
        let digest = parse(raw.as_bytes())?;
        anyhow::ensure!(
            digest.repository == repo && digest.collected_at == at,
            "stored repo digest metadata differs"
        );
        Ok(digest)
    })
    .transpose()
}
fn show(root: &Path, project: &str) -> Result<Value> {
    let path = super::store::business_os_store_path(root);
    if !path.exists() {
        return Ok(Value::Null);
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let Some(digest) = load(&conn, project)? else {
        return Ok(Value::Null);
    };
    let collector: String = conn.query_row(
        "SELECT collector FROM workjet_jour_fixe_repo_digests WHERE project_id=?1",
        [project],
        |r| r.get(0),
    )?;
    Ok(
        json!({"project_id":project,"repository":digest.repository,"collected_at":digest.collected_at,
        "collector":collector,"digest":digest}),
    )
}
/// Local filesystem operator authority only; never exposed as a browser command.
pub(crate) fn cli(root: &Path, args: &[String], input: &mut dyn Read) -> Result<Value> {
    let action = args.first().map(String::as_str).context("usage: ctox jour-fixe repo-digest put --project-id ID --repository OWNER/NAME | show --project-id ID")?;
    let mut project = None;
    let mut repository = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut i = 1;
    while i < args.len() {
        let flag = &args[i];
        anyhow::ensure!(
            seen.insert(flag.clone()),
            "duplicate repo digest option: {flag}"
        );
        let value = args
            .get(i + 1)
            .context("repo digest option value missing")?;
        match flag.as_str() {
            "--project-id" => project = Some(value.as_str()),
            "--repository" => repository = Some(value.as_str()),
            _ => anyhow::bail!("unknown repo digest option: {flag}"),
        }
        i += 2;
    }
    let project = project.context("--project-id required")?;
    anyhow::ensure!(
        !project.is_empty() && project.len() <= 128 && project.trim() == project,
        "repo digest project_id must be canonical and bounded"
    );
    match action {
        "show" => {
            anyhow::ensure!(repository.is_none(), "show does not take --repository");
            show(root, project)
        }
        "put" => {
            let repository = repository.context("--repository required")?;
            let mut raw = Vec::new();
            input.take((MAX_BYTES + 1) as u64).read_to_end(&mut raw)?;
            let digest = parse(&raw)?;
            anyhow::ensure!(
                digest.repository == repository,
                "repo digest /repository differs from --repository"
            );
            let mut conn = super::store::open_store(root)?;
            conn.busy_timeout(std::time::Duration::from_secs(5))?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            anyhow::ensure!(
                super::store::outbound_load_record(&tx, "workjet_projects", project)?.is_some(),
                "repo digest project is not registered"
            );
            tx.execute_batch(SCHEMA)?;
            tx.execute("INSERT INTO workjet_jour_fixe_repo_digests(project_id,repository,digest_json,collected_at,collector)
                VALUES(?1,?2,?3,?4,'ctox_operator_cli') ON CONFLICT(project_id) DO UPDATE SET
                repository=excluded.repository,digest_json=excluded.digest_json,collected_at=excluded.collected_at,collector=excluded.collector",
                params![project,repository,serde_json::to_string(&digest)?,digest.collected_at])?;
            tx.commit()?;
            show(root, project)
        }
        _ => anyhow::bail!("unknown repo digest action: {action}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(crate) fn example() -> Value {
        json!({"repository":"acme/private","since":"2026-10-05T00:00:00Z","collected_at":"2026-10-12T20:00:00Z",
            "commits":[{"sha":"1234567890123456789012345678901234567890","author":"Test operator","date":"2026-10-12T19:00:00Z","message":"A retained work result"}],
            "pulls":[{"number":1,"title":"Merged result","state":"closed","merged_at":"2026-10-12T19:00:00Z","author":"Test operator","updated_at":"2026-10-12T19:00:00Z"}],
            "issues":[{"number":2,"title":"Remaining work","state":"open","updated_at":"2026-10-12T19:00:00Z"}]})
    }
    #[test]
    fn jour_fixe_repo_digest_cli_put_show_scope_and_strict_fields() -> Result<()> {
        let root = tempfile::tempdir()?;
        let conn = super::super::store::open_store(root.path())?;
        super::super::store::upsert_business_record(
            &conn,
            "workjet_projects",
            "project",
            1,
            json!({"id":"project","name":"Project","owner_user_id":"owner"}),
        )?;
        drop(conn);
        let args = |values: &[&str]| values.iter().map(|v| v.to_string()).collect::<Vec<_>>();
        let put = args(&[
            "put",
            "--project-id",
            "project",
            "--repository",
            "acme/private",
        ]);
        let mut input = std::io::Cursor::new(serde_json::to_vec(&example())?);
        let first = cli(root.path(), &put, &mut input)?;
        assert_eq!(first["digest"], example());
        assert_eq!(first["collector"], "ctox_operator_cli");
        let main_args = [
            "jour-fixe",
            "repo-digest",
            "show",
            "--project-id",
            "project",
        ]
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>();
        assert!(crate::skips_cli_startup_db(&main_args));
        assert!(crate::skips_cli_turn_ledger(&main_args));
        crate::dispatch_command(root.path(), &main_args)?;
        assert_eq!(
            cli(
                root.path(),
                &args(&["show", "--project-id", "project"]),
                &mut std::io::empty()
            )?,
            first
        );
        assert!(cli(
            root.path(),
            &args(&["show", "--project-id", "foreign"]),
            &mut std::io::empty()
        )?
        .is_null());
        let mut extra = example();
        extra["token"] = json!("forbidden");
        assert!(cli(
            root.path(),
            &put,
            &mut std::io::Cursor::new(extra.to_string())
        )
        .is_err());
        extra = example();
        extra["commits"][0]["unknown"] = json!(true);
        assert!(parse(extra.to_string().as_bytes()).is_err());
        let foreign = args(&[
            "put",
            "--project-id",
            "foreign",
            "--repository",
            "acme/private",
        ]);
        assert!(cli(
            root.path(),
            &foreign,
            &mut std::io::Cursor::new(example().to_string())
        )
        .is_err());
        assert!(parse(&vec![b' '; MAX_BYTES + 1]).is_err());
        Ok(())
    }
}
