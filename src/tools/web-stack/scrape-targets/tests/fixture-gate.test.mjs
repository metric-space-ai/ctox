import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, readdirSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import test from "node:test";

const testDir = path.dirname(fileURLToPath(import.meta.url));
const targetsDir = path.dirname(testDir);
const fixturesDir = path.join(testDir, "fixtures");
const mockCtox = path.join(testDir, "mock-ctox.mjs");
const sharedScript = path.join(targetsDir, "_shared", "generic-prospect-v1.js");
const PROTECTED_TARGETS = ["dnbhoovers.com", "leadfeeder.com", "linkedin.com", "rocketreach.com", "xing.com"];
const PUBLIC_UNLOCK_TARGETS = [
  "bundesanzeiger.de",
  "companyhouse.de",
  "handelsregister.de",
  "northdata.de",
];

const FIELD_KEYS = new Set([
  "firma_name", "firma_anschrift", "firma_plz", "firma_ort", "firma_email",
  "firma_domain", "firma_telefon", "firma_fax", "wz_code", "umsatz", "mitarbeiter",
  "crm_record_number", "person_geschlecht", "person_titel", "person_vorname",
  "person_nachname", "person_funktion", "person_position", "person_email",
  "person_email_validation", "person_telefon", "person_linkedin", "person_xing",
]);

function targetDirectories() {
  return readdirSync(targetsDir)
    .filter((name) => !name.startsWith("_") && name !== "tests")
    .filter((name) => statSync(path.join(targetsDir, name)).isDirectory())
    .filter((name) => statSync(path.join(targetsDir, name, "target.json"), { throwIfNoEntry: false }))
    .sort();
}

function loadJson(file) {
  return JSON.parse(readFileSync(file, "utf8"));
}

function resolvedScript(targetName) {
  const specialized = path.join(targetsDir, targetName, "scripts", "v1.js");
  return statSync(specialized, { throwIfNoEntry: false }) ? specialized : sharedScript;
}

function executeFixture(targetName, fixturePath, mode, inputOverride) {
  const fixture = loadJson(fixturePath);
  const outputDir = mkdtempSync(path.join(tmpdir(), `ctox-scrape-${targetName}-`));
  const callLog = path.join(outputDir, "ctox-calls.jsonl");
  try {
    const child = spawnSync(process.execPath, [resolvedScript(targetName)], {
      cwd: targetsDir,
      encoding: "utf8",
      timeout: 30_000,
      env: {
        ...process.env,
        CTOX_BIN: mockCtox,
        CTOX_SCRAPE_FIXTURE: fixturePath,
        CTOX_SCRAPE_FIXTURE_MODE: mode,
        CTOX_SCRAPE_CALL_LOG: callLog,
        CTOX_SCRAPE_INPUT_JSON: JSON.stringify(inputOverride === undefined ? fixture.input : inputOverride),
        CTOX_SCRAPE_OUTPUT_DIR: outputDir,
      },
    });
    assert.equal(child.signal, null, `${targetName}/${mode} timed out`);
    assert.equal(child.status, 0, `${targetName}/${mode}: ${child.stderr || child.stdout}`);
    assert.doesNotThrow(() => JSON.parse(child.stdout), `${targetName}/${mode} returned invalid JSON`);
    const calls = statSync(callLog, { throwIfNoEntry: false })
      ? readFileSync(callLog, "utf8").trim().split("\n")
        .filter(Boolean)
        .map((line) => JSON.parse(line).args)
      : [];
    return { result: JSON.parse(child.stdout), calls };
  } finally {
    rmSync(outputDir, { recursive: true, force: true });
  }
}

function runFixture(targetName, fixturePath, mode) {
  return executeFixture(targetName, fixturePath, mode).result;
}

function flagValue(args, name) {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
}

function containsForbiddenSecretKey(value) {
  if (Array.isArray(value)) return value.some(containsForbiddenSecretKey);
  if (!value || typeof value !== "object") return false;
  return Object.entries(value).some(([key, item]) =>
    /^(?:password|passwd|token|api[_-]?key|secret_value|credential_value)$/i.test(key)
      || containsForbiddenSecretKey(item)
  );
}

