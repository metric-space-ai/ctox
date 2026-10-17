# outbound – v4-Handler (Strang H1, Shell v4)

Stand: 15.10.2026, Klon `~/Documents/ctox-shell-v4` (Zweig `shell-v4/prototype`).
Rust-Quelle (nur gelesen): `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/`
(`store_outbound_commands.rs`, `store_outbound_delivery_policy.rs`, `store.rs`, `importer.rs`).
Einstufung aus `~/.local/state/shell-v4/analysis/E-befehlstypen.md` §2, Trennregel Architektur §16.13.

Handler-Vertrag: `handlers/<command_type>.js` mit `export default async function handle(command, ctx)`.
Gemeinsamer Code liegt in `handlers/_lib/` (keine Node-Builtins; SHA-256 für `stable_digest` in reinem JS).
Tests: `handlers/tests/<command_type>.test.mjs` gegen die In-Memory-Attrappe `handlers/tests/_ctx.mjs`.

Abnahme: `cd src/apps/business-os/modules/outbound/handlers && node --test tests/*.test.mjs`

## Stand in Zahlen

| | Anzahl |
|---|---|
| Befehlstypen App `outbound` (E) | 38 |
| davon portiert + getestet | **38** |
| davon legacy (`ctx.cap.legacy.<type>`) | 0 |
| `outbound.*`-Typen der App `outbound-lead-generation` (E; drei davon sendet auch die outbound-UI) | 11 |
| davon hier portiert + getestet | 6 (+ `sellify.lookup` OUT3) |
| davon Weitergabe an die Eigentümer-App OLG per `ctx.command` (OUT3) | 3 |
| davon legacy (`legacy.<type>`) | **0** (vorher 4, Strang OUT3) |
| davon nicht in diesem Ordner (gehören zur Kunden-App) | 2 |
| **Handler-Dateien gesamt** | **53** (50 portiert, 3 Weitergaben an OLG, 0 legacy; +`message.compose` MAIL2, +`tracking.register`/`tracking.event` TRACK, +`company.research.done`/`pipeline.contact_research.done`/`research_source.generate_adapter.done` OUT3) |
| Tests | 156 grün, 1 todo (`node --import ../../../v4/tools/node/ctox-shared.mjs --test tests/*.test.mjs`, Stand OUT3 15.10.) |

## Tabelle aller Befehlstypen

Reihenfolge = Port-Reihenfolge nach UI-Nutzen (Zahl der Sendestellen in `modules/outbound/index.js` +
`active-outreach.js`; Grep auf das Typ-Literal; gleich häufige nach Abhängigkeit: Freigabe-Kette vor Versand).
Status: **P** = portiert + getestet, **L** = legacy-Weiterleitung, **–** = nicht in diesem Strang.

