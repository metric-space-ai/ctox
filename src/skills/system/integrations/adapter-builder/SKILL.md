---
name: adapter-builder
description: Add a connector for a new external service at runtime without recompiling CTOX. Research the official API docs, write a declarative adapter spec (ctox.adapter-spec/v1), validate it, prove it with a real live call, and activate it autonomously (no user approval; a fresh passing live test is the gate). Use when an agent needs a service that `toolhub catalog` does not list.
class: system
state: draft
cluster: integrations
---

# Adapter Builder

Living document. Update the "Stand / Status" and "Lessons" sections whenever the
flow, tooling or rules change. Lifecycle of this skill: see `skill_meta/skill-lifecycle`.

## Wann / When

- A task needs a service that is not in the toolhub catalog, or an existing spec
  needs a new action or a fix after the provider changed its API.
- Not for: services that need multipart upload, request signing (HMAC, AWS SigV4),
  binary downloads, multi-step flows, or a new OAuth provider. Those need a
  compiled adapter or a `subscription_auth` provider in a CTOX release; say so
  and stop.

## Rollen / Roles (hard rules)

- Adapter setup is **autonomous**: no user approval to install or activate.
  The gate is technical: validate passes and a live test passed within 24 h
  for exactly this spec (sha256). Supervisor activates any time; a persistent
  worker only inside a builder mission (`--mission <work_id>`); one-shot
  workers and external MCP never.
- Only the **supervisor** talks to the user (credential card, and per-call
  approval of destructive actions). Persistent workers hand such requests to
  the supervisor and wait. One-shot
  workers never interact: they finish with `needs_user` and a short reason.
- Only the CTOX core instance makes outbound HTTPS calls. Workjet and the
  browser talk to the instance only through ctox sync.
- No new env-var switches, no background threads, no own OAuth code, no HTTP
  endpoints. Secrets only through the instance secret store (`ctox secret`).

## Ablauf / Flow

1. **Research.** Use `research/systematic-research` with the official API
   reference as primary source (OpenAPI file if published). Record: base URL,
   hosts actually used, auth scheme (bearer / API key header / basic / OAuth2),
   per-account host parts (subdomain, region), pagination, error format, rate
   limits. Activepieces pieces (MIT) are a good cross-check for action sets.
2. **Pick a small action set.** Start with the reads the task needs, plus at
   most the writes the task really requires. Every action gets a risk level:
   `read` (GET or query-only POST with `read_only_post`), `write`, or
   `destructive` (every DELETE, and anything that cannot be undone).
3. **Write the spec** (`src/core/execution/toolhub/schema/adapter-spec.v1.schema.json`,
   examples `src/core/execution/toolhub/specs/todoist.json`, `asana.json`):
   - `allowed_hosts` as narrow as possible; `*.domain` only for per-tenant hosts.
   - `api_base_template` + `connection_fields` for per-account hosts (only single
     DNS labels are substituted).
   - Titles and descriptions in DE and EN, plain words, no jargon.
   - Never put tokens, keys, passwords or example credentials into the spec.
   - Every input property must be sent somewhere; declare constraints
     (`at_most_one`, `at_least_one`) and `nonempty_body` messages in plain words.
4. **Validate.** The spec must pass `SpecAdapter::from_json` (strict validator:
   https only, allowed hosts, no IP/local hosts, secret scan, risk vs. method,
   header allowlist, unknown fields rejected). Fix every error; do not weaken
   the spec to get around a check.
5. **Install as draft.** `SpecStore::for_instance_root(<instance root>).install(text)`.
   Versions are immutable: any change needs a new `version`.
6. **Credential.** If the instance has no connection for the service, the
   supervisor asks the user with the credential card (token or API key, plus
   connection fields). OAuth only for providers that `subscription_auth`
   already supports; otherwise ask for a personal token or stop.
7. **Live test** (definition of done): real calls against the real API through
   the hub with the installed spec. Reads first. Writes only with explicit user
   consent, only in a test object, and clean up afterwards. Record per action:
   PASS / FAIL / SKIP with reason. An action without a passing live call is not
   done; remove it from the spec or mark the spec as not ready.
8. **Activate** yourself, right after a passing live test:
   `ctox_toolhub_activate <slug> <version>` (worker: with `mission`). No user
   step. The hub picks it up via `reload_specs_if_changed` without restart.
   Report to the supervisor what was activated (service, hosts, actions by risk).
