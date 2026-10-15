---
name: adapter-builder
description: Add a connector for a new external service at runtime without recompiling CTOX. Research the official API docs, write a declarative adapter spec (ctox.adapter-spec/v1), validate it, prove it with a real live call, and request activation from the user through the supervisor. Use when an agent needs a service that `toolhub catalog` does not list.
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

- Only the **supervisor** talks to the user (credential card, activation card).
  Persistent workers hand the request to the supervisor and wait. One-shot
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
8. **Request activation.** The supervisor shows the activation card: service,
   version, allowed hosts, actions grouped by risk, live-test result, diff to
   the previous version. Only after the user approves:
   `SpecStore::activate(slug, version, actor)`; the hub picks it up via
   `reload_specs_if_changed` without restart.
9. **Rollback** if a new version misbehaves: `SpecStore::rollback(slug, actor)`
   (or `deactivate`). Report what happened.

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
- **Trello (2026-10-15):** hot-plug only (`specs/trello.json`, 22 actions). All
  live-passed; a throwaway board is created and deleted. Needed auth scheme
  `oauth_header` (key+token in the Authorization header). Webhooks need a
  URL that answers HEAD 200 (https://example.com/ works; a path that 404s does not).
- **Not wired yet:** the toolhub is not part of the `ctox` binary on main, so there
  is no `ctox toolhub spec …` command, no MCP tool and no activation card yet.
  Until then steps 4 to 9 run only in tests. The user-facing parts (cards,
  catalog view) need a sync collection with a profile and typed commands, which
  is ctox sync v3 stage S2 work (label `sync-v3:S2` from the sync owner).

## Lessons

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

Ideas not yet automated: nested `each` inference (markers inside nested lists),
template inference for computed strings, switch inference from enum args.
