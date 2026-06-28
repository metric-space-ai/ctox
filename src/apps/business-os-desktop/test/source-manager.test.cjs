"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { createDefaultRegistry, upsertInstance } = require("../src/main/registry.cjs");
const { SourceManager } = require("../src/main/source-manager.cjs");

test("source manager wires an app-owned known_hosts path into the ssh source", () => {
  const manager = new SourceManager({
    registryProvider: () => createDefaultRegistry(),
    registrySaver: () => undefined,
    secretStore: { get: async () => "", set: async () => undefined, delete: async () => undefined },
    fetchImpl: async () => ({ status: 401, ok: false }),
    knownHostsPath: "/tmp/ctox/known_hosts",
  });
  assert.equal(manager.sources.ssh_managed.knownHostsPath, "/tmp/ctox/known_hosts");
});

test("source manager removes unmanaged instances and blocks managed local delete", async () => {
  let registry = createDefaultRegistry();
  registry = upsertInstance(registry, {
    id: "paired-a",
    source: "pairing_invite",
    displayName: "Paired",
    pairing: {
      syncRoom: "ctox-business-os:paired-a",
      signalingUrls: ["wss://signaling.ctox.dev"],
      secretRef: "keychain://ctox/paired-a",
    },
    secretRefs: ["keychain://ctox/paired-a"],
  });
  const deleted = [];
  const manager = new SourceManager({
    registryProvider: () => registry,
    registrySaver: (next) => {
      registry = next;
    },
    secretStore: {
      get: async () => "",
      set: async () => undefined,
      delete: async (ref) => deleted.push(ref),
    },
    fetchImpl: async () => ({ status: 401, ok: false }),
  });
  await manager.removeInstance({ id: "paired-a", source: "pairing_invite" });
  assert.deepEqual(deleted, ["keychain://ctox/paired-a"]);
  await assert.rejects(
    () => manager.removeInstance({ id: "managed-a", source: "ctox_dev" }),
    /managed instances/,
  );
});

test("source manager refreshes ctox.dev revocation without dropping unmanaged instances", async () => {
  let registry = createDefaultRegistry();
  registry = upsertInstance(registry, {
    id: "paired-a",
    source: "pairing_invite",
    displayName: "Paired",
    pairing: {
      syncRoom: "ctox-business-os:paired-a",
      signalingUrls: ["wss://signaling.ctox.dev"],
      secretRef: "keychain://ctox/paired-a",
    },
    secretRefs: ["keychain://ctox/paired-a"],
  });
  registry = upsertInstance(registry, {
    id: "ssh-a",
    source: "ssh_managed",
    displayName: "SSH VPS",
    connection: {
      host: "203.0.113.11",
      user: "ubuntu",
      port: 22,
      managedBy: "desktop",
    },
  });
  let revoked = false;
  const manager = new SourceManager({
    registryProvider: () => registry,
    registrySaver: (next) => {
      registry = next;
    },
    secretStore: {
      get: async () => "",
      set: async () => undefined,
      delete: async () => undefined,
    },
    fetchImpl: async (url) => {
      assert.equal(url, "https://ctox.dev/api/desktop/session-package");
      return {
        ok: true,
        status: 200,
        json: async () => ({
          account: {
            tenants: [
              {
                id: "tenant_example",
                slug: "example",
                domain: "example.ctox.dev",
                businessName: "Example",
                status: "active",
                healthStatus: "ok",
                tenantRole: "admin",
                launchAllowed: true,
              },
              ...(revoked ? [] : [{
                id: "tenant_skf",
                slug: "skf",
                domain: "acme.ctox.dev",
                businessName: "SKF",
                status: "active",
                healthStatus: "ok",
                tenantRole: "owner",
                launchAllowed: true,
              }]),
            ],
          },
        }),
      };
    },
  });

  assert.deepEqual(
    (await manager.listInstances()).map((instance) => [instance.id, instance.source]),
    [
      ["managed:tenant_example", "ctox_dev"],
      ["paired-a", "pairing_invite"],
      ["managed:tenant_skf", "ctox_dev"],
      ["ssh-a", "ssh_managed"],
    ],
  );
  revoked = true;
  assert.deepEqual(
    (await manager.listInstances()).map((instance) => [instance.id, instance.source]),
    [
      ["managed:tenant_example", "ctox_dev"],
      ["paired-a", "pairing_invite"],
      ["ssh-a", "ssh_managed"],
    ],
  );
});

