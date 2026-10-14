# CLIProxyAPI upstream

- Repository: <https://github.com/router-for-me/CLIProxyAPI>
- Pinned commit: `a88197f845c979132c8978ea223c6af05cc81536`
- Commit date: 2026-08-03
- License: MIT; see upstream `LICENSE`
- Local audit checkout: `runtime/cliproxyapi-upstream` (ignored)

The Rust port is deliberately maintained and never fetches or updates upstream
at runtime. `upstream-lock.json` is the machine-readable pin and promotion
policy. `scripts/build_upstream_delta.sh <candidate-ref>` compares any locally
available candidate commit with the pin, classifies added/modified/deleted/
renamed Go files as well as dependency manifests, build/release files,
runtime assets, documentation and licenses, maps them to review modules and
emits the required action for each file without changing the pin.
`scripts/check_upstream_anchors.sh` proves
that every existing mirror still refers to the locked upstream source.

For a Go module namespace migration, the development-only classifier below
reports modified Go files whose only byte changes are parser-identified import
path tokens. It preserves aliases, comments, whitespace, function bodies and
file modes; any additional change remains manual review. It requires locally
available immutable Git objects and the existing Go reference toolchain.

```sh
GOTOOLCHAIN=local GOPROXY=off go run scripts/classify_upstream_import_migration.go \
  -repo <upstream-checkout> -base <full-accepted-hash> \
  -candidate <full-candidate-hash> -out <new-report.json>
```

An import-only candidate is review evidence, not an accepted disposition. The
report never changes the pin, review statuses, source anchors or gate credit;
affected module and full-feature tests still apply. Added/deleted files and
all other deltas remain in the canonical candidate review.

An upstream advance is a staged operation. Discovery and review preparation are
automatable; accepting a new baseline deliberately remains a gated operation:

1. Fetch a candidate explicitly into the ignored audit checkout; runtime code
   never performs this step.
2. Build `upstream-delta.json` and assign every impacted module as one porting
   unit. Added files begin as zero-credit scaffolds; modified/renamed files lose
   update readiness until their module is revalidated. Dependency, build,
   configuration, asset and license changes receive explicit non-Go review
   dispositions rather than disappearing from the scan.
3. Port the delta and run affected Rust module tests plus the corresponding Go
   packages at the candidate commit. Adapted CTOX boundaries retain their
   injected authority and receive an explicit forensic review.
4. Run both full Rust feature matrices, warning-denied Clippy, tracking and the
   standalone dashboard build.
5. Only then promote the candidate hash consistently in `upstream-lock.json`,
   `UPSTREAM.md`, source anchors and generated maps. Candidate scanning itself
   never mutates the accepted baseline.

The repeatable commands are:

```sh
scripts/prepare_upstream_candidate.sh <candidate-ref>
# port modules; fill every file disposition/evidence
# run each gate through record_upstream_gate.sh so command output is attested
scripts/record_upstream_gate.sh <review-dir>/upstream-delta.json <review-dir>/upstream-review.json rust_no_default -- cargo test --manifest-path src/core/execution/cliproxyapi/Cargo.toml --no-default-features
scripts/check_upstream_review.sh <review-dir>/upstream-delta.json <review-dir>/upstream-review.json
scripts/promote_upstream_pin.sh <candidate-ref> <review-dir>/upstream-review.json
# accepted pin is promoted, but overall completion remains false here
scripts/run_strict_umbrella_gate.sh runtime/cliproxyapi-upstream runtime/cliproxyapi-strict-receipts/<candidate-ref>/<run-id>
scripts/record_post_promotion_full_gate.sh runtime/cliproxyapi-strict-receipts/<candidate-ref>/<run-id>/strict-umbrella-receipt.json
```

