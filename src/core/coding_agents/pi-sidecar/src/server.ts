// CTOX pi-coding sidecar — LocalTransport server + request handler.
//
// The native Rust owner (src/core/coding_agents) spawns this sidecar and drives
// it over a Unix socket: one newline-delimited JSON CtoxTurnRequest in, one
// CtoxTurnResponse out. Each request is one bounded turn. The core mapping
// (`handleTurnRequest`) takes an injected `streamFn`, so it is testable without
// a live model provider; the socket glue uses the CTOX gateway provider (or the
// deterministic `fauxStreamFn` in CTOX_PI_SIDECAR_FAUX offline-test mode).
import net from "node:net";
import { stream as piStream, registerBuiltInApiProviders } from "@earendil-works/pi-ai/compat";
import { createAssistantMessageEventStream, type Api, type Model } from "@earendil-works/pi-ai";
import type { StreamFn } from "@earendil-works/pi-agent-core";
import { createVercelVirtualExecutionEnv } from "./execution-env";
import {
  runVercelPiCodingAgentTurn,
  createVercelPiCodingTextMessage,
  createVercelPiCodingToolCallMessage,
  type VercelPiCodingToolName,
} from "./pi-turn";

type TurnResult = Awaited<ReturnType<typeof runVercelPiCodingAgentTurn>>;

export type CtoxTurnRequest = {
  id?: string;
  prompt: string;
  /** App-source snapshot the Rust owner projects from business_module_source_files. */
  files?: Record<string, string>;
  systemPrompt?: string;
  tools?: VercelPiCodingToolName[];
  maxAssistantTurns?: number;
  /** Provider model; its provider points at the CTOX model gateway. */
  model?: Model<Api>;
  cwd?: string;
};

export type CtoxTurnResponse = {
  id?: string;
  ok: boolean;
  error?: string;
  messages?: TurnResult["messages"];
  events?: TurnResult["events"];
  snapshot?: TurnResult["snapshot"];
};

let providersRegistered = false;

export const CTOX_PI_MAX_REQUEST_BYTES = 8 * 1024 * 1024;
export const CTOX_PI_MAX_RESPONSE_BYTES = 32 * 1024 * 1024;
export const CTOX_PI_TURN_TIMEOUT_MS = 600_000;

export type SocketServerLimits = {
  maxRequestBytes?: number;
  maxResponseBytes?: number;
  turnTimeoutMs?: number;
};

/**
 * Default provider stream. Routes to pi-ai's registered providers by the
 * request model's api/provider (env-injected keys). The CTOX owner supplies a
 * model whose provider/baseUrl targets the CTOX model gateway — the sidecar
 * itself opens no arbitrary outbound connections.
 */
export function defaultStreamFn(): StreamFn {
  if (!providersRegistered) {
    registerBuiltInApiProviders();
    providersRegistered = true;
  }
  return piStream as unknown as StreamFn;
}

/**
 * Deterministic write-then-stop stream for offline integration tests and the
 * `CTOX_PI_SIDECAR_FAUX` daemon mode — NO real model. It issues one `write`
 * tool call, then stops once the tool result returns. Never used for real turns.
 */
export function fauxStreamFn(
  write: { path: string; content: string } = { path: "faux-marker.js", content: "// faux\n" },
): StreamFn {
  return (_model, context) => {
    const stream = createAssistantMessageEventStream();
    const hasToolResult = context.messages.some((message) => message.role === "toolResult");
    stream.push({
      type: "done",
      reason: hasToolResult ? "stop" : "toolUse",
      message: hasToolResult
        ? createVercelPiCodingTextMessage("Done (faux).")
        : createVercelPiCodingToolCallMessage("write", write, "faux-w1"),
    });
    return stream;
  };
}

/**
 * Pure request → turn → response. Seeds the ExecutionEnv from the app-source
 * snapshot, runs one bounded turn, returns messages/events/snapshot. `streamFn`
 * is injected so this is unit-testable with a deterministic stub.
 */
