"use strict";

const { contextBridge } = require("electron");

let instances = [{
  id: "managed:tenant_skf",
  source: "ctox_dev",
  displayName: "SKF",
  domain: "acme.ctox.dev",
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
const localInspectRequests = [];
const localAttachRequests = [];
const localInstallRequests = [];
const sshHostKeyRequests = [];
const sshPreflightRequests = [];
const sshAttachRequests = [];
const sshInstallRequests = [];
const inviteImportRequests = [];
const manualPairingRequests = [];
const ctoxDevLoginRequests = [];

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
  importInvite: async (rawInvite) => {
    inviteImportRequests.push({ payloadLength: String(rawInvite || "").length });
    return { ok: true };
  },
  importManualPairing: async (options) => {
    manualPairingRequests.push({
      displayName: options.displayName,
      syncRoom: options.syncRoom,
      signalingUrl: options.signalingUrl,
      roomSecretLength: String(options.roomSecret || "").length,
    });
    return { ok: true };
  },
  inspectLocalDaemon: async (options) => {
    localInspectRequests.push({ ...options });
    return {
      status: "available",
      instanceId: "local-smoke",
      dataPlane: "rxdb-webrtc",
      httpDataProxy: false,
    };
  },
  attachLocalDaemon: async (options) => {
    localAttachRequests.push({ ...options });
    return { id: "local:smoke", source: "local_daemon", displayName: options.displayName || "Local CTOX" };
  },
  installLocalCtox: async (options) => {
    localInstallRequests.push({ ...options });
    return { ok: true, instance: { id: "local:installed", source: "local_daemon", displayName: options.displayName || "Local CTOX" } };
  },
  inspectSshHostKey: async (options) => {
    sshHostKeyRequests.push({ ...options });
    return {
      algorithm: "ED25519",
      keyType: "ssh-ed25519",
      fingerprint: "SHA256:desktop-smoke-host-key",
    };
  },
  preflightSshManaged: async (options) => {
    sshPreflightRequests.push({ ...options });
    return { sshReachable: true, ctoxAvailable: true };
  },
  attachSshManaged: async (options) => {
    sshAttachRequests.push({ ...options });
    return { id: "ssh:smoke", source: "ssh_managed", displayName: options.displayName || options.host };
  },
  installSshManaged: async (options) => {
    sshInstallRequests.push({ ...options });
    return { ok: true };
  },
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
  loginCtoxDev: async () => {
    ctoxDevLoginRequests.push({ opened: true });
    return { ok: true, completed: false, instances };
  },
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
  localInspectRequests: () => localInspectRequests,
  localAttachRequests: () => localAttachRequests,
  localInstallRequests: () => localInstallRequests,
  sshHostKeyRequests: () => sshHostKeyRequests,
  sshPreflightRequests: () => sshPreflightRequests,
  sshAttachRequests: () => sshAttachRequests,
  sshInstallRequests: () => sshInstallRequests,
  inviteImportRequests: () => inviteImportRequests,
  manualPairingRequests: () => manualPairingRequests,
  ctoxDevLoginRequests: () => ctoxDevLoginRequests,
});
