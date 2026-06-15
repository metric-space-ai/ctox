"use strict";

const { execFile, spawn } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { promisify } = require("node:util");
const { normalizeInstance, stableId } = require("../common/instance-model.cjs");
const {
  instanceFromInvite,
  manualPairingToInvite,
  parseInvitePayload,
} = require("../common/invites.cjs");
const {
  buildLaunchUrl,
  buildPairingLaunchConfig,
} = require("./launch-config.cjs");
const {
  removeInstance: removeRegistryInstance,
  upsertInstance,
} = require("./registry.cjs");
const {
  ensureKnownHost,
  inspectSshHostKey,
  verifyTrustedHostKey,
} = require("./ssh-host-key.cjs");
const { WINDOWS_CREDENTIAL_MANAGER_SCRIPT } = require("./secret-store.cjs");

const execFileAsync = promisify(execFile);
const OFFICIAL_CTOX_INSTALL_SCRIPT_URL = "https://raw.githubusercontent.com/metric-space-ai/ctox/main/install.sh";
const OFFICIAL_CTOX_RELEASE_DOWNLOAD_BASE_URL = "https://github.com/metric-space-ai/ctox/releases/latest/download";
const LOCAL_CTOX_RESOURCE_DIR = "ctox";

function normalizeCtoxDevSessionPackage(payload) {
  const account = payload?.account && typeof payload.account === "object" ? payload.account : {};
  const tenants = Array.isArray(account.tenants) ? account.tenants : [];
  return tenants.map((tenant) => normalizeInstance({
    id: `managed:${tenant.id}`,
    source: "ctox_dev",
    displayName: tenant.businessName || tenant.domain || tenant.slug || tenant.id,
    domain: tenant.domain || tenant.businessOsUrl || "",
    tenantId: tenant.id,
    role: tenant.tenantRole || "viewer",
    status: tenant.launchAllowed === false ? "needs_auth" : "available",
    healthSummary: {
      dataPlane: "rxdb-webrtc",
      dataPlaneReady: tenant.healthStatus === "ok" || tenant.status === "active",
      httpDataProxy: false,
      nativePeerObserved: tenant.healthStatus === "ok" || tenant.status === "active",
    },
  }));
}

class CtoxDevInstanceSource {
  constructor({ baseUrl = "https://ctox.dev", fetchImpl = globalThis.fetch } = {}) {
    this.baseUrl = baseUrl.replace(/\/+$/, "");
    this.fetch = fetchImpl;
  }

  async listInstances() {
    if (!this.fetch) return [];
    const response = await this.fetch(`${this.baseUrl}/api/desktop/session-package`, {
      cache: "no-store",
      credentials: "include",
      headers: { "x-ctox-desktop-client": "ctox-business-os-desktop" },
    });
    if (response.status === 401) return [];
    if (!response.ok) throw new Error(`ctox.dev session-package failed: ${response.status}`);
    return normalizeCtoxDevSessionPackage(await response.json());
  }

  async getLaunchConfig(instanceId) {
    const tenantId = String(instanceId || "").replace(/^managed:/, "");
    const tokenResponse = await this.fetch(`${this.baseUrl}/api/desktop/launch-token`, {
      method: "POST",
      credentials: "include",
      headers: {
        "content-type": "application/json",
        "x-ctox-desktop-client": "ctox-business-os-desktop",
      },
      body: JSON.stringify({ tenantId }),
    });
    if (!tokenResponse.ok) throw new Error(`ctox.dev launch token failed: ${tokenResponse.status}`);
    const tokenPayload = await tokenResponse.json();
    if (!tokenPayload.launchConfigUrl) throw new Error("ctox.dev launch token response is missing launchConfigUrl");
    const launchResponse = await this.fetch(tokenPayload.launchConfigUrl, {
      method: "POST",
      credentials: "include",
      headers: { "x-ctox-desktop-client": "ctox-business-os-desktop" },
    });
    if (!launchResponse.ok) throw new Error(`ctox.dev launch config failed: ${launchResponse.status}`);
    const launchPayload = await launchResponse.json();
    const ctoxConfig = launchPayload.pairingConfig || {};
    return {
      source: "ctox_dev",
      launchUrl: buildLaunchUrl(launchPayload.launchUrl, ctoxConfig),
      ctoxConfig,
      expiresAt: tokenPayload.expiresAt,
    };
  }
}

class RegistryBackedSource {
  constructor(source, registryProvider, registrySaver, secretStore, options = {}) {
    this.source = source;
    this.registryProvider = registryProvider;
    this.registrySaver = registrySaver;
    this.secretStore = secretStore;
    this.options = options;
  }

  listInstances() {
    return this.registryProvider().instances.filter((instance) => instance.source === this.source);
  }

  async removeInstance(instanceId) {
    const instance = this.listInstances().find((entry) => entry.id === instanceId);
    if (!instance) throw new Error(`${this.source} instance not found`);
    for (const ref of instance.secretRefs || []) {
      if (this.secretStore?.delete) await this.secretStore.delete(ref);
    }
    this.registrySaver(removeRegistryInstance(this.registryProvider(), instance.id));
    return { ok: true };
  }
}

class PairingInviteInstanceSource extends RegistryBackedSource {
  constructor(registryProvider, registrySaver, secretStore, options = {}) {
    super("pairing_invite", registryProvider, registrySaver, secretStore, options);
  }

  async importInvite(rawInvite, now = new Date()) {
    return this.importParsedInvite(parseInvitePayload(rawInvite, now));
  }

  async importManualPairing(options = {}) {
    return this.importParsedInvite(manualPairingToInvite(options));
  }

  async importParsedInvite(invite) {
    const { instance, secretMaterial } = instanceFromInvite(invite);
    for (const secret of secretMaterial) {
      if (!this.secretStore?.set) throw new Error("secret store is required for invite secrets");
      await this.secretStore.set(secret.ref, secret.value);
    }
    this.registrySaver(upsertInstance(this.registryProvider(), instance));
    return instance;
  }

  async rotateInvite(instanceId, rawInvite, now = new Date()) {
    const existing = this.findInstance(instanceId);
    const { instance, secretMaterial } = instanceFromInvite(parseInvitePayload(rawInvite, now));
    if (!pairingInstancesShareIdentity(existing, instance)) {
      throw new Error("replacement invite does not match paired instance");
    }
    for (const secret of secretMaterial) {
      if (!this.secretStore?.set) throw new Error("secret store is required for invite secrets");
      await this.secretStore.set(secret.ref, secret.value);
    }
    let nextRegistry = this.registryProvider();
    if (instance.id !== existing.id) {
      const nextSecretRefs = new Set(instance.secretRefs || []);
      for (const ref of existing.secretRefs || []) {
        if (!nextSecretRefs.has(ref) && this.secretStore?.delete) await this.secretStore.delete(ref);
      }
      nextRegistry = removeRegistryInstance(nextRegistry, existing.id);
    }
    this.registrySaver(upsertInstance(nextRegistry, instance));
    return instance;
  }

