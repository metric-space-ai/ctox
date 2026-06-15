"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const EventEmitter = require("node:events");
const {
  buildCtoxDevManageUrl,
  buildCtoxDevManagedInstanceUrl,
  buildCtoxDevLoginUrl,
  clearCtoxDevSession,
  completeCtoxDevLoginFromProtocol,
  isCtoxDevLoginCompleteUrl,
  openCtoxDevLoginWindow,
} = require("../src/main/ctox-dev-login.cjs");

test("builds desktop login URL from configured ctox.dev base", () => {
  assert.equal(
    buildCtoxDevLoginUrl("https://ctox.dev/"),
    "https://ctox.dev/dashboard?desktop=1&client=ctox-business-os-desktop",
  );
});

test("recognizes only same-origin ctox.dev desktop auth completion URLs", () => {
  assert.equal(
    isCtoxDevLoginCompleteUrl("ctox-business-os-desktop://auth/callback", "https://ctox.dev"),
    true,
  );
  assert.equal(
    isCtoxDevLoginCompleteUrl(
      "https://ctox.dev/dashboard?desktop=1&client=ctox-business-os-desktop&auth_completed=1",
      "https://ctox.dev",
    ),
    true,
  );
  assert.equal(
    isCtoxDevLoginCompleteUrl("https://ctox.dev/desktop/auth/complete", "https://ctox.dev"),
    true,
  );
  assert.equal(
    isCtoxDevLoginCompleteUrl("https://evil.example/desktop/auth/complete", "https://ctox.dev"),
    false,
  );
  assert.equal(
    isCtoxDevLoginCompleteUrl("https://ctox.dev/desktop/auth", "https://ctox.dev"),
    false,
  );
  assert.equal(
    isCtoxDevLoginCompleteUrl("ctox-business-os-desktop://evil/callback", "https://ctox.dev"),
    false,
  );
});

test("builds ctox.dev management URLs without embedding launch secrets", () => {
  assert.equal(
    buildCtoxDevManageUrl("https://ctox.dev/"),
    "https://ctox.dev/dashboard",
  );
  assert.equal(
    buildCtoxDevManagedInstanceUrl("https://ctox.dev/", {
      id: "managed:tenant_skf",
      tenantId: "tenant_skf",
      launchUrl: "https://skf.ctox.dev/?ctox_config=secret",
    }),
    "https://ctox.dev/dashboard?tenant=tenant_skf",
  );
});

test("clears only the configured ctox.dev session origin", async () => {
  const calls = [];
  const removed = [];
  const result = await clearCtoxDevSession({
    clearStorageData: async (options) => calls.push(options),
    cookies: {
      get: async () => [
        { name: "session", domain: ".ctox.dev", path: "/", secure: true },
        { name: "csrf", domain: "auth.ctox.dev", path: "/", secure: true },
        { name: "too-broad", domain: "dev", path: "/", secure: true },
        { name: "other", domain: "example.com", path: "/", secure: true },
      ],
      remove: async (url, name) => removed.push([url, name]),
    },
  }, "https://ctox.dev/");
  assert.deepEqual(result, { ok: true, origin: "https://ctox.dev", removedCookies: 2 });
  assert.deepEqual(calls, [{
    origin: "https://ctox.dev",
    storages: ["cookies", "localstorage", "indexdb", "cachestorage", "serviceworkers"],
  }]);
  assert.deepEqual(removed, [
    ["https://ctox.dev/", "session"],
    ["https://auth.ctox.dev/", "csrf"],
  ]);
});

test("custom protocol callback completes the active ctox.dev login window", async () => {
  const loginPromise = openCtoxDevLoginWindow({
    BrowserWindow: FakeBrowserWindow,
    baseUrl: "https://ctox.dev",
  });
  assert.equal(FakeBrowserWindow.instances.at(-1).loadedUrl, buildCtoxDevLoginUrl("https://ctox.dev"));
  const completion = completeCtoxDevLoginFromProtocol("ctox-business-os-desktop://auth/callback");
  assert.deepEqual(completion, { ok: true, completed: true });
  const login = await loginPromise;
  assert.deepEqual(login, { ok: true, completed: true, via: "protocol" });
  assert.equal(FakeBrowserWindow.instances.at(-1).closed, true);
});

test("authenticated session check completes the active ctox.dev login window", async () => {
  let authenticated = false;
  const loginPromise = openCtoxDevLoginWindow({
    BrowserWindow: FakeBrowserWindow,
    baseUrl: "https://ctox.dev",
    authCheckIntervalMs: 5,
    isAuthenticated: async () => authenticated,
  });
  authenticated = true;
  const login = await loginPromise;
  assert.deepEqual(login, { ok: true, completed: true, via: "session-check" });
  assert.equal(FakeBrowserWindow.instances.at(-1).closed, true);
});

test("custom protocol callback is a no-op without an active ctox.dev login window", () => {
  assert.deepEqual(
    completeCtoxDevLoginFromProtocol("ctox-business-os-desktop://auth/callback"),
    { ok: true, completed: false, reason: "no-active-login" },
  );
  assert.equal(
    completeCtoxDevLoginFromProtocol("ctox-business-os-desktop://instance/tenant_skf").ok,
    false,
  );
});

class FakeBrowserWindow extends EventEmitter {
  static instances = [];

  constructor(options) {
    super();
    this.options = options;
    this.webContents = new EventEmitter();
    this.loadedUrl = "";
    this.closed = false;
    FakeBrowserWindow.instances.push(this);
  }

  loadURL(url) {
    this.loadedUrl = url;
    return Promise.resolve();
  }

  isDestroyed() {
    return this.closed;
  }

  close() {
    if (this.closed) return;
    this.closed = true;
    this.emit("closed");
  }
}
