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

Anna's Archive support is intentionally metadata-only. The tool may use it to
discover bibliographic records when `--include-annas-archive` is explicit, but
it must not download or reproduce unauthorized copyrighted full text.

## Default search provider

`ctox_web_search` defaults to DuckDuckGo (`duckduckgo`) so normal search does
not open Chrome, clone a browser profile, or depend on Google session cookies.
Google search remains available by setting the local CTOX runtime config key
`CTOX_WEB_SEARCH_PROVIDER` to `google`, `google_browser`, or
`google_bootstrap_native`.

## Google bootstrap profile

`ctox_web_search` with the explicit `google_bootstrap_native` provider fronts
Google search with a cookie profile sampled from a headed Chrome session. The
profile is persisted at `runtime/google_bootstrap_native_profile.json` with
mode `0600` — **it holds live Google session cookies (SID, __Secure-1PSID,
SAPISID, …) that are equivalent to a logged-in auth token for the signed-in
Google account**. Treat the file with the same care as an OAuth refresh
token:

- Do not commit it, back it up unencrypted, or copy it to shared hosts.
- On headless servers, sample the profile on a GUI host and transfer it via
  `ctox web google-bootstrap-import --file <path>` — the import re-applies
  `0600` permissions.
- Run `ctox web google-doctor` to check pipeline readiness (Chrome binary,
  Playwright workspace, helper binary, profile freshness, DISPLAY availability).

### Runtime config keys

| Key | Purpose |
| --- | --- |
| `CTOX_WEB_SEARCH_OPENAI_MODE` | `ctox_primary` (default) routes OpenAI `web_search` tool calls through CTOX; `passthrough` forwards them upstream unchanged. |
| `CTOX_WEB_SEARCH_PROVIDER` | `duckduckgo` (default through `auto`), `google`, `google_browser`, `google_bootstrap_native`, `bing`, `searxng`, or `mock`. |
| `CTOX_WEB_GOOGLE_BOOTSTRAP_TTL_SECS` | Proactive profile refresh window. Default `21600` (6h). |
| `CTOX_WEB_GOOGLE_BOOTSTRAP_PROFILE_PATH` | Override persisted profile location. |
| `CTOX_WEB_GOOGLE_BOOTSTRAP_PROBE` | Override probe script (used by tests). |
| `CTOX_WEB_BROWSER_REFERENCE_DIR` | Directory containing `node_modules/playwright`. Defaults to `runtime/browser/interactive-reference`. |
| `CTOX_WEB_GOOGLE_BOOTSTRAP_QUIT_RUNNING_CHROME` | `1` to explicitly allow the Google bootstrap probe to ask Chrome to quit before cloning. Default is leave running. |
| `CTOX_WEB_CHROME_BIN` | Explicit path to the Chrome/Chromium executable for `google_bootstrap_native`; not auto-discovered. |
| `CTOX_WEB_CHROME_USER_DATA_DIR` | Explicit path to the Chrome profile to clone for `google_bootstrap_native`; not auto-discovered. |

These keys are read from CTOX's local SQLite runtime config store, not from
global process environment variables.
