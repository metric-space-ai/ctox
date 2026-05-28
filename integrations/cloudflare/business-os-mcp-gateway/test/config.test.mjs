import assert from "node:assert/strict";
import { test } from "node:test";
import { readJsonc, validateWranglerConfig } from "../scripts/validate-config.mjs";

test("wrangler config passes production readiness validation", () => {
  const config = readJsonc(new URL("../wrangler.jsonc", import.meta.url));
  const result = validateWranglerConfig(config);

  assert.deepEqual(result, { ok: true, errors: [] });
});

test("wrangler config validation rejects secrets in vars", () => {
  const result = validateWranglerConfig({
    durable_objects: {
      bindings: [{ name: "BUSINESS_OS_MCP_SESSIONS", class_name: "BusinessOsMcpSession" }]
    },
    migrations: [{ new_sqlite_classes: ["BusinessOsMcpSession"] }],
    routes: [{ pattern: "mcp.ctox.dev" }],
    vars: {
      MCP_GATEWAY_TOKEN: "secret",
      MCP_CLIENT_TOKENS: "{}",
      REQUIRE_CONNECT_REPLAY_GUARD: "true",
      MCP_REQUIRE_CLIENT_IDENTITY: "true",
      MAX_MCP_BODY_BYTES: "1",
      MAX_MCP_RESPONSE_BYTES: "1",
      MCP_SESSION_TIMEOUT_MS: "1",
      MAX_PENDING_REQUESTS: "1",
      CONNECT_REPLAY_WINDOW_MS: "1",
      MAX_CONNECT_NONCES: "1"
    }
  });

  assert.equal(result.ok, false);
  assert.ok(result.errors.includes("MCP_GATEWAY_TOKEN must be a Worker secret, not a wrangler vars entry"));
  assert.ok(result.errors.includes("MCP_CLIENT_TOKENS must be a Worker secret, not a wrangler vars entry"));
});

test("wrangler config validation requires managed rendezvous binding", () => {
  const result = validateWranglerConfig({
    durable_objects: { bindings: [] },
    migrations: [],
    routes: [{ pattern: "mcp.ctox.dev" }],
    vars: {
      REQUIRE_CONNECT_REPLAY_GUARD: "true",
      MCP_REQUIRE_CLIENT_IDENTITY: "true",
      MAX_MCP_BODY_BYTES: "1",
      MAX_MCP_RESPONSE_BYTES: "1",
      MCP_SESSION_TIMEOUT_MS: "1",
      MAX_PENDING_REQUESTS: "1",
      CONNECT_REPLAY_WINDOW_MS: "1",
      MAX_CONNECT_NONCES: "1"
    }
  });

  assert.equal(result.ok, false);
  assert.ok(result.errors.includes("missing BUSINESS_OS_MCP_SESSIONS Durable Object binding"));
});
