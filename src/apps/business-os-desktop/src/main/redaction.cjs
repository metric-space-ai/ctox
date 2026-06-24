"use strict";

const { SECRET_KEY_SEGMENT_RE } = require("../common/secret-keys.cjs");

const SECRET_KEY_RE = SECRET_KEY_SEGMENT_RE;
const REDACTED = "[REDACTED]";

function redactSensitiveText(value) {
  return String(value || "")
    .replace(/-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----/g, REDACTED)
    .replace(/\b(authorization:\s*(?:bearer|basic)\s+)[^\s"']+/gi, `$1${REDACTED}`)
    .replace(/\b(ctox_config=)[^&\s"']+/gi, `$1${REDACTED}`)
    .replace(/\b((?:password|token|secret|credential|room_password)=)[^&\s"']+/gi, `$1${REDACTED}`)
    .replace(/\b(sshpass\s+-p\s+)(?:"[^"]*"|'[^']*'|[^\s]+)/gi, `$1${REDACTED}`)
    .replace(/\b(ssh:\/\/[^:\s/@]+:)[^@\s/]+(@)/gi, `$1${REDACTED}$2`)
    .replace(/"([^"]*(?:password|token|secret|credential|private_key|room_password|ctox_config)[^"]*)"\s*:\s*"[^"]*"/gi, (_match, key) => `"${key}":"${REDACTED}"`)
    .replace(/'([^']*(?:password|token|secret|credential|private_key|room_password|ctox_config)[^']*)'\s*:\s*'[^']*'/gi, (_match, key) => `'${key}':'${REDACTED}'`);
}

function redactSensitiveValue(value) {
  return redactValueAtPath(value);
}

function redactValueAtPath(value, key = "") {
  if (value === null || value === undefined) return value;
  if (typeof value === "string") {
    return SECRET_KEY_RE.test(key) ? REDACTED : redactSensitiveText(value);
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return SECRET_KEY_RE.test(key) ? REDACTED : value;
  }
  if (Array.isArray(value)) {
    return value.map((entry) => redactValueAtPath(entry, key));
  }
  if (typeof value === "object") {
    const redacted = {};
    for (const [entryKey, entryValue] of Object.entries(value)) {
      redacted[entryKey] = SECRET_KEY_RE.test(entryKey)
        ? REDACTED
        : redactValueAtPath(entryValue, entryKey);
    }
    return redacted;
  }
  return value;
}

function createSupportBundleSnapshot({ registry, logs = [], appInfo = {}, now = new Date() } = {}) {
  return redactSensitiveValue({
    type: "ctox-business-os-desktop-support-snapshot",
    version: 1,
    generatedAt: now.toISOString(),
    appInfo,
    registry,
    logs: Array.isArray(logs) ? logs.map((entry) => redactSensitiveText(entry)) : [],
  });
}

function containsLikelySecret(value) {
  const text = String(value || "");
  return [
    /ctox_config=(?!\[REDACTED\])[^&\s"']+/i,
    /signaling_room_password["']?\s*[:=]\s*(?!["']?\[REDACTED\])/i,
    /-----BEGIN [A-Z ]*PRIVATE KEY-----/,
    /\bauthorization:\s*(?:bearer|basic)\s+(?!\[REDACTED\])[^\s"']+/i,
    /\bsshpass\s+-p\s+(?!"?\[REDACTED\]"?)(?:"[^"]*"|'[^']*'|[^\s]+)/i,
  ].some((pattern) => pattern.test(text));
}

module.exports = {
  REDACTED,
  redactSensitiveText,
  redactSensitiveValue,
  createSupportBundleSnapshot,
  containsLikelySecret,
};
