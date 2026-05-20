# RxDB → Rust Port — Lookup Table

Upstream: `pubkey/rxdb` tag **16.20.0** @ commit `c69c94bb107a4d36dbf989de0e5f17081dcf7718`

Vendored at `src/core/rxdb/vendor/rxdb-16.20.0/`. See `vendor/rxdb.version` for the pin.

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
- **skip** — out-of-scope (browser-only / SaaS-specific / deferred / MVP trim / V1 trim)

**Tier legend** (drives delegation strategy):
- **T1** — Rust-native re-design. Main agent only, sequential.
- **T2** — Substantive port. Parallel subagents with PORT_STYLE.md.
- **T3** — Syntax-only port. Highly parallel, mechanical.

## Totals — Phases

| Phase | Files |
|---|---:|
| phase-0 | 22 |
| phase-1 | 5 |
| phase-2 | 11 |
| phase-3 | 11 |
| phase-4 | 5 |
| phase-5 | 3 |
| phase-6 | 11 |
| phase-7 | 9 |
| skip | 127 |
| **Total upstream src files** | **204** |
| **To port (phase-0..7)** | **77** |

## Totals — Tiers (of the 77 to-port files)

| Tier | Files | Delegation |
|---|---:|---|
| T1 — Rust-native re-design | 32 | main agent only, sequential |
| T2 — Substantive port | 21 | parallel subagents with PORT_STYLE.md |
| T3 — Syntax-only port | 24 | parallel mechanical subagents |

## Phase × Tier crosstab

| Phase | T1 | T2 | T3 |
|---|---:|---:|---:|
| phase-0 | 4 | 0 | 18 |
| phase-1 | 0 | 0 | 5 |
| phase-2 | 8 | 2 | 0 |
| phase-3 | 6 | 5 | 0 |
| phase-4 | 5 | 0 | 0 |
| phase-5 | 2 | 1 | 0 |
| phase-6 | 7 | 3 | 1 |
| phase-7 | 0 | 9 | 0 |

## New code — no upstream source (additions beyond the port)

| # | Item | Where | Tier | Status | Owner | Notes |
|---:|---|---|:---:|---|---|---|
| N1 | `RxStorageSqlite` — `RxStorageInstance` impl against `runtime/ctox.sqlite3` | `src/storage/sqlite/*.rs` | T1 | pending | — | Premium SQLite plugin not in OSS; own schema layout |
| N2 | `attachments_stub` — no-op `fillWriteDataForAttachmentsChange` | `src/plugins/attachments/stub.rs` | T3 | done | main | wave-036: returns new_document unchanged; CTOX stores attachments as out-of-band Parquet files referenced by hash. Errors with AT_FILL when `_attachments` missing |
| N3 | `rxjs_compat` — `Subject`/`Observable` → tokio broadcast/watch + tokio-stream | `src/rxjs_compat.rs` | T1 | done | main | RxSubject/RxBehaviorSubject/RxStream/first_value_from + 3 unit tests green |
| N4 | `overwritable` Rust idiom — typed registry instead of TS mutable singleton | `src/overwritable.rs` (port + design) | T1 | pending | — | upstream uses prototype mutation |
| N5 | WebRTC connection handler on `webrtc-rs` (replaces `simple-peer` port) | `src/plugins/replication_webrtc/connection_handler_rs.rs` | T1 | wip | main | skeleton: WebRTCRsConnectionHandler impl of WebRTCConnectionHandler trait with RxSubject<...>-backed connect/disconnect/message/response/error streams + peer registry; send/close stubbed pending webrtc-rs crate integration |
| N6 | Signaling client matching browser URL contract | `src/plugins/replication_webrtc/signaling_client.rs` | done | main | SignalingClient: tokio-tungstenite WebSocket, ServerToClient/ClientToServer typed wire protocol (`Init`/`Joined`/`Signal`/`Ping`/`Join`), keepalive task (half PING_INTERVAL), RxBehaviorSubject<peer_list> + RxSubject<server_messages>. signaling_protocol.rs defines PEER_ID_LENGTH=12, SIMPLE_PEER_PING_INTERVAL_MS, RoomId 6..100 chars |
| N7 | Tombstone GC job — `DELETE WHERE _deleted=1 AND updated_at_ms < ?` | `src/storage/sqlite/cleanup.rs` | T3 | pending | — | replaces cleanup plugin |
| N8 | NPM mini-deps in Rust: `custom-idle-queue`, `oblivious-set`, `array-push-at-sort-position` | `src/util/*.rs` | T2 | done | main | wave-036: ObliviousSet (TTL-decay hashset for emittedEventBulkIds dedupe), IdleQueue (serializing tokio queue + request_idle_promise), push_at_sort_position (binary_search_by + Vec::insert). 7 unit tests (4 push_at_sort_position + 3 oblivious_set + 2 idle_queue) |
| N9 | Conformance test harness (Node + JS bundle vs Rust port) | `tests/conformance/harness/` | T2 | pending | — | wire-format identity verification |
| N10 | `RXDB_VERSION` constant (replaces upstream build template) | `src/plugins/utils/utils_rxdb_version.rs` | T3 | done | main | const = '16.20.0' |
| N16 | Minimal Mango-query Rust port (replaces upstream `mingo` NPM package) | `src/util/mango/*.rs` | T1 | done | subagent-afed576b | delivered by background worktree-isolated subagent; 18 operators + Query + sort_documents + Cursor→Vec collapse; 10 unit tests green; vendored mingo 6.5.6 |
| N17 | Core type foundation (schema, document, checkpoint, hash) + phase-1 query types + **phase-2 storage types** (BulkWriteRow, RxStorageWriteError, RxStorageBulkWriteResponse, RxStorageChangeEvent, EventBulk, RxStorageQueryResult, RxStorageCountResult, RxStorageChangedDocumentsSinceResult, RxStorageInstanceCreationParams, **RxStorageInstance trait**, **RxStorage factory trait** (wave-035), PreparedQuery) | `src/types/{mod,util,schema,document,checkpoint,hash,query,storage}.rs` | T1 | done | main | RxStorageInstance + RxStorage are async_trait + Send+Sync; RxStorage factory pattern wires meta-instance creation in replication start() |
| N11 | Type stub for skipped `attachments` plugin (`RxAttachment*` types referenced by replication) | `src/plugins/attachments/types_stub.rs` | T3 | done | main | wave-036: re-exports RxAttachmentData/RxAttachmentWriteData from types/storage.rs; RxAttachment alias to RxAttachmentData (interactive method surface intentionally omitted) |
| N12 | Type stub for skipped `migration-schema` plugin (`RxMigrationState` type) | `src/plugins/migration_schema/types_stub.rs` | T3 | done | main | wave-036: RxMigrationStatus + RxMigrationCount + MigrationStatusUpdate closure + MigrationStrategy/Strategies; PlainJsonError aliased to Value (V8 stack shape opaque) |
| N13 | Type stub for skipped `backup` plugin (`RxBackupState` type) | `src/plugins/backup/types_stub.rs` | T3 | done | main | wave-036: BackupOptions + BackupMetaFileContent + RxBackupCollectionState + RxBackupWriteEvent + RxBackupState placeholder (no in-process backup loop in CTOX MVP) |
| N14 | Type stub for skipped `pipeline` plugin (`RxPipeline*` types) | `src/plugins/pipeline/types_stub.rs` | T3 | done | main | wave-036: RxPipelineHandler closure + RxPipelineOptions + CheckpointDocData + RxPipeline placeholder (no pipeline runtime in CTOX MVP) |
| N15 | `event-reduce` stub returns `null` (always full re-execute) | `src/event_reduce.rs` | T2 | done | main | wave-036: calculate_new_results always returns run_full_query_again=true; behaviorally correct (cache falls back to re-querying storage) |
| **N-OOC** | Parquet blob-sync side-channel (content-addressed, second WebRTC DataChannel) | **outside rxdb-rs** — CTOX glue code | — | — | — | not part of this crate; documents only carry hash refs |

