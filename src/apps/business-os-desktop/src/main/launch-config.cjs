"use strict";

function encodeCtoxConfig(config) {
  return Buffer.from(JSON.stringify(config), "utf8").toString("base64url");
}

function buildLaunchUrl(shellUrl, config) {
  const url = new URL(shellUrl);
  url.searchParams.set("ctox_config", encodeCtoxConfig(config));
  return url.toString();
}

async function buildPairingLaunchConfig(instance, secretStore, options = {}) {
  if (!instance?.pairing) throw new Error("instance has no pairing metadata");
  const roomPassword = await secretStore.get(instance.pairing.secretRef);
  if (!roomPassword) throw new Error("pairing secret is missing");
  const ctoxConfig = {
    transport: "webrtc",
    sync_room: instance.pairing.syncRoom,
    signaling_urls: instance.pairing.signalingUrls,
    signaling_room_password: roomPassword,
    http_bridge_available: false,
  };
  return {
    source: instance.source,
    launchUrl: buildLaunchUrl(options.shellUrl || "https://ctox.dev/business-os/", ctoxConfig),
    ctoxConfig,
  };
}

module.exports = {
  encodeCtoxConfig,
  buildLaunchUrl,
  buildPairingLaunchConfig,
};

