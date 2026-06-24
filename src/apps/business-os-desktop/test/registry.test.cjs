"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const {
  applyUsageToInstances,
  createDefaultRegistry,
  loadRegistry,
  markInstanceUsed,
  normalizeRegistry,
  removeInstance,
  saveRegistry,
  upsertInstance,
} = require("../src/main/registry.cjs");

test("registry stores mixed instances without secrets", () => {
  const registry = upsertInstance(createDefaultRegistry(), {
    id: "paired-a",
    source: "pairing_invite",
    displayName: "Kunde X",
    pairing: {
      syncRoom: "ctox-business-os:kunde-x",
      signalingUrls: ["wss://signaling.ctox.dev"],
      secretRef: "keychain://ctox-business-os-desktop/paired-a/room",
    },
    secretRefs: ["keychain://ctox-business-os-desktop/paired-a/room"],
  });
  assert.equal(JSON.stringify(registry).includes("room-secret"), false);
  assert.equal(registry.instances[0].pairing.transport, "webrtc");
});

test("registry rejects cleartext secret fields", () => {
  assert.throws(
    () => normalizeRegistry({
      instances: [{
        id: "bad",
        source: "local_daemon",
        displayName: "Bad",
        signaling_room_password: "cleartext",
      }],
    }),
    /secret-like key/,
  );
});

test("a corrupt registry file falls back to an empty registry instead of bricking startup", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-registry-"));
  const filePath = path.join(dir, "instances.json");
  fs.writeFileSync(filePath, "{ this is not valid json");
  const registry = loadRegistry(filePath);
  assert.deepEqual(registry.instances, []);
  // The broken file is preserved for forensics, not silently destroyed.
  const backups = fs.readdirSync(dir).filter((name) => name.startsWith("instances.json.corrupt-"));
  assert.equal(backups.length, 1);
  fs.rmSync(dir, { recursive: true, force: true });
});

test("saveRegistry writes atomically and round-trips", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-registry-"));
  const filePath = path.join(dir, "instances.json");
  saveRegistry(filePath, upsertInstance(createDefaultRegistry(), {
    id: "local-a",
    source: "local_daemon",
    displayName: "Local",
  }));
  // No temp file is left behind after an atomic rename.
  assert.deepEqual(fs.readdirSync(dir), ["instances.json"]);
  const reloaded = loadRegistry(filePath);
  assert.equal(reloaded.instances[0].id, "local-a");
  fs.rmSync(dir, { recursive: true, force: true });
});

test("control-plane base URLs are pinned to https on ctox.dev", () => {
  // Loopback http is allowed for local dev / test mocks.
  assert.doesNotThrow(() => normalizeRegistry({ settings: { ctoxDevBaseUrl: "http://127.0.0.1:8765" } }));
  assert.doesNotThrow(() => normalizeRegistry({ settings: { ctoxDevBaseUrl: "https://ctox.dev" } }));
  // An attacker-supplied off-host or cleartext base is rejected.
  assert.throws(
    () => normalizeRegistry({ settings: { ctoxDevBaseUrl: "https://evil.example" } }),
    /control-plane URL must be https on ctox\.dev/,
  );
  assert.throws(
    () => normalizeRegistry({ settings: { ctoxDevBaseUrl: "http://ctox.dev" } }),
    /control-plane URL must be https on ctox\.dev/,
  );
});

test("usage is separate from instance metadata", () => {
  let registry = upsertInstance(createDefaultRegistry(), {
    id: "local-a",
    source: "local_daemon",
    displayName: "Local",
  });
  registry = markInstanceUsed(registry, "local-a", new Date("2026-06-13T00:00:00Z"));
  const [instance] = applyUsageToInstances(registry.instances, registry);
  assert.equal(instance.lastUsedAt, "2026-06-13T00:00:00.000Z");
  registry = removeInstance(registry, "local-a");
  assert.equal(registry.usage["local-a"], undefined);
});

