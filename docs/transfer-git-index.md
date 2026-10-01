# Git staging fidelity for Workjet transfer

The transfer pack preserves HEAD history and the selected branch through the
existing Git bundle. `tracked.patch` represents HEAD to working files;
`untracked.tar` retains untracked files and symlinks. New packs also carry
`index.patch`, a binary HEAD-to-index patch, and optional `git.index` proof:

```json
{"tree":"<40-or-64-hex Git tree>","patch_sha256":"<64-hex SHA-256>"}
```

The canonical manifest hash binds this proof. Apply verifies the index patch
hash before use, restores the working patch without staging it, applies the
index patch with `--cached`, and compares `git write-tree` with the proof.
Only then can the existing file-manifest verification and target publication
finish. Publication flushes every materialized regular file (including Git
objects, refs and index), then directories from children to parents, then the
target's parent after the rename. Symlinks are never traversed. Directory
durability is currently Unix-only; other platforms fail without a success receipt.
Pack creation also flushes the bundle, both patches, untracked archive and
manifest, then the artifact directory and its parent, before returning success.
Index-only changes count as dirty even if working bytes match HEAD.
Unresolved merges and intent-to-add entries are rejected; the latter has no
ordinary Git tree representation. This does not claim support for arbitrary
index flags, submodules, every ref, or an independently running source writer.
The existing owner must quiesce the source before packing. `--source` must name
the worktree root. The artifact directory must be outside both that worktree
and its private/shared Git metadata; symlink aliases are resolved before any
output directory is created, and Unix ancestor identities also guard against
case aliases. Packing into the source is rejected before replacing artifacts.
Existing artifact entries are unlinked before replacement, so symbolic or hard
links cannot cause truncation of source files. The owner must hold exclusive
use of the artifact directory while packing as well as quiescing the source.

Old manifests without `git.index` remain readable with their original behavior:
the combined patch is staged at the destination. They cannot prove preservation
of the source's original staging state. Old strict receivers reject new proof
fields; a caller must not strip the proof or silently downgrade a transfer.

The `pack_complete` command carries the same optional `git.index` object plus
`patch_file_id`. That ID must name a distinct entry of `artifact_file_ids`; the
server validates and persists the complete proof. The collection schema and
native/browser schema hash registries evolve together. The current command
handler is limited to SHA-1 repositories (40-character head/base/tree), even
though standalone manifest syntax accepts 40- or 64-character object IDs.
Missing or malformed members of an advertised index proof fail closed.

Content reconstruction does not authorize execution or takeover. Existing
Workjet/Crew ownership, fence generation, peer admission and durable checkpoint
rules still apply. Local pack/apply tests do not establish two-host acceptance.

## Native transfer workflow

Use the identified native executable with the existing `CTOX_ROOT` bundle/source
selection and `CTOX_STATE_ROOT` state-directory override for isolated instances.
The core transfer metadata, downloaded objects and native query/admission
databases all follow that same state directory. Use the same selections for
every command and daemon restart; an installed launcher may override them.
Create the isolated bundle/source directory before invoking commands. On a fresh
A instance, run `ctox sync init` with those selections before starting its peer,
reading its public identity or publishing files. This explicitly provisions the
source signing identity in that instance's secret store; repeating it retains
the existing identity. Identity reads and publication do not initialize it.
For an isolated acceptance run, A can host its source peer with
`ctox business-os peer start`; B can run the ordinary transfer worker with
`ctox transfer run 300`. The latter owns only the transfer worker, drains it
when the requested 1–3600 second window ends, and returns worker/shutdown
errors. Its `stopped` output is not a job-completion receipt: inspect each job
with `transfer status`. Keep both commands under the existing bounded process
supervisor; restarting B uses the same root, state, jobs and account.

Before the first enrollment, obtain A's public identity with
`ctox transfer source-identity` over the trusted operator channel. On A,
`ctox business-os mobile-invite create --ttl-seconds 300 --display-name transfer-acceptance`
creates a one-time native invitation. Keep that JSON in a mode-0600 temporary
file and deliver it through the authorized operator channel, then run on B:
`ctox transfer pair TARGET SOURCE_PUBLIC_IDENTITY INVITE_FILE`. Reuse that saved
target for resume; creating a replacement invitation is not job recovery.

On A, quiesce the source and run
`ctox workjet-transfer pack --source SOURCE --artifacts ARTIFACTS`.
Publish each of `bundle.gitbundle`, `tracked.patch`, `index.patch`,
`untracked.tar` and `manifest.json` with `ctox transfer publish FILE`.
Keep their returned file IDs, byte hashes and sizes associated with those
exact filenames; zero-byte patches are valid artifacts.

On B, use an independently pinned native enrollment of A and enqueue each
artifact with `ctox transfer peer-download ID TARGET SHA256 SIZE FILE_ID`.
The daemon owns the downloads; pause/resume uses the same job IDs and original
source grants. Wait until each `ctox transfer status ID` reports `state` as
`completed`. Its `receipt.artifact` names the verified local file. Copy that
file under the corresponding artifact filename in a new directory; keep the
daemon-owned object in place. After all five artifacts are present, run
`ctox workjet-transfer apply --artifacts ARTIFACTS --target DESTINATION`.
Apply still validates the manifest, patches, index proof and materialized tree
before publishing a destination. Do not apply partial downloads, replace the
original grant with another account, or treat content reconstruction as a
Workjet/Crew execution takeover. Real two-host execution of this workflow is
still required for acceptance.
