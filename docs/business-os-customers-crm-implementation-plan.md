# Business OS Customers CRM: Production Implementation Plan

Status: Planungsstand 2026-05-27
Ziel: Die neue Business-OS-App `Kunden` wird als production-ready CRM fuer
Bestandskunden, Kontakte, Opportunities, Aufgaben, Notizen, Aktivitaeten und
Outbound-Handoff gebaut. Twenty dient als Referenz fuer CRM-UX und Datenmodell,
aber die Umsetzung ist eine native CTOX Business-OS-App.

Referenz-RFC: `docs/rfcs/0009_business_os_customers_crm.md`

## Readiness Statement

Ja, die Implementierung ist grundsaetzlich startbar, aber nicht als "MVP".
Production-ready heisst hier: keine reine UI-Demo, keine isolierte lokale
Mock-App, keine nicht verdrahteten Controls. Jede Welle muss Datenvertrag,
Runtime-Verhalten, UI, Tests, Sync-/Command-Pfade und QA-Gates mitziehen.

Die Implementierung startet erst sauber, wenn Welle 0 abgeschlossen ist. Welle 0
ist absichtlich kurz, aber verbindlich: bestehende Business-OS-Patterns,
Schema-Contract, Command-Pfade, QA-Smokes und offene Worktree-Aenderungen
werden festgehalten, bevor neue Codepfade entstehen.

## Fortschrittsmodell

Der Projektfortschritt wird ueber Wellengewichtung berechnet. Eine Welle gilt
erst als abgeschlossen, wenn alle Aufgaben, Tests, UI-Smokes und
Akzeptanzkriterien dieser Welle erledigt sind.

| Welle | Gewicht | Status | Fortschritt |
| --- | ---: | --- | ---: |
| 0. Baseline, App-Vertrag & Arbeitsbaum-Sicherung | 5% | Abgeschlossen | 100% |
| 1. Produktvertrag, UX-Topologie & Cross-App-Grenzen | 7% | Abgeschlossen | 100% |
| 2. Datenmodell, Browser-Schemas & Schema-Contract | 10% | Abgeschlossen | 100% |
| 3. Backend Commands, Validierung & Activity Writer | 11% | Abgeschlossen | 100% |
| 4. Modul-Scaffold, Registry, Shell-Integration & Sync | 8% | Abgeschlossen | 100% |
| 5. Accounts/Contacts Core Workbench | 10% | Abgeschlossen | 100% |
| 6. Opportunities Pipeline & Sales Views | 10% | Abgeschlossen | 100% |
| 7. Record Pages, Tasks, Notes & Timeline | 11% | Abgeschlossen | 100% |
| 8. Outbound-Handoff, Import & Dedupe | 9% | Abgeschlossen | 100% |
| 9. Cross-App Links: Conversations, Calendar, Documents, Notes | 6% | Abgeschlossen | 100% |
| 10. Permissions, Audit, Error Recovery & Data Integrity | 5% | Abgeschlossen | 100% |
| 11. UX Hardening, Accessibility, Responsive & i18n | 4% | Abgeschlossen | 100% |
| 12. End-to-End QA, Performance, Release Gates & Docs | 4% | Abgeschlossen | 100% |
| **Gesamt** | **100%** | **Abgeschlossen** | **100%** |

Fortschritt je Welle:

- `0%`: Noch nicht begonnen.
- `25%`: Scope, Schnittstellen und konkrete Dateien sind festgelegt.
- `50%`: Kernverhalten ist implementiert und lokal nutzbar.
- `75%`: Tests, Migrationen und Browser-Smokes sind vorhanden und gruen.
- `100%`: Akzeptanzkriterien erfuellt, Doku aktualisiert, keine bekannten
  Release-Blocker.

Wenn sich Anforderungen, technische Befunde oder Risiken aendern, wird dieser
Plan waehrend der Implementierung aktualisiert. Jede substanzielle Aenderung
muss im Abschnitt "Plan-Aenderungslog" dokumentiert werden.

## Nicht Verhandelbare Produktregeln

1. `Kunden` ist eine native Business-OS-App, kein eingebettetes Twenty.
2. Das Modul bleibt no-build: `module.json`, `schema.js`, `index.html`,
   `index.css`, `index.js`, direkte ESM.
3. Durable Daten laufen ueber CTOX DB / RxDB-Collections aus `schema.js`.
4. Authoritative Domain-Uebergaenge laufen ueber `business_commands` und
   Backend-Validierung.
5. Die Browser-UI ist keine Vertrauensgrenze fuer Stage-Moves, Dedupe, Import
   oder Archivierung.
6. Die App dupliziert keine spezialisierten Business-OS-Apps: Kommunikation,
   Kalender, Dokumente, Tabellen und Longform-Notes werden verlinkt.
7. Keine externe Kundenkommunikation wird aus `Kunden` direkt gesendet.
8. Jede sichtbare Aktion muss echte lokale Daten, echte Commands oder einen
   ehrlich deaktivierten Zustand haben.
9. Leere, fehlerhafte, langsame und permission-limited States muessen gestaltet
   und getestet sein.
