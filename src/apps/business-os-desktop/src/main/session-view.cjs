"use strict";

function createInstanceBrowserView({
  BrowserView,
  instance,
  launch,
  shell,
  scrubCtoxConfigFromWebContents,
  isAllowedBusinessOsNavigation,
  isForbiddenBusinessOsHttpDataRequest,
}) {
  if (!BrowserView) throw new Error("Electron BrowserView is required");
  const view = new BrowserView({
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      partition: instance.sessionPartition,
    },
  });
  view.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url);
    return { action: "deny" };
  });
  view.webContents.on("will-navigate", (event, url) => {
    const allowedOrigins = new Set([new URL(launch.launchUrl).origin]);
    if (!isAllowedBusinessOsNavigation(url, allowedOrigins)) {
      event.preventDefault();
      shell.openExternal(url);
    }
  });
  view.webContents.on("did-finish-load", () => {
    scrubCtoxConfigFromWebContents(view.webContents).catch(() => undefined);
  });
  installBusinessOsHttpDataGuard(view, isForbiddenBusinessOsHttpDataRequest);
  return view;
}

function installBusinessOsHttpDataGuard(view, isForbiddenBusinessOsHttpDataRequest) {
  const webRequest = view.webContents.session?.webRequest;
  if (!webRequest?.onBeforeRequest || typeof isForbiddenBusinessOsHttpDataRequest !== "function") {
    return false;
  }
  webRequest.onBeforeRequest({ urls: ["http://*/*", "https://*/*"] }, (details, callback) => {
    callback({ cancel: isForbiddenBusinessOsHttpDataRequest(details.url) });
  });
  return true;
}

function layoutInstanceBrowserView(view, contentBounds) {
  view.setBounds({
    x: 300,
    y: 52,
    width: Math.max(640, contentBounds.width - 300),
    height: Math.max(480, contentBounds.height - 52),
  });
  view.setAutoResize({ width: true, height: true });
}

module.exports = {
  createInstanceBrowserView,
  installBusinessOsHttpDataGuard,
  layoutInstanceBrowserView,
};
