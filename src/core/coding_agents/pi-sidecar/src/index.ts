// CTOX pi-coding sidecar — library entry.
//
// Re-exports the vendored Pi coding-agent turn primitive and the
// app-source-projection ExecutionEnv under CTOX names. The native Rust owner
// (src/core/coding_agents) seeds the ExecutionEnv from a module's
// `business_module_source_files` snapshot, drives a single bounded turn, and
// reads back the modified snapshot to record as P0 commits.
//
// This is a headless turn/session primitive — no pi-tui, no host filesystem,
// no direct network. The provider stream (`streamFn`) is injected by the owner
// and routes through the CTOX model gateway. See PORTING.md / UPSTREAM.md.

export {
  VercelVirtualExecutionEnv as CtoxSourceExecutionEnv,
  createVercelVirtualExecutionEnv as createCtoxSourceExecutionEnv,
  type VercelVirtualExecutionEnvOptions as CtoxSourceExecutionEnvOptions,
  type VirtualFileSnapshotEntry,
} from "./execution-env";

export {
  runVercelPiCodingAgentTurn as runCtoxPiCodingTurn,
  createVercelPiCodingTools,
  createVercelPiCodingSystemPrompt,
  // Message helpers — owners (and the turn smoke) use these to construct seed
  // messages and, in tests, to drive a deterministic stub stream.
  createVercelPiCodingTextMessage,
  createVercelPiCodingToolCallMessage,
  vercelPiCodingToolNames,
  type RunVercelPiCodingAgentTurnInput as CtoxPiCodingTurnInput,
  type VercelPiCodingAgentTurnResult as CtoxPiCodingTurnResult,
  type VercelPiCodingToolName,
  type VercelPiCodingToolsMode,
} from "./pi-turn";