test("all DACH research scrape targets pass production-like fixture gates", async (t) => {
  const targets = targetDirectories();
  assert.equal(targets.length, 16, `expected 16 targets, found: ${targets.join(", ")}`);

  const seenKeys = new Set();
  for (const targetName of targets) {
    await t.test(targetName, () => {
      const manifest = loadJson(path.join(targetsDir, targetName, "target.json"));
      const fixturePath = path.join(fixturesDir, `${targetName}.json`);
      const fixture = loadJson(fixturePath);
      const script = resolvedScript(targetName);

      assert.equal(manifest.status, "active");
      assert.equal(manifest.target_kind, "prospect-research");
      assert.ok(String(manifest.config?.expected_provider || "").trim());
      assert.ok(Array.isArray(manifest.config?.country_hints));
      assert.ok(manifest.config.country_hints.length > 0);
      assert.deepEqual(manifest.config?.record_key_fields, ["field", "source_url"]);
      assert.equal(manifest.output_schema?.schema_key, "prospect.v1");
      assert.deepEqual(manifest.output_schema?.record_key_fields, ["field", "source_url"]);
      assert.ok(!seenKeys.has(manifest.target_key), `duplicate target_key ${manifest.target_key}`);
      seenKeys.add(manifest.target_key);
      assert.ok(script.endsWith("/scripts/v1.js") || script === sharedScript);
      assert.equal(fixture.input.source_id, targetName);

      const success = runFixture(targetName, fixturePath, "success");
      assert.ok(Array.isArray(success.records), `${targetName} must emit records[]`);
      assert.ok(success.records.length > 0, `${targetName} fixture produced no records`);
      for (const record of success.records) {
        assert.ok(FIELD_KEYS.has(record.field), `${targetName} emitted untyped field ${record.field}`);
        assert.ok(String(record.value || "").trim(), `${targetName}/${record.field} has no value`);
        assert.ok(["low", "medium", "high", "user_provided"].includes(record.confidence));
        assert.doesNotThrow(() => new URL(record.source_url), `${targetName}/${record.field} has invalid source_url`);
      }
      for (const [field, expectedValue] of Object.entries(fixture.expected)) {
        assert.ok(
          success.records.some((record) => record.field === field && record.value === expectedValue),
          `${targetName} missing ${field}=${expectedValue}: ${JSON.stringify(success.records)}`,
        );
      }

      for (const mode of ["identity_mismatch", "portal", "login"]) {
        const rejected = runFixture(targetName, fixturePath, mode);
        assert.deepEqual(rejected.records, [], `${targetName} accepted ${mode} evidence`);
      }
    });
  }
});

test("protected research adapters use secret references and Browser-App handoff", async (t) => {
  for (const targetName of PROTECTED_TARGETS) {
    await t.test(targetName, () => {
      const fixturePath = path.join(fixturesDir, `${targetName}.json`);
      const fixture = loadJson(fixturePath);
      assert.equal(containsForbiddenSecretKey(fixture), false, `${targetName} fixture contains a credential value`);
      assert.match(fixture.input.credential_ref, /^ctox-secret:\/\/credentials\/[A-Z0-9_]+$/);

      const { result, calls } = executeFixture(targetName, fixturePath, "auth_required");
      assert.deepEqual(result.records, [], `${targetName} fabricated records without a login`);
      assert.equal(result.failure_mode, "authorization_required");
      assert.equal(result.browser_assist_requested, true);

      const reauthorization = result.reauthorization;
      assert.ok(reauthorization, `${targetName} did not persist a reauthorization action`);
      assert.equal(reauthorization.kind, "auth-assist-request");
      assert.equal(reauthorization.source_id, targetName);
      assert.equal(reauthorization.login_url, fixture.login_url);
      for (const domain of fixture.browser_allowed_domains) {
        assert.ok(
          reauthorization.allowed_domains.some((allowed) =>
            domain === allowed || domain.endsWith(`.${allowed}`)),
          `${targetName} reauthorization allow-list ${JSON.stringify(reauthorization.allowed_domains)} does not cover ${domain}`,
        );
      }
      assert.equal(reauthorization.credential_ref, fixture.input.credential_ref);
      assert.equal(reauthorization.reason, "session_expired_or_invalid");
      assert.equal(reauthorization.secret_value_in_payload, false);
      assert.equal(containsForbiddenSecretKey(result), false, `${targetName} result contains a credential value`);

      const handoff = calls.find((args) => args[0] === "business-os" && args.includes("auth-assist-request"));
      assert.ok(handoff, `${targetName} did not open a Browser-App authorization request`);
      assert.equal(flagValue(handoff, "--credential-ref"), fixture.input.credential_ref);
      assert.equal(flagValue(handoff, "--target-url"), fixture.login_url);
      assert.equal(flagValue(handoff, "--task-id"), fixture.input.task_id);
      assert.ok(!calls.flat().some((arg) => /(?:password|passwd)=/i.test(String(arg))));
    });
  }
});

