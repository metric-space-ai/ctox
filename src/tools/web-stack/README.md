# CTOX Web Stack

This crate is the owned compile boundary for the CTOX web surface:

- `ctox_web_search`
- `ctox_web_read`
- `ctox_deep_research`
- `ctox_browser_prepare`
- `ctox_browser_automation`

The root `ctox` binary now keeps only thin adapters plus the durable scrape
executor injection, so search/read/browser work can evolve without dragging
unrelated CTOX execution modules into the same edit surface.

The crate also exposes a focused `ctox-web-stack` binary for native platform
acceptance and diagnostics. It accepts the same `browser-prepare`,
`browser-automation`, and `browser-capture` contracts as `ctox web`, plus an
optional global `--root <path>`. Research, search, and durable scrape commands
remain available through the root CTOX daemon; the focused binary can be built
with `--no-default-features` without the Research/PDF dependency graph.

`bench/` contains the standalone regression bench for this module. It is
binary-first and data-driven so fixture and live checks can run against a built
`ctox` binary without recompiling the whole repository for every iteration.

Current ownership boundary:

- `search`, `read`, `deep-research`, `browser-prepare`, and
  `browser-automation` are owned here.
- the `web scrape` request shape and CLI contract are owned here.
- the durable scrape runtime/database still stays in the wider CTOX scrape
  subsystem, so the root injects only that executor.

## Deep research

`ctox web deep-research` runs a multi-query evidence gathering workflow over the
owned web search/read pipeline. It expands the user question across broad web,
scholarly, open-access, DOI/metadata, patent/industry, and failure-mode search
profiles, deduplicates sources, reads top pages, and returns an evidence bundle
plus a report scaffold for the agent to synthesize.

`max_sources` limits the final admitted evidence set, not the discovery pool.
The reader keeps a depth-bounded ranked candidate queue and refills from the
next candidate after an inaccessible, metadata-only, off-topic, or otherwise
rejected read. Repository metadata reads may enqueue directly linked original
data files for immediate verification. Search queries are bounded at word
boundaries, while each page read receives a source-specific relevance query so
an identifier from one repository cannot disqualify independent evidence.

Successful retrieval and evidence promotion are separate gates. An HTTP 2xx
response with a persisted snapshot proves transport and provenance only.
Deep research promotes a source into the evidence bundle only when it also has
a scored topical match, contains extracted evidence text, and is not
metadata-only, an aggregator, a third-party dataset reupload, or a
reference/link collection. Rejected reads remain in the workspace with an
`evidence_rejection_reason` for auditability.

Admitted original data files are validated by media type and file signature,
then stored under `runtime/web_search_data_cache/` using their SHA-256 digest as
the filename. Evidence receipts bind the final URL, response status, byte count,
content kind, and digest to that server-owned artifact. Large binary files are
not serialized into the JSON tool response. Systematic-research completion
recomputes the artifact digest before accepting data-backed evidence.
Repository download routes such as `.../files/archive.zip/content` are treated
as data hints, but promotion still requires matching ZIP/file magic bytes.
Large data downloads use one bounded long-running request instead of short
page-read retries. ZIP evidence additionally produces a persisted manifest with
the archive digest and each safe member's path, sizes, CRC32, and SHA-256.
Unsafe paths, excessive member counts, and excessive expanded sizes fail
closed; a transport receipt alone never proves the archive's dataset contents.

Search and page caches keep bounded JSON indexes over content-addressed response
artifacts. URL aliases do not duplicate response bodies. Oversized legacy JSON
caches are disposable acceleration state and are discarded rather than loaded
into the daemon; durable research receipts and workspace artifacts are
unaffected.

Deep research also creates a persistent research workspace by default under
`runtime/research/deep-research/<timestamp>-<slug>`. The folder contains the
full evidence bundle, source JSONL, per-source read payloads, limited raw
snapshots, figure candidates, discovered data/GitHub links, and `CONTINUE.md`
so a later agent turn can resume the same research project after context
compaction. Use `--workspace <path>` to choose the folder or `--no-workspace`
only for tests/smoke runs.

Anna's Archive support is intentionally metadata-only. The tool may use it to
discover bibliographic records when `--include-annas-archive` is explicit, but
it must not download or reproduce unauthorized copyrighted full text.

## Scholarly providers

`ctox_web_scholarly_search` defaults to `auto`, which queries Crossref,
OpenAlex, and Semantic Scholar independently, tolerates a partial provider
outage, deduplicates DOI and canonical-URL matches, and interleaves the
remaining records. Anna's Archive is available only as an explicitly selected
metadata-only provider. A failed Anna's Archive request must never turn
scientific auto-discovery into a successful empty result.

## Search providers

`ctox_web_search` defaults to provider `auto`, which cascades
`Google → Brave → DuckDuckGo → Bing` with rate-limit cooldown and a quality
gate. Set `CTOX_WEB_SEARCH_PROVIDER` in the CTOX SQLite runtime config to pin
a specific backend.

