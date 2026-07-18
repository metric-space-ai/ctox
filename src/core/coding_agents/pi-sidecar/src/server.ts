// CTOX pi-coding sidecar — LocalTransport server + request handler.
//
// The native Rust owner (src/core/coding_agents) spawns this sidecar and drives
// it over a Unix socket: one newline-delimited JSON CtoxTurnRequest in, one
// CtoxTurnResponse out. Each request is one bounded turn. The core mapping
// (`handleTurnRequest`) takes an injected `streamFn`, so it is testable without
// a live model provider; the socket glue uses the CTOX gateway provider.
import net from "node:net";
import { stream as piStream, registerBuiltInApiProviders } from "@earendil-works/pi-ai/compat";
import type { Api, Model } from "@earendil-works/pi-ai";
import type { StreamFn } from "@earendil-works/pi-agent-core";
import { createVercelVirtualExecutionEnv } from "./execution-env";
import { runVercelPiCodingAgentTurn, type VercelPiCodingToolName } from "./pi-turn";

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
export function startSocketServer(socketPath: string): net.Server {
  const streamFn = defaultStreamFn();
  const server = net.createServer((socket) => {
    let buffer = "";
    socket.on("data", (chunk) => {
      buffer += chunk.toString("utf8");
      for (let nl = buffer.indexOf("\n"); nl >= 0; nl = buffer.indexOf("\n")) {
        const line = buffer.slice(0, nl).trim();
        buffer = buffer.slice(nl + 1);
        if (line) void dispatchLine(line, socket, streamFn);
      }
    });
  });
  server.listen(socketPath);
  return server;
}

async function dispatchLine(line: string, socket: net.Socket, streamFn: StreamFn): Promise<void> {
  let response: CtoxTurnResponse;
  try {
    response = await handleTurnRequest(JSON.parse(line) as CtoxTurnRequest, streamFn);
  } catch (error) {
    response = { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
  socket.write(`${JSON.stringify(response)}\n`);
}