test("expired-session login landing stays distinguishable from genuine portal drift", async (t) => {
  for (const targetName of PROTECTED_TARGETS) {
    await t.test(targetName, () => {
      const fixturePath = path.join(fixturesDir, `${targetName}.json`);
      // Genuine drift/input failure: no usable input at all. The script must
      // NOT cry reauthorization without evidence of a login landing; the
      // native executor upgrades this to authorization_required only when the
      // portal probe actually lands on the source's own login page.
      const { result, calls } = executeFixture(targetName, fixturePath, "success", {});
      assert.equal(result.failure_mode, "portal_drift");
      assert.deepEqual(result.records, []);
      assert.equal(result.reauthorization, undefined);
      assert.equal(result.browser_assist_requested, undefined);
      assert.ok(!calls.some((args) => args[0] === "business-os" && args.includes("auth-assist-request")),
        `${targetName} emitted an auth handoff without a login landing`);
    });
  }
});

test("public sources without credentials never emit auth handoffs", async (t) => {
  for (const targetName of ["zefix.ch", "experte.de", "bundesanzeiger.de"]) {
    await t.test(targetName, () => {
      const fixturePath = path.join(fixturesDir, `${targetName}.json`);
      for (const mode of ["success", "blocked", "auth_required"]) {
        const { result, calls } = executeFixture(targetName, fixturePath, mode);
        assert.equal(result.reauthorization, undefined, `${targetName}/${mode} emitted a reauthorization action`);
        assert.notEqual(result.failure_mode, "authorization_required", `${targetName}/${mode} claimed reauthorization`);
        assert.ok(!calls.some((args) => args[0] === "business-os" && args.includes("auth-assist")),
          `${targetName}/${mode} opened an auth-assist handoff without a protected config`);
      }
    });
  }
});

test("protected providers resume capture after secret-backed Browser-App login", async (t) => {
  for (const targetName of ["dnbhoovers.com", "leadfeeder.com", "linkedin.com", "rocketreach.com", "xing.com"]) {
    await t.test(targetName, () => {
      const fixturePath = path.join(fixturesDir, `${targetName}.json`);
      const fixture = loadJson(fixturePath);
      const { result, calls } = executeFixture(targetName, fixturePath, "auth_recovery");
      for (const [field, expectedValue] of Object.entries(fixture.expected)) {
        assert.ok(result.records.some((record) => record.field === field && record.value === expectedValue));
      }
      const login = calls.find((args) => args[0] === "business-os" && args.includes("auth-assist-login"));
      assert.ok(login, `${targetName} did not run the native secret-backed login`);
      assert.equal(flagValue(login, "--credential-ref"), fixture.input.credential_ref);
      const captures = calls.filter((args) => args[0] === "business-os" && args.includes("source-capture"));
      assert.equal(captures.length, 2);
      assert.match(flagValue(captures[1], "--session-id"), /^browser_session_fixture_/);
    });
  }
});

