# CTOX Web Stack Bench

This bench exists to regression-test the CTOX-owned web surface without tying
every edit loop to a full workspace rebuild.

What it covers today:

- deterministic `ctox_web_search` via the built-in mock provider
- deterministic `ctox_web_read` against a local HTTP fixture
- deterministic PDF-style `ctox_web_read` via the mock provider
- deterministic `ctox_web_scrape` wrapper coverage for `show-latest`
- deterministic `ctox_web_scrape` query-path coverage when semantic retrieval is explicitly disabled
- deterministic `ctox_web_scrape` fail-fast coverage when semantic retrieval is requested without a local embedding engine
- deterministic `ctox_browser_prepare` reference bootstrapping
- deterministic agent-style browser workflows:
  - form submission
  - login-like redirect flow
  - table filtering and extraction
  - multi-step wizard completion
  - docs navigation across linked pages
  - checklist state changes
- deterministic local docs-read coverage for exact command extraction
- optional live checks for real web search/read discovery against stable public sources
- optional live execution of `ctox_browser_automation` against a local fixture page

What it does not cover yet:

- trace or HAR capture
- semantic scrape ranking against seeded embeddings
- multi-step browser task completion scoring

Why the split exists:

- `fixture` tier is hermetic and should be used for regression gating.
- `live` tier is intentionally optional and maps to external benchmark families
  like BrowseComp, WorkArena, WebArena, VisualWebArena, and Mind2Web.

## Usage

Validate manifests and fixtures only:

```bash
python3 tools/web-stack/bench/run.py --validate-only
```

Run the deterministic fixture tier against a built `ctox` binary:

```bash
python3 tools/web-stack/bench/run.py \
  --ctox-bin /absolute/path/to/ctox \
  --tier fixture
```

Run the optional live tier and persist a JSON report:

```bash
python3 tools/web-stack/bench/run.py \
  --ctox-bin /absolute/path/to/ctox \
  --tier live \
  --report /tmp/ctox-web-bench.json
```

## Report Shape

The runner emits a machine-readable JSON report with:

- the selected tier
- the binary path
- per-case duration and status
- per-case stdout/stderr excerpts
- a pass/fail summary
- expected-failure cases for operational guardrails that should abort fast

This keeps the evaluation surface deterministic and reviewable, which mirrors
the structured-evaluation direction seen in WebArena-Verified.

## Agentic Coverage

The suite now mixes two layers:

- tool-level regressions for search, read, scrape, and browser preparation
- agentic workflow tasks that simulate the kind of bounded browser work an agent
  actually performs when gathering evidence or completing simple web tasks

The browser workflow cases reuse a shared reference directory during a single
bench run so that expanding the suite does not require reinstalling Playwright
for every case.
