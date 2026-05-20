# RxDB → Rust Port — Lookup Table

Upstream: `pubkey/rxdb` tag **16.20.0** @ commit `c69c94bb107a4d36dbf989de0e5f17081dcf7718`

Authoritative tracking table for the RxDB → Rust port. Every upstream TypeScript
source file is listed exactly once. Subagents update the `Status` column atomically.

**Scope:** CTOX-as-WebRTC-peer MVP. Browser side (business-os) keeps the upstream JS bundle unchanged.

**Status legend:** `pending` · `claimed` · `wip` · `done` · `skipped`

**Phase legend:**
- **phase-0** — Foundation (sequential, single author)
- **phase-1** — Schema + query layer required by storage (parallel)
- **phase-2** — Storage abstraction + Memory + own SQLite (parallel)
- **phase-3** — Replication core (sequential)
- **phase-4** — WebRTC transport (parallel)
- **phase-5** — Reactive layer (sequential)
- **phase-6** — Assembly: RxDatabase/RxCollection/RxDocument/RxQuery (sequential)
- **phase-7** — Conformance tests (parallel)
- **skip** — out-of-scope (browser-only / SaaS-specific / deferred / MVP trim)

**Tier legend** (drives delegation strategy):
- **T1** — Rust-native re-design. Main agent only, sequential.
- **T2** — Substantive port. Parallel subagents with PORT_STYLE.md.
- **T3** — Syntax-only port. Highly parallel, mechanical.

## Totals — Phases

| Phase | Files |
|---|---:|
| phase-0 | 23 |
| phase-1 | 5 |
| phase-2 | 9 |
| phase-3 | 11 |
| phase-4 | 11 |
| phase-5 | 2 |
| phase-6 | 11 |
| phase-7 | 9 |
| skip | 123 |
| **Total upstream src files** | **204** |
| **To port (phase-0..7)** | **81** |

## Totals — Tiers (of the 81 to-port files)

| Tier | Files | Delegation |
|---|---:|---|
| T1 — Rust-native re-design | 34 | main agent only, sequential |
| T2 — Substantive port | 23 | parallel subagents with PORT_STYLE.md |
| T3 — Syntax-only port | 24 | parallel mechanical subagents |

## Phase × Tier crosstab

| Phase | T1 | T2 | T3 |
|---|---:|---:|---:|
| phase-0 | 4 | 1 | 18 |
| phase-1 | 0 | 0 | 5 |
| phase-2 | 8 | 1 | 0 |
| phase-3 | 6 | 5 | 0 |
| phase-4 | 7 | 4 | 0 |
| phase-5 | 2 | 0 | 0 |
| phase-6 | 7 | 3 | 1 |
| phase-7 | 0 | 9 | 0 |

## New code — no upstream source (additions beyond the port)

These are not in upstream RxDB. They are CTOX-native Rust code that the port depends on
or that fills gaps the upstream stack does not cover for our deployment.

