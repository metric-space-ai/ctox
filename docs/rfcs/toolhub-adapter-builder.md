# RFC: Toolhub Adapter-Builder – Agenten-Schnittstelle für Ad-hoc-Adapter

Status: **Rev. 3, 17.10.2026.** Umgesetzt in PR #933 (`feat/toolhub-adapter-builder`). Rev. 3 übernimmt die Nutzerentscheidung „Adapter-Einrichtung vollautonom“: keine Nutzerfreigabe zum Installieren oder Aktivieren, kein Adapter-Gate, keine Mail-Erinnerung. Rev. 2 vom 16.10.2026 war mit dem Code abgeglichen.

## 0. Ziel und Regeln

Ein CTOX-Agent soll einen fehlenden Dienst als Spec im Meta-Standard `ctox.adapter-spec/v1` anlegen, prüfen, live testen und selbst aktivieren, ohne Nutzerfreigabe, ohne Neukompilieren und ohne Neustart. Vorbild ist Composio (suchen, Schema holen, ausführen), gebaut aus vorhandenen CTOX-Teilen.

Diese Regeln gelten ohne Ausnahme:
- Es gibt keine HTTP-Endpunkte und kein Gateway. Die Browser-Anbindung läuft nur über ctox sync.
- Nur die Core-Instanz ruft HTTPS nach außen auf.
- Kein SSH.
- Keine Umgebungsvariablen als Schalter (AGENTS.md: „Do not add new process-environment toggles“).
- MCP darf weder RxDB noch die Policy umgehen.
- Alles gilt pro Instanz.
- Karten zeigt nur der Supervisor (Zugang; Freigabe einzelner destruktiver Aufrufe).
- Es gibt keinen neuen OAuth-Stack.

**Festgelegte Vorgaben:**
1. **(Rev. 3)** Die Adapter-Einrichtung ist vollautonom. Aktiviert wird ohne Nutzerfreigabe, sobald `validate` besteht und ein bestandener Live-Test vorliegt, der höchstens 24 h alt und an den sha256 der Version gebunden ist. Supervisor: jederzeit. Persistenter Worker: nur innerhalb einer Builder-Mission. One-shot und externes MCP: nie. Die Laufzeit-Risikopolitik bleibt: destruktive Aufrufe brauchen weiter die Freigabe je Aufruf.
2. Die Rust-Adapter bleiben. Neue Dienste kommen als Spec.
3. Ein geprüfter Spec-Katalog wird mit dem Release ausgeliefert. Aktiviert wird pro Instanz.
4. ~~Bis S2 läuft die Freigabe über den Supervisor-Chat.~~ Entfällt mit Rev. 3 für Adapter. Freigaben je destruktivem Aufruf laufen über den bestehenden `approval-gate`-Weg.
5. Externes MCP darf nur lesen.
6. Neue OAuth-Anbieter kommen nur mit einem Release.

## 1. Bestand, auf dem der Entwurf aufbaut (geprüft)

