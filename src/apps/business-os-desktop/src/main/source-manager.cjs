"use strict";

const { compareInstances, mergeInstances } = require("../common/instance-model.cjs");
const { applyUsageToInstances, markInstanceUsed } = require("./registry.cjs");
const {
  CtoxDevInstanceSource,
  LocalDaemonInstanceSource,
  PairingInviteInstanceSource,
  SshManagedInstanceSource,
} = require("./sources.cjs");

class SourceManager {
  constructor({ registryProvider, registrySaver, secretStore, ctoxDevBaseUrl, shellUrl, managedShellUrl, fetchImpl, knownHostsPath }) {
    this.registryProvider = registryProvider;
    this.registrySaver = registrySaver;
    // Wire an app-owned known_hosts path so SSH host-key pinning is durable and
    // independent of the operator's personal ~/.ssh/known_hosts. Without this the
    // implemented ensureKnownHost/UserKnownHostsFile machinery never ran in prod.
    // Every Desktop-owned source renders the version-matched bundled shell.
    // The registry shellUrl is a legacy remote fallback and ctox.dev disables
    // its global /business-os endpoint in production for tenant isolation.
    const desktopShellUrl = managedShellUrl || shellUrl;
    const pairingOptions = { shellUrl: desktopShellUrl, knownHostsPath };
    this.sources = {
      ctox_dev: new CtoxDevInstanceSource({ baseUrl: ctoxDevBaseUrl, shellUrl: desktopShellUrl, fetchImpl }),
      local_daemon: new LocalDaemonInstanceSource(registryProvider, registrySaver, secretStore, pairingOptions),
      pairing_invite: new PairingInviteInstanceSource(registryProvider, registrySaver, secretStore, pairingOptions),
      ssh_managed: new SshManagedInstanceSource(registryProvider, registrySaver, secretStore, pairingOptions),
    };
  }

  async listInstances() {
    const groups = [];
    for (const source of Object.values(this.sources)) {
      try {
        groups.push(await source.listInstances());
      } catch {
        groups.push([]);
      }
    }
    return applyUsageToInstances(mergeInstances(groups), this.registryProvider()).sort(compareInstances);
  }

  async getLaunchConfig(instance) {
    const source = this.sources[instance.source];
    if (!source) throw new Error(`unsupported source: ${instance.source}`);
    if (instance.source === "ctox_dev" && instance.status && instance.status !== "available") {
      throw new Error(`ctox.dev managed instance is not launchable: ${instance.status}`);
    }
    return source.getLaunchConfig(instance.id, instance);
  }

  async importInvite(rawInvite) {
    return this.sources.pairing_invite.importInvite(rawInvite);
  }

  async importManualPairing(options) {
    return this.sources.pairing_invite.importManualPairing(options);
  }

  async rotatePairing(instance, rawInvite) {
    if (instance?.source !== "pairing_invite" || !instance?.id) {
      throw new Error("pairing instance is required");
    }
    return this.sources.pairing_invite.rotateInvite(instance.id, rawInvite);
  }

  async revokePairing(instance) {
    if (instance?.source !== "pairing_invite" || !instance?.id) {
      throw new Error("pairing instance is required");
    }
    return this.sources.pairing_invite.revokeInstance(instance.id);
  }

  async attachLocalDaemon(options) {
    return this.sources.local_daemon.attachLocalDaemon(options);
  }

  async inspectLocalDaemon(options) {
    return this.sources.local_daemon.inspectLocalDaemon(options);
  }

  async installLocalCtox(options) {
    return this.sources.local_daemon.installFresh(options);
  }

  async inspectSshHostKey(options) {
    return this.sources.ssh_managed.inspectHostKeyForProfile(options);
  }

  async preflightSshManaged(options) {
    return this.sources.ssh_managed.preflight(options);
  }

  async attachSshManaged(options) {
    return this.sources.ssh_managed.attachExisting(options);
  }

  async installSshManaged(options) {
    if (options?.freshInstall) {
      return this.sources.ssh_managed.installFresh(options);
    }
    return this.sources.ssh_managed.installOrUpgradeExisting(options);
  }

  async storeSshSudoPassword(options) {
    return this.sources.ssh_managed.storeSudoPassword(options || {});
  }

  async storeSshLoginPassword(options) {
    return this.sources.ssh_managed.storeSshPassword(options || {});
  }

  async removeInstance(instance) {
    if (!instance?.source || !instance?.id) throw new Error("instance source and id are required");
    if (instance.source === "ctox_dev") {
      throw new Error("managed instances must be removed in ctox.dev");
    }
    const source = this.sources[instance.source];
    if (!source?.removeInstance) throw new Error(`source cannot remove instances: ${instance.source}`);
    return source.removeInstance(instance.id);
  }

  markInstanceUsed(instanceId, now = new Date()) {
    const registry = markInstanceUsed(this.registryProvider(), instanceId, now);
    this.registrySaver(registry);
    return registry.usage[instanceId];
  }
}

module.exports = {
  SourceManager,
};