  async revokeInstance(instanceId) {
    return this.removeInstance(this.findInstance(instanceId).id);
  }

  async getLaunchConfig(instanceId) {
    const instance = this.findInstance(instanceId);
    return buildPairingLaunchConfig(instance, this.secretStore, this.options);
  }

  findInstance(instanceId) {
    const instance = this.listInstances().find((entry) => entry.id === instanceId);
    if (!instance) throw new Error("paired instance not found");
    return instance;
  }
}

function pairingInstancesShareIdentity(left, right) {
  return left?.source === "pairing_invite"
    && right?.source === "pairing_invite"
    && Boolean(String(left.instanceId || "").trim())
    && String(left.instanceId || "").trim() === String(right.instanceId || "").trim();
}

class LocalDaemonInstanceSource extends RegistryBackedSource {
  constructor(registryProvider, registrySaver, secretStore, options = {}) {
    super("local_daemon", registryProvider, registrySaver, secretStore, options);
    this.runStatusCommand = options.runStatusCommand || options.runCommand || runLocalPeerStatusCommand;
    this.runEnsureCommand = options.runEnsureCommand || options.runCommand || runLocalPeerEnsureCommand;
    this.runInstallCommand = options.runInstallCommand || runLocalBusinessOsInstallCommand;
  }

  async attachLocalDaemon(options = {}) {
    const profile = normalizeLocalProfile(options);
    const peerStatus = options.ensurePeer === false
      ? await this.runStatusCommand(profile)
      : await this.runEnsureCommand(profile);
    const { instance, secretMaterial } = instanceFromPeerStatus(peerStatus, {
      source: "local_daemon",
      displayName: options.displayName || "Local CTOX",
      connection: {
        ctoxBinary: profile.ctoxBinary,
        ctoxRoot: profile.ctoxRoot,
        managedBy: "local",
      },
    });
    for (const secret of secretMaterial) {
      await this.secretStore.set(secret.ref, secret.value);
    }
    this.registrySaver(upsertInstance(this.registryProvider(), instance));
    return instance;
  }

  async inspectLocalDaemon(options = {}) {
    const profile = normalizeLocalProfile(options);
    try {
      const peerStatus = await this.runStatusCommand(profile);
      return {
        status: peerStatus.native_rxdb_peer_available === false ? "offline" : "available",
        ctoxBinary: profile.ctoxBinary,
        ctoxRoot: profile.ctoxRoot,
        instanceId: String(peerStatus.instance_id || "").trim(),
        syncRoom: String(peerStatus.sync_room || "").trim(),
        signalingUrls: Array.isArray(peerStatus.signaling_urls) ? peerStatus.signaling_urls : [],
        dataPlane: "rxdb-webrtc",
        httpDataProxy: false,
      };
    } catch (error) {
      return localDaemonInspectionError(error, profile);
    }
  }

  async installLocalBusinessOs(options = {}) {
    return this.runInstallCommand(normalizeLocalProfile(options), normalizeLocalInstallOptions(options));
  }

  async getLaunchConfig(instanceId) {
    const instance = this.listInstances().find((entry) => entry.id === instanceId);
    if (!instance) throw new Error("local daemon instance not found");
    return buildPairingLaunchConfig(instance, this.secretStore, this.options);
  }
}

class SshManagedInstanceSource extends RegistryBackedSource {
  constructor(registryProvider, registrySaver, secretStore, options = {}) {
    super("ssh_managed", registryProvider, registrySaver, secretStore, options);
    this.runStatusCommand = options.runStatusCommand || options.runCommand || runSshPeerStatusCommand;
    this.runEnsureCommand = options.runEnsureCommand || options.runCommand || runSshPeerEnsureCommand;
    this.runPreflightCommand = options.runPreflightCommand || runSshPreflightCommand;
    this.runExistingInstallCommand = options.runExistingInstallCommand || options.runInstallCommand || runSshExistingCtoxInstallCommand;
    this.runFreshInstallCommand = options.runFreshInstallCommand
      || ((profile, install) => runSshFreshCtoxInstallCommand(profile, install, this.secretStore));
    this.inspectHostKey = options.inspectHostKey || ((profile) => inspectSshHostKey(profile));
    this.knownHostsPath = options.knownHostsPath || "";
  }

  async inspectHostKeyForProfile(options = {}) {
    return this.inspectHostKey(normalizeSshProfile(options));
  }

  async attachExisting(options = {}) {
    const profile = normalizeSshProfile(options);
    const { hostKey, commandProfile } = await this.trustedCommandProfile(profile, options.trustedHostKeyFingerprint);
    const peerStatus = options.ensurePeer === false
      ? await this.runStatusCommand(commandProfile)
      : await this.runEnsureCommand(commandProfile);
    const { instance, secretMaterial } = instanceFromPeerStatus(peerStatus, {
      source: "ssh_managed",
      displayName: options.displayName || profile.host,
      connection: sshConnectionMetadata(profile, hostKey),
    });
    for (const secret of secretMaterial) {
      await this.secretStore.set(secret.ref, secret.value);
    }
    this.registrySaver(upsertInstance(this.registryProvider(), instance));
    return instance;
  }

  async preflight(options = {}) {
    const profile = normalizeSshProfile(options);
    const { commandProfile } = await this.trustedCommandProfile(profile, options.trustedHostKeyFingerprint);
    return normalizeSshPreflight(await this.runPreflightCommand(commandProfile), profile);
  }

  async installOrUpgradeExisting(options = {}) {
    const profile = normalizeSshProfile(options);
    const install = normalizeSshInstallOptions(options);
    const { hostKey, commandProfile } = await this.trustedCommandProfile(profile, options.trustedHostKeyFingerprint);
    const preflight = normalizeSshPreflight(await this.runPreflightCommand(commandProfile), profile);
    if (!preflight.ctoxAvailable) {
      throw new Error("remote ctox binary is required before existing-instance upgrade");
    }
    const installResult = await this.runExistingInstallCommand(commandProfile, install);
    const peerStatus = await this.runEnsureCommand(commandProfile);
    const { instance, secretMaterial } = instanceFromPeerStatus(peerStatus, {
      source: "ssh_managed",
      displayName: options.displayName || profile.host,
      connection: {
        ...sshConnectionMetadata(profile, hostKey),
        installReleaseChannel: install.releaseChannel,
        lastInstallAt: new Date().toISOString(),
      },
    });
    for (const secret of secretMaterial) {
      await this.secretStore.set(secret.ref, secret.value);
    }
    this.registrySaver(upsertInstance(this.registryProvider(), instance));
    return { instance, preflight, install: installResult };
  }

