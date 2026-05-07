# Workforce Acceptance Matrix

Statuswerte: `missing`, `partial`, `needs proof`, `done`, `blocked`.

| Story | Manual UI | CTOX path | DB | API/runtime | UI file | Context menu | Test | Browser proof | Status | Blocker |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| US-01 | linker Einsatz-Form | Prompt auf Plan/Zelle | `workforce_assignment` | `create_assignment` | `workforce-workbench.tsx` | Prompt CTOX | smoke | route/form visible | done | - |
| US-02 | Drag-Drop Karte auf Zelle | Prompt Umplanung | `workforce_assignment` | `move_assignment` | `workforce-workbench.tsx` | Score/Prompt | smoke | moved `wa_1002` to target cell | done | - |
| US-03 | Bottom `Naechsten Tag duplizieren` | Direct `workforce-assignment-duplicate` | `workforce_assignment` | `duplicate_assignment` | `workforce-workbench.tsx` | global bridge | smoke | duplicate visible next day | done | - |
| US-04 | Overlap Fehler sichtbar | CTOX prueft Konflikt | `workforce_assignment` | overlap guard | `workforce-workbench.tsx` | Prompt CTOX | smoke | error path represented | done | - |
| US-05 | Teamliste + Score | Score pruefen | `workforce_person.active` | score basis check | `workforce-workbench.tsx` | `workforce-assignment-score` | smoke data | inactive Lina blocker visible | done | - |
| US-06 | Bottom `Zeitnachweis erstellen` | Direct `workforce-assignment-time` | `workforce_time_entry` | `create_time_entry` | `workforce-workbench.tsx` | global bridge | smoke | bottom action visible | done | - |
| US-07 | rechte `Freigeben` | Direct `workforce-time-approve` | `workforce_time_entry` | `approve_time_entry` | `workforce-workbench.tsx` | global bridge | smoke | right rail visible | done | - |
| US-08 | rechte `Korrektur` | Direct `workforce-time-correction` | `workforce_time_entry` | `request_correction` | `workforce-workbench.tsx` | global bridge | smoke path | right rail visible | done | - |
| US-09 | Bottom `Rechnung vorbereiten` | Prompt/Direct after approved | `workforce_handoff` | `prepare_invoice_candidate` | `workforce-workbench.tsx` | Prompt CTOX | smoke | right handoff rail visible | done | - |
| US-10 | Bottom Score | `Score pruefen` | computed score | `scoreAssignment` | `workforce-workbench.tsx` | `workforce-assignment-score` | browser | drawer columns visible | done | - |
| US-11 | Score-Spalten | `Score pruefen` | computed checks | `scoreAssignment` | `workforce-workbench.tsx` | Prompt CTOX | browser | basis/leistung/bonus visible | done | - |
| US-12 | Plus in leerer Zelle | Prompt auf Zelle | `workforce_assignment` | `create_assignment` | `workforce-workbench.tsx` | Prompt CTOX | browser | plus buttons visible | done | - |
| US-13 | Setup Drawer Schichttyp | CTOX Setup Prompt | `workforce_shift_type` | `create_shift_type` | `workforce-workbench.tsx` | setup via context action | browser | drawer visible | done | - |
| US-14 | Setup Drawer Person | CTOX Setup Prompt | `workforce_person` | `create_person` | `workforce-workbench.tsx` | `workforce-person-edit` opens setup | browser | drawer visible | done | - |
| US-15 | Bottom `Archivieren` | future direct action | `workforce_assignment.status` | `archive_assignment` | `workforce-workbench.tsx` | Prompt CTOX | smoke cleanup/browser | action visible | done | - |
| US-16 | rechte Blocker-Liste | CTOX Blocker Summary | `workforce_assignment.blocker` | score/status | `workforce-workbench.tsx` | Score Prompt | browser | blocker rail visible | done | - |
| US-17 | Button `Loesen` | CTOX Blocker loesen | `workforce_assignment.status` | `resolve_blocker` | `workforce-workbench.tsx` | Prompt CTOX | API/UI path | blocker button visible | done | - |
| US-18 | rechter Event Drawer | CTOX Ereignisse erklaeren | `events` | event append | `workforce-workbench.tsx` | Prompt CTOX | smoke events | drawer visible | done | - |
| US-19 | `data-context-*` Karten | Prompt CTOX | context metadata | bridge item parser | `workforce-workbench.tsx` | global bridge | operations smoke | right-click menu visible | done | - |
| US-20 | Karte zeigt `keine Istzeit` | CTOX offene Istzeiten | `workforce_time_entry` absent | score check | `workforce-workbench.tsx` | Score Prompt | browser | text visible | done | - |
| US-21 | Score Abweichung | CTOX Varianz pruefen | time vs assignment | `scoreAssignment` | `workforce-workbench.tsx` | Score Prompt | smoke data | variance check visible | done | - |
| US-22 | Handoff erst nach Freigabe | CTOX verweigert ohne Freigabe | `workforce_handoff` | approved guard | `workforce-workbench.tsx` | Prompt CTOX | smoke | disabled before approve | done | - |
| US-23 | Kunde/Projekt im Drawer | CTOX zuordnen | assignment fields | `update_assignment` | `workforce-workbench.tsx` | Edit direct opens drawer | browser | fields visible | done | - |
| US-24 | Person/Schicht/Slot editieren | CTOX umbuchen | assignment fields | `update_assignment` | `workforce-workbench.tsx` | Edit direct opens drawer | browser | selects visible | done | - |
| US-25 | Wochenplan scanbar | CTOX Tageslast | assignments | snapshot scores | `workforce-workbench.tsx` | Prompt CTOX | browser | roster visible | done | - |
| US-26 | Rolle am Schichttyp | CTOX Rollenfit | `workforce_shift_type.role` | score support | `workforce-workbench.tsx` | setup prompt | browser | setup list visible | done | - |
| US-27 | Schichtfarbe | none required | `workforce_shift_type.color` | render style | `workforce-workbench.tsx` | n/a | browser | card border color visible | done | - |
| US-28 | aktiv/inaktiv Personen | CTOX Einsatzlage | `workforce_person.active` | score check | `workforce-workbench.tsx` | person plan prompt | browser | Lina inaktiv visible | done | - |
| US-29 | Nachweis/Evidenz | CTOX Nachweis pruefen | `workforce_time_entry.evidence` | time commands | `workforce-workbench.tsx` | Score Prompt | smoke | score/detail visible | done | - |
| US-30 | objektlokales Bearbeiten | Edit direct | assignment fields | `update_assignment` | `workforce-workbench.tsx` | `workforce-assignment-edit` | browser | bottom drawer opens | done | - |
| US-31 | keine detached card actions | direct menu/actions at card | context payload | same commands | `workforce-workbench.tsx` | global bridge | browser | right-click menu visible | done | - |
| US-32 | CTOX Payload Drawer | CTOX Payload lesen | `ctoxPayloads` | payload append | `workforce-workbench.tsx` | Prompt CTOX | smoke | right drawer visible | done | - |
| US-33 | freie Kapazitaet | CTOX freie Slots | missing assignment | roster cells | `workforce-workbench.tsx` | cell prompt | browser | plus/free cells visible | done | - |
| US-34 | Schichtzeiten defaults | CTOX Schicht planen | shift type times | `create_assignment` | `workforce-workbench.tsx` | Prompt CTOX | smoke | form defaults visible | done | - |
| US-35 | Reload Persistenz | CTOX API Snapshot | file store | `getWorkforceSnapshot` | runtime/API | n/a | smoke | reload checked by smoke | done | - |
| US-36 | Command-Fehler sichtbar | CTOX Konflikt erklaert | rejected mutation | guards throw error | `workforce-workbench.tsx` | Prompt CTOX | smoke overlap | error state supported | done | - |
| US-37 | Location Slot Modell | CTOX Slot anlegen | `workforce_location_slot` | create/rename slot | runtime/API | Prompt CTOX | API path | setup list visible | done | - |
| US-38 | Board-first UI | CTOX auf Karte/Zelle | assignments | snapshot | `workforce-workbench.tsx` | global bridge | browser | center board visible | done | - |
| US-39 | Planung/Ist getrennt | CTOX unterscheidet | assignment/time_entry | separate commands | `workforce-workbench.tsx` | time direct | smoke | right time queue visible | done | - |
| US-40 | nicht abrechenbar blockt Handoff | CTOX erklaert | shift billable flag | handoff guard | runtime/UI | Prompt CTOX | smoke guard | handoff action gated | done | - |
| US-41 | freie Zelle direkt | CTOX Zelle | assignment | `create_assignment` | `workforce-workbench.tsx` | cell context path | browser | plus visible | done | - |
| US-42 | Wiedererkennbarer Plan | CTOX Person/Tag/Einsatz | assignment | snapshot | `workforce-workbench.tsx` | card context | browser | week roster visible | done | - |
| US-43 | eingereichte Zeiten rechts | CTOX Freigabeliste | time entries | filter submitted | `workforce-workbench.tsx` | time context | smoke/browser | right queue visible | done | - |
| US-44 | Bottom ohne Routewechsel | CTOX selected object | UI state | selected assignment | `workforce-workbench.tsx` | edit direct | browser | drawer visible same route | done | - |
| US-45 | Setup Side Drawer | CTOX Stammdaten | person/shift | create commands | `workforce-workbench.tsx` | person edit opens setup | browser | drawer visible | done | - |
| US-46 | Audit Side Drawer | CTOX Audit erklaert | events | event append | `workforce-workbench.tsx` | Prompt CTOX | smoke | drawer visible | done | - |
| US-47 | konkrete Fehlchecks | CTOX failed checks | score checks | `scoreAssignment` | `workforce-workbench.tsx` | Score Prompt | browser | checks visible | done | - |
| US-48 | Blockergrund persistent | CTOX Blocker liest | blocker/note | correction command | `workforce-workbench.tsx` | correction direct | smoke | blocker rail visible | done | - |
| US-49 | wiederholbare Aktionen | CTOX direct actions | same commands | bridge handler | `workforce-workbench.tsx` | global bridge | browser | menu actions visible | done | - |
| US-50 | Regression Proof | CTOX Skill gate | all owned records | all commands | all workforce files | global bridge | typecheck/smoke | click/right-click/drag | done | - |
| US-51 | Setup `Abwesenheit` | CTOX Abwesenheit eintragen | `workforce_absence` | `create_absence` | `workforce-workbench.tsx` | setup prompt | workforce smoke | setup drawer visible | done | - |
| US-52 | Abwesenheit blockt Einsatz | CTOX verweigert Planung | `workforce_absence` + `workforce_assignment` | absence guard | runtime/API | prompt path | workforce smoke | error path covered | done | - |
| US-53 | Setup `Wiederkehrende Schicht` | CTOX Muster anlegen | `workforce_recurring_pattern` | `create_recurring_shift_pattern` | `workforce-workbench.tsx` | setup prompt | workforce smoke | setup drawer visible | done | - |
| US-54 | Muster materialisieren | CTOX Zeitraum planen | `workforce_assignment` | `materialize_recurring_shift_pattern` | runtime/API | prompt path | workforce smoke | generated cards via API | done | - |
| US-55 | Arbeitszeitregeln im Score | `Score pruefen` | computed score | `workingTimePolicyFindings` | `workforce-workbench.tsx` | score prompt | workforce smoke | score drawer visible | done | - |
| US-56 | Payroll vorbereiten | Direct `workforce-assignment-payroll` | `workforce_handoff` | `prepare_payroll_candidate` | `workforce-workbench.tsx` | global bridge | workforce smoke | bottom/right-click visible | done | - |
| US-57 | Payroll-Run liest Workforce | CTOX Payroll Run pruefen | Payroll additional | `additionalsWithWorkforce` | `payroll-runtime.ts` | payroll prompt | payroll smoke | n/a | done | - |
| US-58 | Rechnungsdraft erstellen | Direct `workforce-assignment-invoice-draft` | `workforce_handoff` | `create_invoice_draft` | `workforce-workbench.tsx` | global bridge | workforce smoke | right drawer visible | done | - |
| US-59 | Uebergaben rechts sichtbar | CTOX Uebergaben zusammenfassen | handoff arrays | snapshot | `workforce-workbench.tsx` | prompt path | browser proof | handoff drawer visible | done | - |
| US-60 | M2 Browserproof | CTOX Skill gate | all M2 records | all M2 commands | all M2 UI | global bridge | typecheck/smoke | no console errors | done | - |