10. Production-ready heisst: Tests, Browser-QA, Schema-Contract, responsive
    Layout, i18n und Release-Doku sind Teil des Umfangs.

## Operating Thesis

Sales und Account-Owner pflegen bestehende Kundenbeziehungen in einer
dreigeteilten Arbeitsflaeche. Links waehlen sie Objekt, View und Filter; in der
Mitte bearbeiten sie Accounts, Kontakte oder Pipeline; rechts sehen sie Kontext,
Naechste Aktion, Timeline, Aufgaben, Notizen und Links zu angrenzenden Apps.
Der Loop schliesst, wenn ein Kundenstatus aktualisiert, eine Opportunity bewegt,
eine Aufgabe erledigt, ein Outbound-Handoff verknuepft oder ein naechster
Schritt dokumentiert ist.

## Topology Plan

- Shell: Business-OS full-workspace module.
- Left pane: Objekt-Navigation, gespeicherte Views, Suche, Owner-/Statusfilter.
- Center pane: Accounts table, Contacts table, Opportunities table/kanban.
- Right pane: selected record inspector, activity, tasks, notes, app links.
- Left drawer: Import, view setup, object setup.
- Bottom drawer: Bulk selection, dedupe queue, import batch details.
- Right drawer: focused record edit, task/note editor, linked app context.
- State surfaces: sync status, command status, empty states, loading states,
  failed replication/command states, permission restrictions.

## Action Map

- Creation: compact inline/drawer flows for account, contact, opportunity,
  task, note.
- Filtering: left pane filters and saved views.
- Selection: table row, board card and related-record selection update the
  right pane without losing workbench context.
- Bulk action: bottom drawer after multi-select.
- Review: dedupe and outbound handoff review in bottom/right drawers.
- Save: direct RxDB write only for low-risk local edits; command bus for
  authoritative transitions.
- Sync: module collections registered through `schema.js`, replicated via
  existing Business-OS sync.
- Error recovery: failed command and sync states visible near the action that
  failed.

## Mode Plan

- Light and dark mode must resolve through Business-OS shell tokens.
- German is the primary UX language; English labels must fit the same controls.
- Labels must remain short: `Kunden`, `Kontakte`, `Pipeline`, `Aufgaben`,
  `Notizen`, `Aktivitaet`, `Uebergabe`.
- Long customer names, company domains, opportunity titles and task titles must
  truncate or wrap without breaking pane layout.

## Skill- und App-Contract-Bewertung

Der lokale `frontend-skill` ist fuer `Kunden` passend, aber nur als
UI-/UX-Leitplanke. Er verlangt fuer Business-OS-style Apps genau die richtige
Arbeitsweise: Arbeitsflaeche zuerst, keine Hero-/Marketing-UI, ruhige
operative Komposition, klare Pane-Topologie, echte Interaktionen, stabile
Viewport- und Fehlerzustaende, Light/Dark Mode und Sprachwechsel.

Er reicht aber nicht als alleiniger Implementierungsvertrag. Die bestehenden
Business-OS-Apps erzwingen zusaetzliche lokale Regeln:

- jedes Modul besitzt `module.json`, `schema.js`, `index.html`, `index.css`,
  `index.js`
- fast alle Module deklarieren `layout.shell = full-workspace`
- Daten werden ueber `ctx.db.raw.<collection>` gelesen und geschrieben
- authoritative Arbeit laeuft ueber `ctx.commandBus.dispatch(...)`
- Apps laden HTML/CSS selbst und exportieren `mount(ctx)` mit Cleanup
- `loadModuleMessages`, `CtoxResizer`, Drawers, Context-Menues und lokale
  Persistence Keys sind etablierte Patterns
- Tests liegen appnah als `*.test.mjs`, `test.js` oder Smoke-Skripte
- der QA-Audit vom 2026-05-27 fordert einen shared Business-OS Component
  Contract fuer Header, Toolbar, Filter, Icon Buttons, Tabs, Tabellenheader,
  Dialoge, Resizer und Empty/Error States

Konsequenz: `Kunden` wird nach zwei Ebenen gebaut:

1. `frontend-skill`: UX-Qualitaet, Topologie, Informationsdichte,
   Interaktionsklarheit.
2. lokaler Business-OS-App-Contract: Manifest, Schema, Runtime, Sync, Commands,
   Tests, QA und Shared-Control-Konventionen.

Die Implementierung darf den Skill daher nicht als "Design frei Schnauze"
interpretieren. Er muss durch die bestehenden Module und den Business-OS-QA
Audit konkretisiert werden.

## Baseline Befunde Fuer Die Implementierung

Aktueller Worktree zu Beginn von Welle 0:

- `M src/apps/business-os/scripts/business-os-qa-baseline.mjs`
- `?? docs/audits/business-os-live-forensic-inventory-2026-05-27.md`
- `?? docs/business-os-customers-crm-implementation-plan.md`
- `?? docs/rfcs/0009_business_os_customers_crm.md`

Die vorhandene Aenderung an `src/apps/business-os/scripts/business-os-qa-baseline.mjs`
und das untracked Audit-Dokument werden als fremde oder parallele Arbeiten
behandelt und nicht ohne explizite Anforderung geaendert.

