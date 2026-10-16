# Manifest-Wünsche der Stränge (Strang MAN, 15.10.2026)

Erfasst per `grep -n -i -E 'manifest|generator'` über `modules/*/V4-HANDLERS.md`, `modules/*/V4-PORT.md`,
`customer-modules/*/V4-*.md` (224 Treffer; Boilerplate „In-Memory-ctx“ und reine Erwähnungen ohne Wunsch ausgenommen)
plus `v4/frame/INTEGRATION.md` FR2-4. Quelle des Generators ist **nicht** mehr der Text: abgeleitet wird aus dem Code;
was sich nicht ableiten lässt, steht in `<app>/v4-manifest.json` (Format `ctox-shell-v4-app-declaration-1`, `_basis` = Fundstelle).
Zeilen = Stand 15.10. ~19:00.

Legende „umgesetzt wie“: **Code** = Generator leitet es aus Handler-/UI-Code bzw. Paketdateien ab · **Dekl.** = Feld in
`v4-manifest.json` · **offen** = nicht umgesetzt, Grund dahinter.

| App | Wunsch | Quelle (Datei:Zeile) | umgesetzt wie |
|---|---|---|---|
| app-store | foreign nur business_apps/business_app_versions; Fehltreffer documents/notes/spreadsheets weg; files braucht write | modules/app-store/V4-PORT.md:127 | Dekl. `foreign_exclusive` + `foreign_collections` (files read,write) |
| app-store | schema.js nicht mehr in files | modules/app-store/V4-PORT.md:127 | Code (files = Dateibaum; schema.js gelöscht) |
| app-store | keine eigene Collection, beide sind Plattform (OFFEN-8) | modules/app-store/V4-PORT.md:78 | Code (Eigentum ctox-platform) |
| appsec-pentest | handlers, capabilities, styles; business_commands raus; foreign business_apps | modules/appsec-pentest/V4-PORT.md:69 | Code (handlers/caps/styles); Dekl. foreign business_apps; business_commands = Plattform |
| appsec-pentest | modulweiter Zustand → nur ein Fenster | modules/appsec-pentest/V4-PORT.md:71 | Dekl. `multi_instance:false` |
| browser | foreign lesen: ctox_queue_tasks, outbound_research_adapters, outbound_lead_generation_adapters | modules/browser/V4-PORT.md:141 | Dekl. (nur read; :107 „liest nur, schreibt nicht“ – vorher read,write) |
| browser | `multi_instance: false` | modules/browser/V4-PORT.md:159 | Dekl. |
| browser | Identitäts-Migrationen als `{operations: []}` nachtragen | modules/browser/V4-PORT.md:106 | offen: Schemaquelle ist v4/schemas (Schema-Strang), nicht der Manifest-Generator |
| buchhaltung | styles, handlers | modules/buchhaltung/V4-PORT.md:87 | Code |
| calendar | `vendor/event-calendar/event-calendar.min.css` in styles | modules/calendar/V4-PORT.md:63 | Dekl. `styles` (Datei im Paket geprüft); `<link>`-Weg streicht der App-Strang |
| coding-agents | handlers, capabilities_required, foreign business_apps/business_commands, styles | modules/coding-agents/V4-PORT.md:66 | Code + Dekl. foreign |
| consent | foreign 9 ATS-Collections read,write (Eigentümer müssen exportieren) | modules/consent/V4-HANDLERS.md:25 | Dekl.; exports der Eigentümer aus der Nutzung |
| consent, esign, intake, interviews, nachweise, placements, submissions | command_rights `ats.*` chef/admin (Kern-Policy) | modules/<app>/V4-HANDLERS.md Abweichung 2 | Dekl. `command_rights` |
| consent, nachweise, placements, submissions (+ esign/intake/interviews Text) | Konfig-Schlüssel je App im Manifest freigeben | modules/consent/V4-HANDLERS.md:67, interviews:60, esign:54, intake:50, nachweise:57, placements:53, submissions:52 | Code → `_config_keys` (über Hüllfunktionen _ats.js); offen: Kernvertrag hat kein Vertragsfeld für Konfig-Freigaben (esign/intake/interviews rufen config.get nicht) |
| conversations | `exports.communication_messages` mit watch in der Quelle der App festhalten | modules/kundenpipeline/V4-HANDLERS.md:120 | Dekl. `exports` read,watch,write (write: §16.35 Nr. 7) |
| conversations | business_commands-Eintrag kann weg | modules/conversations/V4-PORT.md:177 | Dekl. `foreign_drop` |
| conversations | foreign outbound_* read | modules/conversations/V4-PORT.md:40 | Code (Zeilen-Scan) |
| conversations | collections.schema.json auf `id` nachziehen | modules/conversations/V4-PORT.md:178 | offen: Schema-Strang (v4/schemas), außerhalb |
| creator | business_commands ist System-Collection | modules/creator/V4-PORT.md:71 | Code (Plattform) |
| credentials | command_rights `ctox.secret.*` SecretsManage | modules/credentials/V4-HANDLERS.md; OLG V4-PORT.md §13 | Dekl. |
| credentials | business_commands aus collections | modules/credentials/V4-PORT.md:67 | Code (Plattform) |
| ctox | command_rights crew/queue/task | modules/ctox/V4-HANDLERS.md:180 | Dekl. (Rollen-_todo bleibt: policy.rs-Übertragung) |
| ctox | Collection-Leserechte (8) | modules/ctox/V4-HANDLERS.md:262 | Dekl. `collection_rights` |
| ctox | documents/communication_messages nur Text-Literal → raus | modules/ctox/V4-PORT.md:136 | Dekl. `foreign_drop` |
| ctox | workjet_* (9) aus ctox herausnehmen (Kern Phase 11) | modules/ctox/V4-PORT.md:132 | offen: Eigentümerwechsel ist Entscheidung Kern Phase 11 (ownership-baseline.json), kein Generatorwunsch |
| ctox | handlers, styles | modules/ctox/V4-PORT.md:137 | Code |
| ctox | §16.40 commands_in ctox.chat.task und ctox.app.modify.request von `*` | Kern §16.40; shared/context-chat.js (SKL-D) | Dekl. `commands_in` |
| ctox (für threads) | exports workjet_project_chats, workjet_worker_profile_bindings read | modules/threads/V4-HANDLERS.md:231 | Dekl. `exports` |
| ctox (für threads) | exports workjet_supervisor_bindings read | modules/threads/V4-HANDLERS.md:229 | offen: Collection existiert noch nicht (Kern Phase 11) |
| customers | handlers (20), foreign outbound_companies/outbound_pipeline_items read, capabilities [] | modules/customers/V4-HANDLERS.md:117-124 | Code (handlers/caps) + Dekl. foreign |
| customers | styles index.css, schema.js raus | modules/customers/V4-PORT.md:124 | Code |
| customers | multi_instance | modules/customers/V4-PORT.md:126 | Dekl. |
| cv-print-builder | foreign files read, documents/document_versions rw, desktop_files read (kein write), desktop_file_chunks read, business_chats entfällt | modules/cv-print-builder/V4-HANDLERS.md:40; V4-PORT.md:120 | Dekl. foreign + `foreign_drop` |
| cv-print-builder | skills parser, prompts parse, Handler `.failed` | modules/cv-print-builder/V4-HANDLERS.md:95 | Code (skills/, prompts/, handlers/) |
| cv-print-builder | command_rights `ctox.cv_print.apply_parse` | modules/cv-print-builder/V4-PORT.md:122 | offen: Wunsch nennt keine Rollen; ohne Eintrag gilt §16.20 Nr. 2 (Rollen mit App-Schreibrecht) |
| cv-print-builder | multi_instance:false | modules/cv-print-builder/V4-PORT.md:137 | Dekl. |
| desktop | Eigentum desktop_files/desktop_file_chunks/channel_pairing_state klären | modules/desktop/V4-PORT.md:82 | offen: Eigentümer-Entscheidung (ownership-baseline), kein Generatorwunsch |
| documents | knowledge_* nur lesen (foreign), document_blob_chunks entfällt, business_commands System | modules/documents/V4-PORT.md:124 | Code (Eigentum knowledge) + Dekl. foreign read |
| documents | ctox_runtime_settings foreign/Plattform-Lesepfad | modules/documents/V4-PORT.md:127 | Dekl. foreign read (Plattform-export read) |
| documents | commands_in research.systematic.report.create von research | Gegenstück zu Code research/index.js:2685 (§16.40) | Dekl. `commands_in` |
| esign | styles, handlers | modules/esign/V4-PORT.md:52 | Code |
| explorer | desktop_files read,write (+ Lesezugriffe OFFEN 6) | modules/explorer/V4-PORT.md:73 | Dekl. foreign |
| file-viewer | nur files + business_commands lesen, keine App-Collection | modules/file-viewer/V4-PORT.md:71 | Dekl. `foreign_exclusive` |
| file-viewer | schema.js nicht in files | modules/file-viewer/V4-PORT.md:73 | Code |
| importer | desktop_files/desktop_file_chunks write, business_commands read | modules/importer/V4-PORT.md:50, :72 | Dekl. foreign |
| intake | handlers, styles | modules/intake/V4-PORT.md:69 | Code |
| interviews | foreign desktop_files read | modules/interviews/V4-HANDLERS.md:18 | Dekl. |
| invoices | foreign accounting_journal_entries/lines/number_series read,write | modules/invoices/V4-HANDLERS.md:53 | Dekl. |
| invoices | Abhängigkeiten customer_accounts u. a. als foreign | modules/invoices/V4-PORT.md:86 | Dekl. customer_accounts read |
| invoices | multi_instance (modulweit) | modules/invoices/V4-PORT.md:87 | Dekl. |
| iot | Trigger signal.evaluate on iot_attributes insert/update, rules.tick every PT1M | modules/iot/V4-HANDLERS.md:50 | Dekl. `triggers` |
| iot | DYNAMIC_CAPS-Regel `./_iot.js` entfällt | modules/iot/V4-HANDLERS.md:52 | Generator: DYNAMIC_CAPS leer |
| iot | handlers task.complete/.failed, skills operations, capabilities agent.task | modules/iot/V4-HANDLERS.md:129 | Code |
| iot | handlers webhook.ingest; command_rights nur Zustellung durch net.ingest | modules/iot/V4-HANDLERS.md:160, :141, :210 | Code: `_net_ingest` aus `net.ingest.webhook.register` (command/path/aliases); command_rights `{roles: []}` für den Zustelltyp |
| iot | capabilities nach Kernvertrag (knowledge.query statt dataframe.*) | Auftrag; iot-Code | Code (Handler rufen knowledge.query) |
| knowledge | Klassen window + client_limit, row_filter actor_id, Trigger view.window insert/update | modules/knowledge/V4-PORT.md:150 | Dekl. `collection_overrides`, `row_filters`, `triggers` |
| knowledge | knowledge_tasks in collections; skills/prompts | modules/knowledge/V4-PORT.md:230, :259 | Code (v4/schemas, skills/, prompts/) |
| knowledge | handlers knowledge.book.ingest; commands_in nur von research | modules/knowledge/V4-PORT.md:259-268 | Code (handlers) + Dekl. `commands_in` |
| knowledge | handlers, capabilities, styles; business_commands raus | modules/knowledge/V4-PORT.md:104 | Code |
| kundenpipeline | foreign communication_messages read,watch; Trigger mail.inbound on insert; skills/prompts | modules/kundenpipeline/V4-HANDLERS.md:116 | Dekl. foreign + triggers; Code skills/prompts |
| kundenpipeline | handlers; business_commands raus | modules/kundenpipeline/V4-PORT.md:66 | Code |
| mail | handlers mail.content.freeze, capabilities office.edit/office.render/blob, mail.server.domain/user.delete | modules/mail/V4-HANDLERS.md:81-91 | Code |
| mail | command_rights mail.content.freeze = Rollen mit Schreibrecht | modules/mail/V4-HANDLERS.md:83 | Standard §16.20 Nr. 2 (ohne Eintrag = Rollen mit App-Schreibrecht), kein Eintrag nötig |
| mail | document_versions/document_blob_chunks nicht mehr nötig | modules/mail/V4-HANDLERS.md:84 | Dekl. `foreign_drop` |
| mail | styles index.css + editor/editor.css | modules/mail/V4-PORT.md:164 | Dekl. `styles` |
| mail | foreign für 11 gelesene Collections + mail_server_state | modules/mail/V4-PORT.md:165, :225 | Code (Zeilen-Scan) + Dekl. mail_server_state read |
| matching | styles, handlers, capabilities doc.extract/doc.html/web.fetch | modules/matching/V4-PORT.md:110 | Code |
| matching | multi_instance:false | modules/matching/V4-PORT.md:93 | Dekl. |
| nachweise | foreign planning_time_records rw, signature_requests r, accounting_invoices rw | modules/nachweise/V4-HANDLERS.md:22 | Dekl. |
| nachweise | Leserecht business_credentials admin/chef | modules/nachweise/V4-HANDLERS.md:59 | Dekl. `collection_rights` |
| notes | conflictStrategy field-merge ausdrücken | modules/notes/V4-PORT.md:68 | offen: §16.27 bildet field-merge auf die Core-Regel ab; kein Manifestfeld |
| outbound | handlers (Vorschlag), compose-Nachtrag, .done-Rückrufe | modules/outbound/V4-HANDLERS.md:191, :223, :225, :453 | Code |
| outbound | foreign Adapter-Writeback, outbound_research_policies | modules/outbound/V4-HANDLERS.md:191 | Dekl. read,write |
| outbound | foreign OLG scrape_targets/runs/account_states/scripts read, sellify_* read | modules/outbound/V4-HANDLERS.md:455-458 | Dekl. |
| outbound | Befehle an OLG (scrape.target.upsert, research_source.test/reprobe, update_digest.send_now) | modules/outbound/V4-HANDLERS.md:459 | Code `commands_out` (Konstante + Weiterleitung command.type) |
| outbound | Trigger scheduler.tick PT5M, tracking.register PT1H | modules/outbound/V4-HANDLERS.md:392, :460; V4-PORT.md:114 | Dekl. `triggers` |
| outbound | net.ingest-Handler outbound.tracking.event | modules/outbound/V4-HANDLERS.md:369 | Code `_net_ingest` + command_rights `{roles: []}` |
| outbound | neue Collections outbound_tracking_* | modules/outbound/V4-HANDLERS.md:359 | Code (v4/schemas) |
| outbound | capabilities aus dem Code; business_commands/ctox_queue_tasks Plattform; styles | modules/outbound/V4-HANDLERS.md:463; V4-PORT.md:135 | Code |
| outbound-lead-generation | exports scrape_targets/runs read,watch, account_states/scripts read, Witnesses privat | customer-modules/outbound-lead-generation/V4-HANDLERS.md:52-56 | Dekl. `exports` |
| outbound-lead-generation | Sellify-Export für outbound | modules/outbound/V4-HANDLERS.md:457 | Dekl. `exports` sellify_* read |
| outbound-lead-generation | commands_in für die vier outbound-Befehle | customer-modules/outbound-lead-generation/V4-HANDLERS.md:58-59 | Dekl. `commands_in` from outbound |
| outbound-lead-generation | registry_read raus, reconcile.done rein | customer-modules/outbound-lead-generation/V4-HANDLERS.md:118 | Code (handlers/) |
| outbound-lead-generation | Trigger email_validation_sweep PT1H, Befehl email_validate | customer-modules/outbound-lead-generation/V4-HANDLERS.md:150 | Dekl. `triggers`; Code handlers |
| outbound-lead-generation | capabilities nach Kernvertrag (browser.capture, files.*, process.run, …; legacy 0) | customer-modules/outbound-lead-generation/V4-HANDLERS.md:61-68; V4-PORT.md:346 | Code |
| outbound-lead-generation | foreign outbound_research_adapters read,write | customer-modules/outbound-lead-generation/V4-PORT.md:353 | Dekl. |
| outbound-lead-generation | files ohne schema.js-Loader, mit v4-port.mjs u. a. | customer-modules/outbound-lead-generation/V4-PORT.md:431 | Code (Dateibaum) |
| outbound-lead-generation | layout.multi_instance:false | customer-modules/outbound-lead-generation/V4-PORT.md:441, :499 | Dekl.; Generator schreibt `layout.multi_instance` und `layout.presentation.multi_instance` (Rahmen liest letzteres) |
| outbound-lead-generation | customer-modules erfassen; Eigentum Adapter/Sellify/Policies; write_guard; Logo | customer-modules/outbound-lead-generation/V4-PORT.md:329-339, :106 | Code (APP_ROOTS, OWNER_RESOLUTIONS) + Dekl. `write_guard`, `layout_icon_svg_from` |
| placements | foreign business_credentials r, accounting_invoices rw; Konfig-Schlüssel | modules/placements/V4-HANDLERS.md:19 | Dekl. + Code `_config_keys` |
| reports | foreign-Liste exklusiv (8), business_module_reports write | modules/reports/V4-PORT.md:53-69, :83 | Dekl. `foreign_exclusive` |
| reports | handlers delegation.complete/.failed; Beleg _delegation.js | modules/reports/V4-PORT.md:133 | Code |
| reports | Befehl threads.ctox_approval.request an threads | modules/reports/index.js:700 (Code) | Code `commands_out`; Gegenstück Dekl. threads `commands_in` |
| reports | business_apps fehlt in module.json | modules/reports/V4-PORT.md:91 | Dekl. foreign read |
| research | foreign business_commands read | modules/research/V4-HANDLERS.md:50 | Dekl. |
| research | commands_out knowledge.book.ingest | modules/research/V4-HANDLERS.md:167 | Code (`ctx.command(KNOWLEDGE_INGEST, …, {module: KNOWLEDGE_APP})`) |
| research | foreign knowledge_table_rows, ctox_queue_tasks read | modules/research/V4-PORT.md:74 | Dekl. |
| spreadsheets | spreadsheet_blob_chunks entfällt, desktop_files read | modules/spreadsheets/V4-PORT.md:89 | Dekl. foreign desktop_files; Chunk-Collection bleibt Klasse blob bis zur Ablösung (§10.2a) |
| submissions | foreign business_consents read; Konfig-Schlüssel | modules/submissions/V4-HANDLERS.md:18 | Dekl. + Code `_config_keys` |
| support | Trigger intake.project on communication_messages | modules/support/V4-HANDLERS.md:81 | Dekl. `triggers` insert/update |
| support | command_rights aus SUPPORT_COMMAND_PERMISSIONS | modules/support/V4-HANDLERS.md:82 | Code (`*_COMMAND_PERMISSIONS`) |
| support | foreign communication_threads/messages read | modules/support/V4-HANDLERS.md:83 | Dekl. |
| support | Befehl an tickets ctox.ticket.local.create | modules/support/V4-HANDLERS.md:79 | Code `commands_out`; Gegenstück Dekl. tickets `commands_in` |
| support | Fremd-/System-Collections; desktop_files nicht gelesen | modules/support/V4-PORT.md:85 | Code (Zeilen-Scan/Plattform) |
| threads | Zulassung approve/reject external.approve, übrige authentifiziert | modules/threads/V4-HANDLERS.md:134 | Dekl. `command_rights` |
| threads | W-4 Projektions-Handler | modules/threads/V4-HANDLERS.md:149 | Dekl. `_proposed_triggers` (Anmerkung); offen: Vertrag kennt nur triggers §16.20 |
| threads | W-5 Sichtbarkeitsregel `visibility` | modules/threads/V4-HANDLERS.md:151 | offen: kein Vertragsfeld (Kern-Entscheidung nötig) |
| threads | Peer-Schreibschutz „nur über Befehle“ (`write: 'commands'`) | modules/threads/V4-HANDLERS.md:146 | offen: §16.21 write_guard kennt nur command_only_fields je Feld, keine ganze Collection |
| threads | prompts + Handler; Weiterleitung an reports.delegation.* deklarieren | modules/threads/V4-HANDLERS.md:312-313 | Code (prompts/handlers) + Dekl. `commands_out` (Ziel aus approval.callback, `from_file` handlers/_agent.js) |
| threads | foreign workjet_projects read | modules/threads/V4-PORT.md:146 | Dekl. (ctox exportiert read) |
| threads | ctox_queue_tasks-Migration 3 ergänzen | modules/threads/V4-PORT.md:74, :151 | offen: Plattform-Collection, Migration im Plattformmanifest/Schema-Strang |
| threads | multi_instance:false | modules/threads/V4-PORT.md:155 | Dekl. |
| tickets | foreign ctox_queue_tasks, ctox_crew_members, business_commands; Capabilities ctox.ticket/agent.crew | modules/tickets/V4-PORT.md:85 | Dekl. foreign; Capabilities aus dem Code |
| tickets | Crew-Stylesheet als Manifest-styles | modules/tickets/V4-PORT.md:83 | offen: Datei liegt in shared/ (Shell), nicht im App-Paket – Shell-Strang |
| alle 23 Apps mit modulweitem Zustand | `multi_instance:false` | v4/frame/INTEGRATION.md:198-209 | Dekl. `multi_instance:false` (knowledge, calendar, appsec-pentest, buchhaltung, creator, coding-agents, customers, invoices, iot, kundenpipeline, matching, news, outbound, notes, reports, tickets, shiftflow, support, threads, browser, documents, cv-print-builder, OLG) |

