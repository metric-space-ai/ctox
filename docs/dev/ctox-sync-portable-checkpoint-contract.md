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
Any pending effect prevents a durable copy receipt, artifact restore, and workspace reconstruction.

## Restore boundary

`CheckpointStore::restore` only creates a new target directory. It verifies the
manifest and every referenced blob before writing, never overwrites an existing
target, and removes the target if any write fails. The restored layout contains
the workspace, provider state, history, attachments, and a `git/` directory
with `base-commit`, `index.patch`, and `worktree.patch`.

The artifact restore operation deliberately does not run Git, apply patches,
start a provider, or reconcile an external effect. Artifact restore is not
workspace reconstruction. Provider export/import/resume remains a separate
adapter acceptance gate for Codex and Claude; a fresh thread is not an
acceptable fallback.

## Workspace reconstruction consumer

`CheckpointStore::reconstruct_workspace` is the adapter-facing Git
reconstruction operation. It takes a verified checkpoint digest and an
explicit local repository that already contains the exact
`workspaceState.baseCommit` objects, then creates a new isolated target
worktree. It does not change `restore`, start a provider, take over
execution, or issue a durable copy receipt.

The consumer:

- loads the checkpoint and keeps pending-effect, manifest, hash, and path
  rejection;
- refuses a missing or different base object instead of substituting `HEAD`
  or fetching remotes, credentials, or other network resources;
- leaves the original source and any existing target untouched; a failed
  reconstruction removes only the directory it created;
- checks out exactly `baseCommit`, applies `indexPatch` with the index
  updated, then applies `worktreePatch` to the worktree, then installs
  `requiredUntracked` and any owner-supplied `workspace` artifacts into the
  reconstructed worktree under the same path, symlink, and collision checks;
- reads the source object format (`sha1` or `sha256`) and initializes the
  target with that format; any other format fails closed before the target
  is created;
- runs Git with a deadline, empty hook/template/config isolation, disabled
  replace-refs, `pack.threads=2`, and no inherited diff, filter, or credential helpers;
- rejects patch paths and workspace symlinks that escape the owned target,
  including chained links such as `a -> .` plus `b -> a/..`;
- rejects Git metadata path components such as `.git` (matched
  case-insensitively) in workspace artifacts, required-untracked paths,
  deleted paths, and patch paths before any target installation;
- parses mixed quoted and unquoted `diff --git` rename headers as Git
  writes them, including an unquoted destination path that contains
  spaces;
- requires Git `120000` entries that Git writes into the worktree to
  materialize as real symlinks. Checkout and `git apply --index` require
  index `120000` paths to be worktree symlinks. After the unstaged
  worktree patch, only paths that should still be worktree symlinks are
  checked, so an unstaged deletion or symlink-to-regular-file change can
  keep `120000` in the index. If a required worktree symlink is written as
  a regular file (`core.symlinks=false` or a host that has not certified
  symlink restoration), reconstruction fails closed. Non-Unix hosts fail
  closed when any Git symlink would be reconstructed. Index listings are
  streamed and filtered with an explicit 8 MiB metadata budget; larger
  listings are rejected before reconstruction can report success.
  Each symlink-validation pass bounds nested symlink expansions to 64
  follows; exceeding that work budget is rejected before reconstruction
  can report success.

This package reconstructs a Git workspace. Source-object transfer across
hosts and real provider export/resume remain subsequent integrations.

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
repository. Reconstruction tests capture staged and unstaged edits to the same
file, names with spaces and Unicode (for example `café.txt`), binary data, a
deletion, and required untracked content, then rebuild an equal
HEAD/index/worktree through the consumer. They also cover a wrong or
missing base, an existing target, corrupt artifacts, malicious patch/path
behavior, chained symlink escape, safe repeated symlink references,
cycle rejection, imported Git-metadata path rejection, real mixed-quote
rename roundtrips, and a bounded symlink-resolution work budget. Unix-only
fixtures create real Git symlink patches;
non-symlink cases remain enabled on every platform. Reconstruction streams Git
index listings larger than 64 KiB, rejects a required worktree `120000`
path materialized as a regular file, and round-trips a staged symlink
with an unstaged deletion or replacement by a regular file.
Production readiness still requires provider export/import/resume
evidence, two eligible durable data copies, and a cross-host failover drill.
Until those callers exist, this contract is a fail-closed workspace-capture
and reconstruction foundation and not a session-portability claim.