Beim abschliessenden Welle-0-Statuscheck waren zusaetzlich diese nicht von
dieser Welle erzeugten Aenderungen sichtbar:

- `M src/apps/business-os/modules/ctox/index.js`
- `M src/apps/business-os/modules/ctox/test.js`
- `M src/apps/business-os/modules/matching/index.css`
- `M src/apps/business-os/modules/matching/index.html`
- `M src/apps/business-os/modules/matching/index.js`
- `M src/apps/business-os/modules/matching/ui/index.js`

Diese Dateien bleiben fuer die Customers-Implementierung tabu, bis ihr Zustand
separat geklaert ist oder sie in einer spaeteren Welle explizit benoetigt
werden.

Gepruefte Referenzmodule:

- `tickets`: sauberer Mount, Realtime-Refresh, Command-Status, rechte
  Kontextspalte, Modul-Smoke.
- `outbound`: Sales-Domain, Import, Pipeline, Command Bus, Active-Outreach als
  angrenzende App.
- `calendar`: eigene Scheduling-/Booking-Domain, die Customers nur verlinkt.
- `conversations`: eigene Kommunikations-/Audit-Domain ueber
  `communication_messages`.
- `notes`: Rich-Text-/Notizpattern, aber Customers nutzt nur CRM-linked Notes
  und verlinkt Longform-Notes.
- `documents`: Dokument-/Version-/Blob-Pattern, Customers verlinkt Dokumente.
- `spreadsheets`: Bulk-/Tabellen-Arbeitsflaeche, Customers verlinkt Analyse und
  Import/Export.

Relevante Runtime-Pfade:

- `src/apps/business-os/app.js:218` laedt Core-Schemas.
- `src/apps/business-os/app.js:2967` registriert Modul-Schemas aus
  `modules/<id>/schema.js`.
- `src/apps/business-os/app.js:3029` startet Modul-Sync.
- `src/apps/business-os/app.js:3142` erstellt den Modul-Kontext.
- `src/apps/business-os/app.js:3181` stellt `ctx.db` bereit.
- `src/apps/business-os/app.js:3208` stellt `ctx.commandBus` bereit.
- `src/apps/business-os/shared/command-bus.js` schreibt Commands als
  `business_commands` Dokumente mit `status = pending_sync`.
- `src/core/business_os/store.rs:4700` akzeptiert RxDB Business Commands.
- `src/core/business_os/store.rs:4852` routet aktive Domain-Commands.
- `src/core/business_os/store.rs:5380` zeigt das aktuelle Outbound-Command
  Pattern als Vorlage fuer Customers.
- `src/core/business_os/store.rs:10205` schreibt generische Business Records.

Referenztests und Smokes:

- `src/apps/business-os/modules/tickets/tickets-module-smoke.mjs`
- `src/apps/business-os/modules/outbound/outbound.test.mjs`
- `src/apps/business-os/modules/calendar/calendar.test.mjs`
- `src/apps/business-os/modules/conversations/conversations.test.mjs`
- `src/apps/business-os/modules/notes/notes.test.mjs`
- `src/apps/business-os/modules/documents/documents.test.mjs`
- `src/apps/business-os/modules/spreadsheets/spreadsheets.test.mjs`
- `src/apps/business-os/modules/desktop/registry-launch-smoke.mjs`

Der QA-Audit `docs/audits/business-os-app-qa-2026-05-27.md` bleibt eine
verbindliche Warnung fuer Customers: Header, Toolbar, Filter, Icon Buttons,
Tabs, Tabellenheader, Dialoge, Resizer und Empty/Error States duerfen nicht
beliebig pro App neu erfunden werden. Wo noch keine shared Component existiert,
wird Customers die konsistenteste bestehende Variante uebernehmen und die
Abweichung dokumentieren.

## Geplanter Datei-Umfang Nach Wellen

Welle 1:

- `docs/rfcs/0009_business_os_customers_crm.md`
- `docs/business-os-customers-crm-implementation-plan.md`

Welle 2:

- `src/apps/business-os/modules/customers/schema.js`
- `src/core/business_os/business_os_schema_contract.json`
- `src/core/business_os/business_os_schema_hashes.json`
- `src/apps/business-os/modules/customers/customers.test.mjs`

Welle 3:

- `src/core/business_os/store.rs`
- `src/apps/business-os/modules/customers/customers.test.mjs`
- optional, falls noetig: `src/core/rxdb/tools/browser_rust_smoke.js`

Welle 4:

- `src/apps/business-os/modules/customers/module.json`
- `src/apps/business-os/modules/customers/icon.svg`
- `src/apps/business-os/modules/customers/index.html`
- `src/apps/business-os/modules/customers/index.css`
- `src/apps/business-os/modules/customers/index.js`
- `src/apps/business-os/modules/customers/locales/de.json`
- `src/apps/business-os/modules/customers/locales/en.json`
- `src/apps/business-os/modules/registry.json`
- `src/apps/business-os/shared/resizer.js`
- optional, falls Fallback-Manifest betroffen ist: `src/apps/business-os/app.js`

