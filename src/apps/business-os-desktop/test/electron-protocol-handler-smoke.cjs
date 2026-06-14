"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawn } = require("node:child_process");

async function main() {
  const electronPath = require("electron");
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-electron-protocol-smoke-"));
  const outputPath = path.join(tempRoot, "result.json");
  const userDataPath = path.join(tempRoot, "userData");
  const fixture = path.join(__dirname, "fixtures/protocol-handler-main.cjs");
  const coldStartUrl = "ctox-business-os-desktop://instance/tenant_cold";
  const openUrl = "ctox-business-os-desktop://pair?payload=mac-open-url";
  const secondInstanceUrl = "ctox-business-os-desktop://instance/tenant_second";
  const authCallbackUrl = "ctox-business-os-desktop://auth/callback?desktop=1";
  const result = await runElectron(electronPath, [
    fixture,
    outputPath,
    userDataPath,
    coldStartUrl,
    openUrl,
    secondInstanceUrl,
    authCallbackUrl,
  ], outputPath);
  assert.equal(result.code, 0, result.stderr || result.stdout);
  const payload = JSON.parse(fs.readFileSync(outputPath, "utf8"));
  assert.equal(payload.ok, true, JSON.stringify(payload, null, 2));
  assert.deepEqual(payload.pendingUrls, []);
  const expectedEvents = process.platform === "win32"
    ? ["managed", "invite", "managed", "auth-callback"]
    : [
      "prevented-open-url-default",
      "managed",
      "invite",
      "managed",
      "prevented-auth-default",
      "auth-callback",
    ];
  assert.deepEqual(payload.events.map((event) => event.type), expectedEvents);
  assert.equal(payload.events.at(-1).callbackUrl, authCallbackUrl);
}

function runElectron(command, args, resultPath) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: path.join(__dirname, ".."),
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    let observedCode = null;
    let settled = false;
    function finish(code) {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      clearInterval(resultPoll);
      child.stdout.destroy();
      child.stderr.destroy();
      child.kill("SIGKILL");
      child.unref();
      resolve({ code, stdout, stderr });
    }
    const timeout = setTimeout(() => {
      if (settled) return;
      settled = true;
      clearInterval(resultPoll);
      child.kill("SIGKILL");
      reject(new Error(`electron protocol handler smoke timed out\nstdout:\n${stdout}\nstderr:\n${stderr}`));
    }, 60000);
    const resultPoll = setInterval(() => {
      const payload = readResultFile(resultPath);
      if (!payload || typeof payload.ok !== "boolean") return;
      observedCode = payload.ok ? 0 : 2;
      finish(observedCode);
    }, 250);
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("error", (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      clearInterval(resultPoll);
      reject(error);
    });
    child.on("close", (code) => {
      if (settled) return;
      const payload = readResultFile(resultPath);
      if (payload && typeof payload.ok === "boolean") {
        observedCode = payload.ok ? 0 : 2;
      }
      if (observedCode === null) {
        settled = true;
        clearTimeout(timeout);
        clearInterval(resultPoll);
        reject(new Error(`electron protocol handler smoke exited before writing result (code ${code})\nstdout:\n${stdout}\nstderr:\n${stderr}`));
        return;
      }
      settled = true;
      clearTimeout(timeout);
      clearInterval(resultPoll);
      resolve({ code: observedCode, stdout, stderr });
    });
  });
}

function readResultFile(filePath) {
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch (_error) {
    return null;
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack || error.message : error);
  process.exit(1);
});
