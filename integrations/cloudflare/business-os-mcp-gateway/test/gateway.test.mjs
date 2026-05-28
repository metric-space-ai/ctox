import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import {
  BusinessOsMcpSession,
  acceptConnectReplay,
  allowedInstanceIds,
  authorizeMcpClient,
  authorizeInstanceConnect,
  authorizeInstanceId,
  gatewayMode,
  authorize,
  connectTokenForInstance,
  handleRequest,
  instanceConnectTokens,
  mcpClientTokens,
  normalizeJsonText,
  normalizedUpstreamUrl,
  parseManagedRoute,
  publicGatewayConfig,
  validateConnectReplayHeaders
} from "../src/index.js";

const originalFetch = globalThis.fetch;

afterEach(() => {
  globalThis.fetch = originalFetch;
});

test("health reports unbound mode without an upstream", async () => {
  const response = await handleRequest(new Request("https://mcp.ctox.dev/health"));
  const body = await response.json();

  assert.equal(response.status, 200);
  assert.equal(body.ok, true);
  assert.equal(body.mode, "unbound");
  assert.equal(response.headers.get("cache-control"), "no-store");
  assert.equal(body.config.mcp_gateway_auth_required, false);
});

test("health reports managed rendezvous when Durable Object binding exists", async () => {
  const response = await handleRequest(new Request("https://mcp.ctox.dev/health"), {
    BUSINESS_OS_MCP_SESSIONS: fakeSessionsBinding()
  });
  const body = await response.json();

  assert.equal(response.status, 200);
  assert.equal(body.mode, "managed_rendezvous");
  assert.equal(body.config.managed_rendezvous_configured, true);
  assert.equal(gatewayMode({ UPSTREAM_MCP_URL: "https://example.com/mcp" }), "http_relay");
});

test("health config exposes only non-secret gateway posture", () => {
  const config = publicGatewayConfig({
    BUSINESS_OS_MCP_SESSIONS: fakeSessionsBinding(),
    ALLOWED_INSTANCE_IDS: "desk_123",
    MCP_GATEWAY_TOKEN: "client-secret",
    INSTANCE_CONNECT_TOKENS: "desk_123=connect-secret",
    MAX_MCP_BODY_BYTES: "42"
  });
  const serialized = JSON.stringify(config);

  assert.equal(config.allowed_instance_ids_configured, true);
  assert.equal(config.mcp_gateway_auth_required, true);
  assert.equal(config.instance_connect_auth_required, true);
  assert.equal(config.connect_replay_guard_required, true);
  assert.equal(config.limits.max_mcp_body_bytes, 42);
  assert.equal(serialized.includes("client-secret"), false);
  assert.equal(serialized.includes("connect-secret"), false);
});

test("gateway auth is optional unless MCP_GATEWAY_TOKEN is configured", () => {
  const request = new Request("https://mcp.ctox.dev/mcp");

  assert.equal(authorize(request, {}).ok, true);
  assert.equal(authorize(request, { MCP_GATEWAY_TOKEN: "secret" }).ok, false);
  assert.equal(
    authorize(
      new Request("https://mcp.ctox.dev/mcp", {
        headers: { authorization: "Bearer secret" }
      }),
      { MCP_GATEWAY_TOKEN: "secret" }
    ).ok,
    true
  );
});

test("mcp client token registry binds bearer tokens to actor context", () => {
  const env = {
    MCP_CLIENT_TOKENS: JSON.stringify({
      "client-token-123": {
        actor: "ctox-dev:user:user_1",
        workspace: "tenant:tenant_1",
        tenant_id: "tenant_1",
        role: "admin",
        client_id: "codex",
        allowed_instances: ["desk_123"]
      }
    })
  };

  assert.equal(mcpClientTokens(env).get("client-token-123").actor, "ctox-dev:user:user_1");
  const authorized = authorizeMcpClient(
    new Request("https://mcp.ctox.dev/mcp/desk_123", {
      headers: { authorization: "Bearer client-token-123" }
    }),
    env,
    "desk_123"
  );
  const denied = authorizeMcpClient(
    new Request("https://mcp.ctox.dev/mcp/other", {
      headers: { authorization: "Bearer client-token-123" }
    }),
    env,
    "other"
  );

  assert.equal(authorized.ok, true);
  assert.equal(authorized.context.actor, "ctox-dev:user:user_1");
  assert.equal(authorized.context.workspace, "tenant:tenant_1");
  assert.equal(authorized.context.instance_id, "desk_123");
  assert.equal(denied.ok, false);
});