Wellen 5 bis 11:

- `src/apps/business-os/modules/customers/index.js`
- `src/apps/business-os/modules/customers/index.css`
- `src/apps/business-os/modules/customers/index.html`
- `src/apps/business-os/modules/customers/customers.test.mjs`
- optional, wenn wiederverwendbare Controls entstehen:
  `src/apps/business-os/shared/`

Welle 12:

- `src/apps/business-os/scripts/business-os-qa-baseline.mjs` nur nach
  expliziter Pruefung der bestehenden Aenderung
- `docs/audits/` fuer neue Customers-QA-Artefakte
- `docs/business-os-customers-crm-implementation-plan.md`
- `docs/rfcs/0009_business_os_customers_crm.md`

## Welle 0: Baseline, App-Vertrag & Arbeitsbaum-Sicherung

Gewicht: 5%

Ziel: Vor dem ersten Implementierungs-Commit ist klar, welche bestehenden
Patterns, Worktree-Aenderungen, Runtime-Vertraege und QA-Gates gelten.

Aufgaben:

- [x] Aktuellen `git status` dokumentieren und fremde/unrelated Aenderungen
  explizit unangetastet lassen.
- [x] Mindestens diese Module als Implementierungsreferenz pruefen:
  `tickets`, `outbound`, `calendar`, `conversations`, `notes`, `documents`,
  `spreadsheets`.
- [x] Business-OS Runtime-Pfade fuer Schema-Registrierung, Sync,
  `ctx.commandBus` und Module-Mount festhalten.
- [x] Relevante Tests und Smokes identifizieren.
- [x] RFC 0009 mit diesem Plan abgleichen.

Akzeptanzkriterien:

- [x] Dieser Plan nennt alle Dateien, die in den naechsten Wellen beruehrt
  werden.
- [x] Es gibt keine Implementierung, die bestehende Outbound/Core-Aenderungen
  ueberschreibt.
- [x] Baseline-Notizen sind im Abschnitt "Umsetzungslog" dokumentiert.

## Welle 1: Produktvertrag, UX-Topologie & Cross-App-Grenzen

Gewicht: 7%

Ziel: Der genaue Produktionsumfang ist festgelegt, bevor Datenmodell und UI
entstehen.

Aufgaben:

- [x] Twenty-Referenzmodell fuer Company, Person, Opportunity, Task, Note,
  Timeline analysieren.
- [x] UI/UX-Feature-Matrix erstellen.
- [x] Finales App-Label festlegen: `Kunden` vs. `CRM` vs. `Customers`.
- [x] Default-Views und initiale Tabellen-/Board-Spalten finalisieren.
- [x] Deep-Link-Vertrag fuer Calendar, Conversations, Documents, Notes und
  Outbound definieren.
- [x] Production-scope explizit von deferred scope trennen.

Akzeptanzkriterien:

- [x] Keine v1-UI-Flaeche ist unklar oder doppelt mit anderer App.
- [x] Jede verlinkte App hat einen konkreten Link-/Context-Vertrag.
- [x] RFC 0009 ist mit finalen Scope-Entscheidungen aktualisiert.

## Welle 2: Datenmodell, Browser-Schemas & Schema-Contract

Gewicht: 10%

Ziel: Kunden-Daten koennen lokal-first gespeichert, repliziert und vom nativen
Peer verstanden werden.

Neue Collections:

- `customer_accounts`
- `customer_contacts`
- `customer_opportunities`
- `customer_tasks`
- `customer_notes`
- `customer_activities`
- `customer_files`
- `customer_views`
- `customer_view_filters`
- `customer_view_sorts`
- `customer_import_batches`
- `customer_dedupe_candidates`

Aufgaben:

- [x] `src/apps/business-os/modules/customers/schema.js` erstellen.
- [x] `src/core/business_os/business_os_schema_contract.json` erweitern.
- [x] Schema-Hashes aktualisieren, falls erforderlich.
- [x] Collection-Indizes fuer Suche, Owner, Stage, Account, updated_at,
  deleted/is_deleted definieren.
- [x] Tombstone-Konvention (`deleted_at_ms` oder `is_deleted`) festlegen.
- [x] Test fuer Schema-Import und Manifest-Collections schreiben.

Akzeptanzkriterien:

- [x] Browser und native Peer kennen alle Customers Collections.
- [x] Leere Collections sind auf Schema-/Sync-Contract-Ebene registriert und
  hash-stabil; UI-Smoke folgt mit dem Modul-Scaffold in Welle 4.
- [x] Schema-Contract-Tests sind gruen.

## Welle 3: Backend Commands, Validierung & Activity Writer

Gewicht: 11%

Ziel: Kritische Domain-Uebergaenge sind backend-validiert und auditierbar.

Commands:

- `customers.account.create`
- `customers.account.update`
- `customers.account.archive`
- `customers.contact.create`
- `customers.contact.update`
- `customers.contact.archive`
- `customers.opportunity.create`
- `customers.opportunity.update`
- `customers.opportunity.move_stage`
- `customers.opportunity.close_won`
- `customers.opportunity.close_lost`
- `customers.task.create`
- `customers.task.update`
- `customers.task.complete`
- `customers.note.create`
- `customers.note.update`
- `customers.activity.record`
- `customers.view.save`
- `customers.import.from_outbound`
- `customers.dedupe.resolve`