| # | Item | Where | Tier | Status | Owner | Notes |
|---:|---|---|:---:|---|---|---|
| N1 | `RxStorageSqlite` — `RxStorageInstance` impl against `runtime/ctox.sqlite3` | `src/core/rxdb/src/storage/sqlite/*.rs` | T1 | pending | — | Premium SQLite plugin not in OSS; own schema layout |
| N2 | `attachments_stub` — no-op `fillWriteDataForAttachmentsChange` | `src/core/rxdb/src/plugins/attachments/stub.rs` | T3 | pending | — | replication-protocol imports it; parquet handled out-of-band |
| N3 | `rxjs_compat` — `Subject`/`Observable` → tokio broadcast/watch + tokio-stream | `src/core/rxdb/src/rxjs_compat.rs` | T1 | pending | — | foundation for all T1 reactive modules |
| N4 | `overwritable` Rust idiom — typed registry instead of TS mutable singleton | `src/core/rxdb/src/overwritable.rs` (port + design) | T1 | pending | — | upstream uses prototype mutation |
| N5 | WebRTC connection handler on `webrtc-rs` (replaces `simple-peer`) | `src/core/rxdb/src/plugins/replication_webrtc/connection_handler_rs.rs` | T1 | pending | — | API mismatch; not a port |
| N6 | Signaling client matching browser URL contract | `src/core/rxdb/src/plugins/replication_webrtc/signaling_client.rs` | T1 | pending | — | must match what business-os's signaling expects |
| N7 | Tombstone GC job — `DELETE WHERE _deleted=1 AND updated_at_ms < ?` | `src/core/rxdb/src/storage/sqlite/cleanup.rs` | T3 | pending | — | replaces cleanup plugin |
| N8 | NPM mini-deps in Rust: `custom-idle-queue`, `oblivious-set` | `src/core/rxdb/src/util/*.rs` | T2 | pending | — | used by `rx-database` |
| N9 | Conformance test harness (Node + JS bundle vs Rust port) | `src/core/rxdb/tests/conformance/harness/` | T2 | pending | — | wire-format identity verification |
| N10 | RXDB_VERSION constant (replaces upstream build template) | `src/core/rxdb/src/util/version.rs` | T3 | pending | — | const = '16.20.0' |
| **N-OOC** | Parquet blob-sync side-channel (content-addressed, second WebRTC DataChannel) | **outside rxdb-rs** — CTOX glue code | — | — | — | not part of this crate; documents only carry hash refs |

## phase-0

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/plugins/utils/utils-regex.ts` | 75 | T3 | `src/plugins/utils/utils_regex.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-rxdb-version.ts` | 111 | T3 | `src/plugins/utils/utils_rxdb_version.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-global.ts` | 152 | T3 | `src/plugins/utils/utils_global.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-number.ts` | 258 | T3 | `src/plugins/utils/utils_number.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-map.ts` | 672 | T3 | `src/plugins/utils/utils_map.rs` | pending | — | foundation utilities |
| `src/plugins/utils/index.ts` | 719 | T3 | `src/plugins/utils/index.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-time.ts` | 1214 | T3 | `src/plugins/utils/utils_time.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-revision.ts` | 1482 | T3 | `src/plugins/utils/utils_revision.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-object-deep-equal.ts` | 1505 | T3 | `src/plugins/utils/utils_object_deep_equal.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-error.ts` | 1665 | T3 | `src/plugins/utils/utils_error.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-hash.ts` | 1713 | T3 | `src/plugins/utils/utils_hash.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-other.ts` | 1720 | T3 | `src/plugins/utils/utils_other.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-string.ts` | 2127 | T3 | `src/plugins/utils/utils_string.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-document.ts` | 3387 | T3 | `src/plugins/utils/utils_document.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-promise.ts` | 3597 | T3 | `src/plugins/utils/utils_promise.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-array.ts` | 5070 | T3 | `src/plugins/utils/utils_array.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-object.ts` | 7323 | T3 | `src/plugins/utils/utils_object.rs` | pending | — | foundation utilities |
| `src/plugins/utils/utils-object-dot-prop.ts` | 9509 | T3 | `src/plugins/utils/utils_object_dot_prop.rs` | pending | — | foundation utilities |
| `src/plugin-helpers.ts` | 12609 | T2 | `src/plugin_helpers.rs` | pending | — | plugin helpers |
| `src/overwritable.ts` | 1483 | T1 | `src/overwritable.rs` | pending | — | overridable defaults |
| `src/plugin.ts` | 2798 | T1 | `src/plugin.rs` | pending | — | plugin system |
| `src/hooks.ts` | 3377 | T1 | `src/hooks.rs` | pending | — | hooks |
| `src/rx-error.ts` | 4553 | T1 | `src/rx_error.rs` | pending | — | error type |

## phase-1

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/rx-query-mingo.ts` | 1607 | T3 | `src/rx_query_mingo.rs` | pending | — | required by rx-query-helper (load-bearing for storage) |
| `src/rx-schema.ts` | 7123 | T3 | `src/rx_schema.rs` | pending | — | schema definition |
| `src/rx-schema-helper.ts` | 10648 | T3 | `src/rx_schema_helper.rs` | pending | — | schema helpers |
| `src/query-planner.ts` | 11984 | T3 | `src/query_planner.rs` | pending | — | required by rx-query-helper (load-bearing for storage) |
| `src/custom-index.ts` | 12134 | T3 | `src/custom_index.rs` | pending | — | custom index — required by query-planner (load-bearing for storage) |