## External Rust crate mapping (V2)

Mapping from upstream NPM deps to chosen Rust crates. CTOX already pulls in most of these.

| NPM dep | Rust crate | Notes |
|---|---|---|
| `rxjs` (peerDep) | `tokio` + `tokio-stream` + `futures` + `async-trait` | wrapped by N3 (`rxjs_compat`) |
| `mingo` (mango engine) | own minimal port | full mingo is ~3-5k LOC; we port only what `prepareQuery`/`normalizeMangoQuery` actually use |
| `custom-idle-queue` | own mini-port (N8) | ~50 LOC |
| `oblivious-set` | own mini-port (N8) | ring-buffer set |
| `array-push-at-sort-position` | own mini-port (N8) | binary-search insert |
| `simple-peer` | replaced by `webrtc` (webrtc-rs) | N5 |
| SQLite | `rusqlite` | CTOX-standard; sync API wrapped in `spawn_blocking` |
| JSON | `serde` + `serde_json` | standard |
| hash | `sha2` | replaces `js-sha256` |
| errors | `thiserror` (typed) + `anyhow` (CTOX-glue side) | standard |
| sync primitives | `parking_lot` (sync), `tokio::sync` (async) | rule in PORT_STYLE.md |
| concurrent map | `dashmap` | for caches (doc-cache, query-cache) |
| atomic shared state | `arc-swap` | for `overwritable` Rust idiom (N4) |
| WebSocket signaling | `tokio-tungstenite` | if WebRTC signaling uses WS |
| Skipped upstream deps | — | `ajv`, `dexie`, `broadcast-channel`, `firebase`, `mongodb`, `nats`, `crypto-js`, `event-reduce-js`, `graphql*`, `is-my-json-valid`, `z-schema`, `jsonschema-key-compression`, `ws`, `isomorphic-ws`, `reconnecting-websocket`, `js-base64`, `p2pcf`, `webtorrent`, all `@types/*` |

