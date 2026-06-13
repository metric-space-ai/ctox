"use strict";

const assert = require("node:assert/strict");
const { spawn } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { createDefaultRegistry } = require("../src/main/registry.cjs");
const { createSecretStore } = require("../src/main/secret-store.cjs");
const { ensureKnownHost } = require("../src/main/ssh-host-key.cjs");
const {
  SshManagedInstanceSource,
  buildScpArgs,
  buildSshFreshCtoxInstallCommand,
  buildSshLocalArtifactInstallCommand,
  buildSshLocalArtifactPrepareCommand,
  buildSshPeerRemoteCommand,
  buildSshArgs,
  buildSshPreflightCommand,
  instanceFromPeerStatus,
  normalizeSshInstallOptions,
  normalizeSshPreflight,
  parseSshPreflightOutput,
} = require("../src/main/sources.cjs");

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const password = await readPassword(options);
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-desktop-ssh-password-live-"));
  const knownHostsPath = path.join(tempRoot, "known_hosts");
  let registry = createDefaultRegistry();
  const secretStore = options.fileAskpassFallback ? new MemorySecretStore() : createSecretStore();
  const source = new SshManagedInstanceSource(
    () => registry,
    (next) => {
      registry = next;
    },
    secretStore,
    { knownHostsPath },
  );
  let sshPasswordRef = "";
  const createdSecretRefs = new Set();
  try {
    const profile = {
      host: options.host,
      user: options.user,
      port: options.port,
    };
    const hostKey = await source.inspectHostKeyForProfile(profile);
    const trustedHostKeyFingerprint = options.trustedHostKeyFingerprint || (
      options.trustScannedHostKey ? hostKey.fingerprint : ""
    );
    if (!trustedHostKeyFingerprint) {
      throw new Error("pass --trusted-host-key-fingerprint <sha256:...> or --trust-scanned-host-key");
    }
    assert.equal(hostKey.fingerprint, trustedHostKeyFingerprint, "host key fingerprint mismatch");

    let preflight;
    let secretBackend = "platform-keychain";
    if (options.fileAskpassFallback) {
      secretBackend = "file-askpass-fallback";
      sshPasswordRef = "keychain://ctox-business-os-desktop/live-smoke/file-askpass-fallback";
      preflight = await runFileAskpassPreflight({
        profile,
        password,
        tempRoot,
        knownHostsPath,
        hostKey,
        sshPasswordRef,
      });
    } else {
      const stored = await source.storeSshPassword({
        ...profile,
        sshPassword: password,
      });
      sshPasswordRef = stored.sshPasswordRef;
      assert.ok(sshPasswordRef.startsWith("keychain://"), "ssh password ref must be a keychain ref");

      preflight = await source.preflight({
        ...profile,
        sshPasswordRef,
        trustedHostKeyFingerprint,
      });
    }
    assert.equal(preflight.host, options.host);
    assert.equal(preflight.user, options.user);
    assert.equal(preflight.port, options.port);
    assert.ok(preflight.shellAvailable, "remote shell must be available");
    assert.ok(fs.readFileSync(knownHostsPath, "utf8").includes(options.host), "known_hosts entry was not written");

    let attachEvidence = null;
    let installEvidence = null;
    if (options.attach || options.freshInstall) {
      const install = normalizeSshInstallOptions({
        releaseChannel: options.releaseChannel,
        restartService: options.restartService,
        localArtifactPath: options.localArtifactPath,
        apiProvider: options.installApiProvider,
        model: options.installModel,
        backend: options.installBackend,
      });
      const result = options.freshInstall
        ? await runFreshInstallFlow({
            source,
            registryProvider: () => registry,
            registrySaver: (next) => {
              registry = next;
            },
            profile,
            password,
            tempRoot,
            knownHostsPath,
            hostKey,
            sshPasswordRef,
            trustedHostKeyFingerprint,
            displayName: options.displayName || `${options.user}@${options.host}`,
            install,
            useFileAskpassFallback: options.fileAskpassFallback,
          })
        : {
            instance: options.fileAskpassFallback
              ? await runFileAskpassAttach({
            source,
            registryProvider: () => registry,
            registrySaver: (next) => {
              registry = next;
            },
            profile,
            password,
            tempRoot,
            knownHostsPath,
            hostKey,
            sshPasswordRef,
            displayName: options.displayName || `${options.user}@${options.host}`,
          })
              : await source.attachExisting({
            ...profile,
            sshPasswordRef,
            trustedHostKeyFingerprint,
            displayName: options.displayName || `${options.user}@${options.host}`,
          }),
            install: null,
          };
      const { instance } = result;
      for (const ref of instance.secretRefs || []) createdSecretRefs.add(ref);
      const launch = await source.getLaunchConfig(instance.id);
      assert.equal(instance.source, "ssh_managed");
      assert.equal(launch.source, "ssh_managed");
      assert.equal(launch.ctoxConfig.transport, "webrtc");
      assert.equal(launch.ctoxConfig.http_bridge_available, false);
      assert.ok(launch.ctoxConfig.sync_room.startsWith("ctox-business-os:"), "launch sync_room must be CTOX Business OS");
      assert.ok(Array.isArray(launch.ctoxConfig.signaling_urls), "launch signaling_urls must be an array");
      assert.ok(launch.ctoxConfig.signaling_urls.length > 0, "launch needs at least one signaling URL");
      assert.ok(launch.ctoxConfig.signaling_room_password, "launch needs a room password from SecretStore");
      assert.equal(
        JSON.stringify(registry).includes(launch.ctoxConfig.signaling_room_password),
        false,
        "registry leaked signaling room password",
      );
      attachEvidence = {
        instanceId: instance.id,
        displayName: instance.displayName,
        source: instance.source,
        sessionPartition: instance.sessionPartition,
        syncRoom: launch.ctoxConfig.sync_room,
        signalingUrlCount: launch.ctoxConfig.signaling_urls.length,
        transport: launch.ctoxConfig.transport,
        httpBridgeAvailable: launch.ctoxConfig.http_bridge_available,
        registrySecretLeak: false,
      };
      if (result.install) {
        installEvidence = {
          mode: result.install.mode,
          artifact: result.install.artifact || "official-installer",
          releaseChannel: result.install.releaseChannel,
          restartService: result.install.restartService,
          apiProvider: result.install.apiProvider || "",
          model: result.install.model || "",
          backend: result.install.backend || "",
          stdoutBytes: Buffer.byteLength(String(result.install.stdout || "")),
          stderrBytes: Buffer.byteLength(String(result.install.stderr || "")),
        };
      }
    }

    const evidence = {
      ok: true,
      host: options.host,
      user: options.user,
      port: options.port,
      hostKeyFingerprint: hostKey.fingerprint,
      secretBackend,
      sshPasswordRef,
      preflight: {
        os: preflight.os,
        shellAvailable: preflight.shellAvailable,
        bashAvailable: preflight.bashAvailable,
        curlAvailable: preflight.curlAvailable,
        systemdAvailable: preflight.systemdAvailable,
        sudoAvailable: preflight.sudoAvailable,
        passwordlessSudoAvailable: preflight.passwordlessSudoAvailable,
        ctoxAvailable: preflight.ctoxAvailable,
        ctoxStatusOk: preflight.ctoxStatusOk,
      },
      install: installEvidence,
      attach: attachEvidence,
    };
    const evidenceText = JSON.stringify(evidence, null, 2);
    assert.equal(evidenceText.includes(password), false, "live smoke evidence leaked password");
    if (options.attach) {
      for (const ref of createdSecretRefs) {
        const secret = await secretStore.get(ref);
        assert.ok(secret, `expected room secret for ${ref}`);
        assert.equal(evidenceText.includes(secret), false, "live smoke evidence leaked room secret");
      }
    }
    console.log(evidenceText);
  } finally {
    if (secretStore) {
      await Promise.all(Array.from(createdSecretRefs, (ref) => secretStore.delete(ref).catch(() => undefined)));
    }
    if (sshPasswordRef && secretStore) {
      await secretStore.delete(sshPasswordRef).catch(() => undefined);
    }
    if (!options.keepTemp) {
      fs.rmSync(tempRoot, { recursive: true, force: true });
    } else {
      console.error(`ssh password live smoke temp kept: ${tempRoot}`);
    }
  }
}

