// CTOX pi-coding sidecar — library entry + LocalTransport daemon.
//
// Re-exports the vendored Pi coding-agent turn primitive, the
// app-source-projection ExecutionEnv, and the LocalTransport server under CTOX
// names. The native Rust owner (src/core/coding_agents) spawns this sidecar,
// seeds the ExecutionEnv from a module's `business_module_source_files`
// snapshot, drives a single bounded turn over a Unix socket, and reads back the
// modified snapshot to record as P0 commits.
//
// Headless turn/session primitive — no pi-tui, no host filesystem, no direct
// network. The provider stream routes through the CTOX model gateway. See
// PORTING.md / UPSTREAM.md.
import { pathToFileURL } from "node:url";
import { startSocketServer, fauxStreamFn } from "./server";

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

export {
  handleTurnRequest,
  startSocketServer,
  defaultStreamFn,
  type CtoxTurnRequest,
  type CtoxTurnResponse,
} from "./server";

// When executed directly (`node ctox-pi-sidecar.mjs <unix-socket-path>`), run as
// the LocalTransport daemon the Rust owner spawns and supervises. When imported
// as a library (or by the smokes), this block is inert.
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const socketPath = process.argv[2];
  if (!socketPath) {
    console.error("usage: ctox-pi-sidecar <unix-socket-path>");
    process.exit(2);
  }
  // CTOX_PI_SIDECAR_FAUX runs a deterministic no-model stream for offline
  // owner-integration tests; otherwise the CTOX gateway provider is used.
  const streamFn = process.env.CTOX_PI_SIDECAR_FAUX ? fauxStreamFn() : undefined;
  startSocketServer(socketPath, streamFn);
  console.error(
    `[ctox-pi-sidecar] LocalTransport listening on ${socketPath}` +
      (streamFn ? " (faux mode)" : ""),
  );
}
