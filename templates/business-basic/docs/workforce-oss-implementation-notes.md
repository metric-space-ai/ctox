# Workforce OSS Implementation Notes

Stand: 2026-05-07. Gelesene Implementierungen:

| System | Gelesene Dateien | Beobachtung fuer CTOX Business Basic |
| --- | --- | --- |
| Frappe HRMS | `hrms/api/roster.py`, `shift_assignment.py`, `employee_checkin.py` | Roster ist ein Kalender aus Personen, Schichten, Feiertagen, Abwesenheiten und Check-ins. Schichtwechsel werden als echte Mutation modelliert, nicht als UI-Filter. Ueberschneidungen und aktive Mitarbeiter werden serverseitig geprueft. |
| Kimai | `Entity/Timesheet.php`, `API/TimesheetController.php`, `PunchInOutMode.php` | Zeit ist ein eigener Beleg mit Status, Projekt/Aktivitaet, Export-/Billable-Markern und API-Filterung. Punch-In/Out ist ein eigener Tracking-Modus, nicht nur ein Datumsfeld. |
| Odoo | `hr_attendance.py`, `hr_attendance/controllers/main.py`, `hr_timesheet.py`, `hr_work_entry.py` | Attendance blockiert offene/ueberlappende Zeiten, fuehrt Geraete-/Ortsevidenz und trennt Work Entries mit Konflikt-/Validierungsstatus von Timesheets. |
| OrangeHRM | `WorkShift.php`, `AttendanceRecord.php`, `Timesheet.php`, `TimesheetService.php` | WorkShift ist Stammdatum. Wochen-Timesheets haben Workflow-Aktionen: view, submit, approve, reject, reset, modify, create. |

Umsetzungsschluss:

1. Einsatzplanung ist ein eigenes `operations/workforce` Modul. Payroll liest spaeter freigegebene `workforce_time_entry` Daten; Rechnungen lesen vorbereitete `workforce_handoff` Daten.
2. Ein sichtbarer Einsatz im Board ist `workforce_assignment`. Er ist das zentrale Arbeitsobjekt.
3. Zeitnachweise sind `workforce_time_entry`, nicht nur Felder auf dem Einsatz.
4. Jede UI-Aktion laeuft ueber einen Command. Fuer M0/M1 sind aktiv: `create_assignment`, `move_assignment`, `duplicate_assignment`, `archive_assignment`, `create_time_entry`, `approve_time_entry`, `request_correction`, `prepare_invoice_candidate`, Stammdaten-Commands.
5. Drag-Drop ist keine Optik. Es ruft `move_assignment` auf und persistiert.
6. Rechtsklick ist Objektsteuerung. Einsatzkarten bieten Bearbeiten, Zeit, Duplizieren, Freigeben, Rechnung vorbereiten, Archivieren.
7. Bottom-Drawer ersetzt ein Detailformular irgendwo anders: Score, Edit, Zeitstatus und Direktaktionen bleiben am gewaehlten Einsatz.
8. Score hat drei feste Spalten analog zur HR-Vorlage: Basis-Anforderungen, Leistungsanforderungen, Begeisterungsfaktoren.
9. Ueberschneidungen werden im Runtime-Command abgelehnt.
10. UI darf keine Aktion zeigen, fuer die es keinen Command, keine Persistenz und keinen Smoke-Test gibt.
