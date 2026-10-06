// ref: sdk/cliproxy/auth/conductor_home.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// Port-Note: instance-bound Home client/registry bundle replaces global dispatcher authority
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, RwLock};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::internal::home::{Client, HomeError};
use crate::sdk::cliproxy::executionregistry::{Registry, RegistryError, ScopeSpec};

use super::{
    decode_home_concurrency, decode_home_dispatch_error, install_home_concurrency_scope,
    verify_home_concurrency_identity, Auth, AuthManager, HomeConcurrencyError,
    HomeDispatchSelection, HomeDispatchStatusError,
};

pub const CLOSE_ALL_EXECUTION_SESSIONS_ID: &str = "__all_execution_sessions__";

pub struct HomeDispatchBundle {
    pub client: Arc<Client>,
    pub registry: Arc<Registry>,
    pub generation: u64,
}

impl fmt::Debug for HomeDispatchBundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HomeDispatchBundle")
            .field("heartbeat_ok", &self.client.heartbeat_ok())
            .field("registry_state", &self.registry.state())
            .field("generation", &self.generation)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SessionSelectionKey {
    session_id: String,
    route_model: String,
}

/// Home control plane bound to exactly one auth manager instance.
pub struct HomeAuthRuntime {
    manager: Arc<AuthManager>,
    dispatch: RwLock<Option<Arc<HomeDispatchBundle>>>,
    sessions: Mutex<BTreeMap<SessionSelectionKey, Arc<HomeDispatchSelection>>>,
    clock: Arc<dyn HomeClock>,
    selected_auth: RwLock<Option<Arc<dyn HomeSelectedAuthPublisher>>>,
    pub(super) usage: RwLock<Option<Arc<crate::sdk::cliproxy::usage::Manager>>>,
}

impl fmt::Debug for HomeAuthRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HomeAuthRuntime")
            .field("home_enabled", &self.home_enabled())
            .field(
                "retained_sessions",
                &self
                    .sessions
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .len(),
            )
            .finish_non_exhaustive()
    }
}

impl HomeAuthRuntime {
    pub fn new(manager: Arc<AuthManager>) -> Self {
        Self::new_with_clock(manager, Arc::new(SystemHomeClock))
    }

    pub fn new_with_clock(manager: Arc<AuthManager>, clock: Arc<dyn HomeClock>) -> Self {
        Self {
            manager,
            dispatch: RwLock::new(None),
            sessions: Mutex::new(BTreeMap::new()),
            clock,
            selected_auth: RwLock::new(None),
            usage: RwLock::new(None),
        }
    }

    pub fn set_usage_manager(&self, manager: Option<Arc<crate::sdk::cliproxy::usage::Manager>>) {
        *self
            .usage
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = manager;
    }

    pub fn set_selected_auth_publisher(
        &self,
        publisher: Option<Arc<dyn HomeSelectedAuthPublisher>>,
    ) {
        *self
            .selected_auth
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = publisher;
    }

