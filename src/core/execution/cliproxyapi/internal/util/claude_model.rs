// ref: internal/util/claude_model.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

/// Reports whether the model name identifies a Claude model.
#[must_use]
pub fn is_claude_model(model: &str) -> bool {
    model.to_lowercase().contains("claude")
}

/// Returns whether the upstream model-name heuristic requires Claude's
/// interleaved-thinking beta header.
pub fn is_claude_thinking_model(model: &str) -> bool {
    let model = model.to_lowercase();
    model.contains("claude") && model.contains("thinking")
}
