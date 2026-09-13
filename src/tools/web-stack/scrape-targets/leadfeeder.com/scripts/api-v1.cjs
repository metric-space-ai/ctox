"use strict";

// Hot-registered native scraper, separate from the legacy visitor-leads API.
// Contract: https://docs.leadfeeder.com/api/public/search-companies-4008955e0
// Company search is credit-free. Never retrieve deep data or follow redirects.
const { execFileSync } = require("node:child_process");
const { readFileSync } = require("node:fs");
const path = require("node:path");

const ENDPOINT = "https://api.leadfeeder.com/v1/companies/search";
const PROVIDER = "leadfeeder.com";

function fail(mode, code) {
  return { schema: "prospect.v1", provider: PROVIDER, records: [], failure_mode: mode, error_code: code };
}

function checkedConfig(config) {
  const match = /^ctox-secret:\/\/credentials\/([A-Z][A-Z0-9_]{0,127})$/.exec(config.credential_ref || "");
  if (!match || !/^\d{1,20}$/.test(String(config.account_id || "")) ||
      typeof config.native_root !== "string" || !path.isAbsolute(config.native_root)) {
    throw new Error("invalid_api_configuration");
  }
  return { ...config, secret_name: match[1], account_id: String(config.account_id) };
}

function loadNativeSecret(config) {
  // Only the operator-owned target manifest selects this reference/root.
  // Query input cannot choose a secret, executable, account or destination.
  let output = execFileSync(process.env.CTOX_BIN || "ctox", [
    "secret", "get", "--scope", "credentials", "--name", config.secret_name,
    "--root", config.native_root,
  ], { encoding: "utf8", timeout: 10_000, maxBuffer: 16_384, stdio: ["ignore", "pipe", "pipe"] });
  const secret = JSON.parse(output);
  output = "";
  if (secret.ok !== true || secret.scope !== "credentials" || secret.name !== config.secret_name ||
      typeof secret.value !== "string" || !secret.value.trim() || secret.value.length > 2048 || /[\r\n]/.test(secret.value)) {
    throw new Error("credential_unavailable");
  }
  return secret.value;
}

function identity(value) {
  return String(value || "").normalize("NFKC").trim().replace(/\s+/g, " ").toLocaleLowerCase("de-DE");
}

function companyRecords(record, sourceUrl) {
  const attrs = record.attributes;
  const records = [];
  function add(field, value, note = "") {
    if (typeof value === "string" && value.trim()) records.push({
      field, value: value.trim(), source_url: sourceUrl, source_key: "primary",
      source_id: PROVIDER, confidence: "high", note,
      provider_record_id: record.id,
    });
  }
  add("firma_name", attrs.name);
  add("firma_land", attrs.address?.country_code);
  add("firma_ort", attrs.address?.city);
  add("mitarbeiter", attrs.employee_range, "Reported employee range, not an exact headcount.");
  if (typeof attrs.url === "string") {
    try {
      const website = new URL(attrs.url);
      if (["https:", "http:"].includes(website.protocol) && !website.username && !website.password) {
        add("firma_domain", website.hostname.replace(/^www\./, ""));
      }
    } catch { /* Missing or malformed website is not evidence. */ }
  }
  // Industry labels are not silently relabelled as WZ codes. No contact data
  // is fabricated from administrative fields or has_mail/has_contacts flags.
  return records;
}

