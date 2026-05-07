# Workforce Implementation Map

Stand: 2026-05-07. Zielmodul: `operations/workforce`.

## OSS Decisions

| Quelle | Gelesene Implementierung | Entscheidung fuer Business Basic |
| --- | --- | --- |
| Frappe HRMS | `roster.py`, `shift_assignment.py`, `employee_checkin.py` | Roster ist die Hauptarbeitsflaeche. Schichtzuweisung und Check-in bleiben getrennte Objekte. Ueberschneidungen werden serverseitig blockiert. |
| Kimai | `Timesheet.php`, `TimesheetController.php`, `PunchInOutMode.php` | Zeitnachweise haben eigenen Status und billable/export-Relevanz. Punch/Erfassung ist Workflow, nicht nur Textfeld. |
| Odoo | `hr_attendance.py`, `hr_timesheet.py`, `hr_work_entry.py` | Attendance/Work Entry pruefen Ueberschneidung, Validierung und Konfliktstatus. Freigabe ist eigener Zustand. |
| OrangeHRM | `WorkShift.php`, `AttendanceRecord.php`, `TimesheetService.php` | WorkShift ist Stammdatum; Timesheet-Freigabe hat explizite Aktionen: submit, approve, reject/reset. |

## Reference UI Analogue Map

| HR-Vorlage | Workforce Umsetzung |
| --- | --- |
| Unternehmen links | Bedarf, Personalliste, Schicht-/Personenstammdaten links |
| Ausschreibungen Mitte | Wochenplan Personen x Tage als zentrales Arbeitsobjekt |
| Kandidaten rechts | Zeitpruefung, Blocker, Rechnungs-/Payroll-Uebergaben rechts |
| Kandidatenkarte im Prozess | Einsatzkarte im Plan |
| Match-Bar unten | Bottom-Drawer pro Einsatz mit Score, Editor, Zeitstatus und Direktaktionen |
| Basis/Leistung/Begeisterung | Basis-Anforderungen, Leistungsanforderungen, Begeisterungsfaktoren je Einsatz |
| Job-Sidepanel links | linker Stammdaten-/Setup-Drawer |
| CV-Sidepanel rechts | rechter Audit-/Payload-/Uebergabe-Drawer |
| Rechtsklick Kandidat | globales CTOX-Rechtsklickmenue fuer `workforce_assignment` |
| Direktaktionen | `ctox:context-action` Handler ruft dieselben Commands wie UI-Buttons |
| Entfernt | CV/PDF, Firmen-Scraping, Kandidatenstatus; fachlich ersetzt durch Person, Schicht, Zeitnachweis, Handoff |

## Central Object Lifecycle

`workforce_assignment` ist das zentrale Objekt.

```text
draft -> planned -> needs_time -> needs_review -> approved -> invoice_ready
                         |             |
                         v             v
                      blocked      correction_requested via time entry
                         |
                         v
                      planned
any active state -> archived
```

Begleitobjekte:

- `workforce_person`
- `workforce_shift_type`
- `workforce_location_slot`
- `workforce_time_entry`
- `workforce_handoff`

## Visible Affordance Inventory

