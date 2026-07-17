#!/usr/bin/env node

import { appendFileSync, readFileSync } from "node:fs";

const fixturePath = process.env.CTOX_SCRAPE_FIXTURE;
if (!fixturePath) throw new Error("CTOX_SCRAPE_FIXTURE is required");

const fixture = JSON.parse(readFileSync(fixturePath, "utf8"));
const args = process.argv.slice(2);
const mode = process.env.CTOX_SCRAPE_FIXTURE_MODE || "success";
const expectedCompany = String(fixture.input?.company || "");
const foreignCompany = "Fremdwerk AG";
const callLog = process.env.CTOX_SCRAPE_CALL_LOG;

if (callLog) {
  appendFileSync(callLog, `${JSON.stringify({ args })}\n`, { mode: 0o600 });
}

function deepReplace(value, from, to) {
  if (typeof value === "string") return value.split(from).join(to);
  if (Array.isArray(value)) return value.map((item) => deepReplace(item, from, to));
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [key, deepReplace(item, from, to)]),
    );
  }
  return value;
}

function modeResponse(value) {
  const response = structuredClone(value);
  if (mode === "identity_mismatch") {
    const replaced = deepReplace(response, expectedCompany, foreignCompany);
    if (fixture.input?.email && replaced?.result?.email) {
      replaced.result.email = "other@example.invalid";
    }
    return replaced;
  }
  if (mode === "portal" || mode === "login") {
    const title = `${mode === "portal" ? "Portal" : "Login"} | ${fixture.input.source_id}`;
    if (response && typeof response === "object") {
      response.title = title;
      if (response.result && typeof response.result === "object") {
        response.result.title = title;
      }
      if (Array.isArray(response.results)) {
        response.results = response.results.map((hit) => ({ ...hit, title }));
      }
    }
  }
  return response;
}

function flagValue(name) {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
}

let response;
if (args[0] === "business-os" && args.includes("source-capture")) {
  const captureCalls = callLog
    ? readFileSync(callLog, "utf8").trim().split("\n").filter(Boolean)
      .map((line) => JSON.parse(line).args)
      .filter((call) => call[0] === "business-os" && call.includes("source-capture"))
      .length
    : 1;
  response = mode === "auth_recovery" && captureCalls > 1
    ? modeResponse(fixture.capture || { ok: false, source_status: "failed", records: [] })
    : mode === "blocked"
      ? { ok: false, source_status: "blocked", source_url: fixture.login_url, records: [] }
      : ["portal", "login", "auth_required", "auth_recovery"].includes(mode)
    ? { ok: false, source_status: "auth_required", records: [] }
    : modeResponse(fixture.capture || { ok: false, source_status: "failed", records: [] });
} else if (args[0] === "business-os" && args.includes("auth-assist-login")) {
  response = mode === "auth_recovery"
    ? {
        ok: true,
        status: "completed",
        session_id: `browser_session_fixture_${fixture.input.source_id}`,
        source_id: fixture.input.source_id,
        target_url: fixture.login_url,
        credential_ref: flagValue("--credential-ref"),
        secret_value_in_payload: false,
        auth_assist_request: {
          allowed_domains: fixture.browser_allowed_domains,
          secret_value_in_payload: false,
        },
      }
    : {
        ok: false,
        status: mode === "blocked" ? "login_failed" : "auth_required",
        source_id: fixture.input.source_id,
        target_url: fixture.login_url,
        secret_value_in_payload: false,
      };
} else if (args[0] === "business-os" && args.includes("auth-assist-request")) {
  response = {
    ok: true,
    status: "pending_sync",
    source_id: fixture.input.source_id,
    target_url: fixture.login_url,
    allowed_domains: fixture.browser_allowed_domains,
    credential_ref: flagValue("--credential-ref"),
    session_id: `browser_session_fixture_${fixture.input.source_id}`,
    secret_value_in_payload: false,
  };
} else if (args[0] === "web" && args[1] === "browser-automation") {
  response = mode === "blocked"
    ? { ok: false, result: null, detection: { markers: ["fixture-access-challenge"] } }
    : modeResponse(fixture.browser || { ok: false, result: null });
} else if (args[0] === "web" && args[1] === "search") {
  response = mode === "blocked"
    ? { results: [], source_failures: [{ kind: "blocked" }] }
    : mode === "auth_required"
      ? { results: [], source_failures: [{ kind: "auth_required" }] }
      : modeResponse(fixture.search || { results: [] });
} else if (args[0] === "web" && args[1] === "read") {
  const url = flagValue("--url");
  response = mode === "blocked"
    ? { ok: false, url }
    : modeResponse(fixture.pages?.[url] || fixture.read || { ok: false, url });
} else if (args[0] === "web" && args[1] === "unlock" && args[2] === "signals") {
  response = { ok: true, recorded: true };
} else {
  process.stderr.write(`unsupported fixture command: ${args.join(" ")}\n`);
  process.exit(2);
}

process.stdout.write(JSON.stringify(response));
