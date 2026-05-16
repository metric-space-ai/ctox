---
name: web-unlock
description: Diagnose and repair CTOX's browser stealth stack when bot detection blocks `ctox web search`, `ctox web browser-automation`, or scraping. Reads the failing detection-site source, maps the failing check to a patch in `stealth_init.js` / `google_browser_runner.mjs` / `tools/web-stack/src/browser.rs`, applies it, rebuilds, retests against the four baseline probes (sannysoft, antoinevastel, creepjs, incolumitas), and commits.
metadata:
  short-description: Diagnose and patch CTOX browser stealth when bot detection regresses
cluster: communication
---

# Web Unlock

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Stealth findings become durable only when reflected in the CTOX runtime store (commit + push, plus an entry in `references/test-baseline.md` and `references/detection-vectors.md` if the fix is novel). Notes and screenshots alone do not count.

## When to invoke this skill

Trigger this skill when any of the following observable signals appear during normal CTOX web work:

1. **`ctox web search` returns empty / wrong results**. The auto-provider cascade has drifted, or Google redirects to `/sorry/index`, or Brave/DDG return Cloudflare challenge HTML.
2. **`ctox web browser-automation` finds expected DOM elements missing** because the page is replaced by a CAPTCHA, Cloudflare Turnstile, or DataDome challenge.
3. **`ctox web scrape` returns empty rows from a target that previously worked**.
4. **A skill or worker explicitly reports a bot-detection block** (HTTP 403/429 with bot-related body, "Just a moment..." Cloudflare page, "Access Denied" Akamai, etc.).
5. **Routine regression check** — invoked by maintenance or by the owner to verify the stealth stack still holds after upstream Patchright / detection-site updates.

