# src/core/business_os — Claude guardrails

Identical rules to `AGENTS.md` in this directory — read that file and
`docs/ctox-rxdb.md` before changing anything here.

Summary: the Business OS data plane is WebRTC-only (no HTTP fallback for
collections/commands/files/manifests/status); `rxdb_peer.rs` lifecycle
invariants are load-bearing (supervised respawn, fatal bring-up, fresh
signaling tokens, `replicationUp` heartbeats); the schema-hash fixture and
the browser registry must change together; no new process-env toggles; keep
the rxdb crate tests and the JS smoke suite green.
