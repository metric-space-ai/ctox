"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const {
  REDACTED,
  containsLikelySecret,
  createSupportBundleSnapshot,
  redactSensitiveText,
  redactSensitiveValue,
} = require("../src/main/redaction.cjs");

test("redacts sensitive URL, auth and ssh command text", () => {
  const input = [
    "https://ctox.dev/business-os?ctox_config=very-secret-config&x=1",
    "authorization: Bearer live-token",
    "sshpass -p SuperDuper!2026 ssh ubuntu@example.com",
    "ssh://ubuntu:secret-password@example.com",
  ].join("\n");
  const output = redactSensitiveText(input);
  assert.equal(output.includes("very-secret-config"), false);
  assert.equal(output.includes("live-token"), false);
  assert.equal(output.includes("SuperDuper!2026"), false);
  assert.equal(output.includes("secret-password"), false);
  assert.equal(containsLikelySecret(output), false);
});

test("redacts private key blocks", () => {
  const output = redactSensitiveText([
    "before",
    "-----BEGIN OPENSSH PRIVATE KEY-----",
    "private-key-material",
    "-----END OPENSSH PRIVATE KEY-----",
    "after",
  ].join("\n"));
  assert.equal(output.includes("private-key-material"), false);
  assert.ok(output.includes(REDACTED));
});

test("redacts nested secret-like object keys recursively", () => {
  const output = redactSensitiveValue({
    displayName: "VPS Demo",
    room_password: "room-secret",
    nested: {
      launch_token: "launch-secret",
      list: [{ ctox_config: "config-secret" }],
    },
  });
  assert.equal(output.displayName, "VPS Demo");
  assert.equal(output.room_password, REDACTED);
  assert.equal(output.nested.launch_token, REDACTED);
  assert.equal(output.nested.list[0].ctox_config, REDACTED);
});

test("redacts broadened credential key names but not public host-key fields", () => {
  const output = redactSensitiveValue({
    apiKey: "live-abc",
    api_key: "live-def",
    accessKey: "AKIA123",
    passphrase: "open-sesame",
    authorization: "Bearer xyz",
    sessionCookie: "sid=secret",
    // Public, non-secret fields must survive redaction.
    hostKeyFingerprint: "SHA256:abcdef",
    displayName: "VPS Demo",
  });
  assert.equal(output.apiKey, REDACTED);
  assert.equal(output.api_key, REDACTED);
  assert.equal(output.accessKey, REDACTED);
  assert.equal(output.passphrase, REDACTED);
  assert.equal(output.authorization, REDACTED);
  assert.equal(output.sessionCookie, REDACTED);
  assert.equal(output.hostKeyFingerprint, "SHA256:abcdef");
  assert.equal(output.displayName, "VPS Demo");
});

test("support bundle snapshot does not include known secret values", () => {
  const snapshot = createSupportBundleSnapshot({
    now: new Date("2026-06-12T10:00:00.000Z"),
    appInfo: { version: "0.1.0", platform: "darwin" },
    registry: {
      instances: [{
        id: "ssh:demo",
        source: "ssh_managed",
        displayName: "VPS Demo",
        secretRefs: ["keychain://ctox-business-os-desktop/ssh-demo/room"],
        accidentally_added_token: "must-not-leak",
      }],
    },
    logs: [
      "ctox_config=abc123",
      "\"signaling_room_password\":\"room-secret\"",
      "authorization: Basic dXNlcjpwYXNz",
    ],
  });
  const serialized = JSON.stringify(snapshot);
  assert.equal(serialized.includes("must-not-leak"), false);
  assert.equal(serialized.includes("abc123"), false);
  assert.equal(serialized.includes("room-secret"), false);
  assert.equal(serialized.includes("dXNlcjpwYXNz"), false);
  assert.equal(containsLikelySecret(serialized), false);
});
