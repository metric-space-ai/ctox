// ref: internal/runtime/executor/helps/apply_patch_responses.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: partial
// License: MIT (upstream); modifications AGPL-3.0-only

//! Request-side `apply_patch` bridge for a non-Codex executor.
//!
//! `ApplyPatchResponsesState` still has no Rust stream transformer. This
//! function is the request half Go runs before Kimi reorders Responses input.

pub use crate::internal::translator::common::normalize_apply_patch_responses_request;
