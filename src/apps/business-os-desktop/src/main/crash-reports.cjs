"use strict";

const { redactSensitiveValue } = require("./redaction.cjs");

const MAX_EXTRA_VALUE_LENGTH = 2048;
const TRUNCATED_SUFFIX = "...[truncated]";

function configureCrashReporter(crashReporter, options = {}) {
  if (!crashReporter?.start) return { ok: false, reason: "missing crashReporter" };
  const extra = createCrashReportExtra(options);
  crashReporter.start({
    uploadToServer: false,
    ignoreSystemCrashHandler: false,
    extra,
  });
  return { ok: true, extra };
}

function updateCrashReportExtra(crashReporter, extra = {}) {
  if (!crashReporter?.addExtraParameter) return {};
  const sanitized = sanitizeCrashReportExtra(extra);
  for (const [key, value] of Object.entries(sanitized)) {
    crashReporter.addExtraParameter(key, value);
  }
  return sanitized;
}

function createCrashReportExtra({ registry, appInfo = {}, activeInstanceId = "" } = {}) {
  const sources = {};
  for (const instance of Array.isArray(registry?.instances) ? registry.instances : []) {
    const source = String(instance.source || "unknown");
    sources[source] = (sources[source] || 0) + 1;
  }
  return sanitizeCrashReportExtra({
    appName: appInfo.name || "CTOX Business OS Desktop Beta",
    appVersion: appInfo.version || "",
    platform: appInfo.platform || process.platform,
    activeInstanceId,
    registrySummary: {
      version: registry?.version || 0,
      instanceCount: Array.isArray(registry?.instances) ? registry.instances.length : 0,
      sources,
    },
  });
}

function sanitizeCrashReportExtra(extra = {}) {
  const redacted = redactSensitiveValue(extra);
  const flattened = {};
  flattenCrashExtra(redacted, "", flattened);
  return Object.fromEntries(Object.entries(flattened).map(([key, value]) => [
    sanitizeCrashExtraKey(key),
    truncateCrashExtraValue(value),
  ]));
}

function flattenCrashExtra(value, prefix, output) {
  if (value === null || value === undefined) return;
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
    output[prefix || "value"] = String(value);
    return;
  }
  if (Array.isArray(value)) {
    output[prefix || "value"] = JSON.stringify(value);
    return;
  }
  if (typeof value === "object") {
    for (const [key, entry] of Object.entries(value)) {
      const nextPrefix = prefix ? `${prefix}.${key}` : key;
      flattenCrashExtra(entry, nextPrefix, output);
    }
  }
}

function sanitizeCrashExtraKey(key) {
  return String(key || "value")
    .replace(/[^A-Za-z0-9_.:-]+/g, "_")
    .slice(0, 128) || "value";
}

function truncateCrashExtraValue(value) {
  const text = String(value || "");
  if (text.length <= MAX_EXTRA_VALUE_LENGTH) return text;
  return `${text.slice(0, MAX_EXTRA_VALUE_LENGTH - TRUNCATED_SUFFIX.length)}${TRUNCATED_SUFFIX}`;
}

module.exports = {
  MAX_EXTRA_VALUE_LENGTH,
  configureCrashReporter,
  createCrashReportExtra,
  sanitizeCrashReportExtra,
  updateCrashReportExtra,
};