  async installFresh(options = {}) {
    const profile = normalizeSshProfile(options);
    const install = normalizeSshInstallOptions(options);
    const { hostKey, commandProfile } = await this.trustedCommandProfile(profile, options.trustedHostKeyFingerprint);
    const preflight = normalizeSshPreflight(await this.runPreflightCommand(commandProfile), profile);
    assertFreshSshInstallPreflight(preflight, install);
    const installResult = await this.runFreshInstallCommand(commandProfile, install);
    const peerStatus = await this.runEnsureCommand(commandProfile);
    const { instance, secretMaterial } = instanceFromPeerStatus(peerStatus, {
      source: "ssh_managed",
      displayName: options.displayName || profile.host,
      connection: {
        ...sshConnectionMetadata(profile, hostKey),
        installMode: "fresh",
        installReleaseChannel: install.releaseChannel,
        lastInstallAt: new Date().toISOString(),
      },
    });
    for (const secret of secretMaterial) {
      await this.secretStore.set(secret.ref, secret.value);
    }
    this.registrySaver(upsertInstance(this.registryProvider(), instance));
    return { instance, preflight, install: installResult };
  }

  async storeSudoPassword(options = {}) {
    const profile = normalizeSshProfile(options);
    const password = String(options.sudoPassword || "");
    if (!password) throw new Error("sudo password is required");
    if (!this.secretStore?.set) throw new Error("secret store is required for sudo password refs");
    const sudoPasswordRef = buildSshSudoPasswordRef(profile);
    await this.secretStore.set(sudoPasswordRef, password);
    return {
      ok: true,
      sudoPasswordRef,
      host: profile.host,
      user: profile.user,
      port: profile.port,
    };
  }

  async storeSshPassword(options = {}) {
    const profile = normalizeSshProfile(options);
    const password = String(options.sshPassword || "");
    if (!password) throw new Error("ssh password is required");
    if (!this.secretStore?.set) throw new Error("secret store is required for ssh password refs");
    const sshPasswordRef = buildSshPasswordRef(profile);
    await this.secretStore.set(sshPasswordRef, password);
    return {
      ok: true,
      sshPasswordRef,
      host: profile.host,
      user: profile.user,
      port: profile.port,
    };
  }

  async trustedCommandProfile(profile, trustedHostKeyFingerprint) {
    const hostKey = await this.inspectHostKey(profile);
    verifyTrustedHostKey(hostKey, trustedHostKeyFingerprint);
    if (this.knownHostsPath) {
      ensureKnownHost({
        knownHostsPath: this.knownHostsPath,
        host: profile.host,
        port: profile.port,
        knownHostsLine: hostKey.knownHostsLine,
      });
    }
    return {
      hostKey,
      commandProfile: {
        ...profile,
        knownHostsPath: this.knownHostsPath,
      },
    };
  }

  async getLaunchConfig(instanceId) {
    const instance = this.listInstances().find((entry) => entry.id === instanceId);
    if (!instance) throw new Error("ssh-managed instance not found");
    return buildPairingLaunchConfig(instance, this.secretStore, this.options);
  }
}

function sshConnectionMetadata(profile, hostKey) {
  return {
    host: profile.host,
    user: profile.user,
    port: profile.port,
    installRoot: profile.installRoot,
    managedBy: "ssh",
    hostKeyFingerprint: hostKey.fingerprint,
    hostKeyAlgorithm: hostKey.algorithm,
    hostKeyType: hostKey.keyType,
    hostKeyScannedAt: hostKey.scannedAt,
  };
}

async function runLocalPeerStatusCommand(profile) {
  const { stdout } = await execFileAsync(profile.ctoxBinary, buildLocalPeerArgs("status"), {
    ...buildLocalCommandOptions(profile, 15000),
  });
  return JSON.parse(stdout);
}

async function runLocalPeerEnsureCommand(profile) {
  await execFileAsync(profile.ctoxBinary, buildLocalPeerArgs("ensure"), {
    ...buildLocalCommandOptions(profile, 30000),
  });
  return runLocalPeerStatusCommand(profile);
}

async function runLocalBusinessOsInstallCommand(profile, install) {
  const { stdout, stderr } = await execFileAsync(profile.ctoxBinary, buildLocalInstallArgs(install), {
    ...buildLocalCommandOptions(profile, install.dryRun ? 30000 : 120000),
  });
  return {
    ok: true,
    target: install.target,
    dryRun: install.dryRun,
    stdout: stdout.trim(),
    stderr: stderr.trim(),
  };
}

function buildLocalCommandOptions(profile, timeout) {
  const options = {
    timeout,
    windowsHide: true,
    cwd: profile.ctoxRoot || undefined,
  };
  if (profile.ctoxRoot) {
    assertLocalCtoxRoot(profile.ctoxRoot);
    options.env = {
      ...process.env,
      CTOX_ROOT: profile.ctoxRoot,
    };
  }
  return options;
}

function assertLocalCtoxRoot(ctoxRoot) {
  const root = String(ctoxRoot || "").trim();
  if (!root) return;
  const hasCargoToml = fs.existsSync(path.join(root, "Cargo.toml"));
  const hasEntrypoint = fs.existsSync(path.join(root, "src", "main.rs"))
    || fs.existsSync(path.join(root, "src", "core", "main.rs"));
  const hasCreationLedger = fs.existsSync(path.join(root, "contracts", "history", "creation-ledger.md"));
  if (!hasCargoToml || !hasEntrypoint || !hasCreationLedger) {
    throw new Error(`ctox root is not a CTOX runtime root: ${root}`);
  }
}

function buildLocalPeerArgs(command) {
  if (!["status", "ensure"].includes(command)) throw new Error(`unsupported local peer command: ${command}`);
  return ["business-os", "peer", command];
}

function buildLocalInstallArgs(install) {
  const args = ["business-os", "install", "--target", install.target];
  if (install.initGit) args.push("--init-git");
  if (install.noCopyEnv) args.push("--no-copy-env");
  if (install.dryRun) args.push("--dry-run");
  return args;
}

async function runSshPeerStatusCommand(profile) {
  return runSshPeerCommand(profile, "status");
}

async function runSshPeerEnsureCommand(profile) {
  return runSshPeerCommand(profile, "ensure");
}

async function runSshPeerCommand(profile, peerCommand) {
  const remoteCommand = buildSshPeerRemoteCommand(profile, peerCommand);
  const { stdout } = await runSshProgram(profile, "ssh", buildSshArgs(profile, remoteCommand), {
    timeout: 30000,
    windowsHide: true,
  });
  return JSON.parse(stdout);
}

function buildSshPeerRemoteCommand(profile, peerCommand) {
  if (!["status", "ensure"].includes(peerCommand)) throw new Error(`unsupported ssh peer command: ${peerCommand}`);
  const peerLine = peerCommand === "ensure"
    ? "ctox business-os peer ensure >/dev/null; ctox business-os peer status"
    : "ctox business-os peer status";
  return [
    "set -eu",
    "export PATH=\"$HOME/.local/bin:$HOME/.local/lib/ctox/current/bin:/usr/local/bin:$PATH\"",
    profile.installRoot ? `cd ${shellQuote(profile.installRoot)}` : "",
    peerLine,
  ].filter(Boolean).join("; ");
}

async function runSshPreflightCommand(profile) {
  const { stdout } = await runSshProgram(profile, "ssh", buildSshArgs(profile, buildSshPreflightCommand(profile)), {
    timeout: 30000,
    windowsHide: true,
  });
  return parseSshPreflightOutput(stdout);
}

