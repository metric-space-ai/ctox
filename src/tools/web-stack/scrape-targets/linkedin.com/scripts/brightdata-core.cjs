"use strict";

// Provider contract only. The native runner must provide encrypted secret
// loading, durable compare-and-set state and a verified company-page binding.
// This file is not an executable replacement for the registered target yet.
const { createHash } = require("node:crypto");
const ORIGIN = "https://api.brightdata.com";
const DATASET = "gd_l1viktl72bvl7bjuj0";
const MAX_PROFILES = 3;
const MAX_SUBMISSIONS = 2;
const identity = value => typeof value === "string"
  ? value.normalize("NFKC").trim().replace(/\s+/g, " ").toLocaleLowerCase("de-DE") : "";

function canonicalLinkedIn(value, kind) {
  if (typeof value !== "string" || value.length > 2048) return null;
  try {
    const url = new URL(value);
    if (url.protocol !== "https:" || url.username || url.password || url.port ||
        !/^(?:www\.|[a-z]{2}\.)?linkedin\.com$/.test(url.hostname)) return null;
    // Do not decode slugs: encoded path separators must never become input URLs.
    const match = new RegExp(`^/${kind}/([A-Za-z0-9_-]{1,200})/?$`).exec(url.pathname);
    return match ? `https://www.linkedin.com/${kind}/${match[1]}/` : null;
  } catch { return null; }
}