## phase-2

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/incremental-write.ts` | 8439 | T2 | `src/incremental_write.rs` | pending | — | incremental write helper |
| `src/plugins/storage-memory/memory-indexes.ts` | 1151 | T1 | `src/plugins/storage_memory/memory_indexes.rs` | pending | — | reference Memory storage for conformance |
| `src/plugins/storage-memory/index.ts` | 1653 | T1 | `src/plugins/storage_memory/index.rs` | pending | — | reference Memory storage for conformance |
| `src/plugins/storage-memory/binary-search-bounds.ts` | 2947 | T1 | `src/plugins/storage_memory/binary_search_bounds.rs` | pending | — | reference Memory storage for conformance |
| `src/plugins/storage-memory/memory-types.ts` | 3045 | T1 | `src/plugins/storage_memory/memory_types.rs` | pending | — | reference Memory storage for conformance |
| `src/plugins/storage-memory/memory-helper.ts` | 4855 | T1 | `src/plugins/storage_memory/memory_helper.rs` | pending | — | reference Memory storage for conformance |
| `src/doc-cache.ts` | 10730 | T1 | `src/doc_cache.rs` | pending | — | document cache — used internally by rx-collection |
| `src/plugins/storage-memory/rx-storage-instance-memory.ts` | 16609 | T1 | `src/plugins/storage_memory/rx_storage_instance_memory.rs` | pending | — | reference Memory storage for conformance |
| `src/rx-storage-helper.ts` | 37030 | T1 | `src/rx_storage_helper.rs` | pending | — | storage abstraction helpers |

## phase-3

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/replication-protocol/default-conflict-handler.ts` | 1231 | T2 | `src/replication_protocol/default_conflict_handler.rs` | pending | — | replication state machine |
| `src/replication-protocol/conflicts.ts` | 2179 | T2 | `src/replication_protocol/conflicts.rs` | pending | — | replication state machine |
| `src/replication-protocol/helper.ts` | 2699 | T2 | `src/replication_protocol/helper.rs` | pending | — | replication state machine |
| `src/plugins/leader-election/index.ts` | 2937 | T2 | `src/plugins/leader_election/index.rs` | pending | — | used by replication |
| `src/replication-protocol/checkpoint.ts` | 5506 | T2 | `src/replication_protocol/checkpoint.rs` | pending | — | replication state machine |
| `src/plugins/replication/replication-helper.ts` | 3606 | T1 | `src/plugins/replication/replication_helper.rs` | pending | — | base RxReplicationState |
| `src/replication-protocol/meta-instance.ts` | 5669 | T1 | `src/replication_protocol/meta_instance.rs` | pending | — | replication state machine |
| `src/replication-protocol/index.ts` | 12372 | T1 | `src/replication_protocol/index.rs` | pending | — | replication state machine |
| `src/replication-protocol/downstream.ts` | 21692 | T1 | `src/replication_protocol/downstream.rs` | pending | — | replication state machine |
| `src/replication-protocol/upstream.ts` | 21852 | T1 | `src/replication_protocol/upstream.rs` | pending | — | replication state machine |
| `src/plugins/replication/index.ts` | 25090 | T1 | `src/plugins/replication/index.rs` | pending | — | base RxReplicationState |

