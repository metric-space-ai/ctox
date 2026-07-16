"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { execFileSync } = require("node:child_process");
const {
  loadRegistry,
  saveRegistry,
} = require("../src/main/registry.cjs");
const {
  SourceManager,
} = require("../src/main/source-manager.cjs");

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const ctoxBinary = options.ctox || findCtoxBinary();
  if (!ctoxBinary) {
    throw new Error("ctox binary not found; pass --ctox <path> to run the local runtime smoke");
  }
  const ctoxRepoRoot = options.ctoxRoot || findCtoxRepoRoot();

  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-business-os-desktop-local-runtime-"));
  const target = path.join(tempRoot, "business-os");
  const desktopProfile = path.join(tempRoot, "desktop-profile");
  const registryPath = path.join(desktopProfile, "instances.json");
  fs.mkdirSync(target, { recursive: true });
  fs.mkdirSync(desktopProfile, { recursive: true });

  const secrets = new Map();
  let registry = loadRegistry(registryPath);
  const saveRegistryState = (next) => {
    registry = saveRegistry(registryPath, next);
  };
  const secretStore = {
    get: async (ref) => secrets.get(ref) || "",
    set: async (ref, value) => secrets.set(ref, value),
    delete: async (ref) => secrets.delete(ref),
  };
  const fetchImpl = async () => ({ ok: false, status: 401 });
  const sourceManager = () => new SourceManager({
    registryProvider: () => registry,
    registrySaver: saveRegistryState,
    secretStore,
    ctoxDevBaseUrl: "https://ctox.dev",
    shellUrl: "https://ctox.dev/business-os/",
    fetchImpl,
  });

  try {
    const invalidRuntimeRoot = await sourceManager().inspectLocalDaemon({
      ctoxBinary,
      ctoxRoot: target,
    });
    assert.equal(invalidRuntimeRoot.status, "error");
    assert.match(invalidRuntimeRoot.message, /not a CTOX runtime root/);

    const instance = await sourceManager().attachLocalDaemon({
      ctoxBinary,
      ctoxRoot: ctoxRepoRoot,
      displayName: "Fresh Local CTOX Runtime Smoke",
    });
    assert.equal(instance.source, "local_daemon");
    assert.equal(instance.connection?.ctoxRoot, ctoxRepoRoot);
    assert.equal(instance.healthSummary?.dataPlane, "rxdb-webrtc");
    assert.equal(instance.healthSummary?.httpDataProxy, false);
    assert.equal(secrets.size, 2);
    assert.equal(JSON.stringify(registry).includes("signaling_room_password"), false);
    assert.equal(JSON.stringify(registry).includes("ctox-room-"), false);

    const mixedList = await sourceManager().listInstances();
    assert.deepEqual(
      mixedList.map((entry) => entry.source),
      ["local_daemon"],
      "fresh profile should not require or leak ctox.dev managed instances",
    );

    registry = loadRegistry(registryPath);
    const restartedList = await sourceManager().listInstances();
    assert.deepEqual(restartedList.map((entry) => entry.id), [instance.id]);

    const launch = await sourceManager().getLaunchConfig(restartedList[0]);
    assert.equal(launch.ctoxConfig.transport, "webrtc");
    assert.equal(launch.ctoxConfig.http_bridge_available, false);
    assert.ok(Array.isArray(launch.ctoxConfig.signaling_urls));
    assert.ok(String(launch.ctoxConfig.sync_room || "").startsWith("ctox-business-os:"));
    assert.ok(String(launch.ctoxConfig.session?.capability_token || ""));

    console.log("desktop local runtime smoke OK");
  } finally {
    if (!options.keepTarget) {
      fs.rmSync(tempRoot, { recursive: true, force: true });
    } else {
      console.log(`desktop local runtime smoke target kept: ${target}`);
      console.log(`desktop local runtime smoke profile kept: ${desktopProfile}`);
    }
  }
}

function parseArgs(args) {
  const options = {
    ctox: "",
    ctoxRoot: "",
    keepTarget: false,
  };
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--ctox") {
      options.ctox = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--ctox-root") {
      options.ctoxRoot = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--keep-target") {
      options.keepTarget = true;
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (options.ctox && !path.isAbsolute(options.ctox)) {
    throw new Error("--ctox must be an absolute path");
  }
  if (options.ctoxRoot && !path.isAbsolute(options.ctoxRoot)) {
    throw new Error("--ctox-root must be an absolute path");
  }
  return options;
}

function findCtoxBinary() {
  for (const candidate of [
    path.join(os.homedir(), ".local", "bin", "ctox"),
    "/usr/local/bin/ctox",
  ]) {
    if (isExecutable(candidate)) return candidate;
  }
  try {
    const resolved = execFileSync("which", ["ctox"], { encoding: "utf8" }).trim();
    if (resolved && isExecutable(resolved)) return resolved;
  } catch {
    // Fall through to explicit error in main.
  }
  return "";
}

function findCtoxRepoRoot() {
  let current = path.resolve(__dirname, "..");
  while (true) {
    const hasCargoToml = fs.existsSync(path.join(current, "Cargo.toml"));
    const hasEntrypoint = fs.existsSync(path.join(current, "src", "main.rs"))
      || fs.existsSync(path.join(current, "src", "core", "main.rs"));
    const hasCreationLedger = fs.existsSync(path.join(current, "contracts", "history", "creation-ledger.md"));
    if (hasCargoToml && hasEntrypoint && hasCreationLedger) {
      return current;
    }
    const parent = path.dirname(current);
    if (parent === current) break;
    current = parent;
  }
  throw new Error("CTOX repo root not found; pass --ctox-root <path>");
}

function isExecutable(filePath) {
  try {
    fs.accessSync(filePath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack || error.message : String(error));
  process.exit(1);
});