test("managed mcp forwards server-authenticated context to session object", async () => {
  let forwardedContext = null;
  const response = await handleRequest(
    new Request("https://mcp.ctox.dev/mcp/desk_123", {
      method: "POST",
      headers: { authorization: "Bearer client-token-123" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 3,
        method: "tools/call",
        params: {
          name: "business_os.status",
          arguments: {
            _context: { actor: "spoofed", workspace: "spoofed" }
          }
        }
      })
    }),
    {
      MCP_CLIENT_TOKENS: JSON.stringify({
        "client-token-123": {
          actor: "ctox-dev:user:user_1",
          workspace: "tenant:tenant_1",
          allowed_instances: ["desk_123"]
        }
      }),
      BUSINESS_OS_MCP_SESSIONS: fakeSessionsBinding(async (request) => {
        forwardedContext = JSON.parse(request.headers.get("x-ctox-mcp-gateway-context"));
        return new Response(JSON.stringify({ jsonrpc: "2.0", id: 3, result: { ok: true } }), {
          headers: { "content-type": "application/json" }
        });
      })
    }
  );

  assert.equal(response.status, 200);
  assert.equal(forwardedContext.actor, "ctox-dev:user:user_1");
  assert.equal(forwardedContext.workspace, "tenant:tenant_1");
  assert.equal(forwardedContext.instance_id, "desk_123");
});

test("instance connect auth is optional unless INSTANCE_CONNECT_TOKEN is configured", () => {
  const request = new Request("https://mcp.ctox.dev/connect/desk_123");

  assert.equal(authorizeInstanceConnect(request, {}).ok, true);
  assert.equal(authorizeInstanceConnect(request, { INSTANCE_CONNECT_TOKEN: "secret" }).ok, false);
  assert.equal(
    authorizeInstanceConnect(
      new Request("https://mcp.ctox.dev/connect/desk_123", {
        headers: { authorization: "Bearer secret" }
      }),
      { INSTANCE_CONNECT_TOKEN: "secret" }
    ).ok,
    true
  );
});

test("instance connect auth supports instance-scoped tokens", () => {
  const env = { INSTANCE_CONNECT_TOKENS: "desk_123=desk-secret,other=other-secret" };

  assert.equal(connectTokenForInstance(env, "desk_123"), "desk-secret");
  assert.equal(instanceConnectTokens(env).get("other"), "other-secret");
  assert.equal(
    authorizeInstanceConnect(
      new Request("https://mcp.ctox.dev/connect/desk_123", {
        headers: { authorization: "Bearer desk-secret" }
      }),
      env,
      "desk_123"
    ).ok,
    true
  );
  assert.equal(
    authorizeInstanceConnect(
      new Request("https://mcp.ctox.dev/connect/desk_123", {
        headers: { authorization: "Bearer other-secret" }
      }),
      env,
      "desk_123"
    ).ok,
    false
  );
});

test("connect replay guard requires fresh timestamp and nonce when connect token exists", () => {
  const env = { INSTANCE_CONNECT_TOKEN: "secret", CONNECT_REPLAY_WINDOW_MS: "1000" };
  const fresh = new Request("https://mcp.ctox.dev/connect/desk_123", {
    headers: connectHeaders("secret", "nonce-fresh-123", Date.now())
  });
  const stale = new Request("https://mcp.ctox.dev/connect/desk_123", {
    headers: connectHeaders("secret", "nonce-stale-123", Date.now() - 10_000)
  });

  assert.equal(validateConnectReplayHeaders(new Request("https://mcp.ctox.dev/connect/desk_123"), env).ok, false);
  assert.equal(validateConnectReplayHeaders(fresh, env).ok, true);
  assert.equal(validateConnectReplayHeaders(stale, env).code, "connect_replay_timestamp_out_of_window");
});