async function runSshExistingCtoxInstallCommand(profile, install) {
  const { stdout, stderr } = await runSshProgram(profile, "ssh", buildSshArgs(profile, buildSshExistingCtoxInstallCommand(profile, install)), {
    timeout: 600000,
    windowsHide: true,
  });
  return {
    ok: true,
    releaseChannel: install.releaseChannel,
    restartService: install.restartService,
    stdout: stdout.trim(),
    stderr: stderr.trim(),
  };
}

function buildSshExistingCtoxInstallCommand(profile, install) {
  const upgradeFlag = install.releaseChannel === "dev" ? "--dev" : "--stable";
  return [
    "set -eu",
    "export PATH=\"$HOME/.local/bin:$HOME/.local/lib/ctox/current/bin:/usr/local/bin:$PATH\"",
    profile.installRoot ? `cd ${shellQuote(profile.installRoot)}` : "",
    `ctox upgrade ${upgradeFlag}`,
    install.restartService ? "ctox start" : "",
    "ctox status",
  ].filter(Boolean).join("; ");
}

async function runSshFreshCtoxInstallCommand(profile, install, secretStore, runner = runProcess) {
  if (install.localArtifactPath) {
    return runSshLocalArtifactInstallCommand(profile, install, runner);
  }
  const input = await resolveSudoPasswordInput(install, secretStore);
  const artifact = shouldUseStableReleaseBundleInstall(install) ? "release" : "source";
  const { stdout, stderr } = await runSshProgram(profile, "ssh", buildSshArgs(profile, buildSshFreshCtoxInstallCommand(profile, install)), {
    timeout: 900000,
    windowsHide: true,
    ...(input ? { input } : {}),
  }, runner);
  return {
    ok: true,
    mode: "fresh",
    artifact,
    releaseChannel: install.releaseChannel,
    restartService: install.restartService,
    apiProvider: install.apiProvider || "",
    model: install.model || "",
    backend: install.backend || "",
    stdout: stdout.trim(),
    stderr: stderr.trim(),
  };
}

async function runSshLocalArtifactInstallCommand(profile, install, runner = runProcess) {
  const remoteArtifactPath = ".cache/ctox/business-os-desktop/ctox-local-artifact";
  const prepareCommand = buildSshLocalArtifactPrepareCommand(remoteArtifactPath);
  await runSshProgram(profile, "ssh", buildSshArgs(profile, prepareCommand), {
    timeout: 30000,
    windowsHide: true,
  }, runner);
  await runSshProgram(profile, "scp", buildScpArgs(profile, install.localArtifactPath, remoteArtifactPath), {
    timeout: 300000,
    windowsHide: true,
  }, runner);
  const { stdout, stderr } = await runSshProgram(
    profile,
    "ssh",
    buildSshArgs(profile, buildSshLocalArtifactInstallCommand(profile, install, remoteArtifactPath)),
    {
      timeout: 300000,
      windowsHide: true,
    },
    runner,
  );
  return {
    ok: true,
    mode: "fresh",
    artifact: "local",
    releaseChannel: install.releaseChannel,
    restartService: install.restartService,
    stdout: stdout.trim(),
    stderr: stderr.trim(),
  };
}

function buildSshFreshCtoxInstallCommand(_profile, install) {
  if (shouldUseStableReleaseBundleInstall(install)) {
    return buildSshStableReleaseBundleFreshInstallCommand(install);
  }
  const upgradeFlag = install.releaseChannel === "dev" ? "--dev" : "--stable";
  const sudoPrelude = install.sudoPasswordRef
    ? buildInteractiveSudoAskpassPrelude()
    : "sudo -n true >/dev/null";
  const installerArgs = buildCtoxInstallerArgs(install);
  const installerCommand = installerArgs.length
    ? `curl -fsSL ${shellQuote(OFFICIAL_CTOX_INSTALL_SCRIPT_URL)} | bash -s -- ${installerArgs.map(shellQuote).join(" ")}`
    : `curl -fsSL ${shellQuote(OFFICIAL_CTOX_INSTALL_SCRIPT_URL)} | bash`;
  return [
    "set -eu",
    "if [ \"$(uname -s 2>/dev/null || true)\" != \"Linux\" ]; then echo 'ctox desktop ssh fresh install requires Linux' >&2; exit 12; fi",
    "command -v bash >/dev/null",
    "command -v curl >/dev/null",
    "command -v sudo >/dev/null",
    sudoPrelude,
    installerCommand,
    "export PATH=\"$HOME/.local/bin:$HOME/.local/lib/ctox/current/bin:/usr/local/bin:$PATH\"",
    `ctox upgrade ${upgradeFlag}`,
    install.restartService ? "ctox start" : "",
    "ctox status",
  ].filter(Boolean).join("; ");
}

function shouldUseStableReleaseBundleInstall(install = {}) {
  return install.releaseChannel === "stable";
}

function buildSshStableReleaseBundleFreshInstallCommand(install) {
  const sudoPrelude = install.sudoPasswordRef
    ? buildInteractiveSudoAskpassPrelude()
    : "sudo -n true >/dev/null";
  const runtimeSeedCommands = buildSshRuntimeSeedCommands(install);
  return [
    "set -eu",
    "if [ \"$(uname -s 2>/dev/null || true)\" != \"Linux\" ]; then echo 'ctox desktop ssh fresh install requires Linux' >&2; exit 12; fi",
    "command -v bash >/dev/null",
    "command -v curl >/dev/null",
    "command -v sudo >/dev/null",
    "command -v tar >/dev/null",
    "command -v sha256sum >/dev/null",
    "command -v mktemp >/dev/null",
    runtimeSeedCommands.length ? "command -v sqlite3 >/dev/null" : "",
    sudoPrelude,
    "ARCH=$(uname -m 2>/dev/null || true)",
    "case \"$ARCH\" in x86_64|amd64) CTOX_ASSET=ctox-linux-x64.tar.gz ;; aarch64|arm64) CTOX_ASSET=ctox-linux-arm64.tar.gz ;; *) echo \"unsupported ctox desktop ssh fresh install architecture: $ARCH\" >&2; exit 13 ;; esac",
    "CTOX_RELEASE_BASE=" + shellQuote(OFFICIAL_CTOX_RELEASE_DOWNLOAD_BASE_URL),
    "CTOX_TMP=$(mktemp -d)",
    "cleanup_ctox_release_bundle() { rm -rf \"$CTOX_TMP\"; }",
    "trap cleanup_ctox_release_bundle EXIT HUP INT TERM",
    "curl -fsSL -o \"$CTOX_TMP/$CTOX_ASSET\" \"$CTOX_RELEASE_BASE/$CTOX_ASSET\"",
    "curl -fsSL -o \"$CTOX_TMP/$CTOX_ASSET.sha256\" \"$CTOX_RELEASE_BASE/$CTOX_ASSET.sha256\"",
    "( cd \"$CTOX_TMP\" && sha256sum -c \"$CTOX_ASSET.sha256\" )",
    "mkdir -p \"$CTOX_TMP/bundle\" \"$HOME/.local/bin\"",
    "tar -xzf \"$CTOX_TMP/$CTOX_ASSET\" -C \"$CTOX_TMP/bundle\"",
    "test -x \"$CTOX_TMP/bundle/target/release/ctox\"",
    "cp \"$CTOX_TMP/bundle/target/release/ctox\" \"$HOME/.local/bin/ctox\"",
    "chmod 755 \"$HOME/.local/bin/ctox\"",
    "export PATH=\"$HOME/.local/bin:$HOME/.local/lib/ctox/current/bin:/usr/local/bin:$PATH\"",
    ...runtimeSeedCommands,
    install.restartService ? "ctox start" : "",
    "ctox status",
  ].filter(Boolean).join("; ");
}

