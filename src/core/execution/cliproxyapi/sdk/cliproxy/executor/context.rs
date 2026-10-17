// ref: sdk/cliproxy/executor/context.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

/// Typed request intentions that affect downstream/upstream WebSocket routing.
///
/// Go stores these as private boolean context keys. Rust carries them as an
/// immutable copyable value so arbitrary context values cannot spoof the flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecutionTransportContext {
    downstream_websocket: bool,
    required_upstream_websocket: bool,
}

/// Marks a request as originating from a downstream WebSocket connection.
#[must_use]
pub fn with_downstream_websocket(
    context: Option<ExecutionTransportContext>,
) -> ExecutionTransportContext {
    ExecutionTransportContext {
        downstream_websocket: true,
        ..context.unwrap_or_default()
    }
}

/// Reports whether a request originated from a downstream WebSocket.
#[must_use]
pub fn downstream_websocket(context: Option<&ExecutionTransportContext>) -> bool {
    context.is_some_and(|context| context.downstream_websocket)
}

/// Marks a request whose incremental state is valid only on the current
/// upstream WebSocket.
#[must_use]
pub fn with_required_upstream_websocket(
    context: Option<ExecutionTransportContext>,
) -> ExecutionTransportContext {
    ExecutionTransportContext {
        required_upstream_websocket: true,
        ..context.unwrap_or_default()
    }
}

/// Reports whether HTTP fallback would lose incremental request context.
#[must_use]
pub fn required_upstream_websocket(context: Option<&ExecutionTransportContext>) -> bool {
    context.is_some_and(|context| context.required_upstream_websocket)
}

#[cfg(test)]
mod tests {
    use super::*;
}
