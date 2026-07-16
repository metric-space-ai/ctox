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
  const authorizationRef = String(instance.pairing.authorizationRef || "").trim();
  const capabilityToken = authorizationRef ? await secretStore.get(authorizationRef) : "";
  if (authorizationRef && !capabilityToken) throw new Error("pairing authorization is missing");
  const capabilityExpiresAtMs = Number(instance.pairing.capabilityExpiresAtMs || 0);
  if (capabilityToken && capabilityExpiresAtMs > 0 && capabilityExpiresAtMs <= Date.now()) {
    throw new Error("pairing authorization is expired; reconnect with a fresh desktop invite");
  }
  const sessionUser = instance.pairing.sessionUser;
  const ctoxConfig = {
    transport: "webrtc",
    sync_room: instance.pairing.syncRoom,
    signaling_urls: instance.pairing.signalingUrls,
    signaling_room_password: roomPassword,
    http_bridge_available: false,
    ...(capabilityToken
      ? {
          session: {
            authenticated: true,
            source: instance.source === "pairing_invite" ? "desktop_invite" : `desktop_${instance.source}`,
            capability_token: capabilityToken,
            ...(capabilityExpiresAtMs > 0 ? { capability_expires_at_ms: capabilityExpiresAtMs } : {}),
            ...(sessionUser
              ? {
                  user: {
                    id: sessionUser.id,
                    display_name: sessionUser.displayName,
                    role: sessionUser.role,
                    is_admin: ["chef", "admin", "founder"].includes(sessionUser.role),
                  },
                }
              : {}),
          },
        }
      : {}),
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
