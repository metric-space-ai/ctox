"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const {
  badgeSearchText,
  badgesForInstance,
  healthBadge,
} = require("../src/common/instance-badges.js");

test("badges include source, role, status and healthy rxdb state", () => {
  const badges = badgesForInstance({
    source: "ctox_dev",
    role: "owner",
    status: "available",
    healthSummary: {
      dataPlane: "rxdb-webrtc",
      dataPlaneReady: true,
      httpDataProxy: false,
      nativePeerObserved: true,
    },
  });
  assert.deepEqual(badges.map((badge) => badge.label), [
    "ctox.dev",
    "owner",
    "online",
    "rxdb",
  ]);
  assert.deepEqual(badges.map((badge) => badge.tone), [
    "managed",
    "neutral",
    "ok",
    "ok",
  ]);
});

test("badges distinguish unmanaged offline and pending sync states", () => {
  const badges = badgesForInstance({
    source: "ssh_managed",
    status: "offline",
    healthSummary: {
      dataPlane: "rxdb-webrtc",
      dataPlaneReady: false,
      httpDataProxy: false,
      nativePeerObserved: false,
    },
  });
  assert.deepEqual(badges.map((badge) => badge.label), [
    "ssh",
    "offline",
    "sync pending",
  ]);
  assert.equal(badgeSearchText({ source: "ssh_managed", status: "offline" }).includes("ssh"), true);
});

test("health badge flags forbidden http data paths", () => {
  assert.deepEqual(healthBadge({
    dataPlane: "rxdb-webrtc",
    dataPlaneReady: true,
    httpDataProxy: true,
    nativePeerObserved: true,
  }), {
    kind: "health",
    tone: "error",
    label: "http data",
    title: "HTTP-Datenpfad ist nicht erlaubt",
  });
});
