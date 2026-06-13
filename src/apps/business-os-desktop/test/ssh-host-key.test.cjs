"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const {
  buildSshKeyscanArgs,
  ensureKnownHost,
  inspectSshHostKey,
  knownHostPattern,
  parseSshKeyscanOutput,
  verifyTrustedHostKey,
} = require("../src/main/ssh-host-key.cjs");

const ED25519_KEY = Buffer.from("ctox-test-ed25519-host-key").toString("base64");
const RSA_KEY = Buffer.from("ctox-test-rsa-host-key").toString("base64");
const EXPECTED_ED25519_FINGERPRINT = `SHA256:${crypto
  .createHash("sha256")
  .update(Buffer.from(ED25519_KEY, "base64"))
  .digest("base64")
  .replace(/=+$/, "")}`;

test("ssh-keyscan args respect host and non-standard port", () => {
  assert.deepEqual(buildSshKeyscanArgs({ host: "51.210.246.120", port: 2222 }), [
    "-p",
    "2222",
    "-T",
    "10",
    "-t",
    "ed25519,ecdsa,rsa",
    "51.210.246.120",
  ]);
});

test("host key inspection prefers ed25519 and returns sha256 fingerprint", async () => {
  const output = [
    `example.com ssh-rsa ${RSA_KEY}`,
    `example.com ssh-ed25519 ${ED25519_KEY}`,
  ].join("\n");
  const hostKey = await inspectSshHostKey(
    { host: "example.com", port: 22 },
    { runKeyscan: async () => output },
  );
  assert.equal(hostKey.keyType, "ssh-ed25519");
  assert.equal(hostKey.fingerprint, EXPECTED_ED25519_FINGERPRINT);
  assert.equal(hostKey.knownHostsLine, `example.com ssh-ed25519 ${ED25519_KEY}`);
});

test("host key parser ignores unsupported and malformed lines", () => {
  const parsed = parseSshKeyscanOutput([
    "# banner",
    "example.com ssh-dss AAAA",
    "malformed",
    `example.com ssh-ed25519 ${ED25519_KEY}`,
  ].join("\n"), { host: "example.com", port: 22 });
  assert.equal(parsed.length, 1);
  assert.equal(parsed[0].fingerprint, EXPECTED_ED25519_FINGERPRINT);
});

test("host key trust requires exact confirmed fingerprint", () => {
  const inspected = { fingerprint: EXPECTED_ED25519_FINGERPRINT };
  assert.equal(verifyTrustedHostKey(inspected, EXPECTED_ED25519_FINGERPRINT), true);
  assert.equal(verifyTrustedHostKey(inspected, EXPECTED_ED25519_FINGERPRINT.replace(/^SHA256:/, "")), true);
  assert.throws(() => verifyTrustedHostKey(inspected, ""), /confirmation is required/);
  assert.throws(() => verifyTrustedHostKey(inspected, "SHA256:different"), /fingerprint mismatch/);
});

test("ensureKnownHost writes app-owned known_hosts entries with port-aware host pattern", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-known-hosts-"));
  const knownHostsPath = path.join(dir, "known_hosts");
  ensureKnownHost({
    knownHostsPath,
    host: "example.com",
    port: 2222,
    knownHostsLine: `[example.com]:2222 ssh-ed25519 ${ED25519_KEY}`,
  });
  assert.equal(fs.readFileSync(knownHostsPath, "utf8"), `[example.com]:2222 ssh-ed25519 ${ED25519_KEY}\n`);
  ensureKnownHost({
    knownHostsPath,
    host: "example.com",
    port: 2222,
    knownHostsLine: `[example.com]:2222 ssh-rsa ${RSA_KEY}`,
  });
  assert.equal(fs.readFileSync(knownHostsPath, "utf8"), `[example.com]:2222 ssh-rsa ${RSA_KEY}\n`);
  assert.equal((fs.statSync(knownHostsPath).mode & 0o777), 0o600);
});

test("known host pattern keeps default port simple", () => {
  assert.equal(knownHostPattern({ host: "example.com", port: 22 }), "example.com");
  assert.equal(knownHostPattern({ host: "example.com", port: 2022 }), "[example.com]:2022");
});
