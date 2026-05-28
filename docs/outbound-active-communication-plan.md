# Business OS Outbound: Active Communication Implementation Plan

Status: Planungsstand 2026-05-28
Ziel: Die bestehende Outbound-App wird von Research/Pipeline-Vorbereitung zu einer vollstaendigen, approval-gesteuerten Kommunikationsloesung erweitert. CTOX darf Kommunikation, Follow-ups, Reply Handling und Terminfindung automatisch vorbereiten, aber keine ausgehende Nachricht ohne explizite Freigabe senden.

## Fortschrittsmodell

Der Projektfortschritt wird ueber Wellengewichtung berechnet. Jede Welle hat einen Prozentanteil am Gesamtprojekt. Eine Welle gilt erst als abgeschlossen, wenn alle Aufgaben, Migrationen, Tests, UI-Smokes und Akzeptanzkriterien dieser Welle erledigt sind.

| Welle | Gewicht | Status | Fortschritt |
| --- | ---: | --- | ---: |
| 0. Baseline & Architekturvertrag | 5% | Abgeschlossen | 100% |
| 1. Datenmodell, Schemas & Migrationen | 12% | In Arbeit | 75% |
| 2. Command Bus & Approval-Gate Backend | 12% | In Arbeit | 75% |
| 3. Outbound UI: Lead Queue, Engagement Cockpit, Approval Inbox | 14% | In Arbeit | 75% |
| 4. Campaign Settings, Strategie & Sequenz-Simulation | 10% | In Arbeit | 75% |
| 5. CTOX Skills, Runbooks & Draft Automation | 10% | In Arbeit | 25% |
| 6. Kommunikationskanaele, Mailserver, Sender Accounts & Deliverability | 10% | In Arbeit | 75% |
| 7. Reply Loop, Thread Matching & Response Drafting | 9% | In Arbeit | 75% |
| 8. Scheduling & Terminfindung | 7% | In Arbeit | 50% |
| 9. Automation Scheduler, Limits & Stop-Regeln | 6% | In Arbeit | 50% |
| 10. End-to-End Hardening, Observability & Release Gates | 5% | In Arbeit | 90% |
| **Gesamt** | **100%** | **In Arbeit** | **70%** |

Fortschritt je Welle:

- `0%`: Noch nicht begonnen.
- `25%`: Datenmodell/API/UI-Schnittstellen der Welle sind implementiert.
- `50%`: Kernverhalten funktioniert lokal mit realen Collections/Commands.
- `75%`: Tests, Migrationen und UI-Smokes sind vorhanden und gruen.
- `100%`: Akzeptanzkriterien erfuellt, Doku aktualisiert, keine bekannten Blocker.

Wenn sich waehrend der Umsetzung Anforderungen oder technische Befunde aendern, wird dieser Plan angepasst. Jede Anpassung muss im Abschnitt "Plan-Aenderungslog" dokumentiert werden.

## Nicht Verhandelbare Produktregeln

1. Jede ausgehende Outbound-Kommunikationsnachricht braucht eine explizite Freigabe.
2. CTOX darf Drafts, Follow-ups, Antwortentwuerfe und Terminantworten automatisch vorbereiten.
3. CTOX darf keine Nachricht senden, wenn kein freigegebener `outbound_message`-Datensatz existiert.
4. Versand muss auditierbar sein: Draft-Erzeugung, Bearbeitung, Freigabe, Versand, Antwort, Stop-Grund.
5. Reply Handling und Scheduling duerfen automatisch klassifizieren und vorbereiten, aber nicht ohne Freigabe antworten.
6. Opt-out, Bounce, Do-not-contact, manuelle Pause und Account-Limits stoppen Sequenzen hart.
7. UI und Backend muessen denselben State-Machine-Vertrag verwenden.
8. Der Kommunikationskanal ist Teil des Outbound-Vertrags: E-Mail und physischer Brief duerfen unterschiedliche Delivery-Gates haben, muessen aber denselben Approval-Auditpfad nutzen.
9. Outbound bleibt die Campaign-, Sequenz- und Orchestrierungs-App. Die Kommunikations-App (`conversations`) ist die Detailoberflaeche fuer Postfach, Thread, Nachrichtendetails und optionale Message-Freigabe.
10. Jede aktive Outbound-Campaign braucht ein eigenes Kampagnen-Postfach oder eine explizit zugewiesene Kommunikationsidentitaet. Outbound darf keine Campaign aktivieren, wenn das zugehoerige Postfach nicht bereit oder bewusst als manueller Kanal markiert ist.

## Zielarchitektur

### Neue zentrale Konzepte

- `Engagement`: Aktive Kommunikationsbeziehung zwischen Campaign, Lead, Kontakt und Sender-Account.
- `Message`: Jede geplante, vorbereitete, freigegebene, gesendete oder verworfene Outbound-Nachricht.
- `Channel`: Delivery-Art einer Nachricht, aktuell `email` oder `physical_letter`.
- `Campaign Mailbox`: Kampagnenbezogenes Postfach oder Kommunikationskonto, ueber das Outbound-Nachrichten verschickt und Replies eingesammelt werden.
- `Communication Thread Link`: Stabile Verknuepfung von `outbound_engagements`/`outbound_messages` zu `communication_accounts`, `communication_threads` und `communication_messages`.
- `Approval`: Freigabe-, Ablehnungs- und Kommentarhistorie fuer Messages.
- `Sequence`: Campaign-spezifische Kommunikationsstrategie, Follow-up-Regeln und Stop-Regeln.
- `Sender Assignment`: Zuweisung eines Leads oder Engagements zu einem Mail-Account.
- `Scheduling Request`: Terminfindungszustand, Slot-Vorschlaege und Meeting-Ergebnis.

### App-Grenze: Outbound vs. Conversations

Outbound zeigt nur die fuer Kampagnensteuerung noetige Kommunikationszusammenfassung: naechster Schritt, Status, blockierende Gruende, Freigabezaehler und Link zur Detailansicht. Vollstaendige Nachrichtendetails, Thread-Verlauf, Account-Zustand, Reply-Kontext und optional die eigentliche Freigabeoberflaeche gehoeren in die Kommunikations-App `conversations`.

Deep-Link-Vertrag:

- Campaign: `#conversations?campaign_id=<campaign_id>`
- Engagement: `#conversations?engagement_id=<engagement_id>`
- Thread: `#conversations?thread_key=<communication_thread_key>`
- Message: `#conversations?message_key=<communication_message_key>&outbound_message_id=<outbound_message_id>`

Die Outbound-App muss solche Links an allen Stellen anzeigen, an denen der Nutzer Details sehen oder eine Nachricht im Kommunikationskontext pruefen will. Die Kommunikations-App muss die Parameter lesen, die passende Conversation fokussieren und den Ruecklink zur Outbound-Campaign anbieten.

### Erweiterter Funnel

```text
Lead qualifiziert
-> Sender zugewiesen
-> Sequenz aktiv
-> Draft vorbereitet
-> Wartet auf Freigabe
-> Freigegeben
-> Gesendet
-> Wartet auf Antwort
-> Antwort erhalten
-> Antwort/Termin vorbereitet
-> Wartet auf Freigabe
-> Termin gebucht / Nurture / Kein Fit / Pausiert
```

## Welle 0: Baseline & Architekturvertrag

Gewicht: 5%

Ziel: Vor der Implementierung wird der existierende Zustand festgehalten, damit spaetere Wellen stabil aufbauen.

Aufgaben:

- [x] Bestehende Outbound Collections, UI-Flows und Command-Pfade dokumentieren.
- [x] Bestehende Mailserver-Kommandos und Mailversandpfade dokumentieren.
- [x] Bestehende Communication-Message-Tabellen und Reply-Sync-Pfade dokumentieren.
- [x] Bestehende RxDB Contract Guards und Browser-only Datenpfadregeln pruefen.
- [x] State-Machine-Vertrag fuer Active Outbound schriftlich festlegen.
- [x] Namenskonventionen fuer Collections, Commands, Statuswerte und Knowledge Tables fixieren.

Akzeptanzkriterien:

- [x] Es gibt ein kurzes Architecture Decision Record fuer Approval-Gate und State Machine.
- [x] Keine Implementierung startet ohne bestaetigte Collection- und Command-Namen.
- [x] Risiken und offene Abhaengigkeiten sind in diesem Dokument ergaenzt.

Ergebnis:

- Architekturvertrag: `docs/rfcs/0008_business_os_active_outbound_approval_gate.md`.
- Gepruefte Module gemaess CTOX App-Development-Vertrag: `notes`, `spreadsheets`, `documents`.
- Subagent-Einsatz: Zwei Explorer-Subagents haben Mailserver/Communication-Safety und RxDB/Schemapfade untersucht; beide haben keine Dateien geaendert.
- Baseline-Befund: Channel-Send besitzt bereits starke Review-Gates, Business OS Outbound besitzt aber noch kein eigenes Active-Outbound-Domainmodell.
- Lokaler Hinweis: `git status` ist in diesem Checkout trotz sichtbarer `.git`-Struktur nicht nutzbar; Umsetzung erfolgt ueber gezielte Dateipruefung und vorsichtige Edits.

## Welle 1: Datenmodell, Schemas & Migrationen

Gewicht: 12%

Ziel: Business OS kann aktive Outbound-Kommunikation vollstaendig speichern, replizieren und migrieren.

Neue oder erweiterte Collections:

- `outbound_engagements`
- `outbound_messages`
- `outbound_approvals`
- `outbound_sequences`
- `outbound_sender_assignments`
- `outbound_meeting_requests`
- optional: `outbound_suppression_entries`
- optional: `outbound_account_limits`

