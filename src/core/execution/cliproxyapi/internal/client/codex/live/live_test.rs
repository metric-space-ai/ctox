// ref: internal/client/codex/live/live_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::sdk::cliproxy::auth::Auth;
use crate::sdk::cliproxy::executor::Headers;

struct FlagResource(AtomicBool);

impl SessionResource for FlagResource {
    fn close(&self) {
        self.0.store(true, Ordering::Release);
    }
}

struct ClosedEndpoint {
    closed: AtomicBool,
}

impl WebSocketEndpoint for ClosedEndpoint {
    fn read(&self) -> Result<WebSocketFrame, WebSocketClose> {
        Err(WebSocketClose::Normal)
    }

    fn write(&self, _frame: WebSocketFrame) -> Result<(), WebSocketClose> {
        Ok(())
    }

    fn close(&self, _close: WebSocketClose) {
        self.closed.store(true, Ordering::Release);
    }
}

struct FixtureConnector {
    request: Mutex<Option<SidebandConnectRequest>>,
    endpoint: Arc<ClosedEndpoint>,
}

impl SidebandConnector for FixtureConnector {
    fn connect(&self, request: &SidebandConnectRequest) -> Result<SidebandHandshake, LiveError> {
        *self.request.lock().unwrap() = Some(request.clone());
        Ok(SidebandHandshake {
            endpoint: self.endpoint.clone(),
            subprotocol: Some("realtime".to_owned()),
        })
    }
}

struct UnauthorizedThenConnected {
    requests: Mutex<Vec<SidebandConnectRequest>>,
    endpoint: Arc<ClosedEndpoint>,
}

impl SidebandConnector for UnauthorizedThenConnected {
    fn connect(&self, request: &SidebandConnectRequest) -> Result<SidebandHandshake, LiveError> {
        self.requests.lock().unwrap().push(request.clone());
        if request.auth_id != "fresh" {
            return Err(LiveError::new(LiveErrorKind::Unauthorized, "unauthorized"));
        }
        Ok(SidebandHandshake {
            endpoint: self.endpoint.clone(),
            subprotocol: None,
        })
    }
}

struct FixtureTransport {
    seen: Mutex<Option<LiveHttpRequest>>,
}

struct UnauthorizedThenSuccessTransport {
    requests: Mutex<Vec<LiveHttpRequest>>,
}

struct AlwaysUnauthorizedTransport;

impl LiveTransport for AlwaysUnauthorizedTransport {
    fn execute<'a>(&'a self, _: LiveHttpRequest) -> LiveTransportFuture<'a> {
        Box::pin(async {
            Ok(LiveHttpResponse {
                status: 401,
                headers: Headers::new(),
                body: Vec::new(),
            })
        })
    }
}

impl LiveTransport for UnauthorizedThenSuccessTransport {
    fn execute<'a>(&'a self, request: LiveHttpRequest) -> LiveTransportFuture<'a> {
        let status = if request.auth_id == "fresh" { 201 } else { 401 };
        self.requests.lock().unwrap().push(request);
        Box::pin(async move {
            Ok(LiveHttpResponse {
                status,
                headers: Headers::from_iter([(
                    "Location".to_owned(),
                    vec!["call_refresh".to_owned()],
                )]),
                body: Vec::new(),
            })
        })
    }
}

struct FixtureRefresher {
    calls: Mutex<Vec<(String, String, bool)>>,
}

impl LiveCredentialRefresher for FixtureRefresher {
    fn report_unauthorized(&self, auth_id: &str, model: &str) {
        self.calls
            .lock()
            .unwrap()
            .push((auth_id.to_owned(), model.to_owned(), false));
    }

    fn refresh_after_unauthorized(
        &self,
        auth_id: &str,
        model: &str,
        current: Option<&Auth>,
    ) -> Result<Auth, LiveError> {
        self.calls
            .lock()
            .unwrap()
            .push((auth_id.to_owned(), model.to_owned(), current.is_some()));
        let mut auth = current.cloned().unwrap_or_default();
        auth.id = "fresh".to_owned();
        Ok(auth)
    }
}

struct FailingRefresher {
    reports: Mutex<Vec<String>>,
}

impl LiveCredentialRefresher for FailingRefresher {
    fn report_unauthorized(&self, auth_id: &str, _: &str) {
        self.reports.lock().unwrap().push(auth_id.to_owned());
    }

    fn refresh_after_unauthorized(
        &self,
        _: &str,
        _: &str,
        _: Option<&Auth>,
    ) -> Result<Auth, LiveError> {
        Err(LiveError::new(LiveErrorKind::Unavailable, "refresh failed"))
    }
}

struct TrackingMediaSession {
    closed: Mutex<Vec<String>>,
}

impl MediaRelaySession for TrackingMediaSession {
    fn accept_upstream_answer<'a>(&'a self, answer: &'a str) -> MediaFuture<'a, String> {
        Box::pin(async move { Ok(answer.to_owned()) })
    }

    fn set_call_id(&self, _: &str) {}

    fn set_close_handler(&self, _: Arc<dyn Fn(&str) + Send + Sync>) {}

    fn close(&self, reason: &str) {
        self.closed.lock().unwrap().push(reason.to_owned());
    }
}

struct TrackingMediaRelay(Arc<TrackingMediaSession>);

impl MediaRelay for TrackingMediaRelay {
    fn new_session<'a>(
        &'a self,
        _: &'a str,
        _: &'a MediaRoute,
    ) -> MediaFuture<'a, (Arc<dyn MediaRelaySession>, String)> {
        let session: Arc<dyn MediaRelaySession> = self.0.clone();
        Box::pin(async move { Ok((session, "v=upstream-offer".to_owned())) })
    }
}

impl LiveTransport for FixtureTransport {
    fn execute<'a>(&'a self, request: LiveHttpRequest) -> LiveTransportFuture<'a> {
        *self.seen.lock().unwrap() = Some(request);
        Box::pin(async {
            Ok(LiveHttpResponse {
                status: 201,
                headers: Headers::from_iter([("Location".to_owned(), vec!["call_9".to_owned()])]),
                body: b"v=answer".to_vec(),
            })
        })
    }
}
