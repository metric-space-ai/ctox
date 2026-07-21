# Developing Business OS Apps

Business OS apps are installable client applications. A normal app consists of
HTML, CSS, JavaScript and a declarative collection schema. It does not need its
own HTTP API, database service, WebSocket server or compiled CTOX handler.

The platform supplies:

- local IndexedDB persistence and reactive queries;
- write-ahead recovery for offline/local writes;
- WebRTC synchronization with the authoritative CTOX SQLite peer;
- reconnect, checkpoints, demand loading and multi-tab leadership;
- server-authoritative read/write grants and multiuser distribution;
- a Command Bus for work that must be durable, delegated or executed outside
  the client.

## Product contract

The intended developer experience is the same as installing a native desktop
app: copy or install an app package into a running Business OS and open it. A
new data app must not require an edit in `src/core/`, a Rust match arm, a wire
fixture change or a CTOX binary rebuild.

Current implementation status:

| Capability | Status |
| --- | --- |
| Runtime-loaded app schemas from `collections.schema.json` | Available |
| Browser CRUD through shell-provided database handles | Available |
| IndexedDB ↔ native SQLite WebRTC sync | Available |
| Permission-filtered multiuser replication | Available |
| Runtime-installed/local app packages | Available |
| Declarative migrations | Available |
| Generic runtime action/Saga definitions without per-app Rust handlers | Available in runtime v1 |
| Supervised schema/action reconciliation without daemon recompile | Available in runtime v1 |

The release qualification still installs unknown schemas/actions into an
already-built binary. Runtime v1 does not permit arbitrary SQL, shell, network,
host-path or browser-code effects.

## Package layout

```text
my-app/
  module.json
  collections.schema.json
  schema.js
  index.html
  index.js
  index.css
  locales/
    de.json
    en.json
```

Business OS has three installation classes. The ten non-removable system apps
are listed once in `src/apps/business-os/system-apps.json` and use
`install_scope: "core"`. Public optional apps use `install_scope: "store"`;
their checked-in source is only App Store inventory until installation copies
the package to `runtime/business-os/installed-modules/`. Private/customer apps
use `install_scope: "local"` and exist only below the target instance's
git-ignored `runtime/business-os/local-modules/`. There is no starter class.

For private development, place the directory under:

```text
runtime/business-os/local-modules/my-app/
```

Dropping the directory there installs the app for that CTOX instance. The
runtime discovers its manifest and schemas, exposes its static files, projects
it into the app catalog and connects its declared collections to CTOX Sync
Engine. Runtime-installed App Creator/App Store packages use the equivalent
`runtime/business-os/installed-modules/` path and lifecycle.

Validate a local app with:

```sh
node src/apps/business-os/scripts/validate-app-module.mjs my-app --local
```

## Define data once

`collections.schema.json` is the shared browser/native source of truth. A
minimal collection looks like this:

```json
{
  "schema_format": "ctox-business-os-module-collections-v1",
  "collections": {
    "my_app_records": {
      "version": 0,
      "primaryKey": "id",
      "type": "object",
      "properties": {
        "id": { "type": "string", "maxLength": 180 },
        "title": { "type": "string" },
        "status": { "type": "string" },
        "updated_at_ms": { "type": "number" }
      },
      "required": ["id", "title", "status", "updated_at_ms"],
      "indexes": ["status", "updated_at_ms"],
      "additionalProperties": true
    }
  }
}
```

Collection names must be app-scoped and must not collide with core or another
app's schema. Browser and native peers calculate the same canonical schema
hash. A divergent duplicate fails loudly instead of silently syncing
incompatible data.

## Mount the app

The shell imports `index.js` and calls `mount(ctx)`. Use only the capabilities
provided in `ctx`; do not import upstream RxDB or create a second data path.

```js
export async function mount(ctx) {
  const records = ctx.db.collection('my_app_records');
  if (!records) throw new Error('my_app_records is unavailable');

  ctx.host.innerHTML = `
    <main class="ctox-workspace">
      <section class="ctox-pane">
        <header class="ctox-pane-band">Records</header>
        <div class="ctox-pane-body" data-records></div>
      </section>
    </main>
  `;

  const list = ctx.host.querySelector('[data-records]');
  const query = records.find({
    selector: {},
    sort: [{ updated_at_ms: 'desc' }]
  });
  const subscription = query.$.subscribe((documents) => {
    list.replaceChildren(...documents.map((rxDocument) => {
      const record = rxDocument.toJSON?.() || rxDocument;
      const row = document.createElement('button');
      row.className = 'ctox-list-item';
      row.textContent = record.title;
      return row;
    }));
  });

  return () => subscription.unsubscribe();
}
```

