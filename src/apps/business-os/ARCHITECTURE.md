# CTOX Business OS Native Architecture

The native Business OS is a CTOX surface, not a separate SaaS stack.

## Runtime Shape

```text
CTOX App
  Rust native core
  Local Business OS webserver
  Browser/WebView host
  SQLite authoritative state
  Command validation
  Agent orchestration
  P2P sync peer

CTOX Business OS Web App
  Served from the active CTOX instance
  Vendored ESM runtime
  RxDB local-first data
  P2P sync peer
  Browser/WebView client surface
```

The Business OS app must not require the CTOX instance to expose a public
inbound Internet IP. HTTP is allowed for local development, static serving, and
diagnostics, but it is not the primary communication model for remote clients.

The default local product shape is **CTOX instance webserver + Business OS Web
App**, with CTOX Desktop acting only as the connector/forwarder to the selected
instance. A separate Electron wrapper is not the default runtime.

## Sync Priority

1. Peer-to-peer RxDB/WebRTC sync through a signaling server between Business OS
   clients, the CTOX desktop app, and the CTOX instance peer.
2. CTOX instance serves the app shell and owns command validation, session
   validation, and authoritative writes.
3. CTOX Desktop forwards or opens the selected instance URL after the user is
   connected/authenticated.

There is no HTTP pull/push or command bridge for Business OS module data. If
the native Rust RxDB peer is not complete, the app must show that sync is not
ready instead of loading fallback data through HTTP.

This keeps the app useful when:

- Multiple clients attach to the same CTOX-managed business instance.
- CTOX Desktop connects to a local or remote CTOX instance and opens/forwards
  the instance-owned Business OS URL.
- The CTOX instance is behind NAT, a residential connection, a firewall, or a
  private network without public inbound ports.
- Local work continues while a CTOX core process restarts.

## Instance Model

Each CTOX-managed business instance has:

- `instance_id`: stable identifier for the business workspace.
- `peer_id`: runtime identifier for a client, desktop app, or CTOX instance
  peer.
- `peer_role`: one of `business_os_client`, `ctox_desktop_app`, or
  `ctox_instance`.
- `sync_room`: deterministic P2P room name for the instance.
- `signaling_urls`: one or more signaling endpoints.
- `collections`: RxDB collections replicated for the active modules.

The CTOX Rust core remains authoritative for commands and hard domain
invariants. Business OS clients hold local RxDB state and exchange it over P2P,
including queued commands. The CTOX instance peer consumes those command
documents, validates them, writes authoritative state, and republishes accepted
projections over the same P2P room.

## JSON-Native Records

Business modules define their master data as JSON. The same definition links
the parser prompt, canonical JSON schema, RxDB storage contract, and display
DSL. RxDB stores the canonical document in `business_records.data`; module
specific table-shaped fields are projections, not the source of truth.

Generic replicated collections:

- `business_definitions`: module/entity definitions, prompts, JSON schemas,
  display DSL, and storage rules.
- `business_records`: actual master data records with canonical `data` JSON,
  source references, links, display cache, and small derived index fields.

The derived fields `index_text`, `sort_key`, `status_key`, and `score_key` exist
only to keep local search, sorting, filters, and sync lightweight. CTOX can
rebuild them from `data` and the definition at any time.

## UI Contract

Every module uses the same spatial model:

- left pane: source context, filters, queues, scopes
- center pane: primary workbench and selected records
- right pane: topics, inspectors, agent context
- left drawer: module navigation and setup
- bottom drawer: selected center items
- right drawer: focused right-column topics

React is optional and embedded for menus, settings, and complex forms. Working
views remain direct HTML, JavaScript, and CSS so CTOX agents can patch them
without build tooling.

The Matching module is the first concrete blueprint for this
contract. Its initial example is the ported NinjaWorkflowTool Matching view. It
keeps Business Basic colors (`--km-*` tokens) while preserving the original
Matching interaction pattern: companies left, jobs center, candidates right,
and directional drawers for job, candidate, and match detail work.

## Local Hosting

The CTOX instance is the only default host for `business-os/`.

It starts a server bound to the configured local interface, serves the static
app files, validates the user session before the web app is allowed to
initialize, and passes launch/sync metadata to the browser/WebView:

```json
{
  "instance_id": "biz_...",
  "peer_id": "client_...",
  "peer_role": "business_os_client",
  "sync_room": "ctox-business-os:biz_...",
  "signaling_urls": ["wss://signaling.ctox.dev"],
  "transport": "webrtc",
  "app_hosting": "ctox_instance_webserver",
  "ctox_instance_required": true
}
```

