"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { createDefaultRegistry, loadRegistry, saveRegistry } = require("../src/main/registry.cjs");
const {
  LocalDaemonInstanceSource,
  assertLocalCtoxRoot,
  buildLocalCommandOptions,
  buildLocalPeerArgs,
  localCtoxBinaryCandidates,
  normalizeLocalInstallOptions,
  normalizeLocalProfile,
  resolveLocalCtoxBinary,
} = require("../src/main/sources.cjs");

test("local install options use the bundled installer and a user-local CTOX binary", () => {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-desktop-local-installer-"));
  const installScriptPath = path.join(tempRoot, "install.sh");
  fs.writeFileSync(installScriptPath, "#!/usr/bin/env bash\n", { mode: 0o700 });
  const options = normalizeLocalInstallOptions({
    platform: "darwin",
    installScriptPath,
    installedCtoxBinary: path.join(tempRoot, "bin", "ctox"),
  });
  assert.equal(options.backend, "metal");
  assert.equal(options.apiProvider, "openai");
  assert.equal(options.installScriptPath, installScriptPath);
  assert.equal(options.ctoxBinary, path.join(tempRoot, "bin", "ctox"));
});

test("local profile validates CLI inputs", () => {
  assert.deepEqual(normalizeLocalProfile({ ctoxBinary: "ctox", ctoxRoot: "/opt/ctox" }), {
    ctoxBinary: "ctox",
    ctoxRoot: "/opt/ctox",
  });
  assert.throws(() => normalizeLocalProfile({ ctoxBinary: "ctox\nbad" }), /unsupported/);
});

test("local profile resolves bundled ctox helper before PATH fallback", () => {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-desktop-bundled-helper-"));
  const helper = path.join(tempRoot, process.platform === "win32" ? "ctox.exe" : "ctox");
  fs.writeFileSync(helper, "#!/bin/sh\nexit 0\n", { mode: 0o700 });

  assert.equal(resolveLocalCtoxBinary({ bundledCtoxCandidates: [helper] }), helper);
  assert.equal(
    normalizeLocalProfile({ bundledCtoxCandidates: [helper] }).ctoxBinary,
    helper,
  );
  assert.equal(
    normalizeLocalProfile({
      ctoxBinary: "/usr/local/bin/ctox",
      bundledCtoxCandidates: [helper],
    }).ctoxBinary,
    "/usr/local/bin/ctox",
  );

  const candidates = localCtoxBinaryCandidates({
    resourcesPath: tempRoot,
    platform: "linux",
    arch: "x64",
  });
  assert.ok(candidates.includes(path.join(tempRoot, "ctox", "ctox")));
  assert.ok(candidates.includes(path.join(tempRoot, "ctox", "linux-x64", "ctox")));
});

test("local CLI args use business-os peer ensure/status", () => {
  assert.deepEqual(buildLocalPeerArgs("status"), ["business-os", "peer", "status"]);
  assert.deepEqual(buildLocalPeerArgs("ensure"), ["business-os", "peer", "ensure"]);
  assert.throws(() => buildLocalPeerArgs("rotate"), /unsupported/);
});

test("local command options bind child processes to the selected ctox root", () => {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-desktop-local-root-"));
  const ctoxRoot = path.join(tempRoot, "ctox-runtime");
  fs.mkdirSync(path.join(ctoxRoot, "src", "core"), { recursive: true });
  fs.mkdirSync(path.join(ctoxRoot, "contracts", "history"), { recursive: true });
  fs.writeFileSync(path.join(ctoxRoot, "Cargo.toml"), "[package]\nname = \"ctox-test\"\n");
  fs.writeFileSync(path.join(ctoxRoot, "src", "core", "main.rs"), "fn main() {}\n");
  fs.writeFileSync(path.join(ctoxRoot, "contracts", "history", "creation-ledger.md"), "# ledger\n");

  assert.doesNotThrow(() => assertLocalCtoxRoot(ctoxRoot));
  const options = buildLocalCommandOptions({
    ctoxBinary: "ctox",
    ctoxRoot,
  }, 15000);
  assert.equal(options.cwd, ctoxRoot);
  assert.equal(options.env.CTOX_ROOT, ctoxRoot);
  assert.equal(options.timeout, 15000);
  assert.equal(options.windowsHide, true);

  const defaultOptions = buildLocalCommandOptions({
    ctoxBinary: "ctox",
    ctoxRoot: "",
  }, 15000);
  assert.equal(defaultOptions.cwd, undefined);
  assert.equal(defaultOptions.env, undefined);
});

