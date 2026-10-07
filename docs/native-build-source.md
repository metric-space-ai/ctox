# Native build source intake

Git-aware worker capture and isolated reconstruction are documented in
[native worker source intake](native-worker-source.md). Ordinary private build
delivery remains a plain frozen tree.

`business_os::build_source::capture(worktree, staging_root, public_base)` freezes
Git-tracked and non-ignored untracked files under a unique disposable directory
outside the worktree. The caller prevents concurrent edits during capture and
keeps the returned value alive until upload finishes. Dropping it removes its
own staging directory, never the original checkout.

The returned `source_id` hashes captured HEAD, ordered file names, file contents,
executable mode, symlink targets and deletions. In particular, changing the
contents of an existing untracked file changes the identity. Remote builds and
Cargo caches use this identity rather than a mutable task checkout. Upload reads
`tree()` from the frozen capture even if the original checkout changes later.

For a public GitHub repository, the native adapter first proves that the build
host can fetch the selected base commit anonymously. It then passes the validated
owner/repository and full base SHA to capture. The build host fetches that base,
imports `bundle` when HEAD contains local commits, checks out `head_revision`,
applies the frozen `overlay_paths` and removes `deleted_paths`. A temporary named
local Git ref pins bundle creation and is removed afterward. Source intake does
not perform network access or copy the client's Git authentication state.

For private repositories, the adapter uploads the frozen full tree. It does not
send `.git` or non-tracked ignored files. Paths are UTF-8 relative Git paths;
newlines and quotes remain literal, and transport must use framed paths rather
than shell interpolation. Symlinks are captured without following their targets.
Replaced parent symlinks, submodules, special files and unsupported paths fail
closed; submodules need separate explicit intake. Source commands are trusted
owner work and this module does not provide an execution sandbox.

This package supplies source preparation for the registered-computer adapter.
SSH transport, remote capability/credential validation, slot admission, durable
job receipt/log return and installed fleet acceptance remain in that adapter.
Tests reconstruct an unpushed public commit plus dirty overlay/deletions, exercise
untracked-content identity and frozen-byte lifetime, and reject a replaced parent
symlink while preserving executable mode.
