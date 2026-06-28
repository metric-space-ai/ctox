"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const {
  mergeInstances,
  normalizeInstance,
  sessionPartitionFor,
} = require("../src/common/instance-model.cjs");

test("normalizes mixed source instances with deterministic collision-free partitions", () => {
  const local = normalizeInstance({
    id: "local-main",
    source: "local_daemon",
    displayName: "Local CTOX",
  });
  assert.match(local.sessionPartition, /^persist:ctox-local-[A-Za-z0-9_-]{18}$/);
  // Deterministic: re-deriving yields the same partition.
  assert.equal(sessionPartitionFor(local), local.sessionPartition);

  const managed = normalizeInstance({
    id: "managed:tenant_skf",
    source: "ctox_dev",
    displayName: "SKF",
    tenantId: "tenant_skf",
  });
  assert.match(managed.sessionPartition, /^persist:ctox-managed-[A-Za-z0-9_-]{18}$/);
  assert.notEqual(managed.sessionPartition, local.sessionPartition);
});

test("case- or punctuation-variant ids never collide onto one partition", () => {
  const a = sessionPartitionFor({ id: "managed:Tenant_SKF", source: "ctox_dev" });
  const b = sessionPartitionFor({ id: "managed:tenant_skf", source: "ctox_dev" });
  const c = sessionPartitionFor({ id: "managed:tenant#1", source: "ctox_dev" });
  const d = sessionPartitionFor({ id: "managed:tenant 1", source: "ctox_dev" });
  assert.notEqual(a, b);
  assert.notEqual(c, d);
});

test("a caller-supplied sessionPartition is ignored and re-derived", () => {
  const instance = normalizeInstance({
    id: "local-main",
    source: "local_daemon",
    displayName: "Local CTOX",
    sessionPartition: "persist:ctox-managed-someone-elses-partition",
  });
  assert.equal(instance.sessionPartition, sessionPartitionFor({ id: "local-main", source: "local_daemon" }));
  assert.notEqual(instance.sessionPartition, "persist:ctox-managed-someone-elses-partition");
});

test("mergeInstances keeps all source kinds in one sorted list", () => {
  const merged = mergeInstances([
    [{ id: "paired-a", source: "pairing_invite", displayName: "Kunde X" }],
    [{ id: "ssh-a", source: "ssh_managed", displayName: "VPS Demo" }],
    [{ id: "managed-a", source: "ctox_dev", displayName: "Example" }],
  ]);
  assert.deepEqual(merged.map((instance) => instance.source).sort(), [
    "ctox_dev",
    "pairing_invite",
    "ssh_managed",
  ]);
});

test("secret-like instance fields are rejected", () => {
  assert.throws(
    () => normalizeInstance({
      id: "bad",
      source: "local_daemon",
      displayName: "Bad",
      roomSecret: "cleartext",
    }),
    /secret-like key/,
  );
});

test("broadened secret-like keys are rejected while public host-key fields are allowed", () => {
  for (const key of ["apiKey", "accessKey", "passphrase", "sessionCookie", "authorization"]) {
    assert.throws(
      () => normalizeInstance({ id: "bad", source: "local_daemon", displayName: "Bad", [key]: "x" }),
      /secret-like key/,
      `expected ${key} to be rejected`,
    );
  }
  // Host-key fingerprints are public and must remain persistable.
  assert.doesNotThrow(() => normalizeInstance({
    id: "ok",
    source: "ssh_managed",
    displayName: "VPS",
    connection: { host: "h", user: "u", hostKeyFingerprint: "SHA256:abc" },
  }));
});

