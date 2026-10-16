# CTOX-OS-Primitive — Zielkatalog

Stand 15.10.2026. CAPA „Shell v4“. Klon `~/Documents/ctox-shell-v4`.

**Owner-Vorgabe (Michael, 15.10., wörtlich):** „CTOX soll nur betriebssystemartige
Schnittstellen und Funktionen bereitstellen, aber eine App-Logik darf niemals in CTOX
selbst landen.“ — „nicht, dass doch für jede App am Ende ein eigener Backend-Code
geschrieben werden muss.“ Ziel: Apps sind Plug-and-Play/Hot-Plug-fähig, weil der Core
nur generische OS-Primitive anbietet und jede Fachlogik in der App (UI + JS-Handler
über eigene Collections) liegt.

**Prüfregel für „OS-artig“ (verbindlich).** Eine Core-Capability ist nur dann legitim,
wenn **alle** gelten:
1. Kein App-Name, keine App-ID im Capability-Namen.
2. Keine App-Collection und kein Fachbegriff in der Semantik (kein „lead“, „ticket“,
   „asset“, „crew“, „sellify“, „campaign“, …).
3. Austauschbar zwischen beliebigen Apps: dieselbe Capability bedient jede App gleich.
4. Fähigkeit statt Fachablauf: sie führt ein generisches Betriebsmittel aus
   (Datei, Netz, Prozess, Modell, Transport, Warteschlange, Zeit, Geheimnis, Policy),
   nicht einen fachlichen Zustandswechsel.

Was die Prüfregel verletzt, ist **App-Logik** (→ App-Handler, `MAPPING.md` (b)) oder
**Plattform** (→ `ctox-platform`-Manifest, `MAPPING.md` (c)). Grundlagen:
`ctox-sync-v4-architektur.md` §9.4 (App-Laufzeit/Fähigkeiten), §16.8 (Executor-Fence),
§16.13 (Trennregel: *Fachlogik → Handler, native Fähigkeit → generische Capability*,
CI-Wächter „keine Modul-IDs/Collection-Namen in `src/core/**`“), §16.16 (Handler-Vertrag
`ctx.cap.<name>.<fn>`), §16.21 (Capability-Erweiterungen); `shell-v4/analysis/E-befehlstypen.md`
§3 (33 Capability-Wünsche aus 403 nativen Befehlstypen).

**Abweichung vom E-Befund (bewusst):** E/die V4-HANDLERS-Dokumente stufen viele Dienste
als „FAEHIGKEIT“ ein, um den Port klein zu halten (Queue, Crew, Tickets, IoT-Engine,
Workjet-Transfer). Nach der strengen Owner-Regel sind davon nur die **generischen**
Betriebsmittel OS-Primitive; alles mit App-Collection/Fachsemantik (iot-Assets/Widgets,
Ticket-Zustandsautomat, Crew-Register, Scrape-Registry, Workjet-Bindung) ist App-Logik.
Der Katalog benennt diese Grenze hart; `MAPPING.md` führt die Verlagerung je Capability.

---

## Katalog (20 Primitive)

**[Kern]** = Form steht im Kernvertrag (CAP-1/CAP-2, Klon `clones/cap` Zweig `v4/cap-2`
@ 83d979cf4, siehe `core-contract.mjs`/`CALL-CONTRACT.md`) und ist verbindlich; **[Ziel]** =
noch kein Kernvertrag, Skizze. Signatur-Skizzen; Aufruf im Handler als `ctx.cap.<primitiv>.<fn>(…)` (§16.16), im
UI-Client über die typisierten Pendants der App-API (§10.2). Policy = Core-Prüfung
**vor** dem Handler (§16.20 Nr. 2); jede Capability ist im Manifest unter
`capabilities_required` deklariert und vom Admin je App-Artefakt freizugeben.

### 1. `files` — inhaltsadressierter Blob-Speicher
- **Signatur:** [Kern] `blob.materialize({blob_id: BlobRef}) → {blob_id, sha256}`,
  `blob.export({blob_id: BlobRef}) → {blob_id, sha256}` — kein Pfad, keine Download-URL, keine
  Formatkonvertierung; `BlobRef = {sha256, size, transfer_id}`. [Ziel, App-API §10.2a Nr. 4]
  `files.get(ref) → Blob`, `files.put(blob, meta) → {ref}`.
