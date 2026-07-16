"use strict";

const path = require("node:path");
const {
  app,
  BrowserView,
  BrowserWindow,
  crashReporter,
  dialog,
  ipcMain,
  nativeImage,
  session,
  shell,
} = require("electron");
const {
  configureCrashReporter,
  updateCrashReportExtra,
} = require("./crash-reports.cjs");
const { autoUpdater } = require("electron-updater");
const { configureAutoUpdates } = require("./auto-update.cjs");
const { SourceManager } = require("./source-manager.cjs");
const { loadRegistry, saveRegistry } = require("./registry.cjs");
const { createSecretStore } = require("./secret-store.cjs");
const { createSupportBundleSnapshot } = require("./redaction.cjs");
const { businessOsShellRoot, startBundledBusinessOsShell } = require("./bundled-shell.cjs");
const {
  createInstanceBrowserView,
  layoutInstanceBrowserView,
} = require("./session-view.cjs");
const {
  isForbiddenBusinessOsHttpDataRequest,
  isForbiddenBusinessOsDataResourceRequest,
  isAllowedBusinessOsNavigation,
  isSafeExternalUrl,
  scrubCtoxConfigFromWebContents,
} = require("./url-safety.cjs");
const { installDesktopProtocolHandling } = require("./protocol-handler.cjs");
const {
  buildCtoxDevManagedInstanceUrl,
  clearCtoxDevSession,
  completeCtoxDevLoginFromProtocol,
  openCtoxDevLoginWindow,
} = require("./ctox-dev-login.cjs");
const { installNativeFileDragBridge } = require("./file-drag.cjs");

let mainWindow;
let sourceManager;
let registryPath;
let registry;
let secretStore;
let bundledShell;
let activeViewId = null;
let chromeOverlayVisible = false;
const views = new Map();
const pendingViewLoads = new Map();
const protocolHandling = installDesktopProtocolHandling({
  app,
  handlersProvider: protocolHandlers,
  isReady: () => Boolean(sourceManager && mainWindow),
  confirmAction: confirmDeepLinkAction,
  onActivate: focusMainWindow,
  onError: (error, rawUrl) => {
    console.error("Desktop protocol link failed", {
      rawUrl,
      error: error instanceof Error ? error.message : String(error),
    });
  },
});

function focusMainWindow() {
  if (!mainWindow) return;
  if (mainWindow.isMinimized?.()) mainWindow.restore?.();
  mainWindow.show?.();
  mainWindow.focus?.();
}

// Deep-links arrive from outside the app (a web page, email, chat). Acting on a
// pair/instance link without consent would let any page silently add an
// attacker-controlled instance or switch the active one, so require an explicit,
// window-focused confirmation before the action runs.
async function confirmDeepLinkAction(descriptor) {
  focusMainWindow();
  if (!dialog?.showMessageBox) return true; // no UI (headless/test) -> do not hard-block
  const copy = describeDeepLinkAction(descriptor);
  const { response } = await dialog.showMessageBox(mainWindow ?? undefined, {
    type: "warning",
    buttons: ["Abbrechen", copy.confirmLabel],
    defaultId: 0,
    cancelId: 0,
    noLink: true,
    title: copy.title,
    message: copy.message,
    detail: copy.detail,
  });
  return response === 1;
}

function describeDeepLinkAction(descriptor) {
  if (descriptor?.action === "instance") {
    return {
      title: "Instanz öffnen?",
      message: "Ein externer Link möchte zu einer verwalteten CTOX-Instanz wechseln.",
      detail: `Instanz: ${descriptor.tenantId || descriptor.instanceId || "unbekannt"}\n`
        + "Nur fortfahren, wenn du diesen Link erwartet hast.",
      confirmLabel: "Wechseln",
    };
  }
  return {
    title: "Instanz koppeln?",
    message: "Ein externer Link möchte eine neue CTOX-Instanz hinzufügen und öffnen.",
    detail: "Pairing-Links enthalten Zugangsdaten zu einem fremden Sync-Raum. "
      + "Nur fortfahren, wenn du diesen Link von einer vertrauenswürdigen Quelle erhalten hast.",
    confirmLabel: "Koppeln & öffnen",
  };
}

