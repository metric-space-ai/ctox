"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { assertRegistrySafe, normalizeInstance } = require("../common/instance-model.cjs");

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
  return normalizeRegistry(JSON.parse(fs.readFileSync(filePath, "utf8")));
}

function saveRegistry(filePath, registry) {
  const normalized = normalizeRegistry(registry);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, `${JSON.stringify(normalized, null, 2)}\n`);
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
  if (!["https:", "http:"].includes(parsed.protocol)) throw new Error("registry URL must be http or https");
  return parsed.toString();
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