| # | Typ | UI-Stellen | Einstufung (E) | Rust-Ort | Ziel | Status |
|---|---|---|---|---|---|---|
| 1 | `outbound.draft.prepare` | 3 | FACHLOGIK | `store_outbound_commands.rs:237` (+ `:6555`, `:6579`) | `outbound.draft.prepare.js`, `_lib/drafting.js` | P |
| 2 | `outbound.message.update_draft` | 2 | FACHLOGIK | `:501` | `outbound.message.update_draft.js` | P |
| 3 | `outbound.engagement.assign_sender` | 2 | FACHLOGIK | `:160` | `outbound.engagement.assign_sender.js` | P |
| 4 | `outbound.engagement.close` | 2 | FACHLOGIK | `:971` | `outbound.engagement.close.js` | P |
| 5 | `outbound.scheduler.tick` | 2 (+5-min-Intervall) | FACHLOGIK | `:4955`, `:5106`, `:5444` | `outbound.scheduler.tick.js` | P (Abgleich über `cap.mail.delivery_log`) |
| 6 | `outbound.letter_template.save` | 2 | FACHLOGIK | `:4850` | `outbound.letter_template.save.js` | P |
| 7 | `outbound.engagement.create` | 1 | FACHLOGIK | `:137` | `outbound.engagement.create.js` | P |
| 8 | `outbound.message.request_approval` | 1 | FACHLOGIK | `:551` | `outbound.message.request_approval.js` | P |
| 9 | `outbound.message.approve` | 1 | FACHLOGIK | `:586`, Ereignis `:6765` | `outbound.message.approve.js` | P (Audit über `cap.audit.event`) |
| 10 | `outbound.message.reject` | 1 | FACHLOGIK | `:653` → `:6804` | `outbound.message.reject.js`, `_lib/decisions.js` | P |
| 11 | `outbound.message.send_approved` | 1 | GEMISCHT | `:657`, Gate `:6911`, Slot `:6402/:6469`, Fehler `:6982`, Kommunikation `:5961` | `outbound.message.send_approved.js`, `_lib/send.js` | P (Versand `cap.mail.send`, Kanalzeile `cap.mail.communication.upsert`) |
| 12 | `outbound.message.pause` | 1 | FACHLOGIK | `:836` | `outbound.message.pause.js`, `_lib/pause-cancel.js` | P |
| 13 | `outbound.message.cancel` | 1 | FACHLOGIK | `:836` | `outbound.message.cancel.js` | P |
| 14 | `outbound.message.resume` | 1 | FACHLOGIK | `:882` | `outbound.message.resume.js` | P |
| 15 | `outbound.engagement.resume` | 1 | FACHLOGIK | `:922` | `outbound.engagement.resume.js` | P |
| 16 | `outbound.engagement.reapply_sequence` | 1 | FACHLOGIK | `:5389` | `outbound.engagement.reapply_sequence.js` | P |
| 17 | `outbound.reply.classify` | 1 | FACHLOGIK | `:1038`, Policy `store_outbound_delivery_policy.rs:72/:97` | `outbound.reply.classify.js` | P |
| 18 | `outbound.scheduling.mark_booked` | 1 | FACHLOGIK | `:1098` | `outbound.scheduling.mark_booked.js` | P |
| 19 | `outbound.scheduling.update_slots` | 1 | FACHLOGIK | `:5464` | `outbound.scheduling.update_slots.js` | P |
| 20 | `outbound.skillbook.save` | 1 | FACHLOGIK | `:4602` | `outbound.skillbook.save.js` | P |
| 21 | `outbound.skillbook.seed_defaults` | 1 | FACHLOGIK | `:4663`, Vorgaben `:4692-4848` | `outbound.skillbook.seed_defaults.js`, `_lib/skillbook-defaults.js` (wörtlich extrahiert) | P |
| 22 | `outbound.campaign.briefing.update` | 1 | FACHLOGIK | `:5704` | `outbound.campaign.briefing.update.js` | P |
| 23 | `outbound.campaign.apply_setup` | 2 | FACHLOGIK | `:5630` | `outbound.campaign.apply_setup.js` | P |
| 24 | `outbound.provider.reconcile` | 1 | GEMISCHT | `:5508` | `outbound.provider.reconcile.js`, `_lib/reconcile.js` | P (Zustellprotokoll `cap.mail.delivery_log.latest`) |
| 25 | `outbound.audit.export` | 1 | FACHLOGIK | `:4894` | `outbound.audit.export.js` | P (1-MiB-Grenze, s. P5) |
| 26 | `outbound.pipeline.write_outreach_draft` | 1 | FACHLOGIK | `:57` | `outbound.pipeline.write_outreach_draft.js` | P |
| 27 | `outbound.research_source.auth_assist` (E: App lead-gen) | 3 | GEMISCHT | `:1180` → `:2776`, `:3873` | `outbound.research_source.auth_assist.js`, `_lib/research-adapter.js` | P (`cap.secrets.exists`, `cap.browser.auth_assist.request`) |
| 28 | `outbound.research_source.generate_adapter` (E: App lead-gen) | 1 | GEMISCHT | `:1174` → `:2776` + Scrape-Wirkung `:3101` (Generierungszweig), `:3759`, `:3937`, `:4239` | `outbound.research_source.generate_adapter.js`, `_lib/scrape-generation.js` | P (OUT3: OLG-Registry foreign read, `ctx.command` an OLG, `cap.agent.task.enqueue`) |
| 29 | `outbound.research_source.test` (E: App lead-gen) | 1 | GEMISCHT | `:1177` → `:2776` + Testzweig `:3330-3758` | `outbound.research_source.test.js` | P (OUT3: Weitergabe an OLG, `_lib/owner-app.js`) |
| 30 | `outbound.source.import` | 2 | GEMISCHT | `store.rs:10177` → `importer.rs:26` → `import_outbound_companies :1652`, Skript `:2055-2131` | `outbound.source.import.js`, `_lib/company-import-script.js` (Skript wörtlich) | P (`cap.browser.automation.run`, `cap.knowledge.command`, `cap.artifact.write`) |
| 31 | `outbound.company.research` | 1 | GEMISCHT | `importer.rs:228` → `:315` (+ `:3134-3360`) | `outbound.company.research.js` (+`.done.js`), `_lib/research.js`, `_lib/research-task.js`, `_lib/knowledge.js` | P (OUT3: `cap.agent.task.enqueue/get` + Rückruf, `cap.knowledge.query`) |
| 32 | `outbound.pipeline.contact_research` | 1 | GEMISCHT | `importer.rs:229` → `:479` (+ `:3361-3517`, DSGVO `:440-475`) | `outbound.pipeline.contact_research.js` (+`.done.js`) | P (OUT3: `cap.agent.task.enqueue/get` + Rückruf, `cap.knowledge.query`) |
| 33 | `outbound.pipeline.lead_qualification` | 1 | GEMISCHT | `importer.rs:232` → `:590` (deterministisch) | `outbound.pipeline.lead_qualification.js` | P (`cap.knowledge.command`) |
| 34 | `outbound.message.prepare` | 0 (Sender m/mail) | FACHLOGIK | `:462` | `outbound.message.prepare.js` | P |
| 35 | `outbound.message.request_changes` | 0 | FACHLOGIK | `:654` → `:6857` | `outbound.message.request_changes.js` | P |
| 36 | `outbound.reply.match` | 0 (Sender m/conversations) | FACHLOGIK | `:5771` | `outbound.reply.match.js` | P (Annotation `cap.mail.message.annotate`, best effort) |
| 37 | `outbound.campaign.mailbox.link` | 0 | GEMISCHT | `:4277` | `outbound.campaign.mailbox.link.js` | P (`cap.mail.account.upsert`) |
| 38 | `outbound.campaign.status.set` | 0 (Sender m/mail) | FACHLOGIK | `:4415` | `outbound.campaign.status.set.js` | P |
| 39 | `outbound.scheduling.prepare` | 0 | FACHLOGIK | `:1078` | `outbound.scheduling.prepare.js` | P |
| 40 | `outbound.sequence.save` | 0 | FACHLOGIK | `:211` | `outbound.sequence.save.js` | P |
| 41 | `outbound.dev.seed_test_data` | 0 | FACHLOGIK | `:5343` | `outbound.dev.seed_test_data.js` | P |
| 41a | `outbound.message.compose` (neu, MAIL2) | 1 (mail-Composer) | FACHLOGIK (Komposition) | keiner (Kette aus #7, #34, #8, #9, #11) | `outbound.message.compose.js` | P (8 Tests, s. Nachtrag MAIL2) |
| 41b | `outbound.tracking.register` (neu, TRACK) | 0 (Trigger `every`, erster Versand) | FACHLOGIK (§16.35 Nr. 3) | `store.rs:16592-16606` (Basis-URL-Prüfung) | `outbound.tracking.register.js`, `_lib/tracking.js` | P (`net.ingest.webhook.register`, s. „Mail-Tracking“) |
| 41c | `outbound.tracking.event` (neu, TRACK) | 0 (Zustellung durch `net.ingest`) | FACHLOGIK (§16.35 Nr. 3) | `server.rs:340-376`, `store_outbound_commands.rs:6265-6337`, Antworten `server.rs:4024-4053` | `outbound.tracking.event.js`, `_lib/tracking.js` | P (16 Tests, s. „Mail-Tracking“) |
| | **App `outbound-lead-generation` (Sender `customer-modules/outbound-lead-generation/index.js`)** | | | | | |
| 42 | `outbound.research_source.upsert` | 0 (Kunden-App) | GEMISCHT | `:1171` → `:2776` | `outbound.research_source.upsert.js` | P |
| 43 | `outbound.research_source.registry_read` | 0 (Kunden-App) | FACHLOGIK* | `:1187` → `:1207` | `outbound.research_source.registry_read.js` | P (OUT3: OLG-Registry foreign read, Skript `cap.files.get`) |
| 44 | `outbound.research_policy.publish` | 0 (Kunden-App) | FACHLOGIK | `:1154` → `:4529` | `outbound.research_policy.publish.js` | P (OUT3: `ctx.command` `outbound.scrape.target.upsert` an OLG; s. P11) |
| 45 | `outbound.research_source.reprobe` | 0 (Kunden-App) | GEMISCHT | `:46` → `:4090` | `outbound.research_source.reprobe.js` | P (OUT3: Weitergabe an OLG) |
| 46 | `outbound.sellify.lookup` | 0 (Kunden-App) | FAEHIGKEIT | `:1183` → `:1673` | `outbound.sellify.lookup.js`, `_lib/sellify-lookup.js` | P (OUT3: foreign read `sellify_*`) |
| 47 | `outbound.update_digest.send_now` | 0 (Kunden-App) | GEMISCHT | `:1184` → `outbound_update_digest.rs` (2 126 Z.) | `outbound.update_digest.send_now.js` | P (OUT3: Weitergabe an OLG) |
| 48 | `outbound.lead.research_writeback` | 0 (Kunden-App) | FACHLOGIK | `command_plane.rs:1746` → `person_research_gap_closure.rs:1745` | gehört in `customer-modules/outbound-lead-generation/handlers/` (nicht im Whitelist) | – |
| 49 | `web_stack.person_research` | 0 (Kunden-App) | GEMISCHT | `command_plane.rs:1744` → `person_research_command.rs:59` | kein `outbound.*`-Typ; Kunden-App | – |

\* E stuft `registry_read` als FACHLOGIK ein; der Rust-Code liest aber ausschließlich Kern-Tabellen der Scrape-Registry
(`scrape_target`, `scrape_run`, `scrape_account_state`, Skripte). Portiert ist die Hüllenprüfung und die Projektion je Ziel;
die Daten kommen über eine Capability.

Nicht-`outbound.*`-Befehle, die die outbound-UI zusätzlich sendet (bleiben Plattform/Capability, kein Handler hier):
`ctox.mailserver.save_domain|delete_domain|save_user|delete_user|get_config` (`cap.mail.server`), `knowledge.command`
(`cap.knowledge`). Strang SKL-O (15.10.2026): der frühere `business_os.chat.task` des Campaign-Setups ist der App-Befehl
`outbound.campaign.setup.request`; `outbound.pipeline.outreach_draft` hat jetzt einen Handler (Abschnitt „Strang SKL-O“).

## Capability-Wünsche (an Strang „ctox sync v4“)

| Capability | Signatur | Beispielaufruf | Handler |
|---|---|---|---|
| `cap.audit.event` | `({collection, record_id, event_type, payload, observed_at_ms}) → {ok}` – schreibt `business_events` (keine App-Collection) | `await ctx.cap.audit.event({collection:'outbound_approvals', record_id, event_type:'business_os.external_approval.decided', payload, observed_at_ms: now})` | approve, reject, request_changes |
| `cap.mail.send` | `({account_id, to[], subject, text, html, idempotency_key}) → {message_id, status}` – baut RFC 822 (`outbound_queue_email_delivery :6099`), reiht in den Mailserver ein. **Ohne Tracking** (§16.35 Nr. 3): Pixel/Links/Tokens macht die App, s. „Mail-Tracking“ | `await ctx.cap.mail.send({account_id, to:[email], subject, text, html, idempotency_key:`${id}:${rev}`})` | send_approved |
| `cap.net.ingest.webhook.register` | `({path, deliver:{command_type, response:'handler'}}) → {token, ingest_path, ingest_url}` – OS-Primitiv `net.ingest` (CATALOG §18); Kernform der Zustellung s. „Mail-Tracking“ | `await ctx.cap.net.ingest.webhook.register({path:'mail-tracking', deliver:{command_type:'outbound.tracking.event', response:'handler'}})` | tracking.register, send_approved (erster Versand) |
| `cap.mail.communication.upsert` | `({message_key, channel, account_key, thread_key, remote_id, direction, folder_hint, sender_display, sender_address, recipient_addresses[], cc_addresses[], bcc_addresses[], subject, body_text, body_html, trust_level, status, seen, has_attachments, metadata, refresh_thread}) → {ok}` – `channels::upsert_communication_message` + `refresh_thread`; Vorschau und Zeitstempel bildet der Kern | s. `_lib/send.js` `syncEmailMessageToCommunication` | send_approved |
| `cap.mail.delivery_log.latest` | `({ids[]}) → {available, entries:{[id]:{outcome, error_text, completed_at}}}` – jüngste Zeile je id aus `stalwart_smtp_delivery_log`; `available:false` (oder `unavailable`) wenn die Kern-DB fehlt | `await ctx.cap.mail.delivery_log.latest({ids:['q1']})` | provider.reconcile, scheduler.tick |
| `cap.mail.message.annotate` | `({message_key, metadata}) → {ok}` – Metadaten in `communication_messages.metadata_json` mischen | `await ctx.cap.mail.message.annotate({message_key, metadata:{outbound_engagement_id}})` | reply.match |
| `cap.mail.account.upsert` | `({account_key, channel, address, provider, profile}) → {ok}` – `channels::upsert_communication_account` | `await ctx.cap.mail.account.upsert({account_key:'email:a@b.de', channel:'email', address, provider, profile})` | campaign.mailbox.link |
| `cap.secrets.exists` | `({scope, name}) → {exists}` – nie den Wert | `await ctx.cap.secrets.exists({scope:'credentials', name})` | research_source.upsert, auth_assist |
| `cap.browser.auth_assist.request` | Felder von `enqueue_web_stack_auth_assist_request` (`service/business_os.rs:5059`): `({source_id, target_url, credential_ref, login_hint, preferred_auth_assist, requesting_task_id, source_module, command_path, owner_user_id, deterministic_for_task, accept_as_trusted_local}) → effect` | s. `_lib/research-adapter.js` | research_source.auth_assist |
| ~~scrape.registry.read~~ (entfällt OUT3: OLG-Collections foreign read) | `({include_script_target?}) → {list_targets, last_runs:{[target_id]:{last, last_successful}}, account_states:{[target_id]:state}, script}` | `await ctx.cap.scrape.registry.read({include_script_target:'dnb'})` | research_source.registry_read |
| ~~scrape.target.upsert~~ (entfällt OUT3: `ctx.command` an OLG) | `({manifest, manifest_dir}) → {manifest_path}` – Manifest schreiben + `scrape upsert-target` | `await ctx.cap.scrape.target.upsert({manifest, manifest_dir:'app-policy'})` | research_policy.publish |
| ~~scrape.target.registration~~ (entfällt OUT3: foreign read) | `({target_key}) → registration \| null` (`scrape::target_script_registration`; enthält `workspace_dir`, `script_registered`) | `await ctx.cap.scrape.target.registration({target_key:'northdata'})` | research_source.generate_adapter |
| ~~scrape.repair_task.create~~ (entfällt OUT3: agent.task.enqueue) | `({kind, target_key, title, prompt, thread_key, workspace_root, priority, suggested_skill, parent_message_key, extra_metadata}) → {task_id, task_status}` (`channels::create_scrape_repair_queue_task`) | s. `_lib/scrape-generation.js` | research_source.generate_adapter |
| `cap.knowledge.command` | `(argv[]) → JSON` – generischer Durchgriff auf die Knowledge-CLI (`crate::knowledge::dispatch_capturing`), Argumentlisten wörtlich aus Rust | `await ctx.cap.knowledge.command(['data','append','--domain',d,'--key',k,'--rows',json])` | company.research, pipeline.contact_research, pipeline.lead_qualification, source.import |
| ~~research.person~~ (entfällt OUT3: agent.task + Rückruf) | `({company, country, mode:'new_record'\|'update_person', fields:[FieldKey::as_str]}) → research` (`ctox_web_stack::run_ctox_person_research_tool`) | `await ctx.cap.research.person({company:'Acme', country:'DE', mode:'new_record', fields:['firma_ort']})` | company.research, pipeline.contact_research |
| `cap.browser.automation.run` | `({source, timeout_ms}) → {ok, result, error?}` (`crate::web_stack::run_browser_automation`; Skript = App-Daten) | `await ctx.cap.browser.automation.run({source, timeout_ms:180000})` | source.import |
| `cap.artifact.write` | `({command_id, name, json}) → {ok}` – Laufzeit-Artefakt `runtime/business-os-imports/<command_id>/<name>` | `await ctx.cap.artifact.write({command_id, name:'outbound_companies.json', json})` | source.import |
| `ctx.uuid()` (Kontext, keine Capability) | `() → string` | `ctx.uuid()` | scheduler.tick (Rust `Uuid::new_v4().simple()`; ersatzweise Web Crypto bzw. Digest, s. P6) |
| ~~legacy.<type>~~ (entfällt OUT3, 0 Aufrufe) | `(command) → result` – unveränderter nativer Pfad bis zur Portierung | `ctx.cap.legacy['outbound.research_source.test'](command)` | die 4 L-Handler |

Die früheren L-Typen brauchen seit Strang OUT3 keine eigene Fähigkeit mehr: research_source.test/.reprobe und
update_digest.send_now laufen in der Eigentümer-App OLG (Weitergabe per `ctx.command`), sellify.lookup liest die
replizierten Sellify-Collections (s. „Nachtrag OUT3“).
Hinweis zu E: `outbound.source.import` braucht nicht `cap.doc.extract` (E), sondern eine Browser-Automation mit einem
App-eigenen Skript; die drei Recherche-Typen laufen ohne Agenten-Queue (nativer Prozessor), brauchen also nicht
`cap.agent.task`, sondern `cap.research.person` + `cap.knowledge.command`.

## Fehlercodes (Abbildung der Rust-`anyhow`-Fehler)

`invalid` = Pflichtfeld fehlt / ungültige Eingabe; `not_found` = Datensatz fehlt; `conflict` = Statusübergang nicht erlaubt,
Empfänger gesperrt, fehlende Freigabe; `forbidden` = Absenderkonto gesperrt, Writeback außerhalb des Modulbereichs;
`quota` = Tageslimit erschöpft; `unavailable` = Capability/Provider nicht erreichbar. Die Fehlertexte sind wörtlich aus Rust
übernommen, weil `outbound_classify_send_block` (`:6943`) auf ihnen verzweigt.

## Offene Punkte / Unklarheiten

- **P1 `record_id` fehlt im Vertrag.** Rust liest `command.record_id` (`outbound_id_from_command`, `store.rs:18316`), die UI
  sendet es (`active-outreach.js:1043`). Der feste v4-Befehl `{id,type,payload,actor,app_id,app_ver}` hat es nicht. Die
  Handler lesen `command.record_id ?? payload.record_id`. Entscheidung nötig: Feld in den Vertrag aufnehmen.
- **P2 `client_context` fehlt im Vertrag.** Genutzt von research_source.* (`campaign_id`, `source_module`, `actor.id`) –
  die Handler lesen `command.client_context ?? {}`; ohne das Feld scheitert jeder Adapter-Writeback mit `invalid`.
- **P3 Pfad-Eigenheit `skillbook.save` / `letter_template.save`.** `outbound_required_string(payload, &["skillbook_id","id"])`
  ist ein PFAD (`payload.skillbook_id.id`), keine Schlüsselliste. Die Id kommt damit praktisch immer aus `record_id`.
  Treu übernommen (Test belegt es).
- **P4 Atomarität.** Rust schreibt ohne Transaktion (außer Slot-Reservierung `BEGIN IMMEDIATE`). Die Handler bündeln
  Mehrfach-Writes in `ctx.transaction`; Fehler, nach denen Rust Teilergebnisse stehen ließ, rollen jetzt zurück. Bewusst
  beibehalten wurde das Rust-Muster „Fehler protokollieren, dann werfen“ (send_approved: Fehler-Commit vor dem `throw`).
- **P5 1-MiB-Ergebnisgrenze.** `audit.export` gibt alle Datensätze von 11 Collections zurück; bei großen Instanzen
  überschreitet das die v4-Grenze. Rust kannte keine Grenze. Lösung (Seitenweise / Datei-Artefakt) offen.
- **P6 Zufalls-Id.** scheduler.tick erzeugt `msg_<uuid simple>`; ctx bietet keine Quelle. Fallback: `globalThis.crypto`
  falls vorhanden, sonst sha256(command.id|engagement|now). Wunsch `ctx.uuid()`.
- **P7 serde-Reihenfolge der Revision.** `outbound_message_revision` hasht `json!({...}).to_string()`. `Cargo.lock` zeigt
  `serde_json` mit `indexmap` (Feature `preserve_order` über `src/core/rxdb`), daher Einfügereihenfolge
  `sender_account_id, recipient_email, subject, body_text, body_html`. Ohne dieses Feature wäre es alphabetisch und alle
  Revisions-Ids bestehender Daten wichen ab. Gegenprobe gegen einen echten Rust-Wert steht aus (kein Rust-Lauf erlaubt).
- **P8 Rust-Klassifizierer-Eigenheit.** Ein Provider-Fehler wird als `"failed to queue approved outbound email"`
  protokolliert; `outbound_classify_send_block` findet „approv“ zuerst → `approval_required` statt `provider_queue_failed`.
  Treu übernommen, Test belegt es. Fachlich vermutlich ein Fehler im Rust-Code.
- **P9 Schema-Abweichungen.** Rust schreibt Felder/Collections außerhalb `collections.schema.json`: Sperrlisten-Einträge
  mit `email/domain/status` statt Pflicht `scope/value/active`; `outbound_sender_assignments` ohne Pflicht `campaign_id`;
  `outbound_research_policies` existiert im Schema gar nicht. Treu übernommen; der App-Port muss das Schema angleichen.
- **P10 Writeback-Prüfung vorgezogen.** research_source.*: Rust prüfte die Writeback-Collection erst nach dem
  Adapter-Write; der Handler prüft vorher (kein halber Zustand). Kleine, bewusste Abweichung.
- **P11 App-Zuordnung.** `research_policy.publish` und `research_source.reprobe` laufen in Rust auch mit
  `module=outbound-lead-generation`; im v4-Modell gehört ein Handler zu genau einer App. Hier liegen sie unter outbound;
  ob die Kunden-App eigene Handler oder eine Fremd-Befehlsfreigabe braucht, ist offen.
- **P12 Kein Rechte-Gate im Handler.** Rust prüft Rechte vor dem Dispatch (Policy), nicht im Arm. Die Handler setzen
  voraus, dass die Laufzeit `actor.role` gegen die Manifest-Rechte prüft. `actor.id` ersetzt `outbound_session_actor_id`.
- **P13 `reply.classify`** stoppt per Sperrliste zwar das Engagement, storniert aber (anders als `reply.match`) keine
  offenen Entwürfe – so steht es im Rust-Code, obwohl ein Kommentar in `store_outbound_delivery_policy.rs:148` das Gegenteil
  behauptet. Treu übernommen.
- **P15 research_source.test bleibt legacy.** Der Testzweig (`:3330-3758`) führt das Scrape-Ziel aus, bewertet Evidenz,
  bildet ~15 Status ab und nutzt einen Probe-Grant, der nur aus einer signierten Nutzersitzung entsteht
  (`outbound_source_test_probe_grant`). Diese Autorisierung muss die Capability selbst prüfen; vor dieser Entscheidung
  wird nichts portiert. Gleiches gilt für `research_source.reprobe`.
- **P16 Recherche-Prozessoren ohne Queue-Task.** Rust startete company/contact research und lead qualification über
  `record_command` + Harness-Prozessor (`store.rs:10149`) und setzte danach `business_commands.status='accepted'`. Als
  Handler laufen sie direkt; die Laufzeit muss lange Recherchen (Browser, bis Minuten) über das 30-s-CPU-Limit hinaus
  erlauben, da die Zeit in der Capability vergeht. `queue_task_id` im Import-Artefakt ist jetzt `null`.
- **P17 Sortierung.** `contacts_from_research` nutzt `BTreeMap` (Byte-Ordnung der UTF-8-Schlüssel); JS sortiert nach
  UTF-16-Codeeinheiten. Unterschied nur bei Zeichen jenseits U+FFFF gegen U+E000–U+FFFF.
- **P18 Vollscans über `ctx.read`.** Wie Rust (`outbound_load_records_by_string_field`, Suppression-/Approval-Scans)
  lesen die Handler ganze Collections (`{selector:{}}`) und filtern mit Rust-Semantik (getrimmter String-Vergleich).
  Liefert `ctx.read` – wie heute `find()` im Browser – höchstens 200 Dokumente, werden Freigaben/Sperrlisten
  übersehen (Versand-Gate!). Die Laufzeit muss vollständige Ergebnisse liefern, sonst Selektor + Seitenweise nötig.
- **P19 Mango-Form.** Die Handler übergeben `ctx.read(collection, {selector, sort?, limit?})`; ob der Vertrag die Hülle
  oder nur den Selektor erwartet, ist nicht festgelegt (Attrappe akzeptiert beides).
- **P20 Manifest.** `v4/manifests/outbound.manifest.json` (anderer Strang) hat `handlers: {}` und
  `capabilities_required: []`. Einzutragen (Vorschlag unten), außerdem `foreign_collections` für den Adapter-Writeback
  in `outbound_lead_generation_*` und die Collection `outbound_research_policies` (fehlt im Schema).
- **P14 Ergebnis-Momentaufnahmen.** Wo Rust das Engagement vor einer späteren Änderung zurückgibt (z. B. reply.classify vor
  dem Sperrlisten-Stopp), gibt der Handler dieselbe Momentaufnahme zurück.

## Nachtrag MAIL2 (15.10.2026): Sammelbefehl `outbound.message.compose`

Ersetzt die Befehlskette, die die mail-UI je Empfänger in Reihe absetzte (mail V4-PORT.md OFFEN 3): die UI sendet nur
noch EINEN Befehl. Keine neue Fachregel: der Handler ruft die unveränderten Einzel-Handler in fester Reihenfolge auf.

- Payload: `{campaign_id, sender_account_id, subject, body_text, body_html, mode:'draft'|'approval'|'send',
  content_revision_id?, created_at_ms?, client_context?, recipients:[{recipient_email, engagement_id?, message_id?}] | string[]}`.
  Fehlen die Ids, werden sie aus `command.id` abgeleitet (`eng_|msg_<stable_digest(id:index)>`).
- Kette je Empfänger: `engagement.create` → `message.prepare` → [`request_approval`] (approval|send) → [`approve`] (send),
  danach (send) `send_approved`. Die Teilbefehle tragen exakt die Payload, die die UI vorher sendete (inkl. `record_id`,
  `client_context`, die die Handler in die Datensätze kopieren); damit sind Ids (auch `approval.id` = `record_id` =
  message_id) und alle Felder identisch. Absender: wie bisher über `sender_account_id` + `status:'assigned'` im
  Engagement-Payload; `assign_sender` gehörte nicht zur mail-Kette und wird nicht aufgerufen (wäre ein zusätzlicher
  `outbound_sender_assignments`-Datensatz).
- Atomarität: alle Datensatz-Schreibungen aller Empfänger in EINER `ctx.transaction` (die inneren `ctx.transaction` der
  Einzel-Handler laufen im selben tx). Capability-Aufrufe in der Kette (`cap.audit.event` der Freigabe) werden bis nach
  dem Commit zurückgestellt. Abweichung zur Kette: ein Fehler bei Empfänger n rollt jetzt auch die Datensätze der
  Empfänger < n zurück (vorher blieben sie stehen).
- Versand (`mode:'send'`) bleibt außerhalb der Transaktion, je Nachricht nach dem Commit: `send_approved` ruft
  `cap.mail.send` und bucht Fehler selbst (Fehler-Commit, dann `throw`, s. P4). Alle Nachrichten werden versucht; bei
  Fehlern wirft compose danach `send failed for k of n messages: <erster Fehler>` mit dem Code des ersten Fehlers.
- Unterschied im Audit-Ereignis: `command_id`/`command_type` im Ereignis-Payload nennen den Teilbefehl
  (`<compose-id>:approve:<i>`, `outbound.message.approve`).
- Test `tests/outbound.message.compose.test.mjs`: je Modus (draft/approval/send) Endzustand von
  `outbound_engagements/messages/approvals/account_limits`, Versandaufrufe und Audit-Ereignisse identisch zur
  Einzelkette (Payloads wörtlich wie die alte UI); Rollback mitten in der Kette; Audit erst nach Commit; Versandfehler;
  Eingabeprüfung. Manifest-Nachtrag: `"outbound.message.compose": "handlers/outbound.message.compose.js"`.

## Manifest-Vorschlag (für den Manifest-Strang, hier nicht geschrieben)

```json
{
  "handlers": {
    "outbound.audit.export": "handlers/outbound.audit.export.js",
    "outbound.campaign.apply_setup": "handlers/outbound.campaign.apply_setup.js",
    "outbound.campaign.briefing.update": "handlers/outbound.campaign.briefing.update.js",
    "outbound.campaign.setup.done": "handlers/outbound.campaign.setup.done.js",
    "outbound.campaign.setup.failed": "handlers/outbound.campaign.setup.failed.js",
    "outbound.campaign.setup.request": "handlers/outbound.campaign.setup.request.js",
    "outbound.campaign.mailbox.link": "handlers/outbound.campaign.mailbox.link.js",
    "outbound.campaign.status.set": "handlers/outbound.campaign.status.set.js",
    "outbound.company.research": "handlers/outbound.company.research.js",
    "outbound.company.research.done": "handlers/outbound.company.research.done.js",
    "outbound.dev.seed_test_data": "handlers/outbound.dev.seed_test_data.js",
    "outbound.draft.prepare": "handlers/outbound.draft.prepare.js",
    "outbound.engagement.assign_sender": "handlers/outbound.engagement.assign_sender.js",
    "outbound.engagement.close": "handlers/outbound.engagement.close.js",
    "outbound.engagement.create": "handlers/outbound.engagement.create.js",
    "outbound.engagement.reapply_sequence": "handlers/outbound.engagement.reapply_sequence.js",
    "outbound.engagement.resume": "handlers/outbound.engagement.resume.js",
    "outbound.letter_template.save": "handlers/outbound.letter_template.save.js",
    "outbound.message.approve": "handlers/outbound.message.approve.js",
    "outbound.message.cancel": "handlers/outbound.message.cancel.js",
    "outbound.message.pause": "handlers/outbound.message.pause.js",
    "outbound.message.prepare": "handlers/outbound.message.prepare.js",
    "outbound.message.reject": "handlers/outbound.message.reject.js",
    "outbound.message.request_approval": "handlers/outbound.message.request_approval.js",
    "outbound.message.request_changes": "handlers/outbound.message.request_changes.js",
    "outbound.message.resume": "handlers/outbound.message.resume.js",
    "outbound.message.send_approved": "handlers/outbound.message.send_approved.js",
    "outbound.message.update_draft": "handlers/outbound.message.update_draft.js",
    "outbound.pipeline.contact_research": "handlers/outbound.pipeline.contact_research.js",
    "outbound.pipeline.contact_research.done": "handlers/outbound.pipeline.contact_research.done.js",
    "outbound.pipeline.lead_qualification": "handlers/outbound.pipeline.lead_qualification.js",
    "outbound.pipeline.outreach_draft": "handlers/outbound.pipeline.outreach_draft.js",
    "outbound.pipeline.outreach_draft.done": "handlers/outbound.pipeline.outreach_draft.done.js",
    "outbound.pipeline.outreach_draft.failed": "handlers/outbound.pipeline.outreach_draft.failed.js",
    "outbound.pipeline.write_outreach_draft": "handlers/outbound.pipeline.write_outreach_draft.js",
    "outbound.provider.reconcile": "handlers/outbound.provider.reconcile.js",
    "outbound.reply.classify": "handlers/outbound.reply.classify.js",
    "outbound.reply.match": "handlers/outbound.reply.match.js",
    "outbound.research_policy.publish": "handlers/outbound.research_policy.publish.js",
    "outbound.research_source.auth_assist": "handlers/outbound.research_source.auth_assist.js",
    "outbound.research_source.generate_adapter": "handlers/outbound.research_source.generate_adapter.js",
    "outbound.research_source.generate_adapter.done": "handlers/outbound.research_source.generate_adapter.done.js",
    "outbound.research_source.registry_read": "handlers/outbound.research_source.registry_read.js",
    "outbound.research_source.reprobe": "handlers/outbound.research_source.reprobe.js",
    "outbound.research_source.test": "handlers/outbound.research_source.test.js",
    "outbound.research_source.upsert": "handlers/outbound.research_source.upsert.js",
    "outbound.scheduler.tick": "handlers/outbound.scheduler.tick.js",
    "outbound.scheduling.mark_booked": "handlers/outbound.scheduling.mark_booked.js",
    "outbound.scheduling.prepare": "handlers/outbound.scheduling.prepare.js",
    "outbound.scheduling.update_slots": "handlers/outbound.scheduling.update_slots.js",
    "outbound.sellify.lookup": "handlers/outbound.sellify.lookup.js",
    "outbound.sequence.save": "handlers/outbound.sequence.save.js",
    "outbound.skillbook.save": "handlers/outbound.skillbook.save.js",
    "outbound.skillbook.seed_defaults": "handlers/outbound.skillbook.seed_defaults.js",
    "outbound.source.import": "handlers/outbound.source.import.js",
    "outbound.tracking.event": "handlers/outbound.tracking.event.js",
    "outbound.tracking.register": "handlers/outbound.tracking.register.js",
    "outbound.update_digest.send_now": "handlers/outbound.update_digest.send_now.js"
  },
  "capabilities_required": [
    "audit.event",
    "mail.send",
    "net.ingest.webhook.register",
    "mail.communication.upsert",
    "mail.delivery_log.latest",
    "mail.message.annotate",
    "mail.account.upsert",
    "secrets.exists",
    "browser.auth_assist.request",
    "browser.automation.run",
    "knowledge.query",
    "agent.task.enqueue",
    "agent.task.get",
    "files.get",
    "artifact.write"
  ]
}
```

## Nachtrag FIXINV (15.10.2026): Invarianten-Brüche INV1-B1 und INV3-B3 behoben

Zwei aus Rust treu mitportierte Lücken der Sicherheits-Invarianten (Testaudit E13) sind geschlossen; Details, Tests und
Mutationsbeleg in `handlers/INVARIANTS.md` (Abschnitt „Behebung“).

- **INV1-B1 (Versand nur bei aktueller freigegebener Revision).** `outbound.message.approve` speichert auf der Freigabe
  `content_hash` (`messageContentHash`: Kanal, Absender kanonisch, Empfänger, Betreff, Text/HTML, Briefdokument, Anhänge).
  Das Versand-Gate (`_lib/send.js` `enforceSendGate` → `hasMatchingApproval(…, message)`) vergleicht den Hash des zu
  sendenden Inhalts mit dem freigegebenen; Bestands-Freigaben ohne Hash werden gegen die aus dem Inhalt neu berechnete
  Revision geprüft. Abweichung = Fehler des bestehenden Freigabe-Gates (`conflict`, „no matching approval for current
  revision“). Abweichung zu Rust: Rust vertraute der gespeicherten `revision_id` (`store_outbound_commands.rs:6917`).
- **INV3-B3 (gesperrte/abgemeldete Empfänger nie anschreiben)** – löst P9 für `outbound_suppression_entries`: neue
  Einträge nur in der Schemaform `scope/value/active` (UI `active-outreach.js` und `applyReplySuppression`; der Rust-Alteintrag
  mit `email`+`domain` wird zu zwei Schemaeinträgen, gleiche Wirkung). Das Gate liest Schema- und Altform; Domain-Sperren
  decken Subdomains. Abweichung zu Rust: `store_outbound_delivery_policy.rs:177-215` las nur die Altform, exakte Domain.
- **INV4-B2** (Slot-Leck nach Commit-Konflikt) bleibt bewusst unverändert (kein Invariantenbruch).


## Kernform der Capabilities (Strang CAPSIG, 15.10.2026, §16.30/§16.32)

| Capability | Aufruf (Kernform) |
|---|---|
| `knowledge.query` | `({argv})` (vormals `knowledge.command(argv)`) |
| `mail.delivery_log.query` | `({message_ids})` → `{deliveries}`; jüngste Zustellung je id wählt der Handler (`latestDeliveries`) |
| `mail.channel.mailbox.link` | native Payload `{account_key, channel, address, provider, profile}` (vormals `mail.account.upsert`) |
| `mail.send` | `({account_id, to, subject, text, html, idempotency_key})`; `from`/`tracking` entfallen. Tracking ist App-Logik (§16.35 Nr. 3, Strang TRACK): `_lib/tracking.js`, Abschnitt „Mail-Tracking“ |
| `net.ingest.webhook.register` | `({path:'mail-tracking', deliver:{command_type:'outbound.tracking.event', response:'handler'}}) → {token, ingest_path, ingest_url}` |

**Entscheidung `mail.communication.upsert` / `mail.message.annotate`:** beide schreiben Collections der App
conversations (`communication_messages`, `communication_threads`). Das ist Datenpflege, kein OS-Primitiv. conversations
hat keine Handler und keine Befehle; ein `ctx.command` an conversations hätte dort eine reine Durchreich-Schicht
erzwungen. Deshalb schreibt outbound direkt über `foreign_collections` (ops `read, write`) und die conversations-
Exporte (`write` ergänzt): `_lib/send.js upsertCommunicationMessage` (id = message_key, Thread-Kennzahlen wie
threads.rs refresh_thread_states) und `annotateCommunicationMessage` (Metadaten zusammenführen).

#772 (main 1c269d63a): Personen brauchen zwei unabhängige Anbieter für Name/Funktion (`person_source_status`
verified | single_source, `person_sources`), jede Person trägt `person_geschlecht`/`person_titel`; Personenanfragen
fordern beide Felder an (`_lib/research.js`, Tests `tests/person-rules.test.mjs`).

