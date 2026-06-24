"use strict";

const { isSafeExternalUrl } = require("./url-safety.cjs");

function normalizeCtoxDevBaseUrl(baseUrl) {
  return String(baseUrl || "https://ctox.dev").replace(/\/+$/, "");
}

function buildCtoxDevLoginUrl(baseUrl) {
  return `${normalizeCtoxDevBaseUrl(baseUrl)}/dashboard?desktop=1&client=ctox-business-os-desktop`;
}

function buildCtoxDevManageUrl(baseUrl) {
  return `${normalizeCtoxDevBaseUrl(baseUrl)}/dashboard`;
}

function buildCtoxDevManagedInstanceUrl(baseUrl, instance) {
  const tenantId = String(instance?.tenantId || instance?.id || "")
    .replace(/^managed:/, "")
    .trim();
  if (!tenantId) return buildCtoxDevManageUrl(baseUrl);
  const url = new URL(buildCtoxDevManageUrl(baseUrl));
  url.searchParams.set("tenant", tenantId);
  return url.toString();
}

function isCtoxDevLoginCompleteUrl(rawUrl, baseUrl) {
  try {
    const url = new URL(String(rawUrl || ""));
    if (
      url.protocol === "ctox-business-os-desktop:"
      && url.hostname === "auth"
      && url.pathname === "/callback"
    ) {
      return true;
    }
    const base = new URL(normalizeCtoxDevBaseUrl(baseUrl));
    return url.origin === base.origin && (
      url.pathname === "/desktop/auth/complete"
      || (
        url.pathname === "/dashboard"
        && url.searchParams.get("desktop") === "1"
        && url.searchParams.get("client") === "ctox-business-os-desktop"
        && url.searchParams.get("auth_completed") === "1"
      )
    );
  } catch (_error) {
    return false;
  }
}

const activeLoginProtocolCompletions = new Set();

function completeCtoxDevLoginFromProtocol(rawUrl) {
  if (!isCtoxDevLoginCompleteUrl(rawUrl, "https://ctox.dev")) {
    return {
      ok: false,
      completed: false,
      error: "unsupported ctox.dev auth callback",
    };
  }
  if (activeLoginProtocolCompletions.size === 0) {
    return {
      ok: true,
      completed: false,
      reason: "no-active-login",
    };
  }
  for (const complete of Array.from(activeLoginProtocolCompletions)) {
    complete(rawUrl);
  }
  return {
    ok: true,
    completed: true,
  };
}

async function clearCtoxDevSession(desktopSession, baseUrl) {
  if (!desktopSession?.clearStorageData) {
    throw new Error("Electron session.clearStorageData is required");
  }
  const base = new URL(normalizeCtoxDevBaseUrl(baseUrl));
  const origin = base.origin;
  await desktopSession.clearStorageData({
    origin,
    storages: ["cookies", "localstorage", "indexdb", "cachestorage", "serviceworkers"],
  });
  const removedCookies = await clearCtoxDevCookies(desktopSession, base);
  return { ok: true, origin, removedCookies };
}

async function clearCtoxDevCookies(desktopSession, base) {
  if (!desktopSession.cookies?.get || !desktopSession.cookies?.remove) return 0;
  const host = base.hostname.toLowerCase();
  const cookies = await desktopSession.cookies.get({});
  let removed = 0;
  for (const cookie of cookies) {
    if (!cookie.name) continue;
    const domain = normalizeCookieDomain(cookie.domain || host);
    if (!cookieMatchesCtoxDevBase(domain, host)) continue;
    const url = `${cookie.secure === false ? base.protocol : "https:"}//${domain}${cookie.path || "/"}`;
    await desktopSession.cookies.remove(url, cookie.name);
    removed += 1;
  }
  return removed;
}

function normalizeCookieDomain(domain) {
  return String(domain || "").trim().replace(/^\.+/, "").toLowerCase();
}

function cookieMatchesCtoxDevBase(domain, host) {
  if (!domain || !host || domain.split(".").length < 2) return false;
  return domain === host
    || domain.endsWith(`.${host}`)
    || host.endsWith(`.${domain}`);
}

function lockDownLoginWindowNavigation(loginWindow, shell) {
  const webContents = loginWindow?.webContents;
  if (!webContents) return;
  const openExternalSafely = (url) => {
    if (shell?.openExternal && isSafeExternalUrl(url)) {
      Promise.resolve(shell.openExternal(url)).catch(() => undefined);
    }
  };
  if (typeof webContents.setWindowOpenHandler === "function") {
    webContents.setWindowOpenHandler(({ url }) => {
      openExternalSafely(url);
      return { action: "deny" };
    });
  }
  if (typeof webContents.on === "function") {
    webContents.on("will-navigate", (event, rawUrl) => {
      // http(s) navigations stay inside the window so cross-origin auth/SSO
      // redirects keep working; non-web schemes (file:, data:, custom) are blocked
      // and only safe ones are handed to the OS browser.
      let protocol = "";
      try {
        protocol = new URL(String(rawUrl || "")).protocol;
      } catch (_error) {
        protocol = "";
      }
      if (protocol === "http:" || protocol === "https:") return;
      if (event && typeof event.preventDefault === "function") event.preventDefault();
      openExternalSafely(rawUrl);
    });
  }
}

