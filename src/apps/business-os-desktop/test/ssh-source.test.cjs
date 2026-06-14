"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { createDefaultRegistry } = require("../src/main/registry.cjs");
const {
  SshManagedInstanceSource,
  assertFreshSshInstallPreflight,
  buildScpArgs,
  buildSshArgs,
  buildSshExistingCtoxInstallCommand,
  buildSshFreshCtoxInstallCommand,
  buildSshLocalArtifactInstallCommand,
  buildSshLocalArtifactPrepareCommand,
  buildSshPasswordRef,
  buildSshPeerRemoteCommand,
  buildSshSudoPasswordRef,
  createSshPasswordAskpass,
  buildSshPreflightCommand,
  normalizeSshInstallOptions,
  normalizeSshPreflight,
  normalizeSshProfile,
  parseSshPreflightOutput,
  runSshFreshCtoxInstallCommand,
  runSshLocalArtifactInstallCommand,
  runSshProgram,
} = require("../src/main/sources.cjs");

test("ssh profile validation requires host, user and valid port", () => {
  assert.throws(() => normalizeSshProfile({ user: "ubuntu" }), /host/);
  assert.throws(() => normalizeSshProfile({ host: "example.com" }), /user/);
  assert.throws(() => normalizeSshProfile({ host: "example.com", user: "ubuntu", port: 70000 }), /port/);
  assert.deepEqual(normalizeSshProfile({ host: "example.com", user: "ubuntu", port: "2222" }), {
    host: "example.com",
    user: "ubuntu",
    port: 2222,
    installRoot: "",
  });
});

test("ssh args are key/agent based and fail closed on host keys", () => {
  const args = buildSshArgs({
    host: "example.com",
    user: "ubuntu",
    port: 2222,
  }, "ctox business-os peer status");
  assertSshOption(args, "BatchMode=yes");
  assertSshOption(args, "StrictHostKeyChecking=yes");
  assertSshOption(args, "PasswordAuthentication=no");
  assertSshOption(args, "KbdInteractiveAuthentication=no");
  assertSshOption(args, "ConnectTimeout=10");
  assert.equal(args.includes("sshpass"), false);
  assert.equal(args.includes("-p"), true);
  assert.equal(args.includes("2222"), true);
  assert.equal(args.includes("--"), true);
  assert.equal(args.includes("ubuntu@example.com"), true);
});

test("ssh password ref enables OpenSSH askpass auth without password args", () => {
  const profile = normalizeSshProfile({
    host: "example.com",
    user: "ubuntu",
    port: 2222,
    sshPasswordRef: "keychain://ctox-business-os-desktop/ssh-login/example",
  });
  const sshArgs = buildSshArgs(profile, "ctox status");
  assertSshOption(sshArgs, "BatchMode=no");
  assertSshOption(sshArgs, "PasswordAuthentication=yes");
  assertSshOption(sshArgs, "KbdInteractiveAuthentication=yes");
  assertSshOption(sshArgs, "PreferredAuthentications=publickey,password,keyboard-interactive");
  assert.equal(JSON.stringify(sshArgs).includes("login-secret"), false);

  const scpArgs = buildScpArgs(profile, "/tmp/ctox");
  assertSshOption(scpArgs, "BatchMode=no");
  assertSshOption(scpArgs, "PasswordAuthentication=yes");
  assert.equal(JSON.stringify(scpArgs).includes("login-secret"), false);
  assert.throws(
    () => normalizeSshProfile({ host: "example.com", user: "ubuntu", sshPasswordRef: "plain-secret" }),
    /keychain/,
  );
});

test("ssh askpass script reads from platform keychain by ref only", () => {
  const ref = "keychain://ctox-business-os-desktop/ssh-login/example";
  const askpass = createSshPasswordAskpass({ sshPasswordRef: ref }, "darwin");
  const script = fs.readFileSync(askpass.env.SSH_ASKPASS, "utf8");
  assert.match(script, /security find-generic-password/);
  assert.match(script, new RegExp(escapeRegExp(ref)));
  assert.equal(script.includes("login-secret"), false);
  const dir = path.dirname(askpass.env.SSH_ASKPASS);
  askpass.cleanup();
  assert.equal(fs.existsSync(dir), false);
});