## Mail-Tracking (Strang TRACK, 15.10.2026, Kern §16.35 Nr. 3)

Mail-Tracking (Öffnungen/Klicks) ist App-Logik und nicht Teil von `mail.send`. Weil nur der Core HTTPS bedient, meldet
outbound über das generische Eingangs-Primitiv `net.ingest` einen eigenen Pfad an; jede Anfrage darauf kommt als
App-Befehl `outbound.tracking.event` zurück, dessen Ergebnis die HTTP-Antwort bestimmt. Pixel einfügen und Links
umschreiben macht outbound beim Versand selbst. `grep -rn "tracking" handlers/*.js` außerhalb von
`_lib/tracking.js`/`outbound.tracking.*.js` trifft nur noch den Aufruf in `send_approved` und den Opt-in-Stempel in `compose`;
`mail.send` bekommt kein Tracking-Feld (Test prüft die Schlüsselmenge).

| Teil | Datei | Rust-Referenz |
|---|---|---|
| Pfad anmelden (einmalig/idempotent; `payload.force` meldet neu an) | `outbound.tracking.register.js`, `_lib/tracking.js ensureTrackingEndpoint` | Basis-URL-Prüfung `store.rs:16592-16606` (Fehlertexte wörtlich) |
| Versand: Token je Link + Öffnungs-Token, Links → `<basis>/c/<token>`, Pixel `<basis>/o/<token>.gif` vor dem letzten `</body>` (sonst angehängt), Payload-Stempel `tracking_enabled/status='armed'/open_token/click_links` | `_lib/tracking.js prepareTrackedHtml`, Aufruf in `outbound.message.send_approved.js` | `store_outbound_commands.rs:6188-6263` (Regex `(?i)href\s*=\s*["'](https?://[^"']+)["']` 1:1), Aufruf nur bei HTML `:6116` |
| Opt-out/Ausnahmen | `payload.tracking_enabled === false` oder keine Basis-URL → `tracking_enabled:false`, HTML unverändert; Nur-Text-Mails ohne Stempel; `mailto:`/relative Links bleiben | `:6194-6201` |
| Ereignis: Token prüfen (fehlt oder falscher Typ → 404 `tracking token not found`), Ereignis anhängen, `open_count|click_count` +1, `first_*_at_ms` nur beim ersten Mal, `last_*_at_ms`, `tracking_status`, `updated_at_ms` | `outbound.tracking.event.js`, `_lib/tracking.js recordTrackingEvent` | `:6265-6337`, Routen `server.rs:340-376` |
| Antworten: 200 GIF (`Cache-Control: no-store, max-age=0`, `Referrer-Policy: no-referrer`), 302 `Location` nur auf gespeicherte HTTP(S)-Ziele, sonst 500/404 | `_lib/tracking.js pixelResponse/redirectResponse` | `server.rs:4024-4053` |