test("protected captures retry transient provider failures after secret-backed login", () => {
  const targetName = "dnbhoovers.com";
  const fixturePath = path.join(fixturesDir, `${targetName}.json`);
  const fixture = loadJson(fixturePath);
  const { result, calls } = executeFixture(targetName, fixturePath, "capture_retry");
  assert.ok(result.records.some((record) => record.field === "firma_name"
    && record.value === fixture.expected.firma_name));
  assert.ok(calls.some((args) => args[0] === "business-os" && args.includes("auth-assist-login")));
  assert.equal(calls.filter((args) => args[0] === "business-os" && args.includes("source-capture")).length, 2);
});

test("RocketReach keeps exact public provider evidence while protected fields await authorization", () => {
  const targetName = "rocketreach.com";
  const fixturePath = path.join(fixturesDir, `${targetName}.json`);
  const fixture = loadJson(fixturePath);
  const { result, calls } = executeFixture(targetName, fixturePath, "provider_page_blocked");
  assert.ok(result.records.some((record) => record.field === "firma_name"
    && record.value === fixture.expected.firma_name));
  assert.equal(result.partial, true);
  assert.equal(result.protected_fields_require_authorization, true);
  assert.equal(result.browser_assist_requested, true);
  assert.ok(calls.some((args) => args[0] === "business-os" && args.includes("auth-assist-request")));
});

test("blocked protected adapters record Web-Unlock evidence and stay non-green", async (t) => {
  for (const targetName of PROTECTED_TARGETS) {
    await t.test(targetName, () => {
      const fixturePath = path.join(fixturesDir, `${targetName}.json`);
      const { result, calls } = executeFixture(targetName, fixturePath, "blocked");
      assert.deepEqual(result.records, []);
      assert.equal(result.failure_mode, "blocked");
      assert.equal(result.browser_assist_requested, true);
      const unlock = calls.find((args) => args[0] === "web" && args[1] === "unlock"
        && args[2] === "signals" && args[3] === "record");
      assert.ok(unlock, `${targetName} did not record a Web-Unlock signal`);
      const evidence = JSON.parse(flagValue(unlock, "--evidence"));
      assert.equal(evidence.source_id, targetName);
      assert.equal(evidence.secret_value_in_payload, false);
      assert.ok(calls.some((args) => args[0] === "business-os" && args.includes("auth-assist-request")));
    });
  }
});

test("blocked public adapters record Web-Unlock evidence and stay non-green", async (t) => {
  for (const targetName of PUBLIC_UNLOCK_TARGETS) {
    await t.test(targetName, () => {
      const fixturePath = path.join(fixturesDir, `${targetName}.json`);
      const { result, calls } = executeFixture(targetName, fixturePath, "blocked");
      assert.deepEqual(result.records, [], `${targetName} fabricated records while blocked`);
      assert.notEqual(result.failure_mode, "succeeded");

      const unlock = calls.find((args) => args[0] === "web" && args.includes("unlock") && args.includes("record"));
      assert.ok(unlock, `${targetName} did not record a Web-Unlock signal`);
      const evidence = JSON.parse(flagValue(unlock, "--evidence"));
      assert.equal(evidence.source_id, targetName);
      assert.equal(evidence.secret_value_in_payload, false);
    });
  }
});

test("Leadfeeder native API adapter uses registry search from minimal scrape input", () => {
  const fixturePath = path.join(fixturesDir, "leadfeeder.com.native-api.json");
  const fixture = loadJson(fixturePath);
  const minimalInput = {
    source_id: "leadfeeder.com",
    company: fixture.input.company,
    country: fixture.input.country,
  };
  const { result, calls } = executeFixture("leadfeeder.com", fixturePath, "success", minimalInput);
  for (const [field, expectedValue] of Object.entries(fixture.expected)) {
    assert.ok(
      result.records.some((record) => record.field === field && record.value === expectedValue),
      `minimal input missing ${field}=${expectedValue}: ${JSON.stringify(result.records)}`,
    );
  }
  const search = calls.find((args) => args[0] === "web" && args[1] === "search");
  assert.ok(search, "minimal scrape input did not reach native web search");
  assert.equal(flagValue(search, "--source"), "leadfeeder.com");
  assert.ok(!calls.some((args) => args[0] === "business-os" && args.includes("source-capture")));
});