| Provider | Notes |
| --- | --- |
| `auto` (default) | Google → Brave → DuckDuckGo → Bing cascade |
| `brave` | Brave HTML scrape |
| `bing` | Bing HTML scrape |
| `duckduckgo` / `ddg` | DuckDuckGo HTML scrape (header-augmented to avoid the anomaly modal) |
| `google` | Playwright-driven Google with stealth init script + EU consent dismissal. Needs `ctox web browser-prepare --install-reference --install-browser` once; state persists in `runtime/google_browser_state/`. |
| `searxng` | Forwards to a user-hosted SearXNG instance set via `CTOX_WEB_SEARCH_SEARXNG_BASE_URL` |
| `annas_archive` | Anna's Archive metadata only |
| `mock` | Deterministic fixture provider for tests |

### Google notes

The `google` provider drives a Playwright-launched persistent-context Chromium
with stealth measures (`--disable-blink-features=AutomationControlled`,
`navigator.webdriver` masked, fake `chrome.runtime` / plugins / languages,
WebGL vendor patched) and automatically dismisses the EU cookie consent
banner. Latency is typically 1–3 s per query once the state directory is warm.

On a fully headless server without a display Google's `/sorry/index` CAPTCHA
can still trigger; the provider surfaces this as an error so the auto-cascade
can fall through to Brave/Bing/DuckDuckGo. There is no longer a separate
cookie-bootstrap profile flow — Playwright owns the entire Google path.

### Runtime config keys

| Key | Purpose |
| --- | --- |
| `CTOX_WEB_SEARCH_OPENAI_MODE` | `local_stack` / `ctox_primary` routes OpenAI `web_search` tool calls through CTOX; `openai` / `passthrough` forwards them upstream unchanged. |
| `CTOX_WEB_SEARCH_PROVIDER` | `auto` (default), `brave`, `bing`, `duckduckgo`, `google`, `searxng`, `annas_archive`, or `mock`. |
| `CTOX_WEB_SEARCH_SEARXNG_BASE_URL` | Required when `CTOX_WEB_SEARCH_PROVIDER=searxng`. |
| `CTOX_WEB_SEARCH_LANGUAGE` / `CTOX_WEB_SEARCH_REGION` | Forwarded to providers as locale/`gl` hints. |
| `CTOX_WEB_SEARCH_TIMEOUT_MS` | Per-request timeout for HTTP and Playwright paths (default 7000). |
| `CTOX_WEB_SEARCH_MAX_PAGE_BYTES` | Maximum response size for ordinary evidence pages (default 2 MB). |
| `CTOX_WEB_SEARCH_MAX_DATA_FILE_BYTES` | Maximum response size for recognized original data files stored in the hash-addressed artifact cache (default 256 MB). |
| `CTOX_WEB_AUTO_PROVIDER_BUDGET` | Max providers tried per query in `auto` mode (default 4). |
| `CTOX_WEB_BROWSER_REFERENCE_DIR` | Directory containing `node_modules/playwright`. Defaults to `runtime/browser/interactive-reference`. |
| `CTOX_WEB_EGRESS_ALLOW` | Comma-separated host allow-list that bypasses the SSRF egress guard (for deliberately-internal endpoints, e.g. a self-hosted SearXNG). Empty by default. |

These keys are read from CTOX's local SQLite runtime config store, not from
global process environment variables.

## Egress (SSRF) guard

Every fetch of an untrusted URL — the model-facing `ctox_web_read` tool,
evidence pages discovered in a SERP, open-access PDF URLs resolved from external
APIs, and deep-research snapshots — goes through `egress::SsrfResolver`, which
filters DNS results to publicly-routable addresses at connect time. Because
`ureq` re-resolves every redirect hop through the agent's resolver, this also
blocks redirect-to-internal and DNS-rebinding attempts. Loopback, RFC1918,
link-local (incl. the `169.254.169.254` cloud-metadata address), shared/CGNAT,
ULA and the IPv4-mapped forms of all of these are rejected. Operator-configured
internal endpoints are exempted via `CTOX_WEB_EGRESS_ALLOW` (and the configured
SearXNG host is auto-allowed). Scraped page content handed back to the model is
fenced with explicit untrusted-content markers so a hostile page cannot smuggle
instructions.

## Legal & ToS posture

This stack performs automated retrieval from third-party sites and must be used
within the operator's legal basis. Key points:

- **Stealth Google search.** The `google` provider drives a real Chromium with
  fingerprint-evasion (`assets/stealth_init.js`) and dismisses the EU consent /
  `/sorry` CAPTCHA. Automated, evasive scraping of Google is contrary to
  Google's Terms of Service; it is suitable for personal/operator use but is not
  a sanctioned API. Prefer an official SERP/grounding API or a self-hosted
  SearXNG (`CTOX_WEB_SEARCH_PROVIDER=searxng`) where ToS compliance matters.
- **People data (GDPR).** `person-research` and the people sources collect
  personal data of identifiable individuals. People scraping is opt-in only
  (`--include-private`, incl. the credential-free `person-discovery` source) and
  must have a recorded lawful basis and retention/erasure handling before
  personal records are persisted (see the hardening plan W2). Inferred gender
  (`person_geschlecht`) is intentionally never emitted.
- **LinkedIn / Xing.** The automatic path is API-only and never scrapes HTML. A
  separate operator-initiated, consent-based browser-assist capture exists
  behind the same Tier-C opt-in; it carries ToS/legal exposure and requires the
  operator's own credentials and a valid lawful basis.
- **Anna's Archive is metadata-only.** No full-text download or reproduction;
  open-access full text is sourced only via legal Unpaywall OA resolution.
- No `robots.txt` handling exists yet; respect target sites' crawl policies.