The example intentionally uses a reactive query. `await query.exec()` returns a
snapshot; `query.$.subscribe(...)` keeps the visible UI updated when the local
user, the native peer or another authorized user changes matching data.

## Write data

Use normal collection operations:

```js
await records.insert({
  id: crypto.randomUUID(),
  title: 'First record',
  status: 'open',
  updated_at_ms: Date.now()
});
```

The successful local operation means the record is available in the local
working copy. CTOX Sync Engine journals pushable writes before confirming the
primary IndexedDB mutation, pushes them when the native peer is reachable and
retains recoverable evidence until native acknowledgement.

Application code does not call `fetch()` to persist a record and does not send
its own WebRTC frames. Realtime sync is the default data mode; permissions,
not app-side filtering, decide which users can read or write a collection.

## Multiuser semantics

All users synchronize through the authoritative CTOX instance. Business OS is
not an uncontrolled browser-to-browser database:

- the native peer validates capabilities and exact collection grants;
- unauthorized documents are not replicated to a peer;
- writes from a peer without `data.write` are rejected server-side;
- reactive queries update after authorized remote changes materialize locally;
- multi-tab coordination ensures one browser tab owns the WebRTC line while
  follower tabs share invalidation and status events.

An app may be realtime by default without being public by default. Draft apps
can remain creator-scoped; released workspace apps receive reviewed grants.

## CRUD, actions and backend capabilities

Choose the smallest execution level that preserves the invariant:

1. **Direct client CRUD** — default for forms, records, inline edits and
   single-collection changes. It is local-first and automatically synchronized.
2. **Declarative runtime action** — for authoritative
   mutations, bulk operations and multi-collection Sagas. The app package
   declares input schema, permission, idempotency and bounded effects; the
   generic native executor loads this at runtime. No per-app Rust handler.
3. **Privileged extension** — only for host files, external systems, special
   protocols or another capability that cannot be expressed safely with the
   generic runtime. It requires explicit review and a supported adapter or
   sandboxed runtime extension.

Never pass free SQL, shell commands, arbitrary file paths or executable
browser code as a supposed declarative action. The native side remains the
authorization boundary.

## External SQL projections

Private local apps may project records from SQL Server into Business OS
collections through the built-in external SQL adapter. This is a privileged
server capability, not an app-side data path:

- declarations are loaded only from operator-owned
  `runtime/business-os/local-modules/` packages;
- packaged, installed and marketplace apps cannot register executable SQL;
- credentials are resolved by name from the CTOX secret store;
- SQL is validated and parameterized by the native adapter;
- projected collections remain server-owned and do not receive legacy browser
  write grants;
- browser apps consume the resulting records through their normal
  shell-provided collection handles and WebRTC synchronization.

Declare mappings inline with `external_data_sources`, or point
`external_data_sources_file` at a JSON file inside the same local module. The
mapping path cannot be absolute or escape the module directory. Invalid
mappings fail startup grant reconciliation instead of silently widening
collection access.

Writes back to SQL Server require all of the following: `allow_writes: true`,
a declared operation with parameter bindings, an authenticated Business OS
command, and an allowed native `data.write` policy decision for the module.
CTOX persists write receipts and rejects reuse of a command id for a different
intent. Use version checks or a transactional source receipt where a remote
write must be conflict-safe and idempotent across retries.

Declare these actions in `module.json` and call them through `ctx.actions`.
There is only one compiled native command type (`ctox.app.action.run`); the
module/action name is resolved from the validated runtime registry.

```json
{
  "id": "my-app",
  "collections": ["my_app_records"],
  "data_runtime": {
    "version": 1,
    "sync": "realtime",
    "scope": "actor",
    "actions": {
      "save": {
        "version": 1,
        "input_schema": {
          "type": "object",
          "required": ["id", "title"],
          "additionalProperties": false,
          "properties": {
            "id": { "type": "string" },
            "title": { "type": "string" }
          }
        },
        "steps": [{
          "name": "save_record",
          "op": "upsert",
          "collection": "my_app_records",
          "record": {
            "id": { "$input": "id" },
            "title": { "$input": "title" },
            "actor_id": { "$actor": "id" },
            "updated_at_ms": { "$now_ms": true }
          }
        }]
      }
    }
  }
}
```

