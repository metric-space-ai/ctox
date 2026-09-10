// Origin: CTOX
// License: AGPL-3.0-only
//
// Native in-process adapter for binding a CTOX worker to a harness thread.
// Persistent workers reuse one named durable thread. Lookup, resume, and
// rejected turn/start failures return an actionable error instead of creating
// a replacement thread or resubmitting the turn. Isolated and first-time
// sessions still start a new thread.

use anyhow::Result;
use ctox_app_server_client::{InProcessAppServerClient, TypedRequestError};
use ctox_app_server_protocol::{
    ClientRequest, RequestId, ThreadListParams, ThreadListResponse, ThreadResumeParams,
    ThreadResumeResponse, ThreadSetNameParams, ThreadSetNameResponse, ThreadSortKey,
    ThreadSourceKind, ThreadStartParams, ThreadStartResponse, TurnStartParams, TurnStartResponse,
};
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::time::Duration;

const THREAD_LIST_PAGE_SIZE: u32 = 20;
const THREAD_LIST_MAX_PAGES: u32 = 50;

/// Marker error for ambiguous turn outcomes that must poison the session.
/// Carried through anyhow so `run_turn_inner_with_context` can flip the
/// session's `poisoned` flag on the way out.
#[derive(Debug)]
pub(crate) struct SessionPoisoned(pub(crate) String);

impl std::fmt::Display for SessionPoisoned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SessionPoisoned {}

/// Fail-closed error for a persistent worker that already has (or should keep)
/// a durable harness thread. Callers must not start a replacement thread.
#[derive(Debug)]
pub(crate) struct PersistentThreadContinuityError {
    pub(crate) kind: PersistentThreadContinuityKind,
    message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PersistentThreadContinuityKind {
    LookupFailed,
    ResumeFailed,
    ThreadIdMismatch,
    TurnStartRejected,
}

impl PersistentThreadContinuityError {
    fn lookup(detail: impl std::fmt::Display) -> Self {
        Self {
            kind: PersistentThreadContinuityKind::LookupFailed,
            message: format!(
                "persistent worker thread lookup failed; refusing to start a replacement thread: {detail}"
            ),
        }
    }

    fn resume(thread_id: &str, detail: impl std::fmt::Display) -> Self {
        Self {
            kind: PersistentThreadContinuityKind::ResumeFailed,
            message: format!(
                "thread/resume failed for {thread_id}; refusing to start a replacement thread: {detail}"
            ),
        }
    }

    fn mismatch(requested: &str, resumed: &str) -> Self {
        Self {
            kind: PersistentThreadContinuityKind::ThreadIdMismatch,
            message: format!(
                "thread/resume returned id {resumed} instead of identified thread {requested}; refusing to start a replacement thread"
            ),
        }
    }

    fn turn_start_rejected(thread_id: &str, detail: impl std::fmt::Display) -> Self {
        Self {
            kind: PersistentThreadContinuityKind::TurnStartRejected,
            message: format!(
                "turn/start on persistent thread {thread_id} was rejected; refusing to rotate or resubmit: {detail}"
            ),
        }
    }
}

impl std::fmt::Display for PersistentThreadContinuityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PersistentThreadContinuityError {}

pub(crate) struct RequestIdSeq {
    next: i64,
}

impl RequestIdSeq {
    pub(crate) fn new() -> Self {
        Self { next: 1 }
    }

    pub(crate) fn next(&mut self) -> RequestId {
        let id = self.next;
        self.next += 1;
        RequestId::Integer(id)
    }
}

pub(crate) struct SessionThreadSpec<'a> {
    pub model: &'a str,
    pub model_provider: Option<&'a str>,
    pub cwd: &'a Path,
    pub base_instructions: &'a str,
    pub disable_active_tools: bool,
    pub disable_mcp_servers: bool,
    pub thread_config: Option<&'a HashMap<String, JsonValue>>,
    pub persistent_worker: bool,
    pub persistent_thread_name: Option<&'a str>,
}

pub(crate) struct SessionControlTimeouts {
    pub list: Duration,
    pub resume: Duration,
    pub start: Duration,
    pub turn_start: Duration,
}

