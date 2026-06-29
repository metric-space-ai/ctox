// Origin: CTOX
// License: AGPL-3.0-only

const MCP_PATH = "/mcp";
const CONNECT_PATH = "/connect";
const STATUS_PATH = "/status";
const DEFAULT_SESSION_TIMEOUT_MS = 30_000;
const DEFAULT_MAX_MCP_BODY_BYTES = 1_000_000;
const DEFAULT_MAX_MCP_RESPONSE_BYTES = 1_000_000;
const DEFAULT_MAX_PENDING_REQUESTS = 16;
const DEFAULT_CONNECT_REPLAY_WINDOW_MS = 5 * 60 * 1000;
const DEFAULT_MAX_CONNECT_NONCES = 128;
const GATEWAY_CONTEXT_HEADER = "x-ctox-mcp-gateway-context";

export default {
  async fetch(request, env) {
    return handleRequest(request, env);
  }
};

export class BusinessOsMcpSession {
  constructor(state, env) {
    this.state = state;
    this.env = env;
    this.socket = null;
    this.pending = new Map();
    this.connectNonces = new Map();
    this.commandStatuses = new Map();
    this.sessionInfo = null;
    this.stats = makeSessionStats();
  }

  async fetch(request) {
    const url = new URL(request.url);
    if (url.pathname.endsWith(CONNECT_PATH)) {
      return this.handleConnect(request);
    }
    if (url.pathname.endsWith(MCP_PATH)) {
      return this.handleMcp(request);
    }
    if (url.pathname.endsWith("/status")) {
      return jsonResponse({
        ok: true,
        connected: Boolean(this.socket),
        pending: this.pending.size,
        session: this.sessionInfo,
        stats: this.stats
      });
    }
    return jsonResponse({ ok: false, error: "not_found" }, 404);
  }

  async handleConnect(request) {
    if (request.headers.get("upgrade") !== "websocket") {
      return jsonResponse({ ok: false, error: "websocket_required" }, 426);
    }
    const replay = acceptConnectReplay(request, this.env, this.connectNonces);
    if (!replay.ok) {
      return jsonResponse(
        {
          ok: false,
          error: replay.code,
          message: replay.message
        },
        replay.status
      );
    }
    if (typeof WebSocketPair === "undefined") {
      return jsonResponse({ ok: false, error: "websocket_runtime_unavailable" }, 501);
    }

    const pair = new WebSocketPair();
    const client = pair[0];
    const server = pair[1];
    server.accept();
    this.bindSocket(server);
    return new Response(null, { status: 101, webSocket: client });
  }