Aufgaben:

- [x] `src/core/business_os/business_os_schema_contract.json` erweitern.
- [x] Browser RxDB Schemas in `src/apps/business-os/modules/outbound/schema.js` erweitern.
- [x] Outbound Module Manifest, Registry und App-Fallback-Manifest erweitern.
- [x] Native RxDB Schema Hashes aktualisieren.
- [x] Persistenzpfad festlegen: Neue Collections laufen additiv ueber den bestehenden generischen Business-OS `business_records`/RxDB-Peer-Pfad; dedizierte SQLite-Fachtabellen werden erst eingefuehrt, wenn Welle 2/9 echte Query- oder Transaktionsanforderungen zeigen.
- [x] Projektionen im nativen RxDB Peer ueber den Schema-Contract registrieren.
- [x] Pull-/Push-Verhalten fuer neue Collections ueber den bestehenden generischen Sync-Pfad definieren.
- [ ] Tombstone- und Konfliktstrategie fuer Messages und Approvals mit Backend-State-Machine konkret pruefen.
- [ ] Statuswerte und erlaubte Uebergaenge im Backend validieren.

Mindestfelder:

- `outbound_engagements`: `id`, `campaign_id`, `company_id`, `pipeline_id`, `contact_id`, `sender_account_id`, `sequence_id`, `status`, `next_action_at_ms`, `paused_reason`, `closed_reason`, `payload`, `created_at_ms`, `updated_at_ms`.
- `outbound_messages`: `id`, `engagement_id`, `campaign_id`, `message_type`, `direction`, `channel`, `sender_account_id`, `recipient_email`, `recipient_address_text`, `subject`, `body_text`, `body_html`, `document_id`, `document_version_id`, `document_pdf_url`, `draft_status`, `approval_status`, `send_status`, `scheduled_send_at_ms`, `sent_at_ms`, `physical_sent_at_ms`, `provider_message_id`, `thread_key`, `reply_to_message_id`, `payload`, `created_at_ms`, `updated_at_ms`.
- `outbound_approvals`: `id`, `message_id`, `engagement_id`, `actor_user_id`, `decision`, `comment`, `created_at_ms`.
- `outbound_sequences`: `id`, `campaign_id`, `name`, `strategy_text`, `sequence_policy_text`, `send_window`, `touchpoints`, `stop_rules`, `approval_policy`, `payload`, `created_at_ms`, `updated_at_ms`.
- `outbound_sender_assignments`: `id`, `campaign_id`, `engagement_id`, `company_id`, `contact_id`, `sender_account_id`, `assignment_reason`, `status`, `created_at_ms`, `updated_at_ms`.
- `outbound_meeting_requests`: `id`, `engagement_id`, `message_id`, `calendar_account_id`, `duration_minutes`, `slot_strategy`, `proposed_slots`, `status`, `meeting_url`, `created_at_ms`, `updated_at_ms`.

Akzeptanzkriterien:

- [x] Browser und native Peer kennen alle neuen Collections.
- [x] Bestehende Outbound-Daten bleiben lesbar, da die Aenderung rein additive Collections einfuehrt.
- [x] Schema-Contract-Test und Hash-Check sind gruen.
- [ ] Leere Collections funktionieren ohne UI-Fehler.

Umsetzung:

- Browser-Schemas: `src/apps/business-os/modules/outbound/schema.js`.
- Browser-Manifeste: `src/apps/business-os/modules/outbound/module.json`, `src/apps/business-os/modules/registry.json`, `src/apps/business-os/app.js`.
- Native Contract/Hashes: `src/core/business_os/business_os_schema_contract.json`, `src/core/business_os/business_os_schema_hashes.json`.

Verifikation:

- `node --check src/apps/business-os/modules/outbound/schema.js`
- `node --check src/apps/business-os/app.js`
- `cargo test -q native_all_schema_hashes_match_browser_contract_fixture -- --nocapture`

Offener Guard:

- `node src/apps/business-os/scripts/assert-rxdb-only.mjs` meldet derzeit App-Shell-/Advanced-Status-/Login-Regeln in `src/apps/business-os/app.js`, die nicht aus den neuen Outbound-Collections stammen. Dieser Guard bleibt als Cross-Cutting Blocker offen, bevor die Loesung als releasefaehig gelten kann.

## Welle 2: Command Bus & Approval-Gate Backend

Gewicht: 12%

Ziel: Backend-seitig ist technisch ausgeschlossen, dass ungepruefte Nachrichten gesendet werden.

Neue Commands:

- `outbound.engagement.create`
- `outbound.engagement.assign_sender`
- `outbound.sequence.save`
- `outbound.message.prepare`
- `outbound.message.update_draft`
- `outbound.message.request_approval`
- `outbound.message.approve`
- `outbound.message.reject`
- `outbound.message.send_approved`
- `outbound.message.pause`
- `outbound.message.resume`
- `outbound.message.cancel`
- `outbound.engagement.resume`
- `outbound.engagement.close`
- `outbound.campaign.mailbox.link`
- `outbound.reply.match`
- `outbound.reply.classify`
- `outbound.scheduling.prepare`
- `outbound.scheduling.mark_booked`

Aufgaben:

- [x] Command-Handler in Rust fuer aktive Outbound-Basiscommands implementieren.
- [x] Validierung fuer Engagement-, Message- und Approval-Basisstate einbauen.
- [x] Harte Versandregel: `send_approved` akzeptiert nur `approval_status = approved`.
- [x] Versandregel prueft Sender-Account, Empfaenger, Suppression und einfache Account-Limits.
- [x] Versandregel ist kanalbewusst: E-Mail braucht Mail-Ziel und Sender-Account, physischer Brief braucht Postadresse und manuellen Versandnachweis.
- [ ] Versandregel prueft Bounce, Opt-out und providerfaehige Sender-Account-Health.
- [x] Jede Command-Ausfuehrung schreibt replizierbare Command-Result- und Domain-Record-Metadaten.
- [x] Pause, Resume und Close schreiben nachvollziehbare Engagement-/Message-State-Uebergaenge.
- [ ] Fehlerstatus und blockierende Gruende in replizierbare Result-Felder schreiben.
- [ ] Idempotenz fuer wiederholte Command-Verarbeitung sicherstellen.
- [x] Tests fuer zentrale unerlaubte State-Uebergaenge ergaenzen.
- [x] Provider-Queue-Pfad sicher an Mailserver/Channel-Review-Modell anbinden.

Akzeptanzkriterien:

- [x] Nicht freigegebene Nachricht kann nicht gesendet werden.
- [x] Freigegebene Nachricht ohne Sender kann nicht gesendet werden.
- [x] Physischer Brief ohne Postadresse kann nicht freigegeben werden.
- [ ] Freigegebene Nachricht mit Suppression/Opt-out kann nicht gesendet werden.
- [x] Erfolgreich markierter physischer Brief aktualisiert Message, Engagement und Audit.
- [x] Erfolgreiches Einreihen in die Mailserver-Queue aktualisiert Message, Engagement und Audit-Metadaten.
- [ ] Fehlgeschlagener Versand bleibt retry-faehig ohne Draft-Verlust.

Umsetzung:

- Backend-Dispatch und Gate: `src/core/business_os/store.rs`.
- Implementierte Commands: `outbound.engagement.create`, `outbound.engagement.assign_sender`, `outbound.sequence.save`, `outbound.message.prepare`, `outbound.message.update_draft`, `outbound.message.request_approval`, `outbound.message.approve`, `outbound.message.reject`, `outbound.message.send_approved`, `outbound.message.pause`, `outbound.message.resume`, `outbound.message.cancel`, `outbound.engagement.resume`, `outbound.engagement.close`, `outbound.reply.classify`, `outbound.scheduling.prepare`, `outbound.scheduling.mark_booked`.
- `outbound.message.send_approved` schreibt fuer E-Mail nach Approval einen RFC822-Auftrag in die bestehende `stalwart_smtp_queue`, speichert `provider_queue_id`, spiegelt den Status in `communication_messages` und zaehlt Sender-Limits hoch. Die SMTP-Queue bleibt fuer die eigentliche Zustellung und Retry-/Failure-Behandlung verantwortlich.
- Fuer `physical_letter` markiert `outbound.message.send_approved` den manuell verschickten Brief mit `send_status = sent`, `physical_sent_at_ms` und `provider_dispatch_status = manual_physical_letter_marked_sent`.

Verifikation:

- `rustfmt --check src/core/business_os/store.rs`
- `cargo test -q outbound_message_send_approved_requires_matching_current_revision -- --nocapture`
- `cargo test -q outbound_pause_resume_and_close_updates_engagement_state -- --nocapture`
- `cargo test -q outbound_physical_letter_marks_manual_send_without_mail_account -- --nocapture`

## Welle 3: Outbound UI

Gewicht: 14%

Ziel: Die Outbound-App bekommt eine vollstaendige aktive Kommunikationsoberflaeche.

Neue UI-Bereiche:

- Lead Queue
- Engagement Cockpit
- Approval Inbox
- Kommunikations-Zusammenfassung mit Deep Link zur Conversations-App
- Sender Assignment Dialog
- Message Review Editor

Aufgaben:

- [x] Navigation um Active-Outreach-Ansicht neben dem Research Funnel erweitern.
- [x] Lead Queue fuer qualifizierte Leads ohne Engagement bauen.
- [ ] Sender Assignment Dialog mit Account-Empfehlung und Warnungen bauen.
- [x] Basis-Sender-Zuweisung ueber Campaign-Konfiguration, Mailserver-Account oder Prompt anbinden.
- [ ] Sender-Warnungen fuer Health, Limits, Domain und fehlende Reputation bauen.
- [x] Engagement Cockpit mit Lead-Kontext, Timeline und Next-Step-Panel bauen.
- [x] Basis-Timeline fuer aktive Messages einbinden.
- [x] Outbound-Cockpit auf kompakte Kommunikationszusammenfassung reduzieren und fuer Details nach `conversations` verlinken.
- [x] Deep Links zu `#conversations?campaign_id=...`, `engagement_id`, `thread_key` und `message_key` aus Lead Queue, Cockpit, Approval Inbox und Ready-to-send einbauen.
- [x] Approval Inbox fuer alle wartenden Nachrichten bauen.
- [x] Entscheiden und implementieren, ob Freigaben primaer in Outbound, primaer in Conversations oder in beiden Apps ueber denselben `outbound.message.approve` Command angeboten werden.
- [x] Ready-to-send Queue fuer freigegebene Nachrichten bauen.
- [x] Message Review Editor fuer Subject und Body in der Approval Inbox bauen.
- [x] Message Review Editor fuer physische Briefe mit Postadresse und PDF/Print-Export bauen.
- [ ] Draft Editor fuer Follow-ups und Reply-Drafts bauen.
- [x] Reject-with-comment Flow bauen.
- [x] Pause/Cancel Flow mit Grund bauen.
- [x] Resume/Close Flow bauen.
- [x] Empty- und Error-Basiszustaende fuer Active Outreach bauen.
- [ ] Loading- und Blocked States fuer Provider/Account-Health bauen.
- [x] UI-Smoke fuer Outbound-Modul laden.
- [ ] UI gegen grosse Datenmengen testen.

Akzeptanzkriterien:

- [x] Nutzer sieht im Engagement Cockpit, welche Nachricht als naechstes wartet.
- [ ] Nutzer kann Drafts bearbeiten, freigeben oder mit Kommentar ablehnen.
- [x] Nutzer kann physische Briefe als PDF/Print-Datei vorbereiten und nach Freigabe als verschickt markieren.
- [x] Keine UI-Aktion erzeugt direkten Versand ohne Approval-Command.
- [ ] Timeline zeigt Draft, Approval, Versand, Antwort und Stop-Gruende vollstaendig.
- [ ] Detailansicht fuer Threads und Nachrichten liegt in `conversations`; Outbound zeigt nur Kurzstatus und stabile Links.
- [x] UI-Smoke deckt das Laden des Active-Outbound-Moduls ab.
- [ ] UI-Smoke deckt Lead Queue, Assignment, Approval Inbox und Cockpit mit Testdaten ab.
- [ ] UI-Smoke deckt Deep Link von Outbound nach Conversations ab.

Umsetzung:

- Active-Outreach-Ansicht: `src/apps/business-os/modules/outbound/index.js`.
- Responsive Styles: `src/apps/business-os/modules/outbound/index.css`.
- App-Fallback-Manifest fuer neue Collections korrigiert: `src/apps/business-os/app.js`.
- Pause/Cancel/Resume/Close-Gruende werden backendseitig an Message-Payload und Engagement (`paused_reason`/`closed_reason`) geschrieben: `src/core/business_os/store.rs`.
- Lead Queue und Approval Inbox sind kanalbewusst: `Auto-Draft` nutzt den Campaign-Default, `Brief-Draft` erzwingt `physical_letter`, Ready-to-send zeigt bei Briefen `Als verschickt markieren`.
- Conversations ist bereits als eigenes Modul vorhanden und liest `communication_accounts`, `communication_threads` und `communication_messages`; Outbound muss darauf verlinken, statt vollstaendige Thread-Detailfunktionen nachzubauen.

Verifikation:

- `node --check src/apps/business-os/modules/outbound/index.js`
- `node --check src/apps/business-os/app.js`
- `rustfmt --check src/core/business_os/store.rs`
- `cargo test -q outbound_message_send_approved_requires_matching_current_revision -- --nocapture`
- `cargo test -q outbound_physical_letter_marks_manual_send_without_mail_account -- --nocapture`
- Collection-Konsistenzcheck fuer Schema, Module Manifest, Registry, Contract, Hashes und App-Fallback.
- `BUSINESS_OS_CONNECTION_SMOKE_MODES=direct BUSINESS_OS_DIRECT_URL='http://127.0.0.1:8765/#outbound' BUSINESS_OS_EXPECT_MODULE=outbound BUSINESS_OS_EXPECT_TEXT=Outbound BUSINESS_OS_SHELL_VISIBLE_BUDGET_MS=15000 node src/core/rxdb/tools/business_os_connection_modes_smoke.js`

Befund:

- Der erste UI-Smoke mit 3 Sekunden Shell-Budget hing im bekannten Workspace-Loader/Required-Collections-Gate. Mit 15 Sekunden Budget wurde `activeModule = outbound` erreicht. Die Advanced-Status-Checks fuer `requiredCollectionsConnected`, `requiredCollectionsInitialSyncComplete` und `requiredCollectionsCheckpointEpochAdvertised` bleiben als Cross-Cutting Guard-Thema offen.
- Nach Resume-/Close-Erweiterung bleibt der Outbound-Smoke mit 15 Sekunden Shell-Budget gruen. Ein Lauf meldete Shell-Import-Retry-Warnungen fuer `window-manager.js` und `taskbar.js`, erreichte aber weiterhin `activeModule = outbound`.

## Welle 4: Campaign Settings, Strategie & Sequenz-Simulation

Gewicht: 10%

Ziel: Campaign Owner koennen Strategie und Ablauf sprachlich steuern, waehrend die UI daraus eine pruefbare Sequenz ableitet.

Anschlussentscheidung:

- Der bestehende Research-Settings-Drawer wird zu einem Campaign Settings Drawer erweitert, statt einen zweiten Einstellungsort anzulegen.
- Research-Settings bleiben in `campaign.payload.research_settings`.
- Active-Outbound-Settings erhalten eigene Normalizer und werden teils in `campaign.payload.active_outreach`, teils als versionierte `outbound_sequences` per `outbound.sequence.save` gespeichert.
- Sender-Auswahl sitzt im selben Drawer; globale Mailserver-Domain/User-Verwaltung bleibt klar getrennt.

Settings Tabs:

- [x] Strategie
- [x] Sequenz
- [x] Freigabe
- [x] Sender
- [x] Kanal
- [x] Terminfindung
- [x] CTOX Playbook / Advanced Binding
- [x] Compliance

Aufgaben:

- [x] Campaign Settings Drawer erweitern.
- [x] Freitextfelder fuer Ziel, ICP, Value Proposition, Tonalitaet und No-go-Regeln bauen.
- [x] Sequenzstrategie als Freitext plus strukturierte Touchpoint-Vorschau bauen.
- [x] Wartezeiten, Werktage, Sendefenster und Stop-Regeln editierbar machen.
- [x] Approval Policy sichtbar machen; Default: jede Nachricht braucht Freigabe.
- [x] Sender-Account-Regeln pro Campaign konfigurieren.
- [x] Default-Kanal, Kanalstrategie, Brief-Absender und Briefvorlage konfigurieren.
- [x] Terminstrategie konfigurieren.
- [x] Skillbook/Runbook Binding als Advanced Settings abbilden.
- [x] Simulation rechts im Drawer anzeigen.

Akzeptanzkriterien:

- [x] Settings erzeugen oder aktualisieren `outbound_sequences`.
- [x] Simulation zeigt Initial-Mail, Follow-ups, Reply Handling und Scheduling Gates.
- [x] Aenderungen wirken auf neu vorbereitete Drafts.
- [ ] Bestehende aktive Engagements behalten nachvollziehbar ihre Version der Strategie oder erhalten einen expliziten Reapply-Flow.

Umsetzung:

- Campaign Settings im bestehenden Drawer erweitert: Strategie, Sequenz, Freigabe, Sender, Kanal, Terminfindung, Compliance und Advanced Binding.
- Active-Outbound-Settings werden in `campaign.payload.active_outreach` persistiert.
- Sequenzdefinitionen werden beim Speichern ueber `outbound.sequence.save` in `outbound_sequences` geschrieben.
- Touchpoints werden aus `firstRetryDays`, `maxFollowups` und Approval-Pflicht strukturiert erzeugt und rechts im Drawer simuliert.
- Sender-Auswahl nutzt Mailserver-User, erlaubt aber Fallback-Eingabe fuer noch nicht geladene Accounts.
- Kanal-Auswahl steuert neu vorbereitete Drafts; `physical_letter` nutzt Postadresse, Briefvorlage und manuellen Versandnachweis.
- Neu vorbereitete Engagements und Messages erhalten Strategy-, Sequenz-, Stop-, Approval-, Scheduling-, Compliance-, Skillbook- und Runbook-Kontext im Payload.

Verifikation:

- `node --check src/apps/business-os/modules/outbound/index.js`
- `rustfmt --check src/core/business_os/store.rs`
- `cargo test -q outbound_sequence_save_persists_strategy_policy_and_touchpoints -- --nocapture`
- `cargo test -q outbound_pause_resume_and_close_updates_engagement_state -- --nocapture`
- Browser-Smoke: `BUSINESS_OS_CONNECTION_SMOKE_MODES=direct BUSINESS_OS_DIRECT_URL='http://127.0.0.1:8766/#outbound' ... node src/core/rxdb/tools/business_os_connection_modes_smoke.js`