| Baustein | Ort im Code |
|---|---|
| Spec-Schema (`risk`: read/write/destructive, GET ⇒ read, DELETE ⇒ destructive) | `src/core/execution/toolhub/schema/adapter-spec.v1.schema.json` |
| Interpreter, Validator, Egress | `toolhub/src/spec/{adapter,validate,egress}.rs`. Der Validator lässt nur öffentliches HTTPS zu, auch bei den OAuth2-Endpunkten |
| Store pro Instanz | `toolhub/src/spec/registry.rs`, `SpecStore::for_instance_root(root)` → `<root>/runtime/toolhub/adapter-specs/<slug>/{versions/<v>.json, active.json, history.jsonl}`; `install` validiert und legt **inaktiv** ab (Version unveränderlich), `activate/rollback/deactivate(…, actor)`, `list`, `fingerprint`, `load_active` |
| Verbindungen | `toolhub/src/store.rs` im CTOX-Secret-Store (`src/core/secrets.rs`), Scope `toolhub:<toolkit>:<account_id>`, Name `token` |
| Secrets aus der UI | Befehl `ctox.secret.put` in `src/core/business_os/command_plane.rs` → `handle_secret_command`; nativ `secrets::write_secret_record` / `read_secret_value` |
| CLI | `src/core/main.rs`, Dispatch `Some("secret") => secrets::handle_secret_command` (dort käme `toolhub` dazu) |
| Harness-Werkzeuge | `harness/core/src/tools/spec.rs` (`create_ctox_web_search_tool`, `register_handler("ctox_web_search", …)`), Handler `tools/handlers/ctox_web.rs` ruft das `ctox`-Programm auf (`resolve_ctox_binary`), eingeschaltet per Config `tools.ctox_web` (`resolve_ctox_web_stack_enabled`, kein Env) |
| Business-OS-MCP | `mcp_channel.rs`: `tool_descriptors()`, `read_tool(…)`, `call_tool`, Policy `BusinessOsMcpPolicy{allow_reads, allow_writes, allow_external_effects, denied_tools…}` |
| Rechte | `business_os/policy.rs::evaluate`; passende Rechte gibt es schon: `IntegrationsManage`, `SecretsManage`, `ExternalApprove` |
| Freigaben | Interne Work-Items der Art `approval-gate` (`mission/tickets/work_items.rs`), Erinnerung und strukturierte Antwort über `mission/approval_nag.rs` (`[ctox-approve:<id>]`, `approval_modality = "tui-only"`) |
| Zeitplan | `mission/schedule.rs::ensure_task` / `emit_due_tasks` |
| Skills | `harness/core/src/skills/loader.rs`, `src/skills/system/skill_meta/skill-lifecycle`, Skill `src/skills/system/integrations/adapter-builder/SKILL.md`, Generator `toolhub/examples/spec_from_builtin.rs` |

Was der erste Entwurf falsch hatte, ist hier korrigiert:
- Greppy ist eine Harness-Wahl in `mcp_supervisor_luma.rs` und kein Werkzeug. Vorbild für das Einhängen ist `ctox_web_*`.
- `store_provider_subscription_auth.rs` kennt heute nur LLM-Abos (`codex|claude|antigravity|kimi|kimi_coding|minimax`). OAuth für Dienste geht dort noch **nicht**, siehe §6.
- Der Store kennt keine Status wie draft/candidate. Die kommen als Prüfnachweise dazu, siehe §2.

## 2. Werkzeuge

Die Werkzeuge werden einmal in der Toolhub-Crate implementiert und auf drei Wegen angeboten:
- **CLI** `ctox toolhub <werkzeug>`
- **Harness**: Werkzeuge `ctox_toolhub_*` nach dem Muster von `ctox_web`, mit Handler über `resolve_ctox_binary` und Config-Schalter `tools.ctox_toolhub`
- **MCP**: nur die Lese-Werkzeuge über `read_tool(...)`

Die Rolle bestimmt die gebundene Sitzung, so wie `ctox_web` „bound business chat“ verlangt, und nie eine Umgebungsvariable.

| Werkzeug | Wirkung | HTTPS nach außen |
|---|---|---|
| `search` | Aktionen aus builtin- und aktiven Specs suchen, mit Verbindungsstatus. Mit `--catalog` werden auch die mitgelieferten, inaktiven Katalog-Specs gezeigt | nein |
| `get_schema` | Eingabeschema, `risk`, nötige Verbindung | nein |
| `execute` | Aktion über eine Verbindung aus `toolhub/src/store.rs` ausführen, Policy nach §4 | ja, Core |
| `draft_spec` | Entwurf im Arbeitsordner der Mission ablegen, noch nicht im Store. Der Generator darf vorbefüllen | nein |
| `validate` | `SpecAdapter::from_json` mit Validator und Egress, dazu die Parität, falls es ein builtin gibt | nein |
| `live_test` | `tests[]` gegen die echte API laufen lassen (§5). Bei Erfolg schreibt `install` die Version und den Prüfnachweis neben die Version | ja, Core |
| ~~`submit_for_approval`~~ | Entfällt mit Rev. 3 | – |
| `activate` | Ohne Nutzerfreigabe: installierte Version validiert, bestandener Live-Test ≤ 24 h mit passendem sha256. Worker nur mit `--mission <work_id>` einer offenen Builder-Mission für diesen Dienst → `SpecStore::activate(slug, version, actor)` | nein |
| `request_adapter` | Builder-Mission als internes Work-Item (`adapter-build`, Skill `adapter-builder`), eine pro Dienst | nein |
| `approve` | Nur noch für das Gate eines einzelnen destruktiven Aufrufs | nein |
| `deactivate` / `rollback` | `SpecStore::deactivate/rollback`. Wirkt sofort und gilt als Notbremse | nein |
| `list` | `SpecStore::list`, dazu letzter Test und Fehlerzähler | nein |

