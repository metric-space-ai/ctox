"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const {
  applyUsageToInstances,
  createDefaultRegistry,
  markInstanceUsed,
  normalizeRegistry,
  removeInstance,
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

