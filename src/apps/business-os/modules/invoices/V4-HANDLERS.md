# invoices – Shell-v4-Handler

Strang H5, Stand 15.10.2026. Quelle (read-only): `~/.local/state/shell-v4-src/4794af03d/src/core/business_os/invoices.rs`
(2 883 Z., davon 1 879 Produktion), Routing `command_plane.rs:2269` (`starts_with("invoices.")`) →
`handle_invoices_active_command :62`.
Handler-Vertrag: `handlers/<command_type>.js`, `export default async function handle(command, ctx)`.
Gemeinsame Fachlogik: `handlers/_lib/invoices.js` (Validierung, Zeilen-/Steuerrechnung, Nummernkreis, Buchungssatz,
Gutschrift-Entwurf, Fälligkeitsabfrage).
Tests: `node --test src/apps/business-os/modules/invoices/handlers/tests/*.test.mjs` (24/24 grün; In-Memory-ctx mit
Schemaprüfung aus `collections.schema.json`; die Rust-Tests post/allocate/dunning/lifecycle/validate sind nachgebildet).

## Stub-Lage (wichtig)

Von den 23 Armen rufen 18 eine Funktion `*_stub`. **Davon tragen 8 trotz des Namens echte Logik**
(create, update, delete, post, payment.allocate, payment.unallocate, dunning.run, dunning.letter.send) – sie sind voll
portiert. **10 sind echte Stubs** und bleiben Stubs: 9 brechen mit „not yet implemented (planned for Phase 7+)“ ab
(→ `throw {code:'unavailable'}` mit wörtlicher Meldung, Export `STUB = true`), `payment.match_suggestions` liefert
immer `{stub:true, suggestions:[]}`. `dunning.letter.send` ist ein **halber Stub**: markiert „per E-Mail gesendet“,
rendert und versendet aber nichts – treu so übernommen, kein Capability-Aufruf.

## Befehlstypen

| Befehlstyp | Einstufung | Rust-Ort | Rust-Charakter | Ziel (v4) | Stand |
|---|---|---|---|---|---|
| `invoices.invoice.create` | FACHLOGIK | `invoices.rs:457` `invoices_invoice_create_stub` | Logik | `handlers/invoices.invoice.create.js` | portiert |
| `invoices.invoice.update` | FACHLOGIK | `:486` `invoices_invoice_update_stub` | Logik | `handlers/invoices.invoice.update.js` | portiert |
| `invoices.invoice.delete` | FACHLOGIK | `:523` `invoices_invoice_delete_stub` | Logik | `handlers/invoices.invoice.delete.js` | portiert |
| `invoices.invoice.post` | FACHLOGIK | `:556` → `:569` `post_invoice_in_conn`, `:797` Nummernkreis, `:859` Buchungssatz | Logik | `handlers/invoices.invoice.post.js` | portiert |
| `invoices.invoice.cancel` | FACHLOGIK | `:1210` `invoices_invoice_cancel`, `:1103` `build_credit_note_draft` | Logik | `handlers/invoices.invoice.cancel.js` | portiert |
| `invoices.invoice.create_credit_note` | FACHLOGIK | `:1284` | Logik | `handlers/invoices.invoice.create_credit_note.js` | portiert |
| `invoices.invoice.assign_payment_terms` | FACHLOGIK | `:1378` | **Stub (bail)** | `handlers/invoices.invoice.assign_payment_terms.js` | Stub treu |
| `invoices.line.create` | FACHLOGIK | `:1423` | Logik | `handlers/invoices.line.create.js` | portiert |
| `invoices.line.update` | FACHLOGIK | `:1442` | Logik | `handlers/invoices.line.update.js` | portiert |
| `invoices.line.delete` | FACHLOGIK | `:1473` | Logik | `handlers/invoices.line.delete.js` | portiert |
| `invoices.payment.allocate` | FACHLOGIK | `:1494` `invoices_payment_allocate_stub` | Logik | `handlers/invoices.payment.allocate.js` | portiert |
| `invoices.payment.unallocate` | FACHLOGIK | `:1607` `invoices_payment_unallocate_stub` | Logik | `handlers/invoices.payment.unallocate.js` | portiert |
| `invoices.payment.match_suggestions` | FACHLOGIK | `:1667` | **Stub (leere Liste)** | `handlers/invoices.payment.match_suggestions.js` | Stub treu |
| `invoices.dunning.run` | FACHLOGIK | `:1677` `invoices_dunning_run_stub`, `:1791` + SQL `:1813` | Logik | `handlers/invoices.dunning.run.js` | portiert |
| `invoices.dunning.letter.send` | GEMISCHT (Ziel) | `:1750` `invoices_dunning_letter_send_stub` | **halber Stub** (kein Versand) | `handlers/invoices.dunning.letter.send.js` | treu, ohne Versand |
| `invoices.recurring.create` | FACHLOGIK | `:1826` | **Stub (bail)** | `handlers/invoices.recurring.create.js` | Stub treu |
| `invoices.recurring.update` | FACHLOGIK | `:1834` | **Stub (bail)** | `handlers/invoices.recurring.update.js` | Stub treu |
| `invoices.recurring.run` | FACHLOGIK | `:1838` | **Stub (bail)** | `handlers/invoices.recurring.run.js` | Stub treu |
| `invoices.recurring.pause` | FACHLOGIK | `:1845` | **Stub (bail)** | `handlers/invoices.recurring.pause.js` | Stub treu |
| `invoices.import.from_outbound` | FACHLOGIK | `:1849` | **Stub (bail)** | `handlers/invoices.import.from_outbound.js` | Stub treu |
| `invoices.proposal.create` | FACHLOGIK | `:1857` | **Stub (bail)** | `handlers/invoices.proposal.create.js` | Stub treu |
| `invoices.proposal.approve` | FACHLOGIK | `:1864` | **Stub (bail)** | `handlers/invoices.proposal.approve.js` | Stub treu |
| `invoices.proposal.reject` | FACHLOGIK | `:1868` | **Stub (bail)** | `handlers/invoices.proposal.reject.js` | Stub treu |

