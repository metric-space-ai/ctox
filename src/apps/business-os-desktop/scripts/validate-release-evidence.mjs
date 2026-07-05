#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const AUTH_E2E_SCHEMA = "ctox-dev-auth-e2e-v1";

export function validateCtoxDevAuthE2e(input, options = {}) {
  const evidence = selectCtoxDevAuthE2eEvidence(input);
  const requireOidc = options.requireOidc !== false;
  const errors = [];
  if (!evidence) {
    errors.push("ctox.dev auth e2e evidence is missing");
    return result(errors);
  }
  if (evidence.evidenceVersion !== AUTH_E2E_SCHEMA) {
    errors.push(`ctox.dev auth e2e evidenceVersion must be ${AUTH_E2E_SCHEMA}`);
  }
  if (evidence.ok !== true) errors.push("ctox.dev auth e2e evidence ok must be true");
  if (evidence.skipped !== false) errors.push("ctox.dev auth e2e evidence skipped must be false");
  const auth = evidence.authE2e || {};
  for (const key of [
    "signupPassed",
    "loginPassed",
    "authenticatedReloadPassed",
    "logoutPassed",
    "loggedOutReloadBlockedProtectedAccess",
    "browserHealthChecked",
  ]) {
    if (auth[key] !== true) errors.push(`ctox.dev auth e2e ${key} must be true`);
  }
  if (requireOidc && auth.oidcProviderTested !== true) {
    errors.push("ctox.dev auth e2e oidcProviderTested must be true");
  }
  if (auth.fakeAuthUsed !== false) errors.push("ctox.dev auth e2e fakeAuthUsed must be false");
  if (Number(auth.consoleErrorCount || 0) !== 0) {
    errors.push("ctox.dev auth e2e consoleErrorCount must be 0");
  }
  if (Number(auth.failedRequestCount || 0) !== 0) {
    errors.push("ctox.dev auth e2e failedRequestCount must be 0");
  }
  errors.push(...scanNoAuthSecretLeaks(evidence));
  return result(errors);
}

function selectCtoxDevAuthE2eEvidence(input) {
  if (!input || typeof input !== "object") return null;
  if (input.evidenceVersion === AUTH_E2E_SCHEMA) return input;
  for (const key of [
    "ctoxDevAuthE2e",
    "ctox_dev_auth_e2e",
    "authE2eEvidence",
    "auth_e2e_evidence",
  ]) {
    const value = input[key];
    if (value && typeof value === "object" && value.evidenceVersion === AUTH_E2E_SCHEMA) return value;
  }
  const nested = input.evidence || input.releaseEvidence || input.artifacts;
  if (nested && typeof nested === "object") return selectCtoxDevAuthE2eEvidence(nested);
  return null;
}

function scanNoAuthSecretLeaks(value) {
  const serialized = JSON.stringify(value);
  const errors = [];
  if (/ctox_session=(?!<redacted>)/i.test(serialized)) {
    errors.push("ctox.dev auth e2e evidence leaks an unredacted ctox_session cookie");
  }
  if (/Bearer\s+(?!<redacted>)[A-Za-z0-9._~+/=-]{12,}/i.test(serialized)) {
    errors.push("ctox.dev auth e2e evidence leaks an unredacted bearer token");
  }
  if (/"(?:password|secret|token|authorization)"\s*:\s*"(?!<redacted>"|Bearer <redacted>")[^"]{4,}"/i.test(serialized)) {
    errors.push("ctox.dev auth e2e evidence contains an unredacted secret-like field");
  }
  return errors;
}

function result(errors) {
  return {
    ok: errors.length === 0,
    errors,
  };
}

function main(argv) {
  const file = argv[2] || process.env.CTOX_RELEASE_EVIDENCE_PATH || "";
  if (!file) {
    console.error("usage: validate-release-evidence.mjs <release-evidence.json>");
    process.exit(2);
  }
  const payload = JSON.parse(readFileSync(file, "utf8"));
  const validation = validateCtoxDevAuthE2e(payload, {
    requireOidc: process.env.CTOX_DEV_AUTH_E2E_REQUIRE_OIDC !== "0",
  });
  if (!validation.ok) {
    console.error(`release evidence validation failed:\n- ${validation.errors.join("\n- ")}`);
    process.exit(1);
  }
  console.log("release evidence validation OK");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main(process.argv);
}