Aufgaben:

- [x] Command-Erkennung in `src/core/business_os/store.rs` ergaenzen.
- [x] Idempotente Upsert-/Patch-Helfer fuer Customers Records implementieren
  oder bestehende generische Pfade wiederverwenden.
- [x] Activity Writer fuer create/update/stage/task/note/handoff implementieren.
- [x] Validierung fuer Pflichtfelder, Statuswerte, Stage-Uebergaenge und Dedupe
  ergaenzen.
- [x] Command-Outcome-Struktur fuer UI definieren.

Akzeptanzkriterien:

- [x] Browser kann Commands schreiben; Backend verarbeitet sie deterministisch.
- [x] Ungueltige Commands schlagen sichtbar und ohne Teilzustand fehl.
- [x] Activities entstehen fuer relevante Domain-Uebergaenge.

## Welle 4: Modul-Scaffold, Registry, Shell-Integration & Sync

Gewicht: 8%

Ziel: `Kunden` ist als Business-OS-App sichtbar, startbar, synchronisiert und
leerzustandsfest.

Dateien:

- `src/apps/business-os/modules/customers/module.json`
- `src/apps/business-os/modules/customers/schema.js`
- `src/apps/business-os/modules/customers/index.html`
- `src/apps/business-os/modules/customers/index.css`
- `src/apps/business-os/modules/customers/index.js`
- `src/apps/business-os/modules/customers/customers.test.mjs`
- `src/apps/business-os/modules/registry.json`

Aufgaben:

- [x] Modulordner und Manifest erstellen.
- [x] Icon und Store-Metadaten ergaenzen.
- [x] `mount(ctx)` mit HTML/CSS/i18n/cleanup implementieren.
- [x] Realtime-Subscriptions fuer Customers Collections verdrahten.
- [x] Empty, loading, sync unavailable und command unavailable states anzeigen.
- [x] Basic mount smoke schreiben.

Akzeptanzkriterien:

- [x] App erscheint im Launcher/App Store nach bestehendem Muster.
- [x] Startet ohne Daten nonblank und ohne Console-Fehler.
- [x] Unmount entfernt Subscriptions, Timer und Resizer.

## Welle 5: Accounts/Contacts Core Workbench

Gewicht: 10%

Ziel: Account- und Kontaktpflege ist produktiv nutzbar.

Aufgaben:

- [x] Account-Tabelle mit Suche, Filtern, Sortierung und Auswahl.
- [x] Contact-Tabelle mit Account-Relation und Primary-Contact-Anzeige.
- [x] Inline-/Drawer-Erstellung fuer Accounts und Contacts.
- [x] Owner, Status, Stage, Health und next action bearbeiten.
- [x] Simple saved views fuer Accounts/Contacts.
- [x] Pure helper tests fuer Filter, Sort, Auswahl, Search.

Akzeptanzkriterien:

- [x] Nutzer kann Account und Kontakt anlegen, bearbeiten, archivieren.
- [x] Auswahl aktualisiert den rechten Inspector stabil.
- [x] Lange Namen und leere Felder brechen Layout nicht.

## Welle 6: Opportunities Pipeline & Sales Views

Gewicht: 10%

Ziel: Die klassische Sales-Pipeline ist production-tauglich.

Aufgaben:

- [x] Opportunity-Tabelle und Opportunity-Kanban implementieren.
- [x] Default-Stages und Stage-Spalten definieren.
- [x] Drag/drop oder explizite Move-Aktion mit Command-Gate verdrahten.
- [x] Column count und Amount-Sum aggregieren.
- [x] `My Pipeline`, `Closing This Month`, `Renewals`, `Closed Won/Lost`
  Views implementieren.
- [x] Stage-Move Tests und Command-Payload Tests schreiben.

Akzeptanzkriterien:

- [x] Stage-Moves erzeugen valide Commands und Activities.
- [x] Board bleibt bei leeren und grossen Spalten stabil.
- [x] Closed Won/Lost sind explizite, auditierbare Uebergaenge.

## Welle 7: Record Pages, Tasks, Notes & Timeline

Gewicht: 11%

Ziel: Einzelne Accounts, Contacts und Opportunities haben eine vollwertige
Arbeitsoberflaeche.

Aufgaben:

- [x] Fixed Account Page mit Overview, Contacts, Opportunities, Activity, Files.
- [x] Fixed Contact Page mit Overview, Activity, Opportunities.
- [x] Fixed Opportunity Page mit Overview, Activity, Files.
- [x] CRM-linked Tasks erstellen, bearbeiten, abschliessen.
- [x] CRM-linked Notes erstellen und bearbeiten.
- [x] `customer_activities` Timeline rendern.
- [x] Tests fuer Timeline-Zusammenstellung und Related-Record-Auswahl.

Akzeptanzkriterien:

