# Installed sync acceptance (DevOps goals5–7)

This runner consumes the single shared Linux package from Main
`5fbba4af6ab4bc58135d71246608484ed8364035`. It never compiles native CTOX,
changes sync production code, or restarts a host/customer service. The Mac
Workjet revision is recorded separately; this is not a Desktop or Shell UI
acceptance proof.

The preparation operator checks the finished producer receipt, package hash,
installed binary, source provenance and seven actual Shell contract files. It
extracts the official runtime into a **new** prefix and initializes new synthetic
state. It refuses reused roots and customer/DR authority. Run preparation and
browser measurement through `gpu-build-run.sh --jobs2`; dependency installation
and the browser belong to that same admitted unit. Reuse the existing shared
artifact rather than creating another native build.

Example command inside the admitted GPU host (paths are operator inputs):

```
python3 scripts/prepare-installed-sync-tenant.py \
  --producer <shared-producer-receipt.json> \
  --acceptance-base <fresh-private-prefix-parent> \
  --host gpu3 --workjet-revision <actual-installed-version-and-source>
```

Use one owned, named Playwright CLI session and its existing browser. `run-code`
imports the runner from this repository and calls:

```
const {runAcceptance} = await import('file://<checkout>/scripts/installed-sync-acceptance.mjs');
await runAcceptance(page.context().browser(), '<private-parent>/runner.private.json');
```

The callback creates two separate Chromium contexts. It opens **installed**
canonical DB, desktop schema and sync modules from the native static server.
Collections travel only over authenticated WebRTC. Native-issued browser
invitations stay in0600 private host files and browser memory; neither stdout,
receipts nor browser URLs contain them. Evidence uses a direct local cached
read and a mode=ro native SQLite readback, not an HTTP record bridge. CLI-generated
identities belong only to this synthetic tenant.

For Goal23's one-component backward/forward proof, call
`measureShellRollback(configPath)` under the same admitted ownership. It stages
the existing signed beta79 through native signature verification, activates it
in the fresh prefix and observes the changed versioned app.js. Supported
shell-update rollback restores builtin Main; the served hash and native binary
must match their baseline again. Only owned static-server groups are restarted.
Failure restoration is retained separately. A successful component proof still
does not certify the health of Workjet, WELSCH native and the installed Shell.

Goal5 performs three200-write rounds:30s offline, owned peer kill/respawn,
B reopen, A native invitation renewal and reopen with unchanged IndexedDB name.
The local write maximum and entire reconnect→exact server/B convergence
interval are recorded. No cache wipe, blind restart loop or final-count-only pass.
`setOffline` is supplemented with the documented CDP WebRTC packet-loss control;
the runner also requires zero native receipt of those200 writes before going
online again. Unsupported emulation is a harness failure, never a sync pass.
[Chrome's protocol definition](https://github.com/ChromeDevTools/devtools-protocol/blob/master/pdl/domains/Network.pdl)
specifies the packet-loss parameter for WebRTC.

Goal6 seeds10000 cached documents, closes B, applies50 updates and checks native
readback before B reopens. Local usability and catchup are separate. A missing
verified WebRTC pull-row counter or backlog indicator keeps this goal false;
HTTP asset bytes and a10000-row final cache do not prove incremental transfer.
The call-through sensor counts real masterChangesSince reply documents and
payload bytes, retaining the original request/response/checkpoint. If it attaches
after a peer is already open, the counter is incomplete and cannot certify a pass.
Shell's actual backlog UI is still a separate required observation.

Goal7 changes the browser Date clock by±10min, tests distinct-field offline
updates and same-field conflicts, and attempts a stale assumed-master revision
through the authenticated WebRTC masterWrite contract. Both conflicting values
must occur in a persisted conflict record. Native state before/after must show
that a typed rejection was unapplied. Missing APIs/revisions remain unverified.
The collection's production conflict policy is used unchanged.

All child groups are owned and bounded. The unit closes its contexts and peer /
static server on completion/failure; it never targets an externally provided PID.
The1800s deadline also closes contexts. Close the named Playwright CLI session
in the outer unit's finally/trap. Keep invitation files private; do not upload
private CLI logs or browser storage snapshots.

Copy only sanitized goal receipts, screenshots and process cleanup evidence into
`~/.codex/task-evidence/teilziele/<NN>-<slug>-<date>.json` on the Mac. Preserve
actual failures with `pass:false`. Include installed revisions, host identity,
steps, measured values, criterion and artifact references. A syntactically valid
runner or successful unit test is never evidence that a goal was reached.
