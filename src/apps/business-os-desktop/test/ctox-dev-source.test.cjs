"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const {
  CtoxDevInstanceSource,
  normalizeCtoxDevSessionPackage,
} = require("../src/main/sources.cjs");

test("normalizes ctox.dev session package into managed instances", () => {
  const instances = normalizeCtoxDevSessionPackage({
    account: {
      tenants: [
        {
          id: "tenant_skf",
          slug: "skf",
          domain: "acme.ctox.dev",
          businessName: "SKF",
          status: "active",
          healthStatus: "ok",
          tenantRole: "admin",
          launchAllowed: true,
        },
      ],
    },
  });
  assert.equal(instances.length, 1);
  assert.equal(instances[0].id, "managed:tenant_skf");
  assert.equal(instances[0].source, "ctox_dev");
  assert.equal(instances[0].displayName, "SKF");
  assert.match(instances[0].sessionPartition, /^persist:ctox-managed-[A-Za-z0-9_-]{18}$/);
  assert.equal(instances[0].healthSummary.httpDataProxy, false);
});

test("normalizes ctox.dev launch denial as non-launchable managed instance", () => {
  const instances = normalizeCtoxDevSessionPackage({
    account: {
      tenants: [
        {
          id: "tenant_revoked",
          slug: "revoked",
          domain: "revoked.ctox.dev",
          businessName: "Revoked",
          status: "active",
          healthStatus: "ok",
          tenantRole: "viewer",
          launchAllowed: false,
        },
      ],
    },
  });
  assert.equal(instances.length, 1);
  assert.equal(instances[0].id, "managed:tenant_revoked");
  assert.equal(instances[0].status, "needs_auth");
  assert.equal(instances[0].healthSummary.httpDataProxy, false);
});

test("ctox.dev source consumes launch token and launch config endpoints", async () => {
  const calls = [];
  const source = new CtoxDevInstanceSource({
    baseUrl: "https://ctox.dev",
    shellUrl: "https://ctox.dev/business-os/",
    shellUrl: "https://ctox.dev/business-os/",
    fetchImpl: async (url, options) => {
      calls.push([url, options]);
      if (url === "https://ctox.dev/api/desktop/launch-token") {
        assert.equal(options.method, "POST");
        assert.equal(JSON.parse(options.body).tenantId, "tenant_skf");
        return jsonResponse({ launchConfigUrl: "https://ctox.dev/api/desktop/launch/token_1", expiresAt: "2099-01-01T00:00:00Z" });
      }
      if (url === "https://ctox.dev/api/desktop/launch/token_1") {
        assert.equal(options.method, "POST");
        return jsonResponse({
          launchUrl: "https://acme.ctox.dev/",
          pairingConfig: {
            transport: "webrtc",
            http_bridge_available: false,
            sync_room: "ctox-business-os:skf",
          },
        });
      }
      throw new Error(`unexpected URL ${url}`);
    },
  });
  const launch = await source.getLaunchConfig("managed:tenant_skf");
  assert.equal(launch.source, "ctox_dev");
  const launchUrl = new URL(launch.launchUrl);
  assert.equal(launchUrl.origin, "https://ctox.dev");
  assert.equal(launchUrl.pathname, "/business-os/");
  assert.equal(launchUrl.searchParams.has("ctox_config"), true);
  assert.deepEqual(decodeCtoxConfig(launchUrl), {
    transport: "webrtc",
    http_bridge_available: false,
    sync_room: "ctox-business-os:skf",
  });
  assert.equal(launch.ctoxConfig.http_bridge_available, false);
  assert.equal(calls.length, 2);
});

test("ctox.dev source preserves server-packed launch URL when pairing metadata is redacted", async () => {
  const packedConfig = {
    transport: "webrtc",
    http_bridge_available: false,
    sync_room: "ctox-business-os:skf:real-room",
    signaling_room_password: "real-room-secret",
    signaling_urls: ["wss://signaling.ctox.dev/?token=real-token"],
  };
  const source = new CtoxDevInstanceSource({
    baseUrl: "https://ctox.dev",
    fetchImpl: async (url) => {
      if (url === "https://ctox.dev/api/desktop/launch-token") {
        return jsonResponse({ launchConfigUrl: "https://ctox.dev/api/desktop/launch/token_1" });
      }
      return jsonResponse({
        launchUrl: `https://acme.ctox.dev/?ctox_config=${Buffer.from(JSON.stringify(packedConfig), "utf8").toString("base64url")}`,
        pairingConfig: {
          transport: "webrtc",
          http_bridge_available: false,
          sync_room: "<redacted>",
          signaling_room_password: "<redacted>",
          signaling_urls: ["wss://signaling.ctox.dev/?token=<redacted>"],
        },
      });
    },
  });

  const launch = await source.getLaunchConfig("managed:tenant_skf");
  const launchUrl = new URL(launch.launchUrl);
  assert.equal(launchUrl.origin, "https://ctox.dev");
  assert.equal(launchUrl.pathname, "/business-os/");
  assert.deepEqual(decodeCtoxConfig(launchUrl), packedConfig);
  assert.equal(launch.ctoxConfig.signaling_room_password, "<redacted>");
});