## phase-0

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/plugins/utils/utils-regex.ts` | 75 | T3 | `src/plugins/utils/utils_regex.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-rxdb-version.ts` | 111 | T3 | `src/plugins/utils/utils_rxdb_version.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-global.ts` | 152 | T3 | `src/plugins/utils/utils_global.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-number.ts` | 258 | T3 | `src/plugins/utils/utils_number.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-map.ts` | 672 | T3 | `src/plugins/utils/utils_map.rs` | done | main | foundation utilities |
| `src/plugins/utils/index.ts` | 719 | T3 | `src/plugins/utils/mod.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-time.ts` | 1214 | T3 | `src/plugins/utils/utils_time.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-revision.ts` | 1482 | T3 | `src/plugins/utils/utils_revision.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-object-deep-equal.ts` | 1505 | T3 | `src/plugins/utils/utils_object_deep_equal.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-error.ts` | 1665 | T3 | `src/plugins/utils/utils_error.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-hash.ts` | 1713 | T3 | `src/plugins/utils/utils_hash.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-other.ts` | 1720 | T3 | `src/plugins/utils/utils_other.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-string.ts` | 2127 | T3 | `src/plugins/utils/utils_string.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-document.ts` | 3387 | T3 | `src/plugins/utils/utils_document.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-promise.ts` | 3597 | T3 | `src/plugins/utils/utils_promise.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-array.ts` | 5070 | T3 | `src/plugins/utils/utils_array.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-object.ts` | 7323 | T3 | `src/plugins/utils/utils_object.rs` | done | main | foundation utilities |
| `src/plugins/utils/utils-object-dot-prop.ts` | 9509 | T3 | `src/plugins/utils/utils_object_dot_prop.rs` | done | main | foundation utilities |
| `src/overwritable.ts` | 1483 | T1 | `src/overwritable.rs` | done | main | overridable defaults |
| `src/plugin.ts` | 2798 | T1 | `src/plugin.rs` | done | main | plugin system |
| `src/hooks.ts` | 3377 | T1 | `src/hooks.rs` | done | main | hooks |
| `src/rx-error.ts` | 4553 | T1 | `src/rx_error.rs` | done | main | error type |

## phase-1

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/rx-query-mingo.ts` | 1607 | T3 | `src/rx_query_mingo.rs` | done | main | re-export shim over `util::mango` (per-Query context replaces upstream global registry) |
| `src/rx-schema.ts` | 7123 | T3 | `src/rx_schema.rs` | done | main | T1 deviations: overwriteGetterForCaching → OnceLock, getDocumentPrototype omitted (no Rust prototypes) |
| `src/rx-schema-helper.ts` | 10648 | T3 | `src/rx_schema_helper.rs` | done | main | 10/11 functions; fillObjectWithDefaults defers to rx-schema.rs port |
| `src/query-planner.ts` | 11984 | T3 | `src/query_planner.rs` | done | main | get_query_plan + is_selector_satisfied_by_index + INDEX_MIN/MAX sentinels |
| `src/custom-index.ts` | 12134 | T3 | `src/custom_index.rs` | done | main | indexable-string monad + bounds string helpers; ParsedLengths struct |