function parseArgs(args) {
  const options = {
    host: "",
    user: "",
    port: 22,
    trustedHostKeyFingerprint: "",
    trustScannedHostKey: false,
    fileAskpassFallback: false,
    passwordStdin: false,
    attach: false,
    freshInstall: false,
    localArtifactPath: "",
    installApiProvider: "",
    installModel: "",
    installBackend: "",
    releaseChannel: "stable",
    restartService: true,
    displayName: "",
    keepTemp: false,
  };
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--host") {
      options.host = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--user") {
      options.user = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--port") {
      options.port = Number(args[index + 1]);
      index += 1;
    } else if (arg === "--trusted-host-key-fingerprint") {
      options.trustedHostKeyFingerprint = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--trust-scanned-host-key") {
      options.trustScannedHostKey = true;
    } else if (arg === "--file-askpass-fallback") {
      options.fileAskpassFallback = true;
    } else if (arg === "--password-stdin") {
      options.passwordStdin = true;
    } else if (arg === "--attach") {
      options.attach = true;
    } else if (arg === "--fresh-install") {
      options.freshInstall = true;
    } else if (arg === "--local-artifact-path") {
      options.localArtifactPath = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--install-api-provider") {
      options.installApiProvider = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--install-model") {
      options.installModel = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--install-backend") {
      options.installBackend = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--release-channel") {
      options.releaseChannel = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--no-restart-service") {
      options.restartService = false;
    } else if (arg === "--display-name") {
      options.displayName = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--keep-temp") {
      options.keepTemp = true;
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (!options.host) throw new Error("--host is required");
  if (!options.user) throw new Error("--user is required");
  if (!Number.isInteger(options.port) || options.port <= 0 || options.port > 65535) {
    throw new Error("--port must be between 1 and 65535");
  }
  if (options.localArtifactPath && !options.freshInstall) {
    throw new Error("--local-artifact-path requires --fresh-install");
  }
  if ((options.installApiProvider || options.installModel || options.installBackend) && !options.freshInstall) {
    throw new Error("--install-api-provider, --install-model and --install-backend require --fresh-install");
  }
  return options;
}

