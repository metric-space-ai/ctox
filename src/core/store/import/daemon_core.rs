//! Frozen Core projection adapters. No connections, writers or replacement
//! conversation-ID algorithm are created here; the host supplies its real rule.
use super::*;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::{
    fs,
    path::{Component, Path},
};

pub type Conversation = fn(&str) -> i64;
pub const TABLES: &[&str] = &[
    "ticket_items",
    "ticket_events",
    "ticket_cases",
    "ticket_approvals",
    "ticket_verifications",
    "ticket_writebacks",
    "ticket_clarification_requests",
    "ticket_execution_actions",
    "local_ticket_events",
    "iot_assets",
    "iot_attributes",
    "iot_asset_types",
    "iot_alarms",
    "iot_rulesets",
    "iot_agents",
    "iot_agent_status",
    "iot_dashboards",
    "iot_widgets",
    "iot_condition_state",
    "iot_recurrence_block",
    "iot_datapoints",
    "crew_members",
    "crew_member_learnings",
    "continuity_documents",
    "continuity_commits",
    "communication_routing_state",
    "crew_attempts",
    "worker_run_identities",
    "scrape_target",
    "scrape_script_revision",
    "scrape_run",
    "scrape_artifact",
    "scrape_account_state",
    "outbound_native_field_status_witnesses",
    "workjet_supervisor_bindings",
    "stalwart_mail_tracking_tokens",
    "stalwart_mail_tracking_events",
];
pub(crate) struct Services {
    conversation: Conversation,
    original: std::path::PathBuf,
    copied: std::path::PathBuf,
    owner_user: Option<String>,
    pub(crate) files: Vec<file_import::ByteFile>,
    bytes: usize,
}
impl Services {
    pub(crate) fn new(
        conversation: Conversation,
        original: &Path,
        copied: &Path,
        owner_user: Option<String>,
    ) -> Result<Self> {
        if !original.is_absolute()
            || original
                .components()
                .any(|c| matches!(c, Component::ParentDir))
        {
            return Err(invalid("cutover_core_original_root_required"));
        }
        Ok(Self {
            conversation,
            original: original.into(),
            copied: copied.into(),
            owner_user,
            files: vec![],
            bytes: 0,
        })
    }
    fn stage(&mut self, bytes: Vec<u8>, expected: Option<&str>) -> Result<String> {
        let sha = format!("{:x}", Sha256::digest(&bytes));
        if expected.is_some_and(|expected| expected != sha) {
            return Err(invalid("cutover_core_artifact_hash_mismatch"));
        }
        if !self.files.iter().any(|f| f.id == format!("core:{sha}")) {
            self.bytes = self.bytes.saturating_add(bytes.len());
            if self.bytes > 256 * 1024 * 1024 {
                return Err(invalid("cutover_core_artifact_budget"));
            }
            self.files.push(file_import::ByteFile {
                id: format!("core:{sha}"),
                name: sha.clone(),
                mime: "application/octet-stream".into(),
                owner_app: "outbound-lead-generation".into(),
                owner_user: self.owner_user.clone(),
                bytes,
            });
        }
        Ok(sha)
    }
}
impl core_import::ImportServices for Services {
    fn member_conversation_id(&self, member: &str) -> i64 {
        (self.conversation)(member)
    }
    fn script(&mut self, _: &str, body: &str, expected: &str) -> Result<Option<(String, u64)>> {
        let bytes = body.trim().as_bytes().to_vec();
        let size = bytes.len() as u64;
        Ok(Some((self.stage(bytes, Some(expected))?, size)))
    }
    fn artifact(&mut self, path: &str, expected: Option<&str>) -> Result<Option<String>> {
        let path = Path::new(path);
        let relative = if path.is_absolute() {
            path.strip_prefix(&self.original)
                .map_err(|_| invalid("file_outside_profile"))?
        } else {
            path
        };
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(invalid("file_unsafe_path"));
        }
        let candidate = self.copied.join(relative);
        match fs::symlink_metadata(&candidate) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
            Ok(_) => (),
        }
        let path = file_import::copied_path(
            &file_import::ProfilePaths {
                original_root: self.original.clone(),
                copied_root: self.copied.clone(),
            },
            path,
        )?;
        let metadata = fs::metadata(&path)?;
        if !metadata.is_file() || metadata.len() > 256 * 1024 * 1024 {
            return Err(invalid("cutover_core_artifact_budget"));
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(256 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        self.stage(bytes, expected).map(Some)
    }
}
