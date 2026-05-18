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
| `CTOX_WEB_AUTO_PROVIDER_BUDGET` | Max providers tried per query in `auto` mode (default 4). |
| `CTOX_WEB_BROWSER_REFERENCE_DIR` | Directory containing `node_modules/playwright`. Defaults to `runtime/browser/interactive-reference`. |

These keys are read from CTOX's local SQLite runtime config store, not from
global process environment variables.