async function runFileAskpassPreflight({ profile, password, tempRoot, knownHostsPath, hostKey, sshPasswordRef }) {
  const passwordPath = path.join(tempRoot, "ssh-password");
  const askpassPath = path.join(tempRoot, "askpass.sh");
  fs.writeFileSync(passwordPath, password, { mode: 0o600 });
  fs.writeFileSync(askpassPath, [
    "#!/bin/sh",
    "set -eu",
    `cat ${shellQuote(passwordPath)}`,
    "",
  ].join("\n"), { mode: 0o700 });
  ensureKnownHost({
    knownHostsPath,
    host: profile.host,
    port: profile.port,
    knownHostsLine: hostKey.knownHostsLine,
  });
  const commandProfile = {
    ...profile,
    knownHostsPath,
    sshPasswordRef,
  };
  const { stdout } = await runProcess("ssh", buildSshArgs(commandProfile, buildSshPreflightCommand(commandProfile)), {
    timeout: 30000,
    env: {
      ...process.env,
      SSH_ASKPASS: askpassPath,
      SSH_ASKPASS_REQUIRE: "force",
      DISPLAY: process.env.DISPLAY || ":0",
    },
  });
  return normalizeSshPreflight(parseSshPreflightOutput(stdout), commandProfile);
}

async function runFreshInstallFlow({
  source,
  registryProvider,
  registrySaver,
  profile,
  password,
  tempRoot,
  knownHostsPath,
  hostKey,
  sshPasswordRef,
  trustedHostKeyFingerprint,
  displayName,
  install,
  useFileAskpassFallback,
}) {
  if (!useFileAskpassFallback) {
    const result = await source.installFresh({
      ...profile,
      sshPasswordRef,
      trustedHostKeyFingerprint,
      displayName,
      releaseChannel: install.releaseChannel,
      restartService: install.restartService,
      ...(install.apiProvider ? { apiProvider: install.apiProvider } : {}),
      ...(install.model ? { model: install.model } : {}),
      ...(install.backend ? { backend: install.backend } : {}),
      ...(install.localArtifactPath ? { localArtifactPath: install.localArtifactPath } : {}),
    });
    return {
      instance: result.instance,
      install: result.install,
    };
  }
  const installResult = await runFileAskpassFreshInstallCommand({
    profile,
    password,
    tempRoot,
    knownHostsPath,
    hostKey,
    sshPasswordRef,
    install,
  });
  const instance = await runFileAskpassAttach({
    source,
    registryProvider,
    registrySaver,
    profile,
    password,
    tempRoot,
    knownHostsPath,
    hostKey,
    sshPasswordRef,
    displayName,
  });
  return {
    instance,
    install: installResult,
  };
}