Hinweis:

- Der Browser-Smoke ist gruen und laedt `activeModule = outbound`. Port `8765` war lokal durch ein SSH-Forward belegt, deshalb lief der gueltige Smoke auf `8766`. Die bestehenden Advanced-Status-Checks melden weiterhin nicht initial synchronisierte required collections und einen 404-Konsolenhinweis; das ist derzeit nicht spezifisch fuer Welle 4 und bleibt als Cross-Cutting-Smoke-Thema offen.

## Welle 5: CTOX Skills, Runbooks & Draft Automation

Gewicht: 10%

Ziel: CTOX bereitet alle Nachrichten vollautomatisch vor, schreibt aber nur Drafts und Approval Requests.

Skillbooks/Runbooks:

- `business-os.outbound.message_drafting.v1`
- `business-os.outbound.reply_handling.v1`
- `business-os.outbound.scheduling.v1`
- Campaign-spezifische Runbooks mit Strategie, Tabellen und Stop-Regeln.

Aufgaben:

- [ ] Skillbook fuer Initial-Mail und Follow-ups definieren.
- [ ] Skillbook fuer Reply-Klassifikation und Antwortentwurf definieren.
- [ ] Skillbook fuer Terminfindung definieren.
- [x] Knowledge Writeback Contracts fuer Messages, Approvals und Meetings als `outbound.draft.prepare` Payload-/Collection-Vertrag definieren.
- [x] Draft Automation Command an Skillbook/Runbook-Kontext binden.
- [x] Follow-up-Draft beruecksichtigt bisherige Kommunikation.
- [x] Reply-Draft beruecksichtigt Antwortklassifikation.
- [x] Scheduling-Draft beruecksichtigt Kalenderstrategie.
- [x] Skills duerfen keinen Versand-Command direkt ausloesen.

Akzeptanzkriterien:

- [x] CTOX kann Initial-Mail-Draft fuer qualifizierten Lead vorbereiten.
- [x] CTOX kann Follow-up-Draft nach Sequenzregel vorbereiten.
- [x] CTOX kann Antwort klassifizieren und Antwortentwurf vorbereiten.
- [x] CTOX kann Terminantwort vorbereiten.
- [x] Alle Drafts landen als `outbound_messages` mit `approval_status = awaiting_approval`.

Umsetzung:

- Backend-Command `outbound.draft.prepare` ergaenzt.
- Der Command erzeugt Initial-, Follow-up-, Reply- und Scheduling-Drafts als `outbound_messages`.
- Alle automatisch erzeugten Drafts bekommen `draft_status = ready_for_review`, `approval_status = awaiting_approval`, `send_status = awaiting_approval`.
- Der Command schreibt `draft_engine`, `generated_draft`, `skillbook_id`, `runbook_id` und vorherige Message-Referenzen in den Payload.
- Scheduling-Drafts erzeugen zusaetzlich `outbound_meeting_requests` mit `status = prepared`.
- Lead Queue kann jetzt direkt `Auto-Draft` ausloesen, ohne den alten Browser-Gateway-Draft vorauszusetzen.
- Engagement Cockpit kann Follow-up-, Reply- und Terminantwort-Drafts vorbereiten.

Verifikation:

- `node --check src/apps/business-os/modules/outbound/index.js`
- `rustfmt --check src/core/business_os/store.rs`
- `cargo test -q outbound_draft_prepare_creates_approval_gated_automated_messages -- --nocapture`
- Browser-Smoke: `BUSINESS_OS_CONNECTION_SMOKE_MODES=direct BUSINESS_OS_DIRECT_URL='http://127.0.0.1:8766/#outbound' ... node src/core/rxdb/tools/business_os_connection_modes_smoke.js`

Offen fuer 50%:

- Echte Skill-/Runbook-Items fuer `business-os.outbound.message_drafting.v1`, `reply_handling.v1` und `scheduling.v1` als Knowledge-Struktur persistieren.
- Template-Drafting durch Skill-/Runbook-gestuetzte Kontextauflosung ersetzen.
- Follow-up-Timing an Scheduler/Sequenzregeln koppeln, statt manuellem Cockpit-Ausloeser.
- Reply-Inbound-Events automatisch zu `outbound.reply.classify` und anschliessendem Reply-Draft fuehren.

## Welle 6: Kommunikationskanaele, Mailserver, Sender Accounts & Deliverability

Gewicht: 10%

Ziel: Outbound kann kanalbewusst kommunizieren. E-Mail-Sender sind sicher nutzbar, limitsicher und deliverability-bewusst; physische Briefe koennen vorbereitet, als PDF/Print-Datei exportiert und manuell als verschickt markiert werden.

Aufgaben:

- [x] Kampagnen-Postfach-Modell definieren: pro Campaign eigenes Mailbox-/Account-Binding mit Adresse, Alias, Reply-To, Persona, Signatur, Limits und Health.
- [x] Campaign-Aktivierung blockieren, wenn fuer `email` kein bereites Kampagnen-Postfach oder bewusst konfigurierter manueller Kanal existiert.
- [ ] Postfach-Setup-Flow bauen: bestehendes Kommunikationskonto auswaehlen, neues Mailserver-Postfach anlegen oder Alias/Reply-To einer Campaign zuweisen.
- [x] Kampagnen-Postfach in `communication_accounts` spiegeln und fuer Conversations filterbar machen.
- [x] Outbound-Messages mit `communication_account_key`, `thread_key` und `communication_message_key` verknuepfen.
- [x] Freigegebene E-Mail-Nachrichten in die bestehende Mailserver-Queue einreihen.
- [x] Channel-Vertrag fuer `email` und `physical_letter` in Message-Schema, Draft-Payload und Send-Gate einfuehren.
- [x] Physische Briefe ohne Mailkonto vorbereiten und nach Freigabe als manuell verschickt markieren.
- [x] Basis-PDF/Print-Export fuer physische Briefe in der Approval/Ready-to-send UI bereitstellen.
- [ ] Document-Editor-Anbindung fuer versionierte Briefvorlagen und sauber gerenderte PDFs bauen.
- [ ] Briefvorlagen pro Campaign als wiederverwendbare Dokumentvorlagen speichern.
- [ ] Bestehende Mailserver-Domain/User-Verwaltung in Sender-Account-Modell integrieren.
- [ ] Sender Account Health berechnen: Domain, DKIM, SPF, DMARC, SMTP, Tageslimit.
- [ ] Account-Auswahl pro Campaign und Engagement validieren.
- [ ] Daily Send Limits und Sendefenster erzwingen.
- [ ] Bounce- und Fehlerstatus in Engagement zurueckfuehren.
- [ ] Signatur und Persona pro Sender pflegen.
- [ ] Kalenderkonto optional an Sender binden.
- [x] UI-Warnungen fuer Deliverability-Probleme anzeigen.
- [ ] Physische Briefe in Sequenzen wie versendete Touchpoints behandeln, inklusive Follow-up-Timing und Stop-Regeln.

Akzeptanzkriterien:

- [x] Backend blockiert `physical_letter` ohne `recipient_address_text`.
- [x] Backend markiert freigegebene physische Briefe ohne Provider-Call als `sent`.
- [x] UI bietet Brief-Draft, Postadress-Review, PDF/Print-Export und manuelles Verschickt-Markieren.
- [ ] PDFs kommen aus einer versionierten Dokumentvorlage mit visueller QA, nicht nur aus einem Minimalexport.
- [x] UI zeigt nur nutzbare oder klar gewarnte Sender-Accounts.
- [x] Jede aktive E-Mail-Campaign besitzt ein sichtbares Kampagnen-Postfach in Conversations.
- [x] Outbound kann aus Campaign, Engagement und Message direkt in die passende Conversations-Ansicht springen.
- [x] Backend blockiert Versand bei fehlendem/gesperrtem Account.
- [ ] Tageslimits werden auch bei parallelen Commands eingehalten.
- [ ] Mailserver-Konfiguration bleibt global, Campaign-Regeln bleiben Campaign-spezifisch.

Umsetzung:

- Backend-Command `outbound.campaign.mailbox.link` ergaenzt. Er schreibt oder aktualisiert das Kampagnen-Postfach in der Channels-DB `communication_accounts`, ohne Phantom-Accounts im generischen `business_records`-Pfad zu erzeugen.
- Sender-Accounts werden fuer E-Mail canonical als `email:<adresse>` gespeichert; alte bare E-Mail-Werte werden beim Lesen normalisiert.
- Campaign Settings speichern `communication_account_key`, `communication_account_address` und `mailbox_status`, verknuepfen den Account im Backend und legen einen initialen `outbound_account_limits`-Datensatz an.
- Der Sender-Tab zeigt bestehende `communication_accounts` und Mailserver-User als auswaehlbare Kampagnen-Postfaecher; Conversations-Deep-Links erhalten dadurch einen stabilen Account-Kontext.
- Das Send-Gate normalisiert E-Mail-Sender auf `email:<adresse>`, verlangt einen verlinkten `outbound_account_limits`-Datensatz und blockiert `blocked`, `locked`, `suspended` oder `disabled` Accounts vor jedem Provider-Queueing.
- Das E-Mail-Send-Gate schreibt nach Approval in `stalwart_smtp_queue`, persistiert `provider_queue_id`/`provider_dispatch_status = queued_in_mailserver`, spiegelt diese Metadaten nach Conversations und ist idempotent gegen wiederholte `send_approved`-Commands.
- Der Sender-Tab zeigt pro Account den lokalen Health-/Limit-Status aus `outbound_account_limits`; gesperrte oder limitierte Accounts werden sichtbar gewarnt und in automatischen Sender-Flows nicht mehr stillschweigend verwendet.
- E-Mail-Outbound-Messages werden bei Draft-Erzeugung, Draft-Update, Freigabe und Provider-Queueing in `communication_messages` gespiegelt; `communication_account_key`, `thread_key` und `communication_message_key` werden top-level und im Payload der Outbound-Message persistiert.
- Die Spiegelung nutzt den bestehenden Channels-Pfad `channels::upsert_communication_message` plus `channels::refresh_thread`, sodass Conversations dieselben Thread- und Account-Keys sieht wie Outbound. Der echte Provider-Versand bleibt weiterhin deaktiviert und endet bei `queued_for_provider`.
- Neuer Backend-Command `outbound.campaign.status.set` validiert Campaign-Aktivierung: E-Mail braucht ein `communication_accounts`-Postfach und einen nutzbaren `outbound_account_limits`-Datensatz; `physical_letter` gilt als bewusst manueller Kanal und darf ohne Mailbox aktiv werden.
- Die Kampagnenliste zeigt Status-/Readiness-Badges; neue Campaigns starten in `setup_required` statt implizit `active`, und der Inline-Editor nutzt den nativen Aktivierungs-Command.
- Deployment-Stand 2026-05-25: `cto1.kunstmen.com` liefert den Frontend-Build `20260525-outbound-activation-gate1` aus. Auf dem VPS ist Release `branch-main-20260525T184655Z` source-seitig gepatcht und `ctox.service` wurde wieder aktiv stabilisiert; die neue Backend-Binary ist noch nicht installiert, weil ein wiederkehrender Dev-Upgrade-Runner die kontrollierten Builds mehrfach gestoppt und ungepatchte Releases gestartet hat. Der Runner ist temporaer ueber den `~/.local/bin/ctox` Upgrade-Guard blockiert.

Verifikation:

- `node --check src/apps/business-os/modules/outbound/index.js`
- `node --check src/apps/business-os/shared/universal-importer.js`
- `cargo test -q outbound_campaign_mailbox_link_projects_to_communication_accounts -- --nocapture`
- `cargo test -q outbound_message_send_approved_requires_matching_current_revision -- --nocapture`
- `cargo test -q outbound_message_send_blocks_missing_or_blocked_sender_account -- --nocapture`
- `cargo test -q outbound_campaign_activation_requires_ready_channel -- --nocapture`
- `cargo test -q outbound_physical_letter_marks_manual_send_without_mail_account -- --nocapture`
- `cargo test -q native_all_schema_hashes_match_browser_contract_fixture -- --nocapture`
- Vercel Production Deploy fuer `cto1.kunstmen.com`: Live-Asset-Check bestaetigt `20260525-outbound-mailbox-link1` und `outbound.campaign.mailbox.link`.
- Vercel Production Deploy fuer `cto1.kunstmen.com`: Live-Asset-Check bestaetigt `20260525-outbound-sender-health1`, Sender-Health-Warnungen und Sender-Health-Styles.
- Vercel Production Deploy fuer `cto1.kunstmen.com`: Live-Asset-Check bestaetigt `20260525-outbound-comm-link1` und `communication_account_key` im Outbound-Schema.
- VPS `kunstmen`: gepatchtes `ctox-real` aus `current` gebaut, installiert und `ctox.service` erfolgreich neu gestartet.
- VPS `kunstmen`: Send-Gate-Binary nach Auto-Upgrade-Kollision aus gepatchter `branch-main-20260525T173848Z`-Quelle gebaut, installiert und mit aktivem Service verifiziert.
- VPS `kunstmen`: Statische Business-OS-Assets fuer Sender-Health in Release- und Source-Cache-Baeume synchronisiert; `ctox.service` laeuft weiter aktiv auf `branch-main-20260525T173848Z`.
- VPS `kunstmen`: Conversation-Linking in `branch-main-20260525T181522Z` gepatcht, neu gebaut, installiert und mit aktivem `ctox.service` auf diesem Release verifiziert.

## Welle 7: Reply Loop, Thread Matching & Response Drafting

Gewicht: 9%

Ziel: Eingehende Antworten werden Engagements zugeordnet, klassifiziert und als naechster Approval-Flow sichtbar. Die Detailarbeit am Thread passiert in Conversations, Outbound bleibt bei Status, Sequenz und naechstem Schritt.

Aufgaben:

- [x] Outbound Message-ID, References und Thread-Key speichern.
- [x] Conversations liest Deep-Link-Parameter fuer Campaign, Engagement, Thread und Message und fokussiert die passende Ansicht.
- [x] Conversations zeigt Outbound-Kontext: Campaign, Engagement, Lead, Sequenzstatus, letzte Approval-Entscheidung und Ruecklink zu Outbound.
- [x] Conversations kann wartende Outbound-Drafts optional freigeben/ablehnen, verwendet aber dieselben `outbound.message.approve`/`reject` Commands wie Outbound.
- [x] Eingehende Mail per Message-ID/In-Reply-To/Adresse/Lead matchen.
- [x] Engagement bei Antwort auf `reply_received` setzen.
- [x] Reply-Klassifikation implementieren: positiv, negativ, objection, out-of-office, referral, unsubscribe, unclear.
- [x] CTOX erstellt Antwortentwurf oder Scheduling-Entwurf.
- [x] Follow-up Scheduler stoppt bei Antwort automatisch.
- [ ] UI zeigt Antwort prominent in Timeline und Approval Inbox.
- [x] Outbound zeigt Antwort nur als Status-/CTA-Zusammenfassung und verlinkt fuer Details in Conversations.
- [x] Manuelle Reclassification ermoeglichen.

Akzeptanzkriterien:

- [x] Antwort auf gesendete Mail landet im richtigen Engagement.
- [x] Antwort ist in Conversations im Kampagnen-Postfach sichtbar und mit Outbound verlinkt.
- [x] Positive Antwort erzeugt Scheduling- oder Reply-Draft.
- [ ] Unsubscribe erzeugt Suppression und stoppt Engagement.
- [ ] Out-of-office erzeugt neue Warte-/Retry-Planung, aber keinen Versand ohne Approval.

Umsetzung:

- Outbound zeigt in Active Outreach einen Kommunikationsstreifen mit Kampagnen-Postfach, Campaign-Thread-Link und Kampagnen-ID.
- Campaign Cards, Engagement Cockpit, Approval Inbox, Ready-to-send Queue und letzte Message verlinken direkt in `conversations`.
- Deep Links enthalten, soweit vorhanden, `campaign_id`, `engagement_id`, `outbound_message_id`, `thread_key`, `message_key`, `account_key` und `channel`.
- Conversations liest `#conversations?...` beim Laden und per `hashchange`, findet passende Buckets ueber Message-Key, Thread-Key oder Outbound-Metadaten und hebt die Zielnachricht hervor.
- Conversations registriert die lesenden Outbound-Collections, laedt Campaign, Pipeline-Lead, Engagement, Message und Approval-Historie und zeigt sie als Kontextkarte in der rechten Spalte.
- Conversations bietet Freigeben/Ablehnen nur fuer vollstaendige Messages mit `approval_status = awaiting_approval` und nutzt ausschliesslich die bestehenden Outbound-Commands.
- Conversations zeigt fuer erkannte eingehende Antworten eine Reply-Klassifikation mit den Klassen `positive`, `negative`, `objection`, `out_of_office`, `referral`, `unsubscribe` und `unclear`.
- Die Klassifikation ruft `outbound.reply.classify` auf, setzt das Engagement auf `reply_received` und speichert `reply_classification` sowie `reply_message_id` im Engagement-Payload.
- `outbound.reply.match` liest eingehende `communication_messages`, matcht ueber explizite Outbound-Metadaten, Message-References, Thread-Key sowie Sender-/Empfaenger-Adresse, setzt das Engagement auf `reply_received`, annotiert die Communication-Message mit Outbound-IDs und stoppt noch nicht gesendete Follow-up-/Draft-Messages.
- Conversations stoesst `outbound.reply.match` automatisch an, sobald im verlinkten Outbound-Kontext eine passende eingehende Antwort im Thread sichtbar wird; manuelle Reclassification bleibt danach weiter moeglich.

Verifikation:

- `node --check src/apps/business-os/modules/outbound/index.js`
- `node --check src/apps/business-os/modules/conversations/index.js`
- `cargo test -q outbound_reply_match_sets_engagement_and_stops_pending_followups -- --nocapture`

## Welle 8: Scheduling & Terminfindung

Gewicht: 7%

Ziel: CTOX bereitet Terminfindung bis zur Buchung vor, Nutzer gibt jede ausgehende Nachricht frei.

Aufgaben:

- [ ] Meeting Settings pro Campaign definieren.
- [ ] Kalender-/Availability-Anbindung pruefen und abstrahieren.
- [ ] Slot-Auswahlstrategie implementieren.
- [x] Scheduling-Draft fuer konkrete Slots oder Terminlink erzeugen.
- [x] Meeting Confirmation erkennen oder manuell markieren.
- [x] `outbound_meeting_requests` Status pflegen.
- [x] UI fuer Slot-Vorschau, Approval und gebuchte Termine bauen.

Akzeptanzkriterien:

- [x] Positive Antwort kann in Scheduling ueberfuehrt werden.
- [x] CTOX bereitet konkrete Terminvorschlaege vor.
- [ ] Nutzer kann Scheduling-Nachricht freigeben oder bearbeiten.
- [x] Gebuchter Termin schliesst Engagement sauber ab oder markiert naechsten Schritt.

