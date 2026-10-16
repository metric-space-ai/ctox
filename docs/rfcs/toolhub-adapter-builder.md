# RFC: Toolhub Adapter-Builder – Agenten-Schnittstelle für Ad-hoc-Adapter

Status: **Entwurf, zur Freigabe durch Michael. Es wird nichts umgesetzt, bevor der Entwurf freigegeben ist.**
Stand: 16.10.2026 · Branch `feat/toolhub-hotplug-specs` (PR #796) · Vorgänger: Entwurf vom 15.10.2026 (Adapter-Builder, Recherche)

## 0. Ziel und Grenzen

Ein CTOX-Agent soll einen fehlenden Dienst selbst als Adapter im Meta-Standard
`ctox.adapter-spec/v1` anlegen, prüfen, live testen und zur Freigabe vorlegen können.
Nach der Freigabe wird der Adapter aktiviert, ohne Neukompilieren und ohne Neustart. Das
Vorbild ist die Agenten-Schnittstelle von Composio (Werkzeuge suchen, Schema holen,
ausführen, Verbindungen verwalten), umgesetzt mit CTOX-Mitteln.

Harte Regeln, die jeder Teil dieses Entwurfs einhält:

1. CTOX kommuniziert nur über ctox sync (WebRTC/RxDB). Es gibt keine HTTP-Endpunkte und kein Gateway für diese Schnittstelle.
2. Nur die Core-Instanz ruft HTTPS nach außen auf, also auch Live-Tests und Ausführung.
3. Kein SSH.
4. Keine Umgebungsvariablen als Laufzeitschalter (AGENTS.md). Konfiguration liegt im Instanz-Store, Zugangsdaten im Secret-Store.
5. MCP umgeht weder RxDB noch Policy. Die Werkzeuge laufen durch dieselbe Policy-Prüfung wie `command_plane.rs`.
6. Alles gilt pro Instanz: Specs, Zeiger, Verbindungen, Audit.
7. Karten zeigt nur der Supervisor. Persistente Worker delegieren an ihn. One-shot-Worker enden bei fehlendem Zugang mit „Zugang fehlt“.
8. OAuth läuft über `store_provider_subscription_auth.rs`, Secrets über `ctox.secret.put` bzw. `crate::secrets`. Es gibt keinen neuen OAuth-Stack.
9. Jede Änderung an Sync-Sammlungen braucht das Label `sync-v3:S<n>`, das nur der Sync-Owner vergibt. Siehe §9.

Nicht im Umfang: Code zur Laufzeit laden (kein WASM, kein JS, keine Skripte), neue OAuth-Anbieter durch den Agenten, und das Teilen von Specs zwischen Instanzen (offene Frage).

## 1. Ausgangslage (PR #796)

Schon vorhanden und unverändert wiederverwendet:

- **Format** `schema/adapter-spec.v1.schema.json`: REST, GraphQL und Form, Basis-URL-Vorlagen je Verbindung, Seitenweise-Abruf, Fehlerprüfung und `risk` je Aktion.
- **`SpecAdapter`** (`toolhub/src/spec/adapter.rs`) implementiert `ToolkitAdapter`. Hub, Transport und Auth-Injektion bleiben gleich.
- **Validator** (`validate.rs`) und **Egress-Prüfung** (`egress.rs`): nur HTTPS, nur `allowed_hosts`, keine IPs, keine Secret-Literale.
- **`SpecStore`** (`registry.rs`, `SpecStore::for_instance_root`): unveränderliche Versionen, `install`, `activate`, `deactivate`, `rollback`, `list`, `fingerprint` und `load_active` für das Neuladen ohne Neustart.
- **Generator** `examples/spec_from_builtin.rs` und **Skill** `src/skills/system/integrations/adapter-builder/SKILL.md`.
- Der Nachweis sind zwölf Dienste, davon fünf komplett als Spec, die übrigen als Mischform mit Rust. Ein neuer Dienst ohne Rust-Adapter wurde im laufenden Betrieb eingesteckt.

## 2. Werkzeuge

Alle Werkzeuge heißen `ctox.toolhub.*` und werden genau einmal in der Toolhub-Crate
definiert. Sie werden auf drei Wegen angeboten, die alle dieselbe Funktion und dieselbe
Policy-Prüfung nutzen:

- **CLI:** `ctox toolhub <werkzeug>`, wie `ctox secret`.
- **Harness-Werkzeug:** wird wie Greppy oder die Websuche über die System-Prompt-Erweiterung bzw. die Werkzeugliste der Sitzung eingehängt. Die Rolle kommt aus dem Sitzungskontext und nie aus einer Umgebungsvariable.
- **Business-OS-MCP:** Eintrag in `mcp_channel.rs::tool_descriptors()` / `call_tool`. Das ist nur ein typisierter Kanal in Store und Policy, kein eigener Datenpfad.

| Werkzeug | Zweck | Schreibt | Ausgehender HTTPS-Aufruf |
|---|---|---|---|
| `search` | Aktionen über builtin- und aktive Spec-Adapter suchen (Name, Titel de/en, Beschreibung), mit Verbindungsstatus pro Dienst | nein | nein |
| `get_schema` | Eingabeschema, Risiko und benötigte Verbindung einer Aktion `DIENST_AKTION` | nein | nein |
| `execute` | Eine Aktion ausführen, über eine bestehende Verbindung, mit Policy nach Risiko (§4) | Audit | ja, nur Core |
| `draft_spec` | Spec-Entwurf als neue Version mit Status `draft` ablegen. Eingabe ist das JSON, optional mit dem Generator vorbefüllt. Der Entwurf wird nie aktiv | Store | nein |
| `validate` | Schema, Validator und Egress-Prüfung, danach der Paritätsvergleich, falls ein builtin existiert | Status | nein |
| `live_test` | Die `tests[]` der Spec gegen die echte API laufen lassen (§5). Bei Erfolg wird der Status `candidate` | Status, Audit | ja, nur Core |
| `submit_for_approval` | `candidate` dem Supervisor vorlegen. Der Supervisor erzeugt die Freigabekarte | Status | nein |
| `activate` | Den aktiven Zeiger auf eine freigegebene Version setzen und neu laden | Zeiger | nein |
| `deactivate` | Den Zeiger entfernen. Die Werkzeuge verschwinden sofort | Zeiger | nein |
| `rollback` | Den Zeiger auf die vorige aktive Version setzen | Zeiger | nein |
| `list` | Specs mit Versionen, Status, aktivem Zeiger, letztem Test und Fingerprint | nein | nein |

Verbindungen und Zugänge bekommen keine eigenen Werkzeuge. `execute` und `live_test` melden
`missing_connection` mit den `connection_fields` der Spec. Der Supervisor holt den Zugang dann per
Karte und speichert ihn über `ctox.secret.put` (Scope `toolhub/<slug>/<connection>`), bei OAuth über
`start_subscription_auth_command`. Die Spec referenziert nur `provider_ref` und enthält nie Secrets.

Lebenszyklus einer Version: `draft → validated → candidate → approved → active` (Zeiger) mit
`rejected` und `superseded`. Versionen sind unveränderlich, Änderungen erzeugen immer eine neue Version.

## 3. Rollen und Rechte

| Werkzeug | Supervisor | Persistenter Worker (Builder) | One-shot-Worker | Externer MCP-Agent |
|---|---|---|---|---|
| search, get_schema, list | ja | ja | ja | ja (Policy-Scope) |
| execute `read` | ja | ja | ja | ja, falls Scope |
| execute `write` | ja, nach Policy | über den Supervisor | nein → „Zugang fehlt“ / Delegation | nur mit Freigabe-Policy |
| execute `destructive` | nur mit Nutzerfreigabe je Aufruf | nein | nein | nein |
| draft_spec, validate | ja | ja | ja (z. B. Generator-Lauf) | nein |
| live_test `read` | ja | ja | ja | nein |
| live_test `write` | ja, mit Aufräum-Aktion und Freigabe | über den Supervisor | nein | nein |
| submit_for_approval | ja | ja (→ Supervisor) | nein (Ergebnis an Auftraggeber) | nein |
| activate | nur nach Freigabe (§4) | nein | nein | nein |
| deactivate, rollback | ja (Notbremse, ohne Karte, mit Audit) | nein, nur Antrag | nein | nein |

Der Builder ist kein neuer Prozess. Er ist eine Worker-Rolle mit dem Skill `adapter-builder`
und läuft als Mission über den plan-/queue-orchestrator. Für Routine wie Doku lesen und Spec
entwerfen nutzt er die Worker-Modelle über den Claude-Code-Harness.

## 4. Risikostufen und Freigabe

Jede Aktion hat `risk`:

- **`read`**: liest nur. `execute` ist frei im Rahmen des Rollen-Scopes.
- **`write`**: erzeugt oder ändert Daten beim Dienst, umkehrbar. Es gilt die bestehende Freigabe-Policy des Business OS.
- **`destructive`**: löscht oder ist nicht umkehrbar (Zahlung, Versand an Menschen, Schema-Änderung). Jeder Aufruf braucht eine Nutzerfreigabe.

Die Stufe eines Adapters ist das Maximum seiner Aktionen. Der Validator lehnt zu niedrige Stufen
ab (Heuristik: `DELETE` ist mindestens `destructive`, `POST/PUT/PATCH` mindestens `write`; Ausnahmen
wie Such-POSTs muss die Spec ausdrücklich begründen).

Ablauf der Freigabe:

1. Der Builder ruft `submit_for_approval` auf. Die Version muss `candidate` sein, mit einem bestandenen Live-Test, der höchstens 24 h alt und an den Fingerprint gebunden ist.
2. Der Supervisor zeigt die Karte „Neuer Adapter“ (de/en, ohne Fachjargon): Dienst, Hosts, Aktionen nach Risiko, Testergebnis, Diff zur vorigen Version und benötigter Zugang.
3. Der Nutzer wählt Freigeben, Ablehnen oder nur Lese-Aktionen freigeben. „Nur Lesen“ aktiviert die Version mit gesperrten `write`/`destructive`-Aktionen.
4. Dann `activate` und neu laden. Das Ergebnis geht als Nachricht an den Builder.

Vorschlag für die Zwischenzeit vor S2: Die Karte läuft über den bestehenden Supervisor-Chat-Weg,
und die Freigabe wird als Audit-Eintrag mit Nutzerbezug gespeichert. Die Sync-Karte kommt mit Phase 3.

## 5. Live-Test

- Er läuft nur in der Core-Instanz, mit der Verbindung aus dem Secret-Store, und Egress wird bei jedem Aufruf neu geprüft.
- Standard sind nur `read`-Proben. `write`-Proben brauchen eine Aufräum-Aktion in der Spec (`cleanup`), ein eigenes Testobjekt (Board, Repo, Kunde) und einen Testmodus-Nachweis, wo es ihn gibt (z. B. Stripe-Schlüssel `sk_test_`).
- `destructive`-Proben gibt es nie automatisch.
- Das Ergebnis enthält pro Aktion bestanden, fehlgeschlagen oder bewusst ausgelassen samt Grund, gebunden an den Spec-Fingerprint. Antworten werden gekürzt und vor dem Speichern mit `sanitizer.rs` geschwärzt.
- Die Definition of Done gilt weiter: Eine Aktion zählt erst nach einem echten Aufruf.

## 6. Audit

Jedes schreibende Werkzeug und jedes `execute` schreibt einen Eintrag ins Instanz-Audit:
Zeit, Akteur (Rolle, Sitzung, Mission), Werkzeug, `slug@version`, Fingerprint, Aktion, Risiko,
Policy-Entscheidung mit Grund (wie `policy.rs`), Ergebnis-Status und Dauer. Es werden nie Secrets
oder Nutzdaten gespeichert, nur Hashes bzw. gekürzte und geschwärzte Auszüge. Das Audit ist nur
erweiterbar (append-only). Die vorhandenen `actor`-Felder in `SpecStore::activate/rollback/deactivate`
werden dafür übernommen. Vor S2 liegt das Audit lokal; die Anzeige im Browser kommt mit Phase 3.

## 7. Fehlerfälle

| Fall | Verhalten |
|---|---|
| Validator lehnt ab | Status bleibt `draft`, mit maschinenlesbaren Fehlern für den Builder, keine Aktivierung |
| Egress fremder Host / Redirect / IP | Der Aufruf wird abgebrochen, mit Audit und Hinweis an den Supervisor |
| Zugang fehlt | Persistenter Worker: an den Supervisor, der fragt per Karte. One-shot: Ende mit „Zugang fehlt“ |
| OAuth-Anbieter fehlt in subscription_auth | Kein Workaround, Ende mit „Anbieter braucht Release“ |
| Live-Test rot | Bleibt `validated`, Builder repariert, höchstens N Versuche je Mission, dann Meldung |
| Aufräumen schlägt fehl | Testobjekt im Audit markieren, Meldung an den Supervisor, keine Freigabe |
| Spec nach Freigabe geändert | Neuer Fingerprint, Freigabe ungültig, neuer Durchlauf |
| Laufzeit-Drift (gehäufte 4xx/Schemafehler) | Schedule-Task (`mission/schedule.rs`) legt eine Reparatur-Mission an, optional automatischer `rollback` |
| Defekter Store bzw. kaputter Zeiger | `load_active` überspringt den Adapter, die übrigen laufen weiter, mit Meldung |
| Rate-Limit / 429 | Retry-After beachten, kein Endlos-Retry |
| Format reicht nicht (Zeit, Schleifen, Signaturen, Umwandlung) | Aktion bleibt in Rust (Mischform) und kommt per Release, kein Format-Ausbau ad hoc |
| Nutzer lehnt ab | `rejected`, kein erneutes Vorlegen derselben Version |

## 8. Phasenplan

| Phase | Inhalt | Sync-Label |
|---|---|---|
| **P0** | PR #776 und #796 fertigstellen: Format, `SpecAdapter`, Validator, Store, Generator, Skill | nein |
| **P1** | Toolhub ins `ctox`-Programm: `ctox toolhub …` (CLI) mit allen Werkzeugen aus §2, Policy nach Risiko, Audit lokal, Live-Test nur im Core | nein |
| **P2** | Dieselben Werkzeuge als Harness-Werkzeug (Einhängen wie Greppy/Websuche) und als Business-OS-MCP-Werkzeug über `mcp_channel.rs` mit Policy-Gate. Builder-Rolle als Mission plus Skill. Freigabe zunächst über den Supervisor-Chat | nein, solange keine Sammlung und kein `business_commands`-Typ dazukommt (siehe §9) |
| **P3** | Sammlung `toolhub_adapters` (Katalog/Status, Profil `control`, Eigentümer Core) und Freigabe- bzw. Zugangskarte im Browser, typisierte Befehle `ctox.toolhub.adapter.{approve,reject,rollback,deactivate}` in `command_plane.rs` | **ja, `sync-v3:S2`** |
| **P4** | Audit-Projektion `toolhub_audit` (Profil `stream`) und Drift-Reparatur per Schedule | **ja, S2/S4** (Projektionsschreiber) |

## 9. Was das Sync-Label braucht

- **Braucht `sync-v3:S2`** (der Sync-Owner vergibt es):
  - jede neue Sammlung (`toolhub_adapters`, `toolhub_audit`) samt Profil und Eigentum
  - neue Befehlstypen in `command_plane.rs` / `business_os_schema_contract.json`
  - Änderungen am Schreiber von `business_commands`
  - Karten, die über RxDB laufen
- **Ohne Sync-Änderung möglich:** P0, P1 und P2, also die Toolhub-Crate, die CLI, die Harness-Werkzeuge, das Business-OS-MCP-Werkzeug (nur Werkzeug-Deskriptor und Policy-Prüfung, keine neue Sammlung), der Skill, die Mission, das lokale Audit und der Store pro Instanz.
- **Grenzfall:** Ob `mcp_channel.rs` unter den Guard-Pfad fällt, entscheidet der Sync-Owner. Andernfalls wird P2-MCP mit P3 zusammengelegt.

## 10. Offene Fragen an Michael

1. Darf der Builder Adapter, die nur Lese-Aktionen haben, selbst aktivieren, oder braucht jeder Adapter eine Karte?
2. Sollen die Rust-Adapter langfristig auf Specs bzw. Mischform umziehen?
3. Sollen geprüfte Specs als gemeinsamer Katalog mit dem Release kommen, oder bleibt alles streng pro Instanz?
4. Reicht die Freigabe über den Supervisor-Chat bis S2, oder soll P2 auf die Sync-Karte warten?
5. Welche Rollen bekommen `execute write` über externes MCP?
6. Sollen neue OAuth-Anbieter weiterhin nur per Release kommen?

---

## English summary

**Goal:** Let a CTOX agent add a missing service ad hoc as a `ctox.adapter-spec/v1` spec (draft → validate → live test → approval → activate) with no recompiling and no restart. The agent interface follows Composio's model (search, get schema, execute, connections), built on CTOX mechanisms.

**Tools:** `ctox.toolhub.{search,get_schema,execute,draft_spec,validate,live_test,submit_for_approval,activate,deactivate,rollback,list}`. They are defined once in the toolhub crate and exposed through three paths:

- the CLI (`ctox toolhub`)
- harness tools, mounted the way Greppy and web search are
- Business OS MCP (`mcp_channel.rs`)

All three use the same policy gate. Connections reuse `ctox.secret.put` and `store_provider_subscription_auth.rs`; there is no new OAuth stack. Only the core instance makes outbound HTTPS calls, and the egress guard is re-checked on every call.

**Roles:**

- The supervisor approves, activates and shows cards, and may deactivate or roll back as an emergency brake.
- The builder is a persistent worker with the skill. It drafts, validates, live-tests and submits to the supervisor.
- One-shot workers only read and draft, and end with "Zugang fehlt" (access missing) when access is missing.
- External MCP agents only read, within their policy scope.

**Risk levels:**

- `read` is free within the role's scope.
- `write` uses the existing approval policy.
- `destructive` needs per-call user approval.

The validator enforces a minimum risk level per HTTP method. Approval binds to the spec fingerprint plus a fresh passing live test, and the user can approve read-only.

**Audit:** An append-only, per-instance log records actor, version, fingerprint, policy decision and outcome. It never stores secrets or payloads.

**Phases:**

- P0: finish PRs #776 and #796.
- P1: CLI in the binary.
- P2: harness and MCP tools, plus the builder mission, with approval through supervisor chat. P0–P2 need **no sync change**.
- P3: the `toolhub_adapters` collection, typed commands and the browser card. **Needs `sync-v3:S2`.**
- P4: the audit stream projection and drift repair. **Needs S2/S4.**

**Open questions:**

1. May the builder self-activate read-only adapters?
2. Should the Rust adapters migrate to specs?
3. Should vetted specs ship as a shared release catalog, or stay per instance only?
4. Is approval through supervisor chat acceptable until S2?
5. Which roles may run `execute write` over external MCP?
6. Should new OAuth providers stay release-only?