  async handleMcp(request) {
    if (request.method !== "POST") {
      recordRejected(this.stats);
      return jsonRpcError(null, -32000, "MCP endpoint requires POST", 405);
    }
    if (!this.socket) {
      recordRejected(this.stats);
      return jsonRpcError(
        requestIdFromBody(await safeJson(request.clone())),
        -32003,
        "CTOX instance is not connected",
        503,
        { code: "runtime_unavailable" }
      );
    }
    const pendingLimit = maxPendingRequests(this.env);
    if (this.pending.size >= pendingLimit) {
      recordRejected(this.stats);
      this.stats.backpressure_rejections += 1;
      return jsonRpcError(null, -32008, "CTOX MCP session has too many pending requests", 429, {
        code: "backpressure",
        limit: pendingLimit
      });
    }

    const requestId = crypto.randomUUID();
    const body = await request.text();
    const rpcRequest = await safeJsonFromText(body);
    const requestLimit = maxMcpBodyBytes(this.env);
    if (byteLength(body) > requestLimit) {
      recordRejected(this.stats);
      this.stats.oversized_request_rejections += 1;
      return jsonRpcError(
        requestIdFromBody(await safeJsonFromText(body)),
        -32005,
        "MCP request body exceeds gateway limit",
        413,
        { code: "request_too_large", limit_bytes: requestLimit }
      );
    }
    const timeoutMs = numericEnv(this.env.MCP_SESSION_TIMEOUT_MS, DEFAULT_SESSION_TIMEOUT_MS);
    const responsePromise = new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(requestId);
        recordFailure(this.stats);
        reject(new Error("CTOX MCP session timed out"));
      }, timeoutMs);
      this.pending.set(requestId, { resolve, reject, timeout, rpcRequest });
    });
    this.stats.accepted_requests += 1;
    this.stats.last_request_at_ms = Date.now();

    this.socket.send(
      JSON.stringify({
        type: "mcp_request",
        request_id: requestId,
        body,
        context: gatewayContextFromHeader(request)
      })
    );

    try {
      const upstream = await responsePromise;
      return new Response(upstream.body || "", {
        status: upstream.status || 200,
        headers: gatewayHeaders(new Headers(upstream.headers || {}))
      });
    } catch (error) {
      return jsonRpcError(null, -32004, error.message, 504, { code: "runtime_unavailable" });
    }
  }

  bindSocket(socket) {
    if (this.socket) {
      this.stats.replaced_connections += 1;
      this.rejectPending("CTOX MCP session replaced");
      try {
        this.socket.close(1012, "replaced");
      } catch {}
    }
    this.socket = socket;
    socket.addEventListener("message", (event) => this.handleSocketMessage(event.data));
    socket.addEventListener("close", () => this.clearSocket(socket));
    socket.addEventListener("error", () => this.clearSocket(socket));
  }

  handleSocketMessage(data) {
    let message;
    try {
      message = JSON.parse(data);
    } catch {
      return;
    }
    if (message.type === "ctox_hello") {
      this.sessionInfo = normalizeSessionInfo(message);
      return;
    }
    if (message.type !== "mcp_response" || !message.request_id) {
      return;
    }
    const pending = this.pending.get(message.request_id);
    if (!pending) {
      return;
    }
    clearTimeout(pending.timeout);
    this.pending.delete(message.request_id);
    const responseLimit = maxMcpResponseBytes(this.env);
    const body = normalizeJsonText(typeof message.body === "string" ? message.body : "");
    if (byteLength(body) > responseLimit) {
      recordFailure(this.stats);
      this.stats.oversized_response_rejections += 1;
      pending.resolve({
        status: 502,
        headers: { "content-type": "application/json; charset=utf-8" },
        body: JSON.stringify({
          jsonrpc: "2.0",
          id: null,
          error: {
            code: -32006,
            message: "MCP response body exceeds gateway limit",
            data: {
              code: "response_too_large",
              limit_bytes: responseLimit
            }
          }
        })
      });
      return;
    }
    const fallback = this.gatewayCommandStatusFallback(pending.rpcRequest, body, message.status || 200);
    if (fallback) {
      recordCompleted(this.stats, fallback.status || 200);
      pending.resolve(fallback);
      return;
    }
    this.rememberCommandStatus(pending.rpcRequest, body);
    recordCompleted(this.stats, message.status || 200);
    pending.resolve({
      status: message.status,
      headers: message.headers,
      body
    });
  }

  rememberCommandStatus(rpcRequest, responseBody) {
    const tool = toolCallName(rpcRequest);
    if (tool !== "business_os.execute_action") {
      return;
    }
    const response = parseJson(responseBody);
    const payload = toolTextPayload(response);
    if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
      return;
    }
    const commandId = cleanCommandId(payload.command_id);
    if (!commandId) {
      return;
    }
    const now = Date.now();
    this.commandStatuses.set(commandId, {
      command_id: commandId,
      task_id: typeof payload.task_id === "string" ? payload.task_id : "",
      status: typeof payload.status === "string" ? payload.status : "accepted",
      task_status: typeof payload.task_status === "string" ? payload.task_status : "queued",
      actor: payload.client_context && typeof payload.client_context.actor === "string" ? payload.client_context.actor : "",
      workspace:
        payload.client_context && typeof payload.client_context.workspace === "string"
          ? payload.client_context.workspace
          : "",
      source: "gateway_execute_action_cache",
      updated_at_ms: now,
      expires_at_ms: now + 24 * 60 * 60 * 1000
    });
    pruneCommandStatuses(this.commandStatuses, now);
  }

  gatewayCommandStatusFallback(rpcRequest, responseBody, status) {
    if (toolCallName(rpcRequest) !== "business_os.get_command_status") {
      return null;
    }
    const response = parseJson(responseBody);
    if (!isRecordNotFoundResponse(response)) {
      return null;
    }
    const commandId = cleanCommandId(
      rpcRequest &&
        rpcRequest.params &&
        rpcRequest.params.arguments &&
        rpcRequest.params.arguments.command_id
    );
    if (!commandId) {
      return null;
    }
    const cached = this.commandStatuses.get(commandId);
    if (!cached || cached.expires_at_ms < Date.now()) {
      this.commandStatuses.delete(commandId);
      return null;
    }
    const body = JSON.stringify(gatewayCommandStatusResponse(rpcRequest.id, cached));
    return {
      status: status >= 500 ? 200 : status,
      headers: { "content-type": "application/json; charset=utf-8" },
      body
    };
  }

  clearSocket(socket) {
    if (this.socket !== socket) {
      return;
    }
    this.socket = null;
    this.sessionInfo = null;
    this.stats.disconnects += 1;
    this.rejectPending("CTOX MCP session disconnected");
  }

  rejectPending(message) {
    for (const [requestId, pending] of this.pending.entries()) {
      clearTimeout(pending.timeout);
      recordFailure(this.stats);
      pending.reject(new Error(message));
      this.pending.delete(requestId);
    }
  }
}

export async function handleRequest(request, env = {}) {
  const url = new URL(request.url);

  if (request.method === "OPTIONS") {
    return jsonResponse({ ok: true });
  }

  if (request.method === "GET" && url.pathname === "/health") {
    return jsonResponse({
      ok: true,
      service: "ctox-business-os-mcp-gateway",
      mode: gatewayMode(env),
      config: publicGatewayConfig(env)
    });
  }

  const managedRoute = parseManagedRoute(url.pathname);
  if (managedRoute && managedRoute.kind === "mcp") {
    return handleManagedMcp(request, env, managedRoute.instanceId);
  }

  if (managedRoute && managedRoute.kind === "connect") {
    return handleManagedConnect(request, env, managedRoute.instanceId);
  }

  if (managedRoute && managedRoute.kind === "status") {
    return handleManagedStatus(request, env, managedRoute.instanceId);
  }

  if (url.pathname === MCP_PATH) {
    return handleMcpRelay(request, env);
  }

  if (url.pathname === CONNECT_PATH) {
    return jsonResponse(
      {
        ok: false,
        error: "websocket_rendezvous_not_enabled",
        message:
          "This gateway build only supports explicit HTTP MCP relay to a configured upstream."
      },
      501
    );
  }

  return jsonResponse({ ok: false, error: "not_found" }, 404);
}

