"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { assertRegistrySafe, normalizeInstance } = require("../common/instance-model.cjs");
const { isLoopbackHost } = require("./url-safety.cjs");

function createDefaultRegistry() {
  return {
    version: 1,
    instances: [],
    usage: {},
    settings: {
      ctoxDevBaseUrl: "https://ctox.dev",
      shellUrl: "https://ctox.dev/business-os/",
    },
  };
}

function loadRegistry(filePath) {
  if (!fs.existsSync(filePath)) return createDefaultRegistry();
  let raw;
  try {
    raw = fs.readFileSync(filePath, "utf8");
  } catch (error) {
    console.error("failed to read instance registry; using defaults", error?.message || error);
    return createDefaultRegistry();
  }
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch (error) {
    // A truncated/corrupt registry (e.g. from a crash mid-write) must never brick
    // app startup. Preserve the bad file for forensics and start empty so the user
    // can re-add instances instead of facing a dead window.
    backupBrokenRegistry(filePath, "corrupt");
    console.error("instance registry is corrupt; starting with an empty registry", error?.message || error);
    return createDefaultRegistry();
  }
  return normalizeRegistry(parsed);
}

function backupBrokenRegistry(filePath, kind) {
  try {
    fs.renameSync(filePath, `${filePath}.${kind}-${Date.now()}`);
  } catch (_error) {
    // best-effort; leave the original in place if the rename fails
  }
}

function saveRegistry(filePath, registry) {
  const normalized = normalizeRegistry(registry);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  // Write atomically (temp file + rename) so a crash or power loss mid-write can
  // never leave a truncated instances.json that bricks the next startup.
  const tempPath = `${filePath}.${process.pid}.tmp`;
  fs.writeFileSync(tempPath, `${JSON.stringify(normalized, null, 2)}\n`);
  fs.renameSync(tempPath, filePath);
  return normalized;
}

function normalizeRegistry(registry) {
  const base = createDefaultRegistry();
  const input = registry && typeof registry === "object" ? registry : {};
  const next = {
    ...base,
    ...input,
    settings: {
      ...base.settings,
      ...(input.settings && typeof input.settings === "object" ? input.settings : {}),
    },
  };
  next.instances = Array.isArray(next.instances)
    ? next.instances.map((instance) => normalizeInstance(instance))
    : [];
  next.usage = normalizeUsage(next.usage);
  next.settings.ctoxDevBaseUrl = cleanUrl(next.settings.ctoxDevBaseUrl, "https://ctox.dev");
  next.settings.shellUrl = cleanUrl(next.settings.shellUrl, "https://ctox.dev/business-os/");
  assertRegistrySafe(next);
  return next;
}

function cleanUrl(value, fallback) {
  const raw = String(value || "").trim();
  if (!raw) return fallback;
  const parsed = new URL(raw);
  assertAllowedControlPlaneUrl(parsed);
  return parsed.toString();
}

// The ctox.dev control-plane / shell base URLs drive login, session-package,
// launch-token and management requests that carry ctox.dev cookies. Pin them to
// https on ctox.dev so a tampered instances.json cannot repoint the whole managed
// flow at an attacker host or downgrade it to cleartext http. http loopback is
// permitted only for local development and the in-process test mocks.
function assertAllowedControlPlaneUrl(parsed) {
  const host = parsed.hostname.toLowerCase();
  if (parsed.protocol === "https:" && (host === "ctox.dev" || host.endsWith(".ctox.dev"))) return;
  if (parsed.protocol === "http:" && isLoopbackHost(host)) return;
  throw new Error("registry control-plane URL must be https on ctox.dev (or http loopback for local dev)");
}

function upsertInstance(registry, instance) {
  const normalized = normalizeInstance(instance);
  const next = normalizeRegistry(registry);
  const index = next.instances.findIndex((entry) => entry.id === normalized.id);
  if (index >= 0) next.instances[index] = normalized;
  else next.instances.push(normalized);
  return normalizeRegistry(next);
}

function removeInstance(registry, id) {
  const next = normalizeRegistry(registry);
  next.instances = next.instances.filter((entry) => entry.id !== id);
  delete next.usage[String(id || "")];
  return next;
}

function markInstanceUsed(registry, id, now = new Date()) {
  const instanceId = String(id || "").trim();
  if (!instanceId) throw new Error("instance id is required");
  const usedAt = now instanceof Date ? now : new Date(now);
  if (!Number.isFinite(usedAt.getTime())) throw new Error("last used timestamp is invalid");
  const next = normalizeRegistry(registry);
  next.usage[instanceId] = { lastUsedAt: usedAt.toISOString() };
  return normalizeRegistry(next);
}

function applyUsageToInstances(instances, registry) {
  const usage = normalizeUsage(registry?.usage);
  return instances.map((instance) => {
    const lastUsedAt = usage[instance.id]?.lastUsedAt;
    return lastUsedAt ? normalizeInstance({ ...instance, lastUsedAt }) : normalizeInstance(instance);
  });
}

function normalizeUsage(usage) {
  if (!usage || typeof usage !== "object" || Array.isArray(usage)) return {};
  const normalized = {};
  for (const [id, value] of Object.entries(usage)) {
    const instanceId = String(id || "").trim();
    const lastUsedAt = typeof value?.lastUsedAt === "string" ? value.lastUsedAt.trim() : "";
    if (!instanceId || !Number.isFinite(Date.parse(lastUsedAt))) continue;
    normalized[instanceId] = { lastUsedAt: new Date(lastUsedAt).toISOString() };
  }
  return normalized;
}

module.exports = {
  createDefaultRegistry,
  loadRegistry,
  saveRegistry,
  normalizeRegistry,
  upsertInstance,
  removeInstance,
  markInstanceUsed,
  applyUsageToInstances,
};

