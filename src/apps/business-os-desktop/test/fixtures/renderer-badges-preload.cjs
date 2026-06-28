"use strict";

const { contextBridge } = require("electron");

let instances = [{
  id: "managed:tenant_skf",
  source: "ctox_dev",
  displayName: "SKF",
  domain: "skf.ctox.dev",
  role: "owner",
  status: "available",
  healthSummary: {
    dataPlane: "rxdb-webrtc",
    dataPlaneReady: true,
    httpDataProxy: false,
    nativePeerObserved: true,
  },
}, {
  id: "ssh:test",
  source: "ssh_managed",
  displayName: "Remote VPS",
  status: "offline",
  connection: {
    host: "203.0.113.11",
    user: "ubuntu",
    port: 22,
  },
  healthSummary: {
    dataPlane: "rxdb-webrtc",
    dataPlaneReady: false,
    httpDataProxy: false,
    nativePeerObserved: false,
  },
}, {
  id: "paired:lab",
  source: "pairing_invite",
  displayName: "Paired Lab",
  status: "available",
  instanceId: "paired_lab",
  pairing: {
    syncRoom: "ctox-business-os:paired_lab:room",
    signalingUrls: ["wss://signaling.ctox.dev"],
    secretRef: "keychain://ctox-business-os-desktop/paired:lab/room",
  },
  healthSummary: {
    dataPlane: "rxdb-webrtc",
    dataPlaneReady: true,
    httpDataProxy: false,
    nativePeerObserved: true,
  },
}];
const manageRequests = [];
const removeRequests = [];
const rotateRequests = [];
const revokeRequests = [];
const activateRequests = [];
const sudoPasswordRequests = [];
const sshPasswordRequests = [];

contextBridge.exposeInMainWorld("ctoxDesktop", {
  activateInstance: async (instance) => {
    activateRequests.push({
      id: instance.id,
      source: instance.source,
      displayName: instance.displayName,
      dataPlane: instance.healthSummary?.dataPlane || "",
      httpDataProxy: Boolean(instance.healthSummary?.httpDataProxy),
    });
    return {
      ok: true,
      ctoxConfig: {
        transport: "webrtc",
        http_bridge_available: false,
      },
    };
  },
  importManualPairing: async () => ({ ok: true }),
  listInstances: async () => instances,
  removeInstance: async (instance) => {
    removeRequests.push({ id: instance.id, source: instance.source });
    instances = instances.filter((entry) => entry.id !== instance.id);
    return { ok: true };
  },
  rotatePairing: async (instance, rawInvite) => {
    rotateRequests.push({ id: instance.id, source: instance.source, payloadLength: String(rawInvite || "").length });
    return { ...instance, rotated: true };
  },
  revokePairing: async (instance) => {
    revokeRequests.push({ id: instance.id, source: instance.source });
    instances = instances.filter((entry) => entry.id !== instance.id);
    return { ok: true };
  },
  storeSshSudoPassword: async (options) => {
    sudoPasswordRequests.push({
      host: options.host,
      user: options.user,
      port: options.port,
      passwordLength: String(options.sudoPassword || "").length,
    });
    return {
      ok: true,
      sudoPasswordRef: `keychain://ctox-business-os-desktop/ssh-sudo/${options.host}`,
    };
  },
  storeSshLoginPassword: async (options) => {
    sshPasswordRequests.push({
      host: options.host,
      user: options.user,
      port: options.port,
      passwordLength: String(options.sshPassword || "").length,
    });
    return {
      ok: true,
      sshPasswordRef: `keychain://ctox-business-os-desktop/ssh-login/${options.host}`,
    };
  },
  showAppShell: async () => ({ ok: true }),
  setChromeOverlayVisible: async (visible) => ({ ok: true, visible: Boolean(visible) }),
  loginCtoxDev: async () => ({ ok: true }),
  logoutCtoxDev: async () => {
    instances = instances.filter((instance) => instance.source !== "ctox_dev");
    return { ok: true, instances };
  },
  openCtoxDevManagedInstance: async (instance) => {
    manageRequests.push({ id: instance.id, source: instance.source, tenantId: instance.tenantId || "" });
    return { ok: true, url: "https://ctox.dev/dashboard?tenant=tenant_skf" };
  },
});

contextBridge.exposeInMainWorld("ctoxDesktopSmoke", {
  activateRequests: () => activateRequests,
  manageRequests: () => manageRequests,
  removeRequests: () => removeRequests,
  rotateRequests: () => rotateRequests,
  revokeRequests: () => revokeRequests,
  sudoPasswordRequests: () => sudoPasswordRequests,
  sshPasswordRequests: () => sshPasswordRequests,
});
