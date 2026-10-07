// Origin: CTOX
// License: AGPL-3.0-only

//! Native host paths come only from local-operator assignments. Workjet's
//! working-copy path remains opaque and never becomes a filesystem authority.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::os::unix::fs::MetadataExt;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkspaceAssignmentInput {
    pub owner_user_id: String,
    pub worker_profile_id: String,
    pub project_id: String,
    pub working_copy_id: String,
    pub native_workspace: PathBuf,
}

fn workspace_identity(path: &Path) -> Result<(u64, u64)> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        path.is_absolute()
            && std::fs::canonicalize(path)? == path
            && metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o022 == 0,
        "native workspace is not canonical, owned or protected from foreign writes"
    );
    Ok((metadata.dev(), metadata.ino()))
}

fn working_copy(
    policy: &Connection,
    owner: &str,
    project: &str,
    computer: &str,
    copy: &str,
) -> Result<Value> {
    let p = outbound_load_record(policy, "workjet_projects", project)?
        .context("native workspace project is unavailable")?;
    let w = outbound_load_record(policy, "workjet_working_copies", copy)?
        .context("native working-copy assignment is unavailable")?;
    ensure!(
        p["owner_user_id"] == owner
            && p["is_deleted"] != true
            && p["status"] != "archived"
            && w["owner_user_id"] == owner
            && w["project_id"] == project
            && w["computer_id"] == computer
            && w["status"] == "active"
            && w["is_deleted"] != true,
        "native workspace has a foreign, detached or archived working copy"
    );
    Ok(w)
}

/// Same policy transaction as provider grants. Invalid input leaves both
/// assignments unchanged; host startup never recreates a revoked grant.
pub(super) fn configure(
    policy: &Connection,
    computer: &str,
    assignments: &[WorkspaceAssignmentInput],
) -> Result<()> {
    ensure!(
        identifier(computer) && assignments.len() <= 64,
        "invalid workspace configuration"
    );
    policy.execute(
        "UPDATE business_native_guest_workspace_assignments SET state='revoked',revision=revision+1 WHERE computer_id=?1",
        [computer],
    )?;
    let mut seen = BTreeSet::new();
    for a in assignments {
        ensure!(
            identifier(&a.owner_user_id)
                && identifier(&a.worker_profile_id)
                && identifier(&a.project_id)
                && identifier(&a.working_copy_id)
                && seen.insert((&a.owner_user_id, &a.worker_profile_id, &a.project_id)),
            "invalid or duplicate native workspace assignment"
        );
        let profile = super::super::worker_profile_bindings::require_active(
            policy,
            &a.owner_user_id,
            &a.worker_profile_id,
        )?;
        ensure!(
            profile["computer_id"] == computer,
            "workspace profile belongs to another computer"
        );
        working_copy(
            policy,
            &a.owner_user_id,
            &a.project_id,
            computer,
            &a.working_copy_id,
        )?;
        let (device, inode) = workspace_identity(&a.native_workspace)?;
        let epoch: i64 = policy.query_row(
            "SELECT capability_epoch FROM business_users WHERE user_id=?1 AND active=1",
            [&a.owner_user_id],
            |row| row.get(0),
        )?;
        policy.execute(
            "INSERT INTO business_native_guest_workspace_assignments
            (owner_user_id,worker_profile_id,project_id,computer_id,working_copy_id,
             native_workspace,workspace_device,workspace_inode,principal_epoch,state,revision)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'active',1)
            ON CONFLICT(owner_user_id,worker_profile_id,project_id) DO UPDATE SET
            computer_id=excluded.computer_id,working_copy_id=excluded.working_copy_id,
            native_workspace=excluded.native_workspace,workspace_device=excluded.workspace_device,
            workspace_inode=excluded.workspace_inode,principal_epoch=excluded.principal_epoch,
            state='active',revision=business_native_guest_workspace_assignments.revision+1",
            rusqlite::params![
                a.owner_user_id,
                a.worker_profile_id,
                a.project_id,
                computer,
                a.working_copy_id,
                a.native_workspace
                    .to_str()
                    .context("native workspace is not UTF-8")?,
                i64::try_from(device)?,
                i64::try_from(inode)?,
                epoch,
            ],
        )?;
    }
    Ok(())
}

pub(crate) fn configure_native_guest_assignments(
    root: &Path,
    computer: &str,
    providers: &[super::accounts::ProviderAssignmentInput],
    workspaces: &[WorkspaceAssignmentInput],
) -> Result<()> {
    let mut policy = super::super::store::open_store(root)?;
    let tx = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
    super::accounts::configure_in_transaction(&tx, computer, providers)?;
    configure(&tx, computer, workspaces)?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn revoke_workspace_assignment(
    root: &Path,
    owner: &str,
    profile: &str,
    project: &str,
) -> Result<()> {
    ensure!(
        identifier(owner) && identifier(profile) && identifier(project),
        "invalid native workspace revocation"
    );
    let mut policy = super::super::store::open_store(root)?;
    let tx = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "UPDATE business_native_guest_workspace_assignments SET state='revoked',revision=revision+1
        WHERE owner_user_id=?1 AND worker_profile_id=?2 AND project_id=?3",
        rusqlite::params![owner, profile, project],
    )?;
    tx.commit()?;
    Ok(())
}

pub(super) fn snapshot(policy: &Connection, d: &GuestRestoreDestination) -> Result<Option<Value>> {
    snapshot_scope(
        policy,
        &d.human_owner_id,
        &d.worker_profile_id,
        &d.project_id,
    )
}