test("connect replay guard rejects reused nonces", () => {
  const env = { INSTANCE_CONNECT_TOKEN: "secret" };
  const nonces = new Map();
  const request = new Request("https://mcp.ctox.dev/connect/desk_123", {
    headers: connectHeaders("secret", "nonce-reused-123", Date.now())
  });

  assert.equal(acceptConnectReplay(request, env, nonces).ok, true);
  const second = acceptConnectReplay(request, env, nonces);
  assert.equal(second.ok, false);
  assert.equal(second.code, "connect_replay_detected");
});

test("allowed instance ids constrain managed gateway routing", () => {
  assert.equal(authorizeInstanceId("desk_123", {}).ok, true);
  assert.equal(
    authorizeInstanceId("desk_123", { ALLOWED_INSTANCE_IDS: "desk_123,org:desk.1" }).ok,
    true
  );
  assert.equal(
    authorizeInstanceId("other", { ALLOWED_INSTANCE_IDS: "desk_123,org:desk.1" }).ok,
    false
  );
  assert.deepEqual([...allowedInstanceIds({ ALLOWED_INSTANCE_IDS: "desk_123,bad/id,org:desk.1" })], [
    "desk_123",
    "org:desk.1"
  ]);
});

test("mcp returns sync_not_ready when no upstream is connected", async () => {
  const response = await handleRequest(
    new Request("https://mcp.ctox.dev/mcp", {
      method: "POST",
      body: JSON.stringify({ jsonrpc: "2.0", id: 7, method: "tools/list" })
    }),
    {}
  );
  const body = await response.json();

  assert.equal(response.status, 503);
  assert.equal(body.id, 7);
  assert.equal(body.error.data.code, "sync_not_ready");
});

test("mcp relays JSON-RPC calls to a configured upstream", async () => {
  let upstreamBody = null;
  globalThis.fetch = async (_url, init) => {
    upstreamBody = init.body;
    return new Response(JSON.stringify({ jsonrpc: "2.0", id: 1, result: { ok: true } }), {
      status: 200,
      headers: { "content-type": "application/json" }
    });
  };

  const response = await handleRequest(
    new Request("https://mcp.ctox.dev/mcp", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/list" })
    }),
    { UPSTREAM_MCP_URL: "https://example.com/mcp" }
  );
  const body = await response.json();

  assert.equal(response.status, 200);
  assert.equal(JSON.parse(upstreamBody).method, "tools/list");
  assert.equal(body.result.ok, true);
});

test("managed mcp route forwards to the instance session object", async () => {
  let forwarded = null;
  const response = await handleRequest(
    new Request("https://mcp.ctox.dev/mcp/desk_123", {
      method: "POST",
      body: JSON.stringify({ jsonrpc: "2.0", id: 3, method: "tools/list" })
    }),
    {
      BUSINESS_OS_MCP_SESSIONS: fakeSessionsBinding(async (request, instanceId) => {
        forwarded = { request, instanceId };
        return new Response(JSON.stringify({ jsonrpc: "2.0", id: 3, result: { ok: true } }), {
          headers: { "content-type": "application/json" }
        });
      })
    }
  );
  const body = await response.json();

  assert.equal(response.status, 200);
  assert.equal(forwarded.instanceId, "desk_123");
  assert.equal(new URL(forwarded.request.url).pathname, "/mcp");
  assert.equal(body.result.ok, true);
});

test("managed mcp route rejects bodies over configured content length", async () => {
  const response = await handleRequest(
    new Request("https://mcp.ctox.dev/mcp/desk_123", {
      method: "POST",
      headers: { "content-length": "32" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 31, method: "tools/list" })
    }),
    {
      MAX_MCP_BODY_BYTES: "16",
      BUSINESS_OS_MCP_SESSIONS: fakeSessionsBinding()
    }
  );
  const body = await response.json();

  assert.equal(response.status, 413);
  assert.equal(body.error.data.code, "request_too_large");
  assert.equal(body.error.data.limit_bytes, 16);
});

