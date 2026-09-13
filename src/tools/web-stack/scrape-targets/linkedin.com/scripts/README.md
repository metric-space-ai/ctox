# BrightData LinkedIn adapter — in progress

`brightdata-core.cjs` implements bounded profile discovery, canonical URL and
current-employer admission, and one transition of an asynchronous BrightData
company/profile collection. `run-brightdata.cjs` now supplies the executable
two-stage integration. The existing target.json and live target remain unchanged
until integration and acceptance are complete. No edits to the excluded historical Rust crate or the
Cargo dependency cache are used to claim a production fix.

## Contract

The original Workjet b80535e scrape bridge supplies company, country
and source_id before its later search cascade. Therefore a collect-by-URL
script alone would be unusable in ordinary research. `discoverProfiles` accepts
company/country and one injected native search call; it keeps at most three
canonical LinkedIn profile candidates from the first twenty results. Search
snippets are never field evidence. Provider failures are not successful empty
searches. `discoverCompanies` uses the same bounded native-search contract for
company-page candidates. `verifyCompanySnapshot` admits exactly one matching
name and requested country from a complete, URL-bound company-dataset snapshot;
errors, missing rows, duplicate URLs and ambiguous matching companies fail
closed. It treats the provider's comma-separated country codes as reported
country presence, not proof of registered headquarters or legal registration.
Employee previews and related companies are never person evidence. The runner
must use `companyCollectionBinding` to collect and verify that dataset/snapshot,
then use the admitted company URL in `collectionBinding`. `advanceCollection`
now supports both allowlisted datasets, including dataset-bound progress and
complete company-snapshot validation. A verified company result is intermediate
identity evidence, not a finished adapter/person result. Company and profile
journals are isolated within the same operation; profile journal paths remain
compatible. Query changes within either stage still fail closed.

The extraction gate requires matching requested/input/returned profile URLs,
exact current employer name and company page URL, and unmasked structured names.
Past employers, conflicting current_company_name, unrelated recommendations,
gender, guessed titles and administrative email addresses are not accepted.
Only supported first/last-name, position and profile URL fields are emitted.
Masked/control-bearing positions are omitted without discarding independently
verified names or the profile URL.

`advanceCollection` requires injected `loadSecret`, `fetch`, `claimSubmission`
and `saveState`. `brightdata-state.cjs` now supplies the latter two through an
append-only POSIX operation journal: complete fsynced files are published with
atomic no-replace hard links, followed by directory fsync. Revisions form a
bounded contiguous hash chain; stale writers conflict rather than overwrite.
Unchanged pending/completed observations reread the full chain and no-op only
when the observed revision/hash still match; they do not consume revisions.
This preserves the native 120-poll budget while a stale process still conflicts.
State roots/revisions reject symlinks and inappropriate permissions. Only
allowlisted state fields are persisted; no credentials or raw provider bodies.
An explicit POST HTTP401 persists `rejected`, permitting at most one further
submission with credentials loaded again. The journal enforces the monotonic
attempt count and exact query binding; ambiguous transport/acceptance still
remains `submitting` and cannot retry automatically. Other provider rejections
are not broadened into this safe-reauthorization exception.
The runner now takes the command-derived operation ID from the native input and
derives its protected state directory from the native target workspace, never an
input-provided path. Immutable, fsynced receipts bind the original raw input and
both discovery searches before any provider POST. Resumes reuse those candidates;
changed input fails closed. A verified company receipt is bound to the completed
company journal and avoids redownloading it while the profile job is pending.

`brightdata-continuation.cjs` now maps internal pending collection transitions
to the native `ctox.scrape.provider_continuation.v1` receipt. The native scrape
executor validates current run/input/operation/target and company/country,
persists `awaiting_provider`, and neither materializes records nor queues repair.
The runner must emit the projected receipt and exit zero, never print the core's
internal `temporary_unreachable` pending result. See
`docs/scrape-provider-continuation.md`. Both datasets project typed native waits.
Workjet099758 and CTOX163 implement bounded command wake/resume and completed-
source preservation; their native tests and actual recovery integration proof
remain outstanding before activation.

