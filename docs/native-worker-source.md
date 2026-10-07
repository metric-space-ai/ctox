# Native worker Git source intake

`build_source::capture_worker(worktree, staging_root, base_revision, public_base)`
captures an ancestor base, the exact HEAD commit, and the frozen working tree.
The staging root must be outside the source; the caller prevents concurrent
edits during capture. Linked worktrees are supported without exporting their
`.git` pointers or common Git directory. Git paths, content, executable bits,
literal links, staged/unstaged deletions and non-ignored untracked files retain
the build source capture contract. Index staging state is not transported;
the destination starts with HEAD's index and the captured working tree.

Public mode requires the same previously proved anonymous GitHub base as build
intake, plus the local-commit bundle and dirty overlay. Private mode sends a
self-contained HEAD ancestry bundle and the frozen tree, without fetching the
private remote. Private Git history is source data: committed historical content
is included. Client config, credential helpers, remote URLs, hooks and ignored
untracked files are not copied. No credentials are minted or resolved here.

`build_delivery::package_worker(grant, capture, staging_root, run_id,
toolchain_fingerprint)` produces owned uploads and the bounded Linux preparation
script. The source schema is `ctox.worker-source.v1`. Its identity binds the
source, base, and compiler/profile fingerprint; ordinary build recipe identity
and private plain-tree delivery stay unchanged. The caller uploads to the given
incoming directory and executes the script using its already-authorized native
transport. The package outlives the capture.

The script rebuilds a detached Git checkout and verifies HEAD, ancestor base,
independent objects and the exact frozen tree before atomic publication.
Git subprocesses ignore ambient Git configuration, authentication agents and
Git environment overrides; credentials and hooks are disabled, and repository
creation uses an empty template. Public checkout objects survive mirror removal.
Each unique run publishes to `worker-sources/<source_id>-<run_id>`, separate from
build cache sources and other worker checkouts. Repeating preparation verifies
that run's original tree; after a worker edits it, preparation refuses reuse.
Never reuse a run ID for another worker. The owning adapter disposes worker
checkouts, upload files, markers and failed partial directories after their
normal lifecycle; capture/package drops remove only their own local staging.

Preparation checks the existing lane disk floor, uses nonblocking publication
and mirror locks (exit75 on contention), and has the existing 15-minute deadline.
The service account exclusively owns the lane. This is not a code sandbox.
A reconstructed checkout proves source intake only: it does not launch a model,
authorize a fresh parent on another environment, dispatch a worker, create a PR,
or prove a build succeeded. Computer IDs remain opaque. Production fresh-parent
authorization, bounded worker dispatch, PR ownership and self-archive wiring are
separate connections.

Linux tests execute actual Git bundle/checkout and archive reconstruction for
public and private sources, including a private linked worktree, an unpushed
commit, dirty/untracked bytes, staged deletion, executable mode, literal links,
independent run checkouts, no source config/credentials/hooks, mirror removal,
and rejected reuse after tree mutation. They do not establish installed fleet
acceptance or production authorization.