| Sichtbares Element | Handler | Command | Persistenz | Beweis |
| --- | --- | --- | --- | --- |
| Formular `Neuer Einsatz` | `handleCreateAssignment` | `create_assignment` | `.ctox-business/workforce.json` | smoke |
| Plus in leerer Zelle | `createDefaultAssignment` | `create_assignment` | Datei | browser |
| Einsatzkarte Drag-Drop | pointer release + HTML5 drop | `move_assignment` | Datei | browser |
| Einsatzkarte Klick | `setSelectedAssignmentId` | Auswahl, kein Datencommand | UI-State | browser |
| Bottom-Drawer Speichern | `handleUpdateAssignment` | `update_assignment` | Datei | smoke/browser |
| Bottom `Zeitnachweis erstellen` | `createTimeForAssignment` | `create_time_entry` | Datei | smoke |
| Bottom `Naechsten Tag duplizieren` | `duplicateNextDay` | `duplicate_assignment` | Datei | smoke/browser |
| Bottom `Archivieren` | inline handler | `archive_assignment` | Datei | browser |
| Bottom `Zeit freigeben` | inline handler | `approve_time_entry` | Datei | smoke |
| Bottom `Rechnung vorbereiten` | inline handler | `prepare_invoice_candidate` | Datei | smoke |
| Rechte Zeitpruefung `Freigeben` | inline handler | `approve_time_entry` | Datei | smoke |
| Rechte Zeitpruefung `Korrektur` | inline handler | `request_correction` | Datei | smoke |
| Rechte Blocker `Loesen` | inline handler | `resolve_blocker` | Datei | UI/API |
| Linker Drawer Schichttyp | `handleCreateShiftType` | `create_shift_type` | Datei | browser |
| Linker Drawer Mitarbeiter | `handleCreatePerson` | `create_person` | Datei | browser |
| Globales Rechtsklick `Duplizieren` | `ctox:context-action` | `duplicate_assignment` | Datei | browser |
| Globales Rechtsklick `Zeit erfassen` | `ctox:context-action` | `create_time_entry` | Datei | smoke |
| Globales Rechtsklick `Freigeben` | `ctox:context-action` | `approve_time_entry` | Datei | smoke |

## Command-To-Affordance Matrix

| Command | UI | CTOX Rechtsklick | Runtime/API | Test |
| --- | --- | --- | --- | --- |
| `create_person` | Setup Drawer | `workforce-person-edit` oeffnet Setup | `executeWorkforceCommand` + API | browser |
| `update_person` | API/runtime | CTOX Prompt | Runtime/API | typecheck |
| `toggle_person_active` | API/runtime | CTOX Prompt | Runtime/API | typecheck |
| `create_shift_type` | Setup Drawer | Setup ueber Person/Edit | Runtime/API | browser |
| `rename_shift_type` | API/runtime | CTOX Prompt | Runtime/API | typecheck |
| `create_location_slot` | API/runtime | CTOX Prompt | Runtime/API | typecheck |
| `rename_location_slot` | API/runtime | CTOX Prompt | Runtime/API | typecheck |
| `create_assignment` | Formular, leere Zelle | CTOX Prompt kann vorbereiten | Runtime/API | smoke/browser |
| `update_assignment` | Bottom Drawer | `workforce-assignment-edit` selektiert Bottom Drawer | Runtime/API | smoke/browser |
| `move_assignment` | Drag-Drop | CTOX Prompt fuer Umplanung | Runtime/API | smoke/browser |
| `duplicate_assignment` | Bottom Drawer | `workforce-assignment-duplicate` | Runtime/API | smoke/browser |
| `archive_assignment` | Bottom Drawer | M2 direkt | Runtime/API | browser |
| `resolve_blocker` | Rechte Blockerkarte | M2 direkt | Runtime/API | API/UI |
| `create_time_entry` | Bottom, Rechtsklick | `workforce-assignment-time` | Runtime/API | smoke |
| `update_time_entry` | API/runtime | CTOX Prompt | Runtime/API | typecheck |
| `approve_time_entry` | Rechts, Bottom | `workforce-time-approve` | Runtime/API | smoke |
| `request_correction` | Rechts | `workforce-time-correction` | Runtime/API | smoke |
| `create_absence` | Setup Drawer | CTOX Prompt/Setup | Runtime/API | smoke/browser |
| `approve_absence` | Runtime + Setup Daten | CTOX Prompt | Runtime/API | smoke |
| `cancel_absence` | Runtime + Setup Daten | CTOX Prompt | Runtime/API | smoke path |
| `create_recurring_shift_pattern` | Setup Drawer | CTOX Prompt/Setup | Runtime/API | smoke/browser |
| `materialize_recurring_shift_pattern` | Setup Drawer submit | CTOX Prompt | Runtime/API | smoke |
| `prepare_payroll_candidate` | Bottom, Rechtsklick | `workforce-assignment-payroll` | Runtime/API + Payroll run | workforce/payroll smoke |
| `prepare_invoice_candidate` | Bottom, Rechtsklick | direct after Freigabe | Runtime/API | smoke |
| `create_invoice_draft` | Bottom, Rechtsklick | `workforce-assignment-invoice-draft` | Runtime/API | smoke/browser |