test("ssh program runner injects askpass env and cleans it up", async () => {
  let askpassPath = "";
  await runSshProgram(
    { sshPasswordRef: "keychain://ctox-business-os-desktop/ssh-login/example" },
    "ssh",
    ["example"],
    { timeout: 1000 },
    async (_program, _args, options) => {
      askpassPath = options.env.SSH_ASKPASS;
      assert.equal(options.env.SSH_ASKPASS_REQUIRE, "force");
      assert.ok(options.env.DISPLAY);
      return { stdout: "", stderr: "" };
    },
  );
  assert.equal(fs.existsSync(path.dirname(askpassPath)), false);
});

test("ssh preflight command checks os, systemd, sudo and ctox without password args", () => {
  const command = buildSshPreflightCommand({ installRoot: "~/.local/lib/ctox/current" });
  assert.match(command, /uname -s/);
  assert.match(command, /command -v bash/);
  assert.match(command, /command -v curl/);
  assert.match(command, /command -v systemctl/);
  assert.match(command, /command -v sudo/);
  assert.match(command, /sudo -n true/);
  assert.match(command, /command -v ctox/);
  assert.equal(command.includes("sshpass"), false);
  assert.equal(command.includes("sudo -S"), false);
});

test("ssh peer ensure command returns full status shape", () => {
  const command = buildSshPeerRemoteCommand({ installRoot: "~/.local/lib/ctox/current" }, "ensure");
  assert.match(command, /ctox business-os peer ensure >\/dev\/null; ctox business-os peer status/);
  assert.match(command, /cd '~\/\.local\/lib\/ctox\/current'/);
  const status = buildSshPeerRemoteCommand({}, "status");
  assert.match(status, /ctox business-os peer status/);
  assert.doesNotMatch(status, /peer ensure/);
});

test("ssh install options and existing-ctox upgrade command are bounded", () => {
  assert.deepEqual(normalizeSshInstallOptions({}), {
    releaseChannel: "stable",
    restartService: true,
  });
  assert.deepEqual(normalizeSshInstallOptions({ releaseChannel: "dev", restartService: false }), {
    releaseChannel: "dev",
    restartService: false,
  });
  assert.throws(() => normalizeSshInstallOptions({ releaseChannel: "nightly" }), /releaseChannel/);
  assert.throws(() => normalizeSshInstallOptions({ localArtifactPath: "target/release/ctox" }), /absolute/);
  assert.equal(
    normalizeSshInstallOptions({ localArtifactPath: "/tmp/ctox" }).localArtifactPath,
    "/tmp/ctox",
  );
  assert.throws(() => normalizeSshInstallOptions({ sudoPasswordRef: "plain-secret" }), /keychain/);
  assert.equal(
    normalizeSshInstallOptions({ sudoPasswordRef: "keychain://ctox-business-os-desktop/ssh/sudo" }).sudoPasswordRef,
    "keychain://ctox-business-os-desktop/ssh/sudo",
  );

  const command = buildSshExistingCtoxInstallCommand(
    { installRoot: "~/.local/lib/ctox/current" },
    { releaseChannel: "dev", restartService: true },
  );
  assert.match(command, /ctox upgrade --dev/);
  assert.match(command, /ctox start/);
  assert.match(command, /ctox status/);
  assert.equal(command.includes("sshpass"), false);
  assert.equal(command.includes("sudo -S"), false);
});

test("ssh local artifact install commands copy only through scp and user-local bin", () => {
  const prepare = buildSshLocalArtifactPrepareCommand(".cache/ctox/business-os-desktop/ctox-local-artifact");
  assert.match(prepare, /mkdir -p/);
  assert.match(prepare, /\.cache\/ctox\/business-os-desktop/);

  const install = buildSshLocalArtifactInstallCommand(
    { installRoot: "~/.local/lib/ctox/current" },
    { releaseChannel: "dev", restartService: true, localArtifactPath: "/tmp/ctox" },
    ".cache/ctox/business-os-desktop/ctox-local-artifact",
  );
  assert.match(install, /ctox desktop ssh artifact install requires Linux/);
  assert.match(install, /cp '\.cache\/ctox\/business-os-desktop\/ctox-local-artifact' "\$HOME\/\.local\/bin\/ctox"/);
  assert.match(install, /ctox start/);
  assert.match(install, /ctox status/);
  assert.doesNotMatch(install, /curl -fsSL/);
  assert.doesNotMatch(install, /install\.sh/);
  assert.doesNotMatch(install, /sudo -S/);
  assert.doesNotMatch(install, /sshpass/);
});

