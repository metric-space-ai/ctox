# rxdb-rs conformance scope

This directory is the Rust conformance harness for the CTOX RxDB port.

The upstream source for this layer is `vendor/rxdb-16.20.0/src/plugins/test-utils/`.
Each upstream test-utils barrel export has a Rust counterpart:

- `config.ts` -> `config.rs`
- `humans-collection.ts` -> `humans-collection.rs`
- `port-manager.ts` -> `port-manager.rs`
- `revisions.ts` -> `revisions.rs`
- `test-util.ts` -> `test-util.rs`
- `schema-objects.ts` -> `schema-objects.rs`
- `schemas.ts` -> `schemas.rs`
- `replication.ts` -> `replication.rs`

The Rust harness intentionally compares against the vendored source shape and
then runs equivalent behavior against the Rust storage and replication APIs.
Browser/WebRTC end-to-end checks belong to the CTOX integration layer where the
browser bundle, signaling server, and Rust daemon can run together.