Neu ist nur Folgendes, alles innerhalb der Crate: die Datei `versions/<v>.review.json` mit dem Testnachweis, die Prüfnachweis-Prüfung in `activate` und der mitgelieferte Katalog-Ordner. Der Katalog wird nur gelesen. `activate` kopiert eine Katalog-Spec per `install` in den Instanz-Store. Es gibt kein Gateway, keine neue Datenbank und keine Sync-Sammlung.

## 3. Rollen und Rechte

Supervisor, persistenter und One-shot-Worker sind heute **Konvention** und keine Code-Rolle. Die Harness kennt nur `agent/builtins/{explorer,awaiter}.toml`. Deshalb prüft `ctox toolhub` die Rechte an der gebundenen Sitzung (Business-OS-Rolle über `policy::evaluate`). Die Harness-Liste blendet nur aus.

| Werkzeug | Supervisor | Persistenter Worker (Builder) | One-shot | Externes MCP |
|---|---|---|---|---|
| search, get_schema, list | ✓ | ✓ | ✓ | ✓ (`allow_reads`) |
| execute `read` | ✓ | ✓ | ✓ | ✓ |
| execute `write` | ✓ nach Policy | über den Supervisor | ✗ | ✗ |
| execute `destructive` | nur mit Nutzerfreigabe je Aufruf | ✗ | ✗ | ✗ |
| draft_spec, validate, live_test `read` | ✓ | ✓ | ✓ | ✗ |
| live_test `write` | ✓ mit Aufräumen und Testobjekt | über den Supervisor | ✗ | ✗ |
| activate (Rev. 3, ohne Nutzerfreigabe) | ✓ mit frischem Prüfnachweis | nur in der Builder-Mission | ✗ | ✗ |
| deactivate, rollback | ✓ (Notbremse, Audit) | nur als Antrag | ✗ | ✗ |
| automatischer Rollback | Kern, nach 5 Fehlschlägen in Folge, einmal | | | |

Fehlt der Zugang, meldet ein One-shot-Worker „Zugang fehlt“ und endet. Ein persistenter Worker delegiert an den Supervisor, und nur der fragt per Karte nach.

## 4. Risiko und Freigabe

- `read` ist frei im Rahmen der Rolle.
- `write` nutzt die bestehende Policy: `policy::evaluate` mit `DataWrite` bzw. `IntegrationsManage`.
- `destructive` braucht pro Aufruf ein `approval-gate`.
- Die Stufe eines Adapters ist die höchste Stufe seiner Aktionen.
- Die Untergrenzen GET ⇒ read und DELETE ⇒ destructive prüft schon das Schema. Der Validator ergänzt POST/PUT/PATCH ⇒ mindestens write. Ausnahmen wie Such-POSTs braucht eine Begründung in der Spec.

Ablauf der Aktivierung (Rev. 3, autonom):
1. `validate` besteht (Schema, Egress-Validator: nur https, erlaubte Hosts, keine IP/lokalen Hosts, Secret-Scan, Risiko je Methode).
2. `live_test` besteht. Der Prüfnachweis ist höchstens 24 h alt und an den sha256 der installierten Version gebunden.
3. `activate` durch den Supervisor oder durch einen Worker innerhalb seiner Builder-Mission. Kein Gate, kein Nutzerschritt, keine Erinnerungsmail. Alles landet in `history.jsonl` (`activate_proof` mit sha256, Testzeit, Tester, Mission).
- Eine Spec-Änderung ergibt einen neuen sha256. Damit ist der Prüfnachweis ungültig; es braucht einen neuen Live-Test.
- Katalog-Specs aus dem Release durchlaufen pro Instanz ebenfalls validate und live_test.
- Automatischer Rollback: Nach 5 fehlgeschlagenen Aufrufen in Folge (Abbildungsfehler, 5xx, 400/410) springt der aktive Spec einmal auf die vorige Version zurück (`auto_rollback` im Audit). Kein Hin und Her; danach entscheidet der Supervisor.
- Destruktive Aufrufe brauchen weiter ein `approval-gate` je Aufruf (einmal nutzbar, an den Argument-Hash gebunden). Das Gate schließt nur die autorisierte Nutzerantwort.
- Nicht verwendet wird `CTOX_AUTO_APPROVE_GATES`, denn das wäre ein Env-Schalter.