test("managed mcp route rejects disallowed instance ids before routing", async () => {
  let routed = false;
  const response = await handleRequest(
    new Request("https://mcp.ctox.dev/mcp/other", {
      method: "POST",
      body: JSON.stringify({ jsonrpc: "2.0", id: 41, method: "tools/list" })
    }),
    {
      ALLOWED_INSTANCE_IDS: "desk_123",
      BUSINESS_OS_MCP_SESSIONS: fakeSessionsBinding(async () => {
        routed = true;
        return new Response("{}");
      })
    }
  );
  const body = await response.json();

  assert.equal(response.status, 403);
  assert.equal(body.error.data.code, "instance_not_allowed");
  assert.equal(routed, false);
});

test("managed connect route forwards only after instance authorization", async () => {
  const unauthorized = await handleRequest(
    new Request("https://mcp.ctox.dev/connect/desk_123", {
      headers: { authorization: "Bearer wrong" }
    }),
    {
      INSTANCE_CONNECT_TOKEN: "secret",
      BUSINESS_OS_MCP_SESSIONS: fakeSessionsBinding()
    }
  );

  assert.equal(unauthorized.status, 401);

  let forwardedPath = null;
  const authorized = await handleRequest(
    new Request("https://mcp.ctox.dev/connect/desk_123", {
      headers: connectHeaders("secret", "nonce-authorized-123", Date.now())
    }),
    {
      INSTANCE_CONNECT_TOKEN: "secret",
      BUSINESS_OS_MCP_SESSIONS: fakeSessionsBinding(async (request) => {
        forwardedPath = new URL(request.url).pathname;
        return new Response(null, { status: 426 });
      })
    }
  );

  assert.equal(authorized.status, 426);
  assert.equal(forwardedPath, "/connect");
});

test("managed connect route requires replay headers after authorization", async () => {
  let routed = false;
  const response = await handleRequest(
    new Request("https://mcp.ctox.dev/connect/desk_123", {
      headers: { authorization: "Bearer secret" }
    }),
    {
      INSTANCE_CONNECT_TOKEN: "secret",
      BUSINESS_OS_MCP_SESSIONS: fakeSessionsBinding(async () => {
        routed = true;
        return new Response(null, { status: 426 });
      })
    }
  );
  const body = await response.json();

  assert.equal(response.status, 401);
  assert.equal(body.error, "connect_replay_header_required");
  assert.equal(routed, false);
});

test("managed connect and status reject disallowed instance ids", async () => {
  const connect = await handleRequest(
    new Request("https://mcp.ctox.dev/connect/other", {
      headers: connectHeaders("secret", "nonce-disallowed-123", Date.now())
    }),
    {
      INSTANCE_CONNECT_TOKEN: "secret",
      ALLOWED_INSTANCE_IDS: "desk_123",
      BUSINESS_OS_MCP_SESSIONS: fakeSessionsBinding()
    }
  );
  const status = await handleRequest(
    new Request("https://mcp.ctox.dev/status/other"),
    {
      ALLOWED_INSTANCE_IDS: "desk_123",
      BUSINESS_OS_MCP_SESSIONS: fakeSessionsBinding()
    }
  );

  assert.equal(connect.status, 403);
  assert.equal((await connect.json()).error, "instance_not_allowed");
  assert.equal(status.status, 403);
  assert.equal((await status.json()).error, "instance_not_allowed");
});

test("managed status route reports a specific instance session", async () => {
  let forwardedPath = null;
  const response = await handleRequest(
    new Request("https://mcp.ctox.dev/status/desk_123", {
      headers: { authorization: "Bearer secret" }
    }),
    {
      MCP_GATEWAY_TOKEN: "secret",
      BUSINESS_OS_MCP_SESSIONS: fakeSessionsBinding(async (request) => {
        forwardedPath = new URL(request.url).pathname;
        return new Response(
          JSON.stringify({
            ok: true,
            connected: true,
            pending: 2
          }),
          { headers: { "content-type": "application/json" } }
        );
      })
    }
  );
  const body = await response.json();

  assert.equal(response.status, 200);
  assert.equal(forwardedPath, "/status");
  assert.equal(body.connected, true);
  assert.equal(body.pending, 2);
});

