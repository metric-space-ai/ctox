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
const PROTECTED_TARGETS = ["dnbhoovers.com", "leadfeeder.com", "rocketreach.com"];

const FIELD_KEYS = new Set([
  "firma_name", "firma_anschrift", "firma_plz", "firma_ort", "firma_email",
  "firma_domain", "firma_telefon", "wz_code", "umsatz", "mitarbeiter",
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

function executeFixture(targetName, fixturePath, mode) {
  const fixture = loadJson(fixturePath);
  const outputDir = mkdtempSync(path.join(tmpdir(), `ctox-scrape-${targetName}-`));
  const callLog = path.join(outputDir, "ctox-calls.jsonl");
  try {
    const child = spawnSync(process.execPath, [resolvedScript(targetName)], {
      cwd: targetsDir,
      encoding: "utf8",
      timeout: 10_000,
      env: {
        ...process.env,
        CTOX_BIN: mockCtox,
        CTOX_SCRAPE_FIXTURE: fixturePath,
        CTOX_SCRAPE_FIXTURE_MODE: mode,
        CTOX_SCRAPE_CALL_LOG: callLog,
        CTOX_SCRAPE_INPUT_JSON: JSON.stringify(fixture.input),
        CTOX_SCRAPE_OUTPUT_DIR: outputDir,
      },
    });
    assert.equal(child.signal, null, `${targetName}/${mode} timed out`);
    assert.equal(child.status, 0, `${targetName}/${mode}: ${child.stderr || child.stdout}`);
    assert.doesNotThrow(() => JSON.parse(child.stdout), `${targetName}/${mode} returned invalid JSON`);
    const calls = readFileSync(callLog, "utf8").trim().split("\n")
      .filter(Boolean)
      .map((line) => JSON.parse(line).args);
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

test("all 14 DACH research scrape targets pass production-like fixture gates", async (t) => {
  const targets = targetDirectories();
  assert.equal(targets.length, 14, `expected 14 targets, found: ${targets.join(", ")}`);

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
      assert.equal(result.failure_mode, "auth_required");
      assert.equal(result.browser_assist_requested, true);

      const handoff = calls.find((args) => args[0] === "business-os" && args.includes("auth-assist-request"));
      assert.ok(handoff, `${targetName} did not open a Browser-App authorization request`);
      assert.equal(flagValue(handoff, "--credential-ref"), fixture.input.credential_ref);
      assert.equal(flagValue(handoff, "--target-url"), fixture.login_url);
      assert.equal(flagValue(handoff, "--task-id"), fixture.input.task_id);
      assert.ok(!calls.flat().some((arg) => /(?:password|passwd)=/i.test(String(arg))));
    });
  }
});

test("D&B and Leadfeeder resume capture after secret-backed Browser-App login", async (t) => {
  for (const targetName of ["dnbhoovers.com", "leadfeeder.com"]) {
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