## 5. Live-Test

- Er läuft nur im Core, die Egress-Prüfung greift bei jedem Aufruf.
- Standard sind nur `read`-Proben.
- `write`-Proben gibt es nur mit `cleanup`, einem eigenen Testobjekt und, wo vorhanden, einem Testmodus-Nachweis (z. B. `sk_test_`).
- `destructive`-Proben gibt es nie.
- Das Ergebnis wird pro Aktion festgehalten (bestanden, fehlgeschlagen oder ausgelassen mit Grund) und an den sha256 gebunden. Antworten werden gekürzt und geschwärzt (`harness/secrets/src/sanitizer.rs`).
- Eine Aktion zählt erst nach einem echten Aufruf.

## 6. Zugänge und OAuth

- API-Key, Bearer, Basic und Header-Token holt der Supervisor per Karte. Gespeichert wird über `ctox.secret.put` bzw. `write_secret_record` im Scope `toolhub:<slug>:<account_id>`. Die Spec enthält nie Secrets.
- Für OAuth gibt es heute **keinen vorhandenen Weg für Dienste**. `store_provider_subscription_auth.rs` lässt nur LLM-Anbieter zu.
  - Vorschlag: Ein Release erweitert diesen vorhandenen Ablauf (`start_subscription_auth_command`, Recht `IntegrationsManage`) um eine Allowlist von Dienst-Anbietern. Neue Anbieter kommen nur per Release.
  - Bis dahin können OAuth-Specs zwar validiert, aber nicht verbunden werden. Der Builder meldet dann „Anbieter braucht Release“.
  - Der Scope `toolhub-config:<toolkit>` in `toolhub/src/store.rs` (OAuth-Client-Secret) wird nicht für einen eigenen Ablauf benutzt.

## 7. Audit

- Store-Ereignisse gehen nach `history.jsonl`. Das gibt es schon, mit `actor`.
- Neu: `execute`, `live_test` und Gate-Ereignisse werden pro Slug angehängt. Erfasst werden Zeit, Akteur und Sitzung, Mission, `slug@version`, sha256, Aktion, Risiko, Policy-Entscheidung mit Grund und Ergebnis.
- Keine Secrets und keine Nutzdaten.
- Die Aufbewahrung richtet sich nach `BusinessOsMcpPolicy.audit_retention_days`.

## 8. Fehlerfälle

| Fall | Verhalten |
|---|---|
| Validator oder Egress lehnt ab | Nichts wird installiert. Der Builder bekommt maschinenlesbare Fehler |
| Fremder Host, Redirect, IP | Abbruch mit Audit und Meldung an den Supervisor |
| Zugang fehlt | One-shot: „Zugang fehlt“. Persistenter Worker: an den Supervisor, der die Karte zeigt |
| OAuth-Anbieter unbekannt | „Anbieter braucht Release“, ohne Workaround |
| Live-Test rot oder Aufräumen scheitert | Kein Prüfnachweis, keine Aktivierung. Bleibt ein Testobjekt übrig, kommt es ins Audit und wird gemeldet |
| sha256 nach dem Test geändert oder Nachweis älter als 24 h | Keine Aktivierung, neuer Live-Test nötig |
| Drift zur Laufzeit (5 Fehlschläge in Folge) | Automatischer Rollback auf die vorige Version, falls vorhanden; sonst `request_adapter` für eine Reparatur-Mission |
| Defekter Spec bzw. Zeiger | `load_active` überspringt ihn, der Rest läuft weiter, mit Meldung |
| 429 | Retry-After beachten, kein Endlos-Retry |
| Format reicht nicht (Zeit, Schleifen, Signaturen, Umwandlung) | Die Aktion bleibt bzw. kommt in Rust, per Release |

## 9. Phasen und Sync-Label

Prüfpfad laut `.github/workflows/sync-scope-guard.yml`:

`src/apps/business-os/rxdb/`, `src/core/rxdb/`, `src/core/business_os/rxdb_peer*.rs`, `shared/{sync,db,rxdb-runtime}.js`, **`src/core/business_os/command_plane.rs`**, `docs/ctox-rxdb.md`