function registryProvider() {
  return registry;
}

function registrySaver(nextRegistry) {
  registry = nextRegistry;
  saveRegistry(registryPath, registry);
}

async function createWindow() {
  registryPath = path.join(app.getPath("userData"), "instances.json");
  secretStore = createSecretStore();
  registry = loadRegistry(registryPath);
  configureCrashReporter(crashReporter, {
    registry,
    appInfo: {
      name: app.getName(),
      version: app.getVersion(),
      platform: process.platform,
    },
  });
  configureAutoUpdates({ app, autoUpdater, logger: console });
  bundledShell = bundledShell || await startBundledBusinessOsShell({
    root: businessOsShellRoot({
      isPackaged: app.isPackaged,
      resourcesPath: process.resourcesPath,
      appDir: __dirname,
    }),
  });
  sourceManager = new SourceManager({
    registryProvider,
    registrySaver,
    secretStore,
    ctoxDevBaseUrl: registry.settings.ctoxDevBaseUrl,
    shellUrl: registry.settings.shellUrl,
    managedShellUrl: bundledShell.url,
    fetchImpl: session.defaultSession.fetch.bind(session.defaultSession),
    knownHostsPath: path.join(app.getPath("userData"), "ssh", "known_hosts"),
  });
  mainWindow = new BrowserWindow({
    width: 1440,
    height: 920,
    minWidth: 1180,
    minHeight: 720,
    title: "CTOX Business OS Desktop Beta",
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      preload: path.join(__dirname, "../preload.cjs"),
    },
  });
  lockDownPrivilegedWindowNavigation(mainWindow);
  mainWindow.loadFile(path.join(__dirname, "../renderer/index.html"));
  mainWindow.on("resize", layoutActiveView);
  mainWindow.on("closed", () => {
    mainWindow = null;
  });
}

function lockDownPrivilegedWindowNavigation(window) {
  // The shell window holds the full, SSH/secret-capable `ctoxDesktop` preload
  // bridge and only ever renders the bundled local index.html. Forbid it from
  // navigating or opening windows to remote content, which would otherwise hand
  // that bridge to an arbitrary origin. Safe links are deflected to the OS browser.
  window.webContents.setWindowOpenHandler(({ url }) => {
    if (isSafeExternalUrl(url)) shell.openExternal(url);
    return { action: "deny" };
  });
  window.webContents.on("will-navigate", (event, url) => {
    if (url.startsWith("file:")) return;
    event.preventDefault();
    if (isSafeExternalUrl(url)) shell.openExternal(url);
  });
}

function layoutActiveView() {
  if (!mainWindow || !activeViewId || chromeOverlayVisible) return;
  const view = views.get(activeViewId);
  if (!view) return;
  layoutInstanceBrowserView(view, mainWindow.getContentBounds());
}

function isViewAttached(view) {
  return Boolean(mainWindow?.getBrowserViews?.().includes(view));
}

function attachActiveView() {
  if (!mainWindow || !activeViewId || chromeOverlayVisible) return;
  const view = views.get(activeViewId);
  if (!view) return;
  if (!isViewAttached(view)) mainWindow.addBrowserView(view);
  layoutActiveView();
}

function detachActiveView() {
  if (!mainWindow || !activeViewId) return;
  const view = views.get(activeViewId);
  if (view && isViewAttached(view)) mainWindow.removeBrowserView(view);
}

function setChromeOverlayVisible(visible) {
  chromeOverlayVisible = Boolean(visible);
  if (chromeOverlayVisible) detachActiveView();
  else attachActiveView();
  return { ok: true, visible: chromeOverlayVisible };
}

function openInstanceSwitcherOverlay() {
  setChromeOverlayVisible(true);
  mainWindow?.webContents?.send?.("desktop:switcher-open");
  return { ok: true };
}

function showAppShell() {
  chromeOverlayVisible = false;
  detachActiveView();
  activeViewId = null;
  updateCrashReportExtra(crashReporter, {
    activeInstanceId: "",
    activeInstanceSource: "",
    activeInstanceStatus: "",
  });
  return { ok: true };
}

