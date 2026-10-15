# CTOX client kernel (App API 4.0)

No UI, build step, runtime package install, HTTP business-data bridge, or imports from the V3 runtime.

```js
import { createKernel, createShellKernel } from './kernel.mjs';
import { createLoader } from './loader.mjs';
const loader = await createLoader({ instanceId, userId, actor, transport });
// Shell module exports startShell(kernel, {restored}) and stopShell().
const kernel = loader.kernel;
const shell = createShellKernel(kernel);
const api = kernel.appApi('notes', '1');
const query = api.query('notes', {selector: {done: false}});
const off = query.subscribe(render);
const h = api.put('notes', {id: 'n1', done: false});
await h.local; // atomic docs+outbox durability
await h.result; // core accepted/rejected/migrated, pending while offline
```

`createKernel` requires authenticated instanceId/userId. A dbName override cannot bypass its durable identity binding.
The default transport is offline; it NEVER manufactures acceptance. `createStubKernel` explicitly selects
`MemoryCoreTransport` for conformance tests and Shell development only. It is not a WebRTC adapter or tenant path.

## Modules

- `store-idb-v5.mjs`: seven generic client stores, atomic IndexedDB and rollback-capable memory backend.
- `cache.mjs`: conservative 2 GB document/blob cache budget, collection LRU, one quota reclaim/retry;
  pending input, archive, window state and release artifacts are protected. Eviction drops both visible/base data,
  invalidates the scoped cursor, and resumes the normal P1 subscription on the next local access (also from readers).
  Protected input may exceed the soft cache budget and is surfaced in the status zone.
- `migrate-v3.mjs`: version4 `documents` takeover in the upgrade transaction; identity-owned database only.
  Confirmed rows retain their base; V3 unconfirmed input is also archived, never blindly dispatched under V4 authority.
- `leader.mjs`: Web Lock generations; effect and generation check share an IDB transaction.
- `outbox.mjs`: optimistic base plus ordered operations, immutable op IDs and chained base revisions.
- `app-api.mjs`/`values.mjs`: Mango, JSON validation, JSON Schema subset, errors, hashing.
- `kernel.mjs`: local live views, shared outbox, leader dispatch, transactional page cursor, reader reconciliation,
  activation, files, Shell/actor/status streams; app mounting lives exclusively in Shell/testkit.
- `transport.mjs`: KernelTransport seam. `connect(identity,{page,write_result,link})`, `subscribe(scope)`,
  `write(entry)`, `appActivation(report)`, `priority(hint)`, `artifact(hash)`, `blob(ref)`, `upload(ref,blob)`.
  A write may return its result or deliver `write_result`; duplicate results are harmless. The adapter receives
  op_id/prev_op_id/base_rev/base_seq/gen/app_ver. Pages carry decimal-string seq/cursor_to, epoch, scope_hash,
  gen, docs, complete and per-subscription backlog. Signature verification is the adapter's responsibility.
- `schema-diff.mjs`: `collections.schema.json` parsing and schema diff (§16.22 A6/A9), byte-identical rules to the stub;
  compatible change vs. new generation, a generation needs a migration (`from_ver` = draft base, `migrate_fn` or `steps`).
- Shell API additions (K-1b, §16.19): `kernel.activation.reportFailed(appId, ver, reason)` (failed for this client,
  pointer back to the last good version, `app_activation{state:'failed'}`), `kernel.windowState.get/put/del(key)` per
  tab (`tabKey`, sessionStorage) and user in IDB `window_state`, `kernel.apps` entries carry `layout` (9 fields, missing =
  null) and `styles`. `shell_preferences` is a ctox-platform collection: one row per user (`id = actor.id`), only the
  own row is readable/writable. Draft publish (§16.22/§16.26): `manifest.json` is never a draft file, references to
  missing files are `invalid`, and every non-computed base manifest field (triggers, command_rights, …) is inherited (B4).
  Artifacts are bytes: `string` stays UTF-8, `Blob`/`ArrayBuffer`/`Uint8Array` are hashed and stored as bytes.
