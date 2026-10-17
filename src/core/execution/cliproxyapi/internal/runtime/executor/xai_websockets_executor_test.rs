// ref: internal/runtime/executor/xai_websockets_executor_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use super::xai_websockets_executor::*;
use crate::sdk::cliproxy::auth::Auth;
use crate::sdk::cliproxy::executor::{Headers, RequestScopedError, StatusError};

fn parse(payload: &[u8]) -> Value {
    serde_json::from_slice(payload).expect("valid JSON")
}

#[derive(Debug)]
struct FixedClock(i64);

impl XaiWebsocketClock for FixedClock {
    fn now_millis(&self) -> i64 {
        self.0
    }
}

#[derive(Clone, Debug, Default)]
struct ConnectionState {
    writes: Arc<Mutex<Vec<XaiWebsocketFrame>>>,
    closes: Arc<Mutex<Vec<String>>>,
}

struct ScriptedConnection {
    state: ConnectionState,
    reads: VecDeque<Result<XaiWebsocketFrame, XaiWebsocketError>>,
    write_error: Option<XaiWebsocketError>,
}

impl XaiWebsocketConnection for ScriptedConnection {
    fn write(&mut self, frame: XaiWebsocketFrame) -> Result<(), XaiWebsocketError> {
        if let Some(error) = self.write_error.take() {
            return Err(error);
        }
        self.state.writes.lock().unwrap().push(frame);
        Ok(())
    }

    fn read(&mut self) -> Result<XaiWebsocketFrame, XaiWebsocketError> {
        self.reads
            .pop_front()
            .unwrap_or_else(|| Err(XaiWebsocketError::transport("script exhausted", false)))
    }

    fn close(&mut self, reason: &str) -> Result<(), XaiWebsocketError> {
        self.state.closes.lock().unwrap().push(reason.to_owned());
        Ok(())
    }
}

#[derive(Default)]
struct ScriptedTransport {
    connections: Mutex<VecDeque<ScriptedConnection>>,
    requests: Mutex<Vec<XaiWebsocketConnectRequest>>,
}

impl ScriptedTransport {
    fn push(&self, connection: ScriptedConnection) {
        self.connections.lock().unwrap().push_back(connection);
    }
}

impl XaiWebsocketTransport for ScriptedTransport {
    fn connect(
        &self,
        request: XaiWebsocketConnectRequest,
    ) -> Result<Box<dyn XaiWebsocketConnection>, XaiWebsocketError> {
        self.requests.lock().unwrap().push(request);
        self.connections
            .lock()
            .unwrap()
            .pop_front()
            .map(|connection| Box::new(connection) as Box<dyn XaiWebsocketConnection>)
            .ok_or_else(|| XaiWebsocketError::transport("no scripted connection", false))
    }
}

fn connection(
    frames: impl IntoIterator<Item = XaiWebsocketFrame>,
) -> (ScriptedConnection, ConnectionState) {
    let state = ConnectionState::default();
    (
        ScriptedConnection {
            state: state.clone(),
            reads: frames.into_iter().map(Ok).collect(),
            write_error: None,
        },
        state,
    )
}

fn execution_request(payload: &[u8]) -> XaiWebsocketExecutionRequest {
    XaiWebsocketExecutionRequest {
        session_id: "session-1".into(),
        url: "wss://api.x.ai/v1/responses".into(),
        headers: Headers::new(),
        credential: XaiWebsocketCredential {
            auth_id: "auth-1".into(),
            bearer_token: "token".into(),
            proxy_url: Some("direct://".into()),
        },
        payload: payload.to_vec(),
    }
}