- **Policy:** Eigentum über `files`-Collection-Metadaten (`owner_app`); Lesen/Schreiben
  nach Nutzer- und Fremd-Collection-Rechten; Blob-Cache LRU-räumbar; Upload über Spur 4.
- **OS-artig:** Datei-/Objektspeicher ist ein klassisches OS-Betriebsmittel; kein Fach.
  Deckt `files.*`, `file.*`, `blob.*` und das Laufzeit-Artefakt `artifact.write`.

### 2. `net.http` — ausgehender HTTP-Abruf
- **Signatur:** [Kern] `web.fetch({url, find?: string[]})` → Text als CAS-BlobRef (nativer
  Web-Reader, kein Cachepfad). [Ziel] authentifizierter Abruf
  `net.http.fetch({url, method, headers?, body?, secret_ref?, timeout_ms}) → {status, text|json}`.
- **Policy:** nur http(s); `public_only` löst DNS nur auf öffentliche Adressen auf (auch
  bei Weiterleitungen); authentifizierter Abruf nur mit Allowlist je App + `secret_ref`
  (Core setzt den Header, die App sieht das Geheimnis nie). Kein Datensatz in URL/Query.
- **OS-artig:** generischer Netzzugriff; vereint `web.fetch` (öffentlich) und `http.fetch`
  (allowlistet/authentifiziert).

### 3. `browser` — verwaltete Browser-Sitzung
- **Signatur:** `browser.session.start({profile?}) → {id}`, `browser.navigate({session,url})`,
  `browser.op({session, tab, op, params})`, `browser.automation.run({source, timeout_ms})`,
  `browser.capture({query|urls, auth_profile?, extract_schema}) → {captures[]}`,
  `browser.auth_assist.request({…}) / .complete({request_id})`, `browser.session.stop({session})`.
- **Policy:** eigener Transfer-/Fence-Rahmen; Automations-Skript und Extraktionsschema sind
  **App-Daten**, nicht Core-Logik; Realm/Owner aus der Sitzung.
- **OS-artig:** Browser-Engine als Gerät. Deckt `browser.*`, `web.capture`, `research.person`
  (Personen-Recherche = Browser-Capture-Werkzeug).

### 4. `process.run` — sandboxed Prozess/Werkzeug
- **Signatur:** `process.run({program: allowlisted, args[], cwd_ref, stdin_ref?, timeout_ms}) → {exit, stdout_ref, stderr_ref}`.
- **Policy:** nur Allowlist-Programme je App; Ausführung im geleasten Workspace unter
  Executor-Fence (§16.8); kein Netz-/FS-Zugriff außerhalb des Workspace.
- **OS-artig:** Prozessstart ist das OS-Primitiv schlechthin. Deckt Pentest-Werkzeuge
  (`pentest.*` — die Engine ist ein Werkzeugläufer), Scrape-Skriptläufe
  (`scrape.execute_test|run_file|reprobe_keep_script`), die Knowledge-CLI
  (`knowledge.command`) und externe SQL-Sync-/Schreibwerkzeuge (`external_sql.*`).

### 5. `mail.transport` — Mailserver, Versand/Empfang, Mailbox-Speicher
- **Signatur:** [Kern] `mail.send({account_id, to[], cc?, subject, text?, html?, attachments?: BlobRef[], thread_ref?, idempotency_key}) → {message_id, status}`
  (`account_id` = konfigurierte Absenderadresse; kein `from`, kein `tracking`);
  `mail.server.{config.get, domain.save, runtime.save, domain.delete, user.save, user.delete}`;
  `mail.channel.{test, sync, settings.save, pair.start, disconnect, jami.create, jami.export, mailbox.link}`;
  `mail.read({thread_key, limit?}) → {messages}`; `mail.delivery_log.query({message_ids?, since?, limit?}) → {deliveries}`.
  [Ziel] `mail.message.upsert|annotate`, `mail.default_sender`, `mail.validate_contacts`.
- **Policy:** Passwörter nur per Secret-Intake; Server-Administration ist Infrastruktur
  (Rollen mit Mailserver-Recht); Plattform-Projektion `mail_server_state` nie mit Secrets.
- **OS-artig:** Mail-Transport und Mailbox sind Infrastruktur, kein App-Fach. Der
  Mailbox-Speicher (Empfang/Upsert) gehört dem Transportprimitiv; App-Bezüge
  (Kampagne, Lead) stehen in der Payload und in App-Collections, nicht im Primitiv.

