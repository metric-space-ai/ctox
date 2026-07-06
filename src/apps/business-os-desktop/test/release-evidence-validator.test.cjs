"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");

async function validator() {
  return import("../scripts/validate-release-evidence.mjs");
}

function validEvidence(overrides = {}) {
  const { authE2e = {}, appsecAuthzE2e = {}, ...rest } = overrides;
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
    appsecAuthzE2e: {
      evidenceVersion: "ctox-appsec-authz-e2e-v1",
      ok: true,
      skipped: false,
      matrixImported: true,
      browserContextArtifactsImported: true,
      redactionAuditPassed: true,
      credentialedSubjectCount: 2,
      unauthenticatedSubjectTested: true,
      crossAccountSuccessesReviewed: true,
      reportArtifact: "runtime/appsec/ctox-dev/reports/report.md",
      completionReviewArtifact: "runtime/appsec/ctox-dev/completion-review.json",
      matrixArtifact: "runtime/appsec/ctox-dev/authz/authz-matrix.json",
      authzMatrix: {
        imported: true,
        requiredFieldsPresent: true,
        subjectCount: 3,
        ownerBaselineCaseCount: 1,
        crossSubjectReplayCaseCount: 2,
        caseCount: 3,
        cases: [
          {
            type: "owner-baseline",
            actor: "user-a",
            owner: "user-a",
            object: "account:user-a",
            endpoint: "/api/accounts/user-a",
            method: "GET",
            expected: "allow",
            actual: "allow",
            actualStatus: 200,
            bodyClass: "owner-data",
            accessDecision: "baseline-allow",
            evidenceArtifact: "runtime/appsec/ctox-dev/authz/user-a/owner-baseline.json",
          },
          {
            type: "cross-subject-replay",
            actor: "user-b",
            owner: "user-a",
            object: "account:user-a",
            endpoint: "/api/accounts/user-a",
            method: "GET",
            expected: "deny",
            actual: "deny",
            actualStatus: 404,
            bodyClass: "denied",
            leakDecision: "no-leak",
            evidenceArtifact: "runtime/appsec/ctox-dev/authz/replay/user-a-as-user-b.json",
          },
          {
            type: "cross-subject-replay",
            actor: "unauthenticated",
            owner: "user-a",
            object: "account:user-a",
            endpoint: "/api/accounts/user-a",
            method: "GET",
            expected: "deny",
            actual: "deny",
            actualStatus: 302,
            bodyClass: "login-required",
            leakDecision: "no-leak",
            evidenceArtifact: "runtime/appsec/ctox-dev/authz/replay/user-a-as-unauthenticated.json",
          },
        ],
      },
      ...appsecAuthzE2e,
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

test("ctox.dev auth e2e release evidence requires AppSec authz proof by default", async () => {
  const { validateCtoxDevAuthE2e } = await validator();
  const { appsecAuthzE2e, ...authOnly } = validEvidence();
  assert.equal(appsecAuthzE2e.ok, true);
  const result = validateCtoxDevAuthE2e(authOnly);
  assert.equal(result.ok, false);
  assert.match(result.errors.join("\n"), /requires AppSec authz e2e evidence/);
  assert.equal(validateCtoxDevAuthE2e(authOnly, { requireAppsecAuthz: false }).ok, true);
});

test("ctox.dev auth e2e release evidence rejects incomplete AppSec authz matrix proof", async () => {
  const { validateCtoxDevAuthE2e } = await validator();
  const result = validateCtoxDevAuthE2e(validEvidence({
    appsecAuthzE2e: {
      matrixImported: false,
      browserContextArtifactsImported: false,
      redactionAuditPassed: false,
      credentialedSubjectCount: 1,
      unauthenticatedSubjectTested: false,
      crossAccountSuccessesReviewed: false,
      reportArtifact: "",
      authzMatrix: {
        imported: false,
        requiredFieldsPresent: false,
        subjectCount: 2,
        ownerBaselineCaseCount: 0,
        crossSubjectReplayCaseCount: 0,
        caseCount: 0,
      },
    },
  }));
  assert.equal(result.ok, false);
  const errors = result.errors.join("\n");
  assert.match(errors, /matrixImported must be true/);
  assert.match(errors, /browserContextArtifactsImported must be true/);
  assert.match(errors, /redactionAuditPassed must be true/);
  assert.match(errors, /credentialedSubjectCount must be at least 2/);
  assert.match(errors, /unauthenticatedSubjectTested must be true/);
  assert.match(errors, /requiredFieldsPresent must be true/);
  assert.match(errors, /crossSubjectReplayCaseCount must be at least 1/);
  assert.match(errors, /reportArtifact must be present/);
});

test("ctox.dev auth e2e release evidence rejects AppSec authz counters without concrete cases", async () => {
  const { validateCtoxDevAuthE2e } = await validator();
  const result = validateCtoxDevAuthE2e(validEvidence({
    appsecAuthzE2e: {
      authzMatrix: {
        imported: true,
        requiredFieldsPresent: true,
        subjectCount: 3,
        ownerBaselineCaseCount: 1,
        crossSubjectReplayCaseCount: 1,
        caseCount: 2,
        cases: [
          {
            type: "cross-subject-replay",
            actor: "user-b",
            owner: "user-a",
            object: "account:user-a",
            endpoint: "/api/accounts/user-a",
            method: "GET",
            expected: "deny",
            actual: "allow",
            bodyClass: "owner-data",
          },
        ],
      },
    },
  }));
  assert.equal(result.ok, false);
  const errors = result.errors.join("\n");
  assert.match(errors, /cases must include at least one owner-baseline case/);
  assert.match(errors, /leak, mutation, or access decision must be present/);
  assert.match(errors, /evidence artifact must be present/);
});

test("ctox.dev auth e2e release evidence accepts native AppSec authz matrix fields", async () => {
  const { validateCtoxDevAuthE2e } = await validator();
  const result = validateCtoxDevAuthE2e(validEvidence({
    appsecAuthzE2e: {
      authzMatrix: {
        imported: true,
        requiredFieldsPresent: true,
        subjectCount: 3,
        ownerBaselineCaseCount: 1,
        crossSubjectReplayCaseCount: 1,
        caseCount: 2,
        cases: [
          {
            id: "owner-baseline-allow",
            endpoint: "/api/instances/tenant-a/health",
            method: "GET",
            actor_subject: "user-a",
            owner_subject: "user-a",
            object_type: "tenant",
            object_ref: "tenant-a",
            expected: "allow",
            actual_status: 200,
            result: "pass",
            body_class: "tenant-json",
            evidence_artifact: "authz/owner-baseline-allow-redacted.json",
          },
          {
            id: "case-001",
            endpoint: "/api/instances/tenant-a/health",
            method: "GET",
            actor_subject: "user-b",
            owner_subject: "user-a",
            object_type: "tenant",
            object_ref: "tenant-a",
            expected: "deny",
            actual_status: 404,
            result: "pass",
            body_class: "not-found",
            leak: false,
            mutation: false,
            evidence_artifact: "authz/replay/user-a-as-user-b.json",
          },
        ],
      },
    },
  }));
  assert.deepEqual(result, { ok: true, errors: [] });
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
  assert.match(
    validateCtoxDevAuthE2e(validEvidence({ appsecAuthzE2e: { screenshot: "/tmp/live.png" } })).errors.join("\n"),
    /forbidden raw browser or session artifact keys/,
  );
});