pub(super) fn snapshot_scope(
    policy: &Connection,
    owner: &str,
    profile: &str,
    project: &str,
) -> Result<Option<Value>> {
    let exists: bool = policy.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table'
        AND name='business_native_guest_workspace_assignments')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    let mut assigned: Option<Value> = policy.query_row(
        "SELECT a.computer_id,a.working_copy_id,a.native_workspace,a.workspace_device,a.workspace_inode,
        a.principal_epoch,a.state,a.revision,u.active,u.capability_epoch
        FROM business_native_guest_workspace_assignments a JOIN business_users u ON u.user_id=a.owner_user_id
        WHERE a.owner_user_id=?1 AND a.worker_profile_id=?2 AND a.project_id=?3",
        rusqlite::params![owner,profile,project],
        |row| Ok(json!({
            "computerId":row.get::<_,String>(0)?,
            "workingCopyId":row.get::<_,String>(1)?,
            "nativeWorkspace":row.get::<_,String>(2)?,
            "device":row.get::<_,i64>(3)?,"inode":row.get::<_,i64>(4)?,
            "principalEpoch":row.get::<_,i64>(5)?,
            "state":row.get::<_,String>(6)?,"revision":row.get::<_,i64>(7)?,
            "principalActive":row.get::<_,i64>(8)?,"currentEpoch":row.get::<_,i64>(9)?,
        })),
    ).optional()?;
    if let Some(a) = &mut assigned {
        let copy = working_copy(
            policy,
            &owner,
            &project,
            a["computerId"]
                .as_str()
                .context("native workspace computer absent")?,
            a["workingCopyId"]
                .as_str()
                .context("native working copy absent")?,
        )?;
        a["workingCopy"] = copy;
    }
    Ok(assigned)
}

pub(super) struct AssignedWorkspace {
    pub path: PathBuf,
    pub working_copy_id: String,
    pub revision: u64,
    identity: (u64, u64),
}
impl AssignedWorkspace {
    pub(super) fn verify(&self) -> Result<()> {
        ensure!(
            workspace_identity(&self.path)? == self.identity,
            "native workspace was replaced"
        );
        Ok(())
    }
}

pub(super) fn require(
    policy: &Connection,
    d: &GuestRestoreDestination,
    actual_cwd: &Path,
) -> Result<AssignedWorkspace> {
    require_scope(
        policy,
        &d.human_owner_id,
        &d.worker_profile_id,
        &d.project_id,
        actual_cwd,
    )
}

pub(super) fn require_scope(
    policy: &Connection,
    owner: &str,
    profile_id: &str,
    project: &str,
    actual_cwd: &Path,
) -> Result<AssignedWorkspace> {
    let a = snapshot_scope(policy, owner, profile_id, project)?
        .context("native workspace has no explicit host assignment")?;
    let profile =
        super::super::worker_profile_bindings::require_active(policy, &owner, &profile_id)?;
    ensure!(
        a["state"] == "active"
            && a["principalActive"] == 1
            && a["principalEpoch"] == a["currentEpoch"]
            && a["computerId"] == profile["computer_id"],
        "native workspace owner, epoch or computer assignment changed"
    );
    let assigned = AssignedWorkspace {
        path: PathBuf::from(
            a["nativeWorkspace"]
                .as_str()
                .context("native workspace path absent")?,
        ),
        working_copy_id: a["workingCopyId"]
            .as_str()
            .context("native working copy absent")?
            .into(),
        revision: a["revision"]
            .as_u64()
            .context("native workspace revision invalid")?,
        identity: (
            a["device"]
                .as_u64()
                .context("native workspace device invalid")?,
            a["inode"]
                .as_u64()
                .context("native workspace inode invalid")?,
        ),
    };
    ensure!(
        actual_cwd == assigned.path,
        "actual Core workspace differs from native assignment"
    );
    working_copy(
        policy,
        &owner,
        &project,
        a["computerId"]
            .as_str()
            .context("native workspace computer absent")?,
        &assigned.working_copy_id,
    )?;
    assigned.verify()?;
    Ok(assigned)
}

impl NativeGuestExecution {
    /// A configured portable workspace must match the actual TurnStart cwd
    /// before the native producer can execute. Unconfigured ordinary chats
    /// retain journal-only capture and gain no filesystem/export grant.
    pub(crate) fn verify_turn_workspace(&self, cwd: &Path) -> Result<()> {
        self.provider
            .with_live_provider_transaction(|worker, facts, _| {
                self.with_held_worker_policy(worker, facts, |entry, verify, policy| {
                    verify()?;
                    if snapshot(policy, &entry.assignment.destination)?.is_some() {
                        require(policy, &entry.assignment.destination, cwd)?;
                    }
                    Ok(())
                })
            })
    }
}

pub(super) fn acquire_lease(
    policy: &Connection,
    destination: &GuestRestoreDestination,
) -> Result<Option<std::fs::File>> {
    let Some(snapshot) = snapshot(policy, destination)? else {
        return Ok(None);
    };
    let path = PathBuf::from(
        snapshot["nativeWorkspace"]
            .as_str()
            .context("native workspace path absent")?,
    );
    let assigned = require(policy, destination, &path)?;
    use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
    let directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&assigned.path)?;
    let metadata = directory.metadata()?;
    ensure!(
        (metadata.dev(), metadata.ino()) == assigned.identity,
        "workspace changed during lease acquisition"
    );
    if unsafe { libc::flock(directory.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        anyhow::bail!(
            "native working copy already has a writer: {}",
            std::io::Error::last_os_error()
        );
    }
    assigned.verify()?;
    Ok(Some(directory))
}
