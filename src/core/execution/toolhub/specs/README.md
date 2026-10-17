# Hot-plug adapter specs / Hot-Plug-Adapter-Specs

**DE.** Ein Adapter-Spec (`ctox.adapter-spec/v1`, Schema: `../schema/adapter-spec.v1.schema.json`)
beschreibt einen Dienst deklarativ. `ctox_toolhub::spec::SpecAdapter` führt ihn zur Laufzeit
aus, ohne dass CTOX neu kompiliert wird. Specs liegen pro Instanz unter
`<root>/runtime/toolhub/adapter-specs/<slug>/` (unveränderliche Versionen, aktiver Zeiger,
Verlauf, Rollback). `ToolHub::reload_specs_if_changed` lädt sie ohne Neustart.

Sicherheitsregeln (erzwingt der Validator bzw. die Laufzeit):
- nur `https`, nur Hosts aus `allowed_hosts`, keine IP-Literale, keine lokalen/internen Namen;
- keine Weiterleitungen, Verbindungen nur zu öffentlichen Adressen (gepinnt im Resolver);
- Pro-Verbindungs-Hosts (`api_base_template`) nur als einzelnes DNS-Label innerhalb der erlaubten Hosts;
- keine Zugangsdaten im Spec (Scan), keine Auth-/Cookie-Header – Anmeldung setzt der Hub ein;
- jede Aktion hat eine Risikostufe; `GET` = `read`, `DELETE` = `destructive`, sonst `write`
  (Ausnahme `read_only_post` für reine Abfrage-POSTs; GraphQL-`mutation` ist nie `read`).

Die beiden Specs hier sind der Nachweis: Todoist und Asana, portiert aus den kompilierten
Adaptern. `tests/spec_adapters.rs` vergleicht 739 Anfrage-/Antwortfälle mit den Rust-Adaptern
(alle gleich), die Live-Tests (`live_todoist`, `live_asana`, Test `…_via_spec`) laufen mit
denselben Ergebnissen gegen die echten APIs.

**EN.** Declarative, versioned service adapters interpreted at runtime by `SpecAdapter`;
stored per instance; activated via an explicit pointer (approval happens outside this crate);
reloaded without restart. The two specs here (Todoist, Asana) are equivalent to the compiled
adapters (739 differential cases, identical live results).

Calendly (`calendly.json`) is the first spec-only build (no compiled code changes; live test `live_calendly`). It added `uri_prefix` / `uri_tail` for resource-URI references.

`zippopotam.json` is a brand-new service (no compiled adapter) with auth `none`: it is loaded only through the store and hot reload (`spec_adapters` test, live test `live_hotplug_public`).

`stripe.json` is a hot-plug only port of the Activepieces Stripe piece (form bodies with bracketed nested keys, `starting_after`/`has_more` cursor pagination via a `-` pointer segment). Live test `live_stripe` refuses any key that is not `sk_test_`.

`trello.json` is a hot-plug only port of the Activepieces Trello piece. Auth scheme `oauth_header` (credential `key:token`) puts the secrets in the Authorization header, never in the URL. Live test `live_trello` creates a throwaway board and deletes it at the end.

`airtable.json` was converted from the compiled adapter with the generator `examples/spec_from_builtin.rs` plus hand fixes; `support::spec_parity` checks it against every recorded case in `tests/port_airtable.rs`.

`open_meteo.json`, `frankfurter.json` and `nager_date.json` are the first specs the autonomous builder wrote itself from public docs (dogfood 2026-10-17, keyless). They needed three format fixes: `number` args keep decimals (coordinates), `..` is allowed as text inside a path segment (Frankfurter `/v1/{start}..{end}`, still rejected as a segment), and response mode `status` maps an empty 2xx answer (Nager.Date 200/204) to a value.

Known limits of v1 (use a compiled adapter for these): multipart uploads, request signing
(HMAC/AWS SigV4), binary downloads, multi-step calls (e.g. upload session then commit),
per-tool response logic beyond pick/list/object/merge/const/array/status, several API hosts in one spec (Open-Meteo geocoding/archive need their own specs).
