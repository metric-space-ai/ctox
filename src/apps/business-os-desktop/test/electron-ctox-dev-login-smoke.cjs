"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawn } = require("node:child_process");

async function main() {
  const electronPath = require("electron");
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-electron-ctox-dev-login-"));
  const outputPath = path.join(tempRoot, "result.json");
  const userDataPath = path.join(tempRoot, "userData");
  const fixture = path.join(__dirname, "fixtures/ctox-dev-login-main.cjs");
  const result = await runElectron(electronPath, [fixture, outputPath, userDataPath], outputPath);
  assert.equal(result.code, 0, result.stderr || result.stdout);
  const payload = JSON.parse(fs.readFileSync(outputPath, "utf8"));
  assert.equal(payload.ok, true, JSON.stringify(payload, null, 2));
  assert.deepEqual(payload.managedInstanceNames, ["Kunstmen", "SKF"]);
  assert.deepEqual(payload.instanceNames, ["Kunstmen", "Local Lab", "SKF"]);
  assert.deepEqual(payload.managedInstanceNamesAfterRevocation, ["Kunstmen"]);
  assert.deepEqual(payload.instanceNamesAfterRevocation, ["Kunstmen", "Local Lab"]);
  assert.deepEqual(payload.instanceNamesAfterLogout, ["Local Lab"]);
  assert.equal(payload.logout.ok, true);
  assert.equal(payload.evidence.sessionPackageSawCookie, true);
  assert.deepEqual(payload.evidence.sessionPackageCookieObservations, [true, true, false]);
  assert.equal(payload.evidence.sessionPackageLastSawCookie, false);
  assert.equal(payload.evidence.launchTokenSawCookie, true);
  assert.deepEqual(payload.evidence.launchTokenTenantIds, ["tenant_kunstmen", "tenant_kunstmen"]);
  assert.equal(payload.evidence.launchConfigSawCookie, true);
  assert.deepEqual(payload.evidence.launchConfigUrls, [
    "/api/desktop/launch/tenant_kunstmen/1",
    "/api/desktop/launch/tenant_kunstmen/2",
  ]);
  assert.equal(payload.launch.ctoxConfig.http_bridge_available, false);
  assert.equal(payload.rotatedLaunch.ctoxConfig.launchEpoch, 2);
  assert.notEqual(payload.rotatedLaunch.expiresAt, payload.launch.expiresAt);
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
      reject(new Error(`electron ctox.dev login smoke timed out\nstdout:\n${stdout}\nstderr:\n${stderr}`));
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
        reject(new Error(`electron ctox.dev login smoke exited before writing result (code ${code})\nstdout:\n${stdout}\nstderr:\n${stderr}`));
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
