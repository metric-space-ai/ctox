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
          domain: "skf.ctox.dev",
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
  assert.equal(instances[0].sessionPartition, "persist:ctox-managed-managed:tenant_skf");
  assert.equal(instances[0].healthSummary.httpDataProxy, false);
});

test("ctox.dev source consumes launch token and launch config endpoints", async () => {
  const calls = [];
  const source = new CtoxDevInstanceSource({
    baseUrl: "https://ctox.dev",
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
          launchUrl: "https://skf.ctox.dev/?ctox_config=packed",
          pairingConfig: {
            transport: "webrtc",
            http_bridge_available: false,
          },
        });
      }
      throw new Error(`unexpected URL ${url}`);
    },
  });
  const launch = await source.getLaunchConfig("managed:tenant_skf");
  assert.equal(launch.source, "ctox_dev");
  assert.equal(launch.launchUrl, "https://skf.ctox.dev/?ctox_config=packed");
  assert.equal(launch.ctoxConfig.http_bridge_available, false);
  assert.equal(calls.length, 2);
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
              id: "tenant_kunstmen",
              slug: "kunstmen",
              domain: "kunstmen.ctox.dev",
              businessName: "Kunstmen",
              status: "active",
              healthStatus: "ok",
              tenantRole: "admin",
              launchAllowed: true,
            },
            ...(revoked ? [] : [{
              id: "tenant_skf",
              slug: "skf",
              domain: "skf.ctox.dev",
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
    ["managed:tenant_kunstmen", "managed:tenant_skf"],
  );
  revoked = true;
  assert.deepEqual(
    (await source.listInstances()).map((instance) => instance.id),
    ["managed:tenant_kunstmen"],
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
        launchUrl: `https://skf.ctox.dev/?ctox_config=token_${tokenCounter}`,
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
  assert.deepEqual(launchUrls, [
    ["https://ctox.dev/api/desktop/launch/token_1", "POST"],
    ["https://ctox.dev/api/desktop/launch/token_2", "POST"],
  ]);
});

function jsonResponse(payload) {
  return {
    ok: true,
    status: 200,
    json: async () => payload,
  };
}
