"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const {
  mergeInstances,
  normalizeInstance,
  sessionPartitionFor,
} = require("../src/common/instance-model.cjs");

test("normalizes mixed source instances with deterministic partitions", () => {
  const local = normalizeInstance({
    id: "local-main",
    source: "local_daemon",
    displayName: "Local CTOX",
  });
  assert.equal(local.sessionPartition, "persist:ctox-local-local-main");
  assert.equal(sessionPartitionFor(local), local.sessionPartition);

  const managed = normalizeInstance({
    id: "managed:tenant_skf",
    source: "ctox_dev",
    displayName: "SKF",
    tenantId: "tenant_skf",
  });
  assert.equal(managed.sessionPartition, "persist:ctox-managed-managed:tenant_skf");
});

test("mergeInstances keeps all source kinds in one sorted list", () => {
  const merged = mergeInstances([
    [{ id: "paired-a", source: "pairing_invite", displayName: "Kunde X" }],
    [{ id: "ssh-a", source: "ssh_managed", displayName: "VPS Demo" }],
    [{ id: "managed-a", source: "ctox_dev", displayName: "Kunstmen" }],
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

