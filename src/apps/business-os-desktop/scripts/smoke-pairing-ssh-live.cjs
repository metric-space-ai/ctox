"use strict";

const assert = require("node:assert/strict");
const { spawn } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { parseInvitePayload } = require("../src/common/invites.cjs");
const { createDefaultRegistry } = require("../src/main/registry.cjs");
const { ensureKnownHost, inspectSshHostKey, verifyTrustedHostKey } = require("../src/main/ssh-host-key.cjs");
const { buildSshArgs, PairingInviteInstanceSource } = require("../src/main/sources.cjs");

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const password = await readPassword(options);
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "ctox-desktop-pairing-ssh-live-"));
  const knownHostsPath = path.join(tempRoot, "known_hosts");
  const secretStore = new MemorySecretStore();
  let registry = createDefaultRegistry();
  const source = new PairingInviteInstanceSource(
    () => registry,
    (next) => {
      registry = next;
    },
    secretStore,
  );
  try {
    const profile = {
      host: options.host,
      user: options.user,
      port: options.port,
    };
    const hostKey = await inspectSshHostKey(profile);
    verifyTrustedHostKey(hostKey, options.trustedHostKeyFingerprint);
    ensureKnownHost({
      knownHostsPath,
      host: profile.host,
      port: profile.port,
      knownHostsLine: hostKey.knownHostsLine,
    });
    const remote = createRemoteRunner({
      profile: {
        ...profile,
        knownHostsPath,
        sshPasswordRef: "keychain://ctox-business-os-desktop/live-smoke/pairing-ssh-password",
      },
      password,
      tempRoot,
    });

    const initialStatus = await remote.json("ctox business-os peer status");
    const initialInviteResult = await readRemoteInvite({
      remote,
      status: initialStatus,
      displayName: options.displayName,
      ttlHours: options.ttlHours,
      allowPeerStatusInvite: options.allowPeerStatusInvite,
    });
    const instance = await source.importInvite(JSON.stringify(initialInviteResult.invite));
    const initialLaunch = await source.getLaunchConfig(instance.id);
    assertPairingLaunch(initialLaunch, initialInviteResult.invite);

    let rotated = null;
    if (options.rotate) {
      await remote.json("ctox business-os peer rotate");
      const rotatedStatus = await remote.json("ctox business-os peer status");
      const rotatedInviteResult = await readRemoteInvite({
        remote,
        status: rotatedStatus,
        displayName: options.displayName,
        ttlHours: options.ttlHours,
        allowPeerStatusInvite: options.allowPeerStatusInvite,
      });
      assert.notEqual(
        rotatedInviteResult.invite.signaling_room_password,
        initialInviteResult.invite.signaling_room_password,
        "remote peer rotate must change the room secret",
      );
      const rotatedInstance = await source.rotateInvite(instance.id, JSON.stringify(rotatedInviteResult.invite));
      assert.equal(rotatedInstance.id, instance.id, "rotated invite must keep the same desktop instance id");
      const rotatedLaunch = await source.getLaunchConfig(instance.id);
      assertPairingLaunch(rotatedLaunch, rotatedInviteResult.invite);
      rotated = {
        inviteSource: rotatedInviteResult.source,
        remoteInviteCliAvailable: rotatedInviteResult.remoteInviteCliAvailable,
        syncRoomChanged: rotatedInviteResult.invite.sync_room !== initialInviteResult.invite.sync_room,
        roomSecretChanged: true,
        activePeerSessionChanged: peerSessionId(rotatedStatus) !== peerSessionId(initialStatus),
        nativePeerAvailable: Boolean(rotatedStatus.native_rxdb_peer_available),
        nativePeerRunning: Boolean(rotatedStatus.native_rxdb_peer_status?.running),
        heartbeatFresh: Boolean(rotatedStatus.native_rxdb_peer_status?.heartbeat?.fresh),
        launchTransport: rotatedLaunch.ctoxConfig.transport,
        httpBridgeAvailable: rotatedLaunch.ctoxConfig.http_bridge_available,
      };
    }

    let revoked = false;
    if (options.revokeLocal) {
      await source.revokeInstance(instance.id);
      revoked = true;
      assert.equal(registry.instances.length, 0, "local pairing revoke must remove registry instance");
      assert.equal(await secretStore.get(instance.pairing.secretRef), "", "local pairing revoke must delete room secret");
    }

    const evidence = {
      ok: true,
      host: options.host,
      user: options.user,
      port: options.port,
      hostKeyFingerprint: hostKey.fingerprint,
      initial: {
        inviteSource: initialInviteResult.source,
        remoteInviteCliAvailable: initialInviteResult.remoteInviteCliAvailable,
        instanceId: instance.instanceId,
        displayName: instance.displayName,
        source: instance.source,
        sessionPartition: instance.sessionPartition,
        syncRoom: instance.pairing.syncRoom,
        signalingUrlCount: instance.pairing.signalingUrls.length,
        nativePeerAvailable: Boolean(initialStatus.native_rxdb_peer_available),
        nativePeerRunning: Boolean(initialStatus.native_rxdb_peer_status?.running),
        heartbeatFresh: Boolean(initialStatus.native_rxdb_peer_status?.heartbeat?.fresh),
        launchTransport: initialLaunch.ctoxConfig.transport,
        httpBridgeAvailable: initialLaunch.ctoxConfig.http_bridge_available,
      },
      rotated,
      revoked,
      registrySecretLeak: false,
      evidenceSecretLeak: false,
    };
    const evidenceText = JSON.stringify(evidence, null, 2);
    const allSecrets = [
      password,
      initialInviteResult.invite.signaling_room_password,
      ...(rotated ? [await secretStore.get(instance.pairing.secretRef)] : []),
    ].filter(Boolean);
    for (const secret of allSecrets) {
      assert.equal(evidenceText.includes(secret), false, "live smoke evidence leaked a secret");
      assert.equal(JSON.stringify(registry).includes(secret), false, "registry leaked a pairing secret");
    }
    console.log(evidenceText);
  } finally {
    if (!options.keepTemp) {
      fs.rmSync(tempRoot, { recursive: true, force: true });
    } else {
      console.error(`pairing ssh live smoke temp kept: ${tempRoot}`);
    }
  }
}