- [x] Rechte Pane und Detailtabs zeigen konsistente Related Records.
- [x] Task/Note-Aktionen sind echte Writes oder Commands.
- [x] Timeline bleibt sortiert, filterbar und leerzustandsfest.

## Welle 8: Outbound-Handoff, Import & Dedupe

Gewicht: 9%

Ziel: Neukunden aus Outbound koennen sauber in Bestandskunden uebergehen.

Aufgaben:

- [x] `customers.import.from_outbound` Command implementieren.
- [x] Link/Import UI fuer Outbound Companies, Contacts, Pipeline Items.
- [x] Domain-/Email-Dedupe Kandidaten erzeugen.
- [x] Dedupe Queue und Resolve Flow implementieren.
- [x] Import batches mit Ergebnis und Fehlern speichern.
- [x] Handoff-Activity schreiben.

Akzeptanzkriterien:

- [x] Import erzeugt keine Duplikate ohne sichtbare Dedupe-Entscheidung.
- [x] Ursprungsdaten bleiben ueber `source` und `source_record_id` nachvollziehbar.
- [x] Nutzer kann bestehende Accounts/Contacts verlinken statt neu anzulegen.

## Welle 9: Cross-App Links

Gewicht: 6%

Ziel: Customers nutzt bestehende Business-OS-Apps, statt sie zu duplizieren.

Aufgaben:

- [x] Conversations-Link fuer `communication_messages`, Threads und Kontaktbezug.
- [x] Calendar-Link fuer Meetings, Close Dates, Follow-up Dates.
- [x] Documents-Link fuer Angebote, Vertraege, Anlagen.
- [x] Notes-Link fuer Longform-Notes.
- [x] Spreadsheets-Link fuer Export/Bulk-Analyse.
- [x] Outbound-Ruecklink fuer Handoff-Evidence.

Akzeptanzkriterien:

- [x] Jeder Link hat stabile Parameter und sinnvollen Fallback.
- [x] Customers zeigt Preview-Kontext, aber nicht die fremde App als Kopie.
- [x] Fehlende Ziel-App oder fehlende Daten erzeugen klaren Empty State.

## Welle 10: Permissions, Audit, Error Recovery & Data Integrity

Gewicht: 5%

Ziel: Die App ist robust gegen fehlerhafte Eingaben, Sync-Probleme und
unberechtigte Aktionen.

Aufgaben:

- [x] Owner/Admin-Konventionen anwenden.
- [x] Permission-limited UI-Zustaende anzeigen.
- [x] Command failure, pending_sync und completed States darstellen.
- [x] Dedupe-, Stage- und Archive-Operationen idempotent machen.
- [x] Soft-delete/Tombstone Verhalten testen.

Akzeptanzkriterien:

- [x] Keine kritische Domain-Aktion verlaesst sich allein auf Browser-State.
- [x] Fehler sind in der UI lokalisierbar und wiederholbar oder korrigierbar.
- [x] Audit-Aktivitaeten bleiben nach Updates konsistent.

## Welle 11: UX Hardening, Accessibility, Responsive & i18n

Gewicht: 4%

Ziel: Die App fuehlt sich wie eine native, stabile Business-OS-App an.

Aufgaben:

- [x] Light/dark mode pruefen.
- [x] Deutsch/Englisch Labels pruefen.
- [x] Keyboard-Fokus, ARIA-Labels und Buttons pruefen.
- [x] Pane-Resizer und Drawers stabilisieren.
- [x] Responsive Verhalten bei kleinen Breiten pruefen.
- [x] Long-name, empty, loading, failed-sync und locked-state Screens pruefen.

Akzeptanzkriterien:

- [x] Keine ueberlappenden Texte oder Controls in Standard-Viewports.
- [x] Wichtige Aktionen sind per Tastatur erreichbar.
- [x] App bleibt bei Shell-Resize und Drawer-Wechsel stabil.

## Welle 12: End-to-End QA, Performance, Release Gates & Docs

Gewicht: 4%

Ziel: Die App ist releasefaehig.

Aufgaben:

- [x] Unit-/Node-Tests fuer Customers laufen gruen.
- [x] Schema-Contract-/Hash-Tests laufen gruen.
- [x] Browser-QA ueber Business-OS Shell: Start, Create, Edit, Pipeline Move,
  Dedupe, Handoff, Cross-App-Link.
- [x] Performance bei realistischen Listen pruefen.
- [x] Docs/RFC/Plan aktualisieren.
- [x] Bekannte Blocker dokumentieren oder fixen.

Akzeptanzkriterien:

- [x] Keine bekannten P0/P1-Fehler.
- [x] Keine blanke App, keine unverdrahteten Primaeraktionen.
- [x] Release-Notizen nennen Scope, Grenzen, Tests und verbleibende Risiken.

## Test- und Verifikationskommandos

Die konkrete Liste wird waehrend Welle 0 finalisiert. Erwartete Basis:

```bash
node --input-type=module --check < src/apps/business-os/modules/customers/index.js
node --input-type=module --check < src/apps/business-os/modules/customers/schema.js
node src/apps/business-os/modules/customers/customers.test.mjs
node src/core/rxdb/tools/build_business_os_schema_contract.mjs
node src/apps/business-os/rxdb/tests/schema-hash-registry-smoke.mjs
git diff --check
```