- K-1c/K-1d (§16.39): `artifacts.url(appId, ver, path, {shellVer?})` is synchronous and remembers nothing; the root carries
  `~shell@<sv>` and `~lib:<name>@<M.m>` for every library of the shell manifest (`libs`). `artifacts.bind(appId, ver)` is
  called once per mount (testkit app host, loader via loadModule) and freezes that binding. Every activation (app, shell)
  and every fetched lib minor writes meta `files:<appId>@<ver>` = {path: sha256}; the SW resolves path → sha256 there and
  reads `artifacts[sha256].content` (string | Uint8Array). There is no `manifests` store. Libraries are
  platform artifacts `lib:<name>` (never activated eagerly); the SW reports `ctox:lib-miss` (kernel fetches exactly the
  URL's minor, sha256-checked) and `ctox:lib-hit` (LRU over IDB `artifacts` within `libByteBudget`). Source drafts live in
  the platform collection `app_drafts` (`<user>/<app>`, files as sha256 in the local CAS). `kernel.shell.publishApp|
  updateApp|rollbackApp|releaseApp|removeApp` issue `ctox.app.*` with CAS on `base_app_ver`. Publishing the same app twice
  queues (no `busy`). An activated version clears kernel status entries of other versions in every tab.
- K-1e: `kernel.metrics.collections[name] = {state: live|catching_up|offline|error, backlog, gen, since_ms, error?}` (§16.45;
  live = link online and backlog 0 after a complete page for the subscribed scope; follower tabs get it via the leader),
  `kernel.reconnect()` re-brings-up the transport (never a page reload), `kernel.identity.device_id` (§16.43, from
  `opts.deviceId`; fake core deterministic per instance+user+device).
- `fake-core.mjs` + `fake-core-runtime.mjs` (tests/Shell development only): one core per `coreNamespace` for several users
  (own DB each, core-authoritative role), row/collection/command rights, in-process handler runtime routed by
  (app_id, type) with relative imports and `ctx.read/get/put/patch/remove/command/asset/cap`, `on`/`every` triggers
  (foreign collections only with owner `exports:['watch']`), `audit.event` and `agent.task.*` with a fake harness and
  exactly-once callbacks, and `ctox.app.*` (archives, never deletes). Commands without a handler fail with `no_handler`.
- `fake-core.mjs`: explicit in-memory core from the executable stub contract; isolated from client IDB.
- `shell-api.mjs`/`loader.mjs`: Shell-only API and live Shell bootstrap/replacement. `/sw.js` belongs to Shell v4;
  artifact route `/~a/<app>@<version>~shell@<shellVersion>/<path>` and `ctox:claim` before every Shell import.
- `transport-webrtc.mjs`: authenticated P4 hello, canonical scopes, signed progress pages, write results,
  clock metrics, and bounded CAS transfers over reliable RTCDataChannels. The host supplies `hello`,
  `verifyPackage`, `resolveScope` and either channels or `createPeer(address, identity)`; reconnect uses the factory.
- `scheduler.mjs`/`ingress.mjs`/`framing.mjs`: control > commands > interactive, then byte DRR 8:3:1
  live/initial/transfer; per-lane ACK windows, aggregate SCTP budget, parallel ordered lane ingress.
  Frames are at most 16 KiB total. The 32-byte big-endian header is CTX4 magic, lane/codec/reserved,
  then u32 packet/index/count/encodedBytes/rawBytes/frameSeq. Logical messages are UTF-8 JSON.
  Native P5/P8 must implement this same framing; the executable fake native peer is under tests only.
- `platform.mjs`: shared collections belong to `ctox-platform`, including user-owned `shell_preferences`.
- `testkit/app-host.mjs`: explicit reference host for tests/Shell development, never imported by the kernel;
  owns mount/snapshot/restore and reports activation failures through the public kernel contract.

`index.d.ts` retains the normative App API from the binding stub, with the real kernel/transport additions.
Readers never wait for initial sync or call a remote query. New collections require no IDB version increase.
Both initial catch-up and changes use `page`; data and the scoped checkpoint commit together, stale doc seq cannot win.

## Checks

On the admitted GPU lane: `node --test --test-concurrency=2 src/apps/business-os/kernel/tests/*.test.mjs`.
Browser: `PLAYWRIGHT_MODULE=<existing package> CHROME_BIN=<existing chromium> node src/apps/business-os/kernel/tests/browser.mjs`.
PNG icon round trip (K-1b): same variables, `node src/apps/business-os/kernel/tests/browser-png.mjs` (runs on macOS with Google Chrome).
The latter starts only its own bounded loopback static fixture and isolated headless context; no tenant or credentials.
The explicit Node lock adapter is test-only; browsers use actual navigator.locks and IndexedDB.

Without a controlling SW, the Blob/data fallback supports a self-contained entry; relative module imports
use the Shell-owned versioned SW artifact route. Publication/authorization remain Core responsibilities.

No old runtime is switched off by this slice. Native-counterpart integration is phases5/8; no production hot-plug
claim until the installed all-app acceptance. Old runtime retirement requires zero usage for24h on WELSCH.