function parseArgs(args) {
  const options = {
    host: "",
    user: "",
    port: 22,
    trustedHostKeyFingerprint: "",
    passwordStdin: false,
    displayName: "CTOX Desktop Live Pairing",
    ttlHours: 1,
    rotate: false,
    revokeLocal: false,
    allowPeerStatusInvite: false,
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
    } else if (arg === "--password-stdin") {
      options.passwordStdin = true;
    } else if (arg === "--display-name") {
      options.displayName = String(args[index + 1] || "").trim();
      index += 1;
    } else if (arg === "--ttl-hours") {
      options.ttlHours = Number(args[index + 1]);
      index += 1;
    } else if (arg === "--rotate") {
      options.rotate = true;
    } else if (arg === "--revoke-local") {
      options.revokeLocal = true;
    } else if (arg === "--allow-peer-status-invite") {
      options.allowPeerStatusInvite = true;
    } else if (arg === "--keep-temp") {
      options.keepTemp = true;
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (!options.host) throw new Error("--host is required");
  if (!options.user) throw new Error("--user is required");
  if (!options.trustedHostKeyFingerprint) throw new Error("--trusted-host-key-fingerprint is required");
  if (!Number.isInteger(options.port) || options.port <= 0 || options.port > 65535) {
    throw new Error("--port must be between 1 and 65535");
  }
  if (!Number.isInteger(options.ttlHours) || options.ttlHours <= 0 || options.ttlHours > 168) {
    throw new Error("--ttl-hours must be between 1 and 168");
  }
  if (!options.displayName) throw new Error("--display-name must not be empty");
  return options;
}

async function readRemoteInvite({ remote, status, displayName, ttlHours, allowPeerStatusInvite }) {
  try {
    const invite = parseInvitePayload(await remote.json([
      "ctox business-os desktop invite",
      "--format json",
      `--display-name ${shellQuote(displayName)}`,
      `--ttl-hours ${String(ttlHours)}`,
    ].join(" ")));
    return {
      invite,
      source: "desktop-invite-cli",
      remoteInviteCliAvailable: true,
    };
  } catch (error) {
    if (!allowPeerStatusInvite || !isDesktopInviteUnavailable(error)) throw error;
    return {
      invite: parseInvitePayload(inviteFromPeerStatus(status, displayName, ttlHours)),
      source: "peer-status-derived",
      remoteInviteCliAvailable: false,
    };
  }
}

function inviteFromPeerStatus(status, displayName, ttlHours) {
  const expiresAt = new Date(Date.now() + ttlHours * 60 * 60 * 1000).toISOString();
  return {
    type: "ctox-business-os-invite",
    version: 1,
    display_name: displayName,
    instance_id: String(status.instance_id || "").trim(),
    sync_room: String(status.sync_room || "").trim(),
    signaling_urls: Array.isArray(status.signaling_urls) ? status.signaling_urls : [],
    signaling_room_password: String(status.signaling_room_password || "").trim(),
    transport: "webrtc",
    expires_at: expiresAt,
    data_plane: "rxdb-webrtc",
    http_bridge_available: false,
    secret_value_in_payload: true,
  };
}

function isDesktopInviteUnavailable(error) {
  const message = String(error?.message || "");
  return message.includes("unknown business-os command `desktop`")
    || message.includes("unknown business-os desktop command")
    || message.includes("unsupported desktop invite format");
}

function assertPairingLaunch(launch, invite) {
  assert.equal(launch.source, "pairing_invite");
  assert.equal(launch.ctoxConfig.transport, "webrtc");
  assert.equal(launch.ctoxConfig.sync_room, invite.sync_room);
  assert.deepEqual(launch.ctoxConfig.signaling_urls, invite.signaling_urls);
  assert.equal(launch.ctoxConfig.signaling_room_password, invite.signaling_room_password);
  assert.equal(launch.ctoxConfig.http_bridge_available, false);
}

function peerSessionId(status) {
  return String(status?.native_rxdb_peer_status?.heartbeat?.peer_session_id || "");
}

function createRemoteRunner({ profile, password, tempRoot }) {
  const { askpassPath } = ensureFileAskpass({ password, tempRoot });
  return {
    json: async (command) => {
      const { stdout } = await runProcess("ssh", buildSshArgs(profile, [
        "set -eu",
        "export PATH=\"$HOME/.local/bin:$HOME/.local/lib/ctox/current/bin:/usr/local/bin:$PATH\"",
        command,
      ].join("; ")), {
        timeout: 30000,
        env: fileAskpassEnv(askpassPath),
      });
      try {
        return JSON.parse(stdout);
      } catch {
        throw new Error("remote command did not return JSON");
      }
    },
  };
}

function ensureFileAskpass({ password, tempRoot }) {
  const passwordPath = path.join(tempRoot, "ssh-password");
  const askpassPath = path.join(tempRoot, "askpass.sh");
  fs.writeFileSync(passwordPath, password, { mode: 0o600 });
  fs.writeFileSync(askpassPath, [
    "#!/bin/sh",
    "set -eu",
    `cat ${shellQuote(passwordPath)}`,
    "",
  ].join("\n"), { mode: 0o700 });
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
      error.stderr = stderr;
      reject(error);
    });
  });
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

main().catch((error) => {
  console.error(error instanceof Error ? error.stack || error.message : String(error));
  process.exit(1);
});
