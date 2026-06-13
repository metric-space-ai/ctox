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
  });
  assert.equal(options.webPreferences.partition, "persist:ctox-local-a");
  assert.equal(options.webPreferences.contextIsolation, true);
  assert.equal(options.webPreferences.nodeIntegration, false);
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
  assert.deepEqual(observedFilter, { urls: ["http://*/*", "https://*/*"] });
  const decisions = [];
  observedHandler({ url: "https://tenant.example.com/api/business-os/status" }, (decision) => decisions.push(decision));
  observedHandler({ url: "https://tenant.example.com/api/business-os/records" }, (decision) => decisions.push(decision));
  assert.deepEqual(decisions, [{ cancel: false }, { cancel: true }]);
});

test("layout keeps BrowserView in the app content region", () => {
  const calls = [];
  const view = {
    setBounds: (bounds) => calls.push(["bounds", bounds]),
    setAutoResize: (resize) => calls.push(["resize", resize]),
  };
  layoutInstanceBrowserView(view, { width: 1440, height: 920 });
  assert.deepEqual(calls[0], ["bounds", { x: 300, y: 52, width: 1140, height: 868 }]);
  assert.deepEqual(calls[1], ["resize", { width: true, height: true }]);
});