async function activateInstance(instance) {
  if (!mainWindow) throw new Error("window is not ready");
  const view = await loadInstanceViewOnce(instance);
  detachActiveView();
  activeViewId = instance.id;
  updateCrashReportExtra(crashReporter, {
    activeInstanceId: activeViewId,
    activeInstanceSource: instance.source,
    activeInstanceStatus: instance.status,
  });
  attachActiveView();
  sourceManager.markInstanceUsed(instance.id);
  return { ok: true };
}

async function loadInstanceViewOnce(instance) {
  const existing = views.get(instance.id);
  if (existing) return existing;
  const pending = pendingViewLoads.get(instance.id);
  if (pending) return pending;
  const load = (async () => {
    const launch = await sourceManager.getLaunchConfig(instance);
    const view = createInstanceBrowserView({
      BrowserView,
      instance,
      launch,
      shell,
      scrubCtoxConfigFromWebContents,
      isAllowedBusinessOsNavigation,
      isForbiddenBusinessOsHttpDataRequest,
      isForbiddenBusinessOsDataResourceRequest,
      isSafeExternalUrl,
    });
    try {
      await view.webContents.loadURL(launch.launchUrl);
      await scrubCtoxConfigFromWebContents(view.webContents).catch(() => undefined);
      views.set(instance.id, view);
      return view;
    } catch (error) {
      view.webContents?.close?.();
      throw error;
    }
  })();
  pendingViewLoads.set(instance.id, load);
  try {
    return await load;
  } finally {
    if (pendingViewLoads.get(instance.id) === load) pendingViewLoads.delete(instance.id);
  }
}

async function removeInstance(instance) {
  const result = await sourceManager.removeInstance(instance);
  destroyInstanceView(instance.id);
  return result;
}

async function revokePairing(instance) {
  const result = await sourceManager.revokePairing(instance);
  destroyInstanceView(instance.id);
  return result;
}

async function rotatePairing(instance, rawInvite) {
  const result = await sourceManager.rotatePairing(instance, rawInvite);
  destroyInstanceView(instance.id);
  return result;
}

function destroyInstanceView(instanceId) {
  const view = views.get(instanceId);
  if (!view) return;
  if (activeViewId === instanceId && mainWindow) {
    mainWindow.removeBrowserView(view);
    activeViewId = null;
  }
  view.webContents.destroy();
  views.delete(instanceId);
}

async function activateManagedInstance(instanceId) {
  if (!sourceManager) throw new Error("source manager is not ready");
  const instances = await sourceManager.listInstances();
  const instance = instances.find((entry) => entry.id === instanceId);
  if (!instance) throw new Error(`managed instance not available: ${instanceId}`);
  if (instance.source !== "ctox_dev") throw new Error(`protocol instance is not managed: ${instanceId}`);
  return activateInstance(instance);
}


function protocolHandlers() {
  return {
    importInvite: async (rawInvite) => {
      const instance = await sourceManager.importInvite(rawInvite);
      return activateInstance(instance);
    },
    activateManagedInstance,
    handleCtoxDevAuthCallback: (rawUrl) => completeCtoxDevLoginFromProtocol(rawUrl, registry?.settings?.ctoxDevBaseUrl),
  };
}

async function openCtoxDevLogin() {
  const result = await openCtoxDevLoginWindow({
    BrowserWindow,
    baseUrl: registry.settings.ctoxDevBaseUrl,
    isAuthenticated: isCtoxDevSessionAuthenticated,
    parentWindow: mainWindow,
    shell,
  });
  const instances = await sourceManager.listInstances();
  return {
    ...result,
    instances,
  };
}

async function isCtoxDevSessionAuthenticated() {
  const baseUrl = String(registry.settings.ctoxDevBaseUrl || "https://ctox.dev").replace(/\/+$/, "");
  const response = await session.defaultSession.fetch(`${baseUrl}/api/desktop/session-package`, {
    cache: "no-store",
    credentials: "include",
    headers: { "x-ctox-desktop-client": "ctox-business-os-desktop" },
  });
  if (!response.ok) return false;
  const payload = await response.json().catch(() => ({}));
  const account = payload?.account;
  // Require an explicit authenticated flag or at least one tenant; an empty
  // tenants array must not be read as an authenticated session.
  return account?.authenticated === true
    || (Array.isArray(account?.tenants) && account.tenants.length > 0);
}