export async function handleManagedMcp(request, env = {}, instanceId) {
  if (request.method !== "POST") {
    return jsonRpcError(null, -32000, "MCP endpoint requires POST", 405);
  }
  const instanceAccess = authorizeInstanceId(instanceId, env);
  if (!instanceAccess.ok) {
    return jsonRpcError(null, -32007, instanceAccess.message, 403, {
      code: "instance_not_allowed"
    });
  }
  const auth = await authorizeMcpClientForRequest(request, env, instanceId);
  if (!auth.ok) {
    return jsonRpcError(null, -32001, auth.message, 401);
  }
  const bodyCheck = checkContentLength(request, maxMcpBodyBytes(env));
  if (!bodyCheck.ok) {
    return jsonRpcError(null, -32005, "MCP request body exceeds gateway limit", 413, {
      code: "request_too_large",
      limit_bytes: bodyCheck.limit
    });
  }
  const policy = await enforceMcpClientPolicy(request.clone(), auth.policy);
  if (!policy.ok) {
    return jsonRpcError(
      requestIdFromBody(await safeJson(request.clone())),
      -32004,
      policy.message,
      403,
      { code: "permission_denied", field: policy.field || "managed_mcp_policy" }
    );
  }
  const stub = sessionStub(env, instanceId);
  if (!stub) {
    return jsonRpcError(
      requestIdFromBody(await safeJson(request.clone())),
      -32002,
      "No CTOX MCP session binding is configured",
      503,
      { code: "sync_not_ready" }
    );
  }
  return stub.fetch(withGatewayContext(new Request(`https://session.local${MCP_PATH}`, request), auth.context));
}

export async function handleManagedConnect(request, env = {}, instanceId) {
  const instanceAccess = authorizeInstanceId(instanceId, env);
  if (!instanceAccess.ok) {
    return jsonResponse(
      {
        ok: false,
        error: "instance_not_allowed",
        message: instanceAccess.message
      },
      403
    );
  }
  const auth = authorizeInstanceConnect(request, env, instanceId);
  if (!auth.ok) {
    return jsonResponse({ ok: false, error: "not_authorized", message: auth.message }, 401);
  }
  const replay = validateConnectReplayHeaders(request, env);
  if (!replay.ok) {
    return jsonResponse(
      {
        ok: false,
        error: replay.code,
        message: replay.message
      },
      replay.status
    );
  }
  const stub = sessionStub(env, instanceId);
  if (!stub) {
    return jsonResponse(
      {
        ok: false,
        error: "sync_not_ready",
        message: "No CTOX MCP session binding is configured"
      },
      503
    );
  }
  return stub.fetch(new Request(`https://session.local${CONNECT_PATH}`, request));
}

export async function handleManagedStatus(request, env = {}, instanceId) {
  if (request.method !== "GET") {
    return jsonResponse({ ok: false, error: "method_not_allowed" }, 405);
  }
  const instanceAccess = authorizeInstanceId(instanceId, env);
  if (!instanceAccess.ok) {
    return jsonResponse(
      {
        ok: false,
        error: "instance_not_allowed",
        message: instanceAccess.message
      },
      403
    );
  }
  const auth = await authorizeMcpClientForRequest(request, env, instanceId);
  if (!auth.ok) {
    return jsonResponse({ ok: false, error: "not_authorized", message: auth.message }, 401);
  }
  if (auth.policy && auth.policy.allowReads === false) {
    return jsonResponse({ ok: false, error: "permission_denied", message: "Managed MCP token does not allow reads" }, 403);
  }
  const stub = sessionStub(env, instanceId);
  if (!stub) {
    return jsonResponse(
      {
        ok: false,
        error: "sync_not_ready",
        message: "No CTOX MCP session binding is configured"
      },
      503
    );
  }
  return stub.fetch(new Request(`https://session.local${STATUS_PATH}`, request));
}

export async function handleMcpRelay(request, env = {}) {
  if (request.method !== "POST") {
    return jsonRpcError(null, -32000, "MCP endpoint requires POST", 405);
  }

  const auth = await authorizeMcpClientForRequest(request, env, null);
  if (!auth.ok) {
    return jsonRpcError(null, -32001, auth.message, 401);
  }
  const bodyCheck = checkContentLength(request, maxMcpBodyBytes(env));
  if (!bodyCheck.ok) {
    return jsonRpcError(null, -32005, "MCP request body exceeds gateway limit", 413, {
      code: "request_too_large",
      limit_bytes: bodyCheck.limit
    });
  }

  const upstreamUrl = normalizedUpstreamUrl(env);
  if (!upstreamUrl) {
    return jsonRpcError(
      requestIdFromBody(await safeJson(request.clone())),
      -32002,
      "No CTOX MCP upstream is connected to this gateway",
      503,
      { code: "sync_not_ready" }
    );
  }

  const body = injectGatewayContext(await request.text(), auth.context);
  const bodyLimit = maxMcpBodyBytes(env);
  if (byteLength(body) > bodyLimit) {
    return jsonRpcError(
      requestIdFromBody(await safeJsonFromText(body)),
      -32005,
      "MCP request body exceeds gateway limit",
      413,
      { code: "request_too_large", limit_bytes: bodyLimit }
    );
  }
  const upstreamResponse = await fetch(upstreamUrl, {
    method: "POST",
    headers: upstreamHeaders(request, env),
    body
  });

  const responseText = await upstreamResponse.text();
  const responseLimit = maxMcpResponseBytes(env);
  if (byteLength(responseText) > responseLimit) {
    return jsonRpcError(null, -32006, "MCP response body exceeds gateway limit", 502, {
      code: "response_too_large",
      limit_bytes: responseLimit
    });
  }

  return new Response(responseText, {
    status: upstreamResponse.status,
    headers: gatewayHeaders(upstreamResponse.headers)
  });
}

export function authorize(request, env = {}) {
  const token = (env.MCP_GATEWAY_TOKEN || "").trim();
  if (!token) {
    return { ok: true };
  }
  const expected = `Bearer ${token}`;
  const actual = request.headers.get("authorization") || "";
  if (actual === expected) {
    return { ok: true };
  }
  return {
    ok: false,
    message: "Invalid or missing MCP gateway authorization"
  };
}

