"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const {
  LinuxSecretServiceStore,
  MacOsKeychainSecretStore,
  MemorySecretStore,
  WindowsCredentialManagerStore,
  createSecretStore,
} = require("../src/main/secret-store.cjs");

test("memory secret store round-trips without registry involvement", async () => {
  const store = new MemorySecretStore();
  await store.set("keychain://ctox/test", "secret");
  assert.equal(await store.get("keychain://ctox/test"), "secret");
  await store.delete("keychain://ctox/test");
  assert.equal(await store.get("keychain://ctox/test"), "");
});

test("macOS keychain store uses security generic password commands", async () => {
  const calls = [];
  const store = new MacOsKeychainSecretStore({
    service: "CTOX Test",
    runner: async (program, args, options = {}) => {
      calls.push([program, args, options]);
      return { stdout: args[0] === "find-generic-password" ? "stored-secret\n" : "" };
    },
  });
  await store.set("ref-1", "secret-1");
  assert.equal(await store.get("ref-1"), "stored-secret");
  await store.delete("ref-1");
  assert.equal(calls[0][0], "security");
  assert.deepEqual(calls[0][1], [
    "add-generic-password",
    "-a",
    "ref-1",
    "-s",
    "CTOX Test",
    "-U",
    "-w",
  ]);
  assert.equal(calls[0][2].input, "secret-1\nsecret-1\n");
  assert.equal(calls[0][2].timeoutMs, 120000);
  assert.equal(JSON.stringify(calls[0][1]).includes("secret-1"), false);
});

test("linux secret service store uses secret-tool with stdin for secret values", async () => {
  const calls = [];
  const store = new LinuxSecretServiceStore({
    service: "CTOX Test",
    appId: "ctox-test",
    runner: async (program, args, options = {}) => {
      calls.push([program, args, options]);
      return { stdout: args[0] === "lookup" ? "stored-secret\n" : "" };
    },
  });
  await store.set("ref-1", "secret-1");
  assert.equal(await store.get("ref-1"), "stored-secret");
  await store.delete("ref-1");
  assert.equal(calls[0][0], "secret-tool");
  assert.deepEqual(calls[0][1], [
    "store",
    "--label",
    "CTOX Test",
    "application",
    "ctox-test",
    "ref",
    "ref-1",
  ]);
  assert.equal(calls[0][2].input, "secret-1\n");
  assert.equal(JSON.stringify(calls[0][1]).includes("secret-1"), false);
});

test("windows credential manager store uses powershell stdin payload without secret args", async () => {
  const calls = [];
  const store = new WindowsCredentialManagerStore({
    service: "CTOX Test",
    runner: async (program, args, options = {}) => {
      calls.push([program, args, options]);
      return { stdout: JSON.parse(options.input).action === "get" ? "stored-secret" : "" };
    },
  });
  await store.set("ref-1", "secret-1");
  assert.equal(await store.get("ref-1"), "stored-secret");
  await store.delete("ref-1");
  assert.equal(calls[0][0], "powershell.exe");
  assert.ok(calls[0][1].includes("-NonInteractive"));
  assert.equal(JSON.stringify(calls[0][1]).includes("secret-1"), false);
  assert.match(calls[0][1].at(-1), /CredWriteW/);
  assert.match(calls[0][1].at(-1), /CredReadW/);
  assert.match(calls[0][1].at(-1), /CredDeleteW/);
  const payload = JSON.parse(calls[0][2].input);
  assert.deepEqual(payload, {
    action: "set",
    target: "CTOX Test:ref-1",
    userName: "CTOX Test",
    secret: "secret-1",
  });
});

test("createSecretStore is fail-closed on unsupported platforms by default", async () => {
  const store = createSecretStore({ platform: "plan9" });
  assert.equal(await store.get("missing"), "");
  await assert.rejects(() => store.set("ref", "secret"), /not implemented/);
});

test("createSecretStore selects platform keychain adapters", () => {
  assert.ok(createSecretStore({ platform: "darwin" }) instanceof MacOsKeychainSecretStore);
  assert.ok(createSecretStore({ platform: "linux" }) instanceof LinuxSecretServiceStore);
  assert.ok(createSecretStore({ platform: "win32" }) instanceof WindowsCredentialManagerStore);
});
