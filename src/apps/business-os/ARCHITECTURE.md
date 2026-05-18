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
4. HTTP pull/push bridge only for local development and diagnostics.

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
  "signaling_urls": ["wss://ctox-signaling.metricspace-ai.workers.dev/signal"],
  "transport": "webrtc",
  "app_hosting": "ctox_instance_webserver",
  "ctox_instance_required": true
}
```

The Business OS web app opens RxDB locally, joins the P2P room through the
signaling server, and communicates with the CTOX instance peer without requiring
that instance to expose a public inbound address.

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
