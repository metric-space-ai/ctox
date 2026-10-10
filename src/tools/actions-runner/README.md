# Workjet Actions engine (S2)

This is the in-tree Rust port of [nektos/act](https://github.com/nektos/act)
preserved at `e215ae179b9e6de10fea369c84f170e7916c2c1f`. Upstream-derived
source and its MIT notice are retained in [LICENSE-MIT](LICENSE-MIT).

CTOX links this workspace member with `default-features = false`. Native
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

Molecularity is a private repository. Its workflow is not copied into this
public repository; provenance records its head/path. It remains an authorized
private pilot input. Existing project workflows containing `sudo apt-get`
need the documented native-toolchain provisioning distinction; no privileged
setup command is silently skipped or run.

## Validation

All Rust commands run on the gpu3 lane, including formatting. Use the same
task for reruns:

```sh
greppy bash-smart -- ~/.codex/bin/gpu-build-run.sh \
  --owner <thread-id> --task ctox-workjet-actions-s2-host --src "$PWD" -- \
  bash -c 'ctox-prep.sh && cargo test -p ctox-actions-runner -j 2 -- --test-threads=2'
```

The workspace lockfile must be updated with the member; validation then uses
`--locked`. A default dependency-tree check must contain no Bollard.
No service is installed or reconfigured by this slice. No live welsch runs are
allowed before **12 October 2026, 13:00 Europe/Berlin**.

## Sync v3 boundary for S3

Collection contracts follow [CTOX Sync v3](../../../docs/rfcs/ctox-sync-v3.md),
RFC v2. Before S3, register the fifteen collections' profiles and field
ownership with the RFC/gate owner (Claude, “Outbound app Funktionsproblem
(fork)”). Native store is the sole authoritative writer; RxDB is a projection,
with no browser write right. The native-to-RxDB outbox is one S4 projection
writer per collection, with no intermediate projection chain.

Ledger/leases use `control`; retained log chunks use `demand`, live progress
uses `stream`. Large/cold collections must not be eager browser replication.
Fixture changes under `src/core/rxdb/` require the owner's `sync-v3:S<n>`
label and `sync-scope-guard` review. This S2 changes no Sync-core contracts.

### Collection profile/ownership proposal for the S3 owner review

Each persisted collection has exactly one profile. All fields are native-owned;
the browser has read permission only after server-side project/computer policy.
The owning native tables and projection writer are declared in the generated
fixture (Sync-v3 S2); the single native outbox → RxDB writer is registered as
Sync-v3 S4 work. Node agents submit authenticated observations/commands to the
native owner; they cannot edit projected documents.

| Collection | Profile | Scoped consumption |
|---|---|---|
| workjet_actions_nodes | control | Node status/heartbeat revisions for admitted computers; no global browser eager pull |
| workjet_actions_volumes | control | Budget/capacity status revisions for relevant nodes |
| workjet_actions_requests | control | Native accepted intake and command status; never browser-owned intake documents |
| workjet_actions_runs | control | Small run status revisions, subscribed by project/thread |
| workjet_actions_jobs | control | Job allocation/attempt/status revisions |
| workjet_actions_steps | demand | Retained step detail/results; current-step changes also ephemeral stream events |
| workjet_actions_queue | control | Fairness/admission/position revisions for the caller's scope |
| workjet_actions_leases | control | Sanitized lease/reservation status; no execution token in the projection |
| workjet_actions_log_chunks | demand | Retained compressed chunks by run/job/cursor; live tail is a stream subscription |
| workjet_actions_receipts | demand | Immutable result detail and hashes on request |
| workjet_actions_ledger | control | Native inventory/lease/delete status through bounded, authorized command reads |
| workjet_actions_quotas | control | Own-project usage/reservation/budget status |
| workjet_actions_gc_runs | demand | GC receipts, candidate/error detail on request |
| workjet_actions_decisions | demand | Candidate observations and placement reasons for explain |
| workjet_actions_policy | replicated | Small authorized project/node policy snapshot, ≤8 KB per document |

Control is command/status traffic with revisions, not an excuse to send all
historical documents to every browser. Reads/subscriptions remain capability-
and view-scoped; long detail belongs to demand. Stream progress/log events carry
a persisted cursor/revision for resumption but create no browser history and no
second projection writer. Readiness follows ctx.data and known/unknown/stale,
never collection phase guessing. The RFC owner must validate this mapping
before the S3 fixture PR and assign its sync-v3:S<n> label; no label is self-issued.
