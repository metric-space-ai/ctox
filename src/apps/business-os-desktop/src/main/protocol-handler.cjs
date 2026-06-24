"use strict";

const DESKTOP_PROTOCOL = "ctox-business-os-desktop";

function isDesktopProtocolUrl(value, protocol = DESKTOP_PROTOCOL) {
  const candidate = String(value || "").trim();
  return candidate.startsWith(`${protocol}:`);
}

function extractDesktopProtocolUrls(argv, protocol = DESKTOP_PROTOCOL) {
  if (!Array.isArray(argv)) return [];
  return argv
    .map((entry) => String(entry || "").trim())
    .filter((entry) => isDesktopProtocolUrl(entry, protocol));
}

function parseDesktopProtocolUrl(rawUrl) {
  const url = new URL(String(rawUrl || ""));
  if (url.protocol !== `${DESKTOP_PROTOCOL}:`) {
    throw new Error("unsupported desktop protocol");
  }
  const action = protocolAction(url);
  if (action === "pair") {
    const payload = url.searchParams.get("payload") || url.searchParams.get("invite");
    if (!payload) throw new Error("pair URL is missing payload");
    return {
      action: "pair",
      rawInvite: rawUrl,
      payload,
    };
  }
  if (action === "instance") {
    const tenantId = decodeURIComponent(url.pathname.replace(/^\/+/, "") || url.host || "").trim();
    if (!tenantId) throw new Error("instance URL is missing tenant id");
    return {
      action: "instance",
      instanceId: `managed:${tenantId}`,
      tenantId,
    };
  }
  if (action === "auth") {
    if (url.pathname !== "/callback") throw new Error("auth URL is missing callback path");
    return {
      action: "auth",
      callbackUrl: rawUrl,
    };
  }
  throw new Error(`unsupported desktop protocol action: ${action || "missing"}`);
}

function protocolAction(url) {
  if (url.host === "pair") return "pair";
  if (url.host === "instance") return "instance";
  if (url.host === "auth") return "auth";
  const firstPath = url.pathname.split("/").filter(Boolean)[0];
  return firstPath || url.host;
}

async function handleDesktopProtocolUrl(rawUrl, handlers, options = {}) {
  const parsed = parseDesktopProtocolUrl(rawUrl);
  const confirmAction = typeof options.confirmAction === "function" ? options.confirmAction : null;
  if (parsed.action === "pair") {
    // A pair link from any web page would otherwise silently import an
    // attacker-controlled invite (writing a sync room + room password) and open
    // it. Require explicit user confirmation before touching the registry/keychain.
    if (confirmAction && !(await confirmAction({ action: "pair", payload: parsed.payload, rawUrl }))) {
      return { ok: false, declined: true, action: "pair" };
    }
    return handlers.importInvite(parsed.rawInvite);
  }
  if (parsed.action === "instance") {
    // Switching the foreground instance from an untrusted deep-link also needs
    // explicit consent, even though it can only select an already-registered tenant.
    if (confirmAction && !(await confirmAction({
      action: "instance",
      instanceId: parsed.instanceId,
      tenantId: parsed.tenantId,
      rawUrl,
    }))) {
      return { ok: false, declined: true, action: "instance" };
    }
    return handlers.activateManagedInstance(parsed.instanceId);
  }
  if (parsed.action === "auth") {
    if (typeof handlers.handleCtoxDevAuthCallback !== "function") {
      return { ok: true, ignored: true, action: "auth" };
    }
    return handlers.handleCtoxDevAuthCallback(parsed.callbackUrl);
  }
  return null;
}

function installDesktopProtocolHandling({
  app,
  argv = process.argv,
  protocol = DESKTOP_PROTOCOL,
  handlersProvider,
  isReady,
  onError,
  confirmAction,
  onActivate,
  registerDefaultProtocolClient = true,
  singleInstanceLock = true,
} = {}) {
  if (!app) throw new Error("Electron app is required");
  if (typeof handlersProvider !== "function") throw new Error("handlersProvider is required");
  if (typeof isReady !== "function") throw new Error("isReady is required");
  const pendingUrls = extractDesktopProtocolUrls(argv, protocol);
  const inFlight = new Set();
  let hasSingleInstanceLock = true;

  if (singleInstanceLock && typeof app.requestSingleInstanceLock === "function") {
    hasSingleInstanceLock = app.requestSingleInstanceLock();
    if (!hasSingleInstanceLock) {
      app.quit();
    }
  }

  function track(promise) {
    inFlight.add(promise);
    promise.finally(() => inFlight.delete(promise));
    return promise;
  }

  async function dispatchOrQueue(rawUrl) {
    const url = String(rawUrl || "").trim();
    if (!isDesktopProtocolUrl(url, protocol)) return { ignored: true };
    if (!isReady()) {
      pendingUrls.push(url);
      return { queued: true };
    }
    try {
      return await handleDesktopProtocolUrl(url, handlersProvider(), { confirmAction });
    } catch (error) {
      if (typeof onError === "function") onError(error, url);
      return {
        ok: false,
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }

  app.on("open-url", (event, rawUrl) => {
    if (event && typeof event.preventDefault === "function") event.preventDefault();
    if (typeof onActivate === "function") onActivate();
    track(dispatchOrQueue(rawUrl));
  });

  app.on("second-instance", (_event, commandLine) => {
    if (typeof onActivate === "function") onActivate();
    for (const rawUrl of extractDesktopProtocolUrls(commandLine, protocol)) {
      track(dispatchOrQueue(rawUrl));
    }
  });

  function registerProtocolClient() {
    if (!registerDefaultProtocolClient || typeof app.setAsDefaultProtocolClient !== "function") {
      return false;
    }
    return app.setAsDefaultProtocolClient(protocol);
  }

  async function flushPending() {
    if (!isReady()) return [];
    const urls = pendingUrls.splice(0);
    const results = [];
    for (const rawUrl of urls) {
      results.push(await dispatchOrQueue(rawUrl));
    }
    await waitForIdle();
    return results;
  }

  async function waitForIdle() {
    while (inFlight.size > 0) {
      await Promise.all(Array.from(inFlight));
    }
  }

  return {
    dispatchOrQueue: (rawUrl) => track(dispatchOrQueue(rawUrl)),
    flushPending,
    getPendingUrls: () => pendingUrls.slice(),
    gotSingleInstanceLock: hasSingleInstanceLock,
    registerDefaultProtocolClient: registerProtocolClient,
    waitForIdle,
  };
}

module.exports = {
  DESKTOP_PROTOCOL,
  extractDesktopProtocolUrls,
  installDesktopProtocolHandling,
  isDesktopProtocolUrl,
  parseDesktopProtocolUrl,
  handleDesktopProtocolUrl,
};
