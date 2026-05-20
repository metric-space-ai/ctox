# PORT_STYLE.md — RxDB → Rust Port Conventions

**This file is binding for every Rust file produced by the port.** Subagents must read it cold before claiming any row in [PORTING.md](PORTING.md).

It exists because byte-correct TS→Rust portage requires uniform decisions on dozens of small things that would otherwise produce inconsistent, non-composable output across files written by different agents.

If something is not covered here, **do not invent** — stop, leave the row at `pending`, and report.

---

## 0. Frozen Upstream

- Source tree: `vendor/rxdb-16.20.0/` (read-only)
- Commit pin: `vendor/rxdb.version`
- Never modify any file under `vendor/`. If upstream needs to be re-pinned, that is a separate decision by the main agent.

## 1. Required reading before porting one row

1. [PORTING.md](PORTING.md) — the row, its phase, its tier, its target path.
2. This `PORT_STYLE.md` — all sections.
3. The upstream file you are porting (`vendor/rxdb-16.20.0/<upstream-path>`) — the entire file, not just the function you start with.
4. Already-ported files in the same module — to keep idiom consistent.

## 2. Crate dependencies (pinned)

The following crates are the **only** sanctioned external dependencies for `rxdb-rs`. Do not add others without main-agent approval.

| Crate | Version | Used for |
|---|---|---|
| `tokio` | `1.50` (CTOX-pin) | async runtime |
| `tokio-stream` | latest CTOX | `Stream` adapters |
| `futures` | latest CTOX | future combinators |
| `async-trait` | latest CTOX | async fns in traits |
| `rusqlite` | CTOX-pin | SQLite backend (sync; wrap in `spawn_blocking`) |
| `serde` (with `derive`), `serde_json` | CTOX-pin | document JSON |
| `thiserror` | latest CTOX | `RxError` typed enum |
| `parking_lot` | latest CTOX | sync `Mutex`/`RwLock` |
| `dashmap` | latest stable | concurrent maps (doc-cache, query-cache) |
| `arc-swap` | latest CTOX | overwritable Rust idiom (N4) |
| `sha2` | latest CTOX | SHA-256 (replaces `js-sha256`) |
| `chrono` *or* `std::time` | std preferred | timestamps; `chrono` only if upstream uses dates |
| `bytes` | latest CTOX | binary buffers |
| `tracing` | latest CTOX | structured logging |
| `webrtc` (webrtc-rs) | latest stable | WebRTC transport (Phase 4) |
| `tokio-tungstenite` | latest CTOX | WebSocket signaling (Phase 4, if needed) |
| `uuid` | latest CTOX | IDs only where upstream uses uuids; otherwise prefer revision-style IDs |

**No new dependency** beyond this list without writing a one-paragraph justification in the PR body. `anyhow` is **forbidden inside `rxdb-rs`** — use `RxError` everywhere. (`anyhow` is fine in CTOX glue outside the crate.)

## 3. Error model

```rust
// src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum RxError {
    /// Upstream error code (e.g. "DOC1", "VAL2"). Preserved 1:1 from upstream rx-error.ts.
    #[error("{code}: {message}")]
    Coded {
        code: &'static str,
        message: String,
        parameters: serde_json::Value,
    },
    // Wrap external errors at the rxdb-rs boundary, never bare.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("schema validation: {0}")]
    Schema(String),
    // ...
}

pub type RxResult<T> = std::result::Result<T, RxError>;
```

- Every function that may fail returns `RxResult<T>`.
- Upstream `newRxError('XYZ1', { foo: bar })` → `Err(RxError::Coded { code: "XYZ1", message: …, parameters: serde_json::json!({ "foo": bar }) })`.
- The mapping table from upstream error codes to `code` constants is the verbatim list from `rx-error.ts`.

## 4. Async model