Do not invoke for: provider rate limits (separate path), legitimate access denials (the site doesn't want you), TLS-only fingerprinting (structurally not solvable in userland — see the structural-limits section below).

## What this skill knows

The CTOX browser stack has four cooperating stealth layers, all touchable from userland:

| Layer | File | Purpose |
|---|---|---|
| Browser runtime | Patchright (`runtime/browser/interactive-reference/`) | Patches CDP-level leaks: `Runtime.enable`, `Console.enable`, sourceURL markers |
| Launch process | `tools/web-stack/src/browser.rs` (generic) & `tools/web-stack/assets/google_browser_runner.mjs` (Google) | Sets `--user-agent`, `--lang`, viewport, `ignoreDefaultArgs`. Browser-process-global, covers Service/Web Worker contexts |
| HTTP layer | `extraHTTPHeaders` in both runners | Sec-CH-UA, Sec-CH-UA-Mobile, Sec-CH-UA-Platform |
| Page JS | `tools/web-stack/assets/stealth_init.js` | navigator.webdriver/plugins/chrome/permissions/WebGL/connection/userAgentData, iframe propagation, Notification.permission, hasFocus, vibrate, etc. |

Plus a behavioral layer for `humanlike.mjs` (mouse/keyboard/scroll) available as `globalThis.humanlike` in the generic runner — opt-in by skill code.

See `references/patch-locations.md` for line-level pointers.

## Central registry — SQLite-backed `ctox web unlock` CLI

All probe configuration, vector knowledge, and test-run history is persisted in the consolidated runtime database (`runtime/ctox.sqlite3`) under four tables:

| Table | Purpose |
|---|---|
| `web_unlock_probes` | Registered detection-site probes (id, url, script path, parser, timeout, enabled) |
| `web_unlock_vectors` | Known detection vectors with their fix-strategy, status (working/broken/untested), and last-verified timestamp |
| `web_unlock_test_runs` | Append-only history of every recorded run (probe_id, executed_at, duration, pass/fail, failed tests) |
| `web_unlock_repairs` | Recorded repair attempts (reserved for future auto-repair workflow) |

Schema and seed are written on first use. The seed lives at `tools/web-stack/assets/web_unlock_seed.json` and is embedded into the `ctox` binary via `include_str!`. To extend or override the seed, edit that JSON file and rebuild.

### CLI surface

```sh
ctox web unlock list-probes
ctox web unlock list-vectors [<probe_id>]
ctox web unlock baseline [<probe_id>] [--record] [--auto-repair]
ctox web unlock history [<probe_id>] [--limit N]
ctox web unlock add-vector --id <vid> --probe <pid> --test <name> \
    --desc <text> --fix <text> [--predicate <js>] [--patch-files <a,b>]
ctox web unlock set-vector-status --id <vid> --status <working|broken|untested>

# Repair flow
ctox web unlock repair start --vector <vid> [--run-id <n>] [--notes <text>]
ctox web unlock repair complete --id <repair_id> (--succeeded | --failed) \
    [--commit <sha>] [--notes <text>]
ctox web unlock repair list [--status <pending|succeeded|failed>] [--limit N]
```

`baseline` returns structured JSON with one entry per probe (`passed_baseline`, `failed_tests`, `duration_ms`, `notes`, `run_id`, `opened_repairs`). Exit code is non-zero if any probe regressed — directly usable in maintenance loops and pre-commit checks. `--record` persists each run to `web_unlock_test_runs` for trend analysis. `--auto-repair` additionally opens pending `web_unlock_repairs` rows for any known vector whose `test_name` matches a failed test under the same probe.

`add-vector` and `set-vector-status` let the agent grow the knowledge base from inside its own loop: when a new probe-failure is diagnosed, the fix path includes registering the vector so future regressions can be looked up.

### Repair flow

`repair start --vector <vid>` is the operator-facing entry into the structured repair workflow:

1. Loads the vector from `web_unlock_vectors`
2. Inserts a `web_unlock_repairs` row with `succeeded=NULL` (pending)
3. Flips the vector's `status` to `broken` until the repair completes
4. Emits a JSON plan with `repair_id`, the vector's `patch_files`, `fix_strategy`, and the precise next-step commands the agent should run

The agent (or operator) then edits the files, runs `ctox web unlock baseline <probe_id> --record` to verify, and creates a commit. The session closes with:

```sh
ctox web unlock repair complete --id <repair_id> --succeeded --commit <sha>
```

On success, the vector flips back to `working`, `last_verified_at` is updated, the commit hash is persisted on the repair row, and the loop is closed. On `--failed`, the vector stays `broken`, the repair row is closed with `succeeded=0`, and `notes` records why — leaving the next iteration a clean slate.

`repair list [--status pending|succeeded|failed]` is the trend query — useful for "what's still broken right now" and for retrospectives.

The `baseline --auto-repair` flag combines the two: if a probe regresses, matching vectors are converted to pending repairs in the same call, ready for the agent to pick up.

## Diagnostic workflow

### Step 1 — Baseline probe

Run the full suite and compare against the registered baseline:

```sh
ctox web unlock baseline --record
```

Non-zero exit means at least one probe regressed. The JSON output names the failing probe(s) and the specific test names that flipped. Persisted to `web_unlock_test_runs` for the history view.

For a faster targeted run (one probe only):

```sh
ctox web unlock baseline sannysoft --record
```

### Step 2 — Locate the failing detection in the site's source

This is the key insight from the May 2026 unlock work: **read the detection-site's own JS source** to know what it actually probes. Web search alone is not enough — implementations change.

```sh
# Each detection site has its own JS file. Fetch the raw source and grep
# for the failing test ID. Example for incolumitas:
ctox web browser-automation --script-file skills/system/communication/web-unlock/agents/probe-scripts/dump_external_scripts.js
# That lists external <script src=> URLs. Then:
curl -sf https://bot.incolumitas.com/newTests.js?version=v0.6.4 | grep -B2 -A15 "<failing-test-id>"
```

The detection is always one of:
- A specific property read (`navigator.webdriver`, `navigator.connection.rtt`, `document.hasFocus()`, ...)
- A consistency check (page vs worker UA, Sec-CH-UA vs navigator.userAgent, HTTP Accept-Language vs navigator.language, ...)
- A behavioral comparison (`navigator.plugins.item(2**32) === plugins[0]` etc.)
- A pattern probe (does Function.prototype.toString look modified? does the permissions.query response shape match puppeteer-extra-stealth's signature?)

### Step 3 — Map to a CTOX patch location

Use `references/detection-vectors.md` as a lookup table. Each known vector has a recipe. New vectors get appended after the fix is verified.

If the vector is not yet listed:
- JS-property tells → patch `tools/web-stack/assets/stealth_init.js` (the IIFE near the relevant section)
- Worker-context tells → typically not patchable via `addInitScript`; consider a Chromium launch arg in `browser.rs` (see how `--user-agent` was applied to fix the Service Worker UA leak)
- HTTP-layer tells (header inconsistency) → `extraHTTPHeaders` in both runners
- Browser-process-wide settings → launch args in both runners

### Step 4 — Apply the patch

Edit the file with `Edit` (preferred for small targeted changes) or `Write` (only for new files). Patches should:
- Be guarded by `try { ... } catch {}` (stealth init must never crash the page)
- Use the existing `asNative()` helper in stealth_init.js to hide the override from `Function.prototype.toString` probes
- Match real Chrome's behavior precisely (e.g. uint32 truncation, not just "any-non-zero")
- Be small and focused — one fix per commit

### Step 5 — Rebuild and reset the runtime workspace

```sh
cargo build -p ctox
# If you touched package.json deps or want a clean reference, reset:
rm -rf runtime/browser/interactive-reference/node_modules runtime/browser/interactive-reference/package.json runtime/browser/interactive-reference/package-lock.json
ctox web browser-prepare --install-reference --install-browser
```

If you only touched stealth_init.js / humanlike.mjs / google_browser_runner.mjs, the asset files are written into the reference workspace on the next `browser-prepare` or `run_browser_automation` call by `ensure_*_module(reference_dir)`. No re-npm-install needed.

### Step 6 — Re-run all four probes

Re-run the same four probes. All must hold their baseline. If any new FAIL appears as a side effect, the patch is wrong — revert and rethink.

### Step 7 — Commit and close the repair

One commit per patch family. Use the HEREDOC style from this skill:

```sh
git add tools/web-stack/...
git commit -m "$(cat <<'EOF'
Close <detection-name> in <site-name>

<one paragraph: what the site probes, why the old code failed it,
 what the fix does>

Verified against the four probes — all hold baseline.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
git push origin main

# Then close the repair so the vector flips back to "working":
COMMIT_SHA=$(git rev-parse HEAD)
ctox web unlock repair complete --id <repair_id> --succeeded --commit "$COMMIT_SHA"
```

If the repair was opened by `baseline --auto-repair`, the `repair_id` is already in the prior `baseline` JSON output under `probes[*].opened_repairs`. Otherwise look it up with `ctox web unlock repair list --status pending`.

### Step 8 — Update knowledge artifacts (CLI-first, markdown as commentary)

If the vector is **new** (not yet in the SQLite registry):

```sh
ctox web unlock add-vector \
  --id <slug-new-vector> \
  --probe <probe_id> \
  --test "<exact test name as the site reports it>" \
  --desc "<one sentence: what the site probes>" \
  --predicate "<js expression, optional>" \
  --fix "<one-line recipe>" \
  --patch-files "tools/web-stack/assets/stealth_init.js,..."
ctox web unlock set-vector-status --id <slug-new-vector> --status working
```

If the vector is **known** but the recipe needed adjustment, edit the seed JSON at `tools/web-stack/assets/web_unlock_seed.json` and rebuild — or update via `add-vector` (it does INSERT OR REPLACE) and adjust the seed in a follow-up.

The `references/detection-vectors.md` markdown stays as human-readable commentary and onboarding aid, but the SQLite registry is the **authoritative** source going forward. Use `ctox web unlock list-vectors` to query the live state. After verifying that the fix sticks, run:

```sh
ctox web unlock baseline --record
```

so the green run is logged in `web_unlock_test_runs` and `list-probes` shows the updated `last_run` block.

## Hard guardrails

- **Never weaken existing stealth to fix a new regression.** If a fix breaks another probe, you have a conflict; resolve it with a more targeted patch, not by reverting earlier work.
- **Never push without all four probes holding baseline.** A regression that loses sannysoft to fix creepjs is a net loss.
- **No `git --no-verify` or amending pushed commits.** Use new commits.
- **No bot-detection workarounds that aim at sites we have no business accessing.** This skill is for CTOX's legitimate web-scraping and search needs against the public web; do not use it to defeat access controls on private or sensitive systems.
- **No CDN-bypass exploits.** Stealth means looking like a real user, not exploiting infrastructure flaws.

## Structural limits (where this skill must stop)

These cannot be fixed in userland and are out of scope:

- **TLS-fingerprint (JA3/JA4)** — comes from Chromium's NSS stack. Requires a different binary (Camoufox / curl-impersonate) or a residential-proxy layer.
- **IP reputation / datacenter detection** — needs proxy infrastructure.
- **Behavioral baseline beyond what `humanlike.mjs` can simulate** — biometric mouse-curve scoring, dwell-time profiles. Real human time is needed.
- **Commercial-grade pattern detection (FingerprintJS Premium, CreepJS's "Stealth" detector, PerimeterX behavioral models)** — these score the *presence* of patches, not their correctness. Reducing the patch surface makes our anti-detection worse, not better. CreepJS at ~33% headless is a known structural ceiling for userland-only stealth.

When the diagnosis lands in one of these categories, document it in the work-item and stop. Escalate to the owner with the analysis; do not invent a workaround.

## Worked example (from the May 2026 unlock session)

Initial symptom: `bot.sannysoft.com` showed `User Agent (Old): FAIL` with "HeadlessChrome/147" in the UA.

Diagnosis:
- Fetched `https://bot.sannysoft.com/` source, found the row class `failed result` for the UA check
- Read `tools/web-stack/src/browser.rs` and noticed `build_browser_runner_script` set no userAgent — Chromium fell back to HeadlessChrome
- Patched: added `defaultUserAgent` platform switch (darwin/win32/linux) + `userAgent: defaultUserAgent` in contextOptions

Re-test: sannysoft now 0/29 FAIL, antoinevastel "not headless".

Follow-up (incolumitas Service Worker still showed HeadlessChrome):
- Fetched `https://bot.incolumitas.com/newTests.js?version=v0.6.4`, found `serviceWorkerRes` extracts navigator.userAgent inside a Service Worker context
- Recognized that context.userAgent override is CDP-only and doesn't reach Service Workers
- Patched: added `--user-agent=<UA>` to Chromium launch args (browser-process-global, covers all contexts)

Re-test: incolumitas Service Worker UA flipped to Chrome/146, both inconsistent* tests OK.

Follow-up (incolumitas `overflowTest: FAIL`):
- Fetched `newTests.js` again, found the exact line:
  `const overflowTest = navigator.plugins.item(4294967296) === navigator.plugins[0];`
- Realized real Chrome truncates index to uint32 (`i >>> 0`)
- Patched: `pluginArray.item = (i) => fakePlugins[i >>> 0] || null` (one-character fix)

Re-test: incolumitas overflowTest OK.

This pattern — read the source, find the exact predicate, patch precisely — is what makes the skill robust against future detection-site updates.

## Files and references

**Source-of-truth (SQLite + seed):**

- `runtime/ctox.sqlite3` tables `web_unlock_probes`, `web_unlock_vectors`, `web_unlock_test_runs`, `web_unlock_repairs` — runtime state, queryable via `ctox web unlock`
- `tools/web-stack/assets/web_unlock_seed.json` — embedded into the binary, populates an empty registry on first use; edit + rebuild to extend defaults
- `tools/web-stack/src/unlock.rs` — Rust module implementing schema, seed, queries, baseline runner, history

**Markdown (commentary and onboarding):**

- `references/detection-vectors.md` — human-readable vector commentary mirroring the SQLite contents
- `references/patch-locations.md` — CTOX code-location map for each stealth layer
- `references/test-baseline.md` — current known-good test pass/fail matrix

**Probe scripts (used by `ctox web unlock baseline`):**

- `agents/probe-scripts/sannysoft.js` — extracts the two test tables
- `agents/probe-scripts/areyouheadless.js` — extracts the "You are…" verdict
- `agents/probe-scripts/creepjs.js` — extracts headless score and trust hash
- `agents/probe-scripts/incolumitas.js` — extracts all `<pre>` test result blocks
- `agents/probe-scripts/dump_external_scripts.js` — lists external `<script src=>` URLs of a detection page (used in Step 2 to find the failing predicate)