pub(crate) trait DirectSessionControlClient {
    fn request_typed<T>(
        &self,
        request: ClientRequest,
    ) -> impl Future<Output = Result<T, TypedRequestError>> + Send
    where
        T: DeserializeOwned + Send;
}

impl DirectSessionControlClient for InProcessAppServerClient {
    fn request_typed<T>(
        &self,
        request: ClientRequest,
    ) -> impl Future<Output = Result<T, TypedRequestError>> + Send
    where
        T: DeserializeOwned + Send,
    {
        InProcessAppServerClient::request_typed(self, request)
    }
}

pub(crate) async fn bind_session_thread<C: DirectSessionControlClient>(
    client: &C,
    seq: &mut RequestIdSeq,
    spec: &SessionThreadSpec<'_>,
    timeouts: &SessionControlTimeouts,
) -> Result<String> {
    if spec.persistent_worker {
        let persistent_thread_name = spec.persistent_thread_name.ok_or_else(|| {
            anyhow::anyhow!("persistent worker session is missing its durable thread name")
        })?;
        match find_named_persistent_thread(client, seq, persistent_thread_name, timeouts.list).await
        {
            Ok(Some(thread_id)) => {
                return resume_identified_thread(client, seq, spec, &thread_id, timeouts.resume)
                    .await;
            }
            Ok(None) => {}
            Err(err) => return Err(err),
        }
    }

    start_session_thread(client, seq, spec, timeouts.start).await
}

async fn find_named_persistent_thread<C: DirectSessionControlClient>(
    client: &C,
    seq: &mut RequestIdSeq,
    persistent_thread_name: &str,
    timeout: Duration,
) -> Result<Option<String>> {
    let mut cursor = None;
    for _ in 0..THREAD_LIST_MAX_PAGES {
        let list_fut = client.request_typed::<ThreadListResponse>(ClientRequest::ThreadList {
            request_id: seq.next(),
            params: ThreadListParams {
                cursor,
                limit: Some(THREAD_LIST_PAGE_SIZE),
                sort_key: Some(ThreadSortKey::UpdatedAt),
                // Empty provider list means every provider. The default
                // `None` would keep only the current provider and miss the
                // named durable thread after a model/provider contract change.
                model_providers: Some(Vec::new()),
                source_kinds: Some(vec![ThreadSourceKind::Exec]),
                archived: Some(false),
                cwd: None,
                // Match the assigned thread name, not the extracted title.
                search_term: None,
            },
        });
        let response = match tokio::time::timeout(timeout, list_fut).await {
            Ok(Ok(response)) => response,
            Ok(Err(err)) => {
                eprintln!(
                    "[ctox direct-session] persistent thread lookup failed; refusing replacement: {err}"
                );
                return Err(PersistentThreadContinuityError::lookup(err).into());
            }
            Err(_) => {
                eprintln!(
                    "[ctox direct-session] persistent thread lookup timed out; refusing replacement"
                );
                return Err(PersistentThreadContinuityError::lookup(format!(
                    "thread/list timed out after {}s",
                    timeout.as_secs().max(1)
                ))
                .into());
            }
        };
        if let Some(thread_id) = response.data.into_iter().find_map(|thread| {
            (!thread.ephemeral && thread.name.as_deref() == Some(persistent_thread_name))
                .then_some(thread.id)
        }) {
            return Ok(Some(thread_id));
        }
        match response.next_cursor {
            Some(next) => cursor = Some(next),
            None => return Ok(None),
        }
    }
    eprintln!(
        "[ctox direct-session] persistent thread lookup did not finish listing; refusing replacement"
    );
    Err(PersistentThreadContinuityError::lookup(
        "thread/list pagination exceeded the native adapter bound before confirming the named thread is absent",
    )
    .into())
}

