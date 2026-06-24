"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const {
  createInstanceBrowserView,
  installBusinessOsHttpDataGuard,
  layoutInstanceBrowserView,
} = require("../src/main/session-view.cjs");

test("instance BrowserView uses the instance session partition", () => {
  let options;
  class FakeBrowserView {
    constructor(input) {
      options = input;
      this.webContents = {
        session: {
          webRequest: {
            onBeforeRequest: () => undefined,
          },
        },
        setWindowOpenHandler: () => undefined,
        on: () => undefined,
      };
    }
  }
  createInstanceBrowserView({
    BrowserView: FakeBrowserView,
    instance: { sessionPartition: "persist:ctox-local-a" },
    launch: { launchUrl: "https://ctox.dev/business-os/" },
    shell: { openExternal: () => undefined },
    scrubCtoxConfigFromWebContents: async () => undefined,
    isAllowedBusinessOsNavigation: () => true,
    isForbiddenBusinessOsHttpDataRequest: () => false,
    instancePreloadPath: "/tmp/ctox-instance-preload.cjs",
  });
  assert.equal(options.webPreferences.partition, "persist:ctox-local-a");
  assert.equal(options.webPreferences.contextIsolation, true);
  assert.equal(options.webPreferences.nodeIntegration, false);
  assert.equal(options.webPreferences.sandbox, true);
  assert.equal(options.webPreferences.preload, "/tmp/ctox-instance-preload.cjs");
});

test("BrowserView installs a fail-closed HTTP data-plane request guard", () => {
  let observedFilter;
  let observedHandler;
  const view = {
    webContents: {
      session: {
        webRequest: {
          onBeforeRequest: (filter, handler) => {
            observedFilter = filter;
            observedHandler = handler;
          },
        },
      },
    },
  };
  assert.equal(installBusinessOsHttpDataGuard(
    view,
    (url) => url.includes("/api/business-os/records"),
  ), true);
  assert.deepEqual(observedFilter, { urls: ["http://*/*", "https://*/*", "ws://*/*", "wss://*/*"] });
  const decisions = [];
  observedHandler({ url: "https://tenant.example.com/api/business-os/status" }, (decision) => decisions.push(decision));
  observedHandler({ url: "https://tenant.example.com/api/business-os/records" }, (decision) => decisions.push(decision));
  assert.deepEqual(decisions, [{ cancel: false }, { cancel: true }]);
});

test("data guard also default-denies unknown same-host data resources", () => {
  let handler;
  const view = {
    webContents: {
      session: {
        webRequest: {
          onBeforeRequest: (_filter, fn) => { handler = fn; },
        },
      },
    },
  };
  const { isForbiddenBusinessOsHttpDataRequest, isForbiddenBusinessOsDataResourceRequest } = require("../src/main/url-safety.cjs");
  installBusinessOsHttpDataGuard(view, isForbiddenBusinessOsHttpDataRequest, {
    launchOrigin: "https://tenant.example.com",
    isForbiddenBusinessOsDataResourceRequest,
  });
  const decide = (details) => {
    let result;
    handler(details, (decision) => { result = decision; });
    return result;
  };
  // Unknown data route, data-shaped request, same host -> cancelled by default-deny.
  assert.deepEqual(decide({ url: "https://tenant.example.com/files", resourceType: "xhr" }), { cancel: true });
  // Same route as a plain asset load (script) -> allowed.
  assert.deepEqual(decide({ url: "https://tenant.example.com/files", resourceType: "script" }), { cancel: false });
  // Control plane stays reachable.
  assert.deepEqual(decide({ url: "https://tenant.example.com/api/business-os/status", resourceType: "xhr" }), { cancel: false });
});

test("layout lets BrowserView own the full app viewport", () => {
  const calls = [];
  const view = {
    setBounds: (bounds) => calls.push(["bounds", bounds]),
    setAutoResize: (resize) => calls.push(["resize", resize]),
  };
  layoutInstanceBrowserView(view, { width: 1440, height: 920 });
  assert.deepEqual(calls[0], ["bounds", { x: 0, y: 0, width: 1440, height: 920 }]);
  assert.deepEqual(calls[1], ["resize", { width: true, height: true }]);
});