test("ctox.dev managed launch carries the selected workspace identity into the bundled shell", async () => {
  const packedConfig = {
    transport: "webrtc",
    http_bridge_available: false,
    sync_room: "ctox-business-os:skf:real-room",
    signaling_room_password: "real-room-secret",
    signaling_urls: ["wss://signaling.ctox.dev/?token=real-token"],
    session: { capability_token: "native-signed-capability" },
  };
  const source = new CtoxDevInstanceSource({
    baseUrl: "https://ctox.dev",
    shellUrl: "http://127.0.0.1:8765/",
    fetchImpl: async (url) => {
      if (url === "https://ctox.dev/api/desktop/launch-token") {
        return jsonResponse({ launchConfigUrl: "https://ctox.dev/api/desktop/launch/token_1" });
      }
      return jsonResponse({
        launchUrl: `https://skf.ctox.dev/?ctox_config=${Buffer.from(JSON.stringify(packedConfig), "utf8").toString("base64url")}`,
        pairingConfig: {
          transport: "webrtc",
          http_bridge_available: false,
          sync_room: "<redacted>",
          signaling_room_password: "<redacted>",
        },
      });
    },
  });

  const launch = await source.getLaunchConfig("managed:tenant_skf", {
    id: "managed:tenant_skf",
    source: "ctox_dev",
    displayName: "SKF",
    domain: "skf.ctox.dev",
  });
  const config = decodeCtoxConfig(new URL(launch.launchUrl));
  assert.deepEqual(config.desktop_instance, {
    id: "managed:tenant_skf",
    source: "ctox_dev",
    display_name: "SKF",
    domain: "skf.ctox.dev",
  });
  assert.deepEqual(config.desktop_managed_auth, { required: true });
  assert.equal(config.session.capability_token, "native-signed-capability");
});

test("ctox.dev source ignores redaction markers outside pairing fields", async () => {
  const source = new CtoxDevInstanceSource({
    baseUrl: "https://ctox.dev",
    shellUrl: "https://ctox.dev/business-os/",
    fetchImpl: async (url) => {
      if (url === "https://ctox.dev/api/desktop/launch-token") {
        return jsonResponse({ launchConfigUrl: "https://ctox.dev/api/desktop/launch/token_1" });
      }
      return jsonResponse({
        launchUrl: "https://legacy.ctox.dev/",
        pairingConfig: {
          transport: "webrtc",
          sync_room: "ctox-business-os:skf",
          signaling_room_password: "room-secret",
          signaling_urls: ["wss://signaling.ctox.dev"],
          session: { diagnostic: "<redacted>" },
        },
      });
    },
  });

  const launch = await source.getLaunchConfig("managed:tenant_skf");
  const url = new URL(launch.launchUrl);
  assert.equal(url.origin, "https://ctox.dev");
  assert.equal(url.pathname, "/business-os/");
});

test("ctox.dev source rejects redacted pairing metadata without a packed launch URL", async () => {
  const source = new CtoxDevInstanceSource({
    baseUrl: "https://ctox.dev",
    fetchImpl: async (url) => {
      if (url === "https://ctox.dev/api/desktop/launch-token") {
        return jsonResponse({ launchConfigUrl: "https://ctox.dev/api/desktop/launch/token_1" });
      }
      return jsonResponse({
        launchUrl: "https://acme.ctox.dev/",
        pairingConfig: {
          transport: "webrtc",
          sync_room: "<redacted>",
          signaling_room_password: "<redacted>",
        },
      });
    },
  });

  await assert.rejects(
    () => source.getLaunchConfig("managed:tenant_skf"),
    /redacted pairing metadata/,
  );
});