9. **Rollback.** After 5 consecutive failed calls (mapping errors, 5xx, 400/410)
   the active version rolls back automatically to the previous one (once, no
   ping-pong; see `history.jsonl` event `auto_rollback`). Otherwise the
   supervisor uses `ctox_toolhub_rollback` or `deactivate`. Report what happened.

## Werkzeuge / Tools (ctox toolhub, since 2026-10-16)

Harness tools `ctox_toolhub_*` (config `tools.ctox_toolhub`) and the CLI
`ctox toolhub --as <role> ...` are the same thing. The role is set by the
session, never by you. Rules per role: `ctox toolhub --as <role> rules`.

| Step | Tool / CLI | Who |
|---|---|---|
| Find existing | `ctox_toolhub_search` (`include_catalog=true`) | all |
| Queue a builder mission | `ctox_toolhub_request_adapter` / `request-adapter <service>` | supervisor, worker |
| Draft | `ctox_toolhub_draft_spec` (`spec_json` or `from_catalog`) | all except MCP |
| Validate | `ctox_toolhub_validate <slug> <version>` | all except MCP |
| Live test + review proof | `ctox_toolhub_live_test` with `probes_json` `[{tool,args,cleanup[]}]` | reads: all; write probes: supervisor; destructive: never |
| Activate (no user approval) | `ctox_toolhub_activate <slug> <version> [mission]` | supervisor; worker only with its builder mission |
| Deactivate / rollback | `ctox_toolhub_deactivate`, `ctox_toolhub_rollback` | supervisor (plus automatic rollback on repeated failures) |
| Per-call approval (destructive only) | `ctox_toolhub_approve gate decision=approve|reject user_quote="<user's words>"` | supervisor only, after asking the user |
| Use | `ctox_toolhub_execute` (destructive: first `approval_required` + gate, then again with `approval_gate`) | per role rules |

Hard checks in code: activation needs a validating spec and a passing live-test
proof younger than 24 h bound to the spec sha256; any change after the test
blocks activation. The egress validator (https only, allowed hosts, public IPs)
always applies. Destructive calls still need a single-use per-call gate. Every step lands in
`runtime/toolhub/adapter-specs/<slug>/history.jsonl` (argument hashes only).

## Stand / Status (2026-10-15)