test("managed route parser accepts only bounded instance ids", () => {
  assert.deepEqual(parseManagedRoute("/mcp/desk_123"), {
    kind: "mcp",
    instanceId: "desk_123"
  });
  assert.deepEqual(parseManagedRoute("/connect/org:desk.1"), {
    kind: "connect",
    instanceId: "org:desk.1"
  });
  assert.deepEqual(parseManagedRoute("/status/org:desk.1"), {
    kind: "status",
    instanceId: "org:desk.1"
  });
  assert.equal(parseManagedRoute("/mcp/no"), null);
  assert.equal(parseManagedRoute("/mcp/../../../etc"), null);
  assert.equal(parseManagedRoute("/mcp/desk_123/extra"), null);
});

test("session object returns runtime_unavailable when no CTOX socket is connected", async () => {
  const session = new BusinessOsMcpSession({}, {});
  const response = await session.fetch(
    new Request("https://session.local/mcp", {
      method: "POST",
      body: JSON.stringify({ jsonrpc: "2.0", id: 11, method: "tools/list" })
    })
  );
  const body = await response.json();

  assert.equal(response.status, 503);
  assert.equal(body.id, 11);
  assert.equal(body.error.data.code, "runtime_unavailable");
});

test("session object relays request bodies through the connected CTOX socket", async () => {
  const session = new BusinessOsMcpSession({}, {});
  const socket = new FakeSocket((message) => {
    const envelope = JSON.parse(message);
    assert.equal(envelope.type, "mcp_request");
    assert.equal(JSON.parse(envelope.body).method, "tools/list");
    socket.emit(
      "message",
      JSON.stringify({
        type: "mcp_response",
        request_id: envelope.request_id,
        status: 200,
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", id: 12, result: { ok: true } })
      })
    );
  });
  session.bindSocket(socket);

  const response = await session.fetch(
    new Request("https://session.local/mcp", {
      method: "POST",
      body: JSON.stringify({ jsonrpc: "2.0", id: 12, method: "tools/list" })
    })
  );
  const body = await response.json();

  assert.equal(response.status, 200);
  assert.equal(body.result.ok, true);
  assert.equal(socket.sent.length, 1);

  const status = await session.fetch(new Request("https://session.local/status"));
  const statusBody = await status.json();
  assert.equal(statusBody.stats.accepted_requests, 1);
  assert.equal(statusBody.stats.completed_requests, 1);
  assert.equal(statusBody.stats.failed_requests, 0);
  assert.ok(statusBody.stats.last_request_at_ms > 0);
  assert.ok(statusBody.stats.last_response_at_ms > 0);
});

test("session object sends gateway-authenticated context in CTOX envelope", async () => {
  const session = new BusinessOsMcpSession({}, {});
  const socket = new FakeSocket((message) => {
    const envelope = JSON.parse(message);
    assert.equal(envelope.context.actor, "ctox-dev:user:user_1");
    assert.equal(envelope.context.workspace, "tenant:tenant_1");
    socket.emit(
      "message",
      JSON.stringify({
        type: "mcp_response",
        request_id: envelope.request_id,
        status: 200,
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", id: 18, result: { ok: true } })
      })
    );
  });
  session.bindSocket(socket);

  const response = await session.fetch(
    new Request("https://session.local/mcp", {
      method: "POST",
      headers: {
        "x-ctox-mcp-gateway-context": JSON.stringify({
          actor: "ctox-dev:user:user_1",
          workspace: "tenant:tenant_1"
        })
      },
      body: JSON.stringify({ jsonrpc: "2.0", id: 18, method: "tools/list" })
    })
  );
  const body = await response.json();

  assert.equal(response.status, 200);
  assert.equal(body.result.ok, true);
});

