# src/core/business_os — agent guardrails

This directory wires the CTOX daemon to Business OS. The data plane between
the browser and this daemon is **WebRTC-only** (CTOX Sync Engine / ctox-rxdb): Business
OS collections, `business_commands`, `ctox_queue_tasks`, desktop files/chunks,
module manifests and runtime status replicate ONLY over RxDB/WebRTC and
persist in the RxDB store opened by `rxdb_peer.rs`. See `docs/ctox-rxdb.md`
and the root `README.md` "Data Boundary".

This directory also owns server-side Business OS policy, module lifecycle,
MCP delegation, app/source mutations, command execution, and projections from
core CTOX state into the RxDB document store. Browser UI helpers may mirror
permissions for ergonomics, but server-side policy remains authoritative.

Hard rules:

1. **Never add an HTTP fallback/bridge for those records** — not in
   `server.rs`, not anywhere. HTTP serves the static shell and bootstrap
   config only. Past agents have tried this repeatedly; every attempt was a
   regression and was reverted.
2. **`rxdb_peer.rs` lifecycle invariants** (see its file header): supervised
   respawn owns `NATIVE_PEER_STARTED`; WebRTC bring-up failure is fatal for
   the run (never "log and keep running" — that is a zombie peer); heartbeats
   carry `replicationUp`; the signaling URL is re-derived per (re)connect so
   the token freshness window never goes stale.
3. **`business_os_schema_hashes.json` is the schema-hash fixture** the browser
   registry (`src/apps/business-os/rxdb/src/schema.mjs`) must mirror —
   regenerate both sides together, never edit one side alone.
4. **No new process-env toggles** — runtime config flows through the SQLite
   runtime store (root `AGENTS.md` operator rules).
5. **Policy-gated mutations stay server-gated.** Any action that installs,
   uninstalls, releases, rolls back, assigns, edits, views source for, or writes
   data through a Business OS app must pass through `policy.rs` or an explicit
   equivalent server-side check. Do not rely on browser-only hiding/disablement.
6. **Business OS MCP is control/delegation, not sync.** MCP queries/actions must
   respect local store policy and must not become a backdoor HTTP data bridge.
7. After changes here run `cargo check`,
   `cargo test --manifest-path src/core/rxdb/Cargo.toml`, and
   `node src/apps/business-os/rxdb/tests/run-all.mjs` — keep all green.
