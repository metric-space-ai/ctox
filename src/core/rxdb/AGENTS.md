# rxdb-rs (CTOX Sync Engine native crate) — agent guardrails

Read `docs/ctox-rxdb.md` before changing anything here. This crate is the
CTOX side of the WebRTC-ONLY data plane to Business OS. Every rule below
exists because an agent broke it in good faith and shipped a regression.

## Hard rules

1. **No HTTP transport for collection data — ever.** No reqwest/hyper/
   tiny_http/TCP in the replication path (root `README.md`, "Data Boundary").
   An HTTP bridge is a regression, not a feature.
2. **The native peer is a passive responder.** It never initiates
   RTCPeerConnections from the signaling peer list; browsers initiate and the
   responder is built when their offer arrives (`connection_handler_rs.rs`).
   "Completing" the seemingly empty peer-list loop reintroduces offer glare.
3. **Native is always master toward `role=browser` peers.** The hash election
   applies only between non-browser peers; an empty token answer is a
   handshake failure. Do not "simplify" the election.
4. **Never hand-edit `*_contract_generated.rs`** (or the JS twins). Wire
   contracts are generated from `tests/fixtures/*.json` via
   `tools/build_webrtc_*_contract.mjs`; change the fixture, regenerate, and
   update both sides' consumers in the same commit.
5. **No new process-env toggles.** Runtime configuration flows through the
   SQLite runtime store (`runtime_env`), per the operator rules in the root
   `AGENTS.md`.
6. **This crate is NOT a cargo workspace member.** Its tests only run via
   `cargo test --manifest-path src/core/rxdb/Cargo.toml` — run that (plus
   `node src/apps/business-os/rxdb/tests/run-all.mjs` for the wire-adjacent
   JS suite) after any change here, and keep both green. Never delete or
   weaken a failing test to make a change pass.
7. **Lifecycle invariants in `src/core/business_os/rxdb_peer.rs`:** supervised
   respawn owns `NATIVE_PEER_STARTED`; bring-up failure is fatal for the run
   (no zombie "running with zero replication"); the signaling URL is derived
   per (re)connect attempt so the token freshness window never goes stale.

## Porting discipline

This is a hard fork derived from RxDB 16.20.0 concepts. `PORTING.md` is the
historical wave ledger, `PORT_STYLE.md` pins style and the sanctioned
dependency list. New dependencies require an explicit architecture decision.
