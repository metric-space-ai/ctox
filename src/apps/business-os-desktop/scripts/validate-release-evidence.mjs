#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const AUTH_E2E_SCHEMA = "ctox-dev-auth-e2e-v1";
const APPSEC_AUTHZ_E2E_SCHEMA = "ctox-appsec-authz-e2e-v1";

export function validateCtoxDevAuthE2e(input, options = {}) {
  const evidence = selectCtoxDevAuthE2eEvidence(input);
  const requireOidc = options.requireOidc !== false;
  const requireAppsecAuthz = options.requireAppsecAuthz !== false;
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
  if (requireAppsecAuthz) {
    errors.push(...validateAppsecAuthzE2e(evidence));
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

function validateAppsecAuthzE2e(evidence) {
  const authz = selectAppsecAuthzE2eEvidence(evidence);
  const errors = [];
  if (!authz) {
    errors.push("ctox.dev release evidence requires AppSec authz e2e evidence");
    return errors;
  }
  if (authz.evidenceVersion !== APPSEC_AUTHZ_E2E_SCHEMA) {
    errors.push(`AppSec authz e2e evidenceVersion must be ${APPSEC_AUTHZ_E2E_SCHEMA}`);
  }
  if (authz.ok !== true) errors.push("AppSec authz e2e evidence ok must be true");
  if (authz.skipped !== false) errors.push("AppSec authz e2e evidence skipped must be false");
  if (authz.matrixImported !== true) errors.push("AppSec authz e2e matrixImported must be true");
  if (authz.browserContextArtifactsImported !== true) {
    errors.push("AppSec authz e2e browserContextArtifactsImported must be true");
  }
  if (authz.redactionAuditPassed !== true) {
    errors.push("AppSec authz e2e redactionAuditPassed must be true");
  }
  if (Number(authz.credentialedSubjectCount || 0) < 2) {
    errors.push("AppSec authz e2e credentialedSubjectCount must be at least 2");
  }
  if (authz.unauthenticatedSubjectTested !== true) {
    errors.push("AppSec authz e2e unauthenticatedSubjectTested must be true");
  }
  const matrix = authz.authzMatrix || {};
  if (matrix.imported !== true) errors.push("AppSec authz matrix imported must be true");
  if (matrix.requiredFieldsPresent !== true) {
    errors.push("AppSec authz matrix requiredFieldsPresent must be true");
  }
  if (Number(matrix.subjectCount || 0) < 3) {
    errors.push("AppSec authz matrix subjectCount must include unauthenticated plus at least two credentialed subjects");
  }
  if (Number(matrix.ownerBaselineCaseCount || 0) < 1) {
    errors.push("AppSec authz matrix ownerBaselineCaseCount must be at least 1");
  }
  if (Number(matrix.crossSubjectReplayCaseCount || 0) < 1) {
    errors.push("AppSec authz matrix crossSubjectReplayCaseCount must be at least 1");
  }
  if (Number(matrix.caseCount || 0) < Number(matrix.ownerBaselineCaseCount || 0) + Number(matrix.crossSubjectReplayCaseCount || 0)) {
    errors.push("AppSec authz matrix caseCount must cover owner baseline and cross-subject replay cases");
  }
  if (authz.crossAccountSuccessesReviewed !== true) {
    errors.push("AppSec authz e2e crossAccountSuccessesReviewed must be true");
  }
  if (!isNonEmptyString(authz.reportArtifact)) {
    errors.push("AppSec authz e2e reportArtifact must be present");
  }
  if (!isNonEmptyString(authz.completionReviewArtifact)) {
    errors.push("AppSec authz e2e completionReviewArtifact must be present");
  }
  if (!isNonEmptyString(authz.matrixArtifact)) {
    errors.push("AppSec authz e2e matrixArtifact must be present");
  }
  return errors;
}

function selectAppsecAuthzE2eEvidence(evidence) {
  for (const key of [
    "appsecAuthzE2e",
    "appsec_authz_e2e",
    "ctoxAppsecAuthzE2e",
    "ctox_appsec_authz_e2e",
  ]) {
    const value = evidence[key];
    if (value && typeof value === "object") return value;
  }
  return null;
}

function isNonEmptyString(value) {
  return typeof value === "string" && value.trim().length > 0;
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
  if (/"(?:cookie|cookies|storageState|screenshot|browser_stream|rawBrowserStream|raw_browser_stream)"\s*:/i.test(serialized)) {
    errors.push("ctox.dev auth e2e evidence contains forbidden raw browser or session artifact keys");
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
    requireAppsecAuthz: process.env.CTOX_DEV_AUTH_E2E_REQUIRE_APPSEC_AUTHZ !== "0",
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