export function authorizeMcpClient(request, env = {}, instanceId = null) {
  const registry = mcpClientTokens(env);
  const requireIdentity = truthyEnv(env.MCP_REQUIRE_CLIENT_IDENTITY);
  const authorization = request.headers.get("authorization") || "";
  const bearer = bearerToken(authorization);

  if (registry.size > 0) {
    const entry = bearer ? registry.get(bearer) : null;
    if (!entry) {
      return {
        ok: false,
        message: "Invalid or missing MCP client authorization"
      };
    }
    const instance = instanceAllowedForClient(entry, instanceId);
    if (!instance.ok) {
      return {
        ok: false,
        message: instance.message
      };
    }
    return { ok: true, context: mcpClientContext(entry, instanceId) };
  }

  if (requireIdentity) {
    return {
      ok: false,
      message: "MCP client identity registry is required"
    };
  }

  const legacy = authorize(request, env);
  if (!legacy.ok) {
    return legacy;
  }
  return {
    ok: true,
    context: {
      channel: "chatgpt_mcp",
      surface: "business_os_mcp",
      actor: stringOr(env.MCP_DEFAULT_ACTOR, "mcp:gateway"),
      workspace: stringOr(env.MCP_DEFAULT_WORKSPACE, instanceId ? `instance:${instanceId}` : "gateway"),
      instance_id: instanceId || null,
      client_id: stringOr(env.MCP_DEFAULT_CLIENT_ID, "legacy_gateway_token"),
      auth_source: "gateway_legacy_token"
    }
  };
}

export async function authorizeMcpClientForRequest(request, env = {}, instanceId = null) {
  const managed = await authorizeCtoxDevManagedMcpClient(request, env, instanceId);
  if (managed) {
    return managed;
  }
  return authorizeMcpClient(request, env, instanceId);
}

export async function authorizeCtoxDevManagedMcpClient(request, env = {}, instanceId = null) {
  const authUrl = managedMcpAuthUrl(env);
  const authorization = request.headers.get("authorization") || "";
  const token = bearerToken(authorization);
  if (!authUrl || !instanceId || !token || !token.startsWith("ctox_mcp_")) {
    return null;
  }
  const headers = new Headers({
    accept: "application/json",
    authorization: `Bearer ${token}`,
    "content-type": "application/json"
  });
  const gatewaySecret = (env.CTOX_MANAGED_MCP_AUTH_TOKEN || "").trim();
  if (gatewaySecret) {
    headers.set("x-ctox-managed-mcp-auth", gatewaySecret);
  }
  let response;
  let payload = null;
  try {
    response = await fetch(authUrl, {
      method: "POST",
      headers,
      body: JSON.stringify({ instanceId })
    });
    payload = await response.json().catch(() => null);
  } catch {
    return {
      ok: false,
      message: "Managed MCP token validation is unavailable"
    };
  }
  if (!response.ok || !payload?.ok) {
    return {
      ok: false,
      message: "Invalid or inactive managed MCP client authorization"
    };
  }
  const context = sanitizeGatewayContext(payload.context);
  if (!context) {
    return {
      ok: false,
      message: "Managed MCP token validation returned invalid context"
    };
  }
  context.instance_id = instanceId;
  return {
    ok: true,
    context,
    policy: normalizeManagedMcpPolicy(payload.policy)
  };
}

export function mcpClientTokens(env = {}) {
  const raw = (env.MCP_CLIENT_TOKENS || "").trim();
  const tokens = new Map();
  if (!raw) {
    return tokens;
  }
  if (raw.startsWith("{")) {
    try {
      const parsed = JSON.parse(raw);
      for (const [token, value] of Object.entries(parsed)) {
        const normalized = normalizeMcpClientEntry(token, value);
        if (normalized) {
          tokens.set(token, normalized);
        }
      }
    } catch {}
    return tokens;
  }
  for (const entry of raw.split(",")) {
    const index = entry.indexOf("=");
    if (index <= 0) {
      continue;
    }
    const token = entry.slice(0, index).trim();
    const actor = entry.slice(index + 1).trim();
    const normalized = normalizeMcpClientEntry(token, { actor });
    if (normalized) {
      tokens.set(token, normalized);
    }
  }
  return tokens;
}

export function authorizeInstanceConnect(request, env = {}, instanceId = null) {
  const token = connectTokenForInstance(env, instanceId);
  if (!token) {
    return { ok: true };
  }
  const expected = `Bearer ${token}`;
  const actual = request.headers.get("authorization") || "";
  if (actual === expected) {
    return { ok: true };
  }
  return {
    ok: false,
    message: "Invalid or missing CTOX instance authorization"
  };
}

function normalizeMcpClientEntry(token, value) {
  if (!token || typeof token !== "string" || token.trim().length < 12) {
    return null;
  }
  const entry = value && typeof value === "object" && !Array.isArray(value) ? value : { actor: value };
  const actor = stringOr(entry.actor, "");
  if (!isValidContextValue(actor)) {
    return null;
  }
  const workspace = stringOr(entry.workspace, "gateway");
  if (!isValidContextValue(workspace)) {
    return null;
  }
  return {
    actor,
    workspace,
    client_id: cleanContextValue(entry.client_id || entry.clientId || actor),
    tenant_id: cleanContextValue(entry.tenant_id || entry.tenantId || ""),
    role: cleanContextValue(entry.role || ""),
    scopes: stringList(entry.scopes).slice(0, 32),
    allowed_instances: stringList(entry.allowed_instances || entry.allowedInstances)
      .filter(isValidInstanceId)
      .slice(0, 100)
  };
}

