// Origin: CTOX
// License: AGPL-3.0-only
//! Durable publication outbox. Speech runs outside the Supervisor turn and
//! database transactions, with one native deck producer per state root.
use super::super::{project_chats::jour_fixe_owner, workjet_jour_fixe_contract as wire};
use super::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;
use sha2::{Digest, Sha256};
use wire::WireValidate;

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_jour_fixe_deck_narration (
 meeting_id TEXT NOT NULL, deck_revision INTEGER NOT NULL, publication_operation_id TEXT NOT NULL,
 authority_json TEXT NOT NULL, state TEXT NOT NULL, error TEXT,
 PRIMARY KEY(meeting_id,deck_revision));";

#[derive(Serialize, Deserialize)]
pub(super) struct Job {
    meeting_id: String,
    deck_revision: u64,
    owner: String,
    actor: String,
    project: String,
    thread: String,
    epoch: i64,
}
impl Job {
    pub(super) fn current(
        &self,
        policy: &Connection,
        context: &McpChannelRequestContext,
        request: &wire::NarrateRequest,
    ) -> anyhow::Result<wire::Meeting> {
        anyhow::ensure!(
            context.actor == self.actor
                && request.meeting_id == self.meeting_id
                && request.deck_revision == self.deck_revision,
            "native narration publication scope differs"
        );
        let meeting =
            jour_fixe_owner::owned(policy, &self.actor, Some(&self.project), &self.meeting_id)?;
        anyhow::ensure!(
            meeting.owner_user_id == self.owner
                && meeting.supervisor.workjet_thread_id == self.thread
                && workjet_worker_dispatch::current_project(
                    policy,
                    context,
                    &self.project,
                    &self.thread
                )? == self.epoch,
            "native narration publication authority changed"
        );
        Ok(meeting)
    }
    fn context(&self, policy: &Connection) -> anyhow::Result<McpChannelRequestContext> {
        let role: String = policy.query_row(
            "SELECT role FROM business_users WHERE user_id=?1 AND active=1",
            [&self.actor],
            |r| r.get(0),
        )?;
        Ok(McpChannelRequestContext {
            channel: "native-jour-fixe-narration".into(),
            surface: "native".into(),
            actor: self.actor.clone(),
            workspace: self.project.clone(),
            tool: workjet_jour_fixe::WRITE_TOOL.into(),
            request_id: self.meeting_id.clone(),
            confirmation_state: McpConfirmationState::NotRequired,
            trusted_role: Some(role),
            trusted_role_source: None,
            trusted_managed_read_scope: None,
            trusted_managed_instance_id: None,
        })
    }
}

/// Called only after the signed Supervisor publication passed its existing
/// lease, project, owner and policy checks. The outbox commits with that deck.
pub(super) fn enqueue(
    policy: &Connection,
    context: &McpChannelRequestContext,
    meeting: &wire::Meeting,
    operation: &str,
) -> anyhow::Result<()> {
    let job = Job {
        meeting_id: meeting.id.clone(),
        deck_revision: meeting.deck_revision,
        owner: meeting.owner_user_id.clone(),
        actor: context.actor.clone(),
        project: meeting.project_id.clone(),
        thread: meeting.supervisor.workjet_thread_id.clone(),
        epoch: workjet_worker_dispatch::current_project(
            policy,
            context,
            &meeting.project_id,
            &meeting.supervisor.workjet_thread_id,
        )?,
    };
    policy.execute_batch(SCHEMA)?;
    policy.execute(
        "INSERT INTO workjet_jour_fixe_deck_narration
        (meeting_id,deck_revision,publication_operation_id,authority_json,state)
        VALUES(?1,?2,?3,?4,'queued') ON CONFLICT(meeting_id,deck_revision) DO NOTHING",
        params![
            job.meeting_id,
            job.deck_revision,
            operation,
            serde_json::to_string(&job)?
        ],
    )?;
    Ok(())
}

pub(super) fn retry_publication(policy: &Connection, operation: &str) -> anyhow::Result<()> {
    let exists: bool = policy.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='workjet_jour_fixe_deck_narration')",
        [],
        |r| r.get(0),
    )?;
    if exists {
        policy.execute("UPDATE workjet_jour_fixe_deck_narration SET state='queued',error=NULL WHERE publication_operation_id=?1 AND state='blocked'", [operation])?;
    }
    Ok(())
}

fn pending(policy: &Connection) -> anyhow::Result<Option<Job>> {
    let exists: bool = policy.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master
        WHERE type='table' AND name='workjet_jour_fixe_deck_narration')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    let raw: Option<String> = policy
        .query_row(
            "SELECT authority_json FROM workjet_jour_fixe_deck_narration
        WHERE state IN ('queued','running') ORDER BY rowid LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|v| serde_json::from_str(&v).map_err(Into::into))
        .transpose()
}

