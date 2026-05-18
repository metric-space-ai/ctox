# RFC 0005: Business Basic Workforce

Status: M0/M1 implementation in progress  
Datum: 2026-05-07  
Route: `/app/operations/workforce`

## Ziel

Business Basic bekommt ein echtes Einsatzplanungs- und Zeiterfassungsmodul zwischen Operations und Payroll. Das Modul plant Personen auf Schichten/Auftraege, erfasst Istzeiten, prueft Abweichungen, gibt Zeiten frei und bereitet Uebergaben an Rechnungen und Payroll vor.

## Nicht-Ziel

- Keine Lohnabrechnung im Workforce-Modul. Payroll bleibt `/app/operations/payroll`.
- Keine Rechnungserstellung im Workforce-Modul. Workforce bereitet nur abrechenbare Zeitpositionen vor.
- Keine reine Listenverwaltung. Die Hauptansicht ist immer ein Wochenplan mit Personen x Tagen.

## Referenztransfer

Die HR-Vorlage (`jobmatchView.html`) wird nicht kopiert, sondern fachlich umgemuenzt:

| HR-Vorlage | Workforce |
| --- | --- |
| Unternehmen links | Bedarf, Personal, Schichttypen links |
| Ausschreibung Mitte | Wochen-Einsatzplan Mitte |
| Kandidaten rechts | Zeitpruefung, Blocker, Uebergaben rechts |
| Match unten | Einsatz-Score unten |
| CV rechts | Uebergabe-/Audit-Side-Panel rechts |
| Job links | Stammdaten-/Setup-Side-Panel links |
| Kandidat im Prozess | Einsatzkarte im Plan |
| Match-Score | Einsatzbereitschaft: Basis, Leistung, Bonus |

## Datenmodell M0/M1

### `workforce_person`

- `id`
- `number`
- `name`
- `role`
- `team`
- `active`
- `location_id`
- `skills`
- `weekly_hours`

### `workforce_shift_type`

- `id`
- `name`
- `start_time`
- `end_time`
- `role`
- `color`
- `billable`

### `workforce_location_slot`

- `id`
- `name`
- `zone`
- `capacity`

### `workforce_assignment`

- `id`
- `title`
- `person_id`
- `shift_type_id`
- `location_slot_id`
- `date`
- `start_time`
- `end_time`
- `customer`
- `project`
- `status`
- `notes`
- `blocker`
- `created_at`
- `updated_at`

Statuswerte: `draft`, `planned`, `in_progress`, `needs_time`, `needs_review`, `approved`, `blocked`, `invoice_ready`, `archived`.

### `workforce_time_entry`

- `id`
- `assignment_id`
- `person_id`
- `date`
- `start_time`
- `end_time`
- `break_minutes`
- `status`
- `evidence`
- `note`
- `approved_at`
- `approved_by`

Statuswerte: `draft`, `submitted`, `approved`, `correction_requested`.

### `workforce_handoff`

Vorbereitete Position fuer `business/invoices` oder `operations/payroll`.

## Serverregeln

1. Ein Einsatz braucht aktive Person, Schichttyp, Arbeitsplatz, Datum und gueltiges Zeitfenster.
2. Zwei aktive Einsaetze derselben Person duerfen sich am gleichen Datum nicht ueberschneiden.
3. Zwei Istzeiten derselben Person duerfen sich am gleichen Datum nicht ueberschneiden.
4. Freigabe ist nur auf `submitted` Zeitnachweisen sinnvoll.
5. Rechnungsuebergabe braucht genehmigte Zeit und abrechenbaren Schichttyp.
6. Archivierte Einsaetze bleiben fuer Audit erhalten, verschwinden aber aus dem Plan.

## UI M0/M1

Mitte ist der Wochenplan. Links stehen Eingang/Bedarf und Stammdatenzugang. Rechts stehen Pruefung, Blocker und Uebergaben. Unten klappt pro Einsatz ein Score-/Edit-Modul auf.

Pflichtinteraktionen:

- Einsatz per Formular anlegen.
- Einsatzkarte per Drag-Drop auf Person/Tag verschieben.
- Einsatzkarte rechtsklicken: Bearbeiten, Zeit erfassen, Duplizieren, Freigeben, Rechnung vorbereiten, Archivieren.
- Leere Zelle rechtsklicken: Einsatz hier anlegen.
- Einsatzkarte anklicken: Bottom-Drawer oeffnen.
- Bottom-Drawer: Einsatz editieren, Zeit anlegen, duplizieren, archivieren, freigeben, Rechnung vorbereiten.
- Linker Side-Drawer: Person und Schichttyp anlegen.
- Rechter Side-Drawer: CTOX Payloads, Events, vorbereitete Uebergaben.

## CTOX

Jede relevante Karte traegt:

- `data-context-module="operations"`
- `data-context-submodule="workforce"`
- `data-context-record-type`
- `data-context-record-id`
- `data-context-skill="product_engineering/business-basic-module-development"`

Kontextaktionen:

- Einsatz bearbeiten
- Zeit erfassen
- Duplizieren
- Score pruefen
- Zeit freigeben
- Korrektur anfordern
- Auslastung pruefen

## Akzeptanz

M1 ist akzeptiert, wenn:

1. Route rendert.
2. API GET liefert Seed/Persistenz.
3. `create_assignment` persistiert.
4. `move_assignment` persistiert Drag-Drop-Ziel.
5. Overlap wird abgelehnt.
6. `duplicate_assignment` erzeugt neuen Einsatz.
7. `create_time_entry` erzeugt Zeitnachweis.
8. `approve_time_entry` setzt Zeit auf `approved` und Einsatz auf `approved`.
9. `prepare_invoice_candidate` erzeugt Uebergabe.
10. Reload erhaelt Status.
11. Browser-Test prueft Klick, Rechtsklick, Drag-Drop und Bottom-Drawer.

## M2 Abschluss

M2 erweitert M1 um die fachlichen Uebergaben und Regeln, die aus einer Einsatzplanung ein produktionsfaehiges Modul machen:

- Postgres-Schema: `0013_operations_workforce.sql` legt Personen, Schichttypen, Slots, Abwesenheiten, wiederkehrende Schichtmuster, Einsaetze, Zeitnachweise und Handoffs an.
- Arbeitszeitregeln: aktive Abwesenheiten blockieren neue Einsaetze; Tagesgrenze, Wochenlast und Ruhezeit werden als Score-Checks bewertet; harte Tages-/Ruhezeitverletzungen blockieren Commands.
- Wiederkehrende Schichten: `create_recurring_shift_pattern` legt Muster an, `materialize_recurring_shift_pattern` erzeugt konkrete Einsaetze im Zeitraum.
- Payroll-Handoff: `prepare_payroll_candidate` erzeugt freigegebene Stunden mit `employeeId`, `periodId`, `componentId`, Stunden, Satz und Betrag. Payroll liest diese Kandidaten beim Run als Komponente `pc-workforce-hours`.
- Rechnungs-Handoff: `prepare_invoice_candidate` erzeugt abrechenbare Positionen, `create_invoice_draft` erzeugt daraus einen persistenten Workforce-Rechnungsdraft mit Business-Invoice-Deep-Link.
- UI: linker Drawer enthaelt Abwesenheit und Muster; rechter Drawer enthaelt Payroll-Kandidaten und Rechnungsdrafts; Bottom-Drawer und Rechtsklick enthalten Payroll- und Rechnungsdraft-Aktionen.

M2-Akzeptanz:

1. `create_absence` persistiert und blockiert Einsaetze im Zeitraum.
2. `create_recurring_shift_pattern` plus `materialize_recurring_shift_pattern` erzeugt konkrete Einsaetze.
3. `prepare_payroll_candidate` erzeugt Betrag aus freigegebener Zeit.
4. Payroll-Run uebernimmt Workforce-Kandidaten in `workforce_hours`.
5. `create_invoice_draft` erzeugt Betrag und Business-Invoice-Link.
6. Browserproof zeigt Abwesenheit, Muster, Payroll, Rechnungsdraft, Score-Spalten und Rechtsklick-Aktionen ohne Console-Fehler.
