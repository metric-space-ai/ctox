# CTOX portable execution checkpoint contract

This document defines the portable checkpoint payload required before a
provider adapter may claim session portability. It is a storage contract, not
an adapter implementation. The checkpoint manifest is version `2`; version 1
manifests are rejected so an old snapshot cannot be mistaken for a complete
workspace.

## Capture boundary

Capture is quiescent. The executor must stop accepting new turns, flush the
execution journal and attachments, and persist provider state before publishing
the manifest. The capture producer records:

- the full Git object ID of `HEAD` as `workspaceState.baseCommit` (a 40 or 64
  character lower-case hexadecimal object ID);
- the staged diff as a content-addressed `indexPatch` artifact;
- the unstaged diff as a content-addressed `worktreePatch` artifact;
- every required untracked file as a `requiredUntracked` workspace entry;
- every deleted path in `deletedPaths`;
- the existing journal, attachment, workspace, and provider-state artifacts.

Patch artifacts may be empty, but they must still be present and hash-verified.
Required untracked paths are restored into the workspace and may not collide
with workspace entries. Deleted paths may not also be present. Paths are
case-folded for collision checks and must pass the existing traversal,
symlink, and platform-reserved-name checks.

Credential bytes, live database files, and unresolved external effects are not
portable payloads. The session manifest may contain credential references only.
Any pending effect prevents a durable copy receipt and prevents restore.

## Restore boundary

`CheckpointStore::restore` only creates a new target directory. It verifies the
manifest and every referenced blob before writing, never overwrites an existing
target, and removes the target if any write fails. The restored layout contains
the workspace, provider state, history, attachments, and a `git/` directory
with `base-commit`, `index.patch`, and `worktree.patch`.

The store deliberately does not run Git, apply patches, start a provider, or
reconcile an external effect. The future adapter must verify that the checkout
is exactly `baseCommit`, apply the staged and unstaged patches in that order,
install required untracked files, and refuse resume if any step is ambiguous.
Provider export/import/resume remains a separate adapter acceptance gate for
Codex and Claude; a fresh thread is not an acceptable fallback.

## Capture producer

`ctox_sync::capture::CaptureRequest` and `CheckpointStore::capture` provide the
first real producer for this format. The asynchronous producer runs Git with a
10-second deadline, captures `HEAD`, staged and unstaged binary diffs, staged
and unstaged deletions, and every untracked file reported by Git. Capture must
start at the repository root. Patch output disables text conversion and relative
paths and uses fixed `a/` and `b/` prefixes so local display settings cannot
change the bytes or patch paths. Untracked paths come directly from Git’s
NUL-delimited file listing, independently of rename status records. It rejects
symlink traversal, non-UTF-8 paths, unbounded files, invalid Git output, and a
workspace without a verifiable commit. Journal, attachment, workspace, and
provider artifacts are supplied by the execution owner inside the same
quiescent boundary and are ingested through the same hash-verified store.

The producer publishes only after the complete manifest validates. It does not
acknowledge a replica, apply the patches, or start a provider; those actions
remain explicit authority and adapter steps.

## Evidence required before production use

The contract tests cover round-trip hashing, Git metadata restoration,
case/path collision rejection, deleted/untracked overlap rejection, corrupt
copy rejection, pending-effect blocking, and capture from a real temporary Git
repository. Production readiness still requires a real Git reconstruction
consumer, provider export/import/resume evidence, two eligible durable data
copies, and a cross-host failover drill. Until those callers exist, this
contract and producer are a fail-closed foundation and not a portability claim.