test("scp args upload local artifact with strict host-key checking", () => {
  const args = buildScpArgs({
    host: "example.com",
    user: "ubuntu",
    port: 2222,
    knownHostsPath: "/tmp/known_hosts",
  }, "/tmp/ctox");
  assertSshOption(args, "BatchMode=yes");
  assertSshOption(args, "StrictHostKeyChecking=yes");
  assertSshOption(args, "PasswordAuthentication=no");
  assert.equal(args.includes("-P"), true);
  assert.equal(args.includes("2222"), true);
  assert.equal(args.includes("/tmp/ctox"), true);
  assert.equal(args.at(-1), "ubuntu@example.com:.cache/ctox/business-os-desktop/ctox-local-artifact");
});

test("ssh stable fresh install command uses verified release bundle and no password args", () => {
  const command = buildSshFreshCtoxInstallCommand(
    { installRoot: "~/.local/lib/ctox/current" },
    { releaseChannel: "stable", restartService: true },
  );
  assert.match(command, /github\.com\/metric-space-ai\/ctox\/releases\/latest\/download/);
  assert.match(command, /curl -fsSL/);
  assert.match(command, /sha256sum -c/);
  assert.match(command, /tar -xzf/);
  assert.match(command, /target\/release\/ctox/);
  assert.match(command, /sudo -n true/);
  assert.match(command, /ctox start/);
  assert.match(command, /ctox status/);
  assert.doesNotMatch(command, /raw\.githubusercontent\.com\/metric-space-ai\/ctox\/main\/install\.sh/);
  assert.doesNotMatch(command, /ctox upgrade --stable/);
  assert.equal(command.includes("sshpass"), false);
  assert.equal(command.includes("sudo -S"), false);
});

test("ssh fresh install command forwards official installer options as CLI args", () => {
  const install = normalizeSshInstallOptions({
    releaseChannel: "stable",
    restartService: false,
    apiProvider: "openai",
    model: "Qwen/Qwen3.6-27B",
    backend: "cpu",
  });
  const command = buildSshFreshCtoxInstallCommand(
    { installRoot: "~/.local/lib/ctox/current" },
    install,
  );
  assert.equal(install.apiProvider, "openai");
  assert.match(command, /\| bash -s -- '--api-provider' 'openai' '--model' 'Qwen\/Qwen3\.6-27B' '--backend' 'cpu'/);
  assert.doesNotMatch(command, /CTOX_API_PROVIDER/);
  assert.doesNotMatch(command, /ctox start/);
  assert.throws(
    () => normalizeSshInstallOptions({ apiProvider: "openai\nmalicious" }),
    /unsupported characters/,
  );
  assert.throws(
    () => normalizeSshInstallOptions({ localArtifactPath: "/tmp/ctox", apiProvider: "openai" }),
    /official installer/,
  );
});

test("ssh fresh install command supports sudo askpass through stdin secret refs", () => {
  const command = buildSshFreshCtoxInstallCommand(
    { installRoot: "~/.local/lib/ctox/current" },
    {
      releaseChannel: "stable",
      restartService: true,
      sudoPasswordRef: "keychain://ctox-business-os-desktop/ssh/sudo",
    },
  );
  assert.match(command, /SUDO_ASKPASS="\$ASKPASS" sudo -A -v/);
  assert.match(command, /github\.com\/metric-space-ai\/ctox\/releases\/latest\/download/);
  assert.match(command, /IFS= read -r CTOX_SUDO_PASSWORD/);
  assert.match(command, /PASS_FIFO/);
  assert.doesNotMatch(command, /sudo -n true/);
  assert.equal(command.includes("sudo -S"), false);
  assert.equal(command.includes("sshpass"), false);
  assert.equal(command.includes("secret-1"), false);
});