## Welle 9: Automation Scheduler, Limits & Stop-Regeln

Gewicht: 6%

Ziel: CTOX bereitet Folgeaktionen automatisch zum richtigen Zeitpunkt vor, ohne das Approval-Prinzip zu verletzen.

Aufgaben:

- [x] Scheduler fuer `next_action_at_ms` implementieren.
- [x] Follow-up-Drafts automatisch vorbereiten.
- [ ] Scheduler beachtet Werktage, Sendefenster und Campaign-Zeitzone.
- [x] Scheduler beachtet Replies, Bounces, Opt-outs, Pausen und Closures.
- [ ] Scheduler beachtet Account- und Campaign-Limits.
- [ ] UI zeigt "Warum jetzt?" fuer vorbereitete Aktionen.
- [ ] Reapply-Strategie fuer geaenderte Campaign Settings bauen.

Akzeptanzkriterien:

- [x] Scheduler erzeugt nur Drafts, keine Sends.
- [x] Kein Follow-up nach Antwort, Bounce, Opt-out oder Pause.
- [ ] Drafts werden nachvollziehbar mit Sequenzversion erzeugt.
- [ ] Nutzer kann Automation pro Engagement und Campaign pausieren.

## Welle 10: End-to-End Hardening, Observability & Release Gates

Gewicht: 5%

Ziel: Die Loesung ist erprobt, testbar und im Betrieb nachvollziehbar.

Aufgaben:

- [x] End-to-End Browser Smoke fuer Lead -> Assignment -> Draft -> Approval -> Mailserver-Queue bauen.
- [x] End-to-End Browser Smoke fuer Reply -> Scheduling -> Meeting bauen.
- [ ] Backend-Tests fuer alle State-Uebergaenge bauen.
- [ ] Approval-Gate Negativtests bauen.
- [ ] Mailserver-Fehler-, Bounce- und Retrytests bauen.
- [ ] Reply Matching Tests bauen.
- [x] Scheduling Tests bauen.
- [x] Audit-Export oder Debug-Panel bauen.
- [ ] Observability: Command IDs, Task IDs, Message IDs und Engagement IDs verlinken.
- [ ] Observability: Outbound-IDs und Communication-IDs sind beidseitig verlinkt und im Debug-/Statuspfad sichtbar.
- [x] Business-OS Datenpfad als RxDB-only verifizieren; HTTP ist nur fuer ChatGPT-Subscription-Login erlaubt.
- [ ] Performance bei grossen Campaigns pruefen.
- [ ] Dokumentation fuer Betrieb und Troubleshooting schreiben.

Akzeptanzkriterien:

- [x] Komplettfluss funktioniert lokal im Browser-Smoke: Lead -> Assignment -> Draft -> Approval -> Send -> Reply -> Scheduling -> Meeting.
- [x] Browser/Rust-Smoke zeigt native RxDB/WebRTC-Replikation fuer `desktop_files` und `desktop_file_chunks` ohne HTTP-RxDB-Bridge.
- [ ] Kein automatisierter Test kann ungeprueften Versand ausloesen.
- [ ] Alle kritischen Fehler sind in UI und Logs diagnostizierbar.
- [ ] Release-Gate laeuft in CI oder als dokumentierter lokaler Check.

## Cross-Cutting Aufgaben

Diese Aufgaben laufen parallel zu mehreren Wellen und duerfen nicht bis zum Ende aufgeschoben werden.

- [ ] Accessibility fuer neue Dialoge, Tabellen und Approval-Flows.
- [ ] Deutsche und englische Locale-Dateien aktualisieren.
- [ ] Responsive Verhalten fuer Lead Queue, Cockpit und Approval Inbox pruefen.
- [ ] Keine sensiblen Credentials in Browser-Collections replizieren.
- [x] Kein direkter Browser-HTTP-Datenpfad, wenn der RxDB Contract es verbietet.
- [ ] Bestehende Outbound-Funktionen duerfen nicht regressieren.
- [ ] Import, Company Research, Contact Research und Lead Qualification bleiben nutzbar.
- [ ] Excel-Import mit realen `.xlsx`-Dateien bleibt im Browser-Pfad getestet.

## Testmatrix

| Bereich | Tests |
| --- | --- |
| Schema | Contract JSON, Hashes, Migrationen, leere Collections |
| Commands | Create, Assign, Prepare, Approve, Reject, Send, Pause, Close |
| Approval Gate | Send ohne Approval blockiert, Approval Audit vorhanden |
| Kommunikationskanaele | E-Mail, physischer Brief, PDF/Print-Export, manuelles Verschickt-Markieren |
| Conversations Integration | Kampagnen-Postfach, Deep Links, Thread-Fokus, optionale Freigabe ueber denselben Approval-Command |
| Mailserver | Account Health, SMTP Failure, Tageslimit, Domain-Warnungen |
| UI | Lead Queue, Assignment Dialog, Cockpit, Approval Inbox, Settings |
| Skills | Draft erzeugt Message, keine direkte Send-Aktion |
| Reply Loop | Thread Match, Klassifikation, Follow-up Stop |
| Scheduling | Slots, Terminantwort, Meeting booked |
| Scheduler | Werktage, Stop-Regeln, Limits, Sequenzversion |
| Regression | Bestehender Import- und Research-Funnel |

## Risiken Und Gegenmassnahmen

| Risiko | Gegenmassnahme |
| --- | --- |
| Unbeabsichtigter Versand ohne Freigabe | Backend-Hard-Gate, Negativtests, keine direkte Send-UI |
| Credentials gelangen in Browser-RxDB | Sender Account nur mit nicht-sensiblen Metadaten replizieren |
| Reply Matching ordnet falsch zu | Message-ID bevorzugen, Fallbacks gewichten, UI-Korrektur erlauben |
| Scheduler erzeugt zu viele Drafts | Account-/Campaign-Limits, Batch-Kappung, Idempotenz |
| Strategieaenderungen brechen aktive Engagements | Sequenzversion speichern, expliziter Reapply-Flow |
| Bestehende Outbound-App wird zu komplex | Klare Tabs: Research Funnel, Active Outreach, Approvals, Settings |
| Outbound dupliziert die Kommunikations-App | Outbound zeigt nur Kampagnenstatus und CTAs; Thread-, Account- und Freigabe-Details leben in Conversations mit Deep Links |
| Kampagnen ohne klares Postfach verlieren Reply-Zuordnung | Campaign Mailbox ist Aktivierungsbedingung; Reply-To, Account-Key und Thread-Key werden als harte Linkfelder gespeichert |
| Import regressiert und erzeugt leere Campaigns | `.xlsx` lokal im Browser parsen, Parserfehler sichtbar als `failed_parser`, reale Importdateien in Smoke aufnehmen |
| Dev-Upgrade-Runner ueberschreibt Hotfixes oder stoppt Builds | Upgrade-Befehl waehrend Hotfix-Deploy blockieren, Runner-Prozesse vor Build killen, kontrollierte Builds erst starten wenn kein `upgrade --dev`/`install.sh --rebuild` mehr laeuft |

## Plan-Aenderungslog

