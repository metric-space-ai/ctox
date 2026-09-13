"use strict";

const path = require("node:path");
const { createHash } = require("node:crypto");
const { checkedQuery, collectionBinding, DATASET } = require("./brightdata-core.cjs");

// The native runner owns this context. Neither research input nor provider
// output may select a state directory, executable or credential reference.
function projectProviderWait(outcome, { rawInput, runDirectory, targetKey }) {
  const fail = () => ({ schema: "prospect.v1", provider: "linkedin.com", records: [],
    failure_mode: "portal_drift", error_code: "invalid_provider_continuation" });
  try {
    if (outcome?.error_code !== "collection_pending" || outcome.failure_mode !== "temporary_unreachable" ||
        outcome.query_completion != null || outcome.partial_output === true || outcome.error ||
        !Array.isArray(outcome.records) || outcome.records.length ||
        typeof rawInput !== "string" || Buffer.byteLength(rawInput) > 65536 ||
        typeof runDirectory !== "string" || !path.isAbsolute(runDirectory) || targetKey !== "linkedin-com") return fail();
    const input = JSON.parse(rawInput), query = checkedQuery(input);
    const operation = input.research_operation_id;
    const runId = path.basename(runDirectory);
    if (!/^research-v1-[a-f0-9]{64}$/.test(operation || "") || input.source_id !== "linkedin.com" ||
        !/^scrape_run-[A-Za-z0-9_-]{1,128}$/.test(runId)) return fail();
    const state = outcome.continuation;
    if (!state || !["pending", "ready"].includes(state.phase) ||
        !/^(?:sd|s)_[A-Za-z0-9]{1,100}$/.test(state.snapshot_id || "") ||
        !Number.isInteger(state.submission_attempt) || state.submission_attempt < 1 || state.submission_attempt > 2) return fail();
    const binding = collectionBinding(query, state.binding?.company_profile_url, state.binding?.urls);
    if (state.query_hash !== binding.query_hash || state.binding.query_hash !== binding.query_hash ||
        state.binding.company !== query.company || state.binding.country !== query.country || state.binding.dataset_id !== DATASET) return fail();
    return { schema: "prospect.v1", provider: "linkedin.com", records: [],
      failure_mode: "awaiting_provider", error_code: "collection_pending",
      continuation: { schema: "ctox.scrape.provider_continuation.v1", run_id: runId,
        input_sha256: createHash("sha256").update(rawInput).digest("hex"),
        operation_id: operation, source_id: "linkedin.com", target_key: targetKey,
        provider: "brightdata", company: query.company, country: query.country,
        dataset_id: DATASET, snapshot_id: state.snapshot_id, query_hash: binding.query_hash,
        phase: state.phase, submission_attempt: state.submission_attempt, retry_after_seconds: 15 },
      api_query_evidence: { company: query.company, country: query.country,
        query_hash: binding.query_hash, dataset_id: DATASET, profile_urls: binding.urls,
        snapshot_id: state.snapshot_id },
    };
  } catch { return fail(); }
}

module.exports = { projectProviderWait };
