// Socket smoke: exercises the REAL LocalTransport end-to-end — starts the Unix
// socket server (with a deterministic stub stream injected), connects a client,
// sends one newline-delimited CtoxTurnRequest, and asserts one CtoxTurnResponse
// comes back with the write applied. This is the exact wire the Rust owner uses.
import assert from "node:assert/strict";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { mkdtempSync, rmSync } from "node:fs";
import { createAssistantMessageEventStream } from "@earendil-works/pi-ai";
import {
  startSocketServer,
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

const dir = mkdtempSync(path.join(os.tmpdir(), "ctox-pi-sock-"));
const socketPath = path.join(dir, "sidecar.sock");
const server = startSocketServer(socketPath, streamFn);

const response = await new Promise((resolve, reject) => {
  const client = net.createConnection(socketPath, () => {
    client.write(
      `${JSON.stringify({
        id: "sock-1",
        prompt: "bump v to 2",
        files: { "index.js": "export const v = 1;\n" },
        maxAssistantTurns: 4,
      })}\n`,
    );
  });
  let buffer = "";
  client.on("data", (chunk) => {
    buffer += chunk.toString("utf8");
    const nl = buffer.indexOf("\n");
    if (nl >= 0) {
      client.end();
      try {
        resolve(JSON.parse(buffer.slice(0, nl)));
      } catch (error) {
        reject(error);
      }
    }
  });
  client.on("error", reject);
  setTimeout(() => reject(new Error("socket turn timed out")), 15000);
});

server.close();
rmSync(dir, { recursive: true, force: true });

assert.equal(response.ok, true, "socket turn ok");
assert.equal(response.id, "sock-1", "response echoes the request id over the wire");
assert.ok(
  response.snapshot.some(
    (entry) => entry.path.endsWith("index.js") && String(entry.content).includes("v = 2"),
  ),
  "snapshot with the write came back over the socket",
);
console.log("SOCKET SMOKE OK — real Unix-socket turn round-trip verified end-to-end");