## phase-4

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/plugins/replication-websocket/index.ts` | 116 | T2 | `src/plugins/replication_websocket/index.rs` | pending | — | only if needed for signaling |
| `src/plugins/replication-websocket/websocket-types.ts` | 1029 | T2 | `src/plugins/replication_websocket/websocket_types.rs` | pending | — | only if needed for signaling |
| `src/plugins/replication-websocket/websocket-server.ts` | 4510 | T2 | `src/plugins/replication_websocket/websocket_server.rs` | pending | — | only if needed for signaling |
| `src/plugins/replication-websocket/websocket-client.ts` | 6524 | T2 | `src/plugins/replication_websocket/websocket_client.rs` | pending | — | only if needed for signaling |
| `src/plugins/replication-webrtc/webrtc-helper.ts` | 1498 | T1 | `src/plugins/replication_webrtc/webrtc_helper.rs` | pending | — | WebRTC transport — primary |
| `src/plugins/replication-webrtc/connection-handler-p2pcf.ts` | 2536 | T1 | `src/plugins/replication_webrtc/connection_handler_p2pcf.rs` | pending | — | WebRTC transport — primary |
| `src/plugins/replication-webrtc/webrtc-types.ts` | 2977 | T1 | `src/plugins/replication_webrtc/webrtc_types.rs` | pending | — | WebRTC transport — primary |
| `src/plugins/replication-webrtc/connection-handler-webtorrent.ts` | 5229 | T1 | `src/plugins/replication_webrtc/connection_handler_webtorrent.rs` | pending | — | WebRTC transport — primary |
| `src/plugins/replication-webrtc/signaling-server.ts` | 5704 | T1 | `src/plugins/replication_webrtc/signaling_server.rs` | pending | — | WebRTC transport — primary |
| `src/plugins/replication-webrtc/connection-handler-simple-peer.ts` | 10999 | T1 | `src/plugins/replication_webrtc/connection_handler_simple_peer.rs` | pending | — | WebRTC transport — primary |
| `src/plugins/replication-webrtc/index.ts` | 11079 | T1 | `src/plugins/replication_webrtc/index.rs` | pending | — | WebRTC transport — primary |

## phase-5

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/rx-change-event.ts` | 4539 | T1 | `src/rx_change_event.rs` | pending | — | reactive change events |
| `src/change-event-buffer.ts` | 5084 | T1 | `src/change_event_buffer.rs` | pending | — | change event buffer |

## phase-6

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/rx-query-single-result.ts` | 3580 | T3 | `src/rx_query_single_result.rs` | pending | — | single-result query |
| `src/index.ts` | 1039 | T2 | `src/index.rs` | pending | — | crate top-level export |
| `src/rx-collection-helper.ts` | 6390 | T2 | `src/rx_collection_helper.rs` | pending | — | collection helpers |
| `src/rx-query-helper.ts` | 10036 | T2 | `src/rx_query_helper.rs` | pending | — | query helpers — required by rx-storage-helper |
| `src/rx-document-prototype-merge.ts` | 3779 | T1 | `src/rx_document_prototype_merge.rs` | pending | — | document prototype merge |
| `src/query-cache.ts` | 4369 | T1 | `src/query_cache.rs` | pending | — | query cache — used internally by rx-collection |
| `src/rx-database-internal-store.ts` | 12046 | T1 | `src/rx_database_internal_store.rs` | pending | — | internal meta store |
| `src/rx-document.ts` | 17383 | T1 | `src/rx_document.rs` | pending | — | RxDocument — required by rx-collection internals |
| `src/rx-query.ts` | 25120 | T1 | `src/rx_query.rs` | pending | — | RxQuery — required by rx-collection internals |
| `src/rx-database.ts` | 27854 | T1 | `src/rx_database.rs` | pending | — | top-level RxDatabase |
| `src/rx-collection.ts` | 38000 | T1 | `src/rx_collection.rs` | pending | — | top-level RxCollection |

## phase-7

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/plugins/test-utils/revisions.ts` | 288 | T2 | `tests/conformance/revisions.rs` | pending | — | conformance test infra |
| `src/plugins/test-utils/port-manager.ts` | 749 | T2 | `tests/conformance/port-manager.rs` | pending | — | conformance test infra |
| `src/plugins/test-utils/index.ts` | 777 | T2 | `tests/conformance/index.rs` | pending | — | conformance test infra |
| `src/plugins/test-utils/test-util.ts` | 1676 | T2 | `tests/conformance/test-util.rs` | pending | — | conformance test infra |
| `src/plugins/test-utils/replication.ts` | 3195 | T2 | `tests/conformance/replication.rs` | pending | — | conformance test infra |
| `src/plugins/test-utils/config.ts` | 3988 | T2 | `tests/conformance/config.rs` | pending | — | conformance test infra |
| `src/plugins/test-utils/schema-objects.ts` | 14880 | T2 | `tests/conformance/schema-objects.rs` | pending | — | conformance test infra |
| `src/plugins/test-utils/humans-collection.ts` | 16186 | T2 | `tests/conformance/humans-collection.rs` | pending | — | conformance test infra |
| `src/plugins/test-utils/schemas.ts` | 34776 | T2 | `tests/conformance/schemas.rs` | pending | — | conformance test infra |