Collections (neu, `v4/schemas/outbound.collections.schema.json`, Version 0, keine Migration – `outbound_messages`
bleibt unverändert, die Zähler liegen wie in Rust im `payload`): `outbound_tracking_endpoints` (Singleton
`mail_tracking`: `path`, `command_type`, `ingest_path`, `base_url`, `status`; das Ingest-Token des Kerns wird bewusst
nicht synchronisiert), `outbound_tracking_tokens` (id = Token), `outbound_tracking_events` (id = Ereignis-id von
`net.ingest`, Idempotenzschlüssel).

### Kernform `net.ingest` für outbound (Kern §16.38 + Ergänzung, verbindlich; nachgezogen in Strang OUT3)

- **Anmeldung** `net.ingest.webhook.register({path:'mail-tracking', command:'outbound.tracking.event',
  default_response:<Pixel>, aliases:['/mail/track/o/','/mail/track/c/']}) → {ingest_url}`; idempotent je (App, path).
  `command` = Befehlstyp, über den der Kern jede Anfrage zustellt; fehlt der Handler im Manifest, lehnt der Kern mit
  `rejected{invalid}` ab (dann bleibt Tracking aus, Versand ungetrackt; Test „Register: Kern lehnt ab“).
  `default_response` = 200 + GIF (`cache-control: no-store, max-age=0`): antwortet der Handler nicht binnen 3 s, sendet
  der Kern diese Antwort (sonst 202 leer); gilt für den ganzen Pfad, also auch für Klicks. **Kein `pass_headers`**:
  gebraucht wird nur `user-agent`, Cookie/Authorization kommen nie an. `ingest_url` = absolute öffentliche HTTP(S)-URL
  (ohne Zugangsdaten); `ingest_path` speichert outbound als deren Pfadteil. Ohne absolute URL bleibt Tracking aus.