function mcpClientContext(entry, instanceId) {
  const context = {
    channel: "chatgpt_mcp",
    surface: "business_os_mcp",
    actor: entry.actor,
    workspace: entry.workspace,
    client_id: entry.client_id || entry.actor,
    auth_source: "gateway_client_token"
  };
  if (entry.tenant_id) {
    context.tenant_id = entry.tenant_id;
  }
  if (entry.role) {
    context.role = entry.role;
  }
  if (entry.scopes.length > 0) {
    context.scopes = entry.scopes;
  }
  if (instanceId) {
    context.instance_id = instanceId;
  }
  return context;
}

function instanceAllowedForClient(entry, instanceId) {
  if (!instanceId || entry.allowed_instances.length === 0 || entry.allowed_instances.includes(instanceId)) {
    return { ok: true };
  }
  return {
    ok: false,
    message: "MCP client is not allowed for this CTOX instance"
  };
}

function withGatewayContext(request, context) {
  if (!context) {
    return request;
  }
  const headers = new Headers(request.headers);
  headers.set(GATEWAY_CONTEXT_HEADER, JSON.stringify(context));
  return new Request(request, { headers });
}

function gatewayContextFromHeader(request) {
  const raw = request.headers.get(GATEWAY_CONTEXT_HEADER);
  if (!raw) {
    return null;
  }
  try {
    const parsed = JSON.parse(raw);
    return sanitizeGatewayContext(parsed);
  } catch {
    return null;
  }
}

function sanitizeGatewayContext(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  const context = {};
  for (const key of ["channel", "surface", "actor", "workspace", "client_id", "tenant_id", "role", "instance_id", "auth_source"]) {
    const clean = cleanContextValue(value[key]);
    if (clean) {
      context[key] = clean;
    }
  }
  const scopes = stringList(value.scopes).slice(0, 32);
  if (scopes.length > 0) {
    context.scopes = scopes;
  }
  return isValidContextValue(context.actor) && isValidContextValue(context.workspace) ? context : null;
}

function injectGatewayContext(body, context) {
  if (!context) {
    return body;
  }
  let parsed;
  try {
    parsed = JSON.parse(body);
  } catch {
    return body;
  }
  const next = injectGatewayContextValue(parsed, context);
  return JSON.stringify(next);
}

function injectGatewayContextValue(value, context) {
  if (Array.isArray(value)) {
    return value.map((item) => injectGatewayContextValue(item, context));
  }
  if (!value || typeof value !== "object") {
    return value;
  }
  if (value.method === "tools/call" && value.params && typeof value.params === "object") {
    const params = { ...value.params };
    const argumentsValue =
      params.arguments && typeof params.arguments === "object" && !Array.isArray(params.arguments)
        ? { ...params.arguments }
        : {};
    argumentsValue._context = context;
    params.arguments = argumentsValue;
    return { ...value, params };
  }
  return value;
}

function bearerToken(authorization) {
  const match = /^Bearer\s+(.+)$/i.exec(authorization.trim());
  return match ? match[1].trim() : "";
}

function managedMcpAuthUrl(env = {}) {
  const value = (env.CTOX_MANAGED_MCP_AUTH_URL || "").trim();
  if (!value) {
    return "";
  }
  try {
    const url = new URL(value);
    if (url.protocol !== "https:" && url.hostname !== "localhost" && url.hostname !== "127.0.0.1") {
      return "";
    }
    return url.toString();
  } catch {
    return "";
  }
}

async function enforceMcpClientPolicy(request, policy) {
  if (!policy) {
    return { ok: true };
  }
  const body = await safeJson(request);
  const tools = mcpToolNamesFromBody(body);
  for (const tool of tools) {
    const decision = managedToolPolicyDecision(tool, policy);
    if (!decision.ok) {
      return decision;
    }
  }
  return { ok: true };
}

function mcpToolNamesFromBody(body) {
  const values = Array.isArray(body) ? body : [body];
  return values
    .filter((value) => value && typeof value === "object")
    .filter((value) => value.method === "tools/call")
    .map((value) => value.params && typeof value.params.name === "string" ? value.params.name.trim() : "")
    .filter(Boolean);
}

function managedToolPolicyDecision(tool, policy) {
  if (policy.deniedTools.includes(tool)) {
    return { ok: false, message: `Managed MCP token denies ${tool}`, field: "deniedTools" };
  }
  if (policy.allowedTools.length > 0 && !policy.allowedTools.includes(tool)) {
    return { ok: false, message: `Managed MCP token does not allow ${tool}`, field: "allowedTools" };
  }
  if (APPROVAL_TOOLS.has(tool) && !policy.allowApprovals) {
    return { ok: false, message: `Managed MCP token does not allow approval tool ${tool}`, field: "allowApprovals" };
  }
  if (WRITE_TOOLS.has(tool) && !policy.allowWrites) {
    return { ok: false, message: `Managed MCP token does not allow write tool ${tool}`, field: "allowWrites" };
  }
  if (READ_TOOLS.has(tool) && !policy.allowReads) {
    return { ok: false, message: `Managed MCP token does not allow read tool ${tool}`, field: "allowReads" };
  }
  return { ok: true };
}

const READ_TOOLS = new Set([
  "business_os.status",
  "business_os.list_modules",
  "business_os.get_module",
  "business_os.list_entities",
  "business_os.search_records",
  "business_os.query_records",
  "business_os.get_record",
  "business_os.get_record_context",
  "business_os.list_record_activity",
  "business_os.list_runs",
  "business_os.get_run",
  "business_os.list_artifacts",
  "business_os.get_artifact",
  "business_os.list_approvals",
  "business_os.open_link",
  "business_os.list_mcp_activity",
  "business_os.list_module_actions",
  "business_os.get_command_status"
]);

const WRITE_TOOLS = new Set([
  "business_os.propose_action",
  "business_os.create_app",
  "business_os.modify_app",
  "business_os.execute_action"
]);

