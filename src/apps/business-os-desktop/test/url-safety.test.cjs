"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const {
  isForbiddenBusinessOsHttpDataRequest,
  isAllowedBusinessOsNavigation,
  scrubCtoxConfigFromUrl,
} = require("../src/main/url-safety.cjs");

test("scrubs bootstrap secrets from launch URLs", () => {
  const scrubbed = scrubCtoxConfigFromUrl("https://ctox.dev/business-os/?ctox_config=secret&view=home");
  assert.equal(scrubbed, "https://ctox.dev/business-os/?view=home");
});

test("allows only expected web origins for embedded Business OS navigation", () => {
  const allowed = new Set(["https://ctox.dev"]);
  assert.equal(isAllowedBusinessOsNavigation("https://ctox.dev/business-os/", allowed), true);
  assert.equal(isAllowedBusinessOsNavigation("https://example.com/", allowed), false);
  assert.equal(isAllowedBusinessOsNavigation("ctox-business-os-desktop://pair", allowed), false);
});

test("classifies Business OS HTTP data requests separately from control plane", () => {
  assert.equal(isForbiddenBusinessOsHttpDataRequest("https://tenant.example.com/api/business-os/status"), false);
  assert.equal(isForbiddenBusinessOsHttpDataRequest("https://tenant.example.com/api/business-os/sync/config"), false);
  assert.equal(
    isForbiddenBusinessOsHttpDataRequest("https://tenant.example.com/api/business-os/ctox/subscription-auth/start"),
    false,
  );
  assert.equal(isForbiddenBusinessOsHttpDataRequest("https://tenant.example.com/api/business-os/records"), true);
  assert.equal(isForbiddenBusinessOsHttpDataRequest("https://tenant.example.com/api/business-os/commands"), true);
  assert.equal(isForbiddenBusinessOsHttpDataRequest("https://tenant.example.com/rxdb/pull"), true);
  assert.equal(isForbiddenBusinessOsHttpDataRequest("https://tenant.example.com/commands"), true);
  assert.equal(isForbiddenBusinessOsHttpDataRequest("wss://signaling.ctox.dev/room"), false);
});