test("Leadfeeder native API adapter dispatches search without browser capture", () => {
  const fixturePath = path.join(fixturesDir, "leadfeeder.com.native-api.json");
  const fixture = loadJson(fixturePath);
  assert.equal(fixture.extracted_fields_fixture, "synthetic_pending_serializer_regression");
  const nativeFields = fixture.search?.results?.[0]?.extracted_fields;
  assert.ok(Array.isArray(nativeFields), "native extracted_fields must be an array");
  assert.ok(nativeFields.every((field) => field.field && field.value && field.source_url));
  const { result, calls } = executeFixture("leadfeeder.com", fixturePath, "success", fixture.input);
  for (const [field, expectedValue] of Object.entries(fixture.expected)) {
    assert.ok(
      result.records.some((record) => record.field === field && record.value === expectedValue),
      `missing ${field}=${expectedValue}: ${JSON.stringify(result.records)}`,
    );
  }
  assert.ok(result.records.every((record) => record.source_url.includes("api.leadfeeder.com/v1/companies/")));
  assert.ok(result.records.every((record) => record.source_id === "leadfeeder.com"));
  const search = calls.find((args) => args[0] === "web" && args[1] === "search");
  assert.ok(search, "native API path did not call ctox web search");
  assert.equal(flagValue(search, "--source"), "leadfeeder.com");
  assert.ok(!calls.some((args) => args[0] === "business-os" && args.includes("source-capture")));
  assert.ok(!calls.some((args) => args[0] === "business-os" && args.includes("auth-assist")));
  assert.notEqual(result.browser_assist_requested, true);
});

test("Leadfeeder native API adapter preserves deliberate browser mode", () => {
  const browserFixture = path.join(fixturesDir, "leadfeeder.com.json");
  const fixture = loadJson(browserFixture);
  const browserInput = {
    ...fixture.input,
    access_mode: "authenticated_browser",
    credential_ref: "ctox-secret://credentials/LEADFEEDER_API_KEY",
  };
  const { result, calls } = executeFixture("leadfeeder.com", browserFixture, "success", browserInput);
  assert.ok(result.records.some((record) => record.field === "firma_name"
    && record.value === fixture.expected.firma_name));
  assert.ok(calls.some((args) => args[0] === "business-os" && args.includes("source-capture")));
  assert.ok(!calls.some((args) => args[0] === "web" && args[1] === "search"));
});

test("Leadfeeder native API failures stay distinguishable and skip browser assist", async (t) => {
  const fixturePath = path.join(fixturesDir, "leadfeeder.com.native-api.json");
  const fixture = loadJson(fixturePath);
  const expected = {
    credential_missing: "credential_missing",
    no_match: "no_match",
    rate_limited: "rate_limited",
    parse_failed: "parse_failed",
    account_selection_required: "account_selection_required",
    entitlement: "entitlement",
    identity_mismatch: "no_match",
    echoed_secret: "entitlement",
  };
  for (const [mode, failureMode] of Object.entries(expected)) {
    await t.test(mode, () => {
      const { result, calls } = executeFixture("leadfeeder.com", fixturePath, mode, fixture.input);
      assert.deepEqual(result.records, []);
      assert.equal(result.failure_mode, failureMode, `${mode} => ${JSON.stringify(result)}`);
      assert.equal(result.browser_assist_requested, false);
      assert.ok(!calls.some((args) => args[0] === "business-os"));
      assert.equal(containsForbiddenSecretKey(result), false);
      assert.equal(
        JSON.stringify(result).includes("FIXTURE_LEADFEEDER_KEY_DO_NOT_LEAK"),
        false,
        `${mode} leaked provider/config secret material: ${JSON.stringify(result)}`,
      );
    });
  }
});
