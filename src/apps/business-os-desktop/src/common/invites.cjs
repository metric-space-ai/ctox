"use strict";

const { normalizeInstance, stableId } = require("./instance-model.cjs");

function parseInvitePayload(rawInvite, now = new Date()) {
  const payload = typeof rawInvite === "string" ? parseInviteString(rawInvite) : rawInvite;
  validateInvite(payload, now);
  return payload;
}

function parseInviteString(rawInvite) {
  const input = String(rawInvite || "").trim();
  if (!input) throw new Error("invite is empty");
  if (input.startsWith("ctox-business-os-desktop://")) {
    const url = new URL(input);
    if (url.hostname !== "pair") throw new Error("unsupported desktop invite URL");
    const encoded = url.searchParams.get("payload");
    if (!encoded) throw new Error("desktop invite URL is missing payload");
    return JSON.parse(Buffer.from(encoded, "base64url").toString("utf8"));
  }
  return JSON.parse(input);
}

function validateInvite(invite, now = new Date()) {
  if (!invite || typeof invite !== "object" || Array.isArray(invite)) throw new Error("invite must be an object");
  if (invite.type !== "ctox-business-os-invite") throw new Error("unsupported invite type");
  if (Number(invite.version) !== 1) throw new Error("unsupported invite version");
  if (!String(invite.display_name || "").trim()) throw new Error("invite display_name is required");
  if (!String(invite.sync_room || "").startsWith("ctox-business-os:")) {
    throw new Error("invite sync_room must start with ctox-business-os:");
  }
  if (!Array.isArray(invite.signaling_urls) || invite.signaling_urls.length === 0) {
    throw new Error("invite needs signaling_urls");
  }
  if (!String(invite.signaling_room_password || "").trim()) {
    throw new Error("invite needs signaling_room_password");
  }
  if (invite.transport && invite.transport !== "webrtc") throw new Error("invite transport must be webrtc");
  if (invite.expires_at && Date.parse(invite.expires_at) <= now.getTime()) {
    throw new Error("invite is expired");
  }
}

function instanceFromInvite(invite) {
  validateInvite(invite);
  const instanceId = String(invite.instance_id || invite.sync_room.split(":")[1] || invite.display_name).trim();
  const id = `paired:${stableId(["pairing_invite", instanceId])}`;
  const secretRef = `keychain://ctox-business-os-desktop/${id}/room`;
  const instance = normalizeInstance({
    id,
    source: "pairing_invite",
    displayName: invite.display_name,
    instanceId,
    status: "available",
    pairing: {
      syncRoom: invite.sync_room,
      signalingUrls: invite.signaling_urls,
      secretRef,
    },
    secretRefs: [secretRef],
    healthSummary: {
      dataPlane: "rxdb-webrtc",
      dataPlaneReady: true,
      httpDataProxy: false,
      nativePeerObserved: true,
    },
  });
  return {
    instance,
    secretMaterial: [{ ref: secretRef, value: invite.signaling_room_password }],
  };
}

function manualPairingToInvite(options = {}) {
  const displayName = String(options.displayName || "").trim();
  const syncRoom = String(options.syncRoom || "").trim();
  const roomPassword = String(options.roomSecret || options.signalingRoomPassword || "").trim();
  const signalingUrls = normalizeSignalingUrls(options.signalingUrls || options.signalingUrl);
  if (!displayName) throw new Error("manual pairing displayName is required");
  const invite = {
    type: "ctox-business-os-invite",
    version: 1,
    display_name: displayName,
    instance_id: String(options.instanceId || displayName).trim(),
    sync_room: syncRoom,
    signaling_urls: signalingUrls,
    signaling_room_password: roomPassword,
    transport: "webrtc",
    expires_at: "2999-01-01T00:00:00.000Z",
  };
  validateInvite(invite);
  return invite;
}

function normalizeSignalingUrls(value) {
  const values = Array.isArray(value) ? value : String(value || "").split(/[\n,]/);
  return values.map((url) => String(url).trim()).filter(Boolean);
}

module.exports = {
  parseInvitePayload,
  validateInvite,
  instanceFromInvite,
  manualPairingToInvite,
};