- Done in the toolhub crate (draft PR, stacked on #776): spec format v1, validator,
  `SpecAdapter`, per-instance `SpecStore` (versions, active pointer, history,
  rollback), hub hot reload, restricted transport (no redirects, public IPs only).
  Todoist and Asana run as specs with the same live results as the compiled adapters.
- **Calendly (2026-10-15): first spec-only build** (`specs/calendly.json`, 17 actions,
  `tests/live_calendly.rs`), no Rust adapter code. 14 actions live-passed against
  api.calendly.com; get/list invitees, get event and cancel event are list-only
  covered (free account has no bookings). Needed one minimal format extension:
  `uri_prefix` (args) and `uri_tail` (path encoding).
- **Brand-new services (2026-10-15):** a spec for a service with no compiled
  adapter loads purely through `SpecStore` install → activate →
  `reload_specs_if_changed` on a running hub (offline test + live test
  `live_hotplug_public` against `specs/zippopotam.json`). Auth scheme `none` exists
  for public APIs.
- **Stripe (2026-10-15):** hot-plug only (`specs/stripe.json`, 21 actions). All
  live-passed on a `sk_test_` key; leftovers cleaned up. Needed two format
  extensions: nested form flattening and a `-` pointer segment for cursor pages.
- **Airtable (2026-10-15): converted Rust -> spec** (`specs/airtable.json`, 22 actions).
  Parity: identical on all 44 recorded port cases (`support::spec_parity`) and 244
  differential cases. Live: 15 PASS via spec, 3 scope-missing (comments; same with
  Rust), 5 schema writes skipped (not reversible). 17/22 actions came straight from
  the generator; 5 needed hand edits plus format extensions (see Lessons).
- **GitLab (2026-10-15): converted Rust -> spec** (`specs/gitlab.json`, 30 actions).
  Parity: all recorded port cases identical, 0 of 723 differential cases differ
  (incl. adversarial values). Live: 30/30 distinct actions PASS via spec and via
  Rust (issue, note, MR, label created in the test project and cleaned up).
  13/30 actions straight from the generator; 8 write/list tools hand-mapped.
- **HubSpot (2026-10-15): converted Rust -> hybrid spec** (`specs/hubspot.json`,
  29 spec actions + 3 Rust actions via `hybrid`). Parity: all recorded port cases
  identical, 0 of 650 differential cases differ. Live (read-only key): 18 read
  actions PASS via spec and via Rust; FIND_TICKET scope-missing, GET_DEAL /
  GET_TICKET skipped (no record / scope). Writes not live-tested (key is read-only).
  11/32 straight from the generator.
- **Monday (2026-10-15): converted Rust -> hybrid spec** (`specs/monday.json`,
  16 spec + 14 Rust actions). Parity: all recorded port cases identical, 0 of 523
  differential cases differ. Live (`tests/live_monday.rs`, only board 5106123210
  "Toolhub Test", guarded by an assert): 28/28 distinct actions PASS via spec and
  via Rust; a throwaway group, text column and item are created and deleted.
  Not run live: CREATE_SUBITEM (leaves a permanent hidden subitems board),
  SEND_NOTIFICATION (notifies a person). 16/30 requests straight from the generator.
- **GitHub (2026-10-15): converted Rust -> hybrid spec** (`specs/github.json`,
  20 spec + 10 Rust actions). Parity: all recorded port cases identical, 0 of 642
  differential cases differ. Live (`tests/live_github.rs`, read-only against
  metric-space-ai/ctox, the only repo the earlier Rust live test used): 15 read
  actions + the 404 error path PASS via spec and via Rust. No writes live-tested:
  there is no GitHub test repo, and the real repo is not a scratch target.
  26/30 requests straight from the generator.
- **Notion (2026-10-15): converted Rust -> hybrid spec** (`specs/notion.json`,
  7 spec + 16 Rust actions). Parity: all recorded port cases identical, 0 of 396
  differential cases differ. Live (`tests/live_notion.rs`, only the shared test
  page 3f94d550-…-7dd9): 7 reads + the 404 error path PASS via spec and via Rust
  (spec part live: GET_PAGE, LIST_USERS, GET_USER). Writes skip: the test
  integration lacks "insert content" (403 restricted_resource), so nothing was
  created; the test writes only a child page that it archives. No shared
  database, so QUERY_DATABASE / UPDATE_DATABASE_ITEM are parity-only.
  15/23 requests straight from the generator. No format change needed.
- **Trello (2026-10-15):** hot-plug only (`specs/trello.json`, 22 actions). All
  live-passed; a throwaway board is created and deleted. Needed auth scheme
  `oauth_header` (key+token in the Authorization header). Webhooks need a
  URL that answers HEAD 200 (https://example.com/ works; a path that 404s does not).
- **Wired (2026-10-16, branch feat/toolhub-adapter-builder):** `ctox toolhub` CLI,
  harness tools `ctox_toolhub_*`, read-only MCP tools `business_os.toolhub_*`,
  builder mission `request-adapter`. Since 2026-10-17 (user decision) adapter
  setup is autonomous: no approval gate, no email nag for adapters; auto-rollback
  after repeated failures. Older note:
- **Not wired yet (before 2026-10-16):** the toolhub was not part of the `ctox` binary on main, so there
  was no `ctox toolhub spec …` command, no MCP tool and no activation card yet.
  Until then steps 4 to 9 run only in tests. The user-facing parts (cards,
  catalog view) need a sync collection with a profile and typed commands, which
  is ctox sync v3 stage S2 work (label `sync-v3:S2` from the sync owner).

- **Dogfood (2026-10-17): first fully autonomous builds** with only the built
  `ctox toolhub` CLI as worker on a local instance root: Open-Meteo
  (`specs/open_meteo.json`, 2 actions), Frankfurter/ECB (`specs/frankfurter.json`,
  4 actions), Nager.Date (`specs/nager_date.json`, 5 actions). Flow per service:
  `request-adapter` -> spec written by hand from the public docs -> `draft-spec
  --spec-file` -> `validate` -> `live-test --probes-json` (all actions, reads) ->
  `activate --mission <work_id>` -> `execute`. 11/11 actions live PASS; no
  approval gate was created. It needed three format fixes, one builder fix and
  one instance-config step (see Lessons "Dogfood").

## Lessons

### Dogfood 2026-10-17 (Open-Meteo, Frankfurter, Nager.Date)

- **There is no docs -> spec generator.** `spec_from_builtin.rs` only converts
  Rust adapters. For a new service you write the JSON yourself from the docs;
  check every endpoint with one real request first (curl or a read probe) and
  copy the shapes, not the docs prose.
- **`number` means decimals.** Coordinates, amounts: use `"type": "number"`
  (since 2026-10-17 it keeps decimals; before, it was an alias of `integer` and
  rejected 52.52). Use `integer` only where the API wants whole numbers.
- **Path arguments are strings or whole numbers.** Year or id in the path:
  declare it as `string` (or `integer`); `segment` encoding sends 2026 as `2026`.
- **`..` inside a segment is fine, as a segment never.** Frankfurter ranges
  (`/v1/{start_date}..{end_date}`) validate since 2026-10-17; `/a/../b` and `/./`
  are still refused.
- **Empty 2xx answers:** when the answer is only the status code (Nager.Date
  `IsTodayPublicHoliday`: 200 = yes, 204 = no), use
  `{"mode": "status", "values": {"200": {...}, "204": {...}}}`. In `body` mode both
  look like `{"text": ""}` and the live test passes without telling you anything:
  look at what a probe returns, not only at PASS.
- **No redirects.** If the documented host answers 301 (api.frankfurter.app ->
  api.frankfurter.dev/v1), point `api_base` and `allowed_hosts` at the final host.
  `curl -sI` the base once before writing the spec.
- **One host per spec.** Open-Meteo geocoding and archive live on other hosts;
  each needs its own spec (and its own builder mission). Keep the first spec to
  the host the task needs.
- **Mission name vs. slug.** `request-adapter` stores `nager-date`; toolkit slugs
  use `nager_date`. Activation treats `-` and `_` the same (since 2026-10-17).
- **Fake-IP boxes:** on a box whose DNS answers 198.18.0.0/15, every probe fails
  with "resolves only to private or reserved addresses" until the instance config
  key `CTOX_TOOLHUB_FAKE_IP_EGRESS=1` is set in `runtime/ctox-runtime.sqlite3`.
  There is no CLI command for it; that is an operator step, not a builder step.
- **Transient TLS errors** (box proxy) fail a probe; the live test has no retry.
  Run the live test again; a fresh passing run replaces the proof.
- **Close the mission yourself.** Activation does not close the builder mission;
  the work item stays `open` until the worker closes it with its report.

### Earlier

- Fake-IP DNS egress (box resolves to 198.18.0.0/15): live tests need
  `TransportOptions { fake_ip_egress: true }`; production instances keep the default.
- **Build time (Calendly, spec-only):** about 45 minutes from token to green live
  test, including the format extension. Most of it was reading the API shape;
  writing the spec via a small Python generator (helpers for paging, URI args,
  response modes) was faster and less error-prone than hand-written JSON.
- **Resource-URI APIs (Calendly, also Stripe-like "self" links):** references are
  full URIs (`https://api.calendly.com/users/<uuid>`) in query and body, but UUIDs in
  paths. Use `"uri_prefix": "https://api.calendly.com/users/"` on args (bare id gets
  expanded; a URI for another resource or host is refused) and
  `"path_encoding": {"x": "uri_tail"}` for path params (takes the last segment).
  Agents can then pass whatever the previous call returned.
- **Pagination style Calendly:** `count` (1–100) + `page_token`; next token at
  `/pagination/next_page_token`, items at `/collection`. Map with
  `response.mode = object` fields `collection` + `next_page_token`. Single reads
  return `{"resource": …}` → `mode: pick, pointer: /resource`.
- **Owner filters:** list endpoints want `user` or `organization`; express this as an
  `at_least_one` constraint, not as required fields. Webhook list/create need
  `organization` + `scope` (and `user` when scope=user) even on a single-user account.
- **Webhooks on a free account work** (create/get/delete) with a harmless
  `https://example.com/…` target; always delete in the same test and verify the
  list is empty afterwards.
- **DELETE returning 204:** use `response.mode = const` (`{"deleted": true}`).
- **Empty test accounts:** seed what you can safely create (scheduling link,
  webhook); actions that need real bookings or other people (cancel event,
  invitees) stay list-only covered. Never book a slot with an invented invitee.
- **Live harness:** `support::use_spec(include_str!("../specs/<slug>.json"))` works
  for spec-only toolkits too as long as `replaces_builtin` matches an existing
  compiled slug; a brand-new slug needs a harness path without that assert.
- **Auth `none` (public APIs):** `"auth_schemes": [{"type": "none"}]`, and it must be the
  only scheme. The hub then executes without an auth config, account or secret, but only
  for toolkits with an egress policy (specs), always over the restricted transport
  (allowed hosts, https, public IPs, no redirects). Credential-based toolkits still
  answer "credential required" without an account. Use it only for truly public,
  read-only data; never to call an authenticated API "anonymously".
- **Brand-new vs replacing:** `replaces_builtin` is only for slugs that already have a
  compiled adapter; a new slug simply appears in `SpecReload.loaded`. The live
  harness now checks exactly that (`replaced_builtins` contains the slug iff a builtin
  exists), so `support::use_spec` works for new token services too.
- **Hot-reload lifecycle to test for every new service:** not served after install, only
  after activate; `reload_specs_if_changed` returns `None` when nothing changed; after
  deactivate the tools disappear without restart.
- **Path params with spaces** (city names): `path_encoding: component` gives `%20`;
  the default `segment` rejects them. Path traversal (`../x`) is refused either way.
- **Live tests without a token** need an explicit opt-in (`CTOX_TOOLHUB_LIVE_PUBLIC=1`),
  so offline CI never reaches the network.
- **Form bodies with nested keys (Stripe):** objects and arrays are flattened to
  bracketed keys (`metadata[order]=42`, `line_items[0][price]=price_x`,
  `address[city]=Berlin`); nulls are dropped; depth is capped at 5; keys with
  `[`/`]` or control characters are refused. Spec authors just put an object/array
  arg into `body.form` (type `object` or `raw`); the executor does the rest. No
  new JSON Schema field was needed.
- **Cursor pagination (Stripe `starting_after` / `has_more`):** response mode
  `object` with a pointer that ends in `/-/` selects the LAST array element
  (RFC 6901's `-` is write-only and never resolves on reads, so we reuse it).
  Map e.g. `"next_starting_after": {"pointer": "/data/-/id"}` together with
  `has_more` and `data`. Empty pages leave the field null.
- **Stripe live-test guard:** refuse to run unless the key starts with
  `sk_test_`, and re-check before every single call. Never point a live harness
  at a live (sk_live_) key. Clean up: refund the payment, delete draft invoices
  and the customer, deactivate payment links and archive prices/products
  (products with prices cannot be deleted).
- **Stripe payment methods:** `payment_method_types` was removed by Stripe; send
  `automatic_payment_methods[enabled]=true&automatic_payment_methods[allow_redirects]=never`
  instead, so a confirmed PaymentIntent with `pm_card_visa` works without a browser.
- **Search endpoints can lag:** Stripe Search is eventually consistent; assert only
  that the call succeeds, not that the just-created object is already findable.
- **Build time (Stripe):** about 35 minutes from token to a green live run,
  including the two format extensions and the leftover cleanup of a failed first
  attempt. The nested-form generator (one recursive flatten) was the bulk of the
  new code; the spec itself was generated with Python helpers again.
- **Auth `oauth_header` (Trello):** `"auth_schemes": [{"type": "oauth_header"}]`.
  Credential is stored as `key:token`. The hub sends
  `Authorization: OAuth oauth_consumer_key="{key}", oauth_token="{token}"`, so
  neither secret appears in the URL (Trello also accepts `?key=&token=` query
  params; we refuse that path). Quotes inside key/token are rejected. Live
  harness: set both `CTOX_TOOLHUB_LIVE_<SLUG>_KEY` and `_TOKEN`; `live_exec`
  joins them as `key:token` when the KEY env is present.
- **Trello webhooks require a live HEAD 200** on the callback URL before create
  succeeds (`VALIDATOR_URL_RETURNED_ERROR` otherwise). `https://example.com/`
  answers 200; a path under it that 404s does not. Prefer a real reachable
  HTTPS endpoint; if none is available, skip create/delete and say so.
- **Board lifecycle for live tests:** create a dedicated board (no default lists),
  do all writes on it, close it, then delete it. Closing alone is not enough —
  closed boards still count against the free quota. Prefer `defaultLists: false`
  so you control the lists you create.
- **List response for top-level arrays:** Trello returns a bare JSON array; use
  `response.mode = list` with `"pointer": ""` (the validator already allows an
  empty pointer) and `as: boards|lists|cards`.
- **Build time (Trello):** about 30 minutes from credentials to a green live run,
  including the auth-scheme extension and one webhook retry (wrong callback
  path). Auth was the only format change; the rest fitted existing JSON bodies
  and path params.

### Converting a compiled adapter to a spec (Rust -> hot-plug)

1. Draft: `cargo run --example spec_from_builtin -- <slug> > /tmp/draft.json 2>/tmp/todo.txt`.
   The generator *probes* the compiled adapter (no source parsing for requests):
   marked sample args -> method, path params (`{arg}`; `component` if spaces pass),
   query/body args, required flags, defaults (from a required-only probe),
   `$omit_empty`, `nonempty_body`; responses by *marker probing* (every key the
   adapter source reads gets a unique marker; output shows the source pointer,
   `each` projections and `count`). `/tmp/todo.txt` lists what it could not infer.
2. Check: `CTOX_SPEC_DRAFT=<slug>:<path> cargo test --test spec_adapters differential_report -- --ignored --nocapture`
   (partial drafts allowed), then put the file at `specs/<slug>.json` and run
   `cargo test --test port_<slug>`: `support::spec_parity` replays every recorded
   port case against the spec. **Recorded cases are the real parity check** —
   the synthetic differential missed every response projection on Airtable.
3. Hand-fix only the reported tools; keep the fixes in a small script (re-runnable
   after regenerating). 4. Live via `support::use_spec`, same scenario as Rust.

Why it was fast/slow (Airtable): fast = plain REST CRUD with path ids and JSON
bodies (generator got 17/22 right). Slow = per-tool Rust logic: formula strings
built from args with escaping, arrays sent as `fields[0]`/`sort[0][field]`
query keys, `""` -> `null` rewrites, field projections (`createdTime` ->
`created_time`, nested tables/fields/views). Each became one small, reusable
format feature instead of per-service code.

Format features added for conversions (all tested in `spec_adapters.rs`):
- `{"template": "FIND(\"{v}\",{{{f}}})", "escape": {"v": "backslash_quote"}, "forbid": {"f": "{}"}}`
- `{"switch": "match_mode", "default": "contains", "cases": {...}}` (unknown case = error)
- arg options `max_items`, `items_segment`, `wrap_object`, `require_key`, `blank_to_null`
- query values that are objects/arrays use bracketed keys (like form bodies)
- response object fields: `each` (item projection, nested), `count`, `"default": null`
  keeps a missing projected list null
- body objects: `"$omit_empty": true`

Why it was fast/slow (GitLab): fast = the 22 read tools (generator + one
re-runnable fix script). Slow = things the synthetic samples never exercise:
`group/project` ids (`%2F`), `@user` stripping, tri-state `"true"/"false"`
dropdowns sent as booleans, string id lists sent as integers, `clear_*` flags
overriding a value, a cross-arg rule (`membership=false` needs `search` or
`owned=true`), top-level array responses. Port cases caught the first two; the
generator now probes them. **Lesson: the differential now also feeds
adversarial strings (`not-a-date`, `@handle`, `grp/sub`, `a b`) into every free
string arg** — that immediately exposed id-charset gaps in the already committed
Airtable and Todoist specs (fixed with `name` encoding / `segment: true`).

Added for GitLab (tested in `gitlab_format_features`):
- request `paths: [{when, path}, …]`; integer path params are sent as decimals
- path encodings `namespaced` (`a/b` -> `a%2Fb`, `{"namespaced": ": "}` allows
  extra chars), `name` (component, but refuses `/ \ ? # %`), `{"strip_prefix": "@"}`
- arg type `tristate`; arg options `items_integer`, `segment`, `strip_prefix`
- formats `date-prefix` (date or date-time), `color`
- switch `else` (fallback value; boolean args switch on `"true"`/`"false"`):
  `{"switch": "clear_x", "cases": {"true": {"const": ""}}, "else": {"arg": "x"}}`
- constraints: args may be `{"arg", "equals"}`; `when: {"arg", "equals"}` makes
  the rule conditional
- response field pointer `""` = the whole document (top-level arrays)
- `nonempty_body` also fires when `$omit_empty` removed every field

Generator now infers: `strip_prefix` (path + query), `namespaced` path ids,
`date-prefix` formats on query args, integer id-list samples (`*_ids`), hex
colour samples. Ratchet: gitlab 13/30.

### When a hybrid (spec + Rust) is the right call

Keep an action in Rust (`"hybrid": {"rust_actions": [...], "reason": "..."}`)
when its logic is **runtime behaviour, not request shape**:
- values computed at call time (HubSpot note/task default `hs_timestamp` = now;
  a spec has no clock and should not get one),
- fan-out: one request item built per element of an input list with shared
  fields (HubSpot CREATE_ASSOCIATIONS),
- multi-request flows, signing, pagination loops, binary/markdown conversion.
Add a format feature instead when it is a *shape* that recurs across APIs:
JSON arrays in bodies, conditional objects (`$if`), merging a free-form
property map (`$merge`), CRM "all values are strings" (`stringify_values`),
"always request these fields" (`prepend`). Rule of thumb: if the feature needs
a loop, a clock, or per-service constants tables, it is Rust.
Hybrid specs only load where the binary ships the compiled adapter of that slug
(validated at load: unknown tool, duplicate spec+Rust action, missing reason
all fail). The differential/port parity still cover all tools.

Why it was fast/slow (HubSpot): fast = reads, after fixing one generator bug
(whole items copied through were mistaken for a projection of every probed
key; now detected as pass-through). Slow = the Find/Get family (default
property lists + user extras, a second filter only when both its args are set)
and property maps (arrays joined with `;`, numbers stringified, explicit field
vs. map precedence differs between upsert and deal). Port cases + adversarial
values were again what proved parity.

Monday (GraphQL) lessons: the request side was easy (documents + variables map
1:1 onto `graphql` bodies); everything hard was response *reshaping*. Split by
response, not by endpoint: plain projections (item summaries, users-like lists,
mutations returning ids) went to the spec; computed strings (comma-joined
member/owner lists, `kind:tier`, re-serialised settings JSON), per-type value
parsing, "first of several documents", dynamic result keys and injected parent
ids stayed Rust. Added only shapes that recur in GraphQL APIs:
- `error_check` kind `graphql` (`errors[].message` joined by `; `, "unknown" when
  none; optional `require_data` / `no_data`). Several checks run in order, so a
  provider-specific `error_message` field slots in between.
- object response `require: {pointer, message}` (GraphQL "no `me`" / "no board")
- `each` fields `{"pointer", "default"}` (bool-or-false flags)
- arg option `json_string` (GraphQL `JSON` scalars take JSON *text*) and template
  escape `json` (a JSON value built from a string arg, e.g. `{"name": …}`)
The generator still misses id checks on GraphQL variables (`segment: true`) and
required flags on them; adversarial differential cases found every one.

GitHub (REST) lessons: requests were nearly free (26/30 straight from the
generator); the hybrid line again ran along *response computation*: base64
file decoding, 404 -> `found: false`, `total_count` -> `found`, derived
`is_pull_request` / joined assignee and label strings, and the gist `files` map
turned into a list stay Rust. Added only:
- path encoding / template escape `segments` (branch names like `grp/sub`: keep
  slashes, check and encode each part; also inside `refs/heads/{branch}`)
- response mode `array` (top-level JSON arrays, optional per-item `each`)
- request `validate_only`: args the Rust adapter checks (required, segment) but
  never sends (e.g. owner/repo on a GraphQL discussion comment). A plain
  "ignored" list was wrong: adversarial `grp/sub` showed Rust rejects them.
Hard-won detail: `commit_id` in a body still needs `segment: true`; the
adversarial cases catch every missing id check, so run them before trusting a
port pass.

Notion lessons: the most hybrid service so far (7/23 spec). Requests are mostly
trivial; the value of the Rust adapter is content *conversion*: markdown <->
block trees, 2000-char rich-text chunking, titles pulled from typed property
maps, comment threads grouped by discussion. Do not grow the format toward a
template language for that: keep conversions in Rust and spec the plain
pass-through/projection tools. Rule of thumb: if the Rust parse/build calls a
helper that recurses or loops with state, it is a Rust action. A read-only
integration makes writes unprovable live; detect `restricted_resource` and
report SKIP instead of creating fixtures elsewhere.

Ideas not yet automated: `segment`/required inference for GraphQL variables
(probe `grp/sub` and absence per variable), `tristate` inference (enum `["true","false"]` sent as
bool), `clear_*`->switch-`else` inference, `segment` on body ids, label-style
`namespaced` extra chars, nested `each` inference (markers inside nested lists),
template inference for computed strings, switch inference from enum args.