const APPROVAL_TOOLS = new Set([
  "business_os.approve",
  "business_os.reject",
  "business_os.request_changes"
]);

function normalizeManagedMcpPolicy(value) {
  const source = value && typeof value === "object" && !Array.isArray(value) ? value : {};
  return {
    allowReads: booleanOr(source.allowReads, true),
    allowWrites: booleanOr(source.allowWrites, false),
    allowApprovals: booleanOr(source.allowApprovals, false),
    allowExternalEffects: booleanOr(source.allowExternalEffects, false),
    allowedTools: stringList(source.allowedTools).slice(0, 50),
    deniedTools: stringList(source.deniedTools).slice(0, 50)
  };
}

function booleanOr(value, fallback) {
  return typeof value === "boolean" ? value : fallback;
}

function stringOr(value, fallback) {
  return typeof value === "string" && value.trim() ? value.trim() : fallback;
}

function stringList(value) {
  if (Array.isArray(value)) {
    return value.map(cleanContextValue).filter(Boolean);
  }
  if (typeof value === "string") {
    return value.split(/[,\s]+/).map(cleanContextValue).filter(Boolean);
  }
  return [];
}

function cleanContextValue(value) {
  if (typeof value !== "string") {
    return "";
  }
  const trimmed = value.trim();
  if (!/^[a-zA-Z0-9][a-zA-Z0-9._:@/-]{0,127}$/.test(trimmed)) {
    return "";
  }
  return trimmed;
}

function isValidContextValue(value) {
  return typeof value === "string" && cleanContextValue(value) === value;
}

export function connectTokenForInstance(env = {}, instanceId = null) {
  const scoped = instanceId ? instanceConnectTokens(env).get(instanceId) : null;
  if (scoped) {
    return scoped;
  }
  return (env.INSTANCE_CONNECT_TOKEN || "").trim();
}

export function instanceConnectTokens(env = {}) {
  const raw = (env.INSTANCE_CONNECT_TOKENS || "").trim();
  const tokens = new Map();
  if (!raw) {
    return tokens;
  }
  if (raw.startsWith("{")) {
    try {
      const parsed = JSON.parse(raw);
      for (const [instanceId, token] of Object.entries(parsed)) {
        if (isValidInstanceId(instanceId) && typeof token === "string" && token.trim()) {
          tokens.set(instanceId, token.trim());
        }
      }
    } catch {}
    return tokens;
  }
  for (const entry of raw.split(",")) {
    const separator = entry.includes("=") ? "=" : ":";
    const index = entry.indexOf(separator);
    if (index <= 0) {
      continue;
    }
    const instanceId = entry.slice(0, index).trim();
    const token = entry.slice(index + 1).trim();
    if (isValidInstanceId(instanceId) && token) {
      tokens.set(instanceId, token);
    }
  }
  return tokens;
}

export function validateConnectReplayHeaders(request, env = {}) {
  if (!connectReplayRequired(env)) {
    return { ok: true };
  }
  const timestamp = Number.parseInt(request.headers.get("x-ctox-mcp-timestamp") || "", 10);
  const nonce = (request.headers.get("x-ctox-mcp-nonce") || "").trim();
  if (!Number.isFinite(timestamp) || timestamp <= 0) {
    return {
      ok: false,
      status: 401,
      code: "connect_replay_header_required",
      message: "CTOX connect replay timestamp is required"
    };
  }
  if (!/^[a-zA-Z0-9._:-]{12,128}$/.test(nonce)) {
    return {
      ok: false,
      status: 401,
      code: "connect_replay_header_required",
      message: "CTOX connect replay nonce is required"
    };
  }
  const windowMs = connectReplayWindowMs(env);
  if (Math.abs(Date.now() - timestamp) > windowMs) {
    return {
      ok: false,
      status: 401,
      code: "connect_replay_timestamp_out_of_window",
      message: "CTOX connect replay timestamp is outside the allowed window"
    };
  }
  return { ok: true, nonce, timestamp };
}

export function acceptConnectReplay(request, env = {}, nonceStore = new Map()) {
  const replay = validateConnectReplayHeaders(request, env);
  if (!replay.ok || !connectReplayRequired(env)) {
    return replay;
  }
  pruneConnectNonces(nonceStore, connectReplayWindowMs(env));
  if (nonceStore.has(replay.nonce)) {
    return {
      ok: false,
      status: 409,
      code: "connect_replay_detected",
      message: "CTOX connect replay nonce has already been used"
    };
  }
  nonceStore.set(replay.nonce, replay.timestamp);
  while (nonceStore.size > maxConnectNonces(env)) {
    const oldest = nonceStore.keys().next().value;
    nonceStore.delete(oldest);
  }
  return { ok: true };
}

function connectReplayRequired(env = {}) {
  if (truthyEnv(env.REQUIRE_CONNECT_REPLAY_GUARD)) {
    return true;
  }
  return Boolean((env.INSTANCE_CONNECT_TOKEN || "").trim() || (env.INSTANCE_CONNECT_TOKENS || "").trim());
}

function connectReplayWindowMs(env = {}) {
  return numericEnv(env.CONNECT_REPLAY_WINDOW_MS, DEFAULT_CONNECT_REPLAY_WINDOW_MS);
}

function maxConnectNonces(env = {}) {
  return numericEnv(env.MAX_CONNECT_NONCES, DEFAULT_MAX_CONNECT_NONCES);
}

function pruneConnectNonces(nonceStore, windowMs) {
  const cutoff = Date.now() - windowMs;
  for (const [nonce, timestamp] of nonceStore.entries()) {
    if (timestamp < cutoff) {
      nonceStore.delete(nonce);
    }
  }
}