Browser-QA erfolgt gegen eine lokale Business-OS-Instanz und wird mit
Screenshots/Console-Befunden dokumentiert.

Finale Release-Notizen: `docs/business-os-customers-crm-release-notes.md`.

## Umsetzungslog

| Datum | Welle | Aenderung | Ergebnis / Risiko |
| --- | --- | --- | --- |
| 2026-05-27 | Planung | Initialen Produktionsplan fuer `Kunden` erstellt | Umsetzung ist in 13 Wellen steuerbar; Welle 0 bleibt Startgate. |
| 2026-05-27 | 0 | Bestehende Business-OS-App-Manifeste, Mount-Patterns, Tests und QA-Audit gegen `frontend-skill` geprueft | Skill ist passend fuer UX, aber nicht ausreichend als Implementierungsvertrag; lokaler Business-OS-App-Contract bleibt verbindlich. |
| 2026-05-27 | 0 | Worktree-Baseline, Runtime-Pfade, Referenztests und geplanter Datei-Umfang dokumentiert | Welle 0 abgeschlossen; Implementierung darf ab Welle 1/2 starten, ohne fremde Worktree-Aenderungen zu ueberschreiben. Beim finalen Statuscheck tauchten parallele Aenderungen in `ctox` und `matching` auf; sie werden nicht angefasst. |
| 2026-05-27 | 1 | App-Label, Default-Views, initiale Spalten, Deep-Link-Vertrag und Production-vs-Deferred Scope in RFC 0009 finalisiert | Welle 1 abgeschlossen; naechster Schritt ist Welle 2 mit Schemas und Schema-Contract. Parallel geaenderte `ctox`/`matching` Dateien bleiben ausserhalb des Customers-Scopes. |
| 2026-05-27 | 2 | Customers-Schemas, Contract-Generator, Business-OS Schema-Contract, Hash-Fixture und Browser-Registry erweitert | Welle 2 abgeschlossen; 12 Customers-Collections sind native/browser-bekannt. Der neue all-collection Rust-Guard deckte zusaetzlich fehlende Hashes fuer `outbound_letter_templates` und `outbound_skillbooks` auf; diese Registry-Drift ist mitbehoben. |
| 2026-05-27 | 2 | Worktree vor Welle 3 erneut geprueft | Parallel sind inzwischen weitere Business-OS-Module geaendert (`app-store`, `conversations`, `documents`, `knowledge`, `notes`, `outbound`, `reports`, `research`, `shiftflow`, `spreadsheets`) sowie ein untracked `package.json`; sie bleiben ausserhalb des Customers-Scopes, ausser eine spaetere Welle benoetigt sie explizit. |
| 2026-05-27 | 3 | Native `customers.*` Active-Commands, Validierungen, Upsert/Patch-Helfer und Activity Writer in `store.rs` implementiert | Welle 3 abgeschlossen; Account, Contact, Opportunity, Task, Note, View, Activity, Outbound-Import und Dedupe-Resolve sind backendseitig verarbeitet. Invalid Commands schreiben eine failed `business_commands` Projection ohne partiellen Kundenrecord. |
| 2026-05-27 | 4 | Customers-Modulmanifest, Icon, Registry-Eintrag, Full-Workspace-Scaffold, RxDB-Subscriptions, Empty/Data-State, Inspector und Account-Create-Command UI ergaenzt | Welle 4 abgeschlossen; Modul ist startbar und syncbar. Der Business-OS Schema-Contract wurde neu generiert und enthaelt dabei auch parallele Ticket-Schema-Aenderungen aus dem bestehenden Arbeitsbaum. |
| 2026-05-27 | 5 | Account-/Contact-Workbench mit Tabs, Sortierung, Suche, gespeicherten Ansichten, Edit/Create/Archive-Commands und Inspector-Aktionen implementiert | Welle 5 abgeschlossen; Account- und Contact-Pflege laeuft ueber native `customers.*` Commands, Helper-Tests decken Filter/Sort/Payloads ab. |
| 2026-05-27 | 6 | Opportunity-Tab mit Tabelle, Kanban-Board, Pipeline-Aggregationen, explizitem Stage-Move, Close-Won/Lost und Pipeline-Views implementiert | Welle 6 abgeschlossen; Opportunity-Commands und Board-/Pipeline-Helfer sind durch Smoke-Tests abgedeckt. |
| 2026-05-27 | 7 | Record-Detailtabs, Account-/Contact-/Opportunity-Kontexte, CRM-linked Tasks, Notes, Files und Timeline-Merge implementiert | Welle 7 abgeschlossen; Task-/Note-Commands, Related-Record-Filter und Timeline-Sortierung sind durch Customers-Smoke-Tests abgedeckt. |
| 2026-05-27 | 8 | Outbound-Uebergabe-Tab, Import-Command-UI, Dedupe-Queue, Dedupe-Entscheidungen und Review-Inspector implementiert | Welle 8 abgeschlossen; Outbound-Handoff-Filter, Import-Payloads und Dedupe-Resolve-Payloads sind durch Customers-Smoke-Tests abgedeckt. |
| 2026-05-27 | 9 | Cross-App-Tab fuer Conversations, Calendar, Documents, Notes, Spreadsheets und Outbound mit stabilen Hash-Kontextparametern implementiert | Welle 9 abgeschlossen; Link-Parameter, Preview-Zaehlung und Fallback-Navigation sind durch Customers-Smoke-Tests abgedeckt. |
| 2026-05-27 | 10 | Permission-Gates, Read-only UI, Command-Protokoll und idempotente Aktionszustaende implementiert | Welle 10 abgeschlossen; business_commands werden lokal angezeigt, Mutationen respektieren canModifyModule/session roles, failed/pending/completed States sind durch Tests und Browser-Smoke abgedeckt. |
| 2026-05-27 | 11 | Keyboard-Auswahl, Tab-Navigation, Sortier-ARIA, Fokuszustaende und Mobile-Pane-Regeln gehaertet | Welle 11 abgeschlossen; lange deutsche/englische Labels, Desktop/Mobile-Iframes, sichtbare Default-Sortierung und Keyboard-Flows sind im Browser-Smoke abgedeckt. |
| 2026-05-27 | 12 | Shell-E2E, Command-Bus-Smoke, Performance-Smoke und Release Notes abgeschlossen | Welle 12 abgeschlossen; `index.html#customers` mountet die App ueber die echte Shell, Create schreibt `customers.account.create` nach `business_commands`, 800/500/400 Listen sind performant. |