function checkedQuery(input) {
  const company = typeof input?.company === "string" ? input.company.trim() : "";
  const country = typeof input?.country === "string" ? input.country.toUpperCase() : "";
  if (!company || company.length > 250 || /[\x00-\x1f\x7f]/.test(company) ||
      !["DE", "AT", "CH"].includes(country)) throw new Error("invalid_company_query");
  // Quoted content cannot inject a second search operator through quotes.
  const searchName = company.replace(/["\\]/g, " ");
  return { company, country, query: `"${searchName}" site:linkedin.com/in/` };
}

async function discoverProfiles(input, search) {
  const query = checkedQuery(input);
  let payload;
  try { payload = await search(query); }
  catch { return { ok: false, code: "discovery_unavailable", ...query, urls: [] }; }
  if (!payload || payload.ok === false || !Array.isArray(payload.results) ||
      !Array.isArray(payload.source_failures) || payload.source_failures.length ||
      typeof payload.provider !== "string" || !payload.provider.trim()) {
    return { ok: false, code: "discovery_incomplete", ...query, urls: [] };
  }
  // Search snippets are discovery only, never employer/person field evidence.
  const urls = [...new Set(payload.results.slice(0, 20)
    .map(hit => canonicalLinkedIn(hit?.url, "in")).filter(Boolean))].slice(0, MAX_PROFILES);
  return { ok: urls.length > 0, code: urls.length ? "discovered" : "no_profiles_in_bounded_search",
    ...query, provider: payload.provider, urls, inspected_count: Math.min(payload.results.length, 20),
    truncated: payload.results.length > 20, evidence_eligible: false };
}

function collectionBinding(query, companyUrl, urls) {
  const checked = checkedQuery(query);
  const company_profile_url = canonicalLinkedIn(companyUrl, "company");
  if (!company_profile_url || !Array.isArray(urls) || !urls.length || urls.length > MAX_PROFILES)
    throw new Error("invalid_collection_binding");
  const profiles = urls.map(url => canonicalLinkedIn(url, "in"));
  if (profiles.some(url => !url) || new Set(profiles).size !== profiles.length)
    throw new Error("invalid_profile_urls");
  const binding = { company: checked.company, country: checked.country,
    company_profile_url, dataset_id: DATASET, urls: profiles.sort() };
  return { ...binding, query_hash: createHash("sha256").update(JSON.stringify(binding)).digest("hex") };
}

function extractProfiles(rows, binding) {
  if (!Array.isArray(rows) || rows.length > MAX_PROFILES) throw new Error("invalid_snapshot_records");
  const records = [], rejected = [], seen = new Set();
  for (const row of rows) {
    const url = canonicalLinkedIn(row?.url, "in");
    const inputUrl = canonicalLinkedIn(row?.input_url, "in");
    const employer = row?.current_company;
    let reason = null;
    if (!row || row.error || row.error_code) reason = "provider_record_error";
    else if (!url || !inputUrl || url !== inputUrl || !binding.urls.includes(url)) reason = "profile_url_mismatch";
    else if (seen.has(url)) reason = "duplicate_profile";
    else if (canonicalLinkedIn(employer?.link, "company") !== binding.company_profile_url ||
      identity(employer?.name) !== identity(binding.company) ||
      (row.current_company_name != null && identity(row.current_company_name) !== identity(binding.company)))
      reason = "current_employer_mismatch";
    else if (![row.first_name, row.last_name].every(name => typeof name === "string" &&
      name.trim().length > 0 && name.length <= 200 && !/[\x00-\x1f*]/.test(name))) reason = "missing_structured_name";
    if (reason) { rejected.push({ profile_url: url, reason }); continue; }
    seen.add(url);
    for (const [field, value] of [["person_vorname", row.first_name], ["person_nachname", row.last_name],
      ["person_position", row.position], ["person_linkedin", url]]) {
      if (typeof value !== "string" || !value.trim() || value.length > 1000) continue;
      if (field === "person_position" && /[\x00-\x1f\x7f*]/.test(value)) continue;
      records.push({ field, value: value.trim(), source_url: url, source_id: "linkedin.com",
        source_key: "primary", confidence: "high", provider_record_id: url,
        note: "BrightData public profile; exact current employer name and company URL matched." });
    }
  }
  // Gender, honorifics, email addresses, past-employer roles and name splitting
  // are deliberately not invented from unrelated provider attributes.
  return { records, rejected, matched_profiles: seen.size,
    missing_profiles: binding.urls.filter(url => !rows.some(row => canonicalLinkedIn(row?.input_url, "in") === url)) };
}

async function boundedJson(response) {
  const reader = response.body?.getReader();
  if (!reader) throw new Error("missing_response_body");
  let size = 0;
  const chunks = [];
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > 2 * 1024 * 1024) { await reader.cancel(); throw new Error("response_too_large"); }
    chunks.push(Buffer.from(value));
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

function failure(code, mode = "blocked", extra = {}) {
  return { schema: "prospect.v1", provider: "linkedin.com", records: [], failure_mode: mode,
    error_code: code, ...extra };
}

// One bounded transition per call. The caller must atomically claim the
// operation before POST, and persist every state before resuming it. A state
// left at submitting means ambiguous provider acceptance, NOT permission to
// POST again. No secret is ever passed to saveState or placed in an outcome.
async function advanceCollection(binding, state, dependencies) {
  const { fetch: request, loadSecret, saveState, claimSubmission } = dependencies;
  let checked;
  try { checked = collectionBinding(binding, binding.company_profile_url, binding.urls); }
  catch { return failure("invalid_collection_binding"); }
  if (checked.query_hash !== binding.query_hash || (state && state.query_hash !== checked.query_hash))
    return failure("checkpoint_query_mismatch");
  if (state?.phase === "submitting") return failure("submission_outcome_unknown");
  if (state && !["rejected", "pending", "ready", "completed"].includes(state.phase)) return failure("checkpoint_phase_invalid");
  const priorAttempt = state?.submission_attempt ?? 1;
  if (!Number.isInteger(priorAttempt) || priorAttempt < 1 || priorAttempt > MAX_SUBMISSIONS)
    return failure("checkpoint_attempt_invalid");
  if (state?.phase === "rejected" && state.error_code !== "api_unauthorized") return failure("checkpoint_rejection_invalid");
  if (state?.phase === "rejected" && priorAttempt >= MAX_SUBMISSIONS) return failure("reauthorization_budget_exhausted", "auth_required");
  const submittingNow = !state || state.phase === "rejected";
  const attempt = state?.phase === "rejected" ? priorAttempt + 1 : priorAttempt;
  if (state && !submittingNow && !/^(?:sd|s)_[A-Za-z0-9]{1,100}$/.test(state.snapshot_id || ""))
    return failure("checkpoint_snapshot_invalid");
  let secret;
  try {
    secret = await loadSecret();
    if (typeof secret !== "string" || !secret.trim() || secret.length > 2048 || /[\r\n]/.test(secret))
      throw new Error("invalid_secret");
  } catch { return failure("credential_unavailable", "auth_required"); }
  const receipt = { company: checked.company, country: checked.country, query_hash: checked.query_hash,
    dataset_id: DATASET, profile_urls: checked.urls, snapshot_id: state?.snapshot_id || null };
  try {
    const headers = { Authorization: `Bearer ${secret}`, "Content-Type": "application/json", accept: "application/json" };
    let endpoint, method = "GET", body;
    if (submittingNow) {
      const submitting = { phase: "submitting", query_hash: checked.query_hash, binding: checked, submission_attempt: attempt };
      if (!await claimSubmission(submitting)) return failure("collection_already_claimed");
      endpoint = `/datasets/v3/trigger?dataset_id=${DATASET}&include_errors=true&limit_per_input=1&limit_multiple_results=${MAX_PROFILES}`;
      method = "POST";
      body = JSON.stringify(checked.urls.map(url => ({ url })));
    } else {
      endpoint = state.phase === "pending" ? `/datasets/v3/progress/${state.snapshot_id}`
        : `/datasets/v3/snapshot/${state.snapshot_id}?format=json`;
    }
    let response;
    try { response = await request(ORIGIN + endpoint, { method, headers, body, redirect: "error", signal: AbortSignal.timeout(20_000) }); }
    catch { return failure(submittingNow ? "submission_outcome_unknown" : "api_transport_failed", "temporary_unreachable", { api_query_evidence: receipt }); }
    if (response.status === 401) {
      if (submittingNow) await saveState({ phase: "rejected", query_hash: checked.query_hash,
        binding: checked, submission_attempt: attempt, error_code: "api_unauthorized" });
      return failure("api_unauthorized", "auth_required", { api_query_evidence: receipt });
    }
    if (response.status === 403) return failure("api_forbidden", "blocked", { api_query_evidence: receipt });
    if (response.status !== 200 && response.status !== 202)
      return failure("api_unexpected_status", "temporary_unreachable", { api_query_evidence: receipt });
    let payload;
    try { payload = await boundedJson(response); }
    catch { return failure(submittingNow ? "submission_outcome_unknown" : "api_invalid_response", "portal_drift", { api_query_evidence: receipt }); }
    if (submittingNow) {
      if (!/^(?:sd|s)_[A-Za-z0-9]{1,100}$/.test(payload?.snapshot_id || ""))
        return failure("submission_outcome_unknown", "portal_drift", { api_query_evidence: receipt });
      const next = { ...receipt, snapshot_id: payload.snapshot_id, phase: "pending", binding: checked, submission_attempt: attempt };
      await saveState(next);
      return failure("collection_pending", "temporary_unreachable", { api_query_evidence: { ...receipt, snapshot_id: payload.snapshot_id }, continuation: next });
    }
    if (state.phase === "pending") {
      if (payload?.snapshot_id !== state.snapshot_id || payload?.dataset_id !== DATASET)
        return failure("progress_identity_mismatch", "portal_drift", { api_query_evidence: receipt });
      if (!["starting", "running", "ready"].includes(payload.status))
        return failure("provider_collection_failed", "blocked", { api_query_evidence: receipt });
      const next = { ...state, phase: payload.status === "ready" ? "ready" : "pending" };
      await saveState(next);
      return failure("collection_pending", "temporary_unreachable", { api_query_evidence: receipt, continuation: next });
    }
    if (response.status !== 200) return failure("snapshot_not_ready", "temporary_unreachable", { api_query_evidence: receipt });
    let extracted;
    try { extracted = extractProfiles(payload, checked); }
    catch { return failure("snapshot_invalid", "portal_drift", { api_query_evidence: receipt }); }
    const evidence = { ...receipt, returned_profiles: payload.length, matched_profiles: extracted.matched_profiles,
      rejected: extracted.rejected, missing_profiles: extracted.missing_profiles };
    await saveState({ ...state, phase: "completed" });
    return { schema: "prospect.v1", provider: "linkedin.com", records: extracted.records,
      api_query_evidence: evidence, ...(extracted.records.length && !extracted.rejected.length && !extracted.missing_profiles.length ? {}
        : { failure_mode: "partial_output", error_code: "profile_evidence_incomplete" }) };
  } catch { return failure("checkpoint_unavailable"); }
  finally { secret = ""; }
}

module.exports = { canonicalLinkedIn, checkedQuery, discoverProfiles, collectionBinding, extractProfiles, advanceCollection, DATASET };
