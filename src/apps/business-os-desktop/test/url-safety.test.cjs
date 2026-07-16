"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const {
  isForbiddenBusinessOsHttpDataRequest,
  isForbiddenBusinessOsDataResourceRequest,
  isAllowedBusinessOsNavigation,
  isSafeExternalUrl,
  scrubCtoxConfigFromUrl,
} = require("../src/main/url-safety.cjs");

test("scrubs bootstrap secrets from launch URLs", () => {
  const scrubbed = scrubCtoxConfigFromUrl("https://ctox.dev/business-os/?ctox_config=secret&view=home");
  assert.equal(scrubbed, "https://ctox.dev/business-os/?view=home");
});

test("allows only expected web origins for embedded Business OS navigation", () => {
  const allowed = new Set(["https://ctox.dev"]);
  assert.equal(isAllowedBusinessOsNavigation("https://ctox.dev/business-os/", allowed), true);
  assert.equal(isAllowedBusinessOsNavigation("about:blank", allowed), true);
  assert.equal(isAllowedBusinessOsNavigation("https://example.com/", allowed), false);
  assert.equal(isAllowedBusinessOsNavigation("ctox-business-os-desktop://pair", allowed), false);
  // data:/file: schemes must never be allowed to navigate the instance view.
  assert.equal(isAllowedBusinessOsNavigation("data:text/html,<script>alert(1)</script>", allowed), false);
  assert.equal(isAllowedBusinessOsNavigation("file:///etc/passwd", allowed), false);
});

test("only http/https/mailto are safe to hand to the OS browser", () => {
  assert.equal(isSafeExternalUrl("https://ctox.dev/help"), true);
  assert.equal(isSafeExternalUrl("mailto:support@ctox.dev"), true);
  assert.equal(isSafeExternalUrl("file:///etc/passwd"), false);
  assert.equal(isSafeExternalUrl("data:text/html,x"), false);
  assert.equal(isSafeExternalUrl("ctox-business-os-desktop://pair?payload=x"), false);
});

test("default-denies unknown same-host data fetches but allows control plane and assets", () => {
  const origin = "https://tenant.example.com";
  // New/unknown data routes on the launch host are denied for xhr/fetch/websocket.
  assert.equal(isForbiddenBusinessOsDataResourceRequest("https://tenant.example.com/files", "xhr", origin), true);
  assert.equal(isForbiddenBusinessOsDataResourceRequest("https://tenant.example.com/sync", "fetch", origin), true);
  assert.equal(isForbiddenBusinessOsDataResourceRequest("wss://tenant.example.com/business_commands", "websocket", origin), true);
  // Explicit control plane + static rxdb bundle stay reachable.
  assert.equal(isForbiddenBusinessOsDataResourceRequest("https://tenant.example.com/api/business-os/status", "xhr", origin), false);
  assert.equal(isForbiddenBusinessOsDataResourceRequest("https://tenant.example.com/rxdb/dist/ctox-rxdb-js.mjs", "fetch", origin), false);
  assert.equal(isForbiddenBusinessOsDataResourceRequest("https://tenant.example.com/system-apps.json", "fetch", origin), false);
  assert.equal(isForbiddenBusinessOsDataResourceRequest("https://tenant.example.com/modules/registry.json", "fetch", origin), false);
  assert.equal(isForbiddenBusinessOsDataResourceRequest("https://tenant.example.com/modules/research/index.html", "fetch", origin), false);
  assert.equal(isForbiddenBusinessOsDataResourceRequest("https://tenant.example.com/modules/research/locales/de.json", "xhr", origin), false);
  assert.equal(isForbiddenBusinessOsDataResourceRequest("https://tenant.example.com/vendor/editor/engine.wasm", "fetch", origin), false);
  assert.equal(isForbiddenBusinessOsDataResourceRequest("https://tenant.example.com/system-apps.json", "fetch", origin, "POST"), true);
  assert.equal(isForbiddenBusinessOsDataResourceRequest("https://tenant.example.com/modules/research/records", "fetch", origin), true);
  // Non-data resource types (the shell's own assets) are never constrained here.
  assert.equal(isForbiddenBusinessOsDataResourceRequest("https://tenant.example.com/app.js", "script", origin), false);
  // Cross-host requests (e.g. the signaling server) are out of scope for this layer.
  assert.equal(isForbiddenBusinessOsDataResourceRequest("wss://signaling.ctox.dev/room", "websocket", origin), false);
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
  assert.equal(
    isForbiddenBusinessOsHttpDataRequest("https://tenant.example.com/rxdb/dist/ctox-rxdb-js.mjs?v=20260614"),
    false,
  );
  assert.equal(isForbiddenBusinessOsHttpDataRequest("https://tenant.example.com/rxdb/pull"), true);
  assert.equal(isForbiddenBusinessOsHttpDataRequest("https://tenant.example.com/commands"), true);
  assert.equal(isForbiddenBusinessOsHttpDataRequest("wss://signaling.ctox.dev/room"), false);
});