test("ssh preflight output normalizes remote capabilities", () => {
  const raw = parseSshPreflightOutput([
    "os_name=Linux",
    "os_arch=x86_64",
    "shell_path=/usr/bin/sh",
    "bash_path=/usr/bin/bash",
    "curl_path=/usr/bin/curl",
    "systemctl_path=/usr/bin/systemctl",
    "sudo_path=/usr/bin/sudo",
    "sudo_nopasswd=false",
    "ctox_path=/usr/local/bin/ctox",
    "install_root=/home/ubuntu/.local/lib/ctox/current",
    "install_root_exists=true",
  ].join("\n"));
  const normalized = normalizeSshPreflight(raw, {
    host: "example.com",
    user: "ubuntu",
    port: 2222,
    installRoot: "/home/ubuntu/.local/lib/ctox/current",
  });
  assert.equal(normalized.os.name, "Linux");
  assert.equal(normalized.os.arch, "x86_64");
  assert.equal(normalized.bashAvailable, true);
  assert.equal(normalized.curlAvailable, true);
  assert.equal(normalized.systemdAvailable, true);
  assert.equal(normalized.sudoAvailable, true);
  assert.equal(normalized.passwordlessSudoAvailable, false);
  assert.equal(normalized.needsSudoPassword, true);
  assert.equal(normalized.ctoxAvailable, true);
  assert.equal(normalized.installRootExists, true);
  assert.equal(normalized.httpDataProxy, false);
});

test("fresh ssh install preflight fails closed without passwordless sudo", () => {
  assert.throws(
    () => assertFreshSshInstallPreflight({
      os: { name: "Linux" },
      shellAvailable: true,
      bashAvailable: true,
      curlAvailable: true,
      systemdAvailable: true,
      sudoAvailable: true,
      passwordlessSudoAvailable: false,
      ctoxAvailable: false,
    }),
    /passwordless sudo/,
  );
});

test("fresh ssh install preflight accepts sudo password secret ref", () => {
  assert.doesNotThrow(() => assertFreshSshInstallPreflight({
    os: { name: "Linux" },
    shellAvailable: true,
    bashAvailable: true,
    curlAvailable: true,
    systemdAvailable: true,
    sudoAvailable: true,
    passwordlessSudoAvailable: false,
    ctoxAvailable: false,
  }, {
    sudoPasswordRef: "keychain://ctox-business-os-desktop/ssh/sudo",
  }));
});

test("fresh ssh install preflight accepts local artifact without curl or sudo", () => {
  assert.doesNotThrow(() => assertFreshSshInstallPreflight({
    os: { name: "Linux" },
    shellAvailable: true,
    bashAvailable: true,
    curlAvailable: false,
    systemdAvailable: true,
    sudoAvailable: false,
    passwordlessSudoAvailable: false,
    ctoxAvailable: false,
  }, {
    localArtifactPath: "/tmp/ctox",
  }));
});

test("ssh fresh install runner pipes sudo secret through stdin only", async () => {
  let captured;
  const result = await runSshFreshCtoxInstallCommand(
    {
      host: "fresh.example.com",
      user: "ubuntu",
      port: 22,
      installRoot: "",
    },
    {
      releaseChannel: "stable",
      restartService: false,
      sudoPasswordRef: "keychain://ctox-business-os-desktop/ssh/sudo",
    },
    {
      get: async (ref) => {
        assert.equal(ref, "keychain://ctox-business-os-desktop/ssh/sudo");
        return "sudo-secret";
      },
    },
    async (program, args, options) => {
      captured = { program, args, options };
      return { stdout: "installed\n", stderr: "" };
    },
  );
  assert.equal(result.ok, true);
  assert.equal(result.artifact, "release");
  assert.equal(captured.program, "ssh");
  assert.equal(captured.options.input, "sudo-secret\n");
  assert.equal(JSON.stringify(captured.args).includes("sudo-secret"), false);
  assert.equal(JSON.stringify(captured.args).includes("sudo -S"), false);
});

test("ssh local artifact install runner prepares, uploads and installs artifact", async () => {
  const calls = [];
  const result = await runSshLocalArtifactInstallCommand(
    {
      host: "fresh.example.com",
      user: "ubuntu",
      port: 2222,
      installRoot: "",
    },
    {
      releaseChannel: "stable",
      restartService: false,
      localArtifactPath: "/tmp/ctox",
    },
    async (program, args, options) => {
      calls.push({ program, args, options });
      return { stdout: program === "ssh" ? "ok\n" : "", stderr: "" };
    },
  );
  assert.equal(result.ok, true);
  assert.equal(result.artifact, "local");
  assert.deepEqual(calls.map((call) => call.program), ["ssh", "scp", "ssh"]);
  assert.equal(calls[1].args.includes("/tmp/ctox"), true);
  assert.equal(JSON.stringify(calls).includes("curl -fsSL"), false);
  assert.equal(JSON.stringify(calls).includes("install.sh"), false);
});

