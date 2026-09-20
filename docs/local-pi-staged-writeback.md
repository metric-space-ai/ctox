# Bounded local Pi writeback candidate

This follow-up is separate from PR142. It is not validated or deployable yet.

Operator-owned local modules are copied to an isolated sibling workspace. The existing module validator runs in local mode with a 30-second Unix process-group bound and without running app-supplied tests. The candidate cannot change module.json or collections.schema.json. Source paths and symlinks fail closed. Full source trees are bounded to 64 MiB and 4096 files.

The complete baseline is checked before staging; current native command authority, selected local provenance and whole live-tree fingerprints are checked before publication. Source file saves and version rollback share a fail-closed local writer lease. Stale leases require explicit recovery, not automatic lock removal. An original full directory, baseline version and prepared/activated/complete receipt are retained beside the local module. Two directory renames publish the prepared tree; this is not a claim of a power-loss atomic filesystem transaction or a distributed reader snapshot.

The existing ctox.coding.turn path re-resolves its signed/native identity and AppsModify permission after the model returns and after staging validation. The operator CLI continues to have explicit operator authority. This change does NOT grant managed Gateway JSON roles new coding.turn authority. PR142's managed app authority remains limited to create/modify; do not claim an MCP coding route for an unprovisioned Gateway principal.

Historical finding at 107e8badf: the checker dynamically imported candidate schema and record helpers despite `--skip-tests`. The repair candidate moves those evaluations to a separate semantic worker. Static schema parity and normalized field-type checks remain in the parent; the worker has read-only candidate/runtime access, no host-network access, a cleared environment, bounded output and a five-second timeout. macOS uses `/usr/bin/sandbox-exec` (Seatbelt); Linux requires `/usr/bin/bwrap` with user/network/PID isolation. Missing or failed isolation rejects validation. Host read/write, candidate write, network and timeout regressions are added but must pass on each supported deployment platform before this blocker is closed. App-supplied tests remain disabled on the local Pi path. No isolation success is claimed merely from these source changes.

Pending before release:


- Compile and execute the local_coding regressions and existing Pi/native policy suites.
- Verify the real local validator against a representative complete app, including failure/timeout cleanup; static validation is not browser acceptance.
- Add command-path revocation and forged-identity regressions and resolve the authenticated Gateway coding entry if required by the actual invocation.
- Audit remaining module writers for lease participation, and verify activation/metadata failure and crash recovery. A retained backup alone does not prove automatic rollback.
- Expose an exact release/rollback receipt to callers, verify projected source/catalog refresh and live reload, and document recovery for prepared/activated receipts.
- Preserve campaigns/data and local provenance; no installed shadow or reinstall.

The shared PR186 proof for PR142 and the follower-shell fix does not include this candidate. Do not add it to that release before focused review and validation.