test("local command options reject non-runtime roots", () => {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-desktop-non-runtime-root-"));
  fs.writeFileSync(path.join(tempRoot, "README.md"), "not a CTOX runtime root\n");

  assert.throws(
    () => assertLocalCtoxRoot(tempRoot),
    /not a CTOX runtime root/,
  );
  assert.throws(
    () => buildLocalCommandOptions({ ctoxBinary: "ctox", ctoxRoot: tempRoot }, 15000),
    /not a CTOX runtime root/,
  );
});

test("local daemon attach stores metadata only and builds webrtc launch", async () => {
  let registry = createDefaultRegistry();
  const secrets = new Map();
  const source = new LocalDaemonInstanceSource(
    () => registry,
    (next) => {
      registry = next;
    },
    {
      get: async (ref) => secrets.get(ref) || "",
      set: async (ref, value) => secrets.set(ref, value),
    },
    {
      shellUrl: "https://ctox.dev/business-os/",
      runEnsureCommand: async (profile) => ({
        instance_id: "local-instance",
        sync_room: "ctox-business-os:local-instance:abc",
        signaling_room_password: "local-room-secret",
        signaling_urls: ["wss://signaling.ctox.dev"],
        native_rxdb_peer_available: true,
        profile,
      }),
    },
  );

  const instance = await source.attachLocalDaemon({
    displayName: "Local CTOX",
    ctoxBinary: "ctox",
    ctoxRoot: "/Users/example/CTOX",
  });
  assert.equal(instance.source, "local_daemon");
  assert.equal(instance.connection.ctoxBinary, "ctox");
  assert.equal(instance.connection.ctoxRoot, "/Users/example/CTOX");
  assert.equal(registry.instances.length, 1);
  assert.equal(JSON.stringify(registry).includes("local-room-secret"), false);
  assert.equal(secrets.size, 1);

  const launch = await source.getLaunchConfig(instance.id);
  assert.equal(launch.ctoxConfig.transport, "webrtc");
  assert.equal(launch.ctoxConfig.http_bridge_available, false);
});

test("local fresh install runs installer then registers the ensured peer", async () => {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-desktop-local-fresh-"));
  const installScriptPath = path.join(tempRoot, "install.sh");
  const installedCtoxBinary = path.join(tempRoot, "bin", "ctox");
  fs.writeFileSync(installScriptPath, "#!/usr/bin/env bash\n", { mode: 0o700 });
  let registry = createDefaultRegistry();
  const secrets = new Map();
  const installs = [];
  const source = new LocalDaemonInstanceSource(
    () => registry,
    (next) => { registry = next; },
    {
      get: async (ref) => secrets.get(ref) || "",
      set: async (ref, value) => secrets.set(ref, value),
    },
    {
      runFreshInstallCommand: async (install) => {
        installs.push(install);
        return { ok: true, mode: "fresh" };
      },
      runEnsureCommand: async (profile) => ({
        instance_id: "installed-local",
        sync_room: "ctox-business-os:installed-local:room",
        signaling_room_password: "installed-room-secret",
        signaling_urls: ["wss://signaling.ctox.dev"],
        native_rxdb_peer_available: true,
        profile,
      }),
    },
  );

  const result = await source.installFresh({
    platform: "darwin",
    installScriptPath,
    installedCtoxBinary,
    displayName: "Installed Local",
  });
  assert.equal(result.install.mode, "fresh");
  assert.equal(result.instance.displayName, "Installed Local");
  assert.equal(result.instance.connection.ctoxBinary, installedCtoxBinary);
  assert.equal(installs.length, 1);
  assert.equal(registry.instances.length, 1);
  assert.equal(JSON.stringify(registry).includes("installed-room-secret"), false);
});

