# Business OS Customers CRM Release Notes

Stand: 2026-05-27

## Scope

`Kunden` ist eine native CTOX Business-OS-App fuer Bestandskunden-CRM.
Sie liefert Accounts, Contacts, Opportunities, CRM-Aufgaben, CRM-Notizen,
Timeline, Dateien, Outbound-Handoff, Dedupe-Review, Cross-App-Links,
Permission-limited States und Command-Audit.

## Native Business-OS Integration

- Modulvertrag: `modules/customers/module.json`, `schema.js`, `index.html`,
  `index.css`, `index.js`, direkte ESM, no-build.
- Datenvertrag: native RxDB/CTOX-DB Collections fuer Customers plus
  `business_commands`.
- Authoritative Aktionen: `customers.*` Commands ueber den Business-OS
  Command-Bus.
- Shell-Integration: full-workspace Layout, Registry-Eintrag, Schema-Contract,
  Hash-Registry und locale bundles.

## Bewusst Nicht Dupliziert

- Kommunikation bleibt in `Conversations`.
- Termine und Booking bleiben in `Calendar`.
- Dokumente bleiben in `Documents`.
- Longform-Notes bleiben in `Notes`.
- Tabellenanalyse und Bulk-Export bleiben in `Spreadsheets`.
- Neukundengewinnung bleibt in `Outbound`; `Kunden` uebernimmt nur qualifizierte
  Handoffs und Dedupe-Entscheidungen.

## Release Gates

- Customers Node-Smoke gruen.
- Schema-Contract und Hash-Registry gruen.
- Business-OS Shell-Smoke ueber `index.html#customers` gruen.
- Command-Bus schreibt `customers.account.create` in `business_commands`.
- Performance-Smoke mit 800 Accounts, 500 Contacts und 400 Opportunities gruen.
- Desktop/Mobile-Smoke fuer responsive Layout, lange Labels und Keyboard-Flows
  gruen.

## Bekannte Grenzen

- Ohne laufenden nativen RxDB/WebRTC Peer bleiben Commands lokal
  `pending_sync`; das ist erwartetes Offline-/Smoke-Verhalten.
- Browser-seitige UI validiert Eingaben fuer schnelle Rueckmeldung, bleibt aber
  keine Vertrauensgrenze. Backend-Commands validieren authoritative.
- Bulk-Selection und erweiterte Account-Owner-Workflows sind nicht Teil dieses
  Release-Scopes.
