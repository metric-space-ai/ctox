// Turn smoke: drives the built sidecar bundle through one real Pi runAgentLoop
// turn with a DETERMINISTIC stub stream (no real LLM), and asserts the write
// landed on the projected app source and shows up in the returned snapshot.
// Stub-stream pattern mirrors upstream MRP's node:test.
import assert from "node:assert/strict";
import { createAssistantMessageEventStream } from "@earendil-works/pi-ai";
import {
  createCtoxSourceExecutionEnv,
  runCtoxPiCodingTurn,
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

// env seeded from an app-source snapshot (what the Rust owner will project from
// business_module_source_files).
const env = createCtoxSourceExecutionEnv({ files: { "index.js": "export const v = 1;\n" } });

const result = await runCtoxPiCodingTurn({
  env,
  prompt: "bump v to 2",
  streamFn,
  maxAssistantTurns: 4,
});

const written = await env.readTextFile("index.js");
assert.ok(written.ok, "readTextFile ok");
assert.equal(written.value, "export const v = 2;\n", "write applied to projected source");
assert.ok(
  result.snapshot.some(
    (entry) => entry.path.endsWith("index.js") && String(entry.content).includes("v = 2"),
  ),
  "snapshot reflects the edit",
);
assert.ok(
  result.messages.some((message) => message.role === "toolResult" && message.toolName === "write"),
  "turn recorded the write tool result",
);
console.log("SMOKE OK — pi turn ran on projected app source, write applied, snapshot updated");