## skip

| Upstream | Bytes | Reason |
|---|---:|---|
| `src/plugins/vector/helper.ts` | 0 | empty/stub upstream |
| `src/plugins/vector/types.ts` | 31 | empty/stub upstream |
| `src/plugins/vector/index.ts` | 71 | empty/stub upstream |
| `src/plugins/utils/utils-rxdb-version.template.ts` | 117 | MVP trim — build-time template; replaced by a Rust const |
| `src/plugins/electron/index.ts` | 126 | electron-specific |
| `src/plugins/electron/electron-helper.ts` | 129 | electron-specific |
| `src/plugins/storage-dexie/index.ts` | 156 | browser-only (Dexie stays JS-side) |
| `src/plugins/storage-mongodb/index.ts` | 164 | out-of-scope backend |
| `src/plugins/storage-remote/index.ts` | 201 | out-of-scope multi-process scheme |
| `src/plugins/storage-foundationdb/foundationdb-helpers.ts` | 217 | out-of-scope backend |
| `src/plugins/cleanup/cleanup-helper.ts` | 346 | MVP trim — replaced by SQL-level tombstone GC (see "New code") |
| `src/plugins/replication-nats/nats-helper.ts` | 355 | SaaS-specific replication |
| `src/plugins/pipeline/index.ts` | 418 | defer |
| `src/plugins/storage-denokv/denokv-types.ts` | 503 | out-of-scope backend |
| `src/plugins/update/mingo-updater.ts` | 611 | MVP trim — mango update DSL; CTOX writes full documents |
| `src/plugins/storage-denokv/denokv-helper.ts` | 645 | out-of-scope backend |
| `src/plugins/pipeline/types.ts` | 651 | defer |
| `src/plugins/replication-supabase/types.ts` | 710 | SaaS-specific replication |
| `src/plugins/storage-remote/storage-remote-helpers.ts` | 770 | out-of-scope multi-process scheme |
| `src/plugins/replication-appwrite/appwrite-types.ts` | 795 | SaaS-specific replication |
| `src/plugins/state/types.ts` | 798 | defer |
| `src/plugins/validate-is-my-json-valid/index.ts` | 833 | pick one validator — only ajv was a candidate; ajv now also deferred |
| `src/plugins/storage-mongodb/mongodb-types.ts` | 918 | out-of-scope backend |
| `src/plugins/replication-couchdb/couchdb-types.ts` | 929 | SaaS-specific replication |
| `src/plugins/storage-localstorage/localstorage-mock.ts` | 934 | browser-only |
| `src/plugins/state/index.ts` | 1011 | defer |
| `src/plugins/query-builder/mquery/mquery-utils.ts` | 1013 | MVP trim — chained query DSL; no user-facing queries on CTOX side |
| `src/plugins/migration-schema/migration-types.ts` | 1042 | MVP trim — defer until schema version drift exists |
| `src/plugins/replication-nats/nats-types.ts` | 1068 | SaaS-specific replication |
| `src/plugins/replication-appwrite/appwrite-helpers.ts` | 1108 | SaaS-specific replication |
| `src/plugins/storage-remote-websocket/types.ts` | 1142 | out-of-scope multi-process scheme |
| `src/plugins/vector/vector-distance.ts` | 1178 | empty/stub upstream |
| `src/plugins/utils/utils-premium.ts` | 1238 | MVP trim — license checks; constant-false replacement is part of foundation glue |
| `src/plugins/storage-denokv/index.ts` | 1269 | out-of-scope backend |
| `src/plugins/utils/utils-base64.ts` | 1277 | MVP trim — only used via utils-blob for attachments |
| `src/plugins/storage-mongodb/rx-storage-mongodb.ts` | 1310 | out-of-scope backend |
| `src/plugins/cleanup/index.ts` | 1442 | MVP trim — replaced by SQL-level tombstone GC (see "New code") |
| `src/plugins/replication-mongodb/mongodb-types.ts` | 1461 | SaaS-specific replication |
| `src/plugins/replication-graphql/helper.ts` | 1474 | GraphQL replication |
| `src/plugins/validate-ajv/index.ts` | 1490 | MVP trim — browser-side validates pre-push |
| `src/plugins/dev-mode/check-migration-strategies.ts` | 1519 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/update/index.ts` | 1519 | MVP trim — mango update DSL; CTOX writes full documents |
| `src/plugins/validate-z-schema/index.ts` | 1539 | pick one validator — only ajv was a candidate; ajv now also deferred |
| `src/plugins/storage-localstorage/index.ts` | 1544 | browser-only |
| `src/plugins/electron/rx-storage-ipc-main.ts` | 1570 | electron-specific |
| `src/plugins/replication-graphql/graphql-websocket.ts` | 1602 | GraphQL replication |
| `src/plugins/replication-supabase/helper.ts` | 1688 | SaaS-specific replication |
| `src/plugins/dev-mode/check-orm.ts` | 1695 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/electron/rx-storage-ipc-renderer.ts` | 1816 | electron-specific |
| `src/plugins/utils/utils-blob.ts` | 1826 | MVP trim — attachment helper, only used by RxDB attachments plugin (parquet lives outside) |
| `src/plugins/storage-foundationdb/index.ts` | 1841 | out-of-scope backend |
| `src/plugins/replication-mongodb/mongodb-helper.ts` | 1921 | SaaS-specific replication |
| `src/plugins/storage-dexie/rx-storage-dexie.ts` | 2111 | browser-only (Dexie stays JS-side) |
| `src/plugins/dev-mode/entity-properties.ts` | 2115 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/migration-schema/index.ts` | 2183 | MVP trim — defer until schema version drift exists |
| `src/plugins/state/helpers.ts` | 2263 | defer |
| `src/plugins/storage-foundationdb/foundationdb-types.ts` | 2268 | out-of-scope backend |
| `src/plugins/replication-firestore/firestore-helper.ts` | 2308 | SaaS-specific replication |
| `src/plugins/replication-firestore/firestore-types.ts` | 2329 | SaaS-specific replication |
| `src/plugins/flutter/index.ts` | 2347 | flutter-specific |
| `src/plugins/query-builder/index.ts` | 2468 | MVP trim — chained query DSL; no user-facing queries on CTOX side |
| `src/plugins/replication-couchdb/couchdb-helper.ts` | 2511 | SaaS-specific replication |
| `src/plugins/storage-sqlite/index.ts` | 2544 | RxDB Premium trial stub — we build our own SQLite backend |
| `src/plugins/attachments/attachments-utils.ts` | 2702 | parquet stored as external content-addressed file, not RxDB attachment; replication-protocol uses a no-op stub instead |
| `src/plugins/storage-remote/message-channel-cache.ts` | 2761 | out-of-scope multi-process scheme |
| `src/plugins/local-documents/index.ts` | 2794 | defer |
| `src/plugins/dev-mode/unallowed-properties.ts` | 2833 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/cleanup/cleanup-state.ts` | 3092 | MVP trim — replaced by SQL-level tombstone GC (see "New code") |
| `src/plugins/backup/file-util.ts` | 3133 | defer |
| `src/plugins/local-documents/local-documents-helper.ts` | 3286 | defer |
| `src/plugins/pipeline/flagged-functions.ts` | 3394 | defer |
| `src/plugins/dev-mode/dev-mode-tracking.ts` | 3513 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/storage-mongodb/mongodb-helper.ts` | 3542 | out-of-scope backend |
| `src/plugins/storage-remote/storage-remote-types.ts` | 3557 | out-of-scope multi-process scheme |
| `src/plugins/json-dump/index.ts` | 3701 | MVP trim — export/import not on sync path |
| `src/plugins/storage-remote-websocket/index.ts` | 3902 | out-of-scope multi-process scheme |
| `src/plugins/dev-mode/check-document.ts` | 4146 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/local-documents/local-documents.ts` | 4224 | defer |
| `src/plugins/attachments-compression/index.ts` | 4228 | parquet self-compressed; rxdb attachments not used |
| `src/plugins/storage-sqlite/sqlite-types.ts` | 4282 | RxDB Premium trial stub — we build our own SQLite backend |
| `src/plugins/storage-denokv/denokv-query.ts` | 4523 | out-of-scope backend |
| `src/plugins/cleanup/cleanup.ts` | 4758 | MVP trim — replaced by SQL-level tombstone GC (see "New code") |
| `src/plugins/migration-schema/migration-helpers.ts` | 5033 | MVP trim — defer until schema version drift exists |
| `src/event-reduce.ts` | 5718 | MVP trim — optimizes RxQuery re-runs that CTOX does not trigger |
| `src/plugins/storage-foundationdb/foundationdb-query.ts` | 5780 | out-of-scope backend |
| `src/rx-storage-multiinstance.ts` | 6244 | MVP trim — browser multi-tab broadcast; CTOX is single-process |
| `src/plugins/replication-mongodb/mongodb-checkpoint.ts` | 6360 | SaaS-specific replication |
| `src/plugins/dev-mode/index.ts` | 6466 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/replication-graphql/query-builder-from-rx-schema.ts` | 6745 | GraphQL replication |
| `src/plugins/dev-mode/check-query.ts` | 6778 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/storage-dexie/dexie-query.ts` | 7391 | browser-only (Dexie stays JS-side) |
| `src/plugins/storage-sqlite/sqlite-helpers.ts` | 7687 | RxDB Premium trial stub — we build our own SQLite backend |
| `src/plugins/encryption-crypto-js/index.ts` | 7747 | defer |
| `src/plugins/migration-storage/index.ts` | 8075 | MVP trim — defer |
| `src/plugins/key-compression/index.ts` | 8105 | defer |
| `src/plugins/replication-graphql/index.ts` | 8196 | GraphQL replication |
| `src/plugins/attachments/index.ts` | 8451 | parquet stored as external content-addressed file, not RxDB attachment; replication-protocol uses a no-op stub instead |
| `src/plugins/storage-remote/rx-storage-remote.ts` | 9452 | out-of-scope multi-process scheme |
| `src/plugins/backup/index.ts` | 9496 | defer |
| `src/plugins/storage-dexie/dexie-helper.ts` | 9564 | browser-only (Dexie stays JS-side) |
| `src/plugins/pipeline/rx-pipeline.ts` | 9997 | defer |
| `src/plugins/storage-remote/remote.ts` | 10687 | out-of-scope multi-process scheme |
| `src/plugins/replication-supabase/index.ts` | 10735 | SaaS-specific replication |
| `src/plugins/replication-appwrite/index.ts` | 11090 | SaaS-specific replication |
| `src/plugins/replication-nats/index.ts` | 11186 | SaaS-specific replication |
| `src/plugins/replication-graphql/graphql-schema-from-rx-schema.ts` | 11281 | GraphQL replication |
| `src/plugins/replication-mongodb/index.ts` | 12060 | SaaS-specific replication |
| `src/plugins/state/rx-state.ts` | 12136 | defer |
| `src/plugins/local-documents/rx-local-document.ts` | 12773 | defer |
| `src/plugins/storage-dexie/rx-storage-instance-dexie.ts` | 12953 | browser-only (Dexie stays JS-side) |
| `src/plugins/replication-couchdb/index.ts` | 13622 | SaaS-specific replication |
| `src/plugins/storage-sqlite/sqlite-storage-instance.ts` | 15647 | RxDB Premium trial stub — we build our own SQLite backend |
| `src/plugins/replication-firestore/index.ts` | 15661 | SaaS-specific replication |
| `src/plugins/storage-denokv/rx-storage-instance-denokv.ts` | 15821 | out-of-scope backend |
| `src/plugins/storage-foundationdb/rx-storage-instance-foundationdb.ts` | 16429 | out-of-scope backend |
| `src/plugins/query-builder/mquery/nosql-query-builder.ts` | 16890 | MVP trim — chained query DSL; no user-facing queries on CTOX side |
| `src/plugins/crdt/index.ts` | 17401 | advanced — defer |
| `src/plugins/storage-mongodb/rx-storage-instance-mongodb.ts` | 17551 | out-of-scope backend |
| `src/plugins/dev-mode/check-schema.ts` | 18662 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/dev-mode/error-messages.ts` | 20088 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/storage-localstorage/rx-storage-instance-localstorage.ts` | 21015 | browser-only |
| `src/plugins/migration-schema/rx-migration-state.ts` | 21720 | MVP trim — defer until schema version drift exists |
| `src/plugins/storage-sqlite/sqlite-basics-helpers.ts` | 21819 | RxDB Premium trial stub — we build our own SQLite backend |