export async function handleTurnRequest(
  request: CtoxTurnRequest,
  streamFn: StreamFn,
): Promise<CtoxTurnResponse> {
  try {
    const env = createVercelVirtualExecutionEnv({
      files: request.files ?? {},
      cwd: request.cwd,
    });
    const result = await runVercelPiCodingAgentTurn({
      env,
      prompt: request.prompt,
      streamFn,
      systemPrompt: request.systemPrompt,
      tools: request.tools,
      maxAssistantTurns: request.maxAssistantTurns,
      model: request.model,
    });
    // pi-ai reports provider failures as terminal assistant messages, not
    // necessarily rejected promises. Never apply a snapshot from a failed turn.
    const failed = result.messages.find((message) => message.role === "assistant"
      && (message.stopReason === "error" || message.stopReason === "aborted"));
    if (failed?.role === "assistant") {
      // Provider error text can contain URLs, headers or credentials. Return a
      // bounded category rather than copying it into app history or CLI logs.
      const detail = failed.errorMessage ?? "";
      const category = failed.stopReason === "aborted" ? "aborted"
        : /ECONNREFUSED|ENOTFOUND|fetch failed|connection error/i.test(detail) ? "connection_error"
        : /\b401\b|\b403\b|unauthorized|forbidden|no api key/i.test(detail) ? "authentication_error"
        : /\b429\b|rate.limit/i.test(detail) ? "rate_limited"
        : /timeout|timed out/i.test(detail) ? "timeout"
        : "provider_error";
      return { id: request.id, ok: false, error: `pi coding turn failed: ${category}` };
    }
    const terminalAssistant = [...result.messages].reverse().find((message) => message.role === "assistant");
    if (terminalAssistant?.role !== "assistant" || terminalAssistant.stopReason !== "stop") {
      // A bounded turn can stop immediately after a tool edit. That is an
      // unfinished in-memory workspace, not an atomic app-source release.
      return { id: request.id, ok: false, error: "pi coding turn failed: incomplete_turn" };
    }
    return {
      id: request.id,
      ok: true,
      messages: result.messages,
      events: result.events,
      snapshot: result.snapshot,
    };
  } catch (error) {
    return {
      id: request.id,
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

/**
 * LocalTransport: newline-delimited JSON over a Unix socket. One
 * CtoxTurnRequest per line in, one CtoxTurnResponse per line out.
 */
export function startSocketServer(
  socketPath: string,
  streamFn: StreamFn = defaultStreamFn(),
  limits: SocketServerLimits = {},
): net.Server {
  const server = net.createServer((socket) => {
    const maxRequestBytes = limits.maxRequestBytes ?? CTOX_PI_MAX_REQUEST_BYTES;
    const maxResponseBytes = limits.maxResponseBytes ?? CTOX_PI_MAX_RESPONSE_BYTES;
    const turnTimeoutMs = limits.turnTimeoutMs ?? CTOX_PI_TURN_TIMEOUT_MS;
    let buffer = "";
    let bufferBytes = 0;
    let terminal = false;
    socket.setTimeout(turnTimeoutMs, () => socket.destroy(new Error("turn timed out")));
    socket.on("data", (chunk) => {
      if (terminal) return;
      bufferBytes += chunk.length;
      if (bufferBytes > maxRequestBytes) {
        terminal = true;
        socket.pause();
        socket.write(
          `${JSON.stringify({ ok: false, error: "turn request is too large" })}\n`,
          () => socket.destroy(),
        );
        return;
      }
      buffer += chunk.toString("utf8");
      for (let nl = buffer.indexOf("\n"); nl >= 0; nl = buffer.indexOf("\n")) {
        const line = buffer.slice(0, nl).trim();
        buffer = buffer.slice(nl + 1);
        bufferBytes = Buffer.byteLength(buffer, "utf8");
        if (line) void dispatchLine(line, socket, streamFn, turnTimeoutMs, maxResponseBytes);
      }
    });
  });
  server.listen(socketPath);
  return server;
}

async function dispatchLine(
  line: string,
  socket: net.Socket,
  streamFn: StreamFn,
  turnTimeoutMs: number,
  maxResponseBytes: number,
): Promise<void> {
  let response: CtoxTurnResponse;
  let timeoutHandle: ReturnType<typeof setTimeout> | undefined;
  try {
    response = await Promise.race([
      handleTurnRequest(JSON.parse(line) as CtoxTurnRequest, streamFn),
      new Promise<CtoxTurnResponse>((resolve) => {
        timeoutHandle = setTimeout(
          () => resolve({ ok: false, error: "turn timed out" }),
          turnTimeoutMs,
        );
      }),
    ]);
  } catch (error) {
    response = { ok: false, error: error instanceof Error ? error.message : String(error) };
  } finally {
    if (timeoutHandle) clearTimeout(timeoutHandle);
  }
  let encoded = JSON.stringify(response);
  if (Buffer.byteLength(encoded, "utf8") > maxResponseBytes) {
    encoded = JSON.stringify({ ok: false, error: "turn response is too large" });
  }
  socket.write(`${encoded}\n`);
}
