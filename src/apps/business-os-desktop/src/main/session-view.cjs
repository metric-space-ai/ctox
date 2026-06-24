"use strict";

const path = require("node:path");

// A remotely-loaded instance shell is denied every device/capability permission
// by default; only the minimal, non-sensitive set the Business-OS UI needs is
// allowed. Camera, microphone, geolocation, MIDI, clipboard-read, etc. are denied.
const ALLOWED_INSTANCE_PERMISSIONS = new Set(["notifications", "clipboard-sanitized-write"]);

function createInstanceBrowserView({
  BrowserView,
  instance,
  launch,
  shell,
  scrubCtoxConfigFromWebContents,
  isAllowedBusinessOsNavigation,
  isForbiddenBusinessOsHttpDataRequest,
  isForbiddenBusinessOsDataResourceRequest,
  isSafeExternalUrl,
  instancePreloadPath = path.join(__dirname, "../instance-preload.cjs"),
}) {
  if (!BrowserView) throw new Error("Electron BrowserView is required");
  const launchOrigin = originOf(launch?.launchUrl);
  const view = new BrowserView({
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      partition: instance.sessionPartition,
      preload: instancePreloadPath,
    },
  });
  installInstancePermissionPolicy(view);
  const openExternalSafely = (url) => {
    if (typeof isSafeExternalUrl === "function" && !isSafeExternalUrl(url)) return;
    shell.openExternal(url);
  };
  view.webContents.setWindowOpenHandler(({ url }) => {
    openExternalSafely(url);
    return { action: "deny" };
  });
  view.webContents.on("will-navigate", (event, url) => {
    const allowedOrigins = new Set([launchOrigin].filter(Boolean));
    if (!isAllowedBusinessOsNavigation(url, allowedOrigins)) {
      event.preventDefault();
      openExternalSafely(url);
    }
  });
  view.webContents.on("did-finish-load", () => {
    scrubCtoxConfigFromWebContents(view.webContents).catch(() => undefined);
    installDesktopInstanceSwitcher(view, instance).catch(() => undefined);
  });
  installBusinessOsHttpDataGuard(view, isForbiddenBusinessOsHttpDataRequest, {
    launchOrigin,
    isForbiddenBusinessOsDataResourceRequest,
  });
  return view;
}

function originOf(rawUrl) {
  try {
    return new URL(String(rawUrl || "")).origin;
  } catch (_error) {
    return "";
  }
}

function installInstancePermissionPolicy(view) {
  const ses = view?.webContents?.session;
  if (!ses) return false;
  if (typeof ses.setPermissionRequestHandler === "function") {
    ses.setPermissionRequestHandler((_webContents, permission, callback) => {
      callback(ALLOWED_INSTANCE_PERMISSIONS.has(permission));
    });
  }
  if (typeof ses.setPermissionCheckHandler === "function") {
    ses.setPermissionCheckHandler((_webContents, permission) => ALLOWED_INSTANCE_PERMISSIONS.has(permission));
  }
  return true;
}

async function installDesktopInstanceSwitcher(view, instance) {
  const payload = {
    id: sanitizeForScriptLiteral(instance?.id || ""),
    name: sanitizeForScriptLiteral(instance?.displayName || instance?.domain || "Instanz"),
    source: sanitizeForScriptLiteral(sourceLabel(instance?.source)),
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

function installBusinessOsHttpDataGuard(view, isForbiddenBusinessOsHttpDataRequest, options = {}) {
  const { launchOrigin = "", isForbiddenBusinessOsDataResourceRequest } = options;
  const webRequest = view.webContents.session?.webRequest;
  if (!webRequest?.onBeforeRequest || typeof isForbiddenBusinessOsHttpDataRequest !== "function") {
    // The instance shell would be able to reach HTTP data routes unguarded; make
    // the failure loud rather than silently shipping an open data path.
    console.error("Business OS HTTP data guard could not be installed for an instance view");
    return false;
  }
  webRequest.onBeforeRequest(
    { urls: ["http://*/*", "https://*/*", "ws://*/*", "wss://*/*"] },
    (details, callback) => {
      const cancel = isForbiddenBusinessOsHttpDataRequest(details.url)
        || (typeof isForbiddenBusinessOsDataResourceRequest === "function"
          && isForbiddenBusinessOsDataResourceRequest(details.url, details.resourceType, launchOrigin));
      callback({ cancel: Boolean(cancel) });
    },
  );
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

function sanitizeForScriptLiteral(value) {
  // The switcher payload (attacker-influenced instance name/domain) is embedded
  // into an executeJavaScript source string via JSON.stringify, which already
  // escapes quotes/backslashes. Additionally strip the U+2028/U+2029 line
  // separators, which are valid inside a JSON string but were historically treated
  // as line terminators inside a JS source string — defense-in-depth.
  return String(value).replace(/[\u2028\u2029]/g, "");
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
  sanitizeForScriptLiteral,
};
