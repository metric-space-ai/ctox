"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");

async function validator() {
  return import("../scripts/validate-release-evidence.mjs");
}

function validEvidence(overrides = {}) {
  const { authE2e = {}, ...rest } = overrides;
  return {
    evidenceVersion: "ctox-dev-auth-e2e-v1",
    ok: true,
    skipped: false,
    authE2e: {
      signupPassed: true,
      loginPassed: true,
      authenticatedReloadPassed: true,
      logoutPassed: true,
      loggedOutReloadBlockedProtectedAccess: true,
      browserHealthChecked: true,
      oidcProviderTested: true,
      fakeAuthUsed: false,
      consoleErrorCount: 0,
      failedRequestCount: 0,
      ...authE2e,
    },
    ...rest,
  };
}

test("ctox.dev auth e2e release evidence accepts complete proof", async () => {
  const { validateCtoxDevAuthE2e } = await validator();
  assert.deepEqual(validateCtoxDevAuthE2e(validEvidence()), {
    ok: true,
    errors: [],
  });
  assert.equal(validateCtoxDevAuthE2e({ ctoxDevAuthE2e: validEvidence() }).ok, true);
});

test("ctox.dev auth e2e release evidence rejects skipped or fake proof", async () => {
  const { validateCtoxDevAuthE2e } = await validator();
  assert.match(validateCtoxDevAuthE2e(validEvidence({ skipped: true })).errors.join("\n"), /skipped must be false/);
  assert.match(
    validateCtoxDevAuthE2e(validEvidence({ authE2e: { fakeAuthUsed: true } })).errors.join("\n"),
    /fakeAuthUsed must be false/,
  );
});

test("ctox.dev auth e2e release evidence requires browser health and protected reload proof", async () => {
  const { validateCtoxDevAuthE2e } = await validator();
  const result = validateCtoxDevAuthE2e(validEvidence({
    authE2e: {
      browserHealthChecked: false,
      loggedOutReloadBlockedProtectedAccess: false,
      consoleErrorCount: 1,
      failedRequestCount: 2,
    },
  }));
  assert.equal(result.ok, false);
  assert.match(result.errors.join("\n"), /browserHealthChecked must be true/);
  assert.match(result.errors.join("\n"), /loggedOutReloadBlockedProtectedAccess must be true/);
  assert.match(result.errors.join("\n"), /consoleErrorCount must be 0/);
  assert.match(result.errors.join("\n"), /failedRequestCount must be 0/);
});

test("ctox.dev auth e2e release evidence can make oidc optional only when explicit", async () => {
  const { validateCtoxDevAuthE2e } = await validator();
  const withoutOidc = validEvidence({ authE2e: { oidcProviderTested: false } });
  assert.match(validateCtoxDevAuthE2e(withoutOidc).errors.join("\n"), /oidcProviderTested must be true/);
  assert.equal(validateCtoxDevAuthE2e(withoutOidc, { requireOidc: false }).ok, true);
});

test("ctox.dev auth e2e release evidence rejects auth secret leaks", async () => {
  const { validateCtoxDevAuthE2e } = await validator();
  assert.match(
    validateCtoxDevAuthE2e(validEvidence({ responseHeaders: { "set-cookie": "ctox_session=live-secret" } })).errors.join("\n"),
    /ctox_session cookie/,
  );
  assert.match(
    validateCtoxDevAuthE2e(validEvidence({ requestHeaders: { authorization: "Bearer live-token-value" } })).errors.join("\n"),
    /bearer token/,
  );
  assert.match(
    validateCtoxDevAuthE2e(validEvidence({ credentials: { password: "live-password" } })).errors.join("\n"),
    /secret-like field/,
  );
});
