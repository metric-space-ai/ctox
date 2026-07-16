"use strict";

const crypto = require("node:crypto");
const { SECRET_KEY_SUBSTRING_RE } = require("./secret-keys.cjs");

const SOURCE_KINDS = Object.freeze([
  "ctox_dev",
  "local_daemon",
  "ssh_managed",
  "pairing_invite",
]);

const STATUS_VALUES = Object.freeze([
  "available",
  "offline",
  "needs_auth",
  "pairing_expired",
  "installing",
  "error",
]);

const SECRET_KEY_RE = SECRET_KEY_SUBSTRING_RE;
const SECRET_KEY_ALLOWLIST = new Set(["secretRef", "secretRefs", "authorizationRef"]);
const DYNAMIC_KEY_PATHS = new Set(["registry.usage"]);

function stableId(parts) {
  const input = parts.map((part) => String(part || "").trim()).filter(Boolean).join(":");
  return crypto.createHash("sha256").update(input).digest("base64url").slice(0, 18);
}

function sourcePrefix(source) {
  switch (source) {
    case "ctox_dev":
      return "managed";
    case "local_daemon":
      return "local";
    case "ssh_managed":
      return "ssh";
    case "pairing_invite":
      return "paired";
    default:
      return "instance";
  }
}

function sessionPartitionFor(instance) {
  const source = assertSourceKind(instance.source);
  const localId = String(
    instance.id || stableId([source, instance.tenantId, instance.instanceId, instance.displayName]),
  ).trim() || "instance";
  // Derive the Electron session partition from a collision-resistant hash of the
  // EXACT (source, id) pair. A lossy lowercase/dash-fold/truncation slug could
  // collapse two distinct ids (e.g. `Tenant_SKF` vs `tenant_skf`, or ids longer
  // than the truncation limit) onto one partition, which is the Electron
  // session-isolation boundary -> cross-tenant cookie/IndexedDB leak. Hashing the
  // untruncated id makes every distinct instance get a distinct partition.
  return `persist:ctox-${sourcePrefix(source)}-${stableId([source, localId])}`;
}

function assertSourceKind(value) {
  if (!SOURCE_KINDS.includes(value)) {
    throw new Error(`unsupported instance source: ${value || "missing"}`);
  }
  return value;
}

function normalizeInstance(raw) {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    throw new Error("instance must be an object");
  }
  assertRegistrySafe(raw);
  const source = assertSourceKind(raw.source);
  const id = String(raw.id || stableId([source, raw.tenantId, raw.instanceId, raw.displayName])).trim();
  const displayName = String(raw.displayName || raw.domain || raw.tenantId || raw.instanceId || id).trim();
  if (!id) throw new Error("instance id is required");
  if (!displayName) throw new Error("instance displayName is required");
  const status = STATUS_VALUES.includes(raw.status) ? raw.status : "available";
  const instance = {
    id,
    source,
    displayName,
    status,
    // Always re-derive the partition from the trusted (id, source); never honor a
    // caller-/registry-supplied sessionPartition, which could be tampered to alias
    // another instance's partition and leak its cookies/IndexedDB.
    sessionPartition: sessionPartitionFor({ ...raw, id, source }),
    secretRefs: Array.isArray(raw.secretRefs) ? raw.secretRefs.map(String).filter(Boolean) : [],
  };
  copyString(raw, instance, "domain");
  copyString(raw, instance, "instanceId");
  copyString(raw, instance, "tenantId");
  copyString(raw, instance, "role");
  copyString(raw, instance, "lastUsedAt");
  if (raw.healthSummary && typeof raw.healthSummary === "object" && !Array.isArray(raw.healthSummary)) {
    instance.healthSummary = {
      dataPlane: raw.healthSummary.dataPlane === "rxdb-webrtc" ? "rxdb-webrtc" : "unknown",
      dataPlaneReady: Boolean(raw.healthSummary.dataPlaneReady),
      httpDataProxy: false,
      nativePeerObserved: Boolean(raw.healthSummary.nativePeerObserved),
    };
  }
  if (raw.pairing && typeof raw.pairing === "object" && !Array.isArray(raw.pairing)) {
    instance.pairing = normalizePairingMetadata(raw.pairing);
  }
  if (raw.connection && typeof raw.connection === "object" && !Array.isArray(raw.connection)) {
    instance.connection = normalizeConnectionMetadata(raw.connection);
  }
  return instance;
}