function buildSshRuntimeSeedCommands(install = {}) {
  const rows = [];
  if (install.apiProvider) {
    rows.push(["CTOX_CHAT_SOURCE", "api"]);
    rows.push(["CTOX_API_PROVIDER", normalizeRuntimeApiProvider(install.apiProvider)]);
  }
  if (install.model) {
    rows.push(["CTOX_CHAT_MODEL", install.model]);
    rows.push(["CTOX_CHAT_MODEL_BASE", install.model]);
    rows.push(["CTOX_ACTIVE_MODEL", install.model]);
  }
  if (rows.length === 0) return [];
  const values = rows
    .map(([key, value]) => `(${sqlQuote(key)}, ${sqlQuote(value)})`)
    .join(", ");
  const sql = [
    "PRAGMA busy_timeout=60000;",
    "CREATE TABLE IF NOT EXISTS runtime_env_kv (env_key TEXT PRIMARY KEY, env_value TEXT NOT NULL);",
    `INSERT OR REPLACE INTO runtime_env_kv(env_key, env_value) VALUES ${values};`,
  ].join(" ");
  return [
    "mkdir -p \"$HOME/runtime\"",
    `printf '%s\\n' ${shellQuote(sql)} > "$CTOX_TMP/runtime-seed.sql"`,
    "for CTOX_RUNTIME_DB in \"$HOME/runtime/ctox.sqlite3\" \"$HOME/runtime/ctox-runtime.sqlite3\"; do sqlite3 \"$CTOX_RUNTIME_DB\" < \"$CTOX_TMP/runtime-seed.sql\"; done",
  ];
}

function normalizeRuntimeApiProvider(provider) {
  const normalized = String(provider || "").trim().toLowerCase();
  if (["azure", "azure-foundry", "azure_openai"].includes(normalized)) return "azure_foundry";
  return normalized;
}

function sqlQuote(value) {
  return `'${String(value).replace(/'/g, "''")}'`;
}

function buildCtoxInstallerArgs(install = {}) {
  const args = [];
  if (install.apiProvider) args.push("--api-provider", install.apiProvider);
  if (install.model) args.push("--model", install.model);
  if (install.backend) args.push("--backend", install.backend);
  return args;
}

function buildSshLocalArtifactPrepareCommand(remoteArtifactPath = ".cache/ctox/business-os-desktop/ctox-local-artifact") {
  return [
    "set -eu",
    `mkdir -p ${shellQuote(remoteDirname(remoteArtifactPath))} "$HOME/.local/bin"`,
    `rm -f ${shellQuote(remoteArtifactPath)}`,
  ].join("; ");
}

function buildSshLocalArtifactInstallCommand(_profile, install, remoteArtifactPath = ".cache/ctox/business-os-desktop/ctox-local-artifact") {
  return [
    "set -eu",
    "if [ \"$(uname -s 2>/dev/null || true)\" != \"Linux\" ]; then echo 'ctox desktop ssh artifact install requires Linux' >&2; exit 12; fi",
    `test -s ${shellQuote(remoteArtifactPath)}`,
    `chmod 755 ${shellQuote(remoteArtifactPath)}`,
    "mkdir -p \"$HOME/.local/bin\"",
    `cp ${shellQuote(remoteArtifactPath)} "$HOME/.local/bin/ctox"`,
    "chmod 755 \"$HOME/.local/bin/ctox\"",
    `rm -f ${shellQuote(remoteArtifactPath)}`,
    "export PATH=\"$HOME/.local/bin:$HOME/.local/lib/ctox/current/bin:/usr/local/bin:$PATH\"",
    install.restartService ? "ctox start" : "",
    "ctox status",
  ].filter(Boolean).join("; ");
}

function buildInteractiveSudoAskpassPrelude() {
  return [
    "ASKPASS_DIR=$(mktemp -d)",
    "cleanup_ctox_sudo_askpass() { rm -rf \"$ASKPASS_DIR\"; }",
    "trap cleanup_ctox_sudo_askpass EXIT HUP INT TERM",
    "PASS_FIFO=\"$ASKPASS_DIR/sudo-pass\"",
    "ASKPASS=\"$ASKPASS_DIR/sudo-askpass.sh\"",
    "mkfifo \"$PASS_FIFO\"",
    "printf '%s\\n' '#!/bin/sh' \"cat \\\"$PASS_FIFO\\\"\" > \"$ASKPASS\"",
    "chmod 700 \"$ASKPASS\"",
    "IFS= read -r CTOX_SUDO_PASSWORD",
    "( printf '%s\\n' \"$CTOX_SUDO_PASSWORD\" > \"$PASS_FIFO\" ) & CTOX_SUDO_WRITER=$!",
    "unset CTOX_SUDO_PASSWORD",
    "set +e",
    "SUDO_ASKPASS=\"$ASKPASS\" sudo -A -v",
    "CTOX_SUDO_RESULT=$?",
    "set -e",
    "kill \"$CTOX_SUDO_WRITER\" 2>/dev/null || true",
    "wait \"$CTOX_SUDO_WRITER\" 2>/dev/null || true",
    "if [ \"$CTOX_SUDO_RESULT\" -ne 0 ]; then exit \"$CTOX_SUDO_RESULT\"; fi",
  ].join("; ");
}

