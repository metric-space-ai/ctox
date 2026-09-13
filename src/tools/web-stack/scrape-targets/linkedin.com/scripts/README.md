# BrightData LinkedIn adapter — in progress

`brightdata-core.cjs` implements bounded profile discovery, canonical URL and
current-employer admission, and one transition of an asynchronous BrightData
profile collection. It is a library, **not yet an executable registered adapter**.
The existing target.json and live target remain unchanged until integration and
acceptance are complete. No edits to the excluded historical Rust crate or the
Cargo dependency cache are used to claim a production fix.

## Contract

The currently compiled Workjet b80535e scrape bridge supplies company, country
and source_id before its later search cascade. Therefore a collect-by-URL
script alone would be unusable in ordinary research. `discoverProfiles` accepts
company/country and one injected native search call; it keeps at most three
canonical LinkedIn profile candidates from the first twenty results. Search
snippets are never field evidence. Provider failures are not successful empty
searches. The production wrapper must bind the selected company page to the
requested legal company/country before calling `collectionBinding`.

The extraction gate requires matching requested/input/returned profile URLs,
exact current employer name and company page URL, and unmasked structured names.
Past employers, conflicting current_company_name, unrelated recommendations,
gender, guessed titles and administrative email addresses are not accepted.
Only supported first/last-name, position and profile URL fields are emitted.

`advanceCollection` requires injected `loadSecret`, `fetch`, `claimSubmission`
and `saveState`. The production implementation must provide:

- Manifest-owned encrypted CTOX secret reference, resolved only in memory.
- Atomically claimed, durable operation identity tied to the actual research
  command and query hash, not a caller-supplied arbitrary state path.
- Durable state before every subsequent transition. Unknown POST acceptance
  remains `submitting` and must be reconciled without blindly resubmitting.
- Snapshot resume with the same query, dataset and input URL set. Pending is
  not success. Completed download is still subject to field admission.
- A bounded supervisor for progress checks and normal research continuation;
  this module does not spawn pollers or background processes.
- Verified account entitlement and available free/explicitly approved quota
  before any live collection, plus native evidence persistence and app readback.

Fixed HTTPS API origin, no redirects, 20-second per-request timeout and 2 MiB
response limit. State and error envelopes do not contain the bearer secret or
raw provider/CLI errors. Snapshot/query identity failures stop admission.

## Sources checked 2026-09-13

- [Profile collection](https://docs.brightdata.com/api-reference/scrapers/social-media-apis/linkedin-profiles-collect-by-url)
- [Async requests](https://docs.brightdata.com/api-reference/rest-api/scraper/asynchronous-requests)
- [Progress](https://docs.brightdata.com/api-reference/scrapers/management-apis/monitor-progress)
- [Download](https://docs.brightdata.com/api-reference/scrapers/delivery-apis/download-snapshot)

## Verification

`node --test --test-concurrency=1 src/tools/web-stack/scrape-targets/tests/brightdata-core.test.mjs`

Ten deterministic tests pass. Network, credentials and state persistence are
injected test doubles; this is not native durable-state or live-provider proof.
Remaining: native runner/Secret Store/state wiring, verified company discovery,
registration and API entitlement, crash/restart tests, registry activation,
real DE/AT/CH research and reload acceptance. Keep the PR draft until completed.
