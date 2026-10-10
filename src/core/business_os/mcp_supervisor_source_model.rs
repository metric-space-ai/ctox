// Origin: CTOX
// License: AGPL-3.0-only
//! Private native model requests for the original enrolled Source.
//! HTTP correlations are upstream observations, not SDK execution/stop proof.
use super::*;
use crate::execution::cliproxyapi_claude_proxy::{
    NativeClaudeLeaseModelProxy, NativeClaudeModelExchange, NativeClaudeOperation,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use sha2::Digest;
use std::collections::VecDeque;

const MAX_BODY: usize = 96 * 1024;
const MAX_REPLY: usize = 8 * 1024 * 1024;
const CHUNK: usize = 32 * 1024;
const MAX_OPERATIONS: usize = 64;
const MODEL_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS workjet_supervisor_native_model_requests (
 operation_id TEXT PRIMARY KEY, execution_key TEXT NOT NULL, lease_hash TEXT NOT NULL,
 controller_id TEXT NOT NULL, sdk_correlation TEXT NOT NULL, body_hash TEXT NOT NULL,
 state TEXT NOT NULL, requested_model TEXT, response_model TEXT, upstream_request_id TEXT, http_status INTEGER,
 created_at_ms INTEGER NOT NULL, finished_at_ms INTEGER);";

#[derive(Default)]
pub(super) struct ModelRegistry {
    sessions: Mutex<HashMap<String, Arc<ModelSession>>>,
}
struct ModelSession {
    controller: Arc<NativeSupervisorHoldingController>,
    proxy: Arc<NativeClaudeLeaseModelProxy>,
    jobs: Mutex<HashMap<String, Arc<ModelJob>>>,
}
struct ModelJob {
    hash: String,
    state: Mutex<ModelState>,
    task: Mutex<Option<tokio::task::AbortHandle>>,
}
#[derive(Default)]
struct ModelState {
    frames: VecDeque<Frame>,
    next: u64,
    acknowledged: u64,
    total: usize,
    finished: bool,
    failed: bool,
    witness: Option<UpstreamObservation>,
    response_model: ResponseModelObservation,
}
#[derive(Default)]
struct ResponseModelObservation {
    pending: Vec<u8>,
    skipping: bool,
    model: Option<String>,
    conflicting: bool,
}
impl ResponseModelObservation {
    fn value(&mut self, value: &Value) {
        let message = if value["type"] == "message_start" {
            &value["message"]
        } else {
            value
        };
        if message["type"] != "message" {
            return;
        }
        let Some(model) = message["model"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
        else {
            return;
        };
        if self.model.as_deref().is_some_and(|old| old != model) {
            self.conflicting = true;
        } else {
            self.model = Some(model.to_owned());
        }
    }
    fn observe(&mut self, bytes: &[u8], streaming: bool, status: u16) {
        if !(200..300).contains(&status) {
            return;
        }
        if !streaming {
            if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
                self.value(&value);
            }
            return;
        }
        // Bounded incremental SSE line decoder; large content deltas are skipped,
        // never reinterpreted as a new message header.
        for byte in bytes {
            if *byte == b'\n' {
                if !self.skipping {
                    if let Some(data) = self.pending.strip_prefix(b"data: ") {
                        if let Ok(value) = serde_json::from_slice::<Value>(data) {
                            if value["type"] == "message_start" {
                                self.value(&value);
                            }
                        }
                    }
                }
                self.pending.clear();
                self.skipping = false;
            } else if !self.skipping {
                if self.pending.len() == 64 * 1024 {
                    self.pending.clear();
                    self.skipping = true;
                } else {
                    self.pending.push(*byte);
                }
            }
        }
    }
}

struct Frame {
    sequence: u64,
    bytes: Vec<u8>,
    status: u16,
    streaming: bool,
}
struct UpstreamObservation {
    // Only constructed from Models' genuine non-deserializable HTTP exchange.
    model: String,
    request_id: String,
    status: u16,
}
struct PreparedInvocation {
    session: Arc<ModelSession>,
    job: Arc<ModelJob>,
    id: String,
    body: Vec<u8>,
    correlation: String,
    operation: NativeClaudeOperation,
    fresh: bool,
}
impl ModelRegistry {
    fn session(
        &self,
        controller: Arc<NativeSupervisorHoldingController>,
    ) -> anyhow::Result<Arc<ModelSession>> {
        let mut sessions = self
            .sessions
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native model registry busy"))?;
        if let Some(session) = sessions.get(controller.controller_id()) {
            anyhow::ensure!(
                Arc::ptr_eq(&session.controller, &controller),
                "native model controller differs"
            );
            return Ok(Arc::clone(session));
        }
        anyhow::ensure!(sessions.len() < 32, "native model session capacity reached");
        // Account/config/secret snapshots are resolved outside responder fences.
        let proxy = NativeClaudeLeaseModelProxy::reserve_shared(Arc::clone(&controller))?;
        let session = Arc::new(ModelSession {
            controller,
            proxy,
            jobs: Mutex::new(HashMap::new()),
        });
        sessions.insert(
            session.controller.controller_id().to_owned(),
            Arc::clone(&session),
        );
        Ok(session)
    }
    pub(super) fn retire(&self, id: &str) {
        let session = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(id);
        if let Some(session) = session {
            let _ = session.proxy.cancel();
            for job in session
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .values()
            {
                if let Some(task) = job
                    .task
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                {
                    task.abort();
                }
            }
        }
    }
}
impl Drop for ModelRegistry {
    fn drop(&mut self) {
        let ids = self
            .sessions
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            self.retire(&id);
        }
    }
}
impl NativeSupervisorSourceHost {
    fn model_controller(
        &self,
        authority: &AdmittedConsumerAuthority,
        operation: &wire::SourceOperation,
    ) -> anyhow::Result<Arc<NativeSupervisorHoldingController>> {
        let id = operation
            .offer_id
            .as_deref()
            .context("native offer missing")?;
        let controller = self
            .controllers
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native Source control busy"))?
            .get(id)
            .cloned()
            .context("original native Source controller unavailable")?;
        anyhow::ensure!(
            operation.controller_id.as_deref() == Some(controller.controller_id()),
            "foreign native model controller"
        );
        // Factory verifies exact incoming peer/credential/generation identity.
        controller.publication_for(authority, Arc::new(ControllerOnly))?;
        controller.with_current(|facts, core, _| {
            let row = read_offer(core, id, facts)?;
            anyhow::ensure!(
                row.state == "claimed"
                    && row.controller_id.as_deref() == Some(controller.controller_id())
                    && row.deadline_ms > now_ms(),
                "native model offer retired"
            );
            Ok(())
        })?;
        Ok(controller)
    }
    pub(super) async fn model_respond(
        self: &Arc<Self>,
        authority: AdmittedConsumerAuthority,
        operation: wire::SourceOperation,
    ) -> anyhow::Result<GuardedAuxiliaryResponse> {
        let host = Arc::clone(self);
        let (session, publication, prepared) =
            tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
                host.prune()?;
                let controller = host.model_controller(&authority, &operation)?;
                let session = host.models.session(controller)?;
                let publication = session.proxy.publication_for(&authority)?;
                let prepared = if operation.action == wire::SourceAction::ModelInvoke {
                    Some(prepare_invocation(&session, &operation)?)
                } else {
                    None
                };
                Ok((session, publication, (operation, prepared)))
            })
            .await
            .context("native model admission context unavailable")??;
        let (operation, prepared) = prepared;
        let id = operation
            .operation_id
            .as_deref()
            .context("native operation missing")?;
        if let Some(prepared) = prepared {
            if prepared.fresh {
                let job = Arc::clone(&prepared.job);
                let task = tokio::spawn(async move {
                    run_invocation(prepared).await;
                });
                *job.task
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(task.abort_handle());
            }
            return Ok(GuardedAuxiliaryResponse {
                result: json!({"version":1,"state":"model_pending","operation_id":id,"execution_ready":false}),
                publication,
            });
        }
        let job = session
            .jobs
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native model operation busy"))?
            .get(id)
            .cloned()
            .context("original native model operation unavailable")?;
        let result = job.read(
            operation
                .sequence
                .context("native model sequence missing")?,
        )?;
        Ok(GuardedAuxiliaryResponse {
            result,
            publication,
        })
    }
}
fn prepare_invocation(
    session: &Arc<ModelSession>,
    operation: &wire::SourceOperation,
) -> anyhow::Result<PreparedInvocation> {
    let id = operation
        .operation_id
        .as_deref()
        .context("native operation missing")?;
    let body = operation
        .body_json
        .as_deref()
        .context("native SDK request missing")?;
    anyhow::ensure!(
        !body.is_empty() && body.len() <= MAX_BODY,
        "native SDK request exceeds budget"
    );
    let correlation = operation
        .sdk_session_id
        .as_deref()
        .context("SDK correlation missing")?;
    anyhow::ensure!(
        !correlation.is_empty()
            && correlation.len() <= 256
            && !correlation.chars().any(char::is_control),
        "invalid SDK correlation"
    );
    let op = operation
        .model_operation
        .context("native model operation missing")?;
    let hash = format!(
        "{:x}",
        sha2::Sha256::digest(serde_json::to_vec(&(body, correlation, op))?)
    );
    let mut jobs = session
        .jobs
        .try_lock()
        .map_err(|_| anyhow::anyhow!("native model operation busy"))?;
    if let Some(job) = jobs.get(id) {
        anyhow::ensure!(job.hash == hash, "native model operation replay differs");
        return Ok(PreparedInvocation {
            session: Arc::clone(session),
            job: Arc::clone(job),
            id: id.to_owned(),
            body: Vec::new(),
            correlation: correlation.to_owned(),
            operation: NativeClaudeOperation::Messages,
            fresh: false,
        });
    }
    anyhow::ensure!(
        jobs.len() < MAX_OPERATIONS,
        "native model operation capacity reached"
    );
    session.controller.with_current(|_,core,_| {
        core.execute_batch(MODEL_SCHEMA)?;
        core.execute("INSERT INTO workjet_supervisor_native_model_requests
            (operation_id,execution_key,lease_hash,controller_id,sdk_correlation,body_hash,state,created_at_ms)
            VALUES (?1,?2,?3,?4,?5,?6,'accepted',?7)",
            params![id,session.controller.execution_key(),session.controller.lease.lease_hash,
                session.controller.controller_id(),correlation,hash,now_ms()])?;
        Ok(())
    })?;
    let job = Arc::new(ModelJob {
        hash,
        state: Mutex::new(ModelState::default()),
        task: Mutex::new(None),
    });
    jobs.insert(id.to_owned(), Arc::clone(&job));
    Ok(PreparedInvocation {
        session: Arc::clone(session),
        job,
        id: id.to_owned(),
        body: body.as_bytes().to_vec(),
        correlation: correlation.to_owned(),
        operation: match op {
            wire::SourceModelOperation::Messages => NativeClaudeOperation::Messages,
            wire::SourceModelOperation::CountTokens => NativeClaudeOperation::CountTokens,
        },
        fresh: true,
    })
}
async fn run_invocation(prepared: PreparedInvocation) {
    let result = async {
        // Scoped model credential never becomes a Source DTO, env or Core row.
        let capability = prepared.session.proxy.with_scoped_capability(|_, token| {
            Ok(Zeroizing::new(token.expose_secret().to_owned()))
        })?;
        let mut reply = prepared
            .session
            .proxy
            .invoke(
                &capability,
                prepared.operation,
                prepared.body,
                &prepared.correlation,
            )
            .await?;
        if reply.is_streaming() {
            while reply
                .publish_next(|bytes, witness| prepared.job.push(bytes, witness, true))
                .await?
                .is_some()
            {}
        } else {
            reply.publish_buffered(|bytes, witness| prepared.job.push(bytes, witness, false))?;
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let (observation, response_model, conflicting) = {
        let mut state = prepared
            .job
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (
            state.witness.take(),
            state.response_model.model.clone(),
            state.response_model.conflicting,
        )
    };
    let upstream_ok = observation
        .as_ref()
        .is_some_and(|w| (200..300).contains(&w.status));
    // No nested store access inside Models' bounded extraction callback.
    let recorded = prepared.session.controller.with_current(|_, core, _| {
        let (model, request, status) = match observation {
            Some(w) => (Some(w.model), Some(w.request_id), Some(w.status)),
            None => (None, None, None),
        };
        let changed = core.execute(
            "UPDATE workjet_supervisor_native_model_requests SET
            state=?1,requested_model=?2,upstream_request_id=?3,http_status=?4,finished_at_ms=?5,response_model=?8
            WHERE operation_id=?6 AND controller_id=?7",
            params![
                if result.is_ok() && !conflicting && upstream_ok { "observed" } else { "failed" },
                model,
                request,
                status,
                now_ms(),
                prepared.id,
                prepared.session.controller.controller_id(),
                response_model
            ],
        )?;
        anyhow::ensure!(changed == 1, "native model observation row changed");
        Ok(())
    });
    let mut state = prepared
        .job
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.finished = true;
    state.failed =
        result.is_err() || recorded.is_err() || state.response_model.conflicting || !upstream_ok;
}
impl ModelJob {
    fn push(
        &self,
        bytes: &[u8],
        witness: &NativeClaudeModelExchange,
        streaming: bool,
    ) -> anyhow::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("native model receiver poisoned"))?;
        anyhow::ensure!(
            !state.finished && state.total.saturating_add(bytes.len()) <= MAX_REPLY,
            "native model receiver budget reached"
        );
        state.total += bytes.len();
        state
            .response_model
            .observe(bytes, streaming, witness.http_status);
        state.witness = Some(UpstreamObservation {
            model: witness.model.clone(),
            request_id: witness.client_request_id.clone(),
            status: witness.http_status,
        });
        for bytes in bytes.chunks(CHUNK) {
            let sequence = state.next;
            state.next += 1;
            state.frames.push_back(Frame {
                sequence,
                bytes: bytes.to_vec(),
                status: witness.http_status,
                streaming,
            });
        }
        Ok(())
    }
    fn read(&self, sequence: u64) -> anyhow::Result<Value> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("native model receiver poisoned"))?;
        if sequence > state.acknowledged {
            anyhow::ensure!(
                sequence == state.acknowledged + 1
                    && state
                        .frames
                        .front()
                        .is_some_and(|f| f.sequence == state.acknowledged),
                "native model acknowledgement is not sequential"
            );
            state.frames.pop_front();
            state.acknowledged = sequence;
        }
        anyhow::ensure!(
            sequence == state.acknowledged,
            "native model replay precedes its retained acknowledgement"
        );
        if let Some(frame) = state.frames.front() {
            anyhow::ensure!(
                frame.sequence == sequence,
                "native model frame sequence differs"
            );
            return Ok(
                json!({"version":1,"state":"model_chunk","sequence":sequence,
                "http_status":frame.status,"streaming":frame.streaming,
                "body_base64":STANDARD.encode(&frame.bytes),"done":false,"execution_ready":false}),
            );
        }
        Ok(
            json!({"version":1,"state":if state.failed {"model_failed"} else if state.finished {"model_done"} else {"model_pending"},
            "sequence":sequence,"done":state.finished,"execution_ready":false}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn job() -> ModelJob {
        ModelJob {
            hash: "request".into(),
            state: Mutex::new(ModelState::default()),
            task: Mutex::new(None),
        }
    }

    #[test]
    fn native_response_model_is_observed_from_header_not_requested_or_delta_text() {
        let mut seen = ResponseModelObservation::default();
        seen.observe(b"data: {\"type\":\"message_start\",\"message\":{\"type\":\"message\",\"model\":\"claude-opus-", true, 200);
        assert!(seen.model.is_none());
        seen.observe(b"5-5\"}}\n", true, 200);
        assert_eq!(seen.model.as_deref(), Some("claude-opus-5-5"));
        let mut missing = ResponseModelObservation::default();
        missing.observe(
            b"data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"model\"}}\n",
            true,
            200,
        );
        assert!(missing.model.is_none());
        missing.observe(
            b"{\"type\":\"message\",\"model\":\"claude-opus-5-5\"}",
            false,
            401,
        );
        assert!(missing.model.is_none());
        missing.observe(
            b"{\"type\":\"message\",\"model\":\"claude-opus-5-5\"}",
            false,
            200,
        );
        assert_eq!(missing.model.as_deref(), Some("claude-opus-5-5"));
    }
    #[test]
    fn oversized_sse_line_does_not_reinterpret_its_tail_as_a_message_header() {
        let mut seen = ResponseModelObservation::default();
        seen.observe(&vec![b'x'; 65537], true, 200);
        seen.observe(b"data: {\"type\":\"message_start\",\"message\":{\"type\":\"message\",\"model\":\"claude-opus-5-5\"}}\n", true, 200);
        assert!(seen.model.is_none());
        seen.observe(b"data: {\"type\":\"message_start\",\"message\":{\"type\":\"message\",\"model\":\"claude-opus-5-5\"}}\n", true, 200);
        assert_eq!(seen.model.as_deref(), Some("claude-opus-5-5"));
    }

    // Buffered queue fixtures only; genuine HTTP witnesses are not fabricated.
    fn frame(job: &ModelJob, bytes: &[u8], status: u16) {
        let mut state = job.state.lock().unwrap();
        let sequence = state.next;
        state.next += 1;
        state.frames.push_back(Frame {
            sequence,
            bytes: bytes.to_vec(),
            status,
            streaming: true,
        });
    }
    #[test]
    fn model_frames_replay_exactly_once_until_sequential_acknowledgement() -> anyhow::Result<()> {
        let job = job();
        frame(&job, b"first", 200);
        frame(&job, b"second", 200);
        assert_eq!(job.read(0)?, job.read(0)?);
        assert_eq!(job.read(0)?["body_base64"], STANDARD.encode(b"first"));
        assert!(job.read(2).is_err());
        assert_eq!(job.read(1)?["body_base64"], STANDARD.encode(b"second"));
        assert!(job.read(0).is_err());
        job.state.lock().unwrap().finished = true;
        let terminal = job.read(2)?;
        assert_eq!(terminal["state"], "model_done");
        assert_eq!(terminal["execution_ready"], false);
        assert_eq!(terminal, job.read(2)?);
        Ok(())
    }
    #[test]
    fn real_upstream_status_is_preserved_and_pending_is_not_execution() -> anyhow::Result<()> {
        let job = job();
        let pending = job.read(0)?;
        assert_eq!(pending["state"], "model_pending");
        assert_eq!(pending["done"], false);
        frame(&job, b"account rejected", 401);
        assert_eq!(job.read(0)?["http_status"], 401);
        job.state.lock().unwrap().failed = true;
        job.state.lock().unwrap().finished = true;
        assert_eq!(job.read(1)?["state"], "model_failed");
        Ok(())
    }
    #[test]
    fn model_operations_cannot_claim_execution_or_modify_control_authority() -> anyhow::Result<()> {
        let offer = uuid::Uuid::new_v4().to_string();
        let controller = uuid::Uuid::new_v4().to_string();
        let operation = uuid::Uuid::new_v4().to_string();
        let read = json!({"version":1,"action":"model_read","offer_id":offer,"controller_id":controller,
            "operation_id":operation,"sequence":0});
        parse_operation(vec![read.clone()])?;
        for field in ["actual", "account", "credential", "owner", "url"] {
            let mut bad = read.clone();
            bad[field] = json!("caller");
            assert!(parse_operation(vec![bad]).is_err());
        }
        assert!(parse_operation(vec![
            json!({"version":1,"action":"poll","operation_id":operation})
        ])
        .is_err());
        assert!(parse_operation(vec![
            json!({"version":1,"action":"model_read","offer_id":offer,
            "controller_id":controller,"operation_id":operation})
        ])
        .is_err());
        Ok(())
    }
}
