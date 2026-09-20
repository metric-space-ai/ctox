"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { createHash } = require("node:crypto");
const { spawnSync } = require("node:child_process");
const core = require("./brightdata-core.cjs");
const { openCheckpoint, immutableReceipt, checkedDirectory } = require("./brightdata-state.cjs");
const { projectProviderWait } = require("./brightdata-continuation.cjs");
const digest = value => createHash("sha256").update(value).digest("hex");
const fail = (code, mode = "blocked", extra = {}) => ({ schema: "prospect.v1", provider: "linkedin.com",
  records: [], failure_mode: mode, error_code: code, ...extra });

function readFileBounded(file, limit) {
  const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
  try {
    const stat = fs.fstatSync(fd);
    if (!stat.isFile() || stat.size > limit) throw new Error("invalid_runner_file");
    const buffer = Buffer.alloc(limit + 1);
    let size = 0, read;
    do {
      read = fs.readSync(fd, buffer, size, buffer.length - size, null); size += read;
      if (size > limit) throw new Error("runner_file_too_large");
    } while (read);
    return buffer.subarray(0, size).toString("utf8");
  } finally { fs.closeSync(fd); }
}

function runnerContext(env) {
  if (env.CTOX_SCRAPE_TARGET_KEY !== "linkedin-com" || typeof env.CTOX_SCRAPE_INPUT_JSON !== "string" ||
      Buffer.byteLength(env.CTOX_SCRAPE_INPUT_JSON) > 65536) throw new Error("invalid_native_context");
  for (const key of ["CTOX_ROOT", "CTOX_BIN", "CTOX_SCRAPE_TARGET_DIR", "CTOX_SCRAPE_RUN_DIR", "CTOX_SCRAPE_MANIFEST_PATH"]) {
    if (typeof env[key] !== "string" || !path.isAbsolute(env[key])) throw new Error("invalid_native_context");
  }
  const target = fs.realpathSync(env.CTOX_SCRAPE_TARGET_DIR);
  const manifestPath = path.join(target, "manifest.json");
  if (fs.realpathSync(env.CTOX_SCRAPE_MANIFEST_PATH) !== manifestPath) throw new Error("invalid_manifest_path");
  const runDirectory = fs.realpathSync(env.CTOX_SCRAPE_RUN_DIR);
  if (path.dirname(runDirectory) !== path.join(target, "runs") ||
      !/^scrape_run-[A-Za-z0-9_-]{1,128}$/.test(path.basename(runDirectory))) throw new Error("invalid_run_path");
  const manifest = JSON.parse(readFileBounded(manifestPath, 262144));
  const config = manifest.config;
  if (manifest.target_key !== "linkedin-com" || config?.expected_provider !== "linkedin.com" ||
      config.async_provider !== "brightdata" || config.access_mode !== "provider_api") throw new Error("invalid_provider_manifest");
  const match = /^ctox-secret:\/\/credentials\/([A-Z][A-Z0-9_]{0,63})$/.exec(config.credential_ref || "");
  if (!match) throw new Error("invalid_credential_reference");
  checkedDirectory(target);
  const stateRoot = path.join(target, "brightdata-state");
  try {
    fs.mkdirSync(stateRoot, { mode: 0o700 });
    const fd = fs.openSync(target, fs.constants.O_RDONLY);
    try { fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
  }
  catch (error) { if (error.code !== "EEXIST") throw error; }
  checkedDirectory(stateRoot);
  return { rawInput: env.CTOX_SCRAPE_INPUT_JSON, targetKey: "linkedin-com", runDirectory, stateRoot,
    root: fs.realpathSync(env.CTOX_ROOT), executable: fs.realpathSync(env.CTOX_BIN),
    credentialName: match[1], collectionAuthorized: config.brightdata_collection_authorized === true };
}

function nativeJson(context, args, secret = false) {
  const result = spawnSync(context.executable, args, { cwd: context.root, env: { ...process.env, CTOX_ROOT: context.root },
    encoding: "utf8", timeout: 20000, killSignal: "SIGKILL", maxBuffer: secret ? 8192 : 2 * 1024 * 1024,
    stdio: ["ignore", "pipe", "pipe"] });
  try {
    if (result.error || result.signal || result.status !== 0) throw new Error("native_dependency_unavailable");
    return JSON.parse(result.stdout);
  } finally {
    // Captured only: never attach stdout/stderr or exception objects to a
    // failure envelope. JS strings cannot promise memory zeroization.
    result.stdout = ""; result.stderr = "";
  }
}

function nativeDependencies(context) {
  return {
    loadSecret: async () => {
      const result = nativeJson(context, ["secret", "get", "--scope", "credentials", "--name", context.credentialName], true);
      if (result?.ok !== true || result.scope !== "credentials" || result.name !== context.credentialName ||
          typeof result.value !== "string") throw new Error("credential_unavailable");
      return result.value;
    },
    search: async query => nativeJson(context, ["web", "search", "--query", query.query, "--country", query.country,
      "--domain", "linkedin.com", "--context-size", "low"]),
    fetch: globalThis.fetch,
  };
}

async function runBrightData(context, dependencies) {
  try {
    const input = JSON.parse(context.rawInput), query = core.checkedQuery(input);
    if (input.source_id !== "linkedin.com" || !/^research-v1-[a-f0-9]{64}$/.test(input.research_operation_id || ""))
      return fail("invalid_research_operation");
    if (!context.collectionAuthorized) return fail("collection_not_authorized");
    const inputHash = digest(context.rawInput);
    const canonicalPlan = value => {
      if (value?.input_sha256 !== inputHash) throw new Error("operation_input_changed");
      const company = core.companyCollectionBinding(query, value.company_urls);
      const profiles = core.collectionBinding(query, company.urls[0], value.profile_urls);
      const canonicalDiscovery = (evidence, suffix) => {
        const expectedQuery = query.query.replace("site:linkedin.com/in/", `site:linkedin.com/${suffix}/`);
        if (typeof evidence?.provider !== "string" || !evidence.provider.trim() || evidence.provider.length > 100 ||
            evidence.query !== expectedQuery || !Number.isInteger(evidence.inspected_count) ||
            evidence.inspected_count < 0 || evidence.inspected_count > 20 || typeof evidence.truncated !== "boolean")
          throw new Error("invalid_discovery_receipt");
        return { provider: evidence.provider, query: evidence.query, inspected_count: evidence.inspected_count, truncated: evidence.truncated };
      };
      return { input_sha256: inputHash, company_urls: company.urls, profile_urls: profiles.urls,
        company_discovery: canonicalDiscovery(value.company_discovery, "company"),
        profile_discovery: canonicalDiscovery(value.profile_discovery, "in") };
    };
    const planStore = immutableReceipt(context.stateRoot, input.research_operation_id, "discovery", canonicalPlan);
    let plan = planStore.load();
    if (!plan) {
      const company = await core.discoverCompanies(query, dependencies.search);
      if (!company.ok) return fail(company.code, "temporary_unreachable");
      const profile = await core.discoverProfiles(query, dependencies.search);
      if (!profile.ok) return fail(profile.code, "temporary_unreachable");
      plan = planStore.save({ input_sha256: inputHash, company_urls: company.urls, profile_urls: profile.urls,
        company_discovery: company, profile_discovery: profile });
    }
    const journalOptions = { stateRoot: context.stateRoot, operationId: input.research_operation_id };
    const companyBinding = core.companyCollectionBinding(query, plan.company_urls);
    const companyJournal = openCheckpoint({ ...journalOptions, binding: companyBinding });
    let companyState = companyJournal.load();
    const companyStore = immutableReceipt(context.stateRoot, input.research_operation_id, "company", value => {
      if (value?.input_sha256 !== inputHash || value.query_hash !== companyBinding.query_hash ||
          !/^(?:sd|s)_[A-Za-z0-9]{1,100}$/.test(value.snapshot_id || "")) throw new Error("invalid_company_receipt");
      const evidence = value.evidence;
      const url = core.canonicalLinkedIn(evidence?.source_url, "company");
      if (!url || !plan.company_urls.includes(url) || !Array.isArray(evidence?.reported_country_codes)) throw new Error("invalid_company_receipt");
      const verified = core.verifyCompanySnapshot([{ url, name: evidence.reported_name,
        country_code: evidence.reported_country_codes.join(",") }], query, [url]);
      if (!verified.ok) throw new Error("invalid_company_receipt");
      return { input_sha256: inputHash, query_hash: companyBinding.query_hash, snapshot_id: value.snapshot_id,
        evidence: { ...verified.company_evidence, checked_company_urls: plan.company_urls, returned_count: plan.company_urls.length } };
    });
    let company = companyStore.load();
    if (company && (companyState?.phase !== "completed" || companyState.snapshot_id !== company.snapshot_id))
      return fail("company_checkpoint_mismatch");
    if (!company) {
      const outcome = await core.advanceCollection(companyBinding, companyState, { ...dependencies, ...companyJournal });
      if (outcome.error_code === "collection_pending") return projectProviderWait(outcome, context);
      if (!outcome.company_verified) return outcome;
      companyState = companyJournal.load();
      company = companyStore.save({ input_sha256: inputHash, query_hash: companyBinding.query_hash,
        snapshot_id: companyState.snapshot_id, evidence: outcome.company_evidence });
    }
    const binding = core.collectionBinding(query, company.evidence.source_url, plan.profile_urls);
    const journal = openCheckpoint({ ...journalOptions, binding });
    const outcome = await core.advanceCollection(binding, journal.load(), { ...dependencies, ...journal });
    const discovery = { company: plan.company_discovery, profiles: plan.profile_discovery };
    const evidence = { company_snapshot_id: company.snapshot_id, ...company.evidence };
    if (outcome.error_code === "collection_pending") return { ...projectProviderWait(outcome, context),
      discovery_evidence: discovery, company_evidence: evidence };
    return { ...outcome, discovery_evidence: discovery, company_evidence: evidence };
  } catch { return fail("runner_state_or_context_invalid"); }
}

async function main() {
  let outcome;
  try {
    const context = runnerContext(process.env);
    outcome = await runBrightData(context, nativeDependencies(context));
  } catch { outcome = fail("runner_context_invalid"); }
  process.stdout.write(JSON.stringify(outcome) + "\n");
}
if (require.main === module) main().catch(() => { process.stdout.write(JSON.stringify(fail("runner_failed")) + "\n"); });
module.exports = { runnerContext, nativeDependencies, runBrightData, main };