test("ssh sudo password storage writes only to secret store ref", async () => {
  let registry = createDefaultRegistry();
  const secrets = new Map();
  const source = new SshManagedInstanceSource(
    () => registry,
    (next) => {
      registry = next;
    },
    {
      get: async (ref) => secrets.get(ref) || "",
      set: async (ref, value) => secrets.set(ref, value),
    },
  );
  const expectedRef = buildSshSudoPasswordRef({
    host: "fresh.example.com",
    user: "ubuntu",
    port: 2222,
  });
  const result = await source.storeSudoPassword({
    host: "fresh.example.com",
    user: "ubuntu",
    port: 2222,
    sudoPassword: "sudo-secret",
  });
  assert.equal(result.ok, true);
  assert.equal(result.sudoPasswordRef, expectedRef);
  assert.equal(secrets.get(expectedRef), "sudo-secret");
  assert.equal(JSON.stringify(registry).includes("sudo-secret"), false);
});

test("ssh login password storage writes only to secret store ref", async () => {
  let registry = createDefaultRegistry();
  const secrets = new Map();
  const source = new SshManagedInstanceSource(
    () => registry,
    (next) => {
      registry = next;
    },
    {
      get: async (ref) => secrets.get(ref) || "",
      set: async (ref, value) => secrets.set(ref, value),
    },
  );
  const expectedRef = buildSshPasswordRef({
    host: "fresh.example.com",
    user: "ubuntu",
    port: 2222,
  });
  const result = await source.storeSshPassword({
    host: "fresh.example.com",
    user: "ubuntu",
    port: 2222,
    sshPassword: "login-secret",
  });
  assert.equal(result.ok, true);
  assert.equal(result.sshPasswordRef, expectedRef);
  assert.equal(secrets.get(expectedRef), "login-secret");
  assert.equal(JSON.stringify(registry).includes("login-secret"), false);
});

function assertSshOption(args, value) {
  const optionIndex = args.indexOf(value);
  assert.ok(optionIndex > 0, `${value} missing`);
  assert.equal(args[optionIndex - 1], "-o");
}

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

test("ssh-managed attach stores metadata only and builds webrtc launch", async () => {
  let registry = createDefaultRegistry();
  const secrets = new Map();
  let ensureCalled = false;
  const source = new SshManagedInstanceSource(
    () => registry,
    (next) => {
      registry = next;
    },
    {
      get: async (ref) => secrets.get(ref) || "",
      set: async (ref, value) => secrets.set(ref, value),
    },
    {
      shellUrl: "https://ctox.dev/business-os/",
      inspectHostKey: async () => ({
        host: "51.210.246.120",
        port: 22,
        keyType: "ssh-ed25519",
        algorithm: "SHA256",
        fingerprint: "SHA256:testfingerprint",
        knownHostsLine: "51.210.246.120 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC6Q",
        scannedAt: "2026-06-12T10:00:00.000Z",
      }),
      runEnsureCommand: async () => {
        ensureCalled = true;
        return {
          instance_id: "vps-demo",
          sync_room: "ctox-business-os:vps-demo:abc",
          signaling_room_password: "ssh-room-secret",
          signaling_urls: ["wss://signaling.ctox.dev"],
          native_rxdb_peer_available: true,
        };
      },
    },
  );

  const instance = await source.attachExisting({
    host: "51.210.246.120",
    user: "ubuntu",
    port: 22,
    installRoot: "~/.local/lib/ctox/current",
    displayName: "VPS Demo",
    trustedHostKeyFingerprint: "SHA256:testfingerprint",
  });
  assert.equal(instance.source, "ssh_managed");
  assert.equal(instance.connection.host, "51.210.246.120");
  assert.equal(instance.connection.user, "ubuntu");
  assert.equal(instance.connection.hostKeyFingerprint, "SHA256:testfingerprint");
  assert.equal(ensureCalled, true);
  assert.equal(JSON.stringify(registry).includes("ssh-room-secret"), false);
  assert.equal(secrets.size, 1);

  const launch = await source.getLaunchConfig(instance.id);
  assert.equal(launch.ctoxConfig.transport, "webrtc");
  assert.equal(launch.ctoxConfig.http_bridge_available, false);
});