async function openCtoxDevLoginWindow({
  BrowserWindow,
  baseUrl,
  authCheckIntervalMs = 1000,
  isAuthenticated,
  parentWindow,
  onWindowCreated,
  shell,
  show = true,
  timeoutMs = 0,
  width = 520,
  height = 720,
} = {}) {
  if (!BrowserWindow) throw new Error("Electron BrowserWindow is required");
  const loginUrl = buildCtoxDevLoginUrl(baseUrl);
  const loginWindow = new BrowserWindow({
    show,
    width,
    height,
    title: "CTOX Login",
    parent: parentWindow || undefined,
    modal: Boolean(parentWindow),
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  lockDownLoginWindowNavigation(loginWindow, shell);
  return new Promise((resolve) => {
    let settled = false;
    let authCheckInFlight = false;
    const timeout = timeoutMs > 0
      ? setTimeout(() => {
        settle({
          ok: false,
          completed: false,
          error: "ctox.dev login window timed out",
        });
      }, timeoutMs)
      : null;
    function settle(result) {
      if (settled) return;
      settled = true;
      if (timeout) clearTimeout(timeout);
      activeLoginProtocolCompletions.delete(handleProtocolCompletion);
      loginWindow.webContents.removeListener("did-navigate", handleNavigation);
      loginWindow.webContents.removeListener("did-redirect-navigation", handleNavigation);
      loginWindow.webContents.removeListener("will-navigate", handleNavigation);
      loginWindow.webContents.removeListener("did-finish-load", handleAuthCheckReady);
      loginWindow.removeListener("closed", handleClosed);
      if (authCheckInterval) clearInterval(authCheckInterval);
      if (!loginWindow.isDestroyed()) loginWindow.close();
      resolve(result);
    }
    function handleNavigation(_event, rawUrl) {
      if (isCtoxDevLoginCompleteUrl(rawUrl, baseUrl)) {
        settle({ ok: true, completed: true, via: "navigation" });
      }
    }
    function handleProtocolCompletion(rawUrl) {
      if (isCtoxDevLoginCompleteUrl(rawUrl, baseUrl)) {
        settle({ ok: true, completed: true, via: "protocol" });
      }
    }
    function handleAuthCheckReady() {
      checkAuthenticatedSession();
    }
    async function checkAuthenticatedSession() {
      if (settled || authCheckInFlight || typeof isAuthenticated !== "function") return;
      authCheckInFlight = true;
      try {
        if (await isAuthenticated()) {
          settle({ ok: true, completed: true, via: "session-check" });
        }
      } catch (_error) {
        // The login page may still be loading or offline; keep waiting for an
        // explicit navigation/callback or a later successful session check.
      } finally {
        authCheckInFlight = false;
      }
    }
    function handleClosed() {
      settle({ ok: true, completed: false });
    }
    const authCheckInterval = typeof isAuthenticated === "function" && authCheckIntervalMs > 0
      ? setInterval(checkAuthenticatedSession, authCheckIntervalMs)
      : null;
    activeLoginProtocolCompletions.add(handleProtocolCompletion);
    loginWindow.webContents.on("did-navigate", handleNavigation);
    loginWindow.webContents.on("did-redirect-navigation", handleNavigation);
    loginWindow.webContents.on("will-navigate", handleNavigation);
    loginWindow.webContents.on("did-finish-load", handleAuthCheckReady);
    loginWindow.on("closed", handleClosed);
    if (typeof onWindowCreated === "function") {
      try {
        Promise.resolve(onWindowCreated(loginWindow, { loginUrl })).catch((error) => {
          settle({
            ok: false,
            completed: false,
            error: error instanceof Error ? error.message : String(error),
          });
        });
      } catch (error) {
        settle({
          ok: false,
          completed: false,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }
    if (settled) return;
    loginWindow.loadURL(loginUrl).catch((error) => {
      settle({
        ok: false,
        completed: false,
        error: error instanceof Error ? error.message : String(error),
      });
    });
  });
}

module.exports = {
  buildCtoxDevManageUrl,
  buildCtoxDevManagedInstanceUrl,
  buildCtoxDevLoginUrl,
  clearCtoxDevSession,
  completeCtoxDevLoginFromProtocol,
  isCtoxDevLoginCompleteUrl,
  openCtoxDevLoginWindow,
};