export function authorizeInstanceId(instanceId, env = {}) {
  const allowed = allowedInstanceIds(env);
  if (!allowed) {
    return { ok: true };
  }
  if (allowed.has(instanceId)) {
    return { ok: true };
  }
  return {
    ok: false,
    message: "CTOX instance id is not allowed for this gateway"
  };
}

export function gatewayMode(env = {}) {
  if (env.BUSINESS_OS_MCP_SESSIONS) {
    return "managed_rendezvous";
  }
  if (env.UPSTREAM_MCP_URL) {
    return "http_relay";
  }
  return "unbound";
}

export function publicGatewayConfig(env = {}) {
  return {
    upstream_configured: Boolean((env.UPSTREAM_MCP_URL || "").trim()),
    managed_rendezvous_configured: Boolean(env.BUSINESS_OS_MCP_SESSIONS),
    allowed_instance_ids_configured: Boolean((env.ALLOWED_INSTANCE_IDS || "").trim()),
    mcp_gateway_auth_required: Boolean((env.MCP_GATEWAY_TOKEN || "").trim()),
    mcp_client_identity_required: truthyEnv(env.MCP_REQUIRE_CLIENT_IDENTITY),
    mcp_client_registry_configured: Boolean((env.MCP_CLIENT_TOKENS || "").trim()),
    managed_mcp_auth_configured: Boolean(managedMcpAuthUrl(env)),
    instance_connect_auth_required: Boolean(
      (env.INSTANCE_CONNECT_TOKEN || "").trim() || (env.INSTANCE_CONNECT_TOKENS || "").trim()
    ),
    connect_replay_guard_required: connectReplayRequired(env),
    limits: {
      max_mcp_body_bytes: maxMcpBodyBytes(env),
      max_mcp_response_bytes: maxMcpResponseBytes(env),
      mcp_session_timeout_ms: numericEnv(env.MCP_SESSION_TIMEOUT_MS, DEFAULT_SESSION_TIMEOUT_MS),
      max_pending_requests: maxPendingRequests(env),
      connect_replay_window_ms: connectReplayWindowMs(env),
      max_connect_nonces: maxConnectNonces(env)
    }
  };
}

export function normalizedUpstreamUrl(env = {}) {
  const value = (env.UPSTREAM_MCP_URL || "").trim();
  if (!value) {
    return null;
  }
  const url = new URL(value);
  if (url.protocol !== "https:" && url.hostname !== "127.0.0.1" && url.hostname !== "localhost") {
    throw new Error("UPSTREAM_MCP_URL must use https outside local development");
  }
  return url.toString();
}

export function parseManagedRoute(pathname) {
  const parts = pathname.split("/").filter(Boolean);
  if (parts.length !== 2) {
    return null;
  }
  if (parts[0] !== "mcp" && parts[0] !== "connect" && parts[0] !== "status") {
    return null;
  }
  const instanceId = decodeURIComponent(parts[1]).trim();
  if (!isValidInstanceId(instanceId)) {
    return null;
  }
  return { kind: parts[0], instanceId };
}

export function isValidInstanceId(value) {
  return /^[a-zA-Z0-9][a-zA-Z0-9._:-]{2,127}$/.test(value);
}

export function allowedInstanceIds(env = {}) {
  const raw = (env.ALLOWED_INSTANCE_IDS || "").trim();
  if (!raw) {
    return null;
  }
  const ids = raw
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean)
    .filter(isValidInstanceId);
  return new Set(ids);
}

function sessionStub(env, instanceId) {
  const binding = env.BUSINESS_OS_MCP_SESSIONS;
  if (!binding) {
    return null;
  }
  const id = binding.idFromName(instanceId);
  return binding.get(id);
}

function upstreamHeaders(request, env) {
  const headers = new Headers();
  headers.set("content-type", request.headers.get("content-type") || "application/json");
  headers.set("accept", "application/json");
  const upstreamAuthorization = (env.UPSTREAM_AUTHORIZATION || "").trim();
  if (upstreamAuthorization) {
    headers.set("authorization", upstreamAuthorization);
  }
  return headers;
}

function gatewayHeaders(upstreamHeadersValue) {
  const headers = new Headers();
  headers.set(
    "content-type",
    upstreamHeadersValue.get("content-type") || "application/json; charset=utf-8"
  );
  headers.set("cache-control", "no-store");
  applyCors(headers);
  return headers;
}

function normalizeSessionInfo(message) {
  return {
    ctox_version: typeof message.ctox_version === "string" ? message.ctox_version : "unknown",
    mcp_protocol_version:
      typeof message.mcp_protocol_version === "string" ? message.mcp_protocol_version : "unknown",
    capabilities: Array.isArray(message.capabilities)
      ? message.capabilities.filter((value) => typeof value === "string").slice(0, 50)
      : [],
    connected_at_ms:
      Number.isFinite(message.connected_at_ms) && message.connected_at_ms > 0
        ? message.connected_at_ms
        : Date.now()
  };
}

function makeSessionStats() {
  return {
    accepted_requests: 0,
    completed_requests: 0,
    failed_requests: 0,
    rejected_requests: 0,
    backpressure_rejections: 0,
    oversized_request_rejections: 0,
    oversized_response_rejections: 0,
    replaced_connections: 0,
    disconnects: 0,
    last_request_at_ms: null,
    last_response_at_ms: null,
    last_error_at_ms: null
  };
}

function recordRejected(stats) {
  stats.rejected_requests += 1;
  stats.last_error_at_ms = Date.now();
}

function recordFailure(stats) {
  stats.failed_requests += 1;
  stats.last_error_at_ms = Date.now();
}