**`mcp_channel.rs` steht nicht im Prüfpfad.** Die Grenzfrage ist damit geklärt.

| Phase | Inhalt | Label |
|---|---|---|
| P0 | #776 und #796 fertig, dazu der Release-Katalog als nur lesbarer Ordner | nein |
| P1 | `ctox toolhub …` in `main.rs`: alle Werkzeuge, Prüfnachweis, Aktivierung ohne Nutzerfreigabe, Auto-Rollback, Audit | nein |
| P2 | Harness-Werkzeuge `ctox_toolhub_*` (`spec.rs`, Config `tools.ctox_toolhub`), MCP-Lese-Werkzeuge in `mcp_channel.rs`, Builder-Mission mit Skill (Rev. 3: ohne Nutzerfreigabe) | nein |
| P3 | Zugangskarte und Adapter-Status im Browser: typisierte Befehle `ctox.toolhub.adapter.{rollback,deactivate}` in `command_plane.rs`, Katalog/Status als Sammlung mit Profil | **ja, `sync-v3:S2`** |
| P4 | Audit als Projektion im Browser | **ja, S2/S4** (Projektionsschreiber) |
| Release | Dienst-OAuth-Anbieter in `store_provider_subscription_auth.rs` | nein (nicht im Prüfpfad), braucht aber eine Produktentscheidung |

## 10. Offen

- **Sync-Owner:** Zeitpunkt von S2 für P3/P4. Ob der Befehl `ctox.secret.put`, der schon in `command_plane.rs` steht, für den Scope `toolhub:*` ohne Änderung dort reicht. Ich gehe davon aus, dass es so ist.
- **Nutzer:** Welche Dienst-OAuth-Anbieter kommen ins erste Release?

---

## English summary

**Goal:** A CTOX agent adds a missing service as a `ctox.adapter-spec/v1` spec. It is validated, live-tested in core and then activated autonomously (rev. 3: no user approval), with no recompiling and no restart. Everything is per instance.

**Defaults:**
- (Rev. 3) Adapter setup is fully autonomous: activation needs a passing validate plus a passing live-test proof under 24 h bound to the sha256. Supervisor any time, worker only inside a builder mission, one-shot and MCP never. Destructive calls still need per-call approval. Auto-rollback after 5 consecutive failures, once.
- The Rust adapters stay; new services ship as specs.
- A vetted catalog ships with the release, and each instance activates specs on its own.
- External MCP is read-only.
- New OAuth providers come only with a release.

**Built on existing code:**
- `SpecStore` (versions, active pointer, `history.jsonl`, rollback, `load_active`)
- toolhub accounts in the CTOX secret store (`toolhub:<toolkit>:<account_id>`)
- `ctox.secret.put`
- `policy::evaluate`
- `approval-gate` work items (per-call approval of destructive actions only)
- `schedule::ensure_task`
- the `ctox_web` harness tool pattern: a handler that calls the `ctox` binary, switched on in config rather than an env var
- `mcp_channel.rs` `read_tool`

**Corrections from rev. 1:**
- Greppy is a harness choice, not a tool.
- subscription_auth supports only LLM subscriptions, so service OAuth needs a release.
- The store has no draft/candidate states; a review proof file plus a proof check in `activate` is added instead.
- The schema lives under `toolhub/schema/`.
- The secret scope format is `toolhub:<toolkit>:<account_id>`.

**Tools:** search, get_schema, execute, draft_spec, validate, live_test, activate (fresh proof whose sha256 matches; no user approval), request_adapter (builder mission), approve (per-call destructive gates only), deactivate and rollback (emergency brake), list.

**Phases:**
- P0–P2 (crate, CLI, harness tools, MCP read tools, builder mission) need **no sync label**: `mcp_channel.rs` is not on the guard path.
- P3 (typed commands in `command_plane.rs`, browser cards, a status collection) needs **`sync-v3:S2`**.
- P4 (the audit projection) needs S2/S4.

## Dogfood 2026-10-17 / Erprobung

