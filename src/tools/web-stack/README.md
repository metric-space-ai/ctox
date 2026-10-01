# CTOX Web Stack

The CTOX daemon compiles `ctox-web-stack` from the immutable Workjet Git
revision in the root `Cargo.toml` and `Cargo.lock`. `src/tools/web-stack` is
excluded from the root workspace. Editing or testing this local mirror does
not change or validate the daemon dependency.

Use these source-bound development entrypoints (local compilation still needs
the shared admission gate):

```sh
python3 scripts/web_stack_source.py describe
python3 scripts/web_stack_source.py test -- --lib unlock:: -- --test-threads=2
python3 scripts/web_stack_source.py build -- --no-default-features --bin ctox-web-stack
python3 scripts/web_stack_source.py daemon-build
```

The tool resolves the direct dependency through locked Cargo metadata, reports
its immutable commit and effective manifest, and rejects modified Git caches
or a local/path substitution. Focused checks use that source package; they do
not establish the complete daemon dependency closure or installed acceptance.
The E2E scripts require the `daemon-build` receipt and verify source state and
binary checksum before choosing a daemon, rather than borrowing an old binary
from another target directory.

The real stealth-mutation stage uses `python3 scripts/web_stack_source.py
mutation-probe` (also the `--stage2` E2E entrypoint). It copies the committed
root revision and resolved canonical Workjet revision into a task-owned
`TMPDIR` sandbox. Only that copy receives a Cargo path patch and isolated
lockfile. Its optional PDF sibling selector retains the original immutable
Git source; otherwise Cargo cannot distinguish that copied path package from
the daemon's root-local PDF package in one lockfile. Metadata must prove the
same canonical Git PDF identity, and the copied manifest is restored on exit.
Each daemon build must report the copied dependency artifact; the
runner records executable, asset and lock checksums. Cold builds have a fixed
1200-second ceiling within the unchanged 1800-second probe deadline. Command
failure and cleanup failure remain separate; denied signals require an independent
process-group check, and an unresolved group stops further probes. It requires an initial
positive probe, actual failed tests after mutation, then restoration, rebuild
and a positive control. A malformed/network failure still fails the run after
the restored control; cancellation restores the asset without claiming an
unexecuted control. Child process groups have bounded lifetimes and retained
logs in the evidence directory. The operator root, lock, runtime and shared
Cargo Git cache remain unchanged. Local compilation/browser preparation still
requires shared admission; set `TMPDIR` on `/Volumes/tmp` on the operator Mac.
The mutation runner supports Linux/macOS process-group cleanup. This does not
remove Windows standalone/platform diagnostics or other source-bound checks.
Cold locked Cargo resolution has a bounded 540-second budget; short Git reads
retain 120 seconds. The mutation phase shares its remaining deadline with final
source verification, so a completed probe cannot start a fresh full timeout
after the phase deadline or claim success without that verification.

Standalone mirror checks via its local manifest remain available for explicit
mirror/platform diagnostics. Preserve this directory: dynamic `scrape-targets`
recipes are runtime inputs even though the Rust mirror is not compiled into
the daemon. Canonical Rust changes belong in Workjet, followed by a reviewed
pin/lock update in CTOX.

The web surface includes:

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

## Public scrape fallback

Registered public scrape adapters have one bounded browser fallback for access
failures. When an adapter classifies a run as `blocked` or
`temporary_unreachable`, the Web Stack opens the source's public start page in
a source-bound Business OS browser session, handles an ordinary consent button
when present, and checks whether the browser reached a usable page. A
successful warm-up passes that session identifier to exactly one retry of the
same registered adapter.

The fallback is restricted to the canonical source host and its declared host
suffixes, passes no credentials or page content to the adapter, rejects
cross-domain redirects, and does not solve or evade CAPTCHAs. A remaining
challenge ends the fallback without another adapter attempt. Other
classifications such as `portal_drift` and `partial_output` never open this
path and continue through their existing repair/evidence handling. Research
results expose the attempt count, initial classification, and redacted browser
outcome for auditability.

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

Authenticated people-source captures expose both the ordinary per-field
ranking under `fields` and profile-bound entries under `person_records`.
Each `person_records` item keeps one person's name, function, provider profile
URL, and provenance together. Consumers that create CRM contacts must prefer
this array when present; the top-ranked `person_*` fields remain a compatible
single-value summary and must not be combined across different profiles.

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
