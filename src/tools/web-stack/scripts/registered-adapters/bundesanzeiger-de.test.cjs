"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const vm = require("node:vm");
const { createHash } = require("node:crypto");
const { spawnSync } = require("node:child_process");
const adapterPath = path.join(__dirname, "bundesanzeiger-de.cjs");
const adapter = require(adapterPath);
const company = "Beiersdorf Manufacturing Leipzig GmbH";
const origin = "https://www.bundesanzeiger.de";
const resultUrl = origin + "/pub/de/suchergebnis?current";
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");

function result(overrides = {}) {
  return { url: resultUrl, response_url: resultUrl, query: company,
    query_completed: true, http_status: 200, blocked: false, results_page: true,
    entries: [{ name: "Anderer Herausgeber AG", city: "Leipzig" }], ...overrides };
}

function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "bundes-receipt-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const run = path.join(dir, "scrape_run-current");
  fs.mkdirSync(run);
  return { dir, run, env: { CTOX_SCRAPE_RUN_DIR: run, CTOX_SCRAPE_TARGET_KEY: "bundesanzeiger-de" } };
}

function request(query, frame, options = {}) {
  return {
    url: () => options.url || origin + "/pub/de/suche",
    frame: () => frame,
    resourceType: () => options.type || "document",
    isNavigationRequest: () => (options.type || "document") === "document",
    postData: () => query === null ? null : new URLSearchParams({ fulltext: query }).toString(),
    redirectedFrom: () => options.from || null,
  };
}

function response(req, status = 200, url = resultUrl) {
  return { request: () => req, status: () => status, url: () => url, finished: async () => null };
}

function generatedBrowserScript(query) {
  let script;
  const sandbox = { module: { exports: {} }, exports: {}, process: { env: {} }, URL,
    require: (name) => name === "child_process" ? {
      execFileSync: (_bin, _args, options) => { script = options.input; return '{"ok":true,"result":{}}'; },
    } : require(name),
  };
  vm.runInNewContext(fs.readFileSync(adapterPath, "utf8"), sandbox, { filename: adapterPath });
  sandbox.module.exports.browserSearch(query);
  return script;
}

async function simulateBrowser(query, { status = 200, matchingResponse = true, noResults = false, navigation = true } = {}) {
  const frame = {};
  let evaluations = 0;
  let filled;
  const page = {
    goto: async () => undefined,
    waitForTimeout: async () => undefined,
    getByRole: () => ({ first: () => ({ count: async () => 0 }) }),
    locator: (selector) => ({ count: async () => 1, fill: async (value) => { filled = value; },
      click: async () => { assert.equal(filled, query); }, press: async () => {} }),
    mainFrame: () => frame,
    waitForNavigation: async () => {
      if (!navigation) throw new Error("AJAX completed but no current-document navigation");
      return response(request(matchingResponse ? query : "old query", frame), status);
    },
    waitForLoadState: async () => {},
    waitForSelector: async () => { if (noResults) throw new Error("no table on explicit empty page"); },
    evaluate: async () => ++evaluations === 1 || noResults || !navigation || !matchingResponse || status >= 300
      ? { blocked: false, noResults }
      : { url: resultUrl, blocked: false, results_page: true, entries: [{ name: "Other AG" }] },
    url: () => resultUrl,
  };
  const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
  return new AsyncFunction("page", generatedBrowserScript(query))(page);
}