Zählung: 23 Typen = 13 mit Logik portiert + 10 Stubs treu als Stubs. Offen über `ctx.cap.legacy.<command_type>`: **0**.
Nicht portiert, weil kein Befehl: `invoices_idempotency_guard :116` (leer), `invoice_root_from_command :120` (tot),
`open_business_os_store :1876` (Speicherzugang).

## Fremd-Collections (Manifest `foreign_collections`, §16.13)

Geschrieben werden Collections der App **buchhaltung**: `accounting_journal_entries`, `accounting_journal_entry_lines`,
`accounting_number_series` (`ops:['read','write']`, buchhaltung muss sie per `exports` freigeben). Eigene
Collections: `accounting_invoices`, `accounting_payment_allocations`, `accounting_dunning_runs`,
`accounting_dunning_letters`.

## Capability-Wünsche

Keine für den heutigen Funktionsumfang (Rust ruft keine native Fähigkeit). Für die spätere Ausbaustufe von
`invoices.dunning.letter.send` (nicht gebaut, weil Rust es nicht tut):

| Capability | Signatur | Beispielaufruf |
|---|---|---|
| `ctx.cap.office.render` | `({model\|blob_id, to:'pdf'}) → {blob_id, sha256}` | `await ctx.cap.office.render({model: letterModel, to:'pdf'})` |
| `ctx.cap.mail.send` | `({account_id, to[], subject, html\|text, attachments[], idempotency_key}) → {message_id, status}` | `await ctx.cap.mail.send({to:[party.email], subject, text, attachments:[{blob_id}], idempotency_key: command.id})` |

Laufzeitwunsch: **`ctx.read` ohne 200er-Kappe** (Memory „Business-OS-Abfrage kappt bei 200“). `dunning.run` und die
kumulative Gutschrift-Obergrenze müssen ALLE Treffer sehen; Rust las per SQL ohne Grenze.

## Abweichungen vom Rust-Verhalten

1. **Speicher**: Rust las/schrieb nur die SQL-Tabelle `business_records`; v4 `ctx.get/put`. `deleted = 1` →
   `ctx.remove` (nach dem Soft-Delete-Schreiben, wie Rust). Die Speicherfelder `_rev` (`rev_<id>_<ms>`) und `_deleted`
   werden nicht mehr ins Dokument geschrieben – Revision und Löschung führt der Core.
2. **Atomarität**: `SAVEPOINT post_invoice` → `ctx.transaction`. Zusätzlich laufen delete, cancel, create_credit_note,
   line.*, payment.*, dunning.* in je einer Transaktion (Rust: Einzelschreibvorgänge). Im Erfolgsfall identisch;
   bei Abbruch bleibt nichts halb stehen. Storno = eine Transaktion über Gutschrift + Buchung + Original.
3. **Modulprüfung** `command.module == "invoices"` → `command.app_id === 'invoices'` (`forbidden`).
4. **Akteur** für `dunning.run.run_by`: `client_context.actor.id` → `command.actor.id`, Rückfall `system`.
5. **Zeit**: Rust nahm `now_ms()` (Systemuhr), auch als Ersatz für fehlendes `invoice_date_ms`; der Port nimmt
   `ctx.now()`.