test("session object applies pending request backpressure", async () => {
  const session = new BusinessOsMcpSession({}, { MAX_PENDING_REQUESTS: "1" });
  const socket = new FakeSocket(() => {});
  session.bindSocket(socket);
  session.pending.set("already_pending", {
    resolve() {},
    reject() {},
    timeout: setTimeout(() => {}, 10_000)
  });

  const response = await session.fetch(
    new Request("https://session.local/mcp", {
      method: "POST",
      body: JSON.stringify({ jsonrpc: "2.0", id: 16, method: "tools/list" })
    })
  );
  const body = await response.json();

  assert.equal(response.status, 429);
  assert.equal(body.error.data.code, "backpressure");
  assert.equal(body.error.data.limit, 1);
  assert.equal(socket.sent.length, 0);
  assert.equal(session.stats.rejected_requests, 1);
  assert.equal(session.stats.backpressure_rejections, 1);
  clearTimeout(session.pending.get("already_pending").timeout);
  session.pending.clear();
});

test("session object records CTOX hello metadata for status", async () => {
  const session = new BusinessOsMcpSession({}, {});
  const socket = new FakeSocket(() => {});
  session.bindSocket(socket);
  socket.emit(
    "message",
    JSON.stringify({
      type: "ctox_hello",
      ctox_version: "0.3.22",
      mcp_protocol_version: "2025-06-18",
      capabilities: ["business_os_mcp_channel_v1", "managed_gateway_connector"],
      connected_at_ms: 1234
    })
  );

  const response = await session.fetch(new Request("https://session.local/status"));
  const body = await response.json();

  assert.equal(body.connected, true);
  assert.equal(body.session.ctox_version, "0.3.22");
  assert.equal(body.session.mcp_protocol_version, "2025-06-18");
  assert.deepEqual(body.session.capabilities, [
    "business_os_mcp_channel_v1",
    "managed_gateway_connector"
  ]);
  assert.equal(body.session.connected_at_ms, 1234);
});

test("session object clears CTOX hello metadata on disconnect", async () => {
  const session = new BusinessOsMcpSession({}, {});
  const socket = new FakeSocket(() => {});
  session.bindSocket(socket);
  socket.emit(
    "message",
    JSON.stringify({
      type: "ctox_hello",
      ctox_version: "0.3.22",
      mcp_protocol_version: "2025-06-18"
    })
  );
  socket.close();

  const response = await session.fetch(new Request("https://session.local/status"));
  const body = await response.json();

  assert.equal(body.connected, false);
  assert.equal(body.session, null);
});

test("session object rejects oversized request bodies before socket send", async () => {
  const session = new BusinessOsMcpSession({}, { MAX_MCP_BODY_BYTES: "12" });
  const socket = new FakeSocket(() => {
    throw new Error("oversized request must not reach socket");
  });
  session.bindSocket(socket);

  const response = await session.fetch(
    new Request("https://session.local/mcp", {
      method: "POST",
      body: JSON.stringify({ jsonrpc: "2.0", id: 14, method: "tools/list" })
    })
  );
  const body = await response.json();

  assert.equal(response.status, 413);
  assert.equal(body.error.data.code, "request_too_large");
  assert.equal(socket.sent.length, 0);
  assert.equal(session.stats.rejected_requests, 1);
  assert.equal(session.stats.oversized_request_rejections, 1);
});

test("session object converts oversized CTOX responses into bounded gateway errors", async () => {
  const session = new BusinessOsMcpSession({}, { MAX_MCP_RESPONSE_BYTES: "10" });
  const socket = new FakeSocket((message) => {
    const envelope = JSON.parse(message);
    socket.emit(
      "message",
      JSON.stringify({
        type: "mcp_response",
        request_id: envelope.request_id,
        status: 200,
        headers: { "content-type": "application/json" },
        body: "x".repeat(40)
      })
    );
  });
  session.bindSocket(socket);

  const response = await session.fetch(
    new Request("https://session.local/mcp", {
      method: "POST",
      body: JSON.stringify({ jsonrpc: "2.0", id: 15, method: "tools/list" })
    })
  );
  const body = await response.json();

  assert.equal(response.status, 502);
  assert.equal(body.error.data.code, "response_too_large");
  assert.equal(body.error.data.limit_bytes, 10);
  assert.equal(session.stats.failed_requests, 1);
  assert.equal(session.stats.oversized_response_rejections, 1);
});