### 6. `model` — LLM-/Embedding-/Vision-Gateway
- **Signatur:** `model.run(request) → response` (Responses-förmiger Vertrag, §System-Map).
- **Policy:** Modellwahl/Kontingent über Gateway (`src/core/execution/`); App deklariert
  Nutzung im Manifest.
- **OS-artig:** Rechen-/Inferenzdienst, app-unabhängig.

### 7. `agent.task` — dauerhafte Agentenarbeit (Enqueue + Lebenszyklus)
- **Signatur:** [Ziel] `agent.task.enqueue({title, instruction, skill?, thread_key?, payload, writeback?, reply_to?, on_complete?, on_failed?}) → {task_id}`;
  `agent.task.{get, find, update, cancel, set_metadata}`; `agent.workspace.read({task_id, path}) → {text, sha256}`,
  `agent.task.workspace_file(...)`.
- **Kernstand (Lücke):** [Kern] `agent.task.enqueue` ist heute **kein** generisches Enqueue,
  sondern ein Aktions-Alias `{action: delegate|company.research|contact_research|lead_qualification|report_bug|request|approve, payload, record_id?}`
  auf sieben Legacy-Befehle (registry.rs `cap1()`). Das generische Primitiv fehlt im Kern;
  alle eigenen Enqueues der Apps (6 Aufrufe) und `get/find/update/cancel/set_metadata`/
  `workspace_file` haben keinen Kernvertrag (CALL-CONTRACT.md).
- **Policy:** Rückkehr-Hooks unter Executor-Fence, höchstens einmal je `task_id` (idempotent);
  Workspace-Lesen kanonisiert Pfade, verweigert Ausbrüche.
- **OS-artig:** „starte bounded Arbeit, beobachte Ergebnis“ ist ein generischer Scheduler-Dienst;
  Skill/Instruction/Writeback sind App-Daten. Deckt auch `scrape.repair_task.create` (Enqueue mit `kind`).

### 8. `agent.turn` — interaktiver Agentenzug
- **Signatur:** [Kern] `agent.turn.{submit, watch, cancel, input, capabilities, history, route.*}` und
  `agent.coding.{models, turn}` jeweils `({payload, record_id?})` (CAP-1-Hülle). [Ziel, flach]
  `agent.turn.submit({thread_id, input}) → {turn_id}`, `.watch({turn_id, after})`,
  `.cancel({turn_id})`, `.history({thread_id})`; `agent.coding.turn({module_id, prompt, model?}) → {turn_id}`, `agent.coding.models() → [...]`.
- **Policy:** je Turn ein frischer Sidecar über LocalTransport, auf Drop beendet (pi-Sidecar-Muster); Fence.
- **OS-artig:** bounded Modell-Turn als Dienst; Thread/Prompt sind Daten. Deckt `agent.coding.*`.

### 9. `agent.queue` — Warteschlangen-Steuerung
- **Signatur:** [Kern] `agent.queue.control({action: abort_turn|block|capacity|pause|release|retry, payload, record_id?})`,
  `agent.queue.task.{update, delete}({payload, record_id?})` — CAP-1-Hülle, Payload = native Befehlsfelder.
  [Ziel] `agent.queue.task.{get, for_command, set_status}`, `agent.queue.projection.refresh()`,
  `agent.command.settle({command_id, status, result?})`.
- **Policy:** schreibt die Core-Queue (Wahrheit in `ctox.sqlite3`) transaktional und projiziert
  `ctox_queue_tasks` neu (read-only für Apps, §16.25 Nr. 2); `agent.command.settle` schließt den
  ursprünglichen `business_commands`-Eintrag (Plattform-Collection).
- **OS-artig:** CTOX besitzt Queueing (AGENTS.md); die Queue ist ein app-unabhängiger Daemon-Dienst.

### 10. `schedule` — zeitgesteuerte Befehle
- **Signatur:** [Ziel] `schedule.ensure({name, cron_expr, command:{type,payload,actor_id}, calendar?}) → task`,
  `schedule.{list, set_enabled, remove}`. [Kern] nur `schedule.upsert({payload, record_id?})` = der ganze
  Legacy-Befehl `news.topic.schedule` — kein generischer Planer-Vertrag.