## Zählung

| | Anzahl |
|---|---:|
| Wünsche erfasst (Tabellenzeilen) | 115 |
| umgesetzt (Code oder Deklaration) | 103 |
| offen bzw. nur als Anmerkung (Grund in der Zeile) | 12 |

Offen: browser Migrationsoperationen, conversations Schema auf `id`, ctox workjet_*-Eigentum, ctox workjet_supervisor_bindings
(Collection fehlt), cv-print command_rights ohne Rollen, desktop Eigentum, notes field-merge, threads W-4/W-5/Collection-
Schreibschutz, threads ctox_queue_tasks-Migration, tickets Crew-Stylesheet, Konfig-Freigabe ohne Vertragsfeld (als
`_config_keys` angemerkt) – je außerhalb des Generators oder ohne Vertragsfeld.

Zusätzlich aus dem Code abgeleitet, ohne Wunschtext: `mail → support support.conversation.open_from_thread`
(mail/index.js:2432, Gegenstück Dekl. support `commands_in`), `research → documents research.systematic.report.create`
(research/index.js:2685), Kontext-Chat-Befehle aller Apps mit `shared/context-chat.js` (buchhaltung, desktop, documents,
knowledge, mail, matching, research, spreadsheets → ctox). Nicht auflösbar bleibt `threads.ctox_approval.approve.js:79`
(Zieltyp/-App aus der Freigabe; §16.40 verlangt eine feste Liste – Entscheidung beim Kern/threads).
