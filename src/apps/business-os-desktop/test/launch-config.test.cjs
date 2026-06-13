"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { buildPairingLaunchConfig } = require("../src/main/launch-config.cjs");

test("pairing launch config keeps data plane on webrtc and no http bridge", async () => {
  const launch = await buildPairingLaunchConfig(
    {
      source: "pairing_invite",
      pairing: {
        syncRoom: "ctox-business-os:kunde-x",
        signalingUrls: ["wss://signaling.ctox.dev"],
        secretRef: "keychain://ctox/room",
      },
    },
    { get: async () => "room-secret" },
    { shellUrl: "https://ctox.dev/business-os/" },
  );
  assert.equal(launch.ctoxConfig.transport, "webrtc");
  assert.equal(launch.ctoxConfig.http_bridge_available, false);
  assert.match(launch.launchUrl, /ctox_config=/);
});