async fn resume_identified_thread<C: DirectSessionControlClient>(
    client: &C,
    seq: &mut RequestIdSeq,
    spec: &SessionThreadSpec<'_>,
    thread_id: &str,
    timeout: Duration,
) -> Result<String> {
    let resume_fut = client.request_typed::<ThreadResumeResponse>(ClientRequest::ThreadResume {
        request_id: seq.next(),
        params: ThreadResumeParams {
            thread_id: thread_id.to_string(),
            history: None,
            path: None,
            model: Some(spec.model.to_string()),
            model_provider: spec.model_provider.map(str::to_string),
            service_tier: None,
            cwd: Some(spec.cwd.to_string_lossy().to_string()),
            approval_policy: Some(ctox_protocol::protocol::AskForApproval::Never.into()),
            approvals_reviewer: None,
            sandbox: Some(ctox_app_server_protocol::SandboxMode::WorkspaceWrite),
            config: None,
            base_instructions: Some(spec.base_instructions.to_string()),
            developer_instructions: None,
            personality: None,
            persist_extended_history: true,
        },
    });
    match tokio::time::timeout(timeout, resume_fut).await {
        Ok(Ok(response)) => {
            let resumed_id = response.thread.id;
            if resumed_id != thread_id {
                eprintln!(
                    "[ctox direct-session] thread/resume id mismatch for {thread_id} -> {resumed_id}; refusing replacement"
                );
                return Err(
                    PersistentThreadContinuityError::mismatch(thread_id, &resumed_id).into(),
                );
            }
            eprintln!("[ctox direct-session] thread resumed: {resumed_id}");
            Ok(resumed_id)
        }
        Ok(Err(err)) => {
            eprintln!(
                "[ctox direct-session] thread/resume failed for {thread_id}; refusing replacement: {err}"
            );
            Err(PersistentThreadContinuityError::resume(thread_id, err).into())
        }
        Err(_) => {
            eprintln!(
                "[ctox direct-session] thread/resume timed out for {thread_id}; refusing replacement"
            );
            Err(PersistentThreadContinuityError::resume(
                thread_id,
                format!(
                    "thread/resume timed out after {}s",
                    timeout.as_secs().max(1)
                ),
            )
            .into())
        }
    }
}

pub(crate) async fn start_session_thread<C: DirectSessionControlClient>(
    client: &C,
    seq: &mut RequestIdSeq,
    spec: &SessionThreadSpec<'_>,
    timeout: Duration,
) -> Result<String> {
    // Bound these control requests: a wedged response path must not hang the
    // worker indefinitely (ctox#21 P1 review). They register/name a thread
    // with no model side effects, so a timeout simply surfaces as an error
    // and the session is rebuilt.
    let thread_start_fut = client.request_typed(ClientRequest::ThreadStart {
        request_id: seq.next(),
        params: ThreadStartParams {
            model: Some(spec.model.to_string()),
            model_provider: spec.model_provider.map(str::to_string),
            cwd: Some(spec.cwd.to_string_lossy().to_string()),
            approval_policy: Some(ctox_protocol::protocol::AskForApproval::Never.into()),
            sandbox: Some(ctox_app_server_protocol::SandboxMode::WorkspaceWrite),
            config: spec.thread_config.cloned(),
            base_instructions: Some(spec.base_instructions.to_string()),
            dynamic_tools: spec.disable_active_tools.then(Vec::new),
            disable_mcp_servers: Some(spec.disable_mcp_servers),
            ephemeral: Some(!spec.persistent_worker),
            persist_extended_history: spec.persistent_worker,
            ..ThreadStartParams::default()
        },
    });
    let response: ThreadStartResponse = match tokio::time::timeout(timeout, thread_start_fut).await
    {
        Ok(result) => result.map_err(|err| anyhow::anyhow!("thread/start: {err}"))?,
        Err(_) => anyhow::bail!("thread/start timed out after {}s", timeout.as_secs().max(1)),
    };
    let thread_id = response.thread.id;
    if let Some(persistent_thread_name) = spec.persistent_thread_name {
        let set_name_fut =
            client.request_typed::<ThreadSetNameResponse>(ClientRequest::ThreadSetName {
                request_id: seq.next(),
                params: ThreadSetNameParams {
                    thread_id: thread_id.clone(),
                    name: persistent_thread_name.to_string(),
                },
            });
        match tokio::time::timeout(timeout, set_name_fut).await {
            Ok(result) => {
                result.map_err(|err| anyhow::anyhow!("thread/name/set: {err}"))?;
            }
            Err(_) => anyhow::bail!(
                "thread/name/set timed out after {}s",
                timeout.as_secs().max(1)
            ),
        }
    }
    eprintln!("[ctox direct-session] thread started: {thread_id}");
    Ok(thread_id)
}