- **Policy:** fällige Läufe feuern als Befehl mit `payload.trigger='schedule'` im Namen von `actor_id`.
- **OS-artig:** Cron/Timer-Dienst; Befehlstyp/Payload sind App-Daten.

### 11. `secrets` — Geheimnis-Speicher
- **Signatur:** `secrets.put({name, value:{secret_intake:true}})`, `secrets.{list, delete, exists}({name|scope})`.
- **Policy:** nur mit Secrets-Berechtigung; Werte verlassen den Core nie (Intake-Fluss); `exists` nie den Wert.
- **OS-artig:** Schlüsselbund/Keystore, klassisches OS-Betriebsmittel.

### 12. `directory` — Nutzer-/Akteur-Verzeichnis
- **Signatur:** `directory.user({user_id}) → {user_id, display_name, active}|null`,
  `directory.users.list({active?, roles?[], order?}) → [...]`.
- **Policy:** nur Lesesicht auf `business_users`; Entscheidung bleibt im Core/Policy.
- **OS-artig:** Benutzerverzeichnis wie ein OS-`passwd`/Directory-Dienst.

### 13. `audit` — unveränderliches Ereignisprotokoll
- **Signatur:** `audit.event({collection, record_id, event_type, payload, observed_at_ms})`,
  `audit.query({command_type_prefix?, summary?, order?}) → {events[]}`.
- **Policy:** schreibt `business_events` (keine App-Collection); Abfrage ersetzt direktes SQL.
- **OS-artig:** append-only Journal, app-unabhängig.

### 14. `config` — typisierte Laufzeitkonfiguration
- **Signatur:** `config.get({key}) → {value|null}`; `config.runtime_state.{read, write}({key, value})`.
- **Policy:** je App im Manifest freigegebene Schlüssel; ersetzt `runtime_env::env_or_config`
  (keine neuen Prozess-Env-Toggles, AGENTS.md). `runtime_state` = Instanz-Laufzeitzustand.
- **OS-artig:** Konfigurations-/Zustandsregister des Betriebssystems.

### 15. `doc` — Dokument-Engine (Extraktion/Render/Konvertierung)
- **Signatur:** [Kern] `doc.extract({blob_id: BlobRef, kind?: pdf|document|spreadsheet|text, ocr?: boolean})` → Text + native Ergebnis
  (kein `schema`, kein `source`, kein `data_base64`); `doc.html({blob_id: BlobRef, selector, attribute?, limit?})`
  (DOM über CAS, kein Inline-HTML); `office.edit.prepare({kind, blob_id: BlobRef}) → {session, …}`,
  `office.edit.apply({session, changes: BlobRef, base_version}) → {session, blob_id, sha256, …}`,
  `office.render({session, to: docx|xlsx|email_html})` (pdf nativ nicht unterstützt; kein bloßes `model`/`blob_id`),
  `office.settings.save(<native Payload>)`.
- **Policy:** Office-Formate und PDF/OCR als generische Engine; jede Datei nur als BlobRef über die App-Grenze.
- **OS-artig:** Dokument-/Konvertierungsdienst (wie ein System-„Vorschau“/Print-Stack);
  deckt `doc.*` und `office.*`. Fachliche Reports bleiben App-Logik, die diese Engine nur aufruft.

### 16. `speech` — STT/TTS
- **Signatur:** [Kern] `speech.transcribe({blob_id: BlobRef, model?, language?})`, `speech.stt.transcribe({blob_id: BlobRef, model?})`
  (per-call `language` nativ nicht unterstützt), `speech.synthesize({text, voice?, format?})` → BlobRef.
- **Policy:** Quelle nur als BlobRef; Rohpfade und `source`-Objekte verboten.
- **OS-artig:** Audio-Codec-/Sprachdienst, app-unabhängig.

### 17. `dataframe` — Abfrage über Tabellen/Parquet
- **Signatur:** [Kern] `dataframe.window({table, skip?|offset?, limit, filter?, sort?})` — `filter`/`sort` nur leer;
  `knowledge.query({payload})` (= knowledge.command, Knowledge-Payload-Form). [Ziel]
  `dataframe.table.{create, describe, import}`, `dataframe.tables.active({domain})`,
  `dataframe.snapshot.sha256({path})`, `dataframe.project()`.