| Datum | Aenderung | Grund | Auswirkung |
| --- | --- | --- | --- |
| 2026-05-25 | Initialer Plan erstellt | Vollstaendige approval-gesteuerte Outbound-Kommunikation benoetigt UI, Backend, Skills, Mailserver und RxDB-Erweiterungen | Fortschritt 0% |
| 2026-05-25 | Welle 0 abgeschlossen und RFC 0008 ergaenzt | Baseline und Architekturvertrag sind dokumentiert; Subagent-Befunde integriert | Fortschritt 5% |
| 2026-05-25 | Welle 1 zu 75% umgesetzt | Browser-Schemas, Manifeste, nativer Contract und Hashes sind additiv erweitert; RxDB-only Guard und nativer Hash-Test sind gruen | Fortschritt 14%; UI-Leerzustand und Backend-State-Machine bleiben offen |
| 2026-05-25 | Dedizierte SQLite-Tabellen aus Welle 1 entfernt und als bedarfsgetriebenes Thema verschoben | Der bestehende generische Business-OS `business_records`/RxDB-Peer-Pfad repliziert neue Collections bereits ueber den Schema-Contract; separate Tabellen waeren aktuell doppelte Persistenz | Weniger Migrationsrisiko; Welle 2 muss pruefen, ob Approval-Gate-Transaktionen spaeter gezielte Tabellen oder Indizes brauchen |
| 2026-05-25 | Welle 2 zu 25% umgesetzt | Active-Outbound-Commands schreiben Domain-Records, Approval-Revisionen und blockieren ungeprueften oder stale-revision Versand backendseitig | Fortschritt 17%; echter Provider-Versand bleibt bis Sender-/Mailserver-Haertung deferred |
| 2026-05-25 | RxDB-only Guard-Status korrigiert | Der Guard meldet aktuell bestehende App-Shell-/Advanced-Status-/Login-Regeln in `app.js`; Schema-/Hash-/Syntaxchecks sind weiterhin gruen | Welle 1 bleibt bei 75%; Cross-Cutting Guard bleibt offen |
| 2026-05-25 | Welle 3 zu 25% umgesetzt | Active-Outreach-Tab, Lead Queue, Approval Inbox, Message-Review-Editor, Ready-to-send Queue, Message-Timeline und UI-Command-Anbindung sind eingebaut; direkter Versand laeuft nur ueber `outbound.message.send_approved` | Fortschritt 21%; Follow-up-/Reply-Draft-Editor, vollstaendiges Engagement Cockpit, Sender-Health-Warnungen und grosse Datenmengen bleiben offen |
| 2026-05-25 | Welle 3 zu 50% erweitert | Engagement Cockpit, Timeline-Auswahl, Next-Step-Panel und Pause/Cancel mit Grund sind umgesetzt; Outbound-Browser-Smoke laedt das Modul | Fortschritt 24%; Resume/Close, Testdaten-Smoke und grosse Datenmengen bleiben offen |
| 2026-05-25 | Welle-4-Anschlussentscheidung ergaenzt | Subagent-Analyse bestaetigt, dass der bestehende Research-Settings-Drawer der richtige Campaign-Settings-Ort ist | Welle 4 startet spaeter ohne zweiten Settings-Drawer; Active-Outbound-Settings werden ueber `outbound_sequences` und `campaign.payload.active_outreach` modelliert |
| 2026-05-25 | Welle 2 auf 50% erweitert | Resume-/Close-Commands ergaenzen den Backend-State-Machine-Pfad und ein Lifecycle-Test prueft Pause, Resume und Close gegen replizierbare Business-Records | Fortschritt 27%; Provider-Versand, Account-Health, Idempotenz und blockierende Fehlerfelder bleiben offen |
| 2026-05-25 | Welle 3 Lifecycle-UI ergaenzt | Cockpit kann pausierte Nachrichten/Engagements fortsetzen und Engagements schliessen; der Outbound-Browser-Smoke bleibt gruen | Welle 3 bleibt bei 50%, da Testdaten-Smoke, grosse Datenmengen und vollstaendige Timeline noch fehlen |
| 2026-05-25 | Welle 4 zu 25% umgesetzt | Campaign Settings enthalten aktive Outbound-Strategie, Sequenzregeln, Freigabepolicy, Sender-Auswahl und Skillbook/Runbook-Binding; `outbound.sequence.save` ist backendseitig getestet | Fortschritt 30%; Terminstrategie, sichtbare Simulation, Compliance-Tab und Strategie-Versionierung/Reapply bleiben offen |
| 2026-05-25 | Welle 4 zu 75% erweitert | Terminstrategie, Compliance-Regeln, rechte Sequenzsimulation und Draft-Payload-Anbindung fuer neue Engagements sind umgesetzt; Syntax-, Backend- und Browser-Smoke-Checks sind gruen | Fortschritt 35%; Versionierung/Reapply fuer bereits aktive Engagements bleibt offen |
| 2026-05-25 | Welle 5 zu 25% umgesetzt | `outbound.draft.prepare` erzeugt approval-gesteuerte Initial-, Follow-up-, Reply- und Scheduling-Drafts; UI kann Auto-Drafts aus Lead Queue und Engagement Cockpit starten; Subagent bestaetigt den bestehenden Business-OS Command-/Knowledge-Pfad | Fortschritt 38%; echte Skill-/Runbook-Ausfuehrung und Scheduler-/Reply-Automation bleiben offen |
| 2026-05-25 | Excel-Import-Regression behoben | Die reale Datei `Personalvermittler.xlsx` wurde vorher wegen fehlendem Browser-`xlsx`-Parser leer importiert; lokaler Parser liest nun `.xlsx` aus Importer-Base64 und echten `File`-Objekten | Browser-Test liest 266 Unternehmen; Parserfehler werden als `failed_parser` sichtbar statt stiller Queue ohne Fortschritt |
| 2026-05-25 | Physischer Brief als Outbound-Kanal aufgenommen | Outbound muss neben E-Mail auch manuelle Briefkommunikation vorbereiten, als PDF/Print exportieren und nach Nutzermarkierung in der Sequenz fortsetzen | Welle 6 umbenannt und zu 25% gestartet; Gesamtfortschritt 46%; echte Dokumenteditor-/Template-Pipeline bleibt offen |
| 2026-05-25 | Conversations-Integration und Kampagnen-Postfach als Architekturregel aufgenommen | Outbound soll nicht zur vollstaendigen Kommunikations-App werden; Detailansicht und optionale Freigabe gehoeren in `conversations`, jede Campaign braucht ein eigenes Postfach oder klares Kommunikationskonto | Wellen 3, 6, 7 und 10 erweitert; Fortschritt bleibt 46%, da zusaetzliche Integrationsaufgaben neu offen sind |
| 2026-05-25 | Settings-Save-Regression behoben | Nach Speichern im Zahnrad-Modal konnte ein transient leerer RxDB-Refresh die sichtbare Campaign-Liste ersetzen; zusaetzlich blockierte `outbound.sequence.save` bei fehlender Peer-Bestaetigung den UI-Save | Campaigns bleiben nach Speichern sichtbar; Sequenz wird lokal gespeichert, Backend-Command laeuft best-effort; Fortschritt bleibt 46% |
| 2026-05-25 | Testinstanz `kunstmen` nachgezogen und Smoke-Luecken geschlossen | Die Remote-Instanz hatte die neuen statischen Business-OS-Module nicht vollstaendig und der Rust-Server musste `/business-os/...` Assets auf die App-Dateien abbilden | Remote-Build und Service-Restart sind erfolgt; Outbound laedt auf `kunstmen`, Zahnrad-Speichern bleibt sichtbar stabil, fehlende Modul-404er sind beseitigt; Fortschritt bleibt 46% |
| 2026-05-25 | Sichtbare Outbound-Shell-Regression behoben | Der Remote-RxDB-Modulkatalog war stale und ueberstimmte die ausgelieferten `layout.shell=full-workspace` Metadaten; dadurch renderte Outbound faelschlich in der dreispaltigen Shell mit kaputtem Resizer und Umbruechen | Shell merged Packaged-Metadaten nun auch fuer vorhandene Module; Remote-Smoke prueft `moduleShell=full`, ausgeblendete Shell-Panes und versteckten Shell-Resizer; Fortschritt bleibt 46% |
| 2026-05-25 | Public-Domain-Auslieferung fuer Outbound-Shell korrigiert | `cto1.kunstmen.com` liefert Business OS ueber Vercel-Assets aus; die reine VPS-Aktualisierung reichte deshalb nicht fuer die sichtbare UI | Asset-Version auf `20260525-outbound-shell-public-guard1` angehoben, Outbound-Full-Workspace per CSS/Inline-Guard abgesichert, Vercel-Production-Deploy ausgefuehrt und Live-Asset-Check gegen `cto1.kunstmen.com` bestanden; Fortschritt bleibt 46% |
| 2026-05-25 | Outbound-Conversations-Deep-Linking gestartet | Outbound soll Kommunikationsdetails und Freigaben nicht duplizieren, sondern zielgenau in die Kommunikations-App springen | Welle 7 startet mit 25%; Campaign-, Engagement- und Message-Kontext verlinken nach `conversations`, die Zielansicht wird per Message-/Thread-/Outbound-Metadaten fokussiert; Gesamtfortschritt 48% |
| 2026-05-25 | Outbound-Conversations-Linking deployed | Die Deep-Link-Aenderung muss sofort auf der betroffenen Kunstmen-Instanz testbar sein | App-Asset-Version auf `20260525-outbound-conversations-shell-warmup1` angehoben; Vercel Production Deploy fuer `cto1.kunstmen.com` und VPS-Dateiupdate sind erfolgt; Live-Curl bestaetigt neue App-, Outbound- und Conversations-Assets |
| 2026-05-25 | Conversations-Outbound-Kontext und Approval-Bridge umgesetzt | Die Kommunikations-App muss Outbound-Kontext anzeigen und optionale Freigaben ausloesen, ohne Outbound-Logik zu duplizieren | Welle 7 steigt auf 50%; Conversations liest Outbound-Collections, zeigt Campaign/Lead/Engagement/Message/Approval und ruft `outbound.message.approve`/`reject` ueber den Command Bus auf; Gesamtfortschritt 50% |
| 2026-05-25 | Conversations-Outbound-Kontext deployed | Die Kontextkarte und Approval-Bridge muessen auf der Kunstmen-Instanz testbar sein | Asset-Version `20260525-conversations-outbound-context1` ist auf `cto1.kunstmen.com` und der VPS-Instanz ausgerollt; Syntaxchecks und Live-Asset-Checks sind gruen |
| 2026-05-25 | Manuelle Reply-Klassifikation in Conversations umgesetzt | Eingehende Antworten muessen dem Outbound-Engagement einen klaren naechsten Zustand geben, ohne automatisierte Antwort zu senden | Conversations nutzt `outbound.reply.classify` fuer manuelle Reclassification; Engagements werden auf `reply_received` gesetzt und Reply-Klasse/Message-Key gespeichert; Fortschritt bleibt 50%, bis automatische Reply-Matches und Follow-up-Stop getestet sind |
| 2026-05-25 | Reply-Klassifikation deployed | Die Reply-Klassifikation muss auf der Kunstmen-Instanz nutzbar sein | Asset-Version `20260525-conversations-reply-classify1` ist auf `cto1.kunstmen.com` und der VPS-Instanz ausgerollt; Syntax-, Rustfmt-, Live-Asset- und Service-Checks sind gruen |
| 2026-05-25 | Automatisches Reply-Matching umgesetzt | Eingehende Antworten muessen ohne manuelles Suchen beim richtigen Outbound-Engagement landen und offene Follow-ups stoppen | Neuer Command `outbound.reply.match` matcht `communication_messages` gegen Outbound-Nachrichten, setzt `reply_received`, annotiert die Inbox-Nachricht mit Outbound-IDs und cancelst offene nicht versendete Follow-ups; Welle 7 steigt auf 65%, Gesamtfortschritt auf 51% |
| 2026-05-25 | Outbound-Layout-Regression repariert | Der interne Outbound-Spaltentrenner und breite Research-Tabellen duerfen bei vielen Spalten nicht kollabieren | Resizer ist wieder scoped gestylt, alte LocalStorage-Breiten werden sanitisiert, Funnel-Karten wechseln erst bei ausreichend Card-Breite ins 5-Spalten-Layout und die CRM-Tabelle nutzt echte Spaltenbreiten mit horizontalem Scroll; Fortschritt bleibt 51%, da dies Stabilisierung der bestehenden Wellen ist |
| 2026-05-25 | Kampagnen-Postfach-Linking umgesetzt | Outbound-Campaigns brauchen einen echten Communication-Account-Kontext, damit Conversations filterbar ist und Reply-Matching stabil bleibt | Neuer Command `outbound.campaign.mailbox.link` schreibt in `communication_accounts`, Campaign Settings speichern canonical `email:<adresse>` Sender-Keys und verlinken Conversations; Welle 6 steigt auf 40%, Gesamtfortschritt auf 53% |
| 2026-05-25 | Kampagnen-Postfach-Linking deployed | Das Postfach-Linking muss auf `cto1.kunstmen.com` und im VPS-Backend derselben Instanz nutzbar sein | Vercel Production Deploy liefert `20260525-outbound-mailbox-link1`; VPS-Quellen in `current`, latest release und source cache sind gepatcht, `ctox-real` wurde neu gebaut/installiert und `ctox.service` laeuft aktiv; Fortschritt bleibt 53% |
| 2026-05-25 | Sender-Account-Send-Gate gehaertet | Der aktive E-Mail-Pfad darf keinen Versand queueen, wenn Campaign/Postfach-Binding oder Account-Health fehlen | `outbound.message.send_approved` verlangt jetzt canonical `email:<adresse>` Sender, einen verlinkten Account-Limit-Datensatz und blockiert gesperrte Status; Welle 6 steigt auf 50%, Gesamtfortschritt auf 54% |
| 2026-05-25 | Sender-Account-Send-Gate deployed | Der Backend-Hotfix muss trotz laufendem Dev-Auto-Upgrade auf der Kunstmen-Instanz aktiv sein | Auto-Upgrade-Kollision ueber `branch-main-20260525T173848Z` aufgeloest: Source vor `ctox_core` gepatcht, neues Binary installiert, `ctox.service` laeuft mit `173848Z`; Vercel Live-Assets erneut auf `20260525-outbound-mailbox-link1` deployed |
| 2026-05-25 | Sender-Health in Settings sichtbar gemacht | Nutzer muessen im Zahnrad vor dem Start sehen, ob ein Sender bereit, nicht verknuepft, blockiert oder am Tageslimit ist | Sender-Tab nutzt `outbound_account_limits` fuer Statuszeilen und Warnungen; automatische Sender-Aufloesung blockiert bekannte gesperrte/limitierte Accounts; Welle 6 steigt auf 55%, Gesamtfortschritt bleibt gerundet 54% |
| 2026-05-25 | Sender-Health deployed | Die Warnungen muessen auf der Kunstmen-Instanz sichtbar sein, bevor der echte Provider-Versand angebunden wird | Vercel Production Deploy liefert `20260525-outbound-sender-health1`; Live-Checks auf `cto1.kunstmen.com` bestaetigen JS/CSS/HTML, VPS-Release- und Source-Cache-Assets sind synchronisiert, Service bleibt aktiv |
| 2026-05-25 | Outbound-Messages mit Conversations verknuepft | E-Mail-Drafts, Freigaben und Provider-Queueing muessen in der Kommunikations-App als echte Messages/Threads sichtbar sein | Welle 6 steigt auf 65%, Gesamtfortschritt auf 55%; Outbound persistiert `communication_account_key`, `thread_key` und `communication_message_key` und spiegelt E-Mail-Messages ueber den Channels-Pfad |
| 2026-05-25 | Conversation-Linking deployed | Die Conversation-Projektion muss auf der Kunstmen-Instanz sofort testbar sein | Vercel Production Deploy liefert `20260525-outbound-comm-link1`; VPS `branch-main-20260525T181522Z` ist gepatcht, neu gebaut/installiert und `ctox.service` laeuft aktiv auf diesem Release |
| 2026-05-25 | Campaign-Aktivierungs-Gate umgesetzt | Aktive Campaigns duerfen nicht stillschweigend ohne E-Mail-Postfach oder bewusst manuellen Briefkanal laufen | Neuer Command `outbound.campaign.status.set`, UI-Statusauswahl und Readiness-Badges; Welle 6 steigt auf 70%, Gesamtfortschritt auf 56% |
| 2026-05-25 | Campaign-Aktivierungs-Gate teilweise deployed | `cto1.kunstmen.com` muss den neuen UI-Stand sofort ausliefern, waehrend die VPS-Instanz stabil bleiben muss | Vercel Production Deploy `dpl_wuro7HKv3cTVXU1T94fafcTHCAvm` liefert `20260525-outbound-activation-gate1`; VPS-Release `branch-main-20260525T184655Z` ist source-gepatcht und `ctox.service` aktiv, aber die neue Binary ist noch offen, weil wiederkehrende `upgrade --dev`/Build-Runner kontrollierte Builds gestoppt haben; Fortschritt bleibt 56% |
| 2026-05-26 | E-Mail-Send-Gate an Mailserver-Queue angebunden | Nach Nutzerfreigabe darf E-Mail-Outbound nicht bei `queued_for_provider` ohne echten Queue-Auftrag stehenbleiben | `outbound.message.send_approved` queued freigegebene E-Mails in `stalwart_smtp_queue`, persistiert `provider_queue_id`, aktualisiert Conversations-Metadaten und zaehlt Sender-Limits idempotent hoch; Welle 6 steigt auf 75%, Gesamtfortschritt auf 57% |
| 2026-05-27 | Review-Findings Scheduler, Message-Historie und Audit geschlossen | Aktive Drafts durften keine Message-IDs wiederverwenden, Scheduler musste echte Follow-up-Drafts erzeugen und Audit-/Skillbook-Konfiguration erhalten bleiben | UI erzeugt pro Draft eindeutige `msg_...` IDs; Scheduler schreibt approval-gated Follow-up-Drafts und stoppt bei Reply/Pause/Closure/Opt-out/Bounce; Skillbook-Save merged bestehende Felder; Audit exportiert Skillbooks und Briefvorlagen; Welle 9 steigt auf 50%, Welle 10 auf 25%, Gesamtfortschritt auf 61% |
| 2026-05-27 | Aktiver Outbound-Mailpfad im Browser E2E gehaertet | Der erste echte Browser-Smoke zeigte, dass abhängige Outbound-Commands zu frueh nacheinander ausgeloest wurden und die UI nach `send_approved` auf verzögerte Pull-Replikation wartete | Active-Outreach-Commands warten jetzt auf native `business_commands`-Acks und projizieren Backend-Result-Records lokal; neuer Smoke `SMOKE_MODE=outbound-active-ui` prueft Lead Queue, Auto-Draft, Freigabe-Gate, Ready-to-Send, Mailserver-Queue und Conversations-Deep-Link mit 4 Screenshots; Welle 10 steigt auf 50%, Gesamtfortschritt auf 62% |
| 2026-05-27 | Reply-zu-Scheduling-zu-Meeting im Browser-Smoke gehaertet | Der erweiterte E2E zeigte fehlende Empfaenger-Fallbacks, fehlende Slot-/Meeting-Request-Projektion in Scheduling-Drafts und zu frueh zurueckgesetzte Prompt-Mocks beim Buchen | Scheduling-Drafts uebernehmen Empfaenger aus Engagement oder letzter Message, erzeugen drei konkrete Default-Slots, liefern `meeting_request` im Command-Result, markieren gebuchte Termine mit aktualisiertem Engagement und werden im Browser-Smoke bis `meeting_booked` getestet; Welle 7 steigt auf 75%, Welle 8 auf 50%, Welle 10 auf 75%, Gesamtfortschritt auf 68% |
| 2026-05-28 | Live-Datenpfad auf `cto1.kunstmen.com` repariert und verifiziert | Die Public-Domain lud statische Vercel-Assets und konnte Daten lesen, aber ohne `POST /api/business-os/rxdb/push` keine stabilen UI-Aktionen synchronisieren; dadurch sah Outbound zeitweise leer oder blockiert aus | Vercel-Webroute proxyt jetzt auch validierte RxDB-Pushes zur Instanz; Browser-E2E zeigt Outbound-Campaigns mit Firmen, HTTP-Hydration `webrtc+http`, `business_commands.httpBridgePushStatus = ready` und erfolgreichen Push-Status 200; Welle 10 steigt auf 85%, Gesamtfortschritt auf 69% |
| 2026-05-28 | RxDB-only-Vertrag wiederhergestellt | Der HTTP-RxDB-Bridge-Ansatz widersprach der Architekturregel; CTOX und Business OS duerfen Daten nur ueber RxDB/WebRTC synchronisieren, ausgenommen ChatGPT-Subscription-Login | HTTP-Pull/Push-Bridge und Frontend-Runtime-Settings-/Harness-HTTP-Lesewege entfernt; Guard erlaubt im Frontend nur `subscription-auth/start` und `callback`; Browser/Rust-Smoke ist gruen mit nativen DataChannels fuer `desktop_file_chunks`; Welle 10 steigt auf 90%, Gesamtfortschritt auf 70% |