function buildSshPreflightCommand(profile = {}) {
  return [
    "set +e",
    "OS_NAME=$(uname -s 2>/dev/null || true)",
    "OS_ARCH=$(uname -m 2>/dev/null || true)",
    "SHELL_PATH=$(command -v sh 2>/dev/null || true)",
    "BASH_PATH=$(command -v bash 2>/dev/null || true)",
    "CURL_PATH=$(command -v curl 2>/dev/null || true)",
    "SYSTEMCTL_PATH=$(command -v systemctl 2>/dev/null || true)",
    "SUDO_PATH=$(command -v sudo 2>/dev/null || true)",
    "CTOX_PATH=$(command -v ctox 2>/dev/null || true)",
    "SUDO_N=false",
    "if [ -n \"$SUDO_PATH\" ]; then sudo -n true >/dev/null 2>&1 && SUDO_N=true; fi",
    profile.installRoot ? `INSTALL_ROOT=${shellQuote(profile.installRoot)}` : "INSTALL_ROOT=",
    "INSTALL_ROOT_EXISTS=false",
    "if [ -n \"$INSTALL_ROOT\" ] && [ -d \"$INSTALL_ROOT\" ]; then INSTALL_ROOT_EXISTS=true; fi",
    "printf 'os_name=%s\\n' \"$OS_NAME\"",
    "printf 'os_arch=%s\\n' \"$OS_ARCH\"",
    "printf 'shell_path=%s\\n' \"$SHELL_PATH\"",
    "printf 'bash_path=%s\\n' \"$BASH_PATH\"",
    "printf 'curl_path=%s\\n' \"$CURL_PATH\"",
    "printf 'systemctl_path=%s\\n' \"$SYSTEMCTL_PATH\"",
    "printf 'sudo_path=%s\\n' \"$SUDO_PATH\"",
    "printf 'sudo_nopasswd=%s\\n' \"$SUDO_N\"",
    "printf 'ctox_path=%s\\n' \"$CTOX_PATH\"",
    "printf 'install_root=%s\\n' \"$INSTALL_ROOT\"",
    "printf 'install_root_exists=%s\\n' \"$INSTALL_ROOT_EXISTS\"",
  ].join("; ");
}

function parseSshPreflightOutput(output) {
  const result = {};
  for (const line of String(output || "").split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const index = trimmed.indexOf("=");
    if (index <= 0) continue;
    result[trimmed.slice(0, index)] = trimmed.slice(index + 1);
  }
  return result;
}

function normalizeSshPreflight(raw, profile) {
  const value = raw && typeof raw === "object" ? raw : {};
  const sudoAvailable = Boolean(value.sudo_path);
  const passwordlessSudoAvailable = value.sudo_nopasswd === "true";
  return {
    host: profile.host,
    user: profile.user,
    port: profile.port,
    installRoot: profile.installRoot,
    sshReachable: true,
    os: {
      name: String(value.os_name || "").trim(),
      arch: String(value.os_arch || "").trim(),
    },
    shellAvailable: Boolean(value.shell_path),
    bashAvailable: Boolean(value.bash_path),
    curlAvailable: Boolean(value.curl_path),
    systemdAvailable: Boolean(value.systemctl_path),
    sudoAvailable,
    passwordlessSudoAvailable,
    needsSudoPassword: sudoAvailable && !passwordlessSudoAvailable,
    ctoxAvailable: Boolean(value.ctox_path),
    installRootExists: value.install_root_exists === "true",
    ctoxPath: String(value.ctox_path || "").trim(),
    bashPath: String(value.bash_path || "").trim(),
    curlPath: String(value.curl_path || "").trim(),
    dataPlane: "rxdb-webrtc",
    httpDataProxy: false,
  };
}

function assertFreshSshInstallPreflight(preflight, install = {}) {
  const usesLocalArtifact = Boolean(install.localArtifactPath);
  if (String(preflight.os?.name || "").toLowerCase() !== "linux") {
    throw new Error("ssh fresh install requires a Linux host");
  }
  if (!preflight.shellAvailable || !preflight.bashAvailable) {
    throw new Error("ssh fresh install requires sh and bash on the remote host");
  }
  if (!usesLocalArtifact && !preflight.curlAvailable) {
    throw new Error("ssh fresh install requires curl on the remote host");
  }
  if (!preflight.systemdAvailable) {
    throw new Error("ssh fresh install requires systemd on the remote host");
  }
  if (!usesLocalArtifact && !preflight.sudoAvailable) {
    throw new Error("ssh fresh install requires sudo on the remote host");
  }
  if (!usesLocalArtifact && !preflight.passwordlessSudoAvailable && !install.sudoPasswordRef) {
    throw new Error("ssh fresh install requires passwordless sudo or a sudo password secret reference");
  }
  if (preflight.ctoxAvailable) {
    throw new Error("remote ctox binary already exists; use existing-instance upgrade");
  }
}

function normalizeLocalProfile(options = {}) {
  const ctoxBinary = resolveLocalCtoxBinary(options);
  const ctoxRoot = String(options.ctoxRoot || "").trim();
  if (!ctoxBinary) throw new Error("ctox binary is required");
  if (ctoxBinary.startsWith("-") || /[\0\r\n]/.test(ctoxBinary)) {
    throw new Error("ctox binary contains unsupported characters");
  }
  if (/[\0\r\n]/.test(ctoxRoot)) {
    throw new Error("ctox root contains unsupported characters");
  }
  return {
    ctoxBinary,
    ctoxRoot,
  };
}

function resolveLocalCtoxBinary(options = {}) {
  const explicit = String(options.ctoxBinary || "").trim();
  if (explicit) return explicit;
  const bundled = localCtoxBinaryCandidates(options)
    .find((candidate) => isExecutableFile(candidate));
  return bundled || "ctox";
}

function localCtoxBinaryCandidates(options = {}) {
  const platform = String(options.platform || process.platform || "").trim();
  const arch = String(options.arch || process.arch || "").trim();
  const executable = platform === "win32" ? "ctox.exe" : "ctox";
  const candidates = [];
  if (Array.isArray(options.bundledCtoxCandidates)) {
    candidates.push(...options.bundledCtoxCandidates);
  }
  const resourcesPath = String(options.resourcesPath || process.resourcesPath || "").trim();
  if (resourcesPath) {
    candidates.push(
      path.join(resourcesPath, LOCAL_CTOX_RESOURCE_DIR, executable),
      path.join(resourcesPath, LOCAL_CTOX_RESOURCE_DIR, platform, executable),
      path.join(resourcesPath, LOCAL_CTOX_RESOURCE_DIR, arch, executable),
      path.join(resourcesPath, LOCAL_CTOX_RESOURCE_DIR, `${platform}-${arch}`, executable),
    );
  }
  const appRoot = path.resolve(__dirname, "..", "..");
  candidates.push(
    path.join(appRoot, "resources", LOCAL_CTOX_RESOURCE_DIR, executable),
    path.join(appRoot, "resources", LOCAL_CTOX_RESOURCE_DIR, platform, executable),
    path.join(appRoot, "resources", LOCAL_CTOX_RESOURCE_DIR, arch, executable),
    path.join(appRoot, "resources", LOCAL_CTOX_RESOURCE_DIR, `${platform}-${arch}`, executable),
  );
  return [...new Set(candidates
    .map((candidate) => String(candidate || "").trim())
    .filter((candidate) => candidate && !/[\0\r\n]/.test(candidate)))];
}