- **Eingang** je HTTP-Anfrage als Befehl `outbound.tracking.event` mit Payload `{event_id, method, path, sub_path, query,
  headers, received_at_ms, body?, body_ref?}`; `body` (UTF-8 ≤ 64 KiB) bzw. `body_ref:{ref}` braucht ein GET-Pixel/Klick
  nicht – sie werden weder gelesen noch gespeichert (Test „§16.38 Nr. 2“). `event_id` je Anfrage eindeutig (fehlt sie,
  dient die Befehls-id).
- **Antwort** = `result.response`, jede läuft durch `ingestResponse` (`_lib/tracking.js`): Status nur 200–299, 301–308,
  400–410, 429; Header nur `content-type`, `location`, `cache-control` (klein); `location` nur absolut http(s);
  Körper ≤ 64 KiB; Verstoß → 404 `not found`. Textkörper als `body`; das GIF legt outbound **einmalig** per
  `ctx.files.put(bytes, {name:'outbound-tracking-pixel.gif', mime:'image/gif'})` ab (ref am Endpunkt als `pixel_ref`,
  sonst beim ersten Abruf nachgeholt) und antwortet mit `body_ref:{ref}` (§16.32/§16.38) – auch als `default_response`.
  **Rückfrage an den Kern:** §16.38 Nr. 3 nennt im Antwortrahmen nur `body`; `body_ref` in der Antwort (und in
  `default_response`) ist dort nicht ausdrücklich erlaubt. outbound sendet es so; `ingestResponse` lässt nur
  `body_ref:{ref:<nicht leer>}` durch. Abweichung zu Rust: ungültiges gespeichertes Ziel → 404 statt 500 (5xx unzulässig),
  `Referrer-Policy` entfällt (nicht in der Whitelist).
