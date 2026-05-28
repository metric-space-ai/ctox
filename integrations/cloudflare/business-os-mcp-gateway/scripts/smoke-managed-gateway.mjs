// Origin: CTOX
// License: AGPL-3.0-only

const baseUrl = trimTrailingSlash(process.env.GATEWAY_BASE_URL || "https://mcp.ctox.dev");
const instanceId = process.env.INSTANCE_ID || "desk_123";
const gatewayToken = process.env.MCP_GATEWAY_TOKEN || "";
const expectConnected = truthy(process.env.EXPECT_CONNECTED);

const headers = {
  accept: "application/json",
  "content-type": "application/json"
};
if (gatewayToken) {
  headers.authorization = `Bearer ${gatewayToken}`;
}

const checks = [];

checks.push(await checkHealth());
checks.push(await checkStatus());
checks.push(await checkMcpToolsList());

const failed = checks.filter((check) => !check.ok);
for (const check of checks) {
  const mark = check.ok ? "ok" : "fail";
  console.log(`${mark} ${check.name}: ${check.message}`);
}

if (failed.length > 0) {
  process.exitCode = 1;
}

async function checkHealth() {
  const response = await fetchJson(`${baseUrl}/health`, { headers: { accept: "application/json" } });
  if (response.status !== 200 || response.body?.ok !== true) {
    if (isCtoxInstanceRouterMiss(response)) {
      return fail("health", "mcp.ctox.dev is not routed to the Business OS MCP Gateway");
    }
    return fail("health", `expected 200 ok, got ${response.status}`);
  }
  if (response.body.mode !== "managed_rendezvous") {
    return fail("health", `expected managed_rendezvous, got ${response.body.mode}`);
  }
  if (response.body.config?.managed_rendezvous_configured !== true) {
    return fail("health", "managed rendezvous binding is not reported as configured");
  }
  return pass("health", "gateway is reachable and configured for managed rendezvous");
}

async function checkStatus() {
  const response = await fetchJson(`${baseUrl}/status/${encodeURIComponent(instanceId)}`, {
    headers
  });
  if (response.status === 401 || response.status === 403) {
    return fail("status", `authorization failed with HTTP ${response.status}`);
  }
  if (response.status !== 200) {
    if (isCtoxInstanceRouterMiss(response)) {
      return fail("status", "mcp.ctox.dev is not routed to the Business OS MCP Gateway");
    }
    return fail("status", `expected 200, got ${response.status}`);
  }
  if (expectConnected && response.body?.connected !== true) {
    return fail("status", "expected connected CTOX instance");
  }
  return pass(
    "status",
    response.body?.connected
      ? "CTOX instance is connected"
      : "status route is reachable; CTOX instance is not connected"
  );
}

async function checkMcpToolsList() {
  const response = await fetchJson(`${baseUrl}/mcp/${encodeURIComponent(instanceId)}`, {
    method: "POST",
    headers,
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: "smoke-tools-list",
      method: "tools/list",
      params: {}
    })
  });
  const code = response.body?.error?.data?.code;
  if (!expectConnected && response.status === 503 && code === "runtime_unavailable") {
    return pass("mcp", "MCP route returns runtime_unavailable until CTOX connects");
  }
  if (expectConnected && response.status === 200 && response.body?.result?.tools) {
    return pass("mcp", `tools/list returned ${response.body.result.tools.length} tools`);
  }
  if (response.status === 401 || response.status === 403) {
    return fail("mcp", `authorization failed with HTTP ${response.status}`);
  }
  if (isCtoxInstanceRouterMiss(response)) {
    return fail("mcp", "mcp.ctox.dev is not routed to the Business OS MCP Gateway");
  }
  return fail("mcp", `unexpected HTTP ${response.status} response`);
}

async function fetchJson(url, init = {}) {
  const response = await fetch(url, init);
  const text = await response.text();
  let body = null;
  try {
    body = text ? JSON.parse(text) : null;
  } catch {
    body = { raw: text };
  }
  return { status: response.status, headers: response.headers, body, text };
}

function pass(name, message) {
  return { ok: true, name, message };
}

function fail(name, message) {
  return { ok: false, name, message };
}

function trimTrailingSlash(value) {
  return value.replace(/\/+$/, "");
}

function truthy(value) {
  return ["1", "true", "yes", "on"].includes(String(value || "").trim().toLowerCase());
}

function isCtoxInstanceRouterMiss(response) {
  return (
    response.status === 404 &&
    response.headers.get("content-type")?.includes("text/html") &&
    response.text.includes("instance_not_found")
  );
}
