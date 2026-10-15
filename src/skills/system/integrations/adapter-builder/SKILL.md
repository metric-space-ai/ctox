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
