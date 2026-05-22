# ctox-rxdb-js Hard Fork

This directory is the CTOX-owned JavaScript RxDB hard-fork control surface.
The upstream source snapshot remains pinned under `../vendor/rxdb-16.20.0/`;
the fork exists so Business OS can evolve RxDB exactly for CTOX's data-plane
needs while `rxdb-rs` evolves in lockstep.

## Product Rule

CTOX Business OS is the source of truth. Upstream RxDB compatibility is useful
only where it does not weaken CTOX requirements for:

- WebRTC-only browser/native replication;
- deterministic peer restart and reconnect behavior;
- typed replication/signaling diagnostics;
- protocol and schema hash negotiation with `rxdb-rs`;
- file metadata, chunk, generation, and lazy-materialization semantics;
- release-matrix evidence across direct, web-deploy, desktop, and private/NAT
  launch modes.

## Baseline

- Upstream provenance: `pubkey/rxdb` tag `16.20.0`
- CTOX source snapshot: `../vendor/rxdb-16.20.0/`
- Hard-fork source: `source/`
- Browser runtime bundle today: `../../apps/business-os/vendor/rxdb-bundle.mjs`
- Rust peer: `../src/`

The hard fork must not be represented only as edits to the generated browser
bundle. Source-level fork decisions belong here first, then the browser bundle is
rebuilt from those decisions.

## Publish And Version Discipline

`ctox-rxdb-js/source` is a private package. It exists to build the Business OS
browser bundle and its provenance, not to publish a public npm package that
claims upstream RxDB compatibility.

- `package.json` must keep `private: true`.
- The npm publish policy is `private-package-only`.
- The package version mirrors the pinned upstream baseline (`16.20.0`).
- CTOX release identity is the generated bundle provenance, bundle SHA-256,
  fork lockfile SHA-256, and Git tag.
- Upstream attribution stays in `ctoxHardFork.upstream`, not in the package
  repository or homepage metadata.

## Required Fork Contracts

| Contract | JavaScript Fork | Rust Side |
|---|---|---|
| Protocol version | Announces `ctox-rxdb-protocol-v1` before replication starts | Rejects unknown/missing protocol versions |
| Schema parity | Sends collection schema hashes and migration versions | Compares the same hashes before accepting writes |
| Peer lifecycle | Tracks peer generation and invalidates stale SimplePeer state | Publishes native peer generation/session id |
| Typed errors | Emits stable `code`, `phase`, `collection`, `peerGeneration` | Maps native errors into the same error code family |
| File chunks | Preserves file generation, chunk count, chunk hash, and materialization state | Persists and validates the same fields in SQLite |
| Readiness | Exposes initial-sync completion per required collection | Persists checkpoints and collection readiness |

## First Implementation Slices

1. Create a reproducible fork source checkout/build pipeline for
   `ctox-rxdb-js`.
2. Move the Business OS bundle build to the fork pipeline.
3. Add protocol/capability handshake support.
4. Add schema hash exchange and rejection on mismatch.
5. Add generation-aware reconnect and stale peer invalidation.
6. Add typed replication/signaling errors used by Advanced Status.
7. Add shared JS/Rust conformance fixtures for file chunks and checkpoints.

## Guardrails

- Keep the upstream provenance pin visible.
- Do not hand-edit `rxdb-bundle.mjs` as the source of truth.
- Any protocol-affecting JS change must name the matching `rxdb-rs` behavior.
- Any file-sync change must have direct Browser/Rust smoke or conformance
  evidence.
- Any reconnect change must be reflected in Advanced Status diagnostics.