test("source manager blocks non-launchable ctox.dev instances before launch token request", async () => {
  let registry = createDefaultRegistry();
  const calls = [];
  const manager = new SourceManager({
    registryProvider: () => registry,
    registrySaver: (next) => {
      registry = next;
    },
    secretStore: {
      get: async () => "",
      set: async () => undefined,
      delete: async () => undefined,
    },
    fetchImpl: async (url, options) => {
      calls.push([url, options?.method || "GET"]);
      return {
        ok: true,
        status: 200,
        json: async () => ({
          account: {
            tenants: [{
              id: "tenant_denied",
              slug: "denied",
              domain: "denied.ctox.dev",
              businessName: "Denied",
              status: "active",
              healthStatus: "ok",
              tenantRole: "viewer",
              launchAllowed: false,
            }],
          },
        }),
      };
    },
  });

  const [instance] = await manager.listInstances();
  assert.equal(instance.id, "managed:tenant_denied");
  assert.equal(instance.status, "needs_auth");
  await assert.rejects(
    () => manager.getLaunchConfig(instance),
    /not launchable: needs_auth/,
  );
  assert.deepEqual(calls, [["https://ctox.dev/api/desktop/session-package", "GET"]]);
});

test("source manager routes ssh fresh installs separately from existing upgrades", async () => {
  const manager = new SourceManager({
    registryProvider: () => createDefaultRegistry(),
    registrySaver: () => undefined,
    secretStore: {
      get: async () => "",
      set: async () => undefined,
      delete: async () => undefined,
    },
    fetchImpl: async () => ({ status: 401, ok: false }),
  });
  const calls = [];
  manager.sources.ssh_managed = {
    installFresh: async (options) => {
      calls.push(["fresh", options.host]);
      return { ok: true, mode: "fresh" };
    },
    installOrUpgradeExisting: async (options) => {
      calls.push(["existing", options.host]);
      return { ok: true, mode: "existing" };
    },
  };

  assert.equal((await manager.installSshManaged({ host: "fresh.example.com", freshInstall: true })).mode, "fresh");
  assert.equal((await manager.installSshManaged({ host: "existing.example.com" })).mode, "existing");
  assert.deepEqual(calls, [
    ["fresh", "fresh.example.com"],
    ["existing", "existing.example.com"],
  ]);
});

test("source manager routes ssh sudo password storage", async () => {
  const manager = new SourceManager({
    registryProvider: () => createDefaultRegistry(),
    registrySaver: () => undefined,
    secretStore: {
      get: async () => "",
      set: async () => undefined,
      delete: async () => undefined,
    },
    fetchImpl: async () => ({ status: 401, ok: false }),
  });
  let received;
  manager.sources.ssh_managed = {
    storeSudoPassword: async (options) => {
      received = options;
      return { ok: true, sudoPasswordRef: "keychain://ctox-business-os-desktop/ssh-sudo/test" };
    },
  };

  const result = await manager.storeSshSudoPassword({
    host: "example.com",
    user: "ubuntu",
    sudoPassword: "secret",
  });
  assert.equal(result.sudoPasswordRef, "keychain://ctox-business-os-desktop/ssh-sudo/test");
  assert.equal(received.sudoPassword, "secret");
});

test("source manager routes ssh login password storage", async () => {
  const manager = new SourceManager({
    registryProvider: () => createDefaultRegistry(),
    registrySaver: () => undefined,
    secretStore: {
      get: async () => "",
      set: async () => undefined,
      delete: async () => undefined,
    },
    fetchImpl: async () => ({ status: 401, ok: false }),
  });
  let received;
  manager.sources.ssh_managed = {
    storeSshPassword: async (options) => {
      received = options;
      return { ok: true, sshPasswordRef: "keychain://ctox-business-os-desktop/ssh-login/test" };
    },
  };

  const result = await manager.storeSshLoginPassword({
    host: "example.com",
    user: "ubuntu",
    sshPassword: "secret",
  });
  assert.equal(result.sshPasswordRef, "keychain://ctox-business-os-desktop/ssh-login/test");
  assert.equal(received.sshPassword, "secret");
});