The Business OS web app opens RxDB locally, joins the P2P room through the
signaling server, and communicates with the CTOX instance peer without requiring
that instance to expose a public inbound address.

The concrete sync contract is documented in `RXDB_SYNC_CONTRACT.md`. That
contract is intentionally strict: browser data lives in RxDB/Dexie, CTOX data
lives in RxDB/SQLite, and both sides exchange records through RxDB WebRTC
replication only.

The Desktop app must not own this webserver. It may open the URL for a local
instance, or forward an authenticated remote session through WebRTC/signaling,
but the served app files and API contract remain owned by the CTOX instance so
CTOX can update the Business OS in place and all connected users see the same
instance version.

## Optional Electron Wrapper

An Electron wrapper may still be useful later for distribution scenarios where a
client should run Business OS without the CTOX Desktop app. It must remain a
thin optional adapter around the same static `business-os/` files and must not
become the primary architecture.

## Desktop Module

The `modules/desktop/` module is the Business OS home screen: icon
surface, wallpaper, drag-drop, and a launcher that resolves icon clicks to
either a CTOX-module tab switch (heavyweight modules like Documents,
Knowledge) or a shell-level desktop-app window (lightweight tools under
`src/apps/business-os/desktop-apps/` such as the file viewer or notes
editor).

Cross-cutting OS-style infrastructure — window manager, notifications,
event bus, context menu — lives at the shell level under
`src/apps/business-os/shared/` so any Business OS module can consume it:

- `shared/window-manager.js`
- `shared/notifications.js`
- `shared/event-bus.js`
- `shared/context-menu.js`

Desktop state persists through RxDB collections (`desktop_icons`,
`desktop_layout`, `desktop_notifications`); no code under `modules/desktop/`
or `shared/` reads or writes IndexedDB directly. Live events surface from
the `business_commands` stream into the notifications layer — the desktop
is the visible "CTOX is working" surface. The desktop module runs at
`module.json: { "shell": "full-workspace" }` so it takes over the whole
pane area rather than sitting inside the 3-pane module shell.

The OS chrome is switchable Windows-style vs macOS-style at the **shell**
level via `[data-shell-style="windows" | "macos"]` on `<body>` — not on
the desktop module root. The desktop module follows that attribute; it
does not own its own taskbar, dock, or menubar (those are the shell
topbar). All colors and dimensions resolve through the shell tokens
defined in `src/apps/business-os/app.css` (`--bg`, `--surface`,
`--surface-2`, `--line`, `--text`, `--muted`, `--accent`, `--accent-soft`,
`--danger`, `--panel-radius`, `--control-radius`, `--panel-shadow`,
`--shadow`, `--shell-*`); neither the module nor the shared helpers
introduce their own theme variables.

## Right-Click → Agent Context

Right-click anywhere inside a module opens a shell-owned "Chat to CTOX"
popover that hands the agent a structured context for **where** the click
happened: `module`, `column` (left/center/right), `record_type`,
`record_id`, `label`, `deep_link`, plus `selected_text` and `clicked_text`.
The handler lives in `app.js` (`handleGlobalContextMenu` →
`extractGlobalCtoxContext`); it is a capture-phase listener on `document`,
so it pre-empts any module-local `contextmenu` handler. It is active for
every full-workspace module and skips `input`/`textarea`/`select`/
`[contenteditable]`/`.monaco-editor`/`.no-ctox-context` targets (those keep
the native menu). A module can suppress the shell menu and run its own by
marking its host with `data-ctox-local-context-menu` (e.g. `app-store`); the
pane-mode reference apps `documents`/`spreadsheets` ship an equivalent
in-module menu because the shell menu only binds to full-workspace modules.

For the agent to know **which record** was clicked, the clicked element (or
an ancestor) must expose an id. `detectRecordFromElement` resolves, in order:

1. `data-context-record-id` (+ `data-context-record-type`, `data-context-label`)
   — the **canonical, preferred hook**: it pins a clean type and human label.
   Put this trio on the outermost element of each record row/card/tree-node;
   the shell walks ancestors, so child buttons inside the row need nothing.
2. Any `data-*-id` attribute (e.g. `data-id`, `data-customer-id`,
   `data-shift-id`) — recognized generically, with the record type derived
   from the attribute name. This means a module's own domain id attributes
   already work without the explicit hook; add the hook only when you want a
   precise type/label or the record element carries no id at all.

`detectColumnFromElement` reports left/right only when a pane ancestor
matches a `*-left`/`*-right`/`*-sidebar` class or carries
`data-left-content`/`data-right-content`; mark a pane root with those
attributes if its class names do not encode the column. New modules should
satisfy at least rule 2 on their record elements so the agent never loses
the click location.