The implementation and activation contract requires:

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
- [Company collection](https://docs.brightdata.com/api-reference/scrapers/social-media-apis/linkedin-companies-collect-by-url)
- [Async requests](https://docs.brightdata.com/api-reference/rest-api/scraper/asynchronous-requests)
- [Progress](https://docs.brightdata.com/api-reference/scrapers/management-apis/monitor-progress)
- [Download](https://docs.brightdata.com/api-reference/scrapers/delivery-apis/download-snapshot)

## Verification

`node --test --test-concurrency=1 src/tools/web-stack/scrape-targets/tests/brightdata-core.test.mjs src/tools/web-stack/scrape-targets/tests/brightdata-state.test.mjs src/tools/web-stack/scrape-targets/tests/brightdata-continuation.test.mjs src/tools/web-stack/scrape-targets/tests/brightdata-company-flow.test.mjs src/tools/web-stack/scrape-targets/tests/brightdata-runner.test.mjs`

Forty JavaScript tests pass (zero failures/skips, latest7284ms). The company
flow uses actual journals across reopenings, exactly one POST per dataset,
separate snapshots under the same operation, company-to-profile identity binding,
wrong-dataset rejection and ambiguous-acceptance no-resubmit. Native tests remain
pending coordinated execution. Core tests inject network/credentials/persistence. State
tests use actual files, reopen journals, race two bounded child processes and
exit a child immediately after its durable claim, and restart after a persisted
HTTP401 to verify bounded reauthorization. Mac tests require TMPDIR on
/Volumes/tmp. These are not a native research-resume or live-provider proof.
Remaining: real native Secret Store/search integration verification,
registration and API entitlement, native command crash/restart tests, registry activation,
real DE/AT/CH research and reload acceptance. Keep the PR draft until completed.

## Executable integration (new; activation still pending)

Native `register-script` stores one immutable script revision; it does not copy
sibling CommonJS files. Build the reviewed modules into a self-contained artifact
using `bundle-brightdata.mjs --output <absolute disposable path>`, then pass that
artifact to `ctox scrape register-script --target-key linkedin-com --script-file
<artifact> --language javascript`. The bundler uses only Node built-ins and the
four checked-in modules, with no fetched code or package installation. Registration
and target changes still require the normal authorized control-plane workflow.

`../brightdata.target.json` is the explicit API-mode target template. It keeps
`brightdata_collection_authorized` false until account entitlement and free or
explicitly approved quota are verified. Its credential reference is a template:
the operator must bind the actual encrypted Crew API key, not merely assume a key
with this name exists. Neither the template nor runner has been activated live.

The runner validates the native target/run/manifest paths, source and operation,
then loads the manifest-owned `ctox-secret://credentials/...` reference through
the exact native `CTOX_BIN`. Secret JSON is captured in memory with an 8-KiB cap,
20-second subprocess bound and no stdout/stderr propagation. It is not placed in
environment variables, arguments, journals, provider error envelopes or generated
automation source. JavaScript strings cannot promise memory zeroization.

Discovery uses the existing native `ctox web search --query ... --country ...
--domain linkedin.com --context-size low` contract (bounded 2-MiB captured JSON).
Each search and provider request is independently bounded to 20 seconds. A first
turn may perform two searches plus one submission; the existing native executor's
120-second default bounds the complete script tree. No polling subprocess survives
the turn. Native scrape execution now explicitly injects its authoritative
`CTOX_ROOT` after clearing ambient environment so nested CLI reads use this instance.

`brightdata-runner.test.mjs` runs the actual generated single-file entry point
in separate processes with real immutable receipts/journals and synthetic native
CLI/provider I/O. It covers two-stage resume, no repeated searches/company download,
changed-input rejection, manifest-only authorization, secret-error redaction,
unknown-acceptance no-resubmit and symlink/path rejection. These tests are not a
real API, native command-recovery or live registration proof. Native command
recovery additionally asserts exact instance-root propagation to both adapters.