struct ProducerLock(std::fs::File);
impl Drop for ProducerLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// One bounded deck per kick. The daemon's existing timer resumes a committed
/// job after a crash/quit; it does not lease a model turn or wait for the UI.
pub(super) fn resume(root: &Path) -> anyhow::Result<()> {
    if !store::business_os_store_path(root).exists() {
        return Ok(());
    }
    let reader = Connection::open_with_flags(
        store::business_os_store_path(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    reader.busy_timeout(std::time::Duration::from_secs(5))?;
    if pending(&reader)?.is_none() {
        return Ok(());
    }
    drop(reader);
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(crate::paths::core_db(root).with_file_name("workjet-deck-narration.lock"))?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Ok(()),
        Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
    }
    let guard = ProducerLock(lock);
    let root = root.to_owned();
    std::thread::Builder::new()
        .name("jour-fixe-narration".into())
        .spawn(move || {
            let _guard = guard;
            if let Err(error) = run_next_with(&root, |request| {
                workjet_narration::synthesize(&root, request)
            }) {
                eprintln!("[jour-fixe] native deck narration stopped: {error}");
            }
        })?;
    Ok(())
}

pub(super) fn run_next_with<F>(root: &Path, mut producer: F) -> anyhow::Result<()>
where
    F: FnMut(
        &crate::execution::speech::SpeechRequest,
    ) -> Result<
        crate::execution::speech::VerifiedSpeechOutput,
        crate::execution::speech::SpeechError,
    >,
{
    let policy = store::open_store(root)?;
    let Some(job) = pending(&policy)? else {
        return Ok(());
    };
    policy.execute(
        "UPDATE workjet_jour_fixe_deck_narration SET state='running',error=NULL
        WHERE meeting_id=?1 AND deck_revision=?2",
        params![job.meeting_id, job.deck_revision],
    )?;
    drop(policy);
    let result = narrate_with(root, &job, &mut producer);
    let (state, error) = match &result {
        Ok(()) => ("complete", None),
        Err(error) => (
            "blocked",
            Some(error.to_string().chars().take(1024).collect::<String>()),
        ),
    };
    store::open_store(root)?.execute(
        "UPDATE workjet_jour_fixe_deck_narration SET state=?3,error=?4
        WHERE meeting_id=?1 AND deck_revision=?2",
        params![job.meeting_id, job.deck_revision, state, error],
    )?;
    result
}

fn narrate_with<F>(root: &Path, job: &Job, producer: &mut F) -> anyhow::Result<()>
where
    F: FnMut(
        &crate::execution::speech::SpeechRequest,
    ) -> Result<
        crate::execution::speech::VerifiedSpeechOutput,
        crate::execution::speech::SpeechError,
    >,
{
    // A prior run may have committed WAV custody but stopped before publishing
    // its RxDB references. Replay exact receipts before skipping retained audio.
    let policy = store::open_store(root)?;
    let context = job.context(&policy)?;
    if policy.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='workjet_jour_fixe_native_narration')", [], |r| r.get::<_,bool>(0))? {
        let mut statement = policy.prepare("SELECT receipt_json FROM workjet_jour_fixe_native_narration
            WHERE meeting_id=?1 AND deck_revision=?2 AND state='complete' LIMIT 100")?;
        let receipts = statement.query_map(params![job.meeting_id,job.deck_revision], |r| r.get::<_,String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        drop(policy);
        for raw in receipts {
            let receipt: Value = serde_json::from_str(&raw)?;
            let narration: wire::NativeNarrationReceipt = serde_json::from_value(receipt["native_narration"].clone())?;
            narration.validate().map_err(anyhow::Error::msg)?;
            let request = wire::NarrateRequest {
                operation_id: narration.operation_id, meeting_id: job.meeting_id.clone(),
                slide_id: narration.slide_id, deck_revision: job.deck_revision,
                expected_revision: narration.revision.checked_sub(1).context("native narration receipt revision missing")?,
                narration_text_sha256: narration.audio.narration_text_sha256,
            };
            workjet_narration::execute_published_with(root, &context,
                &json!({"action":"narrate","request":request}), job, |_| {
                    Err(crate::execution::speech::SpeechError::ExecutionUnavailable)
                })?;
        }
    }
    // Wire decks contain at most 100 slides. One input/revision is read anew
    // after each committed WAV; reserved/uncertain slots never resynthesize.
    for _ in 0..=100 {
        let policy = store::open_store(root)?;
        let context = job.context(&policy)?;
        let meeting =
            jour_fixe_owner::owned(&policy, &job.actor, Some(&job.project), &job.meeting_id)?;
        anyhow::ensure!(
            meeting.deck_revision == job.deck_revision,
            "published deck was replaced"
        );
        let input = workjet_narration::inputs(&meeting).into_iter().next();
        if input.is_none() {
            anyhow::ensure!(
                meeting.state == wire::MeetingState::Ready
                    && !meeting.slides.is_empty()
                    && meeting.slides.iter().all(|s| s.audio.is_some()),
                "published deck is not ready and has no native narration input"
            );
            return Ok(());
        }
        let input = input.unwrap();
        let operation = if policy.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master
            WHERE name='workjet_jour_fixe_native_narration')",
            [],
            |r| r.get::<_, bool>(0),
        )? {
            policy
                .query_row(
                    "SELECT operation_id FROM workjet_jour_fixe_native_narration
                WHERE meeting_id=?1 AND deck_revision=?2 AND slide_id=?3",
                    params![job.meeting_id, job.deck_revision, input.slide_id],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
        } else {
            None
        };
        let operation = operation.unwrap_or_else(|| {
            format!(
                "native_deck_{:.32}",
                format!(
                    "{:x}",
                    Sha256::digest(
                        format!(
                            "{}:{}:{}",
                            job.meeting_id, job.deck_revision, input.slide_id
                        )
                        .as_bytes()
                    )
                )
            )
        });
        let request = wire::NarrateRequest {
            operation_id: operation,
            meeting_id: job.meeting_id.clone(),
            slide_id: input.slide_id,
            deck_revision: input.deck_revision,
            expected_revision: input.expected_revision,
            narration_text_sha256: input.narration_text_sha256,
        };
        // Revalidate the durable publication before synthesis AND audio commit.
        job.current(&policy, &context, &request)?
            .validate()
            .map_err(anyhow::Error::msg)?;
        drop(policy);
        workjet_narration::execute_published_with(
            root,
            &context,
            &json!({"action":"narrate","request":request}),
            job,
            |r| producer(r),
        )?;
    }
    anyhow::bail!("native deck narration slide budget exhausted")
}
