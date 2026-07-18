// Server smoke: verifies the LocalTransport request handler maps a
// CtoxTurnRequest -> bounded turn -> CtoxTurnResponse, seeding the ExecutionEnv
// from the request's app-source snapshot. Uses a deterministic stub stream (no
// LLM), so it exercises the payload/turn contract the Rust owner depends on.
import assert from "node:assert/strict";
import { createAssistantMessageEventStream } from "@earendil-works/pi-ai";
import {
  handleTurnRequest,
  createVercelPiCodingTextMessage,
  createVercelPiCodingToolCallMessage,
} from "../dist/ctox-pi-sidecar.mjs";

const streamFn = (_model, context) => {
  const stream = createAssistantMessageEventStream();
  const hasToolResult = context.messages.some((message) => message.role === "toolResult");
  stream.push({
    type: "done",
    reason: hasToolResult ? "stop" : "toolUse",
    message: hasToolResult
      ? createVercelPiCodingTextMessage("Done.")
      : createVercelPiCodingToolCallMessage(
          "write",
          { path: "index.js", content: "export const v = 2;\n" },
          "w1",
        ),
  });
  return stream;
};

const response = await handleTurnRequest(
  {
    id: "turn-1",
    prompt: "bump v to 2",
    files: { "index.js": "export const v = 1;\n" },
    maxAssistantTurns: 4,
  },
  streamFn,
);

assert.equal(response.ok, true, "turn ok");
assert.equal(response.id, "turn-1", "response echoes the request id");
assert.ok(
  response.snapshot.some(
    (entry) => entry.path.endsWith("index.js") && String(entry.content).includes("v = 2"),
  ),
  "response snapshot reflects the write on projected source",
);
assert.ok(
  response.messages.some((message) => message.role === "toolResult" && message.toolName === "write"),
  "response carries the write tool result",
);
console.log("SERVER SMOKE OK — request -> turn -> response mapping verified over the transport payload");