async function search(input, rawConfig, dependencies = {}) {
  let config;
  try { config = checkedConfig(rawConfig); } catch { return fail("blocked", "invalid_api_configuration"); }
  const company = typeof input.company === "string" ? input.company.trim() : "";
  const country = String(input.country || "").toUpperCase();
  if (!company || company.length > 250 || !["DE", "AT", "CH"].includes(country)) {
    return fail("blocked", "company_and_dach_country_required");
  }
  let key;
  try { key = (dependencies.loadSecret || loadNativeSecret)(config); }
  catch { return fail("auth_required", "credential_unavailable"); }
  let response;
  try {
    const url = new URL(ENDPOINT);
    url.searchParams.set("account_id", config.account_id);
    url.searchParams.set("page[size]", "5");
    response = await (dependencies.fetch || globalThis.fetch)(url, {
      method: "POST", redirect: "error", signal: AbortSignal.timeout(15_000),
      headers: { "X-Api-Key": key, "Content-Type": "application/json", accept: "application/json" },
      body: JSON.stringify({ search_terms: [company], locations: [{ country_code: country }] }),
    });
  } catch { return fail("temporary_unreachable", "api_transport_failed"); }
  finally { key = ""; }
  if (response.status === 401) return fail("auth_required", "api_unauthorized");
  if (response.status === 403) return fail("blocked", "api_forbidden");
  if (response.status === 429 || response.status >= 500) return fail("temporary_unreachable", "api_retryable_status");
  if (response.status !== 200) return fail("portal_drift", "api_unexpected_status");
  let payload;
  try {
    const reader = response.body.getReader();
    const chunks = [];
    let size = 0;
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > 1_048_576) { await reader.cancel(); throw new Error("oversize"); }
      chunks.push(Buffer.from(value));
    }
    payload = JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch { return fail("portal_drift", "api_invalid_response"); }
  if (!Array.isArray(payload?.data) || payload.data.length > 5 ||
      !payload.data.every(record => record && typeof record === "object" && !Array.isArray(record)) ||
      typeof payload.meta?.request_id !== "string" || !payload.meta.request_id) {
    return fail("portal_drift", "api_invalid_envelope");
  }
  if (payload.meta?.credits?.charged !== 0) return fail("blocked", "credit_free_search_not_confirmed");
  const matches = payload.data.filter(record => record.type === "company_summary" &&
    typeof record.id === "string" && /^[A-Za-z0-9_-]+$/.test(record.id) &&
    record.attributes?.address?.country_code === country && identity(record.attributes?.name) === identity(company));
  const evidence = {
    method: "POST", endpoint: ENDPOINT, account_id: config.account_id, company, country,
    request_id: payload.meta.request_id, credits_charged: 0,
    returned_count: payload.data.length, matched_count: matches.length,
    candidates: payload.data.map(record => ({
      type: record.type, id: record.id, name: record.attributes?.name,
      country: record.attributes?.address?.country_code,
    })),
    has_more: Boolean(payload.meta.pagination?.next_cursor),
    queried_at: new Date().toISOString(),
  };
  if (evidence.has_more) return {
    ...fail("partial_output", "bounded_page_incomplete"), api_query_evidence: evidence,
  };
  if (!matches.length) return {
    ...fail("partial_output", "no_exact_company_in_bounded_page"), api_query_evidence: evidence,
  };
  if (matches.length !== 1) return { ...fail("partial_output", "ambiguous_company_identity"), api_query_evidence: evidence };
  const sourceUrl = `${ENDPOINT}?account_id=${config.account_id}#company-${matches[0].id}`;
  return {
    schema: "prospect.v1", provider: PROVIDER,
    records: companyRecords(matches[0], sourceUrl), api_query_evidence: evidence,
    credential_ref: config.credential_ref, secret_value_in_payload: false,
  };
}

async function main() {
  try {
    const manifest = JSON.parse(readFileSync(process.env.CTOX_SCRAPE_MANIFEST_PATH, "utf8"));
    const input = JSON.parse(process.env.CTOX_SCRAPE_INPUT_JSON || "{}");
    const result = await search(input, manifest.config || {});
    process.stdout.write(JSON.stringify(result) + "\n");
    if (result.failure_mode) process.exitCode = 1;
  } catch {
    // Never print native stderr, provider bodies, headers or exception objects.
    process.stdout.write(JSON.stringify(fail("blocked", "adapter_configuration_unavailable")) + "\n");
    process.exitCode = 1;
  }
}

module.exports = { search, checkedConfig, companyRecords };
if (require.main === module) main();
