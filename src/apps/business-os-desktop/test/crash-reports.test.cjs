"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const {
  MAX_EXTRA_VALUE_LENGTH,
  configureCrashReporter,
  createCrashReportExtra,
  sanitizeCrashReportExtra,
  updateCrashReportExtra,
} = require("../src/main/crash-reports.cjs");
const { containsLikelySecret } = require("../src/main/redaction.cjs");

test("crash report extras are redacted, flattened and bounded", () => {
  const extra = sanitizeCrashReportExtra({
    activeInstanceId: "ssh:demo",
    launch: "https://ctox.dev/business-os?ctox_config=very-secret-config",
    nested: {
      signaling_room_password: "room-secret",
      authorization: "authorization: Bearer live-token",
      longValue: "x".repeat(MAX_EXTRA_VALUE_LENGTH + 100),
    },
  });
  const serialized = JSON.stringify(extra);
  assert.equal(serialized.includes("very-secret-config"), false);
  assert.equal(serialized.includes("room-secret"), false);
  assert.equal(serialized.includes("live-token"), false);
  assert.equal(containsLikelySecret(serialized), false);
  assert.equal(extra["nested.longValue"].length, MAX_EXTRA_VALUE_LENGTH);
});

test("crash report summary avoids serializing full registry secrets", () => {
  const extra = createCrashReportExtra({
    activeInstanceId: "ssh:demo",
    appInfo: {
      name: "CTOX Business OS Desktop",
      version: "0.1.0",
      platform: "darwin",
    },
    registry: {
      version: 1,
      instances: [{
        id: "ssh:demo",
        source: "ssh_managed",
        displayName: "VPS Demo",
        accidentally_added_token: "must-not-leak",
      }],
    },
  });
  const serialized = JSON.stringify(extra);
  assert.equal(extra.appName, "CTOX Business OS Desktop");
  assert.equal(extra.activeInstanceId, "ssh:demo");
  assert.equal(extra["registrySummary.instanceCount"], "1");
  assert.equal(extra["registrySummary.sources.ssh_managed"], "1");
  assert.equal(serialized.includes("must-not-leak"), false);
  assert.equal(serialized.includes("VPS Demo"), false);
  assert.equal(containsLikelySecret(serialized), false);
});

test("crash reporter starts with upload disabled and sanitized extras", () => {
  let startOptions;
  const result = configureCrashReporter({
    start: (options) => {
      startOptions = options;
    },
  }, {
    appInfo: { version: "0.1.0" },
    registry: {
      version: 1,
      instances: [{ id: "paired-a", source: "pairing_invite" }],
    },
  });
  assert.equal(result.ok, true);
  assert.equal(startOptions.uploadToServer, false);
  assert.equal(startOptions.extra["registrySummary.sources.pairing_invite"], "1");
});

test("crash reporter extra updates are sanitized before registration", () => {
  const calls = [];
  const updated = updateCrashReportExtra({
    addExtraParameter: (key, value) => calls.push([key, value]),
  }, {
    currentUrl: "https://ctox.dev/?ctox_config=abc123",
    ssh: "sshpass -p SuperDuper!2026 ssh ubuntu@example.com",
  });
  const serialized = JSON.stringify({ calls, updated });
  assert.equal(serialized.includes("abc123"), false);
  assert.equal(serialized.includes("SuperDuper!2026"), false);
  assert.equal(containsLikelySecret(serialized), false);
  assert.equal(calls.length, 2);
});