test("local daemon attach survives app restart without ctox.dev account", async () => {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-desktop-local-restart-"));
  const registryPath = path.join(tempRoot, "instances.json");
  const secrets = new Map();
  let registry = loadRegistry(registryPath);
  const secretStoreFactory = () => ({
    get: async (ref) => secrets.get(ref) || "",
    set: async (ref, value) => secrets.set(ref, value),
    delete: async (ref) => secrets.delete(ref),
  });
  const save = (next) => {
    registry = saveRegistry(registryPath, next);
  };
  const source = new LocalDaemonInstanceSource(
    () => registry,
    save,
    secretStoreFactory(),
    {
      shellUrl: "https://ctox.dev/business-os/",
      runEnsureCommand: async (profile) => ({
        instance_id: "restart-local",
        sync_room: "ctox-business-os:restart-local:room",
        signaling_room_password: "restart-room-secret",
        signaling_urls: ["wss://signaling.ctox.dev"],
        native_rxdb_peer_available: true,
        profile,
      }),
    },
  );

  const attached = await source.attachLocalDaemon({
    displayName: "Restart Local",
    ctoxRoot: path.join(tempRoot, "ctox-root"),
  });
  assert.equal(attached.source, "local_daemon");
  assert.equal(JSON.stringify(loadRegistry(registryPath)).includes("restart-room-secret"), false);
  assert.equal(secrets.size, 1);

  registry = loadRegistry(registryPath);
  const restartedSource = new LocalDaemonInstanceSource(
    () => registry,
    save,
    secretStoreFactory(),
    { shellUrl: "https://ctox.dev/business-os/" },
  );
  const [restarted] = restartedSource.listInstances();
  assert.equal(restarted.id, attached.id);
  assert.equal(restarted.displayName, "Restart Local");
  assert.equal(restarted.connection.ctoxRoot, path.join(tempRoot, "ctox-root"));

  const launch = await restartedSource.getLaunchConfig(attached.id);
  assert.equal(launch.ctoxConfig.transport, "webrtc");
  assert.equal(launch.ctoxConfig.signaling_room_password, "restart-room-secret");
  assert.equal(launch.ctoxConfig.http_bridge_available, false);

  await restartedSource.removeInstance(attached.id);
  assert.equal(loadRegistry(registryPath).instances.length, 0);
  assert.equal(secrets.size, 0);
});

test("local daemon inspection reports missing ctox binary without throwing", async () => {
  const source = new LocalDaemonInstanceSource(
    () => createDefaultRegistry(),
    () => undefined,
    {
      get: async () => "",
      set: async () => undefined,
    },
    {
      runStatusCommand: async () => {
        const error = new Error("spawn ctox ENOENT");
        error.code = "ENOENT";
        throw error;
      },
    },
  );

  const status = await source.inspectLocalDaemon({ ctoxBinary: "ctox" });
  assert.equal(status.status, "missing_binary");
  assert.equal(status.httpDataProxy, false);
  assert.equal(status.dataPlane, "rxdb-webrtc");
});

test("local daemon inspection exposes offline peer status", async () => {
  const source = new LocalDaemonInstanceSource(
    () => createDefaultRegistry(),
    () => undefined,
    {
      get: async () => "",
      set: async () => undefined,
    },
    {
      runStatusCommand: async () => ({
        instance_id: "local-instance",
        sync_room: "ctox-business-os:local-instance:abc",
        signaling_urls: ["wss://signaling.ctox.dev"],
        native_rxdb_peer_available: false,
      }),
    },
  );

  const status = await source.inspectLocalDaemon({});
  assert.equal(status.status, "offline");
  assert.equal(status.instanceId, "local-instance");
  assert.equal(status.httpDataProxy, false);
});