function isExecutableFile(filePath) {
  try {
    const stat = fs.statSync(filePath);
    if (!stat.isFile()) return false;
    fs.accessSync(filePath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function normalizeLocalInstallOptions(options = {}) {
  const target = String(options.target || "").trim();
  if (!target) throw new Error("local install target is required");
  if (/[\0\r\n]/.test(target)) throw new Error("local install target contains unsupported characters");
  return {
    target,
    initGit: Boolean(options.initGit),
    noCopyEnv: Boolean(options.noCopyEnv),
    dryRun: Boolean(options.dryRun),
  };
}

function localDaemonInspectionError(error, profile) {
  if (error?.code === "ENOENT") {
    return {
      status: "missing_binary",
      ctoxBinary: profile.ctoxBinary,
      ctoxRoot: profile.ctoxRoot,
      dataPlane: "rxdb-webrtc",
      httpDataProxy: false,
      message: "ctox binary was not found",
    };
  }
  return {
    status: "error",
    ctoxBinary: profile.ctoxBinary,
    ctoxRoot: profile.ctoxRoot,
    dataPlane: "rxdb-webrtc",
    httpDataProxy: false,
    message: error instanceof Error ? error.message : String(error),
  };
}

function normalizeSshProfile(options = {}) {
  const host = String(options.host || "").trim();
  const user = String(options.user || "").trim();
  const port = Number(options.port || 22);
  if (!host) throw new Error("ssh host is required");
  if (!user) throw new Error("ssh user is required");
  if (host.startsWith("-") || /\s/.test(host) || /["'`;|&$<>\\]/.test(host)) {
    throw new Error("ssh host contains unsupported characters");
  }
  if (user.startsWith("-") || !/^[A-Za-z0-9._-]+$/.test(user)) {
    throw new Error("ssh user contains unsupported characters");
  }
  if (!Number.isInteger(port) || port <= 0 || port > 65535) {
    throw new Error("ssh port must be between 1 and 65535");
  }
  const profile = {
    host,
    user,
    port,
    installRoot: String(options.installRoot || "").trim(),
  };
  const sshPasswordRef = String(options.sshPasswordRef || "").trim();
  if (sshPasswordRef) {
    validateKeychainRef(sshPasswordRef, "ssh sshPasswordRef");
    profile.sshPasswordRef = sshPasswordRef;
  }
  return profile;
}

function normalizeSshInstallOptions(options = {}) {
  const releaseChannel = String(options.releaseChannel || "stable").trim();
  if (!["stable", "dev"].includes(releaseChannel)) {
    throw new Error("ssh install releaseChannel must be stable or dev");
  }
  const sudoPasswordRef = String(options.sudoPasswordRef || "").trim();
  if (sudoPasswordRef) validateKeychainRef(sudoPasswordRef, "ssh sudoPasswordRef");
  const apiProvider = normalizeOptionalSshInstallFlag(options.apiProvider, "ssh install apiProvider");
  const model = normalizeOptionalSshInstallFlag(options.model, "ssh install model");
  const backend = normalizeOptionalSshInstallFlag(options.backend, "ssh install backend");
  const localArtifactPath = normalizeLocalArtifactPath(options.localArtifactPath);
  if (localArtifactPath && (apiProvider || model || backend)) {
    throw new Error("ssh install apiProvider, model and backend are not supported with localArtifactPath");
  }
  const install = {
    releaseChannel,
    restartService: options.restartService !== false,
  };
  if (sudoPasswordRef) install.sudoPasswordRef = sudoPasswordRef;
  if (localArtifactPath) install.localArtifactPath = localArtifactPath;
  if (apiProvider) install.apiProvider = apiProvider;
  if (model) install.model = model;
  if (backend) install.backend = backend;
  return install;
}

function buildSshSudoPasswordRef(profile) {
  return `keychain://ctox-business-os-desktop/ssh-sudo/${stableId([
    profile.host,
    profile.user,
    String(profile.port || 22),
  ])}`;
}

function buildSshPasswordRef(profile) {
  return `keychain://ctox-business-os-desktop/ssh-login/${stableId([
    profile.host,
    profile.user,
    String(profile.port || 22),
  ])}`;
}

function buildSshArgs(profile, remoteCommand) {
  const passwordAuth = Boolean(profile.sshPasswordRef);
  const args = [
    "-o",
    passwordAuth ? "BatchMode=no" : "BatchMode=yes",
    "-o",
    "StrictHostKeyChecking=yes",
    "-o",
    passwordAuth ? "PasswordAuthentication=yes" : "PasswordAuthentication=no",
    "-o",
    passwordAuth ? "KbdInteractiveAuthentication=yes" : "KbdInteractiveAuthentication=no",
    "-o",
    "ConnectTimeout=10",
  ];
  if (passwordAuth) args.push("-o", "PreferredAuthentications=publickey,password,keyboard-interactive");
  if (profile.knownHostsPath) args.push("-o", `UserKnownHostsFile=${profile.knownHostsPath}`);
  args.push("-p", String(profile.port), "--", `${profile.user}@${profile.host}`, remoteCommand);
  return args;
}

function buildScpArgs(profile, localArtifactPath, remoteArtifactPath = ".cache/ctox/business-os-desktop/ctox-local-artifact") {
  const passwordAuth = Boolean(profile.sshPasswordRef);
  const args = [
    "-o",
    passwordAuth ? "BatchMode=no" : "BatchMode=yes",
    "-o",
    "StrictHostKeyChecking=yes",
    "-o",
    passwordAuth ? "PasswordAuthentication=yes" : "PasswordAuthentication=no",
    "-o",
    passwordAuth ? "KbdInteractiveAuthentication=yes" : "KbdInteractiveAuthentication=no",
    "-o",
    "ConnectTimeout=10",
  ];
  if (passwordAuth) args.push("-o", "PreferredAuthentications=publickey,password,keyboard-interactive");
  if (profile.knownHostsPath) args.push("-o", `UserKnownHostsFile=${profile.knownHostsPath}`);
  args.push("-P", String(profile.port), "--", localArtifactPath, `${profile.user}@${profile.host}:${remoteArtifactPath}`);
  return args;
}

async function runSshProgram(profile, program, args, options = {}, runner = runProcess) {
  const askpass = createSshPasswordAskpass(profile);
  try {
    return await runner(program, args, {
      ...options,
      ...(askpass ? { env: { ...(options.env || process.env), ...askpass.env } } : {}),
    });
  } finally {
    if (askpass) askpass.cleanup();
  }
}

function instanceFromPeerStatus(peerStatus, options) {
  if (!peerStatus || typeof peerStatus !== "object" || Array.isArray(peerStatus)) {
    throw new Error("peer status must be an object");
  }
  const syncRoom = String(peerStatus.sync_room || "").trim();
  const signalingUrls = Array.isArray(peerStatus.signaling_urls)
    ? peerStatus.signaling_urls.map((url) => String(url).trim()).filter(Boolean)
    : [];
  const roomPassword = String(peerStatus.signaling_room_password || "").trim();
  if (!syncRoom.startsWith("ctox-business-os:")) throw new Error("peer status sync_room must start with ctox-business-os:");
  if (signalingUrls.length === 0) throw new Error("peer status needs signaling_urls");
  if (!roomPassword) throw new Error("peer status needs signaling_room_password");
  const instanceId = String(peerStatus.instance_id || syncRoom.split(":")[1] || "").trim();
  const id = `${instanceIdPrefix(options.source)}:${stableId([options.source, instanceId, syncRoom])}`;
  const secretRef = `keychain://ctox-business-os-desktop/${id}/room`;
  const instance = normalizeInstance({
    id,
    source: options.source,
    displayName: options.displayName || instanceId || "CTOX",
    instanceId,
    status: "available",
    pairing: {
      syncRoom,
      signalingUrls,
      secretRef,
    },
    secretRefs: [secretRef],
    healthSummary: {
      dataPlane: "rxdb-webrtc",
      dataPlaneReady: Boolean(peerStatus.native_rxdb_peer_available !== false),
      httpDataProxy: false,
      nativePeerObserved: Boolean(peerStatus.native_rxdb_peer_available !== false),
    },
    connection: options.connection,
  });
  return {
    instance,
    secretMaterial: [{ ref: secretRef, value: roomPassword }],
  };
}

function instanceIdPrefix(source) {
  if (source === "local_daemon") return "local";
  if (source === "ssh_managed") return "ssh";
  return "paired";
}

function shellQuote(value) {
  return `'${String(value).replace(/'/g, "'\\''")}'`;
}

function normalizeLocalArtifactPath(value) {
  const artifactPath = String(value || "").trim();
  if (!artifactPath) return "";
  if (!path.isAbsolute(artifactPath)) {
    throw new Error("ssh localArtifactPath must be an absolute local path");
  }
  if (artifactPath.startsWith("-") || /[\0\r\n]/.test(artifactPath)) {
    throw new Error("ssh localArtifactPath contains unsupported characters");
  }
  return artifactPath;
}

function normalizeOptionalSshInstallFlag(value, label) {
  const normalized = String(value || "").trim();
  if (!normalized) return "";
  if (normalized.startsWith("-") || /[\0\r\n\t\s"'`;|&$<>\\]/.test(normalized)) {
    throw new Error(`${label} contains unsupported characters`);
  }
  return normalized;
}

function validateKeychainRef(value, label) {
  if (!String(value || "").startsWith("keychain://") || /[\0\r\n]/.test(String(value || ""))) {
    throw new Error(`${label} must be a keychain secret reference`);
  }
}

function createSshPasswordAskpass(profile, platform = process.platform) {
  if (!profile?.sshPasswordRef) return null;
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-ssh-askpass-"));
  const scriptPath = writeAskpassScript(tempDir, profile.sshPasswordRef, platform);
  return {
    env: {
      SSH_ASKPASS: scriptPath,
      SSH_ASKPASS_REQUIRE: "force",
      DISPLAY: process.env.DISPLAY || ":0",
    },
    cleanup: () => fs.rmSync(tempDir, { recursive: true, force: true }),
  };
}

function writeAskpassScript(tempDir, ref, platform) {
  if (platform === "win32") {
    const payloadPath = path.join(tempDir, "credential-payload.json");
    const scriptPath = path.join(tempDir, "askpass.ps1");
    const commandPath = path.join(tempDir, "askpass.cmd");
    fs.writeFileSync(payloadPath, JSON.stringify({
      action: "get",
      target: `CTOX Business OS Desktop:${ref}`,
    }));
    fs.writeFileSync(scriptPath, WINDOWS_CREDENTIAL_MANAGER_SCRIPT);
    fs.writeFileSync(commandPath, [
      "@echo off",
      `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "${scriptPath}" < "${payloadPath}"`,
      "",
    ].join("\r\n"));
    return commandPath;
  }
  const scriptPath = path.join(tempDir, "askpass.sh");
  const body = platform === "darwin"
    ? [
        "#!/bin/sh",
        "set -eu",
        `exec security find-generic-password -a ${shellQuote(ref)} -s ${shellQuote("CTOX Business OS Desktop")} -w`,
        "",
      ].join("\n")
    : [
        "#!/bin/sh",
        "set -eu",
        `exec secret-tool lookup application ${shellQuote("ctox-business-os-desktop")} ref ${shellQuote(ref)}`,
        "",
      ].join("\n");
  fs.writeFileSync(scriptPath, body, { mode: 0o700 });
  return scriptPath;
}

function remoteDirname(remotePath) {
  const value = String(remotePath || "").trim();
  const index = value.lastIndexOf("/");
  if (index <= 0) return ".";
  return value.slice(0, index);
}

async function resolveSudoPasswordInput(install, secretStore) {
  if (!install.sudoPasswordRef) return "";
  if (!secretStore?.get) throw new Error("secret store is required for sudo password refs");
  const secret = await secretStore.get(install.sudoPasswordRef);
  if (!secret) throw new Error("sudo password secret reference is empty or missing");
  return `${String(secret)}\n`;
}

function runProcess(program, args, options = {}) {
  return new Promise((resolve, reject) => {
    const timeout = Number.isFinite(options.timeout) ? options.timeout : 30000;
    const child = spawn(program, args, {
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: options.windowsHide !== false,
      env: options.env || process.env,
    });
    let stdout = "";
    let stderr = "";
    let timedOut = false;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill("SIGTERM");
    }, timeout);
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      if (timedOut) {
        reject(new Error(`${program} timed out after ${timeout}ms`));
        return;
      }
      if (code === 0) {
        resolve({ stdout, stderr });
        return;
      }
      const error = new Error(`${program} exited with code ${code}${stderr ? `: ${stderr.trim()}` : ""}`);
      error.code = code;
      error.stdout = stdout;
      error.stderr = stderr;
      reject(error);
    });
    if (Object.prototype.hasOwnProperty.call(options, "input")) {
      child.stdin.end(String(options.input));
    } else {
      child.stdin.end();
    }
  });
}

module.exports = {
  CtoxDevInstanceSource,
  PairingInviteInstanceSource,
  LocalDaemonInstanceSource,
  SshManagedInstanceSource,
  normalizeCtoxDevSessionPackage,
  instanceFromPeerStatus,
  normalizeLocalProfile,
  resolveLocalCtoxBinary,
  localCtoxBinaryCandidates,
  normalizeLocalInstallOptions,
  buildLocalPeerArgs,
  buildLocalInstallArgs,
  buildLocalCommandOptions,
  assertLocalCtoxRoot,
  buildSshPeerRemoteCommand,
  runLocalPeerStatusCommand,
  runLocalPeerEnsureCommand,
  runLocalBusinessOsInstallCommand,
  runSshPeerStatusCommand,
  runSshPeerEnsureCommand,
  runSshPreflightCommand,
  runSshExistingCtoxInstallCommand,
  runSshFreshCtoxInstallCommand,
  runSshLocalArtifactInstallCommand,
  runSshProgram,
  buildSshPreflightCommand,
  buildSshExistingCtoxInstallCommand,
  buildSshFreshCtoxInstallCommand,
  buildSshLocalArtifactPrepareCommand,
  buildSshLocalArtifactInstallCommand,
  parseSshPreflightOutput,
  normalizeSshPreflight,
  assertFreshSshInstallPreflight,
  normalizeSshProfile,
  normalizeSshInstallOptions,
  buildSshPasswordRef,
  buildSshSudoPasswordRef,
  buildSshArgs,
  buildScpArgs,
  createSshPasswordAskpass,
};