pub(crate) async fn start_bound_turn<C, F>(
    client: &C,
    seq: &mut RequestIdSeq,
    session_thread_id: &mut String,
    params_for_thread: F,
    spec: &SessionThreadSpec<'_>,
    timeouts: &SessionControlTimeouts,
) -> Result<TurnStartResponse>
where
    C: DirectSessionControlClient,
    F: Fn(&str) -> TurnStartParams,
{
    let thread_id = session_thread_id.clone();
    // A turn/start TIMEOUT is ambiguous: the facade detaches the request
    // onto its own task, so timing out the caller-side future does NOT
    // cancel the server-side turn — it may still be starting. Rotating
    // the thread and re-submitting the same prompt on a timeout would
    // duplicate model/tool side effects (ctox#21 P1 review). So we only
    // rotate isolated sessions on a DEFINITIVE error response (the turn
    // provably did not start); a timeout poisons the session and bails
    // without retry. Persistent workers never rotate: a rejected start
    // must not create a replacement thread or resubmit the turn.
    match tokio::time::timeout(
        timeouts.turn_start,
        client.request_typed(ClientRequest::TurnStart {
            request_id: seq.next(),
            params: params_for_thread(&thread_id),
        }),
    )
    .await
    {
        Ok(Ok(resp)) => Ok(resp),
        Err(_) => Err(anyhow::Error::new(SessionPoisoned(format!(
            "turn/start timed out after {}s; the turn may have started server-side, so the session is poisoned instead of retried to avoid a duplicate turn",
            timeouts.turn_start.as_secs().max(1)
        )))),
        // Transport and decode failures are as ambiguous as a timeout:
        // the request may have reached the processor (transport) or the
        // response arrived but could not be decoded (deserialize) — in
        // both cases the turn may be running. Only a definitive server
        // rejection proves the turn did not start.
        Ok(Err(
            err @ (TypedRequestError::Transport { .. } | TypedRequestError::Deserialize { .. }),
        )) => Err(anyhow::Error::new(SessionPoisoned(format!(
            "turn/start ended ambiguously ({err}); session poisoned instead of retried to avoid a duplicate turn"
        )))),
        Ok(Err(err @ TypedRequestError::Server { .. })) if spec.persistent_worker => {
            eprintln!(
                "[ctox direct-session] turn/start on persistent thread {thread_id} was rejected ({err}); refusing replacement"
            );
            Err(PersistentThreadContinuityError::turn_start_rejected(&thread_id, err).into())
        }
        Ok(Err(err @ TypedRequestError::Server { .. })) => {
            eprintln!(
                "[ctox direct-session] turn/start on session thread {thread_id} was rejected by the server ({err}); rotating isolated thread"
            );
            let rotated_thread_id =
                start_session_thread(client, seq, spec, timeouts.start)
                    .await
                    .map_err(|err| anyhow::anyhow!("thread/start (rotation): {err}"))?;
            eprintln!("[ctox direct-session] rotated session thread: {rotated_thread_id}");
            *session_thread_id = rotated_thread_id.clone();
            // The rotation retry is likewise timeout-poisoned: a fresh
            // thread's turn/start that times out must not fan out again.
            match tokio::time::timeout(
                timeouts.turn_start,
                client.request_typed(ClientRequest::TurnStart {
                    request_id: seq.next(),
                    params: params_for_thread(&rotated_thread_id),
                }),
            )
            .await
            {
                Ok(Ok(resp)) => Ok(resp),
                Ok(Err(err @ TypedRequestError::Server { .. })) => {
                    Err(anyhow::anyhow!("turn/start: {err}"))
                }
                Ok(Err(err)) => Err(anyhow::Error::new(SessionPoisoned(format!(
                    "turn/start on rotated thread ended ambiguously ({err}); session poisoned"
                )))),
                Err(_) => Err(anyhow::Error::new(SessionPoisoned(format!(
                    "turn/start on rotated thread timed out after {}s; session poisoned instead of retried",
                    timeouts.turn_start.as_secs().max(1)
                )))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctox_app_server_protocol::{
        ApprovalsReviewer, AskForApproval, JSONRPCErrorError, SandboxPolicy, SessionSource, Thread,
        ThreadStatus, Turn, TurnStatus,
    };
    use serde::Serialize;
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::Mutex;

    #[derive(Clone)]
    enum ScriptedReply {
        Ok(JsonValue),
        Server(String),
        Transport(String),
        Never,
    }

    struct ScriptedControlClient {
        calls: Mutex<Vec<String>>,
        replies: Mutex<VecDeque<ScriptedReply>>,
    }

    impl ScriptedControlClient {
        fn new(replies: Vec<ScriptedReply>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                replies: Mutex::new(VecDeque::from(replies)),
            }
        }

        fn methods(&self) -> Vec<String> {
            self.calls.lock().expect("calls").clone()
        }
    }

    impl DirectSessionControlClient for ScriptedControlClient {
        fn request_typed<T>(
            &self,
            request: ClientRequest,
        ) -> impl Future<Output = Result<T, TypedRequestError>> + Send
        where
            T: DeserializeOwned + Send,
        {
            let method = request.method();
            self.calls.lock().expect("calls").push(method.clone());
            let reply = self.replies.lock().expect("replies").pop_front();
            async move {
                match reply {
                    Some(ScriptedReply::Ok(value)) => serde_json::from_value(value)
                        .map_err(|source| TypedRequestError::Deserialize { method, source }),
                    Some(ScriptedReply::Server(message)) => Err(TypedRequestError::Server {
                        method,
                        source: JSONRPCErrorError {
                            code: 1,
                            data: None,
                            message,
                        },
                    }),
                    Some(ScriptedReply::Transport(message)) => Err(TypedRequestError::Transport {
                        method,
                        source: std::io::Error::other(message),
                    }),
                    Some(ScriptedReply::Never) => {
                        std::future::pending::<Result<T, TypedRequestError>>().await
                    }
                    None => panic!("unexpected {method} with no scripted reply"),
                }
            }
        }
    }

    fn json(value: impl Serialize) -> ScriptedReply {
        ScriptedReply::Ok(serde_json::to_value(value).expect("scripted json"))
    }

    fn test_thread(id: &str, name: Option<&str>, ephemeral: bool) -> Thread {
        Thread {
            id: id.to_string(),
            preview: String::new(),
            ephemeral,
            model_provider: "openai".to_string(),
            created_at: 1,
            updated_at: 1,
            status: ThreadStatus::Idle,
            path: None,
            cwd: PathBuf::from("/tmp"),
            cli_version: "test".to_string(),
            source: SessionSource::Exec,
            agent_nickname: None,
            agent_role: None,
            git_info: None,
            name: name.map(str::to_string),
            turns: Vec::new(),
        }
    }

    fn thread_list(threads: Vec<Thread>, next_cursor: Option<&str>) -> ScriptedReply {
        json(ThreadListResponse {
            data: threads,
            next_cursor: next_cursor.map(str::to_string),
        })
    }

    fn resume_ok(id: &str, name: Option<&str>) -> ScriptedReply {
        json(ThreadResumeResponse {
            thread: test_thread(id, name, false),
            model: "gpt-5.4".to_string(),
            model_provider: "openai".to_string(),
            service_tier: None,
            cwd: PathBuf::from("/tmp"),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: ApprovalsReviewer::User,
            sandbox: SandboxPolicy::WorkspaceWrite {
                writable_roots: Vec::new(),
                read_only_access: Default::default(),
                network_access: false,
                exclude_tmpdir_env_var: false,
                exclude_slash_tmp: false,
            },
            reasoning_effort: None,
        })
    }

    fn start_ok(id: &str) -> ScriptedReply {
        json(ThreadStartResponse {
            thread: test_thread(id, None, true),
            model: "gpt-5.4".to_string(),
            model_provider: "openai".to_string(),
            service_tier: None,
            cwd: PathBuf::from("/tmp"),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: ApprovalsReviewer::User,
            sandbox: SandboxPolicy::WorkspaceWrite {
                writable_roots: Vec::new(),
                read_only_access: Default::default(),
                network_access: false,
                exclude_tmpdir_env_var: false,
                exclude_slash_tmp: false,
            },
            reasoning_effort: None,
        })
    }

    fn name_ok() -> ScriptedReply {
        json(ThreadSetNameResponse {})
    }

    fn turn_ok(id: &str) -> ScriptedReply {
        json(TurnStartResponse {
            turn: Turn {
                id: id.to_string(),
                items: Vec::new(),
                status: TurnStatus::InProgress,
                error: None,
            },
        })
    }

    fn persistent_spec<'a>(name: &'a str) -> SessionThreadSpec<'a> {
        SessionThreadSpec {
            model: "gpt-5.4",
            model_provider: Some("openai"),
            cwd: Path::new("/tmp"),
            base_instructions: "base",
            disable_active_tools: false,
            disable_mcp_servers: false,
            thread_config: None,
            persistent_worker: true,
            persistent_thread_name: Some(name),
        }
    }

    fn isolated_spec<'a>() -> SessionThreadSpec<'a> {
        SessionThreadSpec {
            model: "gpt-5.4",
            model_provider: Some("openai"),
            cwd: Path::new("/tmp"),
            base_instructions: "base",
            disable_active_tools: false,
            disable_mcp_servers: false,
            thread_config: None,
            persistent_worker: false,
            persistent_thread_name: None,
        }
    }

    fn fast_timeouts() -> SessionControlTimeouts {
        SessionControlTimeouts {
            list: Duration::from_millis(30),
            resume: Duration::from_millis(30),
            start: Duration::from_millis(30),
            turn_start: Duration::from_millis(30),
        }
    }

    fn turn_params(thread_id: &str) -> TurnStartParams {
        TurnStartParams {
            thread_id: thread_id.to_string(),
            ..TurnStartParams::default()
        }
    }

    fn continuity_kind(err: &anyhow::Error) -> PersistentThreadContinuityKind {
        err.downcast_ref::<PersistentThreadContinuityError>()
            .unwrap_or_else(|| panic!("expected continuity error, got {err:#}"))
            .kind
    }

    #[tokio::test]
    async fn successful_restart_resumes_identified_thread_without_thread_start() {
        let client = ScriptedControlClient::new(vec![
            thread_list(
                vec![test_thread("thr-keep", Some("ctox-service-worker"), false)],
                None,
            ),
            resume_ok("thr-keep", Some("ctox-service-worker")),
        ]);
        let mut seq = RequestIdSeq::new();
        let spec = persistent_spec("ctox-service-worker");
        let thread_id = bind_session_thread(&client, &mut seq, &spec, &fast_timeouts())
            .await
            .expect("resume existing thread");
        assert_eq!(thread_id, "thr-keep");
        assert_eq!(
            client.methods(),
            vec!["thread/list".to_string(), "thread/resume".to_string()]
        );
    }

    #[tokio::test]
    async fn successful_restart_finds_named_thread_on_later_list_page() {
        let client = ScriptedControlClient::new(vec![
            thread_list(
                vec![test_thread("thr-other", Some("other-worker"), false)],
                Some("page-2"),
            ),
            thread_list(
                vec![test_thread("thr-keep", Some("ctox-service-worker"), false)],
                None,
            ),
            resume_ok("thr-keep", Some("ctox-service-worker")),
        ]);
        let mut seq = RequestIdSeq::new();
        let spec = persistent_spec("ctox-service-worker");
        let thread_id = bind_session_thread(&client, &mut seq, &spec, &fast_timeouts())
            .await
            .expect("resume existing thread from later page");
        assert_eq!(thread_id, "thr-keep");
        assert_eq!(
            client.methods(),
            vec![
                "thread/list".to_string(),
                "thread/list".to_string(),
                "thread/resume".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn lookup_failure_issues_zero_thread_start_requests() {
        let client = ScriptedControlClient::new(vec![ScriptedReply::Server(
            "thread store unavailable".to_string(),
        )]);
        let mut seq = RequestIdSeq::new();
        let spec = persistent_spec("ctox-service-worker");
        let err = bind_session_thread(&client, &mut seq, &spec, &fast_timeouts())
            .await
            .expect_err("lookup must fail closed");
        assert_eq!(
            continuity_kind(&err),
            PersistentThreadContinuityKind::LookupFailed
        );
        assert_eq!(client.methods(), vec!["thread/list".to_string()]);
    }

    #[tokio::test]
    async fn lookup_timeout_issues_zero_thread_start_requests() {
        let client = ScriptedControlClient::new(vec![ScriptedReply::Never]);
        let mut seq = RequestIdSeq::new();
        let spec = persistent_spec("ctox-service-worker");
        let err = bind_session_thread(&client, &mut seq, &spec, &fast_timeouts())
            .await
            .expect_err("lookup timeout must fail closed");
        assert_eq!(
            continuity_kind(&err),
            PersistentThreadContinuityKind::LookupFailed
        );
        assert_eq!(client.methods(), vec!["thread/list".to_string()]);
    }

    #[tokio::test]
    async fn incomplete_listing_fails_closed_without_thread_start() {
        let replies = (0..THREAD_LIST_MAX_PAGES)
            .map(|i| {
                thread_list(
                    vec![test_thread(
                        &format!("thr-{i}"),
                        Some("other-worker"),
                        false,
                    )],
                    Some("next"),
                )
            })
            .collect();
        let client = ScriptedControlClient::new(replies);
        let mut seq = RequestIdSeq::new();
        let spec = persistent_spec("ctox-service-worker");
        let err = bind_session_thread(&client, &mut seq, &spec, &fast_timeouts())
            .await
            .expect_err("incomplete listing must fail closed");
        assert_eq!(
            continuity_kind(&err),
            PersistentThreadContinuityKind::LookupFailed
        );
        assert_eq!(
            client.methods(),
            vec!["thread/list".to_string(); THREAD_LIST_MAX_PAGES as usize]
        );
    }

    #[tokio::test]
    async fn identified_thread_resume_failure_issues_zero_thread_start_requests() {
        let client = ScriptedControlClient::new(vec![
            thread_list(
                vec![test_thread("thr-keep", Some("ctox-service-worker"), false)],
                None,
            ),
            ScriptedReply::Server("rollout missing".to_string()),
        ]);
        let mut seq = RequestIdSeq::new();
        let spec = persistent_spec("ctox-service-worker");
        let err = bind_session_thread(&client, &mut seq, &spec, &fast_timeouts())
            .await
            .expect_err("resume must fail closed");
        assert_eq!(
            continuity_kind(&err),
            PersistentThreadContinuityKind::ResumeFailed
        );
        assert_eq!(
            client.methods(),
            vec!["thread/list".to_string(), "thread/resume".to_string()]
        );
    }

    #[tokio::test]
    async fn resume_id_mismatch_issues_zero_thread_start_requests() {
        let client = ScriptedControlClient::new(vec![
            thread_list(
                vec![test_thread("thr-keep", Some("ctox-service-worker"), false)],
                None,
            ),
            resume_ok("thr-other", Some("ctox-service-worker")),
        ]);
        let mut seq = RequestIdSeq::new();
        let spec = persistent_spec("ctox-service-worker");
        let err = bind_session_thread(&client, &mut seq, &spec, &fast_timeouts())
            .await
            .expect_err("id mismatch must fail closed");
        assert_eq!(
            continuity_kind(&err),
            PersistentThreadContinuityKind::ThreadIdMismatch
        );
        assert_eq!(
            client.methods(),
            vec!["thread/list".to_string(), "thread/resume".to_string()]
        );
    }

    #[tokio::test]
    async fn genuine_new_persistent_session_still_starts_and_names_a_thread() {
        let client = ScriptedControlClient::new(vec![
            thread_list(Vec::new(), None),
            start_ok("thr-new"),
            name_ok(),
        ]);
        let mut seq = RequestIdSeq::new();
        let spec = persistent_spec("ctox-service-worker");
        let thread_id = bind_session_thread(&client, &mut seq, &spec, &fast_timeouts())
            .await
            .expect("first-time create");
        assert_eq!(thread_id, "thr-new");
        assert_eq!(
            client.methods(),
            vec![
                "thread/list".to_string(),
                "thread/start".to_string(),
                "thread/name/set".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn isolated_session_starts_without_lookup_or_resume() {
        let client = ScriptedControlClient::new(vec![start_ok("thr-iso")]);
        let mut seq = RequestIdSeq::new();
        let spec = isolated_spec();
        let thread_id = bind_session_thread(&client, &mut seq, &spec, &fast_timeouts())
            .await
            .expect("isolated create");
        assert_eq!(thread_id, "thr-iso");
        assert_eq!(client.methods(), vec!["thread/start".to_string()]);
    }

    #[tokio::test]
    async fn rejected_persistent_turn_start_does_not_rotate_or_resubmit() {
        let client = ScriptedControlClient::new(vec![ScriptedReply::Server(
            "thread not accepting turns".to_string(),
        )]);
        let mut seq = RequestIdSeq::new();
        let spec = persistent_spec("ctox-service-worker");
        let mut thread_id = "thr-keep".to_string();
        let err = start_bound_turn(
            &client,
            &mut seq,
            &mut thread_id,
            turn_params,
            &spec,
            &fast_timeouts(),
        )
        .await
        .expect_err("persistent turn/start must fail closed");
        assert_eq!(
            continuity_kind(&err),
            PersistentThreadContinuityKind::TurnStartRejected
        );
        assert_eq!(thread_id, "thr-keep");
        assert_eq!(client.methods(), vec!["turn/start".to_string()]);
    }

    #[tokio::test]
    async fn isolated_turn_start_rejection_still_rotates_once() {
        let client = ScriptedControlClient::new(vec![
            ScriptedReply::Server("thread gone".to_string()),
            start_ok("thr-rotated"),
            turn_ok("turn-2"),
        ]);
        let mut seq = RequestIdSeq::new();
        let spec = isolated_spec();
        let mut thread_id = "thr-old".to_string();
        let response = start_bound_turn(
            &client,
            &mut seq,
            &mut thread_id,
            turn_params,
            &spec,
            &fast_timeouts(),
        )
        .await
        .expect("isolated rotation");
        assert_eq!(thread_id, "thr-rotated");
        assert_eq!(response.turn.id, "turn-2");
        assert_eq!(
            client.methods(),
            vec![
                "turn/start".to_string(),
                "thread/start".to_string(),
                "turn/start".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn ambiguous_turn_start_transport_poisons_without_replacement() {
        let client = ScriptedControlClient::new(vec![ScriptedReply::Transport(
            "connection reset".to_string(),
        )]);
        let mut seq = RequestIdSeq::new();
        let spec = persistent_spec("ctox-service-worker");
        let mut thread_id = "thr-keep".to_string();
        let err = start_bound_turn(
            &client,
            &mut seq,
            &mut thread_id,
            turn_params,
            &spec,
            &fast_timeouts(),
        )
        .await
        .expect_err("ambiguous transport must poison");
        assert!(err.downcast_ref::<SessionPoisoned>().is_some(), "{err:#}");
        assert_eq!(thread_id, "thr-keep");
        assert_eq!(client.methods(), vec!["turn/start".to_string()]);
    }

    #[tokio::test]
    async fn ambiguous_turn_start_timeout_poisons_without_replacement() {
        let client = ScriptedControlClient::new(vec![ScriptedReply::Never]);
        let mut seq = RequestIdSeq::new();
        let spec = persistent_spec("ctox-service-worker");
        let mut thread_id = "thr-keep".to_string();
        let err = start_bound_turn(
            &client,
            &mut seq,
            &mut thread_id,
            turn_params,
            &spec,
            &fast_timeouts(),
        )
        .await
        .expect_err("timeout must poison");
        assert!(err.downcast_ref::<SessionPoisoned>().is_some(), "{err:#}");
        assert_eq!(thread_id, "thr-keep");
        assert_eq!(client.methods(), vec!["turn/start".to_string()]);
    }
}