- **Altpfade** nur über `aliases` (Kern-Zuordnung). Der Handler erkennt keine Altpfade selbst: er liest aus `sub_path`
  (sonst `path`) die letzten zwei Segmente `o/<t>.gif` bzw. `c/<t>` – dieselbe Regel für den angemeldeten Pfad und
  die Aliase (`parseTrackingPath`).

Trigger-Vorschlag für den Manifest-Generator (`DOC_TRIGGERS.outbound`, Strang Manifeste; hier nicht geschrieben):
`{ handler: 'handlers/outbound.tracking.register.js', every: 'PT1H' }` zusätzlich zu `outbound.scheduler.tick`.
Unabhängig davon meldet der erste Versand den Pfad an, wenn noch kein Endpunkt registriert ist.

### Kern-Import (einmalig, liefert der Kern)

| Quelle im Core (`runtime/ctox.sqlite3`, Mailserver-Store) | Ziel-Collection | Feldabbildung |
|---|---|---|
| Tabelle `stalwart_mail_tracking_tokens` | `outbound_tracking_tokens` | `id` = `token` = `token`; `message_id`; `campaign_id` (NULL → `''`); `event_type` (`opened`/`clicked`); `target_url` (NULL → `''`); `created_at_ms` = `created_at` × 1000 (Rust: Sekunden) |
| Tabelle `stalwart_mail_tracking_events` | `outbound_tracking_events` | `id` = `id`; `token`; `message_id`; `campaign_id` = `campaign_id` des Tokens (Join, NULL → `''`); `event_type`; `occurred_at_ms` = `occurred_at` (Rust schreibt bereits Millisekunden, `now_ms`); `user_agent` |
| Mailserver-Laufzeit `tracking_base_url` | – (kein Import) | entfällt (§16.38 Nr. 5); ersetzt durch `ingest_url` aus `net.ingest`, Altpfade über `aliases`. Einstellung samt UI-Feld in der App mail gelöscht (OUT3) |
| Zähler `open_count`, `click_count`, `first/last_*_at_ms`, `tracking_*` | – | liegen schon in `outbound_messages.payload` (Sync-Store), kein Import |

