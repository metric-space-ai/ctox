# Workjet Actions engine (S2)

This is the in-tree Rust port of [nektos/act v0.2.89](https://github.com/nektos/act/tree/v0.2.89),
upstream commit `4f411281417e88660bea1c1a1749aa71ae0bd60f`. The unchanged
workflow schema matches that release. `e215ae179b9e6de10fea369c84f170e7916c2c1f`
is the historical CTOX import commit, not an act revision. License texts,
copyrights, source versions and translation notices are retained in
[THIRD-PARTY-NOTICES](THIRD-PARTY-NOTICES) and [LICENSE-MIT](LICENSE-MIT).

The engine is a workspace member, not a dependency of the CTOX binary: no CTOX
runtime code calls it yet. S3 must add an explicit admitted service call path.
Native
`HostEnvironment` is the execution backend. The default build contains no
Bollard dependency and cannot instantiate a Docker executor. The historical
Docker execution/lifecycle modules are gated by the maintenance-only `docker`
feature; this is not a Workjet runtime option. No Docker, Podman, VM or emulator
is offered by Workjet Actions.

S2 integrates an engine library. It does **not** activate an uncontrolled runner
or install a service on any computer. Node admission, source leases, native OS
limits, aggregate write enforcement, cancellation, scheduling and full step
orchestration follow in the service slices. The existing gpu3 lane remains the
only authorized Rust build path until the managed service proves equivalent.

## API inventory

“Parsed” means the YAML/model retains the definition; “prepared” means native
script/argv preparation; neither implies a complete workflow was executed.

| API / feature | S2 evidence and limitation |
|---|---|
| `on`, jobs/steps, needs, env/defaults, outputs, matrix include/exclude | Workflow model and unchanged act/project fixture parsing; model/matrix unit tests. Needs/matrix scheduling is not wired into a workflow executor. |
| `${{ }}`, github/env/matrix/needs/steps/runner/inputs/secrets/vars | Expression/context/interpolation tests in the port; native fixture exercises step outputs. Secret-store resolution and production log redaction remain service work. |
| `run:` shell/defaults/working-directory | `host::prepare_run_step` reuses act's preparation APIs. Linux native integration tests execute the unchanged act environment-file workflow, including PATH, multiline env and outputs, plus stdout/stderr and failing exit. |
| `hashFiles()` | Interpreter hook exists but has no workspace binding yet. The corpus asserts the explicit unbound-workspace error. Native preparation propagates it; it must never become an empty cache key or a passing step. |
| `if:`, continue-on-error, pre/main/post decisions | Ported decision/evaluator APIs and unit tests. Complete step conditions, cancellation watcher, timeout and post-action orchestration are not integrated. |
| GITHUB_ENV / OUTPUT / PATH / STATE / STEP_SUMMARY | File names and parsing helpers exist. The native integration test wires ENV/OUTPUT/PATH; STATE, summaries and persistent workflow-command interception remain executor integration work. |
| JavaScript and composite `uses:` | Action loader, fetch/pinning, pre/post and native Node/composite execution are missing. `HostWorkflow` reports `action_loader`; never silently skip actions. The Node tool-path helper alone does not execute an action. |
| checkout, cache, upload/download-artifact, setup-node, rust-toolchain | Currently the same action-loader gap. Checkout must bind to the submitted snapshot, cache/artifact actions to managed leases/quotas; no claim of end-to-end support yet. |
| Artifact/cache protocol servers | Existing HTTP protocol tests for auth, signatures, paths and storage. They are local engine protocols, not an HTTP browser/CTOX data bridge. Ledger/leases/quotas and remote artifact transfer are not wired yet. |
| reusable workflows | Inputs/context model exists; resolution/execution is missing, reported as `reusable_workflow`. |
| workflow/job concurrency | Raw YAML is inspected, but the model does not enforce concurrency. Reported as `concurrency` until the authoritative scheduler owns it. |
| `container:`, `services:`, Docker actions | Permanently prohibited by Workjet. Inspection reports typed gaps, including empty container/service maps, before execution. Local action metadata must also be checked when an action loader exists. |
| Linux/macOS/Windows | Native cross-platform source exists. S2 validation is Linux on gpu3 only; no Mac/Windows acceptance claim. |
| event triggers, workflow/job/step selection, PR gates | Future service/CLI contracts; this crate has no act-compatible CLI and does not create GitHub checks. |

`host::HostWorkflow::parse` returns both the model/document and structured
`HostGap` values with job/step locations and a reason.
`require_no_gaps` rejects the listed unsupported features. Passing that check
does not prove scheduling, limits or the entire workflow lifecycle is present.
`prepare_run_step` only prepares an individual native run step; it never
spawns a process. The admitted executor must write its script under the build
root, apply limits and file-command handling, then invoke the host backend.

## Real compatibility corpus

The unmodified public `.github/workflows/ci.yml` files from CTOX, Workjet and
Greppy are checked in under `tests/fixtures/workflows/`. Exact repository heads
and paths are recorded in `tests/fixtures/provenance.json`. Tests parse the
jobs/matrices and prove that missing action support produces a diagnostic rather
than a false green workflow. These are **parsing and gap evidence**, not runs of
those projects' complete CI suites.

The unchanged act `environment-files` workflow is additionally executed
natively by a bounded test driver. That driver is explicitly test-only; it
does not claim to implement production admission, quotas or scheduler behavior.

Private pilot workflows and their repository identifiers, commits and paths
are not copied into this public repository. They remain in authorized private
evidence. Existing project workflows containing `sudo apt-get`
need the documented native-toolchain provisioning distinction; no privileged
setup command is silently skipped or run.

## Validation

All Rust commands run on the gpu3 lane, including formatting. Use the same
task for reruns:

```sh
greppy bash-smart -- ~/.codex/bin/gpu-build-run.sh \
  --owner <thread-id> --task ctox-workjet-actions-s2-host --src "$PWD" -- \
  bash -c 'ctox-prep.sh && cargo test --locked -p ctox-actions-runner --no-fail-fast -j 2 -- --test-threads=2'
```

The workspace lockfile must be updated with the member; validation then uses
`--locked`. A default dependency-tree check must contain no Bollard.
No service is installed or reconfigured by this slice. No live welsch runs are
allowed before **12 October 2026, 13:00 Europe/Berlin**.

## Sync boundary (CTOX Sync v4 classes)

This slice registers **no** RxDB collection, wire fixture or native table and
changes no Sync-core contract. The collections below are the planned S3
surface; they are listed here only so S3 lands against the Sync v4 class model
instead of the retired Sync v3 profiles (`control`/`demand`/`stream`/
`replicated`, `demand-only`/`demand-chunks`).

Rules for S3 (Sync v4 architecture §6, §8, §16.3):

- The native Actions store in the CTOX core is the only writer. Each collection
  is written **once**, by the core, directly into the Sync store; there is no
  RxDB mirror that is projected again (no projection chain) and no browser or
  node write right. Nodes submit authenticated observations and commands to
  the native owner as command documents.
- Every collection carries exactly one class (`full`, `window`, `blob`) plus
  `client_limit` and `byte_budget` in the platform manifest. Clients only read
  what they hold locally; there is no query-demand or server read path.
- Large retained log chunks and artifacts are `blob` (file channel), never a
  chunk collection. Live log tail uses the live lane of the existing run, not a
  second collection.
- Wire contracts are generated from `src/core/rxdb/tests/fixtures/*.json`
  (or the v4 platform manifest once it replaces them); both sides are
  regenerated, never edited by hand. Core-sync paths need the owner's stage
  label and `sync-scope-guard`.

| Collection | v4 class | Rights / window |
|---|---|---|
| workjet_actions_nodes | full | admin/owner of admitted computers; one small doc per node |
| workjet_actions_volumes | full | same scope as the bound node |
| workjet_actions_policy | full | authorized project/node policy, ≤8 KB per document |
| workjet_actions_quotas | full | own project usage/reservation/budget |
| workjet_actions_requests | window | own requests, last 7 days (command-like intake status) |
| workjet_actions_runs | window | project/thread scope, last 7 days |
| workjet_actions_jobs | window | per run, last 7 days |
| workjet_actions_steps | window | per run, last 500 |
| workjet_actions_queue | window | caller's project scope, open entries |
| workjet_actions_leases | window | sanitized status only, no execution token; open + last 24 h |
| workjet_actions_receipts | window | immutable hashes/results, last 7 days |
| workjet_actions_ledger | window | admin only, bounded inventory/delete status |
| workjet_actions_gc_runs | window | admin only, last 7 days |
| workjet_actions_decisions | window | placement reasons per run, last 7 days |
| workjet_actions_log_chunks | blob | retained compressed logs via file channel, not a collection |

## Admitted host pilot (vorgabe-38)

`examples/workjet_host_pilot.rs` is a bounded operator pilot for running supplied
**trusted** PR snapshots through this engine inside `gpu-build-run.sh`. This
is native execution under the lane user, not a security sandbox: that user's
SSH identity and host files remain reachable by executed commands. Never admit
untrusted forks or unreviewed snapshots. The manifest must explicitly set
`trusted_snapshot: true`. Only workflows declaring `on: pull_request` are
selected by default; other workflows require an exact filename in the trusted
manifest's `allowed_workflows` list (default empty). Nonselected workflows are
reported `not_triggered`, without dispatch fallback or execution. An explicitly
allowlisted manual workflow uses its `workflow_dispatch` input defaults.

Its typed JSON
manifest pins the project/core/engine commits, source/root/cache paths, installed
Node binary and per-step timeout. It performs no action downloads and reads no
operator credentials. It preserves native exit codes, step conditions,
continue-on-error, environment/output/path files, bounded redacted logs and
per-workflow/job/step JSON receipts. `::add-mask::` values apply to every
subsequent step in the same job and reset between jobs. Worktrees and job temporary files are removed
after completion, including preparation failures.

Three explicit native adapters cover snapshot `actions/checkout`, verification
of an already installed `actions/setup-node` toolchain and local file capture for
`actions/upload-artifact`. They are **not** execution of the upstream JavaScript
action at its pinned revision. Unsupported inputs/actions, services, containers,
job conditions, needs/matrices and job outputs remain visible blockers. Only the
standalone pilot resolves these adapter gaps; `HostWorkflow::require_no_gaps`
continues to reject general action loading. Git metadata/generated caches and
the unrelated captured browser profile in the supplied core contract snapshot
are excluded; project sources are otherwise copied from the exact supplied head.
Legacy literal `/tmp/` evidence paths are mapped to the job's own temporary root.

The operator invokes the pilot within transient native cgroup scopes through the
existing lane, with two CPU workers, bounded RAM, timeout and a bulk-disk check.
Secrets are not resolved: requested secrets are reported unavailable. Live jobs
without an isolated target and deployment/mutation commands are blocked. The
local artifact shim records retention intent; periodic expiry, quotas and the
native ledger still require the managed service. This pilot is not S3 and does
not replace either admission wrapper or claim distributed-service acceptance.

Source receipts must use the committed final engine head. The prototype's dirty
hash includes untracked filenames but not their contents, so it is insufficient
for reusing uncommitted pilot results by hash alone; the managed snapshot manifest
must hash every submitted file's bytes.

## S3 review findings to resolve before exposing managed endpoints

- F5: authenticate v4 artifact routes; reject empty, `.` and `..` artifact IDs
  before any deletion; replace the cache server's LAN-IP binding with the
  authenticated managed transport. These legacy endpoints are not exposed by S2.
- F6: move act's runtime URL/token, XDG cache, color settings and process
  environment inheritance into typed service configuration. Do not introduce
  ambient production toggles or pass the lane user's environment to jobs.
