// ref: sdk/cliproxy/executor/websocket_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use std::error::Error;
use std::fmt;

use super::{
    is_upstream_websocket_replay_required, new_upstream_websocket_replay_required_error,
    RequestScopedError, StatusError,
};

#[derive(Debug)]
struct WrappedReplayError {
    source: super::UpstreamWebsocketReplayRequiredError,
}

impl fmt::Display for WrappedReplayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("wrapped")
    }
}

impl Error for WrappedReplayError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}