6. **Rundung**: Rust `f64::round` (Hälfte weg von Null) als `roundAway`; `{:.4}`-Schlüssel der Steuersätze als
   `toFixed(4)`, sortiert wie `BTreeMap`. `iso_year_from_ms`/`iso_date_from_ms` = UTC-Kalender (`Date`).
7. **Zahlentypen**: serde `as_i64` lehnt `5.0` ab, JS kann `5.0` und `5` nicht unterscheiden – ganzzahlige Floats
   gelten im Port als Ganzzahl.
8. **Fälligkeitsabfrage**: SQL `json_extract`/`CAST AS INTEGER`/`COALESCE(…, i64::MAX)` nachgebildet
   (`sqliteCastInt`: Text liefert sein Ganzzahl-Präfix, z. B. `"1000"` → 1000); Vorselektion per Mango `{state}`.
9. **Fehlercodes**: Pflichtfeld/Validierung/unausgeglichen → `invalid`; nicht gefunden → `not_found`;
   falscher Zustand/Überzahlung/Gutschrift-Obergrenze → `conflict`; echte Stubs → `unavailable`. Meldungstexte wörtlich.

## Befunde

- **I1 update setzt alles**: `invoices.invoice.update` mischt jedes Payload-Feld außer `invoice_id`/`id` ein – auch
  `state`, `invoice_number`, `total_cents`, `paid_cents`. Ein Client kann so einen Entwurf auf `posted` setzen, ohne
  Buchung und Nummer. Treu übernommen; Kandidat für eine Feld-Allowlist.
- **I2 currency fest EUR**: `invoices_object_from_command` überschreibt `currency` immer mit `EUR` (Test belegt).
- **I3 Steuersätze außer 19 %/7 %**: bekommen keine Steuerzeile, ihre Steuer steckt aber im Personenkonto → Buchung
  unausgeglichen → `post` scheitert (Test „mix“). Ein Satz allein (z. B. nur 16 %) bucht netto ohne Steuer, obwohl
  `tax_cents` die Steuer zeigt.
- **I4 Schema `accounting_dunning_runs.invoices_total`**: Rust schreibt ein ID-Array, Schema sagt `number` → unter
  v4-Schemaprüfung scheitert jeder Mahnlauf (`schema-befund.test.mjs`).
- **I5 Schema `accounting_payment_allocations`**: `note: null` (Schema `string`) und `payment_id: null` ohne Zahlung
  (Pflichtfeld `string`) → jede Zuordnung scheitert unter v4-Schemaprüfung.
- **I6 Schema `accounting_dunning_letters`**: `pdf_attachment_id`, `sent_via`, `sent_at_ms` = `null` (Schema
  `string`/`number`).
- **I7 UI-Filter wirkungslos**: Die UI startet einen Mahnlauf mit `filter:{invoice_id}` (`index.js:1477`); Rust (und
  der Port) werten nur `state` und `min_open_cents` aus → der Lauf mahnt ALLE überfälligen Rechnungen.
- **I8 leere payment_id**: Die UI sendet `payment_id: ''` (`index.js:1447`); `''` ist ein String, also wird die
  Zuordnungs-ID `alloc__<invoice>` – die zweite Zahlung ohne Kennung auf dieselbe Rechnung gilt als idempotente
  Wiederholung und wird still verworfen.
- **I9 Storno-Datum**: Die Storno-Gutschrift bekommt `invoice_date_ms = jetzt`, ihre Nummer also das aktuelle Jahr
  (z. B. `GS-2025-0001` zu `RE-2023-0001`). Rust-Verhalten, treu.

Für die Tests der betroffenen Pfade (allocate, dunning) läuft die Attrappe deshalb mit `strict:false`; die Befunde
selbst sind in `tests/schema-befund.test.mjs` mit Schemaprüfung belegt.

## Unklarheiten

- `command.record_id` steht nicht im festen Handler-Vertrag; Rust nutzt es als Rückfall für die ID
  (`invoices_record_requirement`). Der Port liest `command.record_id`, falls die Laufzeit es mitliefert; die UI-Umschläge
  setzen es immer (Empfehlung: in den Vertrag aufnehmen).
- Verschachtelte Transaktion: `postInvoice(…, {inTransaction:true})` nutzt im Storno-Pfad die äußere `tx`. Ob die
  Laufzeit verschachtelte `transaction`-Aufrufe erlaubt, ist offen; der Port vermeidet sie.
- Relative Importe innerhalb von `handlers/` (`./_lib/invoices.js`) müssen von der Laufzeit erlaubt werden.