    pub(super) fn publish_selected_auth(&self, auth: &Auth) {
        if let Some(publisher) = self
            .selected_auth
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            publisher.selected(&auth.id, &auth.index);
        }
    }

    pub fn manager(&self) -> Arc<AuthManager> {
        self.manager.clone()
    }

    pub fn publish_dispatch(
        &self,
        client: Arc<Client>,
        registry: Arc<Registry>,
        generation: u64,
    ) -> Arc<HomeDispatchBundle> {
        let bundle = Arc::new(HomeDispatchBundle {
            client,
            registry,
            generation,
        });
        *self
            .dispatch
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(bundle.clone());
        bundle
    }

    pub fn clear_dispatch(&self, expected: &Arc<HomeDispatchBundle>) -> bool {
        let mut current = self
            .dispatch
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if current
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, expected))
        {
            *current = None;
            true
        } else {
            false
        }
    }

    pub fn dispatch_bundle(&self) -> Option<Arc<HomeDispatchBundle>> {
        self.dispatch
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn home_enabled(&self) -> bool {
        self.dispatch_bundle().is_some()
    }

    pub fn local_fallback_auth(&self, auth_id: &str) -> Option<Auth> {
        if self.home_enabled() {
            return None;
        }
        self.manager.lifecycle().get_cached(auth_id.trim())
    }

    pub fn retained_selection(
        &self,
        session_id: &str,
        route_model: &str,
    ) -> Option<Arc<HomeDispatchSelection>> {
        let key = session_key(session_id, route_model)?;
        let selection = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
            .cloned()?;
        selection.active().then_some(selection)
    }

    pub fn retain_selection(
        &self,
        session_id: &str,
        route_model: &str,
        selection: Arc<HomeDispatchSelection>,
    ) -> bool {
        let Some(key) = session_key(session_id, route_model) else {
            return false;
        };
        selection.retain();
        let replaced = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, selection.clone());
        if let Some(replaced) = replaced {
            if !Arc::ptr_eq(&replaced, &selection) {
                replaced.end("session_selection_replaced");
            }
        }
        true
    }

    pub fn close_execution_session(&self, session_id: &str) -> usize {
        let session_id = session_id.trim();
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let keys = sessions
            .keys()
            .filter(|key| {
                session_id == CLOSE_ALL_EXECUTION_SESSIONS_ID || key.session_id == session_id
            })
            .cloned()
            .collect::<Vec<_>>();
        let removed = keys
            .into_iter()
            .filter_map(|key| sessions.remove(&key))
            .collect::<Vec<_>>();
        drop(sessions);
        for selection in &removed {
            selection.end("execution_session_closed");
        }
        removed.len()
    }

    pub fn pick_selection(
        &self,
        request: HomeSelectionRequest,
    ) -> Result<Arc<HomeDispatchSelection>, HomeDispatchError> {
        if let Some(selection) = self.retained_selection(&request.session_id, &request.model) {
            return Ok(selection);
        }
        let bundle = self
            .dispatch_bundle()
            .ok_or(HomeDispatchError::Unavailable)?;
        if !bundle.client.heartbeat_ok() {
            return Err(HomeDispatchError::Unavailable);
        }
        let pending = bundle
            .registry
            .begin_dispatch()
            .map_err(HomeDispatchError::Registry)?;
        let raw = match bundle.client.rpop_auth(
            &request.model,
            &request.session_id,
            request.headers,
            request.count,
            &request.credential_policy,
        ) {
            Ok(raw) => raw,
            Err(error) => {
                pending.end();
                return Err(HomeDispatchError::Transport(classify_home_error(error)));
            }
        };
        if !self
            .dispatch_bundle()
            .is_some_and(|active| Arc::ptr_eq(&active, &bundle))
        {
            pending.end();
            return Err(HomeDispatchError::DetachedLifetime);
        }
        let tuple = decode_home_concurrency(&raw).map_err(|error| {
            pending.end();
            HomeDispatchError::Concurrency(error)
        })?;
        let response: HomeDispatchResponse = serde_json::from_slice(&raw).map_err(|_| {
            pending.end();
            HomeDispatchError::InvalidResponse
        })?;
        if response.error.is_some() {
            pending.end();
            return Err(decode_home_dispatch_error(&raw)
                .map(HomeDispatchError::Status)
                .unwrap_or(HomeDispatchError::Rejected));
        }
        let mut auth = response.auth;
        if auth.id.trim().is_empty() {
            auth.id.clone_from(&response.auth_index);
        }
        if auth.index.trim().is_empty() {
            auth.index.clone_from(&response.auth_index);
        }
        let provider = if response.provider.trim().is_empty() {
            auth.provider.trim().to_ascii_lowercase()
        } else {
            response.provider.trim().to_ascii_lowercase()
        };
        if auth.provider.trim().is_empty() {
            auth.provider.clone_from(&provider);
        }
        // Routing values belong to central dispatch, never caller metadata.
        if !response.model.trim().is_empty() {
            auth.attributes.insert(
                "home_upstream_model".to_owned(),
                response.model.trim().to_owned(),
            );
        }
        if response.force_mapping {
            auth.attributes
                .insert("home_force_mapping".to_owned(), "true".to_owned());
        }
        if !response.original_alias.trim().is_empty() {
            auth.attributes.insert(
                "home_original_alias".to_owned(),
                response.original_alias.trim().to_owned(),
            );
        }
        verify_home_concurrency_identity(tuple.as_ref(), &auth.id, &response.auth_index).map_err(
            |error| {
                pending.end();
                HomeDispatchError::Concurrency(error)
            },
        )?;
        let registration = self.manager.executors().get(&provider).ok_or_else(|| {
            pending.end();
            HomeDispatchError::ExecutorNotFound
        })?;
        let executor = registration.execution().ok_or_else(|| {
            pending.end();
            HomeDispatchError::ExecutorNotFound
        })?;
        let scope = install_home_concurrency_scope(
            &bundle.registry,
            &pending,
            tuple.as_ref(),
            ScopeSpec {
                request_id: request.request_id,
                credential_id: auth.id.clone(),
                model: request.model,
                kind: request.kind,
                started_at: self.clock.now(),
                accounted: false,
            },
        )
        .map_err(HomeDispatchError::Concurrency)?;
        HomeDispatchSelection::new_with_model_info(
            auth,
            executor,
            registration.auth_preparer(),
            &provider,
            scope,
            response.model_info,
        )
        .map_err(HomeDispatchError::Registry)
    }
}