`prepare_upstream_candidate.sh` is idempotent for a candidate commit and writes
an ignored, commit-addressed review directory containing the complete delta,
an initially fail-closed review ledger and a module-grouped impact summary. It
does not fetch, edit Rust sources or change the accepted pin, so a scheduler or
CI job can run it safely after an explicit fetch. Changed Go production and
test files, dependencies, build/release inputs, runtime assets, documentation
and licenses therefore all enter the same review queue instead of relying on a
maintainer noticing them manually.

`.github/workflows/cliproxyapi-upstream-watch.yml` runs that preparation daily
and on demand against a fresh isolated checkout. It publishes the immutable
delta, fail-closed review ledger and impact summary as a commit-addressed CI
artifact and writes the headline impact into the job summary. The watcher has
read-only repository permissions and never edits the accepted pin, Rust files
or dashboard; it is detection automation, not merge authority.

The same artifact contains `ctox-integration-impact.json`, generated from the
candidate delta and the Accepted-Pin-bound Track-B provider inventory. Any
non-empty upstream delta conservatively marks every provider mode and every
integration gate for impact review. This deliberately favors extra review over
silently reusing CTOX host, Business OS or Pi evidence across a changed pin.

`promote_upstream_pin.sh` refuses a mismatched checkout, incomplete file review,
project-state counter drift or any missing gate. A gate is accepted only when
`record_upstream_gate.sh` executed its recorded argv, captured a commit-local
log and stored its SHA-256; review validation rehashes every expected log and
rejects hand-toggled booleans, missing evidence or path substitution. It updates
hashes and generated artifacts only after that proof; it never fetches, merges
or resolves port changes. Promotion snapshots
all affected anchors, candidate-deleted mirrors, pins, strict-audit state and
generated artifacts before the first mutation. Header normalization is itself
staged and validates every candidate header/review pair before touching a live
mirror; modified, added and unchanged mirrors survive, while only reviewed
upstream deletions are removed. Promotion then reruns anchor and tracking checks
and restores the old accepted baseline if any mutation or generator fails. This
makes a failed promotion rollback-safe;
the completed review remains the authorization record. After all mutation and
tracking checks pass, promotion also writes a non-overwritable,
commit-addressed receipt below `docs/cliproxyapi-upstream-history/`. The receipt
embeds the full delta, every semantic file disposition and every gate
command/log hash, so accepted history does not disappear when ignored review
directories or CI artifacts expire. `check_upstream_receipt.sh` validates its
identity and completeness.

Promotion is deliberately not project completion. It advances the accepted pin
and sets `candidate_promoted=true`, but sets `accepted_pin_complete=false`,
`post_promotion_full_gate=false` and `complete=false`. The dashboard recognizes
only this exact transitional state, renders strict accepted-pin credit as zero,
and keeps the old strict audit visibly non-authoritative. Tracking skips only
that historical strict-audit binding during this exact state; pin, map, anchor
and generated-artifact checks remain fail-closed. A fresh
`run_strict_umbrella_gate.sh` receipt must start no earlier than
the immutable promotion receipt and be stored below the new pin's strict-receipt
directory. `record_post_promotion_full_gate.sh` validates every receipt log/hash,
rebinds the strict-credit audit to that receipt, reruns tracking and dashboard
validation, then atomically sets the three completion fields true. Any failure
restores project state, strict audit and dashboard. Thus neither the ten
pre-promotion gates nor promotion alone can make the dashboard claim completion.

New strict umbrella runs emit `strict-umbrella-receipt.v2`. Unlike historical
v1 receipts, v2 contains only portable-crate sources and gates: it deliberately
does not compile or hash `cliproxyapi_host.rs`. The outer CTOX host, secrets,
Business OS and Pi routing are independently evidenced by the Track-B provider
integration ledger.

Upstream Go files are mirrored as `.rs` files. Scaffold files are intentionally
outside the Rust module graph and receive no progress credit. Go `init`
registration is replaced by explicit Rust registration functions; cgo plugins
will be replaced by a process or WASM boundary rather than loaded into CTOX.