test("receipt binds exact raw UTF-8 bytes and actual query/run/provider, without records", (t) => {
  const f = fixture(t);
  const raw = '{ "company": "  Beiersdorf Manufacturing Leipzig GmbH  ", "country":"DE" }\n';
  const output = adapter.writeQueryCompletion(raw, company, result(), f.env);
  assert.deepEqual(Object.keys(output).sort(), ["query_completion", "records"]);
  assert.deepEqual(output.records, []);
  const bytes = fs.readFileSync(path.join(f.run, output.query_completion.evidence_path));
  assert.equal(output.query_completion.evidence_sha256, hash(bytes));
  const receipt = JSON.parse(bytes);
  assert.equal(receipt.schema, "ctox.scrape.query_completion.v1");
  assert.equal(receipt.run_id, "scrape_run-current");
  assert.equal(receipt.target_key, "bundesanzeiger-de");
  assert.equal(receipt.input_sha256, hash(raw));
  assert.notEqual(receipt.input_sha256, hash(JSON.stringify(JSON.parse(raw))));
  assert.equal(receipt.query, company);
  assert.equal(receipt.provider_url, resultUrl);
  assert.equal(receipt.http_status, 200);
  assert.equal(receipt.matched_records, 0);
  assert.equal(receipt.observed_records, 1);
  assert.deepEqual(receipt.observation.publisher_names, ["Anderer Herausgeber AG"]);
});

test("explicit successful zero-result page is completion, ordinary empty selector result stays drift", () => {
  assert.equal(adapter.classifyBrowserResult(company, result({ entries: [], no_results: true })).query_completed, true);
  assert.equal(adapter.classifyBrowserResult(company, result({ entries: [] })).failure_mode, "portal_drift");
});

test("an exact publisher still produces normal field records with no completion receipt", () => {
  const output = adapter.classifyBrowserResult(company, result({ entries: [{ name: company, city: "Leipzig" }] }));
  assert.deepEqual(output.records.map(({ field, value }) => ({ field, value })), [
    { field: "firma_name", value: company }, { field: "firma_ort", value: "Leipzig" },
  ]);
  assert.equal(output.query_completed, undefined);
});

for (const [name, override] of Object.entries({
  challenge: { blocked: true }, missing_response: { query_completed: false },
  missing_status: { http_status: null }, failed_status: { http_status: 503 },
  auth_status: { http_status: 403 }, redirect_status: { http_status: 302 },
  wrong_origin: { response_url: "https://bundesanzeiger.de/pub/de/suche" },
  credentials: { response_url: "https://name:password@www.bundesanzeiger.de/pub/de/suche" },
  unrecognized_page: { results_page: false }, invalid_rows: { entries: [{}] },
  contradictory_empty: { no_results: true },
})) {
  test(`does not promote ${name} into successful empty query`, (t) => {
    const f = fixture(t);
    const value = result(override);
    assert.equal(adapter.isCompletedEmptyQuery(company, value), false);
    assert.equal(adapter.classifyBrowserResult(company, value).query_completed, undefined);
    assert.throws(() => adapter.writeQueryCompletion(JSON.stringify({ company }), company, value, f.env));
    assert.equal(fs.existsSync(path.join(f.run, "outputs/query-evidence.json")), false);
  });
}

test("query override must be the actual submitted query; null cannot fall back to company", (t) => {
  const f = fixture(t);
  const raw = JSON.stringify({ company, query: "  Veröffentlichung Müller GmbH  " });
  assert.throws(() => adapter.writeQueryCompletion(raw, company, result(), f.env));
  const output = adapter.writeQueryCompletion(raw, company, result({ query: "Veröffentlichung Müller GmbH" }), f.env);
  const receipt = JSON.parse(fs.readFileSync(path.join(f.run, output.query_completion.evidence_path)));
  assert.equal(receipt.query, "Veröffentlichung Müller GmbH");
  assert.equal(adapter.requestedQuery({ company, query: null }), "");
});

test("receipt cannot overwrite stale evidence or follow an outputs symlink", (t) => {
  const f = fixture(t);
  const raw = JSON.stringify({ company });
  adapter.writeQueryCompletion(raw, company, result(), f.env);
  const p = path.join(f.run, "outputs/query-evidence.json");
  const before = fs.readFileSync(p);
  assert.throws(() => adapter.writeQueryCompletion(raw, company, result(), f.env));
  assert.deepEqual(fs.readFileSync(p), before);
  const anotherRun = path.join(f.dir, "scrape_run-other");
  const outside = path.join(f.dir, "outside");
  fs.mkdirSync(anotherRun); fs.mkdirSync(outside);
  fs.symlinkSync(outside, path.join(anotherRun, "outputs"));
  assert.throws(() => adapter.writeQueryCompletion(raw, company, result(), { ...f.env, CTOX_SCRAPE_RUN_DIR: anotherRun }));
  assert.deepEqual(fs.readdirSync(outside), []);
});

