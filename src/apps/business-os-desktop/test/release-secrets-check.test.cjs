"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");
const {
  REQUIRED_RELEASE_SECRETS,
  missingRequiredSecrets,
  parseSecretNames,
} = require("../scripts/check-release-secrets.cjs");

test("release secret preflight parses gh secret list output", () => {
  assert.deepEqual(parseSecretNames(JSON.stringify([
    { name: "APPLE_ID" },
    { name: "APPLE_ID_PASSWORD" },
    "APPLE_TEAM_ID",
  ])), [
    "APPLE_ID",
    "APPLE_ID_PASSWORD",
    "APPLE_TEAM_ID",
  ]);
});

test("release secret preflight reports only missing required secrets", () => {
  const present = REQUIRED_RELEASE_SECRETS.filter((name) => name !== "APPLE_TEAM_ID");
  assert.deepEqual(missingRequiredSecrets(present), ["APPLE_TEAM_ID"]);
  assert.deepEqual(missingRequiredSecrets(REQUIRED_RELEASE_SECRETS), []);
});