test("ctox.dev source refreshes managed tenants after server-side revocation", async () => {
  let revoked = false;
  const source = new CtoxDevInstanceSource({
    baseUrl: "https://ctox.dev",
    fetchImpl: async (url) => {
      assert.equal(url, "https://ctox.dev/api/desktop/session-package");
      return jsonResponse({
        account: {
          tenants: [
            {
              id: "tenant_example",
              slug: "example",
              domain: "example.ctox.dev",
              businessName: "Example",
              status: "active",
              healthStatus: "ok",
              tenantRole: "admin",
              launchAllowed: true,
            },
            ...(revoked ? [] : [{
              id: "tenant_skf",
              slug: "skf",
              domain: "acme.ctox.dev",
              businessName: "SKF",
              status: "active",
              healthStatus: "ok",
              tenantRole: "owner",
              launchAllowed: true,
            }]),
          ],
        },
      });
    },
  });

  assert.deepEqual(
    (await source.listInstances()).map((instance) => instance.id),
    ["managed:tenant_example", "managed:tenant_skf"],
  );
  revoked = true;
  assert.deepEqual(
    (await source.listInstances()).map((instance) => instance.id),
    ["managed:tenant_example"],
  );
});

test("ctox.dev source requests a fresh launch token for each activation", async () => {
  let tokenCounter = 0;
  const launchUrls = [];
  const source = new CtoxDevInstanceSource({
    baseUrl: "https://ctox.dev",
    fetchImpl: async (url, options) => {
      if (url === "https://ctox.dev/api/desktop/launch-token") {
        tokenCounter += 1;
        return jsonResponse({
          launchConfigUrl: `https://ctox.dev/api/desktop/launch/token_${tokenCounter}`,
          expiresAt: `2099-01-01T00:00:0${tokenCounter}.000Z`,
        });
      }
      launchUrls.push([url, options.method]);
      return jsonResponse({
        launchUrl: "https://acme.ctox.dev/",
        pairingConfig: {
          transport: "webrtc",
          http_bridge_available: false,
          epoch: tokenCounter,
        },
      });
    },
  });

  const first = await source.getLaunchConfig("managed:tenant_skf");
  const second = await source.getLaunchConfig("managed:tenant_skf");
  assert.equal(first.expiresAt, "2099-01-01T00:00:01.000Z");
  assert.equal(second.expiresAt, "2099-01-01T00:00:02.000Z");
  assert.equal(first.ctoxConfig.epoch, 1);
  assert.equal(second.ctoxConfig.epoch, 2);
  assert.equal(decodeCtoxConfig(new URL(first.launchUrl)).epoch, 1);
  assert.equal(decodeCtoxConfig(new URL(second.launchUrl)).epoch, 2);
  assert.deepEqual(launchUrls, [
    ["https://ctox.dev/api/desktop/launch/token_1", "POST"],
    ["https://ctox.dev/api/desktop/launch/token_2", "POST"],
  ]);
});

test("ctox.dev source rejects an off-origin launch config URL (SSRF guard)", async () => {
  const source = new CtoxDevInstanceSource({
    baseUrl: "https://ctox.dev",
    fetchImpl: async (url) => {
      if (url === "https://ctox.dev/api/desktop/launch-token") {
        return jsonResponse({ launchConfigUrl: "https://evil.example/api/desktop/launch/token_1" });
      }
      throw new Error(`launch config URL must not be fetched: ${url}`);
    },
  });
  await assert.rejects(
    () => source.getLaunchConfig("managed:tenant_skf"),
    /control-plane origin/,
  );
});

test("ctox.dev source rejects a launch config that tries to enable the HTTP bridge", async () => {
  const source = new CtoxDevInstanceSource({
    baseUrl: "https://ctox.dev",
    fetchImpl: async (url) => {
      if (url === "https://ctox.dev/api/desktop/launch-token") {
        return jsonResponse({ launchConfigUrl: "https://ctox.dev/api/desktop/launch/token_1" });
      }
      return jsonResponse({
        launchUrl: "https://acme.ctox.dev/",
        pairingConfig: { transport: "webrtc", http_bridge_available: true, sync_room: "ctox-business-os:skf" },
      });
    },
  });
  await assert.rejects(
    () => source.getLaunchConfig("managed:tenant_skf"),
    /HTTP data bridge/,
  );
});

test("ctox.dev source exposes the redacted public launch failure", async () => {
  const source = new CtoxDevInstanceSource({
    baseUrl: "https://ctox.dev",
    fetchImpl: async (url) => {
      if (url === "https://ctox.dev/api/desktop/launch-token") {
        return jsonResponse({ launchConfigUrl: "https://ctox.dev/api/desktop/launch/token_1" });
      }
      return {
        ok: false,
        status: 400,
        json: async () => ({ code: "capability_issue_failed", error: "CTOX capability command is unavailable." }),
      };
    },
  });
  await assert.rejects(
    () => source.getLaunchConfig("managed:tenant_skf"),
    /400 \(CTOX capability command is unavailable\.\)/,
  );
});

function decodeCtoxConfig(url) {
  return JSON.parse(Buffer.from(url.searchParams.get("ctox_config"), "base64url").toString("utf8"));
}

function jsonResponse(payload) {
  return {
    ok: true,
    status: 200,
    json: async () => payload,
  };
}
