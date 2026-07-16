"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawn } = require("node:child_process");

async function main() {
  const electronPath = require("electron");
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-electron-renderer-smoke-"));
  const outputPath = path.join(tempRoot, "result.json");
  const userDataPath = path.join(tempRoot, "userData");
  const fixture = path.join(__dirname, "fixtures/renderer-badges-main.cjs");
  const result = await runElectron(electronPath, [fixture, outputPath, userDataPath], outputPath);
  assert.equal(result.code, 0, result.stderr || result.stdout);
  const payload = JSON.parse(fs.readFileSync(outputPath, "utf8"));
  assert.equal(payload.ok, true, JSON.stringify(payload, null, 2));
  assert.equal(payload.initial.length, 3);
  assert.equal(payload.connectionRequests.localAttach.length, 1);
  assert.equal(payload.connectionRequests.localInstall.length, 1);
  assert.equal(payload.connectionRequests.sshAttach.length, 1);
  assert.equal(payload.connectionRequests.sshInstall.length, 1);
  assert.equal(payload.connectionRequests.inviteImport.length, 1);
  assert.equal(payload.connectionRequests.manualPairing.length, 1);
  assert.equal(payload.connectionChoice.peerToPeerVisible, true);
  assert.equal(payload.connectionChoice.ctoxDevLoginRequests, 1);
  assert.equal(payload.connectionChoice.peerTabLabel, "Peer2Peer");
  assert.equal(payload.quickSwitchFocused, true);
  assert.deepEqual(payload.activateRequests.map((request) => request.source), [
    "ctox_dev",
    "ssh_managed",
    "pairing_invite",
  ]);
  assert.equal(payload.activateRequests.every((request) => request.dataPlane === "rxdb-webrtc"), true);
  assert.equal(payload.activateRequests.every((request) => request.httpDataProxy === false), true);
  assert.equal(payload.filtered.length, 1);
  assert.equal(payload.revokeRequests.length, 1);
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
      reject(new Error(`electron renderer badges smoke timed out\nstdout:\n${stdout}\nstderr:\n${stderr}`));
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
      settled = true;
      clearTimeout(timeout);
      clearInterval(resultPoll);
      resolve({ code: observedCode === null ? code : observedCode, stdout, stderr });
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