## Data Ownership And Adjacent Handoffs

Owned by Workforce:

- `workforce_person`
- `workforce_shift_type`
- `workforce_location_slot`
- `workforce_assignment`
- `workforce_time_entry`
- `workforce_absence`
- `workforce_recurring_pattern`
- `workforce_handoff`
- `workforce_assignment_events`
- `workforce_time_entry_events`

Adjacent reads and writes:

- Payroll reads prepared `workforce_payroll_candidate` rows during `queue_run` and adds them as `pc-workforce-hours`.
- Invoices receive prepared invoice candidates through `workforce_invoice_draft` plus a Business-Invoice deep link.
- Operations projects/work-items can be linked through `project`/`customer` text, with typed keys reserved in the Postgres schema.

## Browser Proof Plan

Executed against `http://localhost:3001/app/operations/workforce?locale=de&theme=light`.

1. Login and open route.
2. Confirm text `Einsatzplanung`, `Wochenplan`, `Eingang & Personal`.
3. Click `wa_1002`; confirm bottom drawer with `Basis-Anforderungen`, `Leistungsanforderungen`, `Begeisterungsfaktoren`.
4. Right-click same card; confirm global CTOX menu with `Bearbeiten`, `Duplizieren`, `Zeit erfassen`, `Payroll vorbereiten`, `Rechnungsdraft`, `Score pruefen`.
5. Execute `Duplizieren`; confirm duplicate visible on following day.
6. Drag `wa_1002` to Anna/Dienstag; confirm card visible in target cell and status `Gespeichert`.
7. Open setup drawer; confirm `Abwesenheit` and `Wiederkehrende Schicht`.
8. Open handoff drawer; confirm `Payroll-Kandidaten` and `Rechnungsdrafts`.
9. Check browser console errors: none.

## Regression Story Set

Re-tested after final changes:

- US-01 create assignment: `workforce-smoke.mjs`
- US-02 move/drag: browser proof
- US-06 create time entry: `workforce-smoke.mjs`
- US-07 approve time entry: `workforce-smoke.mjs`
- US-09 handoff: `workforce-smoke.mjs`
- US-31 right-click action: browser proof
- US-36 command error surfaced: overlap assertion in smoke
- US-50 smoke/browser proof: typecheck, workforce smoke, operations smoke, browser proof

## M2 Completion

- Postgres migration added: `packages/db/drizzle/0013_operations_workforce.sql`.
- Drizzle schema added: `workforcePeople`, `workforceShiftTypes`, `workforceLocationSlots`, `workforceAbsences`, `workforceRecurringShiftPatterns`, `workforceAssignments`, `workforceTimeEntries`, `workforceHandoffs`.
- Runtime commands added: absence, recurring pattern, materialization, payroll candidate, invoice draft.
- Payroll bridge added: Payroll run reads Workforce candidates by period and applies them to `pc-workforce-hours`.
- Browser proof: route, setup drawer, handoff drawer, bottom score drawer and right-click M2 actions visible with zero console errors.

Remaining product hardening after M2:

- Country-specific legal calendars/collective agreements instead of default policy limits.
- Typed Customer/Project selectors once Sales/Projects expose stable picker APIs.
- Accounting invoice creation beyond draft/link handoff once the Business invoice writer is finalized.
