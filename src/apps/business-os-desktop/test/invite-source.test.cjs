"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { createDefaultRegistry } = require("../src/main/registry.cjs");
const { PairingInviteInstanceSource } = require("../src/main/sources.cjs");
const { parseInvitePayload } = require("../src/common/invites.cjs");
const { stableId } = require("../src/common/instance-model.cjs");

test("manual pairing import stores secret material outside registry", async () => {
  let registry = createDefaultRegistry();
  const secrets = new Map();
  const source = new PairingInviteInstanceSource(
    () => registry,
    (next) => {
      registry = next;
    },
    {
      get: async (ref) => secrets.get(ref) || "",
      set: async (ref, value) => secrets.set(ref, value),
      delete: async (ref) => secrets.delete(ref),
    },
  );
  const instance = await source.importManualPairing({
    displayName: "Kunde X",
    syncRoom: "ctox-business-os:kunde-x",
    signalingUrl: "wss://signaling.ctox.dev",
    roomSecret: "room-secret",
  });
  assert.equal(instance.source, "pairing_invite");
  assert.equal(JSON.stringify(registry).includes("room-secret"), false);
  assert.equal(secrets.size, 1);
});

test("desktop CLI invite contract imports through JSON and desktop link", () => {
  const invite = {
    type: "ctox-business-os-invite",
    version: 1,
    display_name: "CLI Lab",
    instance_id: "cli_lab",
    sync_room: "ctox-business-os:cli_lab:room",
    signaling_urls: ["wss://signaling.ctox.dev"],
    signaling_room_password: "room-secret",
    transport: "webrtc",
    expires_at: "2099-01-01T00:00:00.000Z",
    data_plane: "rxdb-webrtc",
    http_bridge_available: false,
    secret_value_in_payload: true,
  };
  const parsedJson = parseInvitePayload(invite);
  assert.equal(parsedJson.display_name, "CLI Lab");
  const payload = Buffer.from(JSON.stringify(invite), "utf8").toString("base64url");
  const parsedLink = parseInvitePayload(`ctox-business-os-desktop://pair?payload=${payload}`);
  assert.equal(parsedLink.sync_room, "ctox-business-os:cli_lab:room");
  assert.equal(parsedLink.transport, "webrtc");
});

test("pairing rotation replaces only matching invite secret and revoke clears local state", async () => {
  let registry = createDefaultRegistry();
  const secrets = new Map();
  const source = new PairingInviteInstanceSource(
    () => registry,
    (next) => {
      registry = next;
    },
    {
      get: async (ref) => secrets.get(ref) || "",
      set: async (ref, value) => secrets.set(ref, value),
      delete: async (ref) => secrets.delete(ref),
    },
  );
  const instance = await source.importInvite(JSON.stringify({
    type: "ctox-business-os-invite",
    version: 1,
    display_name: "CLI Lab",
    instance_id: "cli_lab",
    sync_room: "ctox-business-os:cli_lab:room",
    signaling_urls: ["wss://signaling.ctox.dev"],
    signaling_room_password: "old-room-secret",
    transport: "webrtc",
    expires_at: "2099-01-01T00:00:00.000Z",
  }));

  const rotated = await source.rotateInvite(instance.id, JSON.stringify({
    type: "ctox-business-os-invite",
    version: 1,
    display_name: "CLI Lab",
    instance_id: "cli_lab",
    sync_room: "ctox-business-os:cli_lab:rotated-room",
    signaling_urls: ["wss://signaling.ctox.dev", "wss://backup.signaling.ctox.dev"],
    signaling_room_password: "new-room-secret",
    transport: "webrtc",
    expires_at: "2099-01-01T00:00:00.000Z",
  }));
  assert.equal(rotated.id, instance.id);
  assert.equal(JSON.stringify(registry).includes("new-room-secret"), false);
  assert.equal(secrets.get(rotated.pairing.secretRef), "new-room-secret");
  const launch = await source.getLaunchConfig(instance.id);
  assert.equal(launch.ctoxConfig.signaling_room_password, "new-room-secret");
  assert.deepEqual(launch.ctoxConfig.signaling_urls, [
    "wss://signaling.ctox.dev",
    "wss://backup.signaling.ctox.dev",
  ]);

  await assert.rejects(
    () => source.rotateInvite(instance.id, JSON.stringify({
      type: "ctox-business-os-invite",
      version: 1,
      display_name: "Other Lab",
      instance_id: "other_lab",
      sync_room: "ctox-business-os:other_lab:room",
      signaling_urls: ["wss://signaling.ctox.dev"],
      signaling_room_password: "other-secret",
      transport: "webrtc",
      expires_at: "2099-01-01T00:00:00.000Z",
    })),
    /does not match/,
  );
  assert.equal(secrets.get(rotated.pairing.secretRef), "new-room-secret");

  await source.revokeInstance(instance.id);
  assert.equal(registry.instances.length, 0);
  assert.equal(secrets.has(rotated.pairing.secretRef), false);
});

test("pairing rotation migrates legacy sync-room-scoped ids", async () => {
  const oldId = `paired:${stableId(["pairing_invite", "cli_lab", "ctox-business-os:cli_lab:old-room"])}`;
  const oldSecretRef = `keychain://ctox-business-os-desktop/${oldId}/room`;
  let registry = {
    ...createDefaultRegistry(),
    instances: [{
      id: oldId,
      source: "pairing_invite",
      displayName: "CLI Lab",
      instanceId: "cli_lab",
      status: "available",
      pairing: {
        syncRoom: "ctox-business-os:cli_lab:old-room",
        signalingUrls: ["wss://signaling.ctox.dev"],
        secretRef: oldSecretRef,
      },
      secretRefs: [oldSecretRef],
    }],
  };
  const secrets = new Map([[oldSecretRef, "old-room-secret"]]);
  const source = new PairingInviteInstanceSource(
    () => registry,
    (next) => {
      registry = next;
    },
    {
      get: async (ref) => secrets.get(ref) || "",
      set: async (ref, value) => secrets.set(ref, value),
      delete: async (ref) => secrets.delete(ref),
    },
  );

  const rotated = await source.rotateInvite(oldId, JSON.stringify({
    type: "ctox-business-os-invite",
    version: 1,
    display_name: "CLI Lab",
    instance_id: "cli_lab",
    sync_room: "ctox-business-os:cli_lab:new-room",
    signaling_urls: ["wss://signaling.ctox.dev"],
    signaling_room_password: "new-room-secret",
    transport: "webrtc",
    expires_at: "2099-01-01T00:00:00.000Z",
  }));

  assert.notEqual(rotated.id, oldId);
  assert.equal(registry.instances.length, 1);
  assert.equal(registry.instances[0].id, rotated.id);
  assert.equal(secrets.has(oldSecretRef), false);
  assert.equal(secrets.get(rotated.pairing.secretRef), "new-room-secret");
});
