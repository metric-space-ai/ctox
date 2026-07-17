"use strict";

const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("ctoxDesktop", {
  getAppInfo: () => ipcRenderer.invoke("app:info"),
  listInstances: () => ipcRenderer.invoke("instances:list"),
  activateInstance: (instance) => ipcRenderer.invoke("instances:activate", instance),
  removeInstance: (instance) => ipcRenderer.invoke("instances:remove", instance),
  showAppShell: () => ipcRenderer.invoke("app-shell:show"),
  setChromeOverlayVisible: (visible) => ipcRenderer.invoke("app-shell:set-overlay-visible", Boolean(visible)),
  onOpenSwitcher: (callback) => {
    if (typeof callback !== "function") return () => undefined;
    const listener = () => callback();
    ipcRenderer.on("desktop:switcher-open", listener);
    return () => ipcRenderer.removeListener("desktop:switcher-open", listener);
  },
  importInvite: (rawInvite) => ipcRenderer.invoke("invites:import", rawInvite),
  importManualPairing: (options) => ipcRenderer.invoke("pairing:manual", options),
  rotatePairing: (instance, rawInvite) => ipcRenderer.invoke("pairing:rotate", instance, rawInvite),
  revokePairing: (instance) => ipcRenderer.invoke("pairing:revoke", instance),
  inspectLocalDaemon: (options) => ipcRenderer.invoke("local:inspect", options),
  attachLocalDaemon: (options) => ipcRenderer.invoke("local:attach", options),
  installLocalCtox: (options) => ipcRenderer.invoke("local:install", options),
  inspectSshHostKey: (options) => ipcRenderer.invoke("ssh:inspect-host-key", options),
  preflightSshManaged: (options) => ipcRenderer.invoke("ssh:preflight", options),
  attachSshManaged: (options) => ipcRenderer.invoke("ssh:attach", options),
  installSshManaged: (options) => ipcRenderer.invoke("ssh:install", options),
  storeSshSudoPassword: (options) => ipcRenderer.invoke("ssh:store-sudo-password", options),
  storeSshLoginPassword: (options) => ipcRenderer.invoke("ssh:store-login-password", options),
  loginCtoxDev: () => ipcRenderer.invoke("ctox-dev:login"),
  logoutCtoxDev: () => ipcRenderer.invoke("ctox-dev:logout"),
  openCtoxDevManagedInstance: (instance) => ipcRenderer.invoke("ctox-dev:manage-instance", instance),
  createSupportSnapshot: () => ipcRenderer.invoke("support:create-snapshot"),
});