### Rust-Gleichheit (Tests `tests/outbound.tracking.test.mjs`, 16) und Mutationsbeleg

Konkrete Fälle: zwei absolute Links (einer `HREF = 'HTTP://…'`) umgeschrieben, `mailto:`/relativ unverändert, Pixel vor
`</BODY>`; ohne `</body>` angehängt; Nur-Text ohne Stempel; Opt-out; erster Versand meldet an, zweiter nicht; Basis mit
Zugangsdaten → `tracking_base_url must not contain credentials`; Öffnung zweimal → `open_count 2`, `first` bleibt;
Klick → 302 auf Originalziel, Altpfad gleichwertig; Nachricht gelöscht → Ereignis trotzdem gezählt; Nicht-GET → 404.

Invarianten mit Mutationsbeleg (`node tests/mutation-proof.mjs`, Liste `TRACKING_MUTATIONS`, je Mutation rot, danach grün):
TRK1 Opt-out (`explicitlyDisabled`), TRK2 Ereignis-Idempotenz (Duplikatprüfung; Fallback Befehls-id), TRK3 Typprüfung
Token/Pfad, TRK4 kein offener Redirect (Schemaprüfung; leeres Ziel → 404), TRK5 unbekanntes Token → 404 ohne Schreiben.
TRK6 (OUT3, §16.38 Nr. 3) Antwortgrenzen in `ingestResponse`: Header-Whitelist, Status-Whitelist, `body_ref` nur mit
ref, absolute Location, Körper ≤ 64 KiB (5 Mutationen, alle rot).

Abweichungen zu Rust (bewusst): (1) ein von `net.ingest` erneut zugestelltes Ereignis zählt nicht doppelt (Rust kannte
keine Wiederzustellung, jede HTTP-Anfrage zählte – das bleibt so, jede Anfrage hat ihre eigene `event_id`);
(2) ist `net.ingest` nicht erreichbar, geht die Mail ungetrackt raus statt zu scheitern (Rust scheiterte nur, wenn die
Mailserver-Einstellungen nicht ladbar waren); (3) leere `campaign_id`/`target_url` als `''` statt NULL (Schema).

## Nachtrag OUT3 (15.10.2026): App-Logik raus aus dem Core

Owner-Regel: keine App-Logik im Core. Ausgang (`check-caps --calls`, OHNE_VERTRAG): outbound rief 4× `legacy.outbound.*`,
6× `scrape.*` und 2× `research.person` auf. Stand jetzt: **0** (`grep -rnE "cap\.(legacy|scrape|research\.person)|legacy\[" handlers --include=*.js`
→ 0 Treffer; Test `legacy-forwarders.test.mjs` prüft es). `agent.command` (im Auftrag genannt): in outbound nie
aufgerufen (grep `agent\.command` über `handlers/` → 0 Treffer; die MAPPING-Zeile `agent.command.settle` gehört der App research).

| vorher (Core) | jetzt (App) | Datei | Rust-Referenz |
|---|---|---|---|
| `legacy.outbound.research_source.test` | Weitergabe an die Eigentümer-App OLG per `ctx.command(type, payload, {id:'<id>:outbound-lead-generation', module:'outbound-lead-generation', record_id, client_context})`; dort Port über `process.run` (Strang OLGSCR) | `outbound.research_source.test.js`, `_lib/owner-app.js` | `store_outbound_commands.rs:1177 → :2776`, Testzweig `:3330-3758` |
| `legacy.outbound.research_source.reprobe` | dito | `outbound.research_source.reprobe.js` | `:46 → :4090-4121`, `execute.rs:377-440` |
| `legacy.outbound.update_digest.send_now` | dito (OLG: Bericht + `mail.send`) | `outbound.update_digest.send_now.js` | `:1184 → outbound_update_digest.rs` |
| `legacy.outbound.sellify.lookup` | Fachlogik hier über foreign read `sellify_companies/people/campaigns` (Eigentum OLG §16.21); inhaltsgleich mit OLG `_lib/sellify-lookup.js`, gleiche Tests | `outbound.sellify.lookup.js`, `_lib/sellify-lookup.js` | `:1183 → :1673`, `:1483-2048` |
| `scrape.registry.read` | foreign read der OLG-Registry (Ziele updated_at DESC, letzte/erfolgreiche Läufe, Konto-Zustände, Skript über `files.get`) | `outbound.research_source.registry_read.js`, `_lib/olg-registry.js` | `registry.rs:18`, `:1306-1481` |
| `scrape.target.registration` ×2 | foreign read des Ziel-Dokuments (`script_registered`, `workspace_dir`, Revision) | `_lib/scrape-generation.js`, `_lib/olg-registry.js targetRegistration` | `registry.rs:66` |
| `scrape.target.upsert` ×2 | `ctx.command('outbound.scrape.target.upsert', {manifest})` an OLG (Angebot OLG V4-HANDLERS „Angebot an den Strang outbound“) | `_lib/scrape-generation.js`, `outbound.research_policy.publish.js` | `:3937`, `:4239`, `:4529` |
| `scrape.repair_task.create` | `agent.task.enqueue` flach (§16.30) mit `on_complete`/`on_failed` = `outbound.research_source.generate_adapter.done` (Ergebnis am Adapter: registriertes Skript → `adapter_ready/registered`, sonst `adapter_failed/generation_incomplete` bzw. `generation_failed`; idempotent je task_id, älterer Auftrag ändert nichts); Wortlaut der Anweisung wie OLG (Befehle `outbound.scrape.script.register`/`outbound.scrape.execute` statt `ctox scrape …`) | `_lib/scrape-generation.js`, `outbound.research_source.generate_adapter.done.js` | `:3759`, `channels::create_scrape_repair_queue_task` |
| `research.person` ×2 (= `ctox_web_stack::run_ctox_person_research_tool`, eine Recherche-Engine im Core) | Phase 1 `agent.task.enqueue({title, instruction, thread_key, idempotency_key:'<type>:<id>', payload:{command, research_request}, on_complete/on_failed:'<type>.done'})`; Rückruf `<type>.done` liest den Ausgangsbefehl über `agent.task.get`, Phase 2 unverändert | `outbound.company.research.js`, `outbound.pipeline.contact_research.js`, `*.done.js`, `_lib/research-task.js` | `importer.rs:228/:315`, `:229/:479`, `:3266` |

### Capabilities (nur OS-Primitive; neuer Bedarf aus OUT3)

- `cap.agent.task.enqueue` (Generierungsauftrag Scraper, Recherche-Aufträge), `cap.agent.task.get` (Rückrufe).
- `cap.files.get` (Skripttext der OLG-Registry für `registry_read` mit `include_script_target`).
- `ctx.files.put` (§16.35 Nr. 1; Tracking-Pixel einmalig als files-Datensatz für `body_ref`).
- entfallen: `scrape.registry.read`, `scrape.target.registration`, `scrape.target.upsert`, `scrape.repair_task.create`,
  `research.person`, `legacy.*` (4). Unverändert übrige: `knowledge.query`, `mail.*`, `audit.event`, `secrets.exists`,
  `browser.auth_assist.request`, `browser.automation.run`, `artifact.write`, `net.ingest.webhook.register`.

### Manifest-Wünsche (für den Manifest-Strang; Generator hier nicht geändert)

- `handlers`: neu `outbound.company.research.done`, `outbound.pipeline.contact_research.done`,
  `outbound.research_source.generate_adapter.done` (Rückruf-Befehle §16.30 Nr. 3).
- `foreign_collections` (`DOC_FOREIGN.outbound.wishes`): `outbound_lead_generation_scrape_targets`, `…_scrape_runs`,
  `…_scrape_account_states`, `…_scrape_scripts` je `read` (OLG exportiert sie, s. OLG V4-HANDLERS „Angebot“);
  `sellify_companies`, `sellify_people`, `sellify_campaigns` je `read` – **OLG-Export nötig** (heute exportiert OLG nur
  `outbound_lead_generation_adapters`/`outbound_research_policies`; Wunsch an OLG/Manifest-Strang).
- Befehls-Ziel: outbound sendet an OLG die Typen `outbound.scrape.target.upsert`, `outbound.research_source.test`,
  `outbound.research_source.reprobe`, `outbound.update_digest.send_now` (`ctx.command` mit `module:'outbound-lead-generation'`).
- **DOC_TRIGGERS.outbound** zusätzlich: `{handler:'handlers/outbound.tracking.register.js', every:'PT1H'}` (neben
  `outbound.scheduler.tick` PT5M).
