// Faux daemon smoke: spawns the built sidecar as a real child process
// (`node ctox-pi-sidecar.mjs <socket>`) in CTOX_PI_SIDECAR_FAUX mode — exactly
// how the native Rust owner will spawn it — connects over the Unix socket,
// sends one CtoxTurnRequest, and asserts the faux write round-trips. Proves the
// full spawn -> daemon -> socket -> turn path offline (no model, no gateway).
import assert from "node:assert/strict";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const dist = path.join(here, "..", "dist", "ctox-pi-sidecar.mjs");
const dir = mkdtempSync(path.join(os.tmpdir(), "ctox-pi-daemon-"));
const socketPath = path.join(dir, "sidecar.sock");

const child = spawn(process.execPath, [dist, socketPath], {
  env: { ...process.env, CTOX_PI_SIDECAR_FAUX: "1" },
  stdio: ["ignore", "ignore", "inherit"],
});

const cleanup = () => {
  try {
    child.kill("SIGKILL");
  } catch {
    /* ignore */
  }
  rmSync(dir, { recursive: true, force: true });
};

const waitForSocket = async (timeoutMs = 10000) => {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (existsSync(socketPath)) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error("daemon socket did not appear");
};

try {
  await waitForSocket();
  const response = await new Promise((resolve, reject) => {
    const client = net.createConnection(socketPath, () => {
      client.write(
        `${JSON.stringify({
          id: "d1",
          prompt: "add a marker",
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
    setTimeout(() => reject(new Error("daemon turn timed out")), 15000);
  });

  assert.equal(response.ok, true, "spawned daemon turn ok");
  assert.equal(response.id, "d1", "daemon echoes the request id");
  assert.ok(
    response.snapshot.some((entry) => entry.path.endsWith("faux-marker.js")),
    "faux write landed via the spawned daemon over the socket",
  );
  console.log("FAUX DAEMON SMOKE OK — spawned sidecar daemon served a turn over the socket");
} finally {
  cleanup();
}
