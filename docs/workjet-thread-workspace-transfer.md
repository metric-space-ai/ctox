# Workjet thread workspace transfer

Workjet calls the native `ctox workjet-transfer workspace-*` commands on the
computer owning each endpoint. The Workjet thread ID remains unchanged. These
commands move a Git workspace; transcript/session import, compaction, goal/loop
state, safe turn boundaries and the first target turn belong to their existing
Workjet owners. They grant no checkpoint protection or execution authority.

Stop the source at a safe tool/turn boundary before exporting. Keep it retained
until Workjet has observed a successful target turn. This component does not
commit WIP, pause a worker loop, terminate jobs or remove the source.

## Native invocation contract

Each command emits one JSON value on success and a nonzero exit on failure.
Invoke with the endpoint's existing isolated/native CTOX root and state. Source
and target must have an enrolled CTOX native transfer account and a live source
CTOX signaling/peer endpoint. Credentials remain in the local CTOX secret store.

1. On the source:
   `ctox workjet-transfer workspace-export --thread-id THREAD --move-id MOVE --source /absolute/worktree`
   returns a version 1 descriptor: unchanged `thread_id`, `move_id`, source
   instance/public identity, Git manifest, raw porcelain status SHA-256 and five
   ordered artifact references (`name`, `file_id`, `sha256`, `size`). The artifact
   names are `bundle.gitbundle`, `tracked.patch`, `index.patch`, `untracked.tar`
   and `manifest.json`. Use a new MOVE (ASCII letters/digits/hyphens/underscores,
   at most 96 characters) for every snapshot, including the return journey.
2. Pass that JSON descriptor as control metadata to the target computer. No Git
   or file payload is sent over SSH, SCP or rsync. On the target:
   `ctox workjet-transfer workspace-start --thread-id THREAD --descriptor /private/descriptor.json --source-target ENROLLED_SOURCE --target /absolute/new/worktree`
   admits five immutable peer download jobs, checks the enrolled source's
   identity, and returns their durable transfer state. A repeated start with the
   same descriptor/account/target retains the original jobs, grants and offsets.
   A partially admitted start can be retried; it never refreshes the account
   binding of a saved job.
3. The existing CTOX transfer daemon downloads and checkpoints the payload.
   `workspace-status`, `workspace-pause`, `workspace-resume` and
   `workspace-cancel` take `--thread-id THREAD --move-id MOVE`. Pause/resume
   retain bytes and original bindings; cancel is terminal. Start does not spawn
   an unowned/background payload worker. An operator test may run the existing
   bounded `ctox transfer run SECONDS` instead of a foreground CTOX service.
4. Once the jobs complete:
   `ctox workjet-transfer workspace-finish --thread-id THREAD --move-id MOVE`
   revalidates every original account/grant and completed receipt, streams and
   rehashes their private content objects, reconstructs Git in an owned sibling
   stage, compares the full file/index/HEAD/branch manifest and exact porcelain
   status digest, revalidates grants again, then publishes with an atomic
   no-replacement rename. It returns `imported:true`, the target path, Git proof
   and status digest. A lost response can be retried while the imported target
   remains unchanged. Changes after the first target turn cannot be overwritten
   by replaying the import.

The source export is durably bound to its original canonical source path. A
retry fails if that source has changed. The target operation is durably bound to
its thread, descriptor, source account and target path. All five jobs must retain
one original account epoch/principal. Each artifact retains its own file grant.
The publication/consumption path is the production CTOX `desktop_files` peer
transport, not a browser HTTP data bridge. No caller clean/ownership booleans
are accepted.

## Git fidelity and platform scope

The bundle contains HEAD ancestry and the current branch; it does not copy a
linked worktree's `.git` pointer. The target gets its own real Git directory.
Combined working-tree and separate cached patches preserve staged and unstaged
changes independently, including a newly staged file subsequently deleted from
the working tree. Untracked regular files and symlinks retain their hashes and
modes. Ignored files, build caches, other worktrees and unrelated branch refs are
not workspace artifacts. Workjet's repository binding owns remote URL setup;
credentials and machine-local remotes are not copied from Git config.

Import publication is certified for Linux/macOS. No Windows durable publication
claim is made. Empty/nonempty existing destination paths are never replaced.
The prior local `pack` and `apply` interface remains available.

## Acceptance

Run a linked worktree with staged, unstaged and untracked changes Mac → gpu3 →
Mac through these commands and existing CTOX peer admission. Compare the raw
`git status --porcelain=v1 -z --untracked-files=all`, HEAD, branch, index tree,
working-tree diff and file manifest at all three points. Record exact installed
source/target revisions, both elapsed times, transfer receipts and retained
source paths. Unit tests cover local linked-worktree roundtrip, changed-source
proof rejection, foreign thread/artifact/binding rejection, atomic record and
no-replacement publication, and exclusive move writers. Unit tests alone are not
installed two-host acceptance.