function copyString(source, target, key) {
  if (typeof source[key] === "string" && source[key].trim()) {
    target[key] = source[key].trim();
  }
}

function normalizePairingMetadata(pairing) {
  const syncRoom = String(pairing.syncRoom || pairing.sync_room || "").trim();
  const signalingUrls = Array.isArray(pairing.signalingUrls || pairing.signaling_urls)
    ? (pairing.signalingUrls || pairing.signaling_urls).map((url) => String(url).trim()).filter(Boolean)
    : [];
  if (!syncRoom.startsWith("ctox-business-os:")) {
    throw new Error("pairing syncRoom must start with ctox-business-os:");
  }
  if (signalingUrls.length === 0) {
    throw new Error("pairing needs at least one signaling URL");
  }
  return {
    syncRoom,
    signalingUrls,
    transport: "webrtc",
    secretRef: typeof pairing.secretRef === "string" ? pairing.secretRef : "",
    authorizationRef: typeof pairing.authorizationRef === "string" ? pairing.authorizationRef : "",
    capabilityExpiresAtMs: Number.isFinite(Number(pairing.capabilityExpiresAtMs))
      ? Number(pairing.capabilityExpiresAtMs)
      : 0,
    sessionUser: normalizePairingSessionUser(pairing.sessionUser),
  };
}

function normalizePairingSessionUser(user) {
  if (!user || typeof user !== "object" || Array.isArray(user)) return null;
  const id = String(user.id || "").trim();
  if (!id) return null;
  return {
    id,
    displayName: String(user.displayName || user.display_name || id).trim(),
    role: String(user.role || "user").trim(),
  };
}

function normalizeConnectionMetadata(connection) {
  const metadata = {};
  for (const key of [
    "host",
    "user",
    "installRoot",
    "ctoxBinary",
    "ctoxRoot",
    "hostKeyFingerprint",
    "hostKeyAlgorithm",
    "hostKeyType",
    "hostKeyScannedAt",
    "installMode",
    "installReleaseChannel",
    "lastInstallAt",
  ]) {
    if (typeof connection[key] === "string" && connection[key].trim()) {
      metadata[key] = connection[key].trim();
    }
  }
  const port = Number(connection.port);
  if (Number.isInteger(port) && port > 0 && port <= 65535) {
    metadata.port = port;
  }
  if (typeof connection.managedBy === "string" && connection.managedBy.trim()) {
    metadata.managedBy = connection.managedBy.trim();
  }
  return metadata;
}

function mergeInstances(groups) {
  const byId = new Map();
  for (const group of groups) {
    for (const instance of group || []) {
      const normalized = normalizeInstance(instance);
      byId.set(normalized.id, normalized);
    }
  }
  return Array.from(byId.values()).sort(compareInstances);
}

function compareInstances(left, right) {
  const leftLastUsed = Date.parse(left.lastUsedAt || "");
  const rightLastUsed = Date.parse(right.lastUsedAt || "");
  const leftHasLastUsed = Number.isFinite(leftLastUsed);
  const rightHasLastUsed = Number.isFinite(rightLastUsed);
  if (leftHasLastUsed || rightHasLastUsed) {
    if (leftHasLastUsed !== rightHasLastUsed) return leftHasLastUsed ? -1 : 1;
    if (leftLastUsed !== rightLastUsed) return rightLastUsed - leftLastUsed;
  }
  const leftName = left.displayName.toLowerCase();
  const rightName = right.displayName.toLowerCase();
  if (leftName !== rightName) return leftName < rightName ? -1 : 1;
  return left.id < right.id ? -1 : left.id > right.id ? 1 : 0;
}

function assertRegistrySafe(value, path = "registry") {
  if (!value || typeof value !== "object") return;
  if (Array.isArray(value)) {
    value.forEach((item, index) => assertRegistrySafe(item, `${path}[${index}]`));
    return;
  }
  for (const [key, entry] of Object.entries(value)) {
    if (!DYNAMIC_KEY_PATHS.has(path) && SECRET_KEY_RE.test(key) && !SECRET_KEY_ALLOWLIST.has(key)) {
      throw new Error(`registry contains secret-like key at ${path}.${key}`);
    }
    assertRegistrySafe(entry, `${path}.${key}`);
  }
}

module.exports = {
  SOURCE_KINDS,
  STATUS_VALUES,
  stableId,
  sessionPartitionFor,
  normalizeInstance,
  mergeInstances,
  compareInstances,
  assertRegistrySafe,
  normalizePairingMetadata,
  normalizeConnectionMetadata,
};
