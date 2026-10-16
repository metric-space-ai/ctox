# kundenpipeline (Decision Hub) – Shell-v4-Handler

Strang H5, Stand 15.10.2026. Quelle (read-only): `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/decision_hub.rs`
(1 566 Z.), Routing heute über den Legacy-Zweig `command_plane.rs:2416–2427` → `decision_hub::handle_command :244`.
Handler-Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`.
Tests: `node --test src/apps/business-os/modules/kundenpipeline/handlers/tests/*.test.mjs` (25/25 grün, In-Memory-ctx
`handlers/tests/_ctx.mjs` mit Schemaprüfung `required` + Top-Level-`type` aus `collections.schema.json`).

## Befehlstypen

| Befehlstyp | Einstufung | Rust-Ort | Ziel (v4) | Stand |
|---|---|---|---|---|
| `kundenpipeline.triage.write` | FACHLOGIK | `decision_hub.rs:713` `handle_triage_write` | `handlers/kundenpipeline.triage.write.js` | portiert, 2 Tests |
| `kundenpipeline.decision.request` | FACHLOGIK | `:274` `handle_decision_request`, `:306` `request_agent_decision`, `:985` `normalize_agent_decision_request` | `handlers/kundenpipeline.decision.request.js` (+ Export `requestAgentDecision` für den MCP-Pfad) | portiert, 6 Tests (inkl. 3 Rust-Tests nachgebildet) |
| `kundenpipeline.decision.resolve` | FACHLOGIK | `:292` `handle_decision_resolve`, `:441` `resolve_agent_decision` | `handlers/kundenpipeline.decision.resolve.js` (Logik in `_lib/hub.js resolveAgentDecision`) | portiert, 2 Tests |
| `kundenpipeline.decision.answer` | FACHLOGIK | `:524` `handle_decision_answer`, `:630` `run_acceptance_followup` | `handlers/kundenpipeline.decision.answer.js` | portiert, 5 Tests |
| `kundenpipeline.mail.send` | GEMISCHT | `:787` `handle_mail_send` | `handlers/kundenpipeline.mail.send.js` → `ctx.cap.mail.server.config.get`, `ctx.cap.mail.send` | portiert, 3 Tests |
| `kundenpipeline.delegate` | GEMISCHT | `:899` `handle_delegate` | `handlers/kundenpipeline.delegate.js` → `ctx.cap.agent.task.enqueue` | portiert, 2 Tests |

Offen über `ctx.cap.legacy.<command_type>`: **0 von 6**.

### Kein Befehlstyp, aber App-Fachlogik im Binary (nicht portiert, offen)

| Funktion | Rust-Ort | Art | Vorschlag |
|---|---|---|---|
| `project_inbound_messages` | `:60` | Projektion: eingehende Mails (`communication_messages`) → Vorgänge, Routing gegen `kundenpipeline_projekte`, Zuordnungs-Entscheidung | JS-Handler für ein Core-Ereignis „mail.inbound“ (Wunsch `ctx.cap.mail.inbound.list` bzw. Ereignis-Abo); nicht als Befehlstyp vorhanden, daher außerhalb dieses Auftrags |
| `enqueue_triage_task` | `:183` | Triage-Prompt + Queue-Task je zugeordnetem Vorgang | gehört zur Projektion oben; nutzt `ctx.cap.agent.task.enqueue` |
| `cards_json` | `:688` | Karten für die Brille (offene Entscheidungen + Vorgänge) | reine Leseabfrage, kann App-UI/Brille direkt per `ctx.read` |
| `get_agent_decision` | `:431` | MCP-Lesepfad (Workjet) | MCP-Kanal, nicht Browser-Befehl |

## Capability-Wünsche (an „ctox sync v4“)

| Capability | Signatur | Beispielaufruf | genutzt von |
|---|---|---|---|
| `ctx.cap.identity.users.list` | `({active, roles[], order[]}) → [{user_id, role, created_at_ms}]` | `await ctx.cap.identity.users.list({active:true, roles:['chef','admin'], order:['created_at_ms:asc','user_id:asc']})` | alle Handler, die Entscheidungen anlegen (Rust `workspace_decision_user_ids :1317` liest `business_users` direkt) |
| `ctx.cap.mail.server.config.get` | `() → {hostname, …}` | `const {hostname} = await ctx.cap.mail.server.config.get()` | `kundenpipeline.mail.send` (Absender `decisions@<hostname>`, Rückfall `ctox.local`) |
| `ctx.cap.mail.send` | `({transport:'mailserver_outbound', from, to[], subject, text, content_type, idempotency_key}) → {message_id, status}` | `await ctx.cap.mail.send({transport:'mailserver_outbound', from:'decisions@h', to:[to], subject, text, idempotency_key})` | `kundenpipeline.mail.send`; Rust baut RFC 822 selbst und ruft `SqliteStore::queue_email` – das gehört in die Capability. **Muss `message_id` zurückgeben** (wird in `mails_json` gespeichert). |
| `ctx.cap.agent.task.enqueue` | `({title, instruction, thread_key, workspace_root, priority, skill, parent_message_key, metadata}) → {task_id, thread_key}` | siehe `kundenpipeline.delegate.js` | `kundenpipeline.delegate` (Rust `channels::create_queue_task`, `QueueTaskCreateRequest`) |

## Abweichungen vom Rust-Verhalten (bewusst, klein)

1. **Rechteprüfung**: Rust ruft `enforce_command_policy` mit `DataWrite` auf `kundenpipeline_vorgaenge` (bzw.
   `_entscheidungen` für request/resolve). Im Handler entfällt das; der Core prüft jede `ctx.put` gegen Akteur und
   Collection. Folge: `triage.write`, `mail.send`, `delegate`, `answer` brauchen in v4 Schreibrecht auf **beide**
   Collections, die sie schreiben (Rust prüfte nur eine).
2. **Akteur**: `session.user.id` → `command.actor.id`; Ersatznamen wie in Rust (`decision-hub-agent` für request,
   `owner` sonst, `triage-agent` fest für triage.write).
3. **Zweitspeicher**: `load_any_record` fiel auf `business_records` zurück; v4 kennt nur `ctx.get`. Geprüft wird
   weiterhin `is_deleted !== true`.
4. **Idempotenzschlüssel für Mail**: Rust hatte keinen (doppelte Befehle → doppelte Mail). Der Port gibt
   `ctx.cap.mail.send` einen Schlüssel `<command.id>:kundenpipeline.mail.send:<vorgang>:<art>` mit. Ändert nur
   Wiederholungen, nicht den Erstversand.
5. **Message-ID** erzeugt Rust lokal (`<uuid@hostname>`); der Port übernimmt sie aus dem Capability-Ergebnis.
6. **Fehlercodes**: Rust kennt nur `anyhow`-Text. Zuordnung: Pflichtfeld/Format → `invalid`, unbekannter
   Datensatz → `not_found`, Schlüssel-/Antwortkonflikt und „not open“ → `conflict`, Fristablauf → `expired`.
   Die Meldungstexte sind wörtlich übernommen.
7. `run_acceptance_followup`: Folgefehler werden als `folge.error` berichtet (wie Rust, Text ohne Fehlerkette).

## Befunde (für den App-Port / Schema)

- **B1 Schema vs. Ingest**: `project_inbound_messages` schreibt `triage_json: null` und `run_json: null`
  (`decision_hub.rs:143–144`), `collections.schema.json` erlaubt dort nur `object`. Unter v4-Schemaprüfung scheitert
  jeder Rückschrieb eines solchen Vorgangs (`schema-befund.test.mjs`). Lösung im Schema-Port: `type:['object','null']`
  oder Ingest ohne die Felder.
- **B2 Projekte-Feldnamen**: Rust-Routing liest `adressen_json`, `domains_json`, `aktiv` (`route_sender :1224`), das
  Schema deklariert `adressen`, `domains`, `active`. Betrifft nur die (nicht portierte) Ingest-Projektion; der
  Delegate-Handler nutzt nur `code_projekt`.
- **B3 FNV-Startwert**: `deterministic_id :1376` nutzt `1469598103934665603` statt des FNV-Standards
  `14695981039346656037` (eine Ziffer fehlt). Der Port bleibt treu, sonst ändern sich alle Vorgangs-/Karten-IDs.
- **B4 triage.write überschreibt Karten**: jede Triage schreibt die Karte `kpl-e-triage-…` neu mit `status:'offen'`,
  auch wenn sie schon beantwortet war (Rust-Verhalten, treu übernommen; vermutlich ungewollt).
- **B5 Fingerprint**: `request_fingerprint` = SHA-256 über das normalisierte JSON in Rust-Schlüsselreihenfolge
  (serde_json mit `preserve_order`, `Cargo.lock` zieht `indexmap`). Der Port baut dieselbe Reihenfolge; Altbestand
  bleibt idempotent. SHA-256 in reinem JS (`_lib/hub.js`), weil der vm-Kontext kein `node:crypto` hat – gegen
  `node:crypto` getestet.

## Unklarheiten

- `command.record_id` steht nicht im festen Handler-Vertrag; dieser App-Port braucht ihn nicht.
- Relative Importe innerhalb von `handlers/` (`./_lib/hub.js`, Querimporte zwischen Handlern für die Annahme-Folge)
  müssen von der App-Laufzeit erlaubt werden.
- Ob `ctx.cap.identity.users.list` oder ein Plattform-Lesezugriff auf `business_users` der richtige Weg ist,
  entscheidet die Capability-Schicht.


## Kernform der Capabilities (Strang CAPSIG, 15.10.2026, §16.30/§16.32)

| Capability | Aufruf (Kernform) |
|---|---|
| `mail.send` | `({account_id: decisions@<hostname>, to, subject, text, idempotency_key})` → `{message_id, status}` |
| `agent.task.enqueue` | flach `({title, instruction, thread_key, priority, payload:{source, vorgang_id, workspace_root}, idempotency_key})` |

Ergebnis des Auftrags schreibt der Agent selbst per `kundenpipeline.triage.write` zurück (kein Rückruf nötig).


## Strang SKL-B4 (15.10.2026): Triage-Auslöser, App-Skill, Templates, Rückrufe

| Auslöser vorher | nachher |
|---|---|
| Rust `decision_hub.rs:60 project_inbound_messages` + `:183 enqueue_triage_task` (Prompt in Rust, Skill `decision-hub-triage`, Agent schreibt per CLI `ctox business-os commands dispatch … kundenpipeline.triage.write`) | Auslöser-Handler `handlers/kundenpipeline.mail.inbound.js` auf `communication_messages` (insert) → Vorgang (bzw. Zuordnungskarte) + `agent.task.enqueue({skill:'kundenpipeline/triage', on_complete:'kundenpipeline.triage.write', on_failed:'kundenpipeline.triage.failed'})`, Template `prompts/triage.md` |
| `kundenpipeline.delegate` ohne Skill und ohne Rückruf, Agent sollte per CLI `triage.write` mit leerem `triage:{}` senden (überschrieb `triage_json`) | `delegate` mit Template `prompts/delegate.md`, `on_complete:'kundenpipeline.auftrag.complete'`, `on_failed:'kundenpipeline.auftrag.failed'`; CLI-Rückschrieb und `triage:{}` gestrichen |

- **Neue Handler:** `kundenpipeline.mail.inbound` (Auslöser, Port `:60`/`:183`), `kundenpipeline.triage.failed`,
  `kundenpipeline.auftrag.complete` (Ergebnis → `run_json.zusammenfassung`, Status `ergebnisVorliegend`, Karte
  `mailfreigabe` – genau, was `decision.answer`/`mail.send` schon erwarten), `kundenpipeline.auftrag.failed`.
  `kundenpipeline.triage.write` nimmt zusätzlich die Rückruf-Form `{task_id, status, result}` (vorgang_id aus der
  Aufgabe, Vorschlag = erstes JSON-Objekt der Antwort); direkte Form `{vorgang_id, triage}` bleibt.
- **Idempotenz:** Auslöser (Vorgang existiert → nichts), triage.write (nur bei Status `zugeordnet` und passender
  `triage_task_id`), triage.failed (`triage_fehler.task_id`), auftrag.* (`run_json.task_key` = task_id und
  `run_json.status` leer). Neue Felder am Vorgang: `triage_task_id`, `triage_fehler`, `run_json.status/zusammenfassung/
  workspace_refs/fehler`.
- **Abweichungen:** Prompts wörtlich bis auf den Rückweg (CLI-Befehl → „antworte mit genau einem JSON-Objekt“ bzw.
  „antworte mit einer Zusammenfassung“). B1 gelöst: Auslöser schreibt `triage_json`/`run_json` nicht als `null`.
  B2: Routing liest `adressen`/`domains`/`active`, sonst die Rust-Namen `*_json`/`aktiv`. Triage wird vor dem
  Schreiben des Vorgangs eingereiht (Rust umgekehrt), damit kein Vorgang ohne Triage zurückbleibt.
- **Manifest-Wünsche (Generator-Lauf):** `foreign_collections.communication_messages = {owner_app:'conversations',
  ops:['read','watch']}`; `triggers:[{handler:'handlers/kundenpipeline.mail.inbound.js', on:{collection:
  'communication_messages', op:'insert'}}]` (Eintrag in `DOC_TRIGGERS`/`DOC_FOREIGN` des Generators);
  `skills:[{name:'triage'}]`, `prompts:[{name:'triage'},{name:'delegate'}]`. **Wunsch an die conversations-Seite:**
  `exports.communication_messages` muss `watch` enthalten (heute im generierten Manifest bereits `read, watch, write`;
  bitte in der Quelle der conversations-App festhalten).
- **Kern-Abbau:** `decision_hub.rs project_inbound_messages :60`, `enqueue_triage_task :183`, Aufrufer der
  Projektion; `src/skills/system/communication/decision-hub-triage/`. Baseline `check-skills`: die zwei Einträge
  `enqueue-no-callback|kundenpipeline|…delegate.js` sind erledigt (`--write-baseline` im Integrationslauf).
- Tests: `node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs` (aus `handlers/`), 32/32.