## phase-2

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/rx-storage-multiinstance.ts` | 6244 | T2 | `src/rx_storage_multiinstance.rs` | done | main | single-process stub; BROADCAST_CHANNEL_BY_TOKEN registry kept for shape but no-op when multi_instance=false (always in CTOX) |
| `src/incremental-write.ts` | 8439 | T2 | `src/incremental_write.rs` | done | main | IncrementalWriteQueue with tokio::sync::{Mutex, oneshot}; Promise/resolve/reject → oneshot channel; modifier as Box<dyn FnOnce -> BoxFuture> |
| `src/plugins/storage-memory/memory-indexes.ts` | 1151 | T1 | `src/plugins/storage_memory/memory_indexes.rs` | done | main | add_indexes_to_internals_state + get_memory_index_name |
| `src/plugins/storage-memory/index.ts` | 1653 | T1 | `src/plugins/storage_memory/index_mod.rs` | done | main | renamed to index_mod.rs (Rust reserved-name avoidance); get_rx_storage_memory + create_storage_instance |
| `src/plugins/storage-memory/binary-search-bounds.ts` | 2947 | T1 | `src/plugins/storage_memory/binary_search_bounds.rs` | done | main | bound_ge/gt/lt/le/eq with Ordering-based Compare; 3 unit tests green |
| `src/plugins/storage-memory/memory-types.ts` | 3045 | T1 | `src/plugins/storage_memory/memory_types.rs` | done | main | MemoryStorageInternals + ByIndex + DocWithIndexString; SharedMemoryStorageInternals = Arc<Mutex<...>>; RxAttachmentData/WriteData added to types/storage.rs |
| `src/plugins/storage-memory/memory-helper.ts` | 4855 | T1 | `src/plugins/storage_memory/memory_helper.rs` | done | main | get_memory_collection_key, ensure_not_removed, attachment_map_key, put_write_row_to_state hotpath, remove_doc_from_state, compare_docs_with_index. `array-push-at-sort-position` inlined as `Vec::binary_search_by` + `Vec::insert` |
| `src/doc-cache.ts` | 10730 | T1 | `src/doc_cache.rs` | pending | — | document cache — used internally by rx-collection |
| `src/plugins/storage-memory/rx-storage-instance-memory.ts` | 16609 | T1 | `src/plugins/storage_memory/rx_storage_instance_memory.rs` | done | main | full RxStorageInstance trait impl: bulk_write + find_documents_by_id + query + count + cleanup + change_stream + remove + close + get_changed_documents_since (delegates to rx_storage_helper fallback). query uses index-bound BSearch + queryMatcher fallback + manual resort + skip/limit; deserializes prepared_query JSON. T1 deviations: synchronous persistence (no requestIdleCallback); OPEN_MEMORY_INSTANCES test-hook omitted |
| `src/rx-storage-helper.ts` | 37030 | T1 | `src/rx_storage_helper.rs` | wip | main | most helpers ported (get_single_document, write_single, stack_checkpoints, throw_if_is_storage_write_error, flat_clone_doc_with_meta, ensure_rx_storage_instance_params_are_correct, has_encryption, get_written_documents_from_bulk_write_response, attachment helpers, **categorize_bulk_write_rows**, **get_changed_documents_since_query + get_changed_documents_since_via_query**). Deferred: observeSingle (RxJS pipe ops phase-5), getWrappedStorageInstance (RxDatabase phase-6), randomDelayStorage (test helper) |
| `src/plugin-helpers.ts` | 12609 | T2 | `src/plugin_helpers.rs` | pending | — | reclassified V1 — depends on rx-schema-helper (phase-1), rx-storage-helper (phase-2), rxjs_compat (N3); ported at end of phase-2 |

## phase-3

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/replication-protocol/default-conflict-handler.ts` | 1231 | T2 | `src/replication_protocol/default_conflict_handler.rs` | done | main | DefaultConflictHandler trait impl: is_equal via deep_equal, resolve returns real_master_state |
| `src/replication-protocol/conflicts.ts` | 2179 | T2 | `src/replication_protocol/conflicts.rs` | done | main | resolve_conflict_error; depends on RxStorageInstanceReplicationState + RxConflictHandlerInput (added to types/replication.rs) |
| `src/replication-protocol/helper.ts` | 2699 | T2 | `src/replication_protocol/helper.rs` | wip | main | doc_state_to_write_doc + write_doc_to_doc_state done; stripAttachmentsDataFromMetaWriteRows + getUnderlyingPersistentStorage deferred (need replication-state types) |
| `src/plugins/leader-election/index.ts` | 2937 | T2 | `src/plugins/leader_election/mod.rs` | done | main | single-process stub: is_leader always true, wait_for_leadership resolves immediately |
| `src/replication-protocol/checkpoint.ts` | 5506 | T2 | `src/replication_protocol/checkpoint.rs` | done | main | get_last_checkpoint_doc + set_checkpoint (with 409 conflict retry loop) + get_checkpoint_key (async hash); serialized via state.checkpoint_queue tokio::sync::Mutex |
| `src/plugins/replication/replication-helper.ts` | 3606 | T1 | `src/plugins/replication/replication_helper.rs` | wip | main | default_modifier, swap_default_deleted_to_deleted_field, await_retry, handle_pulled_documents_with_schema (schema variant). Deferred: handle_pulled_documents (RxCollection, phase-6), prevent_hibernate_browser_tab (browser-only) |
| `src/replication-protocol/meta-instance.ts` | 5669 | T1 | `src/replication_protocol/meta_instance.rs` | done | main | META_INSTANCE_SCHEMA_TITLE const + get_rx_replication_meta_instance_schema (Composite PrimaryKey id from itemId+isCheckpoint) + get_assumed_master_state + get_meta_write_row |
| `src/replication-protocol/index.ts` | 12372 | T1 | `src/replication_protocol/index_mod.rs` | done | main | All 6 exports ported: await_first_in_sync (tokio::select! over BehaviorSubjects), await_in_sync (lock all queues), await_idle (double-drain), cancel, **replicate_rx_storage_instance** (state construction + spawn upstream/downstream), **rx_storage_instance_to_replication_handler** (StorageReplicationHandler impl of RxReplicationHandler: master_change_stream maps EventBulk→DocumentsWithCheckpoint, master_changes_since via get_changed_documents_since_via_query, master_write with conflict detection). Note: upstream/downstream are skeleton-ports (wip), so replication isn't yet fully functional |
| `src/replication-protocol/downstream.ts` | 21692 | T1 | `src/replication_protocol/downstream.rs` | wip | main | **conflict-aware initial-sync + ongoing sync**: initial_checkpoint write, paginated master_changes_since (per-batch stream_queue.down lock for interleaving), persist_from_master with full 4-case decision (insert / skip-if-already-equal / fast-forward / true-conflict-resolve via conflict_handler.resolve), parallel fork+assumed-master reads (tokio::join!), meta-doc writes, set_checkpoint("down"). **Ongoing sync**: spawn_ongoing_downstream subscribes to replication_handler.master_change_stream(), each bulk runs through persist_from_master under stream_queue.down lock. **Cancel**: awaits events.canceled, aborts task. Deferred: addNewTask/streamQueue.then chained pattern, lastTimeMasterChangesRequested cutoff, nonPersistedFromMaster aggregation |
| `src/replication-protocol/upstream.ts` | 21852 | T1 | `src/replication_protocol/upstream.rs` | wip | main | **conflict-aware initial-sync + ongoing sync**: initial_checkpoint write, per-batch stream_queue.up lock initial sync via get_changed_documents_since_via_query, persist_to_master with assumed-master tracking + skip-if-already-equal + meta-doc writes after master_write, conflict-id detection, set_checkpoint("up") always. **Ongoing sync**: spawn_ongoing_upstream subscribes to fork.change_stream(), loop-avoidance via downstream_bulk_write_flag context check, each bulk runs through persist_to_master under stream_queue.up lock. **Cancel**: awaits events.canceled, aborts task. Deferred: masterChangeStream subscribe with active-up race protection, streamQueue.then chained pattern with initialSyncStartTime cutoff, conflict-retry via resolve_conflict_error + re-push, waitBeforePersist throttle, push.batchSize strict ongoing-event batching |
| `src/plugins/replication/index.ts` | 25090 | T1 | `src/plugins/replication/index_mod.rs` | wip | main | **functional port (wave-035)**: ReplicationOptions/PullOptions/PushOptions/PullHandlerResult types, PullHandler/PushHandler/DocumentModifier/StreamFactory closure types, RxReplicationState struct with 5 reactive subjects, replicate_rx_collection constructor + auto_start. `start()` is **now functional**: builds meta-instance via `database.storage.create_storage_instance` (new RxStorage trait), wraps user pull/push closures in `ClosureReplicationHandler` and calls `replicate_rx_storage_instance`. cancel() forwards to underlying state. Deferred: retry/awaitRetry, leader-election gating actually wired, browser-visibility toggle, addConnectedStorageToCollection |