test("ssh-managed preflight requires trusted host key and returns capabilities", async () => {
  let preflightProfile;
  const source = new SshManagedInstanceSource(
    () => createDefaultRegistry(),
    () => undefined,
    {
      get: async () => "",
      set: async () => undefined,
    },
    {
      inspectHostKey: async () => ({
        host: "example.com",
        port: 22,
        keyType: "ssh-ed25519",
        algorithm: "SHA256",
        fingerprint: "SHA256:trusted",
        knownHostsLine: "example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC6Q",
        scannedAt: "2026-06-12T10:00:00.000Z",
      }),
      runPreflightCommand: async (profile) => {
        preflightProfile = profile;
        return {
          os_name: "Linux",
          os_arch: "aarch64",
          shell_path: "/usr/bin/sh",
          systemctl_path: "",
          sudo_path: "/usr/bin/sudo",
          sudo_nopasswd: "true",
          ctox_path: "/usr/local/bin/ctox",
          install_root_exists: "false",
        };
      },
    },
  );

  await assert.rejects(
    () => source.preflight({ host: "example.com", user: "ubuntu" }),
    /confirmation is required/,
  );
  const preflight = await source.preflight({
    host: "example.com",
    user: "ubuntu",
    trustedHostKeyFingerprint: "SHA256:trusted",
  });
  assert.equal(preflight.os.arch, "aarch64");
  assert.equal(preflight.systemdAvailable, false);
  assert.equal(preflight.passwordlessSudoAvailable, true);
  assert.equal(preflight.needsSudoPassword, false);
  assert.equal(preflight.ctoxAvailable, true);
  assert.equal(preflightProfile.host, "example.com");
});

test("ssh-managed install upgrades existing ctox and registers ensured peer", async () => {
  let registry = createDefaultRegistry();
  const secrets = new Map();
  let installProfile;
  let installOptions;
  let ensureCalled = false;
  const source = new SshManagedInstanceSource(
    () => registry,
    (next) => {
      registry = next;
    },
    {
      get: async (ref) => secrets.get(ref) || "",
      set: async (ref, value) => secrets.set(ref, value),
    },
    {
      inspectHostKey: async () => ({
        host: "example.com",
        port: 22,
        keyType: "ssh-ed25519",
        algorithm: "SHA256",
        fingerprint: "SHA256:trusted",
        knownHostsLine: "example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC6Q",
        scannedAt: "2026-06-12T10:00:00.000Z",
      }),
      runPreflightCommand: async () => ({
        os_name: "Linux",
        os_arch: "x86_64",
        shell_path: "/usr/bin/sh",
        systemctl_path: "/usr/bin/systemctl",
        sudo_path: "/usr/bin/sudo",
        sudo_nopasswd: "true",
        ctox_path: "/usr/local/bin/ctox",
      }),
      runInstallCommand: async (profile, install) => {
        installProfile = profile;
        installOptions = install;
        return { ok: true, releaseChannel: install.releaseChannel, stdout: "updated", stderr: "" };
      },
      runEnsureCommand: async () => {
        ensureCalled = true;
        return {
          instance_id: "upgraded",
          sync_room: "ctox-business-os:upgraded:abc",
          signaling_room_password: "upgraded-room-secret",
          signaling_urls: ["wss://signaling.ctox.dev"],
          native_rxdb_peer_available: true,
        };
      },
    },
  );

  const result = await source.installOrUpgradeExisting({
    host: "example.com",
    user: "ubuntu",
    releaseChannel: "dev",
    trustedHostKeyFingerprint: "SHA256:trusted",
  });
  assert.equal(result.instance.source, "ssh_managed");
  assert.equal(result.instance.connection.installReleaseChannel, "dev");
  assert.equal(result.install.releaseChannel, "dev");
  assert.equal(installOptions.releaseChannel, "dev");
  assert.equal(installProfile.host, "example.com");
  assert.equal(ensureCalled, true);
  assert.equal(JSON.stringify(registry).includes("upgraded-room-secret"), false);
  assert.equal(secrets.size, 1);
});