- Async runtime: **`tokio` multi-threaded**. No `async-std`, `smol`, `futures::executor`.
- `Promise<T>` in TS → `impl Future<Output = RxResult<T>> + Send` in Rust (with `async fn` where method).
- `Promise<void>` → `RxResult<()>`.
- Methods on traits that return `Promise` → `#[async_trait]`.
- Blocking work (rusqlite calls, file I/O) → wrap in `tokio::task::spawn_blocking`. Never block the async runtime.
- Mutexes inside async code:
  - Use `parking_lot::Mutex` only if the critical section is short and pure-CPU.
  - Use `tokio::sync::Mutex` if you `.await` inside the critical section.
  - Never hold a `parking_lot::Mutex` across `.await`.

## 5. Reactive layer (RxJS → Rust)

This is the most common TS→Rust friction. The mapping is via `src/rxjs_compat.rs` (N3). Use these mappings consistently:

| RxJS | Rust |
|---|---|
| `Subject<T>` | `tokio::sync::broadcast::{Sender<T>, Receiver<T>}` wrapped in our `RxSubject<T>` |
| `BehaviorSubject<T>` | `tokio::sync::watch::{Sender<T>, Receiver<T>}` wrapped in our `RxBehaviorSubject<T>` |
| `Observable<T>` | `impl Stream<Item = T>` (from `tokio_stream`) |
| `observable.subscribe(fn)` | `tokio::spawn` a task that consumes the stream |
| `firstValueFrom(obs)` | `stream.next().await.ok_or(RxError::…)` |
| `obs.pipe(filter(p))` | `stream.filter(p)` (from `futures::StreamExt`) |
| `obs.pipe(map(f))` | `stream.map(f)` |
| `obs.pipe(mergeMap(f))` | `stream.then(f).flatten()` |
| `obs.pipe(startWith(v))` | `tokio_stream::once(v).chain(stream)` |
| `obs.pipe(switchMap(f))` | not a direct equivalent — use a select-loop, document inline |
| `Subject.complete()` | drop the `Sender` |
| `Subject.error(e)` | send a sentinel `Result::Err(e)` over the channel; document |

