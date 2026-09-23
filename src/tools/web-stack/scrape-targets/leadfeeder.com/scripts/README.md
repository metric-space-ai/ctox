# Leadfeeder v1 native search script

`api-v1.cjs` is a native hot-registered scraper, separate from Workjet's
legacy visitor-leads adapter. It uses the documented credit-free company
search, never enrichment, deep-data retrieval, contact search, or CRM writes.

The operator-owned native target manifest supplies `config.account_id`,
`config.credential_ref` (`ctox-secret://credentials/NAME`), and an absolute
`config.native_root` pointing at the selected CTOX root. Query input supplies
only `company` and a DE/AT/CH `country`. The key is resolved in memory through
the existing native secret CLI, never copied into runtime config or files.

One fixed-host POST, five candidates maximum, 15-second network timeout and
1 MiB response bound. A complete page must contain exactly one normalized
exact-name/country match. Truncated, ambiguous, or nonmatching results are
partial output, not success or an exhaustive no-match. Industry descriptions
are not WZ codes, and employee ranges are not exact headcounts.

The native manifest serializes `ScrapeTargetView` including its config. The
registry retains old script revisions for explicit rollback. Do not replace a
live target until its current config, revision and research routing have been
verified; registration alone does not establish Outbound integration.

Verification: `node --test --test-concurrency=1
src/tools/web-stack/scrape-targets/tests/leadfeeder-api-v1.test.mjs`.

On 2026-09-13 an isolated invocation of this script on THESEN, using its native
encrypted secret store, returned an exact Beiersdorf Manufacturing Leipzig
GmbH match with five field records and a zero-credit API receipt. A separate
Chemotechnik query returned two differently named legal entities and correctly
remained partial. These invocations did not modify the active registry or
write records into Outbound; full live research acceptance is still pending.