async function runFileAskpassFreshInstallCommand({
  profile,
  password,
  tempRoot,
  knownHostsPath,
  hostKey,
  sshPasswordRef,
  install,
}) {
  const { askpassPath } = ensureFileAskpass({ password, tempRoot });
  ensureKnownHost({
    knownHostsPath,
    host: profile.host,
    port: profile.port,
    knownHostsLine: hostKey.knownHostsLine,
  });
  const commandProfile = {
    ...profile,
    knownHostsPath,
    sshPasswordRef,
  };
  const env = fileAskpassEnv(askpassPath);
  if (install.localArtifactPath) {
    const remoteArtifactPath = ".cache/ctox/business-os-desktop/ctox-local-artifact";
    await runProcess("ssh", buildSshArgs(commandProfile, buildSshLocalArtifactPrepareCommand(remoteArtifactPath)), {
      timeout: 30000,
      env,
    });
    await runProcess("scp", buildScpArgs(commandProfile, install.localArtifactPath, remoteArtifactPath), {
      timeout: 300000,
      env,
    });
    const { stdout, stderr } = await runProcess(
      "ssh",
      buildSshArgs(commandProfile, buildSshLocalArtifactInstallCommand(commandProfile, install, remoteArtifactPath)),
      { timeout: 300000, env },
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
  const { stdout, stderr } = await runProcess("ssh", buildSshArgs(commandProfile, buildSshFreshCtoxInstallCommand(commandProfile, install)), {
    timeout: 900000,
    env,
  });
  return {
    ok: true,
    mode: "fresh",
    artifact: "official-installer",
    releaseChannel: install.releaseChannel,
    restartService: install.restartService,
    apiProvider: install.apiProvider || "",
    model: install.model || "",
    backend: install.backend || "",
    stdout: stdout.trim(),
    stderr: stderr.trim(),
  };
}

async function runFileAskpassAttach({
  source,
  registryProvider,
  registrySaver,
  profile,
  password,
  tempRoot,
  knownHostsPath,
  hostKey,
  sshPasswordRef,
  displayName,
}) {
  const { askpassPath } = ensureFileAskpass({ password, tempRoot });
  ensureKnownHost({
    knownHostsPath,
    host: profile.host,
    port: profile.port,
    knownHostsLine: hostKey.knownHostsLine,
  });
  const commandProfile = {
    ...profile,
    knownHostsPath,
    sshPasswordRef,
  };
  const { stdout } = await runProcess("ssh", buildSshArgs(commandProfile, buildSshPeerRemoteCommand(commandProfile, "ensure")), {
    timeout: 30000,
    env: fileAskpassEnv(askpassPath),
  });
  const { instance, secretMaterial } = instanceFromPeerStatus(JSON.parse(stdout), {
    source: "ssh_managed",
    displayName,
    connection: {
      host: profile.host,
      user: profile.user,
      port: profile.port,
      hostKeyFingerprint: hostKey.fingerprint,
      managedBy: "ssh",
    },
  });
  for (const secret of secretMaterial) {
    await source.secretStore.set(secret.ref, secret.value);
  }
  registrySaver({
    ...registryProvider(),
    instances: [
      ...registryProvider().instances.filter((entry) => entry.id !== instance.id),
      instance,
    ],
  });
  return instance;
}

function ensureFileAskpass({ password, tempRoot }) {
  const passwordPath = path.join(tempRoot, "ssh-password");
  const askpassPath = path.join(tempRoot, "askpass.sh");
  if (!fs.existsSync(passwordPath)) {
    fs.writeFileSync(passwordPath, password, { mode: 0o600 });
  }
  if (!fs.existsSync(askpassPath)) {
    fs.writeFileSync(askpassPath, [
      "#!/bin/sh",
      "set -eu",
      `cat ${shellQuote(passwordPath)}`,
      "",
    ].join("\n"), { mode: 0o700 });
  }
  return { passwordPath, askpassPath };
}

function fileAskpassEnv(askpassPath) {
  return {
    ...process.env,
    SSH_ASKPASS: askpassPath,
    SSH_ASKPASS_REQUIRE: "force",
    DISPLAY: process.env.DISPLAY || ":0",
  };
}

function runProcess(program, args, options = {}) {
  return new Promise((resolve, reject) => {
    const timeout = Number.isFinite(options.timeout) ? options.timeout : 30000;
    let stdout = "";
    let stderr = "";
    let timedOut = false;
    const child = spawn(program, args, {
      env: options.env || process.env,
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    });
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
  });
}

class MemorySecretStore {
  constructor() {
    this.values = new Map();
  }

  async set(ref, value) {
    this.values.set(ref, value);
  }

  async get(ref) {
    return this.values.get(ref) || "";
  }

  async delete(ref) {
    this.values.delete(ref);
  }
}

function shellQuote(value) {
  return `'${String(value).replace(/'/g, "'\\''")}'`;
}

function readPassword(options) {
  if (!options.passwordStdin) {
    throw new Error("--password-stdin is required; never pass SSH passwords as command arguments");
  }
  return new Promise((resolve, reject) => {
    let input = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => {
      input += chunk;
    });
    process.stdin.on("end", () => {
      const password = input.replace(/\r?\n$/, "");
      if (!password) {
        reject(new Error("password stdin was empty"));
        return;
      }
      resolve(password);
    });
    process.stdin.on("error", reject);
  });
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack || error.message : String(error));
  process.exit(1);
});
