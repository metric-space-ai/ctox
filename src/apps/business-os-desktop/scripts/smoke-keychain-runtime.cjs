"use strict";

const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const {
  LinuxSecretServiceStore,
  MacOsKeychainSecretStore,
  WindowsCredentialManagerStore,
} = require("../src/main/secret-store.cjs");

async function main() {
  const platformStore = createPlatformStore();
  if (!platformStore) {
    console.log(`desktop keychain runtime smoke skipped (${process.platform})`);
    return;
  }

  const { label, store } = platformStore;
  const ref = `keychain://ctox-business-os-desktop/${process.platform}/runtime-smoke/${crypto.randomUUID()}`;
  const secret = `ctox-smoke-${crypto.randomUUID()}`;
  try {
    await store.set(ref, secret);
    assert.equal(await store.get(ref), secret);
  } finally {
    await store.delete(ref).catch(() => undefined);
  }
  assert.equal(await store.get(ref).catch(() => ""), "");
  console.log(`desktop keychain runtime smoke OK (${label})`);
}

function createPlatformStore() {
  const service = "CTOX Business OS Desktop Runtime Smoke";
  if (process.platform === "darwin") {
    return {
      label: "macOS Keychain",
      store: new MacOsKeychainSecretStore({ service }),
    };
  }
  if (process.platform === "linux") {
    return {
      label: "Linux Secret Service",
      store: new LinuxSecretServiceStore({
        service,
        appId: "ctox-business-os-desktop-runtime-smoke",
      }),
    };
  }
  if (process.platform === "win32") {
    return {
      label: "Windows Credential Manager",
      store: new WindowsCredentialManagerStore({ service }),
    };
  }
  return null;
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack || error.message : String(error));
  process.exit(1);
});
