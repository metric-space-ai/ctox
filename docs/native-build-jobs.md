# Native registered-computer builds

The CLI `ctox build-job --input <private-json-file>` authenticates a current unbound Business OS Owner/Admin capability. The JSON envelope is `{"capability_token":"…","request":{"action":"…"}}`. The file must be a private regular file of at most 1 MiB; tokens are never saved in jobs or sent to the build computer. Device-bound capabilities require their existing proof-of-possession path and are rejected here.

Actions:
- `profile`: `computer_id` and a typed Rust profile (`name`, `home`, `bin_dirs`, absolute `rustc/cargo/cc/cxx/protoc/node/libclang_dir`, optional absolute `ctox_prep`, typed `library_dirs` and `protoc_include`). Profiles live in native SQLite, owner/computer scoped; they require an assigned matching build grant.
- `submit`: `source_root`, existing `staging_root` outside the source, `toolchain`, optional opaque `computer_id`, stable `task_id`, explicit `timeout_seconds` (1–86400), optional `public_base` with anonymous GitHub repository/revision, and `recipe`.
- `step` / `status`: `job_id`. `list` accepts an optional `after_job_id` and returns at most 100 owner jobs.

A recipe has `kind: build|check|test`, `release`, optional source-relative `manifest_path`, optional `package`, and optional `test_filter` for tests. Cargo uses the granted jobs cap, locked dependencies, a source/compiler-specific target, and explicit test workers. It accepts no arbitrary shell command or caller-supplied worker override. CTOX profiles call the configured prep helper before Cargo.

Selection reads the existing native registry and observes the granted prototype slot locks/disk floor, with one SSH attempt per candidate and no host table. A selected computer still acquires its remote slot at launch. A missing profile, stale observation or unavailable slot is not a successful build.

Admission freezes tracked and nonignored untracked source (including local edits) into the caller's disposable staging volume. Public mode first verifies that the chosen base is anonymously advertised by GitHub, then sends a commit bundle and overlay. Private mode sends the complete frozen tree. No GitHub bearer or Git credential configuration is copied.

Rustc/Cargo must be direct binaries; context-sensitive rustup shims are rejected. The compiler probe has a seven-second deadline. The target identity hashes actual compiler/library assets, version output and typed profile together with source identity. Before launch the compiler fingerprint is checked again. Endpoint/grant/credential fingerprints and the original owner's revocation epoch are retained; changed authority requires a new job rather than rebinding an old one.

Each step sends at most four 512 KiB chunks or advances one remote stage. Every acknowledged chunk is checkpointed before the next send. Restart after a lost response compares the same bytes/script/run. Source preparation and the build are detached with deadlines. Binary logs are read in 64 KiB cursor pages, written/fsynced locally and replayed idempotently. Completion waits for explicit exit status and drains the remaining log. SQL claims are fenced by owner, revision, generation and token, and released before a normal step returns.

The lane account owns its directories exclusively. This capacity mechanism is not an untrusted-code sandbox. A launcher failure after creating its directory can remain unresolved rather than being relaunched implicitly; the receipt records actual observed state. Concurrent operator source edits during capture remain disallowed by the source-intake contract.

The SQL admission and checkpoints are durable; frozen uploads/logs remain in the explicitly supplied staging directory until owned lifecycle cleanup. On Michael's Mac this directory belongs under /Volumes/tmp, which can be wiped in four days. Do not describe a job as resumable after its local source was lost. Remote artifacts and compiler targets need the existing build-lane/TransferEngine retention policy.

Real gpu4 CTOX execution, NAS offload, and installed Workjet visibility are acceptance work after merge; unit/fixture checks do not establish those outcomes.
