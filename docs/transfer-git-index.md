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
The existing owner must quiesce the source before packing.

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
