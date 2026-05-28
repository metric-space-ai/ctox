// Origin: CTOX
// License: AGPL-3.0-only

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const configPath =
  process.argv[2] ||
  path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../wrangler.jsonc");

if (import.meta.url === `file://${process.argv[1]}`) {
  const result = validateWranglerConfig(readJsonc(configPath));
  if (!result.ok) {
    for (const error of result.errors) {
      console.error(`fail ${error}`);
    }
    process.exitCode = 1;
  } else {
    console.log("ok wrangler config is production-ready for managed rendezvous");
  }
}

export function readJsonc(filePath) {
  const raw = fs.readFileSync(filePath, "utf8");
  return JSON.parse(stripJsonComments(raw));
}

export function validateWranglerConfig(config) {
  const errors = [];
  const bindings = config?.durable_objects?.bindings || [];
  const migrations = config?.migrations || [];
  const routes = config?.routes || [];
  const vars = config?.vars || {};

  const sessionBinding = bindings.find(
    (binding) =>
      binding?.name === "BUSINESS_OS_MCP_SESSIONS" &&
      binding?.class_name === "BusinessOsMcpSession"
  );
  if (!sessionBinding) {
    errors.push("missing BUSINESS_OS_MCP_SESSIONS Durable Object binding");
  }

  const hasSessionMigration = migrations.some((migration) =>
    (migration?.new_sqlite_classes || migration?.new_classes || []).includes("BusinessOsMcpSession")
  );
  if (!hasSessionMigration) {
    errors.push("missing BusinessOsMcpSession Durable Object migration");
  }

  const hasCtoxRoute = routes.some(
    (route) =>
      route?.pattern === "mcp.ctox.dev" ||
      (route?.pattern === "mcp.ctox.dev/*" && route?.custom_domain !== true)
  );
  if (!hasCtoxRoute) {
    errors.push("missing mcp.ctox.dev route");
  }

  for (const secretKey of [
    "MCP_GATEWAY_TOKEN",
    "MCP_CLIENT_TOKENS",
    "INSTANCE_CONNECT_TOKEN",
    "INSTANCE_CONNECT_TOKENS",
    "UPSTREAM_AUTHORIZATION"
  ]) {
    if (Object.prototype.hasOwnProperty.call(vars, secretKey)) {
      errors.push(`${secretKey} must be a Worker secret, not a wrangler vars entry`);
    }
  }

  if (String(vars.REQUIRE_CONNECT_REPLAY_GUARD || "").toLowerCase() !== "true") {
    errors.push("REQUIRE_CONNECT_REPLAY_GUARD must default to true");
  }

  if (String(vars.MCP_REQUIRE_CLIENT_IDENTITY || "").toLowerCase() !== "true") {
    errors.push("MCP_REQUIRE_CLIENT_IDENTITY must default to true");
  }

  for (const numericKey of [
    "MAX_MCP_BODY_BYTES",
    "MAX_MCP_RESPONSE_BYTES",
    "MCP_SESSION_TIMEOUT_MS",
    "MAX_PENDING_REQUESTS",
    "CONNECT_REPLAY_WINDOW_MS",
    "MAX_CONNECT_NONCES"
  ]) {
    const value = Number.parseInt(vars[numericKey], 10);
    if (!Number.isFinite(value) || value <= 0) {
      errors.push(`${numericKey} must be a positive numeric string`);
    }
  }

  return {
    ok: errors.length === 0,
    errors
  };
}

function stripJsonComments(raw) {
  return raw
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:])\/\/.*$/gm, "$1")
    .replace(/,\s*([}\]])/g, "$1");
}
