# rxdb-rs — Claude guardrails

Identical rules to `AGENTS.md` in this directory — read that file and
`docs/ctox-rxdb.md` before changing anything here.

Summary of the hard rules: WebRTC-only (no HTTP transport in the replication
path); the native peer is a passive responder (never initiate from the peer
list); native is always master toward browsers and an empty token answer
fails the handshake; never hand-edit generated contract files; no new
process-env toggles; the crate is not a workspace member — run
`cargo test --manifest-path src/core/rxdb/Cargo.toml` and
`node src/apps/business-os/rxdb/tests/run-all.mjs` and keep both green.