pub trait HomeClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Clone, Copy, Debug)]
pub struct SystemHomeClock;

impl HomeClock for SystemHomeClock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::<Utc>::from(std::time::SystemTime::now())
    }
}

pub trait HomeSelectedAuthPublisher: Send + Sync {
    fn selected(&self, auth_id: &str, auth_index: &str);
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HomeSelectionRequest {
    pub model: String,
    pub session_id: String,
    pub request_id: String,
    pub kind: String,
    pub headers: BTreeMap<String, String>,
    pub count: i32,
    pub credential_policy: String,
}

#[derive(Debug)]
pub enum HomeDispatchError {
    Unavailable,
    DetachedLifetime,
    InvalidResponse,
    Rejected,
    Status(HomeDispatchStatusError),
    ExecutorNotFound,
    Registry(RegistryError),
    Concurrency(HomeConcurrencyError),
    Transport(HomeTransportFailure),
}

impl fmt::Display for HomeDispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "Home dispatch is unavailable",
            Self::DetachedLifetime => "Home dispatch lifetime was replaced",
            Self::InvalidResponse => "Home returned an invalid auth response",
            Self::Rejected => "Home rejected auth dispatch",
            Self::Status(_) => "Home rejected auth dispatch with a typed status",
            Self::ExecutorNotFound => "Home provider executor is not registered",
            Self::Registry(_) => "Home execution registry is unavailable",
            Self::Concurrency(_) => "Home concurrency contract failed",
            Self::Transport(_) => "Home transport failed",
        })
    }
}

impl std::error::Error for HomeDispatchError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeTransportFailure {
    Ambiguous,
    Deterministic,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct HomeDispatchResponse {
    model: String,
    provider: String,
    auth_index: String,
    force_mapping: bool,
    original_alias: String,
    model_info: Option<super::home_model_capabilities::HomeDispatchModelInfo>,
    auth: Auth,
    error: Option<serde_json::Value>,
}

fn classify_home_error(error: HomeError) -> HomeTransportFailure {
    if matches!(error, HomeError::AmbiguousDispatch(_)) {
        HomeTransportFailure::Ambiguous
    } else {
        HomeTransportFailure::Deterministic
    }
}

fn session_key(session_id: &str, route_model: &str) -> Option<SessionSelectionKey> {
    let session_id = session_id.trim();
    let route_model = route_model.trim().to_ascii_lowercase();
    (!session_id.is_empty() && !route_model.is_empty()).then(|| SessionSelectionKey {
        session_id: session_id.to_owned(),
        route_model,
    })
}