## Plan-Aenderungslog

| Datum | Aenderung | Begruendung | Auswirkung |
| --- | --- | --- | --- |
| 2026-05-27 | Plan initial angelegt | User verlangt production-ready Umsetzung statt MVP | Plan wird waehrend Implementierung laufend aktualisiert. |
| 2026-05-27 | Skill-/App-Contract-Bewertung ergaenzt | Pruefung der bestehenden Business-OS-Apps zeigt zusaetzliche lokale Runtime- und QA-Regeln | `Kunden` wird gegen Skill plus lokalen Business-OS-App-Contract gebaut. |
| 2026-05-27 | Welle 1 abgeschlossen | Produkt- und App-Grenzen sind konkret genug fuer Schema- und Modulaufbau | Umsetzung kann mit Welle 2 starten. |
| 2026-05-27 | Welle 2 abgeschlossen | Datenmodell, Contract-Generator, Schema-Hashes und native/browser Guards sind gruen | Umsetzung kann mit Welle 3 Backend Commands, Validierung und Activity Writer fortfahren. |
| 2026-05-27 | Welle 3 abgeschlossen | Backend-Domainpfade und Activity-Audit sind gruen getestet | Umsetzung kann mit Welle 4 Modul-Scaffold, Registry, Shell-Integration und Sync fortfahren. |
| 2026-05-27 | Welle 4 abgeschlossen | Modul-Scaffold, Registry, Sync-Collections und erste echte Command-Oberflaeche sind gruen getestet | Umsetzung kann mit Welle 5 Accounts/Contacts Core Workbench fortfahren. |
| 2026-05-27 | Welle 5 abgeschlossen | Account-/Contact-Pflege ist als native Workbench nutzbar und mit Command-Payload-Tests abgesichert | Umsetzung kann mit Welle 6 Opportunities Pipeline & Sales Views fortfahren. |
| 2026-05-27 | Welle 6 abgeschlossen | Pipeline-Tabelle, Kanban, Stage-Moves und Close-Uebergaenge sind native Commands | Umsetzung kann mit Welle 7 Record Pages, Tasks, Notes & Timeline fortfahren. |
| 2026-05-27 | Welle 7 abgeschlossen | Detailseiten, Aufgaben, Notizen, Dateien und Timeline sind in die rechte Record Page integriert | Umsetzung kann mit Welle 8 Outbound-Handoff, Import & Dedupe fortfahren. |
| 2026-05-27 | Welle 8 abgeschlossen | Outbound-Import und Dedupe-Review sind als native Customers-Workbench verfuegbar | Umsetzung kann mit Welle 9 Cross-App Links fortfahren. |
| 2026-05-27 | Welle 9 abgeschlossen | Customers verlinkt angrenzende Apps mit Kontext statt deren Funktionen zu duplizieren | Umsetzung kann mit Welle 10 Permissions, Audit, Error Recovery & Data Integrity fortfahren. |
| 2026-05-27 | Welle 10 abgeschlossen | Permission-limited States, Command-Audit und idempotente UI-Aktionen sind implementiert | Umsetzung kann mit Welle 11 UX Hardening, Accessibility, Responsive & i18n fortfahren. |
| 2026-05-27 | Welle 11 abgeschlossen | Accessibility- und Responsive-Haertung ist gruen getestet | Umsetzung kann mit Welle 12 End-to-End QA, Performance, Release Gates & Docs fortfahren. |
| 2026-05-27 | Welle 12 abgeschlossen | Shell-E2E, Performance, Release-Gates und Release Notes sind abgeschlossen | Customers CRM ist gemaess Plan zu 100% umgesetzt. |
