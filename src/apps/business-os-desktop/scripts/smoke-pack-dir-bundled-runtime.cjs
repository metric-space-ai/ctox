"use strict";

const assert = require("node:assert/strict");
const { spawnSync } = require("node:child_process");
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
  if (process.platform !== "darwin") {
    throw new Error(`pack directory bundled runtime smoke is not implemented for platform: ${process.platform}`);
  }
  const appRoot = path.join(__dirname, "..");
  const resourcesRoot = path.join(appRoot, "resources");
  const helperDir = path.join(resourcesRoot, "ctox");
  const helperPath = path.join(helperDir, "ctox");
  const releaseDir = path.join(appRoot, "release", `mac-${process.arch}`);
  const resourcesRootExisted = fs.existsSync(resourcesRoot);
  const helperDirExisted = fs.existsSync(helperDir);
  let helperCreated = false;
  if (fs.existsSync(helperPath)) {
    throw new Error(`refusing to overwrite existing helper: ${helperPath}`);
  }
  try {
    fs.rmSync(releaseDir, { recursive: true, force: true });
    fs.mkdirSync(helperDir, { recursive: true });
    writeBundledCtoxHelper(helperPath);
    helperCreated = true;
    runPackDir(appRoot);
    const appPath = findPackagedApp(releaseDir);
    const packagedHelperPath = path.join(appPath, "Contents", "Resources", "ctox", "ctox");
    assertPackagedHelper(packagedHelperPath);
    await exercisePackagedHelper(packagedHelperPath);
    console.log(`desktop pack dir bundled runtime smoke OK: ${packagedHelperPath}`);
  } finally {
    if (helperCreated) fs.rmSync(helperPath, { force: true });
    if (!helperDirExisted) fs.rmSync(helperDir, { recursive: true, force: true });
    if (!resourcesRootExisted) fs.rmSync(resourcesRoot, { recursive: true, force: true });
  }
}

function findPackagedApp(releaseDir) {
  const appPaths = fs.readdirSync(releaseDir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && entry.name.endsWith(".app"))
    .map((entry) => path.join(releaseDir, entry.name));
  assert.equal(appPaths.length, 1, `expected exactly one packaged app in ${releaseDir}, found ${appPaths.length}`);
  return appPaths[0];
}

function runPackDir(appRoot) {
  const result = spawnSync("npm", ["run", "pack:dir"], {
    cwd: appRoot,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    throw new Error(`npm run pack:dir failed\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`);
  }
}

function assertPackagedHelper(helperPath) {
  assert.ok(fs.existsSync(helperPath), `packaged helper is missing: ${helperPath}`);
  const stat = fs.statSync(helperPath);
  assert.equal(stat.isFile(), true, "packaged helper must be a file");
  assert.notEqual(stat.mode & 0o111, 0, "packaged helper must be executable");
}

async function exercisePackagedHelper(helperPath) {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-business-os-desktop-packaged-runtime-"));
  const desktopProfile = path.join(tempRoot, "desktop-profile");
  const registryPath = path.join(desktopProfile, "instances.json");
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
  const bundledOptions = {
    bundledCtoxCandidates: [helperPath],
  };
  try {
    const inspect = await sourceManager().inspectLocalDaemon(bundledOptions);
    assert.equal(inspect.status, "available");
    assert.equal(inspect.ctoxBinary, helperPath);
    assert.equal(inspect.httpDataProxy, false);

    const instance = await sourceManager().attachLocalDaemon({
      ...bundledOptions,
      displayName: "Packaged Local CTOX Runtime Smoke",
    });
    assert.equal(instance.source, "local_daemon");
    assert.equal(instance.connection?.ctoxBinary, helperPath);
    assert.equal(instance.healthSummary?.dataPlane, "rxdb-webrtc");
    assert.equal(instance.healthSummary?.httpDataProxy, false);
    assert.equal(secrets.size, 2);
    assert.equal(JSON.stringify(registry).includes("packaged-room-secret"), false);

    registry = loadRegistry(registryPath);
    const restartedList = await sourceManager().listInstances();
    assert.deepEqual(restartedList.map((entry) => entry.id), [instance.id]);
    const launch = await sourceManager().getLaunchConfig(restartedList[0]);
    assert.equal(launch.ctoxConfig.transport, "webrtc");
    assert.equal(launch.ctoxConfig.http_bridge_available, false);
    assert.equal(launch.ctoxConfig.signaling_room_password, "packaged-room-secret");
    assert.equal(launch.ctoxConfig.session.capability_token, "packaged-native-capability");
    assert.ok(String(launch.ctoxConfig.sync_room || "").startsWith("ctox-business-os:"));
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
  }
}

function writeBundledCtoxHelper(helperPath) {
  const body = [
    "#!/usr/bin/env node",
    "\"use strict\";",
    "const args = process.argv.slice(2);",
    "if (args[0] === 'business-os' && args[1] === 'peer' && args[2] === 'ensure') {",
    "  process.exit(0);",
    "}",
    "if (args[0] === 'business-os' && args[1] === 'peer' && args[2] === 'status') {",
    "  console.log(JSON.stringify({",
    "    instance_id: 'packaged-local',",
    "    sync_room: 'ctox-business-os:packaged-local:room',",
    "    signaling_room_password: 'packaged-room-secret',",
    "    signaling_urls: ['wss://signaling.ctox.dev'],",
    "    native_rxdb_peer_available: true",
    "  }));",
    "  process.exit(0);",
    "}",
    "if (args[0] === 'business-os' && args[1] === 'desktop' && args[2] === 'invite') {",
    "  console.log(JSON.stringify({",
    "    type: 'ctox-business-os-invite', version: 1, display_name: 'Packaged Local',",
    "    instance_id: 'packaged-local', sync_room: 'ctox-business-os:packaged-local:room',",
    "    signaling_room_password: 'packaged-room-secret', signaling_urls: ['wss://signaling.ctox.dev'],",
    "    transport: 'webrtc', expires_at: '2099-01-01T00:00:00.000Z',",
    "    session: { authenticated: true, capability_token: 'packaged-native-capability', capability_expires_at_ms: 4070908800000, user: { id: 'desktop-owner', display_name: 'Desktop Owner', role: 'chef' } }",
    "  }));",
    "  process.exit(0);",
    "}",
    "console.error(`unsupported fake ctox command: ${args.join(' ')}`);",
    "process.exit(64);",
    "",
  ].join("\n");
  fs.writeFileSync(helperPath, body, { mode: 0o700 });
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack || error.message : String(error));
  process.exit(1);
});