- **Policy:** Metadaten als `knowledge`-/App-Collections, Daten inhaltsadressiert über `files`.
- **OS-artig:** generische Tabellen-/Zeitreihen-Abfrage; deckt `knowledge.*` (Tabellen-Engine)
  und `iot.datapoints.query` (Zeitreihen). Fachliche Tabellen-Kataloge bleiben App-Collections.

### 18. `net.ingest` — generische Geräte-/Webhook-/MQTT-Ein-/Ausgabe
- **Signatur:** `net.ingest.webhook.register({path?}) → {token, ingest_path}`,
  `net.ingest.write({endpoint, attribute, value})`, `net.ingest.subscribe({endpoint})`.
- **Policy:** rein transportiv; **kein** IoT-Fachmodell (keine Assets/Dashboards/Regeln).
- **OS-artig:** Geräte-/Nachrichten-Transport. Deckt `iot.webhook.register`, `iot.attribute.write`
  (Datenpunkt/Attribut als Telemetrie). Das IoT-Fachmodell ist App-Logik (siehe MAPPING (b)).

### 19. `policy` — Rechteprüfung
- **Signatur:** `policy.module_decision({permission:'data.read'|'data.write'|'apps.modify', module}) → {allowed, display_reason}`;
  `policy.command_access({id, type, record_id, payload}) → void|throw forbidden`.
- **Policy:** Akteur = Befehlsakteur; Entscheidung server-autoritativ (`policy.rs`), Browser-Helfer nur Spiegel.
- **OS-artig:** zentraler Autorisierungsdienst, app-unabhängig. (Der heutige
  `workjet.command_access_check` ist der Kandidat zur Verallgemeinerung hierher.)

### 20. `lock` — generische benannte Sperre (Advisory Lease)
- **Signatur:** `lock.claim({key, ttl_ms?}) → {token}|null`, `lock.release({token})`.
- **Policy:** TTL/Fence über Core-Uhr (§16.8); genau ein Halter je `key`.
- **OS-artig:** Advisory-Lock/Mutex wie ein OS-Dateilock. Deckt `fence.claim/.release`
  (z. B. „ein Research-Lauf je Record“). Der Schlüssel ist eine App-Zeichenkette, kein Fach.

---

## Nicht im Katalog — und warum (Abweichungen begründet)

- **Kein `iot`-Primitiv.** IoT-Assets/Dashboards/Widgets/Regeln/Alarme sind Fachmodell →
  `iot`-App-Collections. Nur Telemetrie-Transport (`net.ingest`) und Zeitreihenabfrage
  (`dataframe`) sind generisch. Heutiger Stand: die IoT-Engine ist Wahrheit im Core
  (`store::open_iot_store`) und erst nach dem Umbau „Engine liest aus dem Sync-Store“
  ohne Rebuild änderbar — Übergangsschuld, in `MAPPING.md` (b) als L geführt.
- **Kein `ticket`-Primitiv.** Der Fall-Zustandsautomat ist App-Fach (tickets/support);
  Ausführung über `agent.task`, Protokoll über `audit`. E stuft ihn als FAEHIGKEIT ein,
  weil er heute Core-Engine ist — nach Owner-Regel App-Logik.
- **Kein `crew`-Primitiv.** Mitglieder/Lernen/Continuity sind ctox-App-Collections;
  Agentenlauf über `agent.task`/`agent.turn`.
- **Kein `scrape`-/`sellify`-/`outbound`-Primitiv.** Ziel-/Skript-/Feld-Register sind
  App-Collections der OLG-/Outbound-App; Ausführung über `process.run`/`browser`.
- **Kein `workjet`-Primitiv.** Workjet ist eine App (Phase 11); nur Anbieterföderation,
  SSH-Schlüssel und Sitzungstransfer sind generische Fähigkeiten und würden — falls als
  OS-Primitiv gewünscht — unter `process.run`/eigene Transfer-Capability fallen (hier
  nicht aufgenommen, da in den 42 Manifesten nicht als `capabilities_required` geführt).
- **`model`, `net.ingest`, `lock`, `process.run`** stehen im Katalog, obwohl in den heutigen
  42 Manifesten teils noch unter Fach-Namen (`iot.*`, `pentest.*`, `fence.*`) verlangt; der
  Katalog benennt das generische Ziel, `MAPPING.md` die Umbenennung.