test("response binding follows the submitted query through a same-origin redirect", () => {
  const frame = {};
  const initial = request(company, frame);
  const redirected = request(null, frame, { from: initial });
  assert.equal(adapter.isQueryResponse(response(redirected), company, origin, frame), true);
  assert.equal(adapter.isQueryResponse(response(request("old company", frame)), company, origin, frame), false);
  assert.equal(adapter.isQueryResponse(response(request(company, {})), company, origin, frame), false);
  assert.equal(adapter.isQueryResponse(response(initial, 200, "https://example.com/"), company, origin, frame), false);
  assert.equal(adapter.isQueryResponse(response(request(null, frame)), company, origin, frame), false);
});

test("rejects ambiguous URL/body query values and AJAX-only responses", () => {
  const frame = {};
  const url = origin + "/pub/de/suche?" + new URLSearchParams({ fulltext: company });
  for (const body of ["Old Company", company]) {
    assert.equal(adapter.isQueryResponse(response(request(body, frame, { url })), company, origin, frame), false);
  }
  const duplicate = request(null, frame, { url: url + "&fulltext=" + encodeURIComponent(company) });
  assert.equal(adapter.isQueryResponse(response(duplicate), company, origin, frame), false);
  assert.equal(adapter.isQueryResponse(response(request(company, frame, { type: "xhr" })), company, origin, frame), false);
});

test("generated browser flow captures current response and explicit empty page", async () => {
  for (const noResults of [false, true]) {
    const value = await simulateBrowser(company, { noResults });
    assert.equal(value.query, company);
    assert.equal(value.http_status, 200);
    assert.equal(value.response_url, resultUrl);
    assert.equal(adapter.isCompletedEmptyQuery(company, value), true);
  }
});

test("generated browser flow cannot reuse an old query response or a server error", async () => {
  assert.equal(adapter.isCompletedEmptyQuery(company, await simulateBrowser(company, { matchingResponse: false })), false);
  assert.equal(adapter.isCompletedEmptyQuery(company, await simulateBrowser(company, { status: 503 })), false);
});

for (const update of ["delayed", "absent"]) {
  test(`old result container plus AJAX200 with ${update} DOM update cannot emit completion`, async () => {
    // The old container is already present. A current-query AJAX200 is not a
    // document commit; neither a delayed nor an absent update may reuse it.
    const value = await simulateBrowser(company, { navigation: false });
    assert.deepEqual(value.entries, []);
    assert.equal(value.results_page, false);
    assert.equal(adapter.isCompletedEmptyQuery(company, value), false);
  });
}

test("CLI stdout contains only current receipt reference and empty records", (t) => {
  const f = fixture(t);
  const fakeCli = path.join(f.dir, "ctox-fixture");
  fs.writeFileSync(fakeCli, '#!/usr/bin/env node\nprocess.stdout.write(JSON.stringify({ok:true,result:JSON.parse(process.env.TEST_BROWSER_RESULT)}));\n', { mode: 0o700 });
  const child = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8", timeout: 5000,
    env: { PATH: path.dirname(process.execPath) + path.delimiter + (process.env.PATH || ""),
      TMPDIR: f.dir, ...f.env, CTOX_BIN: fakeCli,
      CTOX_SCRAPE_INPUT_JSON: JSON.stringify({ company }), TEST_BROWSER_RESULT: JSON.stringify(result()) },
  });
  assert.equal(child.status, 0, child.stderr);
  const output = JSON.parse(child.stdout);
  assert.deepEqual(Object.keys(output).sort(), ["query_completion", "records"]);
  assert.equal(output.records.length, 0);
  assert.equal(output.query_completion.evidence_sha256,
    hash(fs.readFileSync(path.join(f.run, output.query_completion.evidence_path))));
});