## phase-4

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/plugins/replication-webrtc/webrtc-helper.ts` | 1498 | T1 | `src/plugins/replication_webrtc/webrtc_helper.rs` | done | main | is_master_in_webrtc_replication (hash-based deterministic master picking), send_message_and_await_answer (subscribes to response stream, sends frame, awaits matching (peer, request_id)) |
| `src/plugins/replication-webrtc/webrtc-types.ts` | 2977 | T1 | `src/plugins/replication_webrtc/webrtc_types.rs` | done | main | WebRTCMessage, WebRTCResponse, WebRTCWireFrame enum (collapses TS Message\|Response union), PeerWithMessage<P>, PeerWithResponse<P>, WebRTCConnectionHandler async-trait with associated Peer type. SyncOptionsWebRTC/RxWebRTCReplicationState deferred (need RxCollection phase-6) |
| `src/plugins/replication-webrtc/signaling-server.ts` | 5704 | T1 | `src/plugins/replication_webrtc/signaling_protocol.rs` | done | main | CTOX is signaling **client**, not server — skipped server-side WebSocketServer/peerById/peersByRoom routing. Protocol wire-types extracted into signaling_protocol.rs (ServerToClient/ClientToServer enums); N6 SignalingClient consumes them |
| `src/plugins/replication-webrtc/connection-handler-simple-peer.ts` | 10999 | T1 | `src/plugins/replication_webrtc/connection_handler_simple_peer.rs` | pending | — | WebRTC transport — primary (simple-peer handler replaced by webrtc-rs, see N5) |
| `src/plugins/replication-webrtc/index.ts` | 11079 | T1 | `src/plugins/replication_webrtc/index_mod.rs` | wip | main | **both paths wired (wave-035)**: replicate_web_rtc async fn + RxWebRTCReplicationPool struct. Master path: master_change_stream relay + message dispatcher (masterChangesSince/masterWrite). Fork path: `build_fork_replication_state` constructs PullHandler+PushHandler closures that tunnel `masterChangesSince` / `masterWrite` over the peer via send_message_and_await_answer, plus a StreamFactory that filters `response_stream` for `id == "masterChangeStream$"` and decodes to DocumentsWithCheckpoint. Per-peer RxReplicationState lives in PeerState; remove_peer cancels it. Deferred: SyncOptionsWebRTC user-facing shape + `RxWebRTCReplicationState` rename |

## phase-5

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/event-reduce.ts` | 5718 | T2 | `src/event_reduce.rs` | done | main | wave-036: stub-only — calculate_new_results always run_full_query_again=true (drops event-reduce-js dep) |
| `src/rx-change-event.ts` | 4539 | T1 | `src/rx_change_event.rs` | done | main | get_document_data_of_rx_change_event, rx_change_event_to_event_reduce_change_event (event-reduce-js JSON shape preserved despite N15 stub), flatten_events (dedup), rx_change_event_bulk_to_rx_change_events. EVENT_BULK_CACHE WeakMap omitted (perf-opt). New types: RxChangeEvent, RxChangeEventBulk |
| `src/change-event-buffer.ts` | 5084 | T1 | `src/change_event_buffer.rs` | done | main | T1 redesign: takes RxStream<EventBulk> instead of RxCollection (phase-6); WeakMap counter replaced by oldest_counter+index; RxJS Subscription → tokio JoinHandle.abort(); IDLE-task batching collapsed to sync mutex on event-arrival |

