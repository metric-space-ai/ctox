---
name: universal-scraping
description: Plan, build, revise, schedule, and operate reusable scraping workflows when CTOX must extract structured data from websites, APIs, feeds, documents, or browser-backed portals without reinventing the storage, script, and run-management model each time.
cluster: communication
---

# Universal Scraping

A scrape is a registered **target** (`target_key`) with a versioned extraction **script**: built once, reused by every run. Use only the tools below — not the shell `ctox` CLI (fails in the worker sandbox), `curl`, or scripts you run yourself.

## 1. Use what exists

1. **Stored records are enough?** `ctox_web_scrape {"target_key": "…", "mode": "latest", "limit": 20}` or `{"mode": "semantic", "query": "…", "limit": 10}`. No live call, but shared by all runs: no evidence for one specific record.
2. **Live run:** `{"target_key": "…", "mode": "execute", "input": {…}, "timeout_seconds": 180}` (max 420). The result carries `status`, `reason` and `records_preview` `{shown, total, records}` (≤ 40 records). Take values from it; never read run or state files.
3. **Read the status:**

| status | meaning | next step |
| --- | --- | --- |
| `succeeded`, `partial_output` | records delivered | use them; partial → consider §2 |
| `completed_empty` | query proven empty | accept |
| `portal_drift` | page reached, nothing extracted, or script error | check with `ctox_web_read` that the data is really there, then §2 |
| `invalid_input` | input missing or wrong | fix your input once; still failing → §2 |
| `blocked` | challenge, 401/403 | no repeat; §3 only if needed for this task |
| `temporary_unreachable` | timeout, 429, 5xx | no repeat in this task |
| `authorization_required` | login wall | `auto_reauthorization.ok: true` → rerun once; otherwise the run has already handed the login to the owner (§5) |
| `provider_account_inactive` | paid account inactive | no repeat; report |

## 2. Write or repair the script (priority 1)

1. **Draft** in your workspace, e.g. `adapters/<target_key>.js`. To repair, start from the target's current script (`scripts/current.js` in the target folder, two levels above `run_manifest_path`); read nothing else from that tree.
2. **Test:** `{"target_key": "…", "mode": "test", "script_path": "adapters/<target_key>.js", "input": {…}}` runs the draft through the real runner and stores nothing (no revision, records, state, repair or re-login); `reason` starts with `script_override_test:`. At most 3 tests per source and task.
3. **Register:** `{"target_key": "…", "mode": "register_script", "script_path": "…", "change_reason": "<what changed and why>"}` makes it the active revision (old ones stay). Only after a test with correct records.
4. **Run:** one `execute` to confirm the registered revision.
5. **New source:** first `{"target_key": "…", "mode": "upsert_target", "target": {"display_name": "…", "start_url": "https://…", "target_kind": "prospect-research", "config": {"record_key_fields": ["field", "source_url"], "expected_min_records": 1}, "output_schema": {"schema_key": "prospect.v1"}}}`, then steps 1–4. It replaces the whole definition: never use it on an existing target.

**Script contract** (the runner starts `node <script>` with a cleared environment plus these variables):
- `CTOX_SCRAPE_INPUT_JSON` — the call's `input` (JSON text); `CTOX_SCRAPE_START_URL`, `CTOX_SCRAPE_TARGET_KEY`, `CTOX_SCRAPE_RUN_DIR`, `CTOX_SCRAPE_OUTPUT_DIR` (scratch for captures); `CTOX_BIN` — the running ctox binary for `web browser-capture`, `web read`, `secret get`.
- stdout: exactly one JSON value — `{"records": [...]}` (also accepted: a bare array, `items`, `jobs`, `result.records`), or `{"records": [], "failure_mode": "<mode>", "detail": "<why>"}`. Without an optional `query_completion` receipt, 0 records counts as `portal_drift`.
- `failure_mode`: `invalid_input`, `temporary_unreachable`, `blocked`, `portal_drift`, `authorization_required`, `provider_account_inactive`, `partial_output`.
- Prospect records: `{"field", "value", "confidence": "high|medium", "source_url", "note"}`.

Minimal skeleton (Node ≥ 18, CommonJS, built-ins only):