test("ssh-managed fresh install runs official installer contract and registers peer", async () => {
  let registry = createDefaultRegistry();
  const secrets = new Map();
  let installProfile;
  let installOptions;
  let ensureCalled = false;
  const source = new SshManagedInstanceSource(
    () => registry,
    (next) => {
      registry = next;
    },
    {
      get: async (ref) => secrets.get(ref) || "",
      set: async (ref, value) => secrets.set(ref, value),
    },
    {
      inspectHostKey: async () => ({
        host: "fresh.example.com",
        port: 22,
        keyType: "ssh-ed25519",
        algorithm: "SHA256",
        fingerprint: "SHA256:trusted",
        knownHostsLine: "fresh.example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC6Q",
        scannedAt: "2026-06-12T10:00:00.000Z",
      }),
      runPreflightCommand: async () => ({
        os_name: "Linux",
        os_arch: "x86_64",
        shell_path: "/usr/bin/sh",
        bash_path: "/usr/bin/bash",
        curl_path: "/usr/bin/curl",
        systemctl_path: "/usr/bin/systemctl",
        sudo_path: "/usr/bin/sudo",
        sudo_nopasswd: "true",
        ctox_path: "",
      }),
      runFreshInstallCommand: async (profile, install) => {
        installProfile = profile;
        installOptions = install;
        return { ok: true, mode: "fresh", releaseChannel: install.releaseChannel, stdout: "installed", stderr: "" };
      },
      runEnsureCommand: async () => {
        ensureCalled = true;
        return {
          instance_id: "fresh-vps",
          sync_room: "ctox-business-os:fresh-vps:abc",
          signaling_room_password: "fresh-room-secret",
          signaling_urls: ["wss://signaling.ctox.dev"],
          native_rxdb_peer_available: true,
        };
      },
    },
  );

  const result = await source.installFresh({
    host: "fresh.example.com",
    user: "ubuntu",
    releaseChannel: "stable",
    apiProvider: "openai",
    backend: "cpu",
    trustedHostKeyFingerprint: "SHA256:trusted",
  });
  assert.equal(result.instance.source, "ssh_managed");
  assert.equal(result.instance.connection.installMode, "fresh");
  assert.equal(result.instance.connection.installReleaseChannel, "stable");
  assert.equal(result.install.mode, "fresh");
  assert.equal(result.install.releaseChannel, "stable");
  assert.equal(installOptions.releaseChannel, "stable");
  assert.equal(installOptions.apiProvider, "openai");
  assert.equal(installOptions.backend, "cpu");
  assert.equal(installProfile.host, "fresh.example.com");
  assert.equal(ensureCalled, true);
  assert.equal(JSON.stringify(registry).includes("fresh-room-secret"), false);
  assert.equal(secrets.size, 1);
});

test("ssh-managed install refuses hosts without existing ctox", async () => {
  const source = new SshManagedInstanceSource(
    () => createDefaultRegistry(),
    () => undefined,
    {
      get: async () => "",
      set: async () => undefined,
    },
    {
      inspectHostKey: async () => ({
        host: "example.com",
        port: 22,
        keyType: "ssh-ed25519",
        algorithm: "SHA256",
        fingerprint: "SHA256:trusted",
        knownHostsLine: "example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC6Q",
        scannedAt: "2026-06-12T10:00:00.000Z",
      }),
      runPreflightCommand: async () => ({
        os_name: "Linux",
        os_arch: "x86_64",
        shell_path: "/usr/bin/sh",
        ctox_path: "",
      }),
    },
  );

  await assert.rejects(
    () => source.installOrUpgradeExisting({
      host: "example.com",
      user: "ubuntu",
      trustedHostKeyFingerprint: "SHA256:trusted",
    }),
    /remote ctox binary is required/,
  );
});

test("ssh-managed attach requires explicit host key confirmation", async () => {
  let registry = createDefaultRegistry();
  const source = new SshManagedInstanceSource(
    () => registry,
    (next) => {
      registry = next;
    },
    {
      get: async () => "",
      set: async () => undefined,
    },
    {
      inspectHostKey: async () => ({
        host: "example.com",
        port: 22,
        keyType: "ssh-ed25519",
        algorithm: "SHA256",
        fingerprint: "SHA256:scanned",
        knownHostsLine: "example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC6Q",
        scannedAt: "2026-06-12T10:00:00.000Z",
      }),
      runCommand: async () => {
        throw new Error("must not connect before trust confirmation");
      },
    },
  );

  await assert.rejects(
    () => source.attachExisting({ host: "example.com", user: "ubuntu" }),
    /confirmation is required/,
  );
  await assert.rejects(
    () => source.attachExisting({
      host: "example.com",
      user: "ubuntu",
      trustedHostKeyFingerprint: "SHA256:different",
    }),
    /fingerprint mismatch/,
  );
});
