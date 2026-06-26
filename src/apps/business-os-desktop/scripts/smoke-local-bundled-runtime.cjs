"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const {
  loadRegistry,
  saveRegistry,
} = require("../src/main/registry.cjs");
const {
  SourceManager,
} = require("../src/main/source-manager.cjs");

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-business-os-desktop-bundled-runtime-"));
  const helperPath = path.join(tempRoot, "resources", "ctox", process.platform === "win32" ? "ctox.cmd" : "ctox");
  const target = path.join(tempRoot, "business-os");
  const desktopProfile = path.join(tempRoot, "desktop-profile");
  const registryPath = path.join(desktopProfile, "instances.json");
  fs.mkdirSync(path.dirname(helperPath), { recursive: true });
  fs.mkdirSync(target, { recursive: true });
  fs.mkdirSync(desktopProfile, { recursive: true });
  writeBundledCtoxHelper(helperPath);

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
  const bundledOptions = {
    bundledCtoxCandidates: [helperPath],
  };

  try {
    const inspect = await sourceManager().inspectLocalDaemon(bundledOptions);
    assert.equal(inspect.status, "available");
    assert.equal(inspect.ctoxBinary, helperPath);
    assert.equal(inspect.ctoxRoot, "");
    assert.equal(inspect.httpDataProxy, false);

    const instance = await sourceManager().attachLocalDaemon({
      ...bundledOptions,
      displayName: "Bundled Local CTOX Runtime Smoke",
    });
    assert.equal(instance.source, "local_daemon");
    assert.equal(instance.connection?.ctoxBinary, helperPath);
    assert.equal(Boolean(instance.connection?.ctoxRoot), false);
    assert.equal(instance.healthSummary?.dataPlane, "rxdb-webrtc");
    assert.equal(instance.healthSummary?.httpDataProxy, false);
    assert.equal(secrets.size, 1);
    assert.equal(JSON.stringify(registry).includes("bundled-room-secret"), false);

    const mixedList = await sourceManager().listInstances();
    assert.deepEqual(
      mixedList.map((entry) => entry.source),
      ["local_daemon"],
      "bundled fresh profile must not require ctox.dev managed instances",
    );

    registry = loadRegistry(registryPath);
    const restartedList = await sourceManager().listInstances();
    assert.deepEqual(restartedList.map((entry) => entry.id), [instance.id]);

    const launch = await sourceManager().getLaunchConfig(restartedList[0]);
    assert.equal(launch.ctoxConfig.transport, "webrtc");
    assert.equal(launch.ctoxConfig.http_bridge_available, false);
    assert.equal(launch.ctoxConfig.signaling_room_password, "bundled-room-secret");
    assert.ok(String(launch.ctoxConfig.sync_room || "").startsWith("ctox-business-os:"));

    console.log("desktop local bundled runtime smoke OK");
  } finally {
    if (!options.keepTarget) {
      fs.rmSync(tempRoot, { recursive: true, force: true });
    } else {
      console.log(`desktop local bundled runtime smoke target kept: ${target}`);
      console.log(`desktop local bundled runtime smoke profile kept: ${desktopProfile}`);
      console.log(`desktop local bundled runtime smoke helper kept: ${helperPath}`);
    }
  }
}

function parseArgs(args) {
  const options = {
    keepTarget: false,
  };
  for (const arg of args) {
    if (arg === "--keep-target") {
      options.keepTarget = true;
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  return options;
}

function writeBundledCtoxHelper(helperPath) {
  const jsBody = [
    process.platform === "win32" ? "" : "#!/usr/bin/env node",
    "\"use strict\";",
    "const args = process.argv.slice(2);",
    "if (args[0] === 'business-os' && args[1] === 'peer' && args[2] === 'ensure') {",
    "  process.exit(0);",
    "}",
    "if (args[0] === 'business-os' && args[1] === 'peer' && args[2] === 'status') {",
    "  console.log(JSON.stringify({",
    "    instance_id: 'bundled-local',",
    "    sync_room: 'ctox-business-os:bundled-local:room',",
    "    signaling_room_password: 'bundled-room-secret',",
    "    signaling_urls: ['wss://signaling.ctox.dev'],",
    "    native_rxdb_peer_available: true",
    "  }));",
    "  process.exit(0);",
    "}",
    "console.error(`unsupported fake ctox command: ${args.join(' ')}`);",
    "process.exit(64);",
    "",
  ].filter((line, index) => index !== 0 || line).join("\n");
  if (process.platform === "win32") {
    const jsPath = path.join(path.dirname(helperPath), "ctox-helper.js");
    fs.writeFileSync(jsPath, jsBody);
    fs.writeFileSync(helperPath, `@echo off\r\nnode "%~dp0ctox-helper.js" %*\r\n`);
    return;
  }
  fs.writeFileSync(helperPath, jsBody, { mode: 0o700 });
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack || error.message : String(error));
  process.exit(1);
});