When porting a file that uses RxJS:
- Import only from `crate::rxjs_compat` (never from a third-party "Rust RxJS" crate — there isn't a stable one).
- If you hit an operator not on the table, **stop and report**. We extend `rxjs_compat` centrally, not per-file.

## 6. Identifier naming

| Upstream (TS) | Rust |
|---|---|
| `camelCaseFn` | `snake_case_fn` |
| `PascalCaseClass` / `PascalCaseType` | `PascalCaseStruct` / `PascalCaseEnum` |
| `UPPER_CONST` | `UPPER_CONST` |
| `_underscorePrefix` (TS private) | `pub(crate)` or `pub(super)` visibility; drop the leading underscore unless required by an RxDB protocol field (`_rev`, `_deleted`, etc., which are wire-level field names) |
| RxDB protocol fields (`_rev`, `_deleted`, `_meta`, `_attachments`) | preserved verbatim as struct field names |
| File names | `kebab-case.ts` → `snake_case.rs`; e.g. `rx-storage-helper.ts` → `rx_storage_helper.rs` |

Function arguments and locals follow the same `camelCase → snake_case` rule. **Do not abbreviate** beyond what upstream uses (`doc`, `evt`, `cb` stay; do not invent `d`, `e`).

## 7. `// ref:` anchor format

Every ported `fn`, `struct`, `enum`, `trait`, and top-level `const` must carry a `ref:` anchor immediately above it:

```rust
// ref: rxdb/src/plugins/utils/utils-revision.ts:34-52
pub fn create_revision(...) -> String {
    // ...
}
```

Rules:
- Path is relative to upstream root, i.e. starts with `rxdb/src/...`. **Not** `vendor/rxdb-16.20.0/...`.
- Line range is the exact upstream lines (inclusive).
- One anchor per item. If multiple TS functions collapse into one Rust function (rare; needs main-agent approval), list both ranges separated by ` + `.
- If a function is added that has no upstream counterpart (rare for ports; common for `New code` items), use `// ref: rxdb-rs new code` and explain in a doc-comment why.

## 8. Module layout

```
src/core/rxdb/
├── Cargo.toml
├── PORTING.md
├── PORT_STYLE.md
├── revisions/
├── vendor/rxdb-16.20.0/
└── src/
    ├── lib.rs                 # crate top-level (mirrors upstream `src/index.ts`)
    ├── error.rs               # RxError
    ├── overwritable.rs        # N4
    ├── rxjs_compat.rs         # N3
    ├── hooks.rs
    ├── plugin.rs
    ├── plugin_helpers.rs
    ├── rx_schema.rs
    ├── rx_schema_helper.rs
    ├── custom_index.rs
    ├── query_planner.rs
    ├── rx_query_mingo.rs
    ├── rx_query_helper.rs
    ├── rx_storage_helper.rs
    ├── rx_storage_multiinstance.rs
    ├── incremental_write.rs
    ├── doc_cache.rs
    ├── replication_protocol/
    │   ├── mod.rs
    │   ├── upstream.rs
    │   ├── downstream.rs
    │   └── …
    ├── plugins/
    │   ├── mod.rs
    │   ├── attachments/stub.rs       # N2/N11
    │   ├── backup/types_stub.rs      # N13
    │   ├── migration_schema/types_stub.rs  # N12
    │   ├── pipeline/types_stub.rs    # N14
    │   ├── leader_election/
    │   ├── replication/
    │   ├── replication_webrtc/
    │   │   ├── mod.rs
    │   │   ├── connection_handler_rs.rs   # N5 (replaces simple-peer)
    │   │   ├── signaling_client.rs        # N6
    │   │   ├── webrtc_helper.rs
    │   │   ├── webrtc_types.rs
    │   │   └── signaling_server.rs
    │   └── storage_memory/
    ├── storage/
    │   └── sqlite/               # N1, N7
    │       ├── mod.rs
    │       ├── instance.rs
    │       ├── schema.rs
    │       └── cleanup.rs
    ├── util/                     # N8, N10
    │   ├── mod.rs
    │   ├── idle_queue.rs         # custom-idle-queue mini-port
    │   ├── oblivious_set.rs      # oblivious-set mini-port
    │   ├── push_at_sort.rs       # array-push-at-sort-position mini-port
    │   └── version.rs            # RXDB_VERSION const
    ├── rx_change_event.rs
    ├── change_event_buffer.rs
    ├── event_reduce.rs           # N15 (stub-port)
    ├── rx_database.rs
    ├── rx_database_internal_store.rs
    ├── rx_collection.rs
    ├── rx_collection_helper.rs
    ├── rx_document.rs
    ├── rx_document_prototype_merge.rs
    ├── rx_query.rs
    ├── rx_query_single_result.rs
    └── query_cache.rs
```

When you port a file, place it at the path PORTING.md says. If your file's natural parent module doesn't exist yet, **stop and report** — the main agent creates module roots.

## 9. Imports and visibility

- Inside `rxdb-rs`, always use `crate::` paths, never `super::` chains longer than 1 level.
- `pub` only when the symbol is part of the public crate API. Default to `pub(crate)`.
- Group `use` statements: std → external → crate. One blank line between groups.

## 10. JSON document representation

Upstream RxDB documents are loosely typed `Record<string, any>`. In Rust:

```rust
pub type DocumentData = serde_json::Map<String, serde_json::Value>;
```

- Document-shape data uses `serde_json::Value` end-to-end on the storage path. Type-safe structs are defined per-collection by the **user**, not by `rxdb-rs`.
- Protocol fields are accessed with `.get("_rev")` etc.; we provide helper functions in `util::doc::*` for the common ones.

## 11. Tests

- Each ported file gets a `#[cfg(test)] mod tests { ... }` block at the bottom **only if upstream has tests for it** (port them). For ported code without upstream tests, add tests in Phase-7 (conformance harness), not inline.
- `cargo test --package rxdb-rs` must pass after every wave.

## 12. Subagent workflow (T2 / T3 only)

A subagent that claims a row from PORTING.md does exactly the following:

1. Re-read this `PORT_STYLE.md` from top.
2. `Edit` PORTING.md: change the row's `Status` `pending → claimed` and `Owner` `— → <agent-id>`. **One Edit, one row.** Commit.
3. Read the upstream file at `vendor/rxdb-16.20.0/<row-upstream>` completely.
4. Read all already-ported files that the upstream imports from (their Rust counterparts in `src/`). If any required Rust counterpart is missing, revert the claim (`claimed → pending`, owner cleared) and stop.
5. Create the Rust file at the row's `Rust target` path. Apply all rules above. Every item carries a `// ref:` anchor.
6. `cargo build -p rxdb-rs` must succeed. If not, fix until it does. Do not mark `done` with a red build.
7. `Edit` PORTING.md: change `Status` `claimed → done`. Commit.

Subagents may not:
- Edit any column other than `Status` and `Owner`.
- Modify files outside their claimed row's target path (no drive-by edits).
- Add new external dependencies.
- Touch `vendor/`, `revisions/`, `PORT_STYLE.md`, or any T1 file.

## 13. What NOT to do

- ❌ Refactor for "cleanliness" — upstream's structure is the spec.
- ❌ Skip an edge case because "Rust handles that". Port the check.
- ❌ Replace an explicit loop with iterator combinators unless upstream is already iterator-style.
- ❌ Add new helper functions not present upstream.
- ❌ Reorder items within a file (top-to-bottom order matches upstream).
- ❌ Translate JSDoc into Rust doc-comments verbatim if it references TS types — adapt the type names, keep the algorithmic description.
- ❌ Use `anyhow::Error` inside `rxdb-rs`.
- ❌ `unwrap()` outside tests. Use `?` with `RxError` or document why a panic is correct.
- ❌ `clone()` proliferation — match upstream's reference semantics with `&T` / `Arc<T>` deliberately.

## 14. Quick TS → Rust mapping cheatsheet

| TS pattern | Rust equivalent |
|---|---|
| `class Foo { constructor(a) { this.a = a } method() {} }` | `pub struct Foo { pub a: A } impl Foo { pub fn new(a: A) -> Self { Self { a } } pub fn method(&self) {} }` |
| `class Foo extends Bar` | Composition (`pub struct Foo { base: Bar, … }`) or trait (`impl BarLike for Foo`). No inheritance. |
| `Map<K, V>` (preserves insertion order) | `indexmap::IndexMap<K, V>` if order matters; `HashMap<K, V>` otherwise |
| `Set<T>` | `HashSet<T>` |
| `Promise.all(arr)` | `futures::future::try_join_all(arr).await` |
| `Promise.race(arr)` | `futures::future::select_all(arr).await` |
| `Object.entries(o)` | `o.iter()` over `Map`/`Vec<(K,V)>` |
| `JSON.parse(s)` | `serde_json::from_str::<Value>(&s)?` |
| `JSON.stringify(v)` | `serde_json::to_string(&v)?` |
| `setTimeout(fn, ms)` | `tokio::time::sleep(Duration::from_millis(ms)).await; fn()` |
| `setInterval(fn, ms)` | `tokio::time::interval(...)` driven loop |
| `throw new Error('…')` | `return Err(RxError::Coded { code: …, message: … });` |
| `try { … } catch (e) { … }` | `match expr { Ok(v) => …, Err(e) => … }` or `expr.unwrap_or_else(…)` |
| TS optional chaining `a?.b?.c` | `a.as_ref().and_then(|x| x.b.as_ref()).and_then(|y| y.c.clone())` |
| TS nullish coalescing `a ?? b` | `a.unwrap_or(b)` |

---

## 15. Updating this file

Only the main agent edits `PORT_STYLE.md`. If a subagent hits a recurring pattern that needs a rule, it reports — does not edit. New rules land between waves, never during one.