---

## Update protocol for subagents

1. Subagents may only claim rows with `Tier = T2` or `Tier = T3`. T1 rows are reserved for the main agent.
2. Before claiming a row, verify `Status` is `pending`.
3. Atomic claim: change row's `Status` → `claimed` and `Owner` → `<agent-id>` in a single `Edit` operation, then commit.
4. On completion: `Status` → `done`. Add the Git SHA of the porting commit in `Notes` if helpful.
5. If a dependency is missing, set `Status` → `pending` again, leave a note, and stop. Do not invent missing modules.
6. Never edit `Upstream`, `Bytes`, `Tier`, or `Rust target` columns — those are derived from the pinned upstream commit and the agreed taxonomy.

## Revision protocol (main agent only)

- This `PORTING.md` is the **single live master**. Subagents edit it in place during a wave.
- At every wave boundary or scope revision, the main agent takes an **immutable snapshot** into `revisions/PORTING.wave-NNN-<slug>.md`. Subagents must never touch `revisions/`.
- A "wave" = one parallel subagent batch (or one sequential foundation step) that ends with `cargo build` green. After commit, snapshot before kicking off the next wave.
- Snapshot naming: zero-padded sequential number + short slug. Examples: `wave-000-baseline.md`, `wave-001-scope-trimmed.md`, `wave-002-phase0-foundation-t1.md`.
- The Git history of `PORTING.md` is the secondary audit trail; the files under `revisions/` are the primary human-readable history.