```js
"use strict";
const BASE = "https://www.example.org"; // the source
const TIMEOUT_MS = 20000;
const out = (o) => process.stdout.write(JSON.stringify(o));
const fail = (mode, detail) => out({ records: [], failure_mode: mode, detail });

async function main() {
  let input;
  try { input = JSON.parse(process.env.CTOX_SCRAPE_INPUT_JSON || "{}"); }
  catch { return fail("invalid_input", "input is not JSON"); }
  const company = String(input.company || "").trim();
  if (!company) return fail("invalid_input", "input.company missing");

  let res;
  try {
    res = await fetch(`${BASE}/suche?q=${encodeURIComponent(company)}`,
      { signal: AbortSignal.timeout(TIMEOUT_MS), headers: { "user-agent": "Mozilla/5.0" } });
  } catch (e) { return fail("temporary_unreachable", `fetch: ${e.name}`); }
  if (res.status === 401 || res.status === 403) return fail("blocked", `HTTP ${res.status}`);
  if (res.status === 429 || res.status >= 500) return fail("temporary_unreachable", `HTTP ${res.status}`);
  if (!res.ok) return fail("portal_drift", `HTTP ${res.status}`);
  const html = await res.text();
  if (/captcha|cf-chl|verify you are human/i.test(html)) return fail("blocked", "challenge page");

  const records = [];
  const tel = html.match(/Telefon:?\s*([+0-9][0-9 ()\/-]{5,})/);
  if (tel) records.push({ field: "firma_telefon", value: tel[1].trim(), confidence: "high",
    source_url: res.url, note: "Telefon line on result page" });

  if (!records.length) return fail("portal_drift", "page loaded, no extractable fields");
  out({ records });
}
main().catch((e) => fail("portal_drift", `unexpected: ${e.message}`));
```

JS-rendered page: `execFileSync(process.env.CTOX_BIN, ["web", "browser-capture", "--url", url, "--out-dir", dir, "--timeout-ms", "45000"])` (`dir` under `CTOX_SCRAPE_OUTPUT_DIR`), then read `dir/page.html` (as the `impressum` adapter does).

## 3. Browser (priority 2)

`ctox_browser_automation` runs plain JavaScript in CTOX's browser: `await ctoxBrowser.goto(url)`, `observe()`, `click(target)`, `fill(target, value)`, `press(target, key)`, `screenshot()`. Only when a script is not feasible (interactive flow) or for one record now; write the working path (URLs, selectors, steps) into `adapters/<target_key>.notes.md` so it can become a script.

## 4. Robustness rules

- **Selectors:** stable labels, `id`, `name`, `data-*`, `aria-*`, JSON-LD, embedded JSON — never generated classes or positions. API or feed beats HTML.
- **Timeouts:** every request has its own timeout well below the run's `timeout_seconds`; at that limit the runner kills the whole process tree and the run counts as `temporary_unreachable`. At most one retry, only for a transient load failure — never for a loaded page that yields nothing.
- **No loops:** bounded pagination (fixed page cap), no `while (true)`, no retry without a counter.
- **Honest outcome:** set `failure_mode` yourself instead of crashing. Non-zero exit or unparseable stdout → `portal_drift`; "timeout", "429", "ssl" in stderr can turn a run `temporary_unreachable`. stdout holds only the JSON.
- **No fabrication:** only what the page states, with its exact `source_url`.
- **Self-contained:** Node built-ins only; no relative `require`, no npm packages.
- **Secrets:** never print, log or put into records or `detail` a credential, token or cookie.

## 5. Authenticated sources

- The credential belongs to the target config: `credential_ref: "ctox-secret://credentials/<NAME>"` (or `credential_secret_name`). Set it in `upsert_target` for a new target; never put a value into a script or input.
- The script fetches it at runtime only when it must: `execFileSync(process.env.CTOX_BIN, ["secret", "get", "--scope", "credentials", "--name", NAME])` returns `{"value": …}`. An API key is sent as the API expects (e.g. `Authorization: Bearer …`).
- Landing on a login page → `failure_mode: "authorization_required"`; a rejected key → `blocked` with the HTTP status in `detail`. Never mask a credential problem as `temporary_unreachable`.
- Re-login is the run's job: on `authorization_required` it signs in with the stored credential itself (`auto_reauthorization`, including an e-mail one-time code) and otherwise hands the login to the owner. You never type credentials and do not request a second login.

## 6. Where the tools work

- `execute`/`test`: only in an Outbound research task, for targets in its `source_policy`. Elsewhere draft and report; never register an untested script.
- `register_script`/`upsert_target`: any bound Business OS task. `latest`/`semantic`: everywhere.

Operators (host shell only): `ctox scrape list-targets | show-target --target-key <key> | execute --target-key <key> --input-json '<json>'`; `ctox scrape register-script --target-key <key> --script-file <path> --change-reason <text>`; `ctox scrape upsert-target --input <target.json>`.
