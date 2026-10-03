"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const EMAIL = "info@destilla.com";
const providers = [
  { name: "experte-de", url: "https://www.experte.de/email-pruefen", verdict: "Gültig", source: "experteBrowserSource" },
  { name: "mailtester-com", url: "https://mailtester.com/email-checker/", verdict: "Deliverable", source: "browserSource" },
];

function runAdapter(provider, response) {
  const source = fs.readFileSync(path.join(__dirname, provider.name + ".cjs"), "utf8");
  const module = { exports: {} };
  let stdout = "";
  let browserCalls = 0;
  const requireStub = name => {
    assert.equal(name, "child_process");
    return { execFileSync(_bin, args) {
      if (args[0] === "web" && args[1] === "browser-automation") {
        browserCalls++;
        return JSON.stringify({ ok: true, result: response });
      }
      assert.deepEqual(Array.from(args.slice(0, 4)), ["web", "unlock", "signals", "record"]);
      return "{}";
    } };
  };
  requireStub.main = module;
  vm.runInNewContext(source, {
    module, require: requireStub, URL,
    process: {
      env: { CTOX_SCRAPE_INPUT_JSON: JSON.stringify({ email: EMAIL }) },
      stdout: { write: value => { stdout += value; } },
      stderr: { write: () => {} },
    },
  }, { timeout: 1000 });
  assert.equal(browserCalls, 1);
  return JSON.parse(stdout);
}

function response(provider, overrides = {}) {
  return { email: EMAIL, subject_email: EMAIL, status: "valid",
    evidence: `${EMAIL} | ${provider.verdict}`, url: provider.url, title: "Email checker", ...overrides };
}

async function runBrowserSource(provider, rows, texts) {
  const adapter = require("./" + provider.name + ".cjs");
  const document = {
    querySelectorAll(selector) {
      if (selector === "table tbody tr") {
        return rows.map(cells => ({ querySelectorAll: () => cells.map(innerText => ({ innerText })) }));
      }
      return texts.map(innerText => ({ innerText }));
    },
  };
  function locator(selector) {
    const consent = selector.startsWith("#") || selector === "consent";
    return {
      first() { return this; }, count: async () => consent ? 0 : 1,
      isVisible: async () => true, fill: async () => {}, click: async () => {},
      innerText: async () => "Email checker", allInnerTexts: async () => texts,
    };
  }
  const page = {
    locator, getByRole: () => locator(provider.name === "experte-de" ? "consent" : "submit"),
    url: () => provider.url, title: async () => "Email checker",
    waitForLoadState: async () => {}, waitForTimeout: async () => {},
    waitForFunction: async (fn, needle) => {
      const ready = vm.runInNewContext(`(${fn.toString()})(needle)`, { document, needle });
      if (!ready) throw new Error("fixture has no ready verdict");
    },
    evaluate: async fn => vm.runInNewContext(`(${fn.toString()})()`, { document }),
  };
  const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
  return new AsyncFunction("page", "ctoxBrowser", adapter[provider.source](EMAIL))(
    page, { goto: async () => {} },
  );
}

for (const provider of providers) {
  test(`${provider.name}: real adapter entry emits provider-bound subject`, () => {
    const out = runAdapter(provider, response(provider));
    assert.equal(out.records.length, 1);
    assert.equal(out.records[0].subject_email, EMAIL);
    assert.equal(out.records[0].field, "person_email_validation");
    assert.equal(out.records[0].value, "valid");
  });
  for (const [name, overrides] of [
    ["missing subject", { subject_email: undefined }],
    ["mismatched subject", { subject_email: "other@destilla.com" }],
    ["contradictory response email", { email: "other@destilla.com" }],
    ["missing provider evidence", { evidence: "" }],
    ["wrong provider evidence", { evidence: "other@destilla.com | valid" }],
    ["substring collision", { evidence: "not-info@destilla.com | valid" }],
    ["ambiguous provider evidence", { evidence: `${EMAIL} | other@destilla.com | valid` }],
    ["wrong origin", { url: "https://wrong.example/check" }],
    ["provider challenge", { status: "blocked", body: "Verify you are human" }],
    ["login page", { title: "Sign in" }],
  ]) {
    test(`${provider.name}: rejects ${name}`, () => {
      assert.deepEqual(runAdapter(provider, response(provider, overrides)).records, []);
    });
  }
  test(`${provider.name}: executed browser source extracts actual DOM subject`, async () => {
    const result = await runBrowserSource(provider, [[EMAIL, "Gültig"]], [`${EMAIL} Deliverable`]);
    assert.equal(result.subject_email, EMAIL);
    assert.equal(result.status, "valid");
    assert.equal(runAdapter(provider, result).records[0].subject_email, EMAIL);
  });
  test(`${provider.name}: executed browser source rejects another address`, async () => {
    const result = await runBrowserSource(provider, [["not-info@destilla.com", "Gültig"]], ["not-info@destilla.com Deliverable"]);
    assert.equal(result.status, "failed");
    assert.equal(result.subject_email, undefined);
  });
}

test("MailTester: ambiguous DOM results do not manufacture a requested subject", async () => {
  const result = await runBrowserSource(providers[1], [], [`${EMAIL} Deliverable`, "other@destilla.com Undeliverable"]);
  assert.equal(result.status, "failed");
  assert.equal(result.subject_email, undefined);
});