Actor-scoped collections must declare `actor_id`. Draft/local apps default to
actor scope; workspace scope requires reviewed native grants.

```js
const receipt = await ctx.actions.run('save', {
  id: crypto.randomUUID(),
  title: 'First record'
}, {
  idempotencyKey: 'save:first-record',
  until: 'terminal'
});

const current = await ctx.actions.getStatus(receipt.command_id);
const unsubscribe = ctx.actions.subscribe(receipt.command_id, (status) => {
  renderActionStatus(status);
});
// Later: unsubscribe();
```

At admission the native side snapshots the action definition and input. App
updates therefore cannot change an in-flight Saga. Insert, upsert, patch and
delete effects store their previous document state and compensate in reverse
order; a failed compensation becomes durable `manual_intervention`.

The operator can inspect and reconcile the validated runtime without changing
or rebuilding CTOX:

```bash
ctox business-os app runtime inspect my-app --json
ctox business-os app runtime reconcile my-app --dry-run
ctox business-os app runtime reconcile my-app --apply
ctox business-os app access grant my-app \
  --subject user-42 --permission data.write --collection my_app_records \
  --reason "Workspace release review"
```

`reconcile --apply` performs a supervised in-process peer reconfiguration.
Grant/revoke commands go through the native policy and audit path; they never
write access tables directly from the browser.

The app-facing command contract is deliberately narrow:

- call `ctx.commandBus.dispatch(command, { until })`; never insert or patch
  `business_commands` directly and never construct a private sync fallback;
- set `command_type` to the registered command identifier. The historical
  input key `type` is accepted only as a compatibility alias. If both are
  present they must be identical; the persisted v2 document contains the
  canonical `command_type`;
- use `until: 'local'` only when the UI explicitly continues tracking the
  returned `command_id`; use `accepted` for native acceptance and `terminal`
  only when the interaction really needs the final result;
- use `submit`, `waitForAccepted`, `waitForTerminal`, `resumeTracking`,
  `subscribe`, `getStatus` and `cancel` from the same facade for lifecycle
  handling. Apps must not poll private projection fields or rebuild the
  cancellation command themselves;
- authorization is fail-closed. If the shell cannot obtain a current native
  capability, the action reports that state instead of persisting an intent
  that can never be authorized.

Commands have three server-owned execution routes: native control handlers,
browser-runtime handlers and durable CTOX queue/harness work. An app chooses a
registered `command_type`; it does not choose or emulate the route.

## Offline, conflicts and recovery

Normal app code does not implement retry loops. CTOX Sync Engine owns:

- pending-write journal and replay;
- checkpoint persistence and reconnect catch-up;
- quota-safe cache eviction;
- whole-document/HLC ordering;
- durable structured-conflict evidence;
- recovery export/import for browser-origin loss.

Apps must still present meaningful UI for offline, permission-denied, conflict
and recovery-required states. Automatic transport handling does not make
business conflicts disappear.

## Development definition of done

A client-only app is ready when:

- the module validator passes;
- every manifest collection exists in `collections.schema.json`;
- the app uses only shell-provided DB, sync, storage and command facades;
- direct CRUD works offline and synchronizes after reconnect;
- two authorized browser profiles observe a remote change reactively;
- an unauthorized profile cannot read or write the collection;
- reload and multi-tab leader handover preserve data;
- no `src/core/` edit, generated core fixture or Rust rebuild is required.

The platform-level release gate additionally installs an app with a previously
unknown schema and action into an already-running release binary and proves
this complete flow without a developer-triggered daemon restart.

## Related references

- `docs/business-os-module-context.md` — full `mount(ctx)` contract
- `src/apps/business-os/README.md` — module layout and validator rules
- `docs/ctox-rxdb.md` — sync architecture and data boundary
- `docs/business-os-app-platform-refactoring-plan.md` — client-only SDK work
- `docs/business-os-dynamic-module-schemas-plan.md` — runtime schema loader