test("session object fails pending requests when CTOX socket disconnects", async () => {
  const session = new BusinessOsMcpSession({}, {});
  const socket = new FakeSocket(() => {
    socket.close();
  });
  session.bindSocket(socket);

  const response = await session.fetch(
    new Request("https://session.local/mcp", {
      method: "POST",
      body: JSON.stringify({ jsonrpc: "2.0", id: 13, method: "tools/list" })
    })
  );
  const body = await response.json();

  assert.equal(response.status, 504);
  assert.equal(body.error.data.code, "runtime_unavailable");
  assert.equal(session.pending.size, 0);
});

test("session object fails pending requests when CTOX socket is replaced", async () => {
  const session = new BusinessOsMcpSession({}, {});
  const firstSocket = new FakeSocket(() => {});
  const secondSocket = new FakeSocket(() => {});
  session.bindSocket(firstSocket);
  const pending = session.fetch(
    new Request("https://session.local/mcp", {
      method: "POST",
      body: JSON.stringify({ jsonrpc: "2.0", id: 17, method: "tools/list" })
    })
  );

  await waitFor(() => session.pending.size === 1);
  assert.equal(session.pending.size, 1);
  session.bindSocket(secondSocket);
  const response = await pending;
  const body = await response.json();

  assert.equal(response.status, 504);
  assert.equal(body.error.data.code, "runtime_unavailable");
  assert.match(body.error.message, /replaced/);
  assert.equal(session.pending.size, 0);
  assert.equal(session.socket, secondSocket);
  assert.equal(session.stats.replaced_connections, 1);
  assert.equal(session.stats.failed_requests, 1);
});

test("upstream url must be https outside local development", () => {
  assert.equal(
    normalizedUpstreamUrl({ UPSTREAM_MCP_URL: "http://127.0.0.1:8788/mcp" }),
    "http://127.0.0.1:8788/mcp"
  );
  assert.throws(() => normalizedUpstreamUrl({ UPSTREAM_MCP_URL: "http://example.com/mcp" }));
});

test("normalizes raw control characters inside JSON strings", () => {
  const invalid = '{"jsonrpc":"2.0","result":{"content":[{"type":"text","text":"{\n  \\"ok\\": true\n}"}]}}';
  const normalized = normalizeJsonText(invalid);
  const parsed = JSON.parse(normalized);

  assert.equal(parsed.result.content[0].text, '{\n  "ok": true\n}');
});

function fakeSessionsBinding(handler = defaultSessionHandler) {
  return {
    idFromName(name) {
      return { name };
    },
    get(id) {
      return {
        fetch(request) {
          return handler(request, id.name);
        }
      };
    }
  };
}

async function defaultSessionHandler() {
  return new Response(JSON.stringify({ ok: true }), {
    headers: { "content-type": "application/json" }
  });
}

function connectHeaders(token, nonce, timestamp) {
  return {
    authorization: `Bearer ${token}`,
    "x-ctox-mcp-nonce": nonce,
    "x-ctox-mcp-timestamp": String(timestamp)
  };
}

class FakeSocket {
  constructor(onSend) {
    this.onSend = onSend;
    this.sent = [];
    this.listeners = new Map();
  }

  send(message) {
    this.sent.push(message);
    queueMicrotask(() => this.onSend(message));
  }

  close() {
    this.emit("close", "");
  }

  addEventListener(event, listener) {
    const listeners = this.listeners.get(event) || [];
    listeners.push(listener);
    this.listeners.set(event, listeners);
  }

  emit(event, data) {
    for (const listener of this.listeners.get(event) || []) {
      listener({ data });
    }
  }
}

async function waitFor(predicate) {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    if (predicate()) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}