- `capabilities_required` folgt aus dem Handler-Code (Generatorlauf): s. „Capabilities“ oben.

### Kern-Import

Keiner für OUT3: die Recherche hatte keine Core-Daten; Scrape-Registry-Daten importiert der Kern für OLG (OLG
V4-HANDLERS „Kern-Import“); die Sellify-Projektionen bestehen bereits als OLG-Collections. Tracking unverändert
(Abschnitt „Mail-Tracking“).

### Invarianten und Mutationsbeleg (`node tests/mutation-proof.mjs`, Liste `OUT3_MUTATIONS`, Test `tests/out3.invariants.test.mjs`)

| Inv. | Regel | Mutation (je rot, danach grün) |
|---|---|---|
| OUT3A | Rückruf wirkt nur für eine Aufgabe des eigenen Befehlstyps | Typprüfung in `loadResearchCallback` entfernt |
| OUT3B | gescheiterte Recherche: keine Firmen-/Kontaktzeile, nur Laufstatus `failed` | Statuszweig in beiden `.done`-Handlern entfernt (2) |
| OUT3C | Ablehnung durch die Eigentümer-App ist ein Fehler (`unavailable`), nie stilles ok | Ablehnungsprüfung in `owner-app.js` entfernt |
| OUT3D | gleiche Ausgangs-Id → gleiche OLG-Befehls-Id (keine Doppelläufe) | Folge-Id zufällig |
| OUT3E | `probe_in_progress` nur bei nicht abgelaufener Lease (INV-SCR4) | `>` → `>=` |
| OUT3F | Sellify-Kampagnengruppen ohne gelöschte Kampagnen; read-only | `is_deleted`-Filter entfernt |
| OUT3G | Generierungs-Rückruf: `adapter_ready` nur mit registriertem Skript | Registrierungsprüfung entfernt |
| OUT3H | Generierungs-Rückruf idempotent je task_id | Duplikatprüfung entfernt |
| OUT3I | Rückruf eines älteren Auftrags ändert den Status nicht | Aktualitätsprüfung entfernt |

### Abweichungen (bewusst)

- Recherche ist asynchron: `outbound.company.research`/`outbound.pipeline.contact_research` liefern die Annahme
  `{ok, status:'accepted', execution_mode:'agent_task', task_id, callback}`; das frühere Ergebnis (`record_ids`,
  `records_count`) liefert der Rückruf-Befehl. Eine gescheiterte Aufgabe schreibt Laufstatus `failed` (Rust: Befehl
  scheiterte ohne Laufzeile). „keine öffentlichen Kontakte“ bleibt `not_found` (im Rückruf).
- Die Recherche-Engine (Suchplan/Quellenwahl/Extraktion) läuft jetzt im Agenten mit der App-Anweisung
  (`_lib/research-task.js researchInstruction`); das Ergebnisformat `{fields:{…}}` ist das der Rust-Engine.
- Weitergaben an OLG liefern die Annahme, nicht das OLG-Ergebnis (§16.21 `ctx.command`). Die OLG-UI sendet diese Typen
  selbst an OLG; outbound-Pfade sind nur noch Zubringer.
- Neues Ziel bei `generate_adapter`: der Generierungsauftrag nennt den Arbeitsbereich, den OLG deterministisch vergibt
  (`outbound-lead-generation/scrape/targets/<slug>`), weil das Anlegen asynchron über `ctx.command` läuft.
- `generate_adapter`: Rust hatte keinen Rückweg vom Generierungsauftrag (Status blieb `generation_queued` bis zum
  Test); jetzt setzt der Rückruf den Ausgang (`check-skills`: kein enqueue ohne Rückruf).
- `research_policy.publish`: `manifest_path` in OLG-Form `outbound_lead_generation_scrape_targets/<target_key>`.

### App mail: `tracking_base_url` gelöscht

Liest außer der mail-Einstellung niemand mehr (grep `tracking_base_url` über `src/apps/business-os` vorher: mail-Handler
`ctox.mailserver.save_runtime`, mail-UI `index.js`/`index.html`, mail-Tests; outbound nur Fehlertexte der Basis-URL-Prüfung).
Gelöscht: Feld + Prüfung in `save_runtime` (ein mitgesendeter Wert wird ignoriert, nicht an den Kern gegeben; Test),
UI-Feld „Öffentlich erreichbare Tracking-URL“, Fixtures, `V4-PORT.md`-Projektion.



## Strang SKL-O (15.10.2026): Automatisierungsknöpfe mit App-Skills, Templates und Rückruf (Kern §16.30/§16.34)

| Auslöser | vorher | nachher |
|---|---|---|
| Campaign mit geändertem Briefing speichern (`index.js` `requestCampaignSetup`) | `business_os.chat.task` über die CTOX-App (`dispatchCampaignSetupPromptTask`/`dispatchOutboundPromptTask`), Prompt `campaignSetupPrompt` im UI, Skill `business-os-outbound-campaign-setup` (Core), Agent schrieb per CLI `outbound.campaign.apply_setup` | `outbound.campaign.setup.request` → `prompts/campaign-setup.md` → `agent.task.enqueue({skill:'outbound/campaign-setup', idempotency_key:'outbound-campaign-setup-<source_command_id>', on_complete:'outbound.campaign.setup.done', on_failed:'outbound.campaign.setup.failed'})`; `.done` bildet `{task_id,status,result}` auf `outbound.campaign.apply_setup` ab |
| Spalte „Anschreiben generieren“ (`generateOutreachForContact`) | `outbound.pipeline.outreach_draft` ohne Handler; Anweisung `t('outreachDraftInstruction')` im UI, Skill per Typname im Core (`store.rs:27743`), Skillbook per MCP, Agent schrieb per CLI `outbound.pipeline.write_outreach_draft` | Handler `outbound.pipeline.outreach_draft` → `prompts/outreach-draft.md` (Skillbook aus `outbound_skillbooks`, sonst Vorgabe) → `agent.task.enqueue({skill:'outbound/message-drafting', on_complete:'….done', on_failed:'….failed'})`; `.done` bildet auf `outbound.pipeline.write_outreach_draft` ab |
| Firmen-/Kontaktrecherche (`_lib/research-task.js`, OUT3) | `agent.task.enqueue` ohne Skill | `skill:'outbound/prospect-research'` |

Paket: `skills/campaign-setup`, `skills/message-drafting`, `skills/prospect-research` (Kopien aus `src/skills/system/…`,
ohne CLI-/MCP-Rückschrieb; Ergebnis = Antwort), `prompts/campaign-setup.md`, `prompts/outreach-draft.md`.

Zustand (nur Datensatz + Zustandszone, `index.js` `reportAgentTaskState`):
- `outbound_campaigns.payload.campaign_setup_task` = `{task_id, source_command_id, status: queued|completed|failed|cancelled, error?}`;
  `outbound.campaign.briefing.update` schreibt ihn nicht mehr (Wettlauf mit `task_id`). `apply_setup` übernimmt `task_id`.
- Kontakt: `outreach_task_id`, `outreach_request_id`, `outreach_generating`, `outreach_status` (`queued` → `drafted` |
  `draft_failed` | `draft_cancelled`), `outreach_error`. Die UI schreibt die Flags nicht mehr selbst.

Antwortauswertung (früher im Agenten/Skill): erstes JSON-Objekt (`_lib/agent-callback.js`, ref `store.rs:23796`);
Setup-Patch nur mit den Schlüsseln des früheren Schreibvertrags, `campaign_setup.status` Pflicht; Entwurf nur mit allen
vier Feldern. Unbrauchbare Antwort → `failed` mit Grund. Idempotent je `task_id` (Doppelzustellung, veralteter Auftrag).

Tests: `handlers/tests/outbound.automation.sim.test.mjs` (SKLO1–SKLO8, agent-task-Simulation); Mutationsbeleg
`node handlers/tests/mutation-proof.mjs` (Teil SKLO, Spiegel von business-os mit `ctox:shared`-Lader).

Kern-Abbau-Hinweise (tot, sobald dieser Stand aktiv ist): `src/skills/system/communication/business-os-outbound-campaign-setup`,
`…/business-os-outbound-message-drafting`, `src/skills/system/research/prospect-research` (Quelle jetzt im Paket);
`store.rs:27743` (`suggested_skill_for_command` für `outbound.pipeline.outreach_draft`); `store_outbound_commands.rs:5677`
(Skill-Vorgabe in `apply_setup`); Chat-Pfad für das Campaign-Setup (`business_os.chat.task` mit `required_skills`/
`writeback_contract` aus Outbound).