function recordCompleted(stats, status) {
  stats.last_response_at_ms = Date.now();
  if (status >= 400) {
    recordFailure(stats);
    return;
  }
  stats.completed_requests += 1;
}

function jsonRpcError(id, code, message, status, data) {
  return jsonResponse(
    {
      jsonrpc: "2.0",
      id,
      error: {
        code,
        message,
        ...(data ? { data } : {})
      }
    },
    status
  );
}

async function safeJson(request) {
  try {
    return await request.json();
  } catch {
    return null;
  }
}

async function safeJsonFromText(text) {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

function requestIdFromBody(body) {
  if (body && Object.prototype.hasOwnProperty.call(body, "id")) {
    return body.id;
  }
  return null;
}

function toolCallName(rpcRequest) {
  if (
    !rpcRequest ||
    rpcRequest.method !== "tools/call" ||
    !rpcRequest.params ||
    typeof rpcRequest.params !== "object"
  ) {
    return "";
  }
  return typeof rpcRequest.params.name === "string" ? rpcRequest.params.name : "";
}

function toolTextPayload(rpcResponse) {
  const content =
    rpcResponse &&
    rpcResponse.result &&
    Array.isArray(rpcResponse.result.content)
      ? rpcResponse.result.content
      : [];
  const text = content.find((item) => item && item.type === "text" && typeof item.text === "string");
  if (!text) {
    return null;
  }
  return parseJson(text.text);
}

function isRecordNotFoundResponse(rpcResponse) {
  return Boolean(
    rpcResponse &&
      rpcResponse.error &&
      rpcResponse.error.data &&
      rpcResponse.error.data.code === "record_not_found"
  );
}

function gatewayCommandStatusResponse(id, cached) {
  return {
    jsonrpc: "2.0",
    id,
    result: {
      content: [
        {
          type: "text",
          text: JSON.stringify(
            {
              ok: true,
              record: {
                id: cached.command_id,
                collection: "business_commands",
                type: "business_command_status",
                source: cached.source,
                title: cached.command_id,
                status: cached.status,
                task_status: cached.task_status,
                task_id: cached.task_id || null,
                actor: cached.actor || null,
                workspace: cached.workspace || null,
                updated_at_ms: cached.updated_at_ms,
                fields: {
                  command_id: cached.command_id,
                  task_id: cached.task_id || null,
                  status: cached.status,
                  task_status: cached.task_status,
                  source: cached.source
                }
              }
            },
            null,
            2
          )
        }
      ]
    }
  };
}

function cleanCommandId(value) {
  if (typeof value !== "string") {
    return "";
  }
  const trimmed = value.trim();
  return /^[a-zA-Z0-9][a-zA-Z0-9._:-]{2,160}$/.test(trimmed) ? trimmed : "";
}

function pruneCommandStatuses(statuses, now = Date.now()) {
  for (const [commandId, status] of statuses.entries()) {
    if (!status || status.expires_at_ms < now) {
      statuses.delete(commandId);
    }
  }
  while (statuses.size > 256) {
    statuses.delete(statuses.keys().next().value);
  }
}

function parseJson(text) {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

export function normalizeJsonText(text) {
  if (typeof text !== "string" || text === "") {
    return "";
  }
  const parsed = parseJson(text);
  if (parsed) {
    return JSON.stringify(parsed);
  }
  const escaped = escapeControlCharactersInJsonStrings(text);
  const repaired = parseJson(escaped);
  return repaired ? JSON.stringify(repaired) : text;
}

function escapeControlCharactersInJsonStrings(text) {
  let output = "";
  let inString = false;
  let escaped = false;
  for (const char of text) {
    if (!inString) {
      output += char;
      if (char === '"') {
        inString = true;
      }
      continue;
    }
    if (escaped) {
      output += char;
      escaped = false;
      continue;
    }
    if (char === "\\") {
      output += char;
      escaped = true;
      continue;
    }
    if (char === '"') {
      output += char;
      inString = false;
      continue;
    }
    const code = char.codePointAt(0);
    if (code < 0x20) {
      output += `\\u${code.toString(16).padStart(4, "0")}`;
      continue;
    }
    output += char;
  }
  return output;
}

function checkContentLength(request, limit) {
  const raw = request.headers.get("content-length");
  if (!raw) {
    return { ok: true, limit };
  }
  const length = Number.parseInt(raw, 10);
  if (!Number.isFinite(length)) {
    return { ok: true, limit };
  }
  return { ok: length <= limit, limit };
}

function jsonResponse(body, status = 200) {
  const headers = new Headers({
    "content-type": "application/json; charset=utf-8",
    "cache-control": "no-store"
  });
  applyCors(headers);
  return new Response(JSON.stringify(body, null, 2), { status, headers });
}

function applyCors(headers) {
  headers.set("access-control-allow-origin", "*");
  headers.set("access-control-allow-methods", "GET, POST, OPTIONS");
  headers.set("access-control-allow-headers", "content-type, authorization");
}

function numericEnv(value, fallback) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return fallback;
  }
  return parsed;
}

function truthyEnv(value) {
  return ["1", "true", "yes", "on"].includes(String(value || "").trim().toLowerCase());
}

function maxMcpBodyBytes(env = {}) {
  return numericEnv(env.MAX_MCP_BODY_BYTES, DEFAULT_MAX_MCP_BODY_BYTES);
}

function maxMcpResponseBytes(env = {}) {
  return numericEnv(env.MAX_MCP_RESPONSE_BYTES, DEFAULT_MAX_MCP_RESPONSE_BYTES);
}

function maxPendingRequests(env = {}) {
  return numericEnv(env.MAX_PENDING_REQUESTS, DEFAULT_MAX_PENDING_REQUESTS);
}

function byteLength(value) {
  return new TextEncoder().encode(value || "").length;
}
