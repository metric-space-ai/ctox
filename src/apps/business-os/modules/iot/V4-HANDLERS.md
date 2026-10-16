# iot – v4-Handler (Strang IOTAPP, Shell v4)

Stand 15.10.2026. Vorgänger: Strang H8 (alle 17 Typen delegierten an Core-Capabilities `iot.<op>`). Jetzt gilt die
Owner-Regel „App-Logik nie im Core“: das IoT-Fachmodell (Assets, Attribute, Typen, Alarme, Regeln, Agenten-Konfiguration,
Dashboards, Widgets, Wächter) lebt in dieser App. Fachliche Referenz: `src/core/iot/{commands,model,store,alarms,
conditions,watcher,widget_runtime}.rs` @ main@4794af03d (`// ref:` an jeder übernommenen Stelle).
Handler-Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`; Fehler `throw {code, message}`.
Tests: `cd modules/iot/handlers && node --test tests/*.test.mjs` (53 Tests, davon 12 in `tests/rules.test.mjs` für die Regelbestandteile, 11 in `tests/webhook.test.mjs` für den Webhook-Eingang und 8 in `tests/agenttask.test.mjs` für die Alarm-Aufträge mit Rückruf; Mutationsbeleg `node modules/iot/handlers/tests/mutation-proof-skl.mjs` (13/13 rot); In-Memory-ctx `tests/_ctx.mjs` validiert gegen
`v4/schemas/iot.collections.schema.json`).

**Stand: 17 von 17 Befehlstypen + Eingangsbefehl `ctox.iot.webhook.ingest` + 2 Rückrufe `ctox.iot.task.complete`/`.failed` + 2 Trigger-Handler als JS-Fachlogik, 0 `ctx.cap.legacy`, 0 Aufrufe der früheren iot-Capabilities.**

## Befehlstypen

| # | Befehlstyp | Rust-Referenz | Logik in der App | OS-Primitive |
|---|---|---|---|---|
| 1 | `ctox.iot.asset.upsert` | `commands.rs:604` | Typ-Deskriptor zuerst, `Asset::new_with_type`, Attribute bleiben beim Update, Attribut-Deckel 1000 | – |
| 2 | `ctox.iot.asset.delete` | `commands.rs:728`, `store.rs:400` | Realm-Prüfung, Asset + Attributzeilen löschen | – |
| 3 | `ctox.iot.attribute.write` | `commands.rs:797`, `store.rs:467` | Typauflösung, Koerzierung, Zeitnormierung, Outdated-Regel (§2A.1-3) | `knowledge.query` `data append` (Datenpunkt, `_timeseries.js`) |
| 4 | `ctox.iot.datapoints.query` | `commands.rs:902` | Realm, Formprüfung, Formen all/interval/lttb (datapoints.rs portiert, `_timeseries.js`), Fensterzeile `iot_datapoints` | `knowledge.query` `data select` |
| 5 | `ctox.iot.alarm.update` | `commands.rs:1027`, `alarms.rs:60` | Aktionen ack/progress/resolve/close/assign/status, Übergangsregeln | – |
| 6 | `ctox.iot.ruleset.save` | `commands.rs:1089` | Upsert, Realm-Schutz, Vorgabe `enabled=true` | – |
| 7 | `ctox.iot.ruleset.toggle` | `commands.rs:1145` | fehlend/fremd = „ruleset not found“ | – |
| 8 | `ctox.iot.agent.configure` | `commands.rs:1773` | Agent + Status `unconfigured` (bestehender `link_state` bleibt) | `net.ingest.subscribe` |
| 9 | `ctox.iot.dashboard.upsert` | `commands.rs:1262` | Upsert, Realm-Schutz | – |
| 10 | `ctox.iot.dashboard.delete` | `commands.rs:1304` | löscht zugehörige Widgets mit | – |
| 11 | `ctox.iot.widget.upsert` | `commands.rs:1401` | Selbstreparatur-Tor `armed`/`needs_attention` | – |
| 12 | `ctox.iot.widget.delete` | `commands.rs:1462` | Realm-Schutz | – |
| 13 | `ctox.iot.widget.arrange` | `commands.rs:1487` | x/y/w/h als Ganzzahl Pflicht | – |
| 14 | `ctox.iot.widget.compile_trigger` | `commands.rs:1676` | Prompt, Prüfung des Wächters, Rückschreiben | `model.run` |
| 15 | `ctox.iot.widget.generate_render` | `commands.rs:1702` | Prompt, Rückschreiben `render_code` | `model.run` |
| 16 | `ctox.iot.widget.pause` | `commands.rs:1526` | `paused` Vorgabe true, Fortsetzen prüft Wächter neu | – |
| 17 | `ctox.iot.webhook.register` | `commands.rs:2498`, `webhook.rs:150-196` | Bindungsform `<asset>::<attr>`, Token (128 Bit) einmalig im Ergebnis, nur sha256 in `iot_agents` (kind `webhook`), Neu-Registrierung erneuert das Token; Realm nie aus dem Payload | `net.ingest.webhook.register` (§16.38) |
| 18 | `ctox.iot.webhook.ingest` (Eingang, zugestellt von `net.ingest`) | `webhook.rs:218-245` handle_http, `:60-80` ingest, `service.rs:4563-4620` | Token-Header konstant-zeitig prüfen, Körper inline oder `body_ref`, `value_path`, Wert ≠ null, `attributeWrite` (Datenpunkt über `_timeseries.js`), idempotent je `event_id`, Antwort `result.response` | `ctx.files.get` (nur bei `body_ref`) |

Bibliotheken: `handlers/_iotcore.js` (Modell, Zeilenbauer, Alarm-Lebenszyklus, Wächter, Regelprädikate),
`handlers/_webhook.js` (Webhook: Bindungsform, value_at_path, constant_eq, Token, Antworten),
`handlers/_codegen.js` (Prompts, Antworttext), `handlers/_base.js`. `_iot.js` (Delegation) ist entfernt.

Realm: `sessionRealm` = immer `master` (Phase-2-Modell, `commands.rs:567`), nie aus dem Payload; fremder Realm = „nicht
gefunden“. Auth: der Core erzwingt `command_rights` vor dem Handler; der Handler verlangt nur einen Akteur (`iot: forbidden`).

## Trigger

| Handler | Auslöser | Inhalt |
|---|---|---|
| `handlers/ctox.iot.signal.evaluate.js` | `on: {collection: iot_attributes, op: insert}` und `{…, op: update}` | Regelauswertung (`conditions.rs:702` evaluate_event, `:944` emit_matches) und Widget-Wächter (`widget_runtime.rs:181` tick_widget) |
| `handlers/ctox.iot.rules.tick.js` | `every: 'PT1M'` | Zeitbasierte Regelbedingungen ohne neues Signal: Ablauf eines Dauerfensters (nur Assets mit offenem Fenster) und `timestamp_older_than` (alle Assets des Realms); dieselbe Auswertung `_iotcore.evaluateRuleset`, Ereignis ohne Attribut |

Manifest-Eintrag für den Generator (Tabelle `DOC_TRIGGERS`, Strang Manifeste):
`iot: { basis: 'modules/iot/V4-HANDLERS.md „Trigger“', triggers: [{ handler: 'handlers/ctox.iot.signal.evaluate.js', on: { collection: 'iot_attributes', op: 'insert' } }, { handler: 'handlers/ctox.iot.signal.evaluate.js', on: { collection: 'iot_attributes', op: 'update' } }, { handler: 'handlers/ctox.iot.rules.tick.js', every: 'PT1M' }] }`.
Außerdem entfällt in `DYNAMIC_CAPS` die Regel `./_iot.js → iot.<op>` (kein Handler importiert `_iot.js` mehr).

Regeln (`JsonRule`, `conditions.rs:34-204`): Wertprädikate eq/neq/greater_than/greater_equal/less_than/less_equal/between/
contains/regex/is_null/not_null, `previous_value` (gegen den Vorwert des Ereignisattributs, sonst null), Meta-Prädikate
`attribute.meta: [{name, value?}]` (UND, je anyMatch über die Attribut-Meta, `:509-524`), `attribute.timestamp_older_than`
(ms; erfüllt ab `timestamp <= now - dur`, fehlendes Attribut = Zeitstempel 0, `:492-499`), Dauerfenster `duration` (ms;
Prädikat muss ab dem ersten Wahr `dur` lang halten, Grenze inklusive, Falsch löscht das Fenster, kein Fortsetzen,
`:580-666`; Zustand `match_state[asset].windows[pfad] = first_match_ms`, Pfad `"0"`, `"g0.1"` …), `track_unmatched`
(nur AND, nur wenn eine anwendbare Bedingung Unmatched – nicht Pending – ist, nur neu Unmatched: Bericht `otherwise`,
Filter `match_state[asset].unmatched`, Wiedertreffer setzt zurück; wie `emit_matches :964-968` kein Alarm, `:856-903`),
Asset-Filter (`asset_ids`, `asset_types`, `realm`; nicht anwendbare Bedingungen fallen aus AND/OR heraus, keine
anwendbare → Regel unberührt, `:739-761`), AND/OR mit verschachtelten Gruppen. Formprüfung wie serde (`duration`/
`timestamp_older_than` Ganzzahl, `meta` Liste, `attribute.name` Pflicht): kaputte Regel wird übersprungen (`:726-733`).
Regeln mit Zeitbedingung werten bei jedem Ereignis des Assets aus (wie Rust), andere nur bei erwähntem Attribut. Feuern: ohne
`recurrence.mins` nur beim Übergang nicht erfüllt → erfüllt, `mins: 0` jedes Mal, `mins > 0` Sperrfenster je Asset
(`scope: GLOBAL` je Regel). Treffer → Alarm „IoT: <regel> on <asset>“ (Schwere aus `on_match.alarm_severity`,
Vorgabe MEDIUM), bei `on_match.enqueue_task` zusätzlich `agent.task.enqueue`. Zustand in `iot_rulesets.match_state`.

Wächter (Widget-Trigger): statt Rhai (Skript-Engine im Core) eine deklarative JSON-Spezifikation, die die App selbst
prüft und auswertet – `{"when": <ausdruck>, "reason": "…", "for_ms"?: n}`, Ausdrücke `gt|gte|lt|lte|eq|neq|and|or|not`,
Werte `last|avg|min|max|count|rate|age_ms|has_data` mit Fenster `ms|s|m|h|d`. Deckel wie `watcher.rs:32-37`
(64 KiB, Schachtelung 32). Feuern → `fired`, Alarm „IoT-Auftrag ausgelöst: <signal>“ und `agent.task.enqueue` mit dem
Seed-Prompt aus `widget_runtime.rs:137`. Ungültiger Wächter → `needs_attention`, nie Abbruch.

## Capabilities

Neuer Bedarf der App (nur OS-Primitive aus `v4/os-primitives/CATALOG.md`); `iot.*` entfällt vollständig:

| Capability | Aufruf | Handler |
|---|---|---|
| `ctx.cap.model.run` | `model.run({input, metadata:{kind, widget_id, signal_ref, skill}}) → Responses-Antwort` | widget.compile_trigger, widget.generate_render |
| `ctx.cap.knowledge.query` | Kernform §16.30 flach `({argv:['data', …]})` = `ctox knowledge data …` (Parquet-Tabelle `iot`/`iot_timeseries`, dieselbe Engine wie `dataframe.window`): `append --rows <json>` (Datenpunkt; bei fehlender Tabelle `describe` → `create` → `append`), `select --where asset_id=… --where attribute_name=… --where timestamp_ms>=… --where timestamp_ms<=… --order-by timestamp_ms[:desc] --limit N` | attribute.write, datapoints.query, signal.evaluate |
| `ctx.cap.net.ingest.subscribe` | `({endpoint, protocol:'mqtt'|'http'|'websocket', enabled, config})` | agent.configure |
| `ctx.cap.net.ingest.webhook.register` | Kernform §16.38 Nr. 1 + Ergänzung: `({path:'webhook', command:'ctox.iot.webhook.ingest', pass_headers:['x-webhook-token'], default_response:{status:202}, aliases:['/ctox/iot/webhook/']}) → {ingest_url}` (idempotent je App+Pfad) | webhook.register |
| `ctx.files.get` | Handler-Vertrag (§16.32, Spiegel von `api.files.get`): `(ref) → Blob` für `body_ref` > 64 KiB | webhook.ingest |
| `ctx.cap.agent.task.enqueue` | §16.30: `({title, instruction, skill:'iot/operations', thread_key, payload:{alarm_id,…}, idempotency_key:'iot-alarm:<alarm_id>', on_complete:'ctox.iot.task.complete', on_failed:'ctox.iot.task.failed'})` | signal.evaluate (Regeln über `_iotcore.evaluateRuleset`, Widget-Wächter) |
| `ctx.cap.agent.task.get` | `({task_id}) → {payload:{alarm_id}}` (Alarm zum Rückruf finden) | task.complete, task.failed |

Kern-Bedarf dazu: `net.ingest` stellt Protokoll-Agenten-Telemetrie als Befehl `ctox.iot.attribute.write`
`{asset_id, name, value, timestamp_ms}` an die App zu (wie `schedule` Befehle feuert). Webhooks laufen über die Kernform
(Abschnitt „Webhook-Eingang“). Den Verbindungszustand eines
Endpunkts (`iot_agent_status.link_state`) liefert der Kern über einen generischen Statusbefehl an die App (offen, siehe
unten).

CAPSIG2 (15.10.2026): `dataframe.table.import` und `dataframe.query` gibt es im Kernvertrag nicht (`core-contract.mjs`: nur
`dataframe.window` mit leerem filter/sort). Schreiben und Lesen der Zeitreihe laufen jetzt über `knowledge.query`; die
Abfrageformen sind App-Logik: `_timeseries.js` portiert `datapoints.rs` (fetch_range mit Grenze 100 000 + Kennzeichen
`truncated`, nearest, interval mit Epochen-Buckets/LOCF/führender Lücke, LTTB inkl. Abweisung nicht-numerischer Werte).
Spalte `value_json` (kanonisches JSON wie Rust `value TEXT`), weil eine Parquet-Spalte nur einen Typ hat. Zahlartige
asset_id/attribute_name: der Kern parst `col=val` als Zahl (parquet_io.rs parse_value) → Abfrage mit `~` (String,
Teilzeichenkette) und exaktem Nachfilter in der App. Ausgabeform der Punkte bleibt `{timestamp, value}` (interval:
timestamp = Bucket-Beginn, value null in der führenden Lücke). Kernwunsch (keine Abweichung mehr): `dataframe.window`
mit Filter/Sortierung bzw. ein Zeitreihen-Append würde die Kopplung an die Knowledge-CLI-Argumente ablösen.

## Alarm-Aufträge mit Rückruf (Strang SKL-A4, Kern §16.30)

Auslöser vorher → nachher: Regeltreffer mit `on_match.enqueue_task` (`_iotcore.js` evaluateRuleset) und gefeuerter
Widget-Wächter (`ctox.iot.signal.evaluate.js` tickWidgets) riefen `agent.task.enqueue` mit dem Core-Skill `iot-operations`
(bzw. `on_match.suggested_skill`) **ohne Rückruf** auf (Bestandsaufnahme F, R9: fire-and-forget; der Agent quittierte den
Alarm laut Skill selbst per `ctox iot alarm ack`). Jetzt: Skill `iot/operations` aus dem Paket, `idempotency_key`
`iot-alarm:<alarm_id>`, Rückrufe `ctox.iot.task.complete` / `ctox.iot.task.failed`. Prompts bleiben Collection-Daten
(`on_match.task_prompt`, `widget.action_prompt` im Seed-Prompt `widget_runtime.rs:137`, wörtlich, nur der Skill-Name im
Schlusssatz heißt jetzt `iot/operations`). `on_match.suggested_skill` wird nicht mehr als Skill-Name übernommen (fremder
Namensraum/Core-Skill wäre nach §16.30 unzulässig). Auch `_codegen.js` (model.run-Metadatum) nennt `iot/operations`.

- Skill: `skills/operations/SKILL.md` (Kopie von `src/skills/system/host_ops/iot-operations`, zugeschnitten): Antwort ist
  genau ein JSON-Objekt `{diagnose, massnahme, alarm: acknowledge|resolve|keep_open, schreibvorschlaege:[{asset_id,
  attribute, value, grund}], notizen}`; keine CLI-/MCP-Rückschreibwege, keine Gerätschreibung durch den Agenten.
- `ctox.iot.task.complete` (`_agenttask.js`): Alarm über `agent.task.get` → `payload.alarm_id`; Urteil als
  `iot_alarms.data.agent_task {task_id, status:'completed', alarm_decision, diagnose, massnahme, notizen,
  schreibvorschlaege, finished_ms}`; Entscheidung mit den Übergangsregeln (`alarms.rs:60-75`): `acknowledge` (Vorgabe, auch
  bei Freitext-Antwort) nur aus OPEN, `resolve` wenn erlaubt, `keep_open` lässt den Status. Schreibvorschläge werden nur
  abgelegt (gefiltert, ≤ 20), nie ausgeführt.
- `ctox.iot.task.failed`: `data.agent_task {task_id, status:'failed'|'cancelled', error, finished_ms}`, Alarmstatus bleibt
  (offen für den Menschen). Abbruch kommt als on_failed mit `status:'cancelled'`.
- Idempotent: liegt am Alarm schon eine terminale Spur derselben task_id, ändert ein weiterer Rückruf (complete oder failed)
  nichts; gelöschter Alarm → ohne Wirkung. Lesen–Prüfen–Schreiben in einer Transaktion.
- Manifest (Strang Manifeste, außerhalb dieses Arbeitsorts): `handlers` braucht `ctox.iot.task.complete` und
  `ctox.iot.task.failed` (Rückrufe, kein UI-Aufruf), `skills:[{name:'operations', path:'skills/operations/SKILL.md'}]`,
  `capabilities_required` `agent.task` (enqueue, get).
- Kern-Abbau: `src/skills/system/host_ops/iot-operations/` ist für iot tot (Paketkopie ist Quelle der Wahrheit);
  `mission/channels/mod.rs:4849 ingest_iot_event_message`, `iot/conditions.rs:1025-1066` und `iot/widget_runtime.rs:289`
  (Queue-Task je Treffer) haben keinen App-Aufrufer mehr.

## Webhook-Eingang (Strang IOTIN, Kernform §16.38)

- Anmelden: `ctox.iot.webhook.register {id?, signal_ref, value_path?}` ruft `net.ingest.webhook.register({path:'webhook',
  command:'ctox.iot.webhook.ingest', pass_headers:['x-webhook-token'], default_response:{status:202},
  aliases:['/ctox/iot/webhook/']})` → `{ingest_url}` (§16.38-Ergänzung, SKL-A4: `command` = Befehlstyp der Zustellung, ohne
  Handler im Manifest antwortet der Kern `rejected{invalid}`; `aliases` hält die Rust-URL `/ctox/iot/webhook/<id>` für
  bestehende Kundengeräte erreichbar, Webhook-ID bleibt der Teilpfad; `X-Webhook-Token` kommt nur über `pass_headers`) und liefert `{id, token, ingest_path,
  ingest_url, header:'X-Webhook-Token', signal_ref}`; `ingest_url`/`ingest_path` = `<ingest_url>/<id>` (absolut). Webhook-IDs
  nur `[A-Za-z0-9._~-]{1,160}` (Teilpfad ohne Kodierung). `pass_headers` ist nötig, weil §16.38 Nr. 2 sonst nur
  Cookie/Authorization entfernt – der Signatur-Header ist in Rust `X-Webhook-Token` (`webhook.rs:192`, `service.rs:4582`).
- Eingang: der Kern stellt jede Anfrage auf `<ingest_url>/<id>` als Befehl **`ctox.iot.webhook.ingest`** an iot zu,
  payload `{event_id, method, path, sub_path, query, headers, received_at_ms, body? | body_ref?:{ref}}`.
  Ablauf (ref `webhook.rs:218-245`, `:60-80`): nur POST (sonst 404 `{"error":"not found"}`); Webhook-ID = letztes Segment
  von `sub_path`; Bindung fehlt/deaktiviert, Header fehlt oder Token falsch → 401 `{"ok":false}` ohne Unterscheidung
  (Vergleich `constant_eq` über sha256-Digests); dann Idempotenzanspruch; Körper `body` (UTF-8) bzw. `body_ref` über
  `ctx.files.get` (≤ 1 MiB, ungültiges UTF-8 → 400 `bad request`), JSON-Fehler/leer → 400 `invalid json`; Wert an
  `value_path` (Punkt-Pfad nur durch Objekte, wie serde `Value::get(&str)`), fehlend/null → 400 `{"ok":false}`;
  `attributeWrite` (Koerzierung, Datenpunkt über `_timeseries.js`, Attribut-/Asset-Zeile; Regeln/Wächter laufen über den
  Trigger `ctox.iot.signal.evaluate`) mit `timestamp_ms = received_at_ms`; Fachfehler → 400 `{"ok":false}`; Erfolg →
  202 `{"ok":true}`. Ergebnis `{response:{status, headers:{'content-type':'application/json'}, body}, event_id, …}`.
- Idempotenz je `event_id`: Fenster der letzten 256 Ereignisse in `iot_agents[webhook:<id>].data.events`
  (`{id, status, at_ms}`), Anspruch in einer Schreibtransaktion vor dem Schreiben. Wiederzustellung → dieselbe Antwort,
  `duplicate:true`, kein zweiter Datenpunkt; läuft das Ereignis noch → 202. Vorübergehende Fehler (`unavailable` aus der
  Zeitreihe, `body_ref` nicht lesbar) geben den Anspruch frei, damit eine erneute Zustellung neu verarbeitet.
- Manifest (Strang Manifeste, außerhalb dieses Arbeitsorts): `handlers` braucht den Eintrag
  `"ctox.iot.webhook.ingest": "handlers/ctox.iot.webhook.ingest.js"`; `command_rights` für diesen Typ nur für die
  Zustellung durch `net.ingest` (kein UI-Aufruf).

## Kern-Import

Einmaliger Import aus dem Core-Store (`runtime/ctox.sqlite3`) in die App-Collections, geliefert vom Kern:

| Quelle im Core | Ziel-Collection | Feldabbildung |
|---|---|---|
| Tabelle `iot_assets` (+ `iot_attributes`) | `iot_assets` | `id`, `realm`, `parent_id`, `asset_type`, `name`; `data` = serialisiertes `Asset` (Spalte `data`, Attribute aus `iot_attributes` eingehängt); `index_text` = name klein, `sort_key` = name |
| Tabelle `iot_attributes` | `iot_attributes` | `id` = `<asset_id>:<name>`; `asset_id`, `attribute_name`=name, `value_type`, `timestamp_ms`, `data` = `{name, value_type, value, timestamp, meta}`, `status_key` = value_type |
| Tabelle `iot_asset_types` | `iot_asset_types` | `id` = `asset_type`; `data` = Spalte `data`; `attribute_count` |
| Tabelle `iot_alarms` | `iot_alarms` | `id`; `data` = Spalte `data` (Alarm); `status`/`status_key` = OPEN…CLOSED, `severity` = LOW/MEDIUM/HIGH, `source`, `created_ms`, `sort_key` = created 20-stellig |
| Tabelle `iot_rulesets` | `iot_rulesets` | `id`, `realm`, `name`, `enabled` (0/1 → bool), `data` = JSON der Spalte `data` (null → `{}`), `last_fired_ms`; `match_state` aus `iot_condition_state`/`iot_recurrence_block`: je Asset `{matched` = MAX(previously_matched), `fired_ms` = `iot_recurrence_block.last_fired_ms` (Schlüssel `<ruleset>:<asset>`; GLOBAL-Schlüssel `<ruleset>` → Eintrag `*`), `windows` = `{"<condition_index>": first_match_ms}` für alle Zeilen mit first_match_ms ≠ NULL (weglassen, wenn leer), `unmatched: true` falls eine Zeile previously_unmatched = 1 hat`}` |
| Tabelle `iot_agents` | `iot_agents` | `id`, `realm`, `name`, `kind`, `enabled`, `data` (null → `{}`) |
| Tabelle `iot_agent_status` | `iot_agent_status` | `id` = `agent_id`, `link_state`, `last_event_ms`, `error` (bereinigt, Redaktionsvertrag `commands.rs:259-265`) |
| Tabelle `iot_dashboards` | `iot_dashboards` | Spalten 1:1, NULL-Spalten weglassen |
| Tabelle `iot_widgets` | `iot_widgets` | Spalten 1:1, `trigger_state` als JSON-Objekt; `trigger_code` (Rhai) bleibt erhalten → beim ersten Datenpunkt `needs_attention`, CTOX erzeugt den JSON-Wächter neu (`compile_trigger`) |
| Tabelle `iot_datapoints` (Rohdatenpunkte) | Knowledge-Datentabelle `iot`/`iot_timeseries` | `asset_id`, `attribute_name`, `timestamp_ms`, `value_json` = Spalte `value` (JSON-Text unverändert) |
| Tabelle `iot_webhooks` (+ Secret-Store `iot_webhook`) | `iot_agents` | `id` = `webhook:<id>`, `realm`, `name` = `Webhook <signal_ref>`, `kind` = `webhook`, `enabled` = true, `data` = `{webhook_id, signal_ref, value_path?, token_sha256` = sha256-Hex des Secret-Werts `read_secret_value('iot_webhook', id)`, `path: 'webhook', events: []}`; `ingest_url` setzt die erste Neu-Registrierung (alte Ingest-URLs `/ctox/iot/webhook/<id>` sind kein §16.38-Alias) |

Danach entfallen im Core: `src/core/iot/commands.rs` (Ops, Projektion `project_record` für `iot_*`), `conditions.rs`,
`watcher.rs`, `widget_runtime.rs`, die Fachtabellen in `store.rs`/`alarms.rs`; die Protokoll-Agenten (`agents/*`) werden
Teil von `net.ingest`; aus `webhook.rs` bleibt im Kern nur der HTTP-Eingang (`net.ingest`, §16.38), Token/Bindung/
ingest sind App-Logik (`ctox.iot.webhook.*`); `datapoints.rs` wird Teil von `dataframe`.

## Abweichungen (bewusst) und Offen

- Wächter-Sprache JSON statt Rhai: entschieden (keine Skript-Engine im Core; die App wertet selbst aus). Importierte
  Rhai-Wächter laufen über das Selbstreparatur-Tor in `needs_attention` (Kern-Import-Hinweis oben, Tabelle `iot_widgets`).
- Zeitbedingungen werden zusätzlich ohne neues Signal über `every: PT1M` ausgewertet (Rust nur ereignisgetrieben);
  Auflösung damit 1 Minute für Fensterablauf/Veralten ohne Datenpunkt.
- Verschachtelte Gruppen (`when.groups`) wertet die App aus; Rust ignorierte sie in `evaluate_event` (nur Wurzel-items).
- Angeglichen an Rust (IOT2): fehlendes Attribut = Wert null/Zeitstempel 0 statt „nicht erfüllt“; `previous_value` eines
  Nicht-Ereignisattributs prüft gegen null; nicht anwendbare Bedingungen (Asset-Filter) zählen nicht mehr als falsch.
- Codegen synchron über `model.run` statt Queue-Task mit Rückschreiben per CLI; Ergebnis `{widget_id, kind,
  trigger_status?, projections}` statt `{queued, …}`. Modell nicht erreichbar → Befehl scheitert mit `unavailable`,
  nichts geschrieben.
- `data` von Regeln/Agenten ist ein Objekt (Collection-Schema); fehlend/null → `{}`, andere JSON-Typen → `invalid`.
- `dashboard.delete` entfernt die Widget-Zeilen (Rust ließ deren Projektionen verwaist stehen).
- `created_at_ms` bleibt beim Update erhalten (Rust setzte es bei jeder Projektion neu).
- Offen (Kern): Statusrückmeldung der Agenten (`link_state`) braucht einen generischen Statusbefehl von `net.ingest` an
  die App.
- Webhook-Token nur als sha256 in `iot_agents` statt Klartext im Secret-Store (Rust); der Vergleich bleibt konstant-zeitig.
- Webhook: Token-Prüfung vor dem Lesen/Parsen des Körpers (Rust parste das JSON zuerst): ungültiges JSON mit falschem Token
  → 401 statt 400; unautorisierte Anfragen lösen kein `files`-Lesen aus.
- Webhook: Zeitstempel = `received_at_ms` des Kerns statt `0` → Bearbeitungszeit (Rust); gleich bei synchroner Annahme,
  stabil bei Wiederzustellung. `body_ref` über 1 MiB → 400 (Rust las beliebig groß).
- Webhook: Neuer Eingang `ctox.iot.webhook.ingest`; offen ist nur, dass §16.38 den Befehlstyp der Zustellung nicht
  benennt – die App erwartet `ctox.iot.webhook.ingest` (Manifest-Eintrag, s. „Webhook-Eingang“).
- UI: `index.js` sendet nur Befehle (`dispatch('ctox.iot.…')`), keine Capability-Aufrufe → unverändert.

## Nicht portiert