**DE.** Drei neue Dienste (Open-Meteo, Frankfurter/EZB, Nager.Date) wurden nur mit `ctox toolhub` als Worker auf einer lokalen Instanz gebaut, getestet (11/11 Aktionen live bestanden) und ohne Nutzerschritt aktiviert. Nötig waren drei Formatkorrekturen (`number` mit Nachkommastellen, `..` innerhalb eines Pfadsegments, Antwortmodus `status`) und eine Builder-Korrektur (Mission `nager-date` = Slug `nager_date`). Offene Lücken: kein Generator von API-Doku/OpenAPI zu Spec, ein Host pro Spec, keine Weiterleitungen, kein Wiederholen bei Netzfehlern im Live-Test, Mission wird nach Aktivierung nicht geschlossen, Fake-IP-Schalter nur per Instanzkonfiguration ohne CLI.

**EN.** Three new services were built, live-tested (11/11 actions pass) and activated by a worker with no user step. Needed three format fixes and one builder fix; open gaps: no docs/OpenAPI-to-spec generator, one host per spec, no redirects, no retry on transient network errors in the live test, mission not closed on activation, fake-IP egress switch only via instance config (no CLI).

### Lücken geschlossen / Gaps closed (2026-10-17)

**DE.** Geschlossen: OpenAPI/Swagger-Generator (`draft-spec --from-openapi <url|datei>`: Aktionen, Risiko nach Methode, Auth-Vorschlag, Host-Allowlist, Lese-Proben; Detail-Proben holen ihre ID aus einer Listen-Probe, `from`), Weiterleitungsprüfung in validate/live-test (nennt den Zielhost), Form-Prüfung der Proben (leere Antwort fällt durch, außer `allow_empty`), ein Wiederholungsversuch bei Netz-/TLS-Fehlern im Live-Test, Mission schließt sich nach Aktivierung selbst (Kern-Grant prüft aktiven sha256 und den an die Mission gebundenen Nachweis), `config set fake-ip-egress on|off`. Erprobt mit Spaceflight News, Autobahn und PEGELONLINE (24 Aktionen aktiv). Offen: mehrere Hosts pro Spec, DE/EN-Übersetzung der übernommenen Texte, API-Schlüssel in der Query, binäre Antworten, Wiederholung bei `execute`, Pfade der Weiterleitung (nur Host).

**EN.** Closed: OpenAPI/Swagger generator with chained detail probes, redirect check, result-shape assertions, one retry on transient errors in the live test, mission self-closes after activation (bound to the work item id), typed fake-IP config command. Dogfooded on Spaceflight News, Autobahn, PEGELONLINE (24 actions active). Open: multi-host specs, translating copied texts, query API keys, binary responses, retry in `execute`, redirect path (host only).

### Nachlauf / Follow-up (2026-10-17, frische Instanz / fresh instance root)

**DE.** Erneuter Durchlauf aller drei OpenAPI-Dienste auf einer frischen lokalen Instanz, nur mit der gebauten CLI als Worker: Spaceflight News 7/7, Autobahn 12/12, PEGELONLINE 5/5 live bestanden, 24 Aktionen aktiv, Missionen jeweils selbst geschlossen. Gefunden und behoben: HTTP 429 im Live-Test (jetzt ein Wiederholungsversuch nach 2 s), Detail-Proben griffen bei gleichem Schema auf die falsche Liste (jetzt zuerst die Liste am Elternpfad), Slug aus dem Dokumenttitel passte nicht zur Mission (`draft-spec --mission` übernimmt den Slug der Mission), Weglassen einzelner Aktionen (`--skip-operations`). Eingriffe: 1 Betreiberschritt (Fake-IP auf dieser Box), 3 Builder-Schritte nach Skill (Webcam weglassen, Dokument-URL ausweichen, eine falsch dokumentierte Antwortform in der Probe korrigieren), 0 Spec-Handarbeit. Offen zusätzlich: `execute` ohne 429-/Netz-Wiederholung, Formprüfung erkennt fehlende optionale Felder nicht.

**EN.** Re-run of all three OpenAPI services on a fresh local instance with only the built CLI as worker: 7/7, 12/12, 5/5 live PASS, 24 actions active, missions self-closed. Found and fixed: HTTP 429 in live-test (one retry after 2 s), detail probes chained to the wrong list when schemas are shared (parent-path list first), slug from the document title not matching the mission (`draft-spec --mission`), leaving out single operations (`--skip-operations`). Interventions: 1 operator step (fake-IP on this box), 3 skill-level builder steps, 0 hand-written spec edits. Still open in addition: no 429/network retry in `execute`; shape checks cannot see missing optional fields.

