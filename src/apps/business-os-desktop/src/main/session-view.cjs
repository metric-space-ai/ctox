"use strict";

const path = require("node:path");

function createInstanceBrowserView({
  BrowserView,
  instance,
  launch,
  shell,
  scrubCtoxConfigFromWebContents,
  isAllowedBusinessOsNavigation,
  isForbiddenBusinessOsHttpDataRequest,
  instancePreloadPath = path.join(__dirname, "../instance-preload.cjs"),
}) {
  if (!BrowserView) throw new Error("Electron BrowserView is required");
  const view = new BrowserView({
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      partition: instance.sessionPartition,
      preload: instancePreloadPath,
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
    installDesktopInstanceSwitcher(view, instance).catch(() => undefined);
  });
  installBusinessOsHttpDataGuard(view, isForbiddenBusinessOsHttpDataRequest);
  return view;
}

async function installDesktopInstanceSwitcher(view, instance) {
  const payload = {
    id: String(instance?.id || ""),
    name: String(instance?.displayName || instance?.domain || "Instanz"),
    source: sourceLabel(instance?.source),
  };
  await view.webContents.executeJavaScript(`(() => {
    const payload = ${JSON.stringify(payload)};
    const bridge = window.ctoxBusinessOsDesktop;
    if (!bridge || typeof bridge.openSwitcher !== "function") return false;
    const host = document.querySelector(".topbar-actions")
      || document.querySelector(".topbar-status-bar")
      || document.querySelector(".topbar");
    if (!host) return false;

    let style = document.getElementById("ctox-desktop-instance-switcher-style");
    if (!style) {
      style = document.createElement("style");
      style.id = "ctox-desktop-instance-switcher-style";
      style.textContent = [
        ".ctox-desktop-instance-switcher{align-items:center;display:inline-grid;grid-template-columns:minmax(0,1fr) max-content;gap:8px;max-width:min(340px,32vw);min-width:150px;}",
        ".ctox-desktop-instance-switcher .desktop-instance-name{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;}",
        ".ctox-desktop-instance-switcher .desktop-instance-source{border:1px solid color-mix(in srgb,var(--border, #46505a) 70%, transparent);border-radius:6px;color:var(--muted,#9aa4b2);font-size:11px;line-height:1;padding:3px 5px;}",
        "@media (max-width: 820px){.ctox-desktop-instance-switcher{max-width:190px;min-width:112px}.ctox-desktop-instance-switcher .desktop-instance-source{display:none;}}"
      ].join("");
      document.head.appendChild(style);
    }

    let button = document.getElementById("ctox-desktop-instance-switcher");
    if (!button) {
      button = document.createElement("button");
      button.id = "ctox-desktop-instance-switcher";
      button.type = "button";
      button.className = "account-button ctox-desktop-instance-switcher";
      button.innerHTML = '<span class="desktop-instance-name"></span><span class="desktop-instance-source"></span>';
      host.insertBefore(button, host.firstChild);
    }
    button.querySelector(".desktop-instance-name").textContent = payload.name;
    button.querySelector(".desktop-instance-source").textContent = payload.source;
    button.title = "Instanz wechseln";
    button.setAttribute("aria-label", "Instanz wechseln: " + payload.name);
    button.onclick = () => bridge.openSwitcher();

    if (!window.__ctoxDesktopSwitcherShortcutInstalled) {
      window.__ctoxDesktopSwitcherShortcutInstalled = true;
      document.addEventListener("keydown", (event) => {
        if (!(event.metaKey || event.ctrlKey) || String(event.key || "").toLowerCase() !== "k") return;
        event.preventDefault();
        bridge.openSwitcher();
      }, true);
    }
    return true;
  })()`, true);
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
    x: 0,
    y: 0,
    width: Math.max(640, contentBounds.width),
    height: Math.max(480, contentBounds.height),
  });
  view.setAutoResize({ width: true, height: true });
}

function sourceLabel(source) {
  return {
    ctox_dev: "ctox.dev",
    local_daemon: "lokal",
    ssh_managed: "ssh",
    pairing_invite: "pairing",
  }[source] || String(source || "");
}

module.exports = {
  createInstanceBrowserView,
  installDesktopInstanceSwitcher,
  installBusinessOsHttpDataGuard,
  layoutInstanceBrowserView,
};