async function logoutCtoxDev() {
  const result = await clearCtoxDevSession(session.defaultSession, registry.settings.ctoxDevBaseUrl);
  destroyManagedViews();
  const instances = await sourceManager.listInstances();
  return {
    ...result,
    instances,
  };
}

function destroyManagedViews() {
  for (const instanceId of Array.from(views.keys())) {
    if (String(instanceId).startsWith("managed:")) destroyInstanceView(instanceId);
  }
}

async function openCtoxDevManagedInstance(instance) {
  if (instance?.source !== "ctox_dev") {
    throw new Error("only ctox.dev managed instances can be managed in ctox.dev");
  }
  const url = buildCtoxDevManagedInstanceUrl(registry.settings.ctoxDevBaseUrl, instance);
  await shell.openExternal(url);
  return { ok: true, url };
}

ipcMain.handle("instances:list", async () => sourceManager.listInstances());
ipcMain.handle("instances:activate", async (_event, instance) => activateInstance(instance));
ipcMain.handle("instances:remove", async (_event, instance) => removeInstance(instance));
ipcMain.handle("app-shell:show", async () => showAppShell());
ipcMain.handle("app-shell:set-overlay-visible", async (_event, visible) => setChromeOverlayVisible(visible));
ipcMain.handle("app-shell:open-switcher", async () => openInstanceSwitcherOverlay());
ipcMain.handle("invites:import", async (_event, rawInvite) => sourceManager.importInvite(rawInvite));
ipcMain.handle("pairing:manual", async (_event, options) => sourceManager.importManualPairing(options || {}));
ipcMain.handle("pairing:rotate", async (_event, instance, rawInvite) => rotatePairing(instance, rawInvite));
ipcMain.handle("pairing:revoke", async (_event, instance) => revokePairing(instance));
ipcMain.handle("local:inspect", async (_event, options) => sourceManager.inspectLocalDaemon(options || {}));
ipcMain.handle("local:attach", async (_event, options) => sourceManager.attachLocalDaemon(options || {}));
ipcMain.handle("local:install", async (_event, options) => sourceManager.installLocalCtox(options || {}));
ipcMain.handle("ssh:inspect-host-key", async (_event, options) => sourceManager.inspectSshHostKey(options || {}));
ipcMain.handle("ssh:preflight", async (_event, options) => sourceManager.preflightSshManaged(options || {}));
ipcMain.handle("ssh:attach", async (_event, options) => sourceManager.attachSshManaged(options || {}));
ipcMain.handle("ssh:install", async (_event, options) => sourceManager.installSshManaged(options || {}));
ipcMain.handle("ssh:store-sudo-password", async (_event, options) => sourceManager.storeSshSudoPassword(options || {}));
ipcMain.handle("ssh:store-login-password", async (_event, options) => sourceManager.storeSshLoginPassword(options || {}));
ipcMain.handle("ctox-dev:login", async () => openCtoxDevLogin());
ipcMain.handle("ctox-dev:logout", async () => logoutCtoxDev());
ipcMain.handle("ctox-dev:manage-instance", async (_event, instance) => openCtoxDevManagedInstance(instance));
ipcMain.handle("support:create-snapshot", async () => createSupportBundleSnapshot({
  registry,
  appInfo: {
    name: app.getName(),
    version: app.getVersion(),
    platform: process.platform,
  },
}));

installNativeFileDragBridge({
  ipcMain,
  app,
  nativeImage,
  viewsProvider: () => views,
});

app.whenReady().then(async () => {
  protocolHandling.registerDefaultProtocolClient();
  await createWindow();
  await protocolHandling.flushPending();
  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});

app.on("before-quit", () => {
  bundledShell?.close?.().catch(() => undefined);
  bundledShell = null;
});

module.exports = {
  activateInstance,
  activateManagedInstance,
  setChromeOverlayVisible,
  showAppShell,
};
