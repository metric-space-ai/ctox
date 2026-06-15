"use strict";

const assert = require("node:assert/strict");
const { spawn } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const password = await readPassword(options);
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-desktop-ctox-dev-live-"));
  const outputPath = path.join(tempRoot, "result.json");
  const userDataPath = path.join(tempRoot, "userData");
  try {
    const result = await runLiveFixture({
      outputPath,
      userDataPath,
      password,
      options,
    });
    const resultText = JSON.stringify(result, null, 2);
    assert.equal(resultText.includes(password), false, "ctox.dev live smoke evidence leaked password");
    if (!result.ok) {
      throw new Error(result.error || "ctox.dev live smoke failed");
    }
    console.log(resultText);
  } finally {
    if (!options.keepTemp) {
      fs.rmSync(tempRoot, { recursive: true, force: true });
    } else {
      console.error(`ctox.dev live smoke temp kept: ${tempRoot}`);
    }
  }
}

function parseArgs(args) {
  const options = {
    baseUrl: "https://ctox.dev",
    email: "",
    passwordStdin: false,
    expectedTenants: [],
    launchFirst: false,
    manageFirst: false,
    authWindow: false,
    sessionRotation: false,
    keepTemp: false,
  };
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--base-url") {
      options.baseUrl = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--email") {
      options.email = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--expected-tenant") {
      options.expectedTenants.push(String(args[index + 1] || "").trim());
      index += 1;
    } else if (arg === "--launch-first") {
      options.launchFirst = true;
    } else if (arg === "--manage-first") {
      options.manageFirst = true;
    } else if (arg === "--auth-window") {
      options.authWindow = true;
    } else if (arg === "--session-rotation") {
      options.sessionRotation = true;
    } else if (arg === "--password-stdin") {
      options.passwordStdin = true;
    } else if (arg === "--keep-temp") {
      options.keepTemp = true;
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (!options.baseUrl) throw new Error("--base-url must not be empty");
  if (!options.email) throw new Error("--email is required");
  if (!options.passwordStdin) {
    throw new Error("--password-stdin is required; never pass ctox.dev passwords as command arguments");
  }
  options.expectedTenants = options.expectedTenants.filter(Boolean);
  return options;
}

function runLiveFixture({ outputPath, userDataPath, password, options }) {
  return new Promise((resolve, reject) => {
    const electronPath = require("electron");
    const fixture = path.join(__dirname, "fixtures/ctox-dev-live-main.cjs");
    const args = [
      fixture,
      outputPath,
      userDataPath,
      "--base-url",
      options.baseUrl,
      "--email",
      options.email,
      ...(options.launchFirst ? ["--launch-first"] : []),
      ...(options.manageFirst ? ["--manage-first"] : []),
      ...(options.authWindow ? ["--auth-window"] : []),
      ...(options.sessionRotation ? ["--session-rotation"] : []),
      ...options.expectedTenants.flatMap((tenant) => ["--expected-tenant", tenant]),
    ];
    let stdout = "";
    let stderr = "";
    let settled = false;
    const child = spawn(electronPath, args, {
      cwd: path.join(__dirname, ".."),
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    child.stdin.end(`${password}\n`);
    const timeoutMs = options.sessionRotation ? 180000 : 90000;
    const timeout = setTimeout(() => {
      if (settled) return;
      settled = true;
      child.kill("SIGKILL");
      const partial = readResult(outputPath);
      reject(new Error(`ctox.dev live smoke timed out\npartial:\n${JSON.stringify(partial, null, 2)}\nstdout:\n${stdout}\nstderr:\n${stderr}`));
    }, timeoutMs);
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
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
      reject(error);
    });
    child.on("close", (code) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      const result = readResult(outputPath);
      if (!result) {
        reject(new Error(`ctox.dev live smoke exited without result (code ${code})\nstdout:\n${stdout}\nstderr:\n${stderr}`));
        return;
      }
      resolve(result);
    });
  });
}

function readResult(filePath) {
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch (_error) {
    return null;
  }
}

function readPassword(options) {
  if (!options.passwordStdin) {
    throw new Error("--password-stdin is required");
  }
  return new Promise((resolve, reject) => {
    let input = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => {
      input += chunk;
    });
    process.stdin.on("end", () => {
      const password = input.replace(/\r?\n$/, "");
      if (!password) {
        reject(new Error("password stdin was empty"));
        return;
      }
      resolve(password);
    });
    process.stdin.on("error", reject);
  });
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack || error.message : String(error));
  process.exit(1);
});
