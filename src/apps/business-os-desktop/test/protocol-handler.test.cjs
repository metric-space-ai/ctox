"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const EventEmitter = require("node:events");
const {
  extractDesktopProtocolUrls,
  handleDesktopProtocolUrl,
  installDesktopProtocolHandling,
  parseDesktopProtocolUrl,
} = require("../src/main/protocol-handler.cjs");

test("parses managed instance protocol links from ctox.dev", () => {
  const parsed = parseDesktopProtocolUrl("ctox-business-os-desktop://instance/tenant_skf");
  assert.equal(parsed.action, "instance");
  assert.equal(parsed.tenantId, "tenant_skf");
  assert.equal(parsed.instanceId, "managed:tenant_skf");
});

test("parses pairing protocol links", () => {
  const parsed = parseDesktopProtocolUrl("ctox-business-os-desktop://pair?payload=abc");
  assert.equal(parsed.action, "pair");
  assert.equal(parsed.payload, "abc");
});

test("parses ctox.dev auth callback protocol links", () => {
  const parsed = parseDesktopProtocolUrl("ctox-business-os-desktop://auth/callback?desktop=1");
  assert.equal(parsed.action, "auth");
  assert.equal(parsed.callbackUrl, "ctox-business-os-desktop://auth/callback?desktop=1");
  assert.throws(
    () => parseDesktopProtocolUrl("ctox-business-os-desktop://auth/not-callback"),
    /auth URL is missing callback path/,
  );
});

test("dispatches protocol links to the right handler", async () => {
  const calls = [];
  await handleDesktopProtocolUrl("ctox-business-os-desktop://instance/tenant_1", {
    importInvite: async () => calls.push("invite"),
    activateManagedInstance: async (id) => calls.push(id),
  });
  await handleDesktopProtocolUrl("ctox-business-os-desktop://auth/callback", {
    handleCtoxDevAuthCallback: async (rawUrl) => calls.push(rawUrl),
  });
  assert.deepEqual(calls, ["managed:tenant_1", "ctox-business-os-desktop://auth/callback"]);
});

test("deep-link pair/instance actions are blocked when the user declines confirmation", async () => {
  const calls = [];
  const handlers = {
    importInvite: async (raw) => calls.push(["invite", raw]),
    activateManagedInstance: async (id) => calls.push(["managed", id]),
  };
  const deny = async () => false;
  const pair = await handleDesktopProtocolUrl(
    "ctox-business-os-desktop://pair?payload=abc",
    handlers,
    { confirmAction: deny },
  );
  const inst = await handleDesktopProtocolUrl(
    "ctox-business-os-desktop://instance/tenant_x",
    handlers,
    { confirmAction: deny },
  );
  assert.deepEqual(calls, []);
  assert.equal(pair.declined, true);
  assert.equal(inst.declined, true);

  // An approving confirmation lets the action through.
  await handleDesktopProtocolUrl(
    "ctox-business-os-desktop://instance/tenant_x",
    handlers,
    { confirmAction: async () => true },
  );
  assert.deepEqual(calls, [["managed", "managed:tenant_x"]]);
});

test("extracts desktop protocol links from noisy process argv", () => {
  assert.deepEqual(extractDesktopProtocolUrls([
    "/Applications/CTOX.app",
    "--flag",
    "ctox-business-os-desktop://instance/tenant_1",
    "https://ctox.dev",
    "ctox-business-os-desktop://pair?payload=abc",
  ]), [
    "ctox-business-os-desktop://instance/tenant_1",
    "ctox-business-os-desktop://pair?payload=abc",
  ]);
});

test("protocol handling queues os links until the app is ready", async () => {
  const app = new FakeElectronApp();
  const calls = [];
  let ready = false;
  const handling = installDesktopProtocolHandling({
    app,
    argv: ["ctox-business-os-desktop://instance/tenant_cold"],
    registerDefaultProtocolClient: false,
    singleInstanceLock: false,
    isReady: () => ready,
    handlersProvider: () => ({
      importInvite: async (rawInvite) => calls.push(["invite", rawInvite]),
      activateManagedInstance: async (id) => calls.push(["managed", id]),
    }),
  });
  app.emit("open-url", { preventDefault: () => calls.push(["prevented"]) }, "ctox-business-os-desktop://pair?payload=warm");
  assert.deepEqual(calls, [["prevented"]]);
  assert.equal(handling.getPendingUrls().length, 2);
  ready = true;
  await handling.flushPending();
  app.emit("second-instance", {}, ["--flag", "ctox-business-os-desktop://instance/tenant_second"]);
  await handling.waitForIdle();
  assert.deepEqual(calls, [
    ["prevented"],
    ["managed", "managed:tenant_cold"],
    ["invite", "ctox-business-os-desktop://pair?payload=warm"],
    ["managed", "managed:tenant_second"],
  ]);
});

test("protocol handling registers the scheme and quits without single-instance lock", () => {
  const app = new FakeElectronApp({ lock: false });
  const handling = installDesktopProtocolHandling({
    app,
    argv: [],
    isReady: () => true,
    handlersProvider: () => ({}),
  });
  assert.equal(handling.gotSingleInstanceLock, false);
  assert.equal(app.quitCalled, true);
  assert.equal(handling.registerDefaultProtocolClient(), true);
  assert.deepEqual(app.registeredProtocols, ["ctox-business-os-desktop"]);
});

class FakeElectronApp extends EventEmitter {
  constructor({ lock = true } = {}) {
    super();
    this.lock = lock;
    this.quitCalled = false;
    this.registeredProtocols = [];
  }

  requestSingleInstanceLock() {
    return this.lock;
  }

  quit() {
    this.quitCalled = true;
  }

  setAsDefaultProtocolClient(protocol) {
    this.registeredProtocols.push(protocol);
    return true;
  }
}