## phase-6

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/rx-query-single-result.ts` | 3580 | T3 | `src/rx_query_single_result.rs` | pending | — | single-result query |
| `src/index.ts` | 1039 | T2 | `src/index.rs` | pending | — | crate top-level export |
| `src/rx-collection-helper.ts` | 6390 | T2 | `src/rx_collection_helper.rs` | pending | — | collection helpers |
| `src/rx-query-helper.ts` | 10036 | T2 | `src/rx_query_helper.rs` | wip | main | normalize_mango_query + get_sort_comparator + get_query_matcher + prepare_query done (storage-relevant). runQueryUpdateFunction deferred (needs RxQuery/RxDocument phase-6). mingoSortComparator inlined as value_compare. New types: MangoQuery, DeterministicSortComparator, QueryMatcher |
| `src/rx-document-prototype-merge.ts` | 3779 | T1 | `src/rx_document_prototype_merge.rs` | done | main | T1 deviation: skipped. JS prototype manipulation (schemaProto + ormProto + basePrototype merge via defineProperty) has no Rust analog. ORM methods become user-defined impl blocks; createNewRxDocument folds into rx_collection.rs when ported |
| `src/query-cache.ts` | 4369 | T1 | `src/query_cache.rs` | pending | — | query cache — used internally by rx-collection |
| `src/rx-database-internal-store.ts` | 12046 | T1 | `src/rx_database_internal_store.rs` | done | main | wave-037: full port — INTERNAL_STORE_SCHEMA (build via fill_with_default_settings), 4 context constants, get_primary_key_of_internal_document, get_all_collection_documents (normalize_mango_query+prepare_query path), ensure_storage_token_document_exists with full conflict-path validation (DM5 major-skew + DB1 password-hash mismatch), is_database_state_version_compatible_with_database_code (v15→v16 carveout), add_connected_storage_to_collection + remove_connected_storage_from_collection with retry-on-conflict loops, _collection_name_primary. 10 unit tests including end-to-end ensure_storage_token_doc against memory storage (insert + conflict-path + DM5 reject). T1 deviation: helpers take explicit `internal_store: &Arc<dyn RxStorageInstance>` argument since full RxDatabase/RxCollection are stubs. Browser-only `sharding` config omitted. RxDatabase stub extended with internal_store/password/rxdb_version. Added types/internal_store.rs (InternalStoreDocType + StorageTokenData + CollectionDocData + ConnectedStorage) |
| `src/rx-document.ts` | 17383 | T1 | `src/rx_document.rs` | pending | — | RxDocument — required by rx-collection internals |
| `src/rx-query.ts` | 25120 | T1 | `src/rx_query.rs` | pending | — | RxQuery — required by rx-collection internals |
| `src/rx-database.ts` | 27854 | T1 | `src/rx_database.rs` | wip | main | **minimal stub**: name, token, storage_token, multi_instance, hash_function fields. wait_for_leadership (delegates to leader_election), is_leader. Full DB lifecycle (createRxDatabase, collections registry, internal-store, cleanup, plugins) deferred — this stub satisfies only the surface that replicate_web_rtc requires |
| `src/rx-collection.ts` | 38000 | T1 | `src/rx_collection.rs` | wip | main | **minimal stub**: name, database, storage_instance, conflict_handler, on_close_push, close. Full collection API (insert/upsert/find/observe, eventBulks$, etc.) deferred — this stub satisfies only the surface that replicate_web_rtc and rx_storage_instance_to_replication_handler require |

## phase-7

| Upstream | Bytes | Tier | Rust target | Status | Owner | Notes |
|---|---:|:---:|---|---|---|---|
| `src/plugins/test-utils/revisions.ts` | 288 | T2 | `tests/conformance/revisions.rs` | pending | — | conformance test infra — encryption import handled at phase-7 start |
| `src/plugins/test-utils/port-manager.ts` | 749 | T2 | `tests/conformance/port-manager.rs` | pending | — | conformance test infra — encryption import handled at phase-7 start |
| `src/plugins/test-utils/index.ts` | 777 | T2 | `tests/conformance/index.rs` | pending | — | conformance test infra — encryption import handled at phase-7 start |
| `src/plugins/test-utils/test-util.ts` | 1676 | T2 | `tests/conformance/test-util.rs` | pending | — | conformance test infra — encryption import handled at phase-7 start |
| `src/plugins/test-utils/replication.ts` | 3195 | T2 | `tests/conformance/replication.rs` | pending | — | conformance test infra — encryption import handled at phase-7 start |
| `src/plugins/test-utils/config.ts` | 3988 | T2 | `tests/conformance/config.rs` | pending | — | conformance test infra — encryption import handled at phase-7 start |
| `src/plugins/test-utils/schema-objects.ts` | 14880 | T2 | `tests/conformance/schema-objects.rs` | pending | — | conformance test infra — encryption import handled at phase-7 start |
| `src/plugins/test-utils/humans-collection.ts` | 16186 | T2 | `tests/conformance/humans-collection.rs` | pending | — | conformance test infra — encryption import handled at phase-7 start |
| `src/plugins/test-utils/schemas.ts` | 34776 | T2 | `tests/conformance/schemas.rs` | pending | — | conformance test infra — encryption import handled at phase-7 start |

## skip

| Upstream | Bytes | Reason |
|---|---:|---|
| `src/plugins/vector/helper.ts` | 0 | empty/stub upstream |
| `src/plugins/vector/types.ts` | 31 | empty/stub upstream |
| `src/plugins/vector/index.ts` | 71 | empty/stub upstream |
| `src/plugins/replication-websocket/index.ts` | 116 | V1 trim — WebSocket replication transport; we use WebRTC; signaling lives in replication-webrtc/signaling-server |
| `src/plugins/utils/utils-rxdb-version.template.ts` | 117 | MVP trim — build-time template; replaced by a Rust const (see "New code" N10) |
| `src/plugins/electron/index.ts` | 126 | electron-specific |
| `src/plugins/electron/electron-helper.ts` | 129 | electron-specific |
| `src/plugins/storage-dexie/index.ts` | 156 | browser-only (Dexie stays JS-side) |
| `src/plugins/storage-mongodb/index.ts` | 164 | out-of-scope backend |
| `src/plugins/storage-remote/index.ts` | 201 | out-of-scope multi-process scheme |
| `src/plugins/storage-foundationdb/foundationdb-helpers.ts` | 217 | out-of-scope backend |
| `src/plugins/cleanup/cleanup-helper.ts` | 346 | MVP trim — replaced by SQL-level tombstone GC (see "New code") |
| `src/plugins/replication-nats/nats-helper.ts` | 355 | SaaS-specific replication |
| `src/plugins/pipeline/index.ts` | 418 | V1 trim — type-only import from rx-collection; type stub provided (see "New code" N14) |
| `src/plugins/storage-denokv/denokv-types.ts` | 503 | out-of-scope backend |
| `src/plugins/update/mingo-updater.ts` | 611 | MVP trim — mango update DSL; CTOX writes full documents |
| `src/plugins/storage-denokv/denokv-helper.ts` | 645 | out-of-scope backend |
| `src/plugins/pipeline/types.ts` | 651 | V1 trim — type-only import from rx-collection; type stub provided (see "New code" N14) |
| `src/plugins/replication-supabase/types.ts` | 710 | SaaS-specific replication |
| `src/plugins/storage-remote/storage-remote-helpers.ts` | 770 | out-of-scope multi-process scheme |
| `src/plugins/replication-appwrite/appwrite-types.ts` | 795 | SaaS-specific replication |
| `src/plugins/state/types.ts` | 798 | defer |
| `src/plugins/validate-is-my-json-valid/index.ts` | 833 | pick one validator; ajv also deferred |
| `src/plugins/storage-mongodb/mongodb-types.ts` | 918 | out-of-scope backend |
| `src/plugins/replication-couchdb/couchdb-types.ts` | 929 | SaaS-specific replication |
| `src/plugins/storage-localstorage/localstorage-mock.ts` | 934 | browser-only |
| `src/plugins/state/index.ts` | 1011 | defer |
| `src/plugins/query-builder/mquery/mquery-utils.ts` | 1013 | MVP trim — chained query DSL; no user-facing queries on CTOX side |
| `src/plugins/replication-websocket/websocket-types.ts` | 1029 | V1 trim — WebSocket replication transport; we use WebRTC; signaling lives in replication-webrtc/signaling-server |
| `src/plugins/migration-schema/migration-types.ts` | 1042 | V1 trim — type-only import from rx-database/rx-collection; type stub provided (see "New code" N12) |
| `src/plugins/replication-nats/nats-types.ts` | 1068 | SaaS-specific replication |
| `src/plugins/replication-appwrite/appwrite-helpers.ts` | 1108 | SaaS-specific replication |
| `src/plugins/storage-remote-websocket/types.ts` | 1142 | out-of-scope multi-process scheme |
| `src/plugins/vector/vector-distance.ts` | 1178 | empty/stub upstream |
| `src/plugins/utils/utils-premium.ts` | 1238 | MVP trim — license checks; utils/index.ts barrel re-export omitted, constant-false replacement in foundation |
| `src/plugins/storage-denokv/index.ts` | 1269 | out-of-scope backend |
| `src/plugins/utils/utils-base64.ts` | 1277 | MVP trim — via utils-blob for attachments; utils/index.ts barrel re-export omitted |
| `src/plugins/storage-mongodb/rx-storage-mongodb.ts` | 1310 | out-of-scope backend |
| `src/plugins/cleanup/index.ts` | 1442 | MVP trim — replaced by SQL-level tombstone GC (see "New code") |
| `src/plugins/replication-mongodb/mongodb-types.ts` | 1461 | SaaS-specific replication |
| `src/plugins/replication-graphql/helper.ts` | 1474 | GraphQL replication |
| `src/plugins/validate-ajv/index.ts` | 1490 | MVP trim — browser-side validates pre-push |
| `src/plugins/dev-mode/check-migration-strategies.ts` | 1519 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/update/index.ts` | 1519 | MVP trim — mango update DSL; CTOX writes full documents |
| `src/plugins/validate-z-schema/index.ts` | 1539 | pick one validator; ajv also deferred |
| `src/plugins/storage-localstorage/index.ts` | 1544 | browser-only |
| `src/plugins/electron/rx-storage-ipc-main.ts` | 1570 | electron-specific |
| `src/plugins/replication-graphql/graphql-websocket.ts` | 1602 | GraphQL replication |
| `src/plugins/replication-supabase/helper.ts` | 1688 | SaaS-specific replication |
| `src/plugins/dev-mode/check-orm.ts` | 1695 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/electron/rx-storage-ipc-renderer.ts` | 1816 | electron-specific |
| `src/plugins/utils/utils-blob.ts` | 1826 | MVP trim — attachments helper only; utils/index.ts barrel ported without this re-export |
| `src/plugins/storage-foundationdb/index.ts` | 1841 | out-of-scope backend |
| `src/plugins/replication-mongodb/mongodb-helper.ts` | 1921 | SaaS-specific replication |
| `src/plugins/storage-dexie/rx-storage-dexie.ts` | 2111 | browser-only (Dexie stays JS-side) |
| `src/plugins/dev-mode/entity-properties.ts` | 2115 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/migration-schema/index.ts` | 2183 | V1 trim — type-only import from rx-database/rx-collection; type stub provided (see "New code" N12) |
| `src/plugins/state/helpers.ts` | 2263 | defer |
| `src/plugins/storage-foundationdb/foundationdb-types.ts` | 2268 | out-of-scope backend |
| `src/plugins/replication-firestore/firestore-helper.ts` | 2308 | SaaS-specific replication |
| `src/plugins/replication-firestore/firestore-types.ts` | 2329 | SaaS-specific replication |
| `src/plugins/flutter/index.ts` | 2347 | flutter-specific |
| `src/plugins/query-builder/index.ts` | 2468 | MVP trim — chained query DSL; no user-facing queries on CTOX side |
| `src/plugins/replication-couchdb/couchdb-helper.ts` | 2511 | SaaS-specific replication |
| `src/plugins/replication-webrtc/connection-handler-p2pcf.ts` | 2536 | V1 trim — alternative connection handler (p2pcf lib); we use simple-peer port → webrtc-rs |
| `src/plugins/storage-sqlite/index.ts` | 2544 | RxDB Premium trial stub — we build our own SQLite backend |
| `src/plugins/attachments/attachments-utils.ts` | 2702 | parquet stored as external content-addressed file; type stub provided (see "New code" N11) |
| `src/plugins/storage-remote/message-channel-cache.ts` | 2761 | out-of-scope multi-process scheme |
| `src/plugins/local-documents/index.ts` | 2794 | defer |
| `src/plugins/dev-mode/unallowed-properties.ts` | 2833 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/cleanup/cleanup-state.ts` | 3092 | MVP trim — replaced by SQL-level tombstone GC (see "New code") |
| `src/plugins/backup/file-util.ts` | 3133 | V1 trim — type-only import from rx-database; type stub provided (see "New code" N13) |
| `src/plugins/local-documents/local-documents-helper.ts` | 3286 | defer |
| `src/plugins/pipeline/flagged-functions.ts` | 3394 | V1 trim — type-only import from rx-collection; type stub provided (see "New code" N14) |
| `src/plugins/dev-mode/dev-mode-tracking.ts` | 3513 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/storage-mongodb/mongodb-helper.ts` | 3542 | out-of-scope backend |
| `src/plugins/storage-remote/storage-remote-types.ts` | 3557 | out-of-scope multi-process scheme |
| `src/plugins/json-dump/index.ts` | 3701 | MVP trim — export/import not on sync path |
| `src/plugins/storage-remote-websocket/index.ts` | 3902 | out-of-scope multi-process scheme |
| `src/plugins/dev-mode/check-document.ts` | 4146 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/local-documents/local-documents.ts` | 4224 | defer |
| `src/plugins/attachments-compression/index.ts` | 4228 | parquet self-compressed; rxdb attachments not used |
| `src/plugins/storage-sqlite/sqlite-types.ts` | 4282 | RxDB Premium trial stub — we build our own SQLite backend |
| `src/plugins/replication-websocket/websocket-server.ts` | 4510 | V1 trim — WebSocket replication transport; we use WebRTC; signaling lives in replication-webrtc/signaling-server |
| `src/plugins/storage-denokv/denokv-query.ts` | 4523 | out-of-scope backend |
| `src/plugins/cleanup/cleanup.ts` | 4758 | MVP trim — replaced by SQL-level tombstone GC (see "New code") |
| `src/plugins/migration-schema/migration-helpers.ts` | 5033 | V1 trim — type-only import from rx-database/rx-collection; type stub provided (see "New code" N12) |
| `src/plugins/replication-webrtc/connection-handler-webtorrent.ts` | 5229 | V1 trim — alternative connection handler (webtorrent); we use simple-peer port → webrtc-rs |
| `src/plugins/storage-foundationdb/foundationdb-query.ts` | 5780 | out-of-scope backend |
| `src/plugins/replication-mongodb/mongodb-checkpoint.ts` | 6360 | SaaS-specific replication |
| `src/plugins/dev-mode/index.ts` | 6466 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/replication-websocket/websocket-client.ts` | 6524 | V1 trim — WebSocket replication transport; we use WebRTC; signaling lives in replication-webrtc/signaling-server |
| `src/plugins/replication-graphql/query-builder-from-rx-schema.ts` | 6745 | GraphQL replication |
| `src/plugins/dev-mode/check-query.ts` | 6778 | MVP trim — runtime debug checks, not needed in production |
| `src/plugins/storage-dexie/dexie-query.ts` | 7391 | browser-only (Dexie stays JS-side) |
| `src/plugins/storage-sqlite/sqlite-helpers.ts` | 7687 | RxDB Premium trial stub — we build our own SQLite backend |
| `src/plugins/encryption-crypto-js/index.ts` | 7747 | defer |
| `src/plugins/migration-storage/index.ts` | 8075 | MVP trim — defer |
| `src/plugins/key-compression/index.ts` | 8105 | defer |
| `src/plugins/replication-graphql/index.ts` | 8196 | GraphQL replication |
| `src/plugins/attachments/index.ts` | 8451 | parquet stored as external content-addressed file; type stub provided (see "New code" N11) |
| `src/plugins/storage-remote/rx-storage-remote.ts` | 9452 | out-of-scope multi-process scheme |
| `src/plugins/backup/index.ts` | 9496 | V1 trim — type-only import from rx-database; type stub provided (see "New code" N13) |
| `src/plugins/storage-dexie/dexie-helper.ts` | 9564 | browser-only (Dexie stays JS-side) |
| `src/plugins/pipeline/rx-pipeline.ts` | 9997 | V1 trim — type-only import from rx-collection; type stub provided (see "New code" N14) |
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
| `src/plugins/migration-schema/rx-migration-state.ts` | 21720 | V1 trim — type-only import from rx-database/rx-collection; type stub provided (see "New code" N12) |
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

- This `PORTING.md` is the single live master. Subagents edit it in place during a wave.
- At every wave boundary or scope revision, the main agent snapshots into `revisions/PORTING.wave-NNN-<slug>.md` (immutable).
- A wave = one parallel subagent batch (or one sequential foundation step) that ends with `cargo build` green. After commit, snapshot before kicking off the next wave.
- Snapshot naming: `wave-000-baseline.md`, `wave-001-scope-trimmed.md`, `wave-002-v1-applied.md`, `wave-003-phase0-foundation-t1.md`, …
