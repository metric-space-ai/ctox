# Plan: Rechnungsstellung als Business-OS Modul

**Status:** v6 — Skill-konform + echter Shell-Proof nach Fixes (`business-os-app-module-development`)
**Autor:** Mavis
**Datum:** 2026-06-11 (v4 P-Fixes: 2026-06-12, v5 Skill-Refactor: 2026-06-12, v6 Review/Fixes: 2026-06-12)
**Skill:** `src/skills/system/product_engineering/business-os-app-module-development`
**Quelle:** Port von `archive/reorg-review/templates/business-basic/packages/accounting/src/invoice/` und verwandten Workflow-/Payment-/Dunning-Paketen in das aktive Business-OS-Modulformat.

## 0. Few-Shot Map (3 Apps, Skill-Pflicht)

| Existing app | Files read | Shell/data pattern | Collections | Commands/mutations | UI pattern | Reusable decisions | Rejected decisions |
|---|---|---|---|---|---|---|---|
| `customers` | `index.js` (3500 LoC), `schema.js`, `module.json`, `app-store/test.mjs` | `getCollection` Resolver via `state.ctx.db` Proxy; reaktive `find().exec()` pollig (kein `.$`); Mutation via `upsertLocalDoc` Helper; Command-Dispatch via `state.ctx.commandBus.dispatch`; Audit via `customer_activities` | `customer_accounts`, `customer_contacts`, `customer_opportunities`, `customer_tasks`, `customer_notes`, `customer_activities` (alle module-owned) | `customers.account.create/update/archive` (direct CRUD), `customers.contact.*`, `customers.opportunity.move_stage/close_won/lost` (Command → native Handler) | 3-Pane, Filter-Pills, Inspector, Form-Modal, optimistic update via `upsertLocalDoc` | Module-owned records via direct CRUD, cross-module effects via `business_commands` + native Handler, `customer_activities` als Audit-Trail | Kein `collection.$` Subscribe, sondern manueller Re-Render; ich nutze `collection.find().$.subscribe(...)` für echte Reaktivität |
| `buchhaltung` | `index.js` (Layer: render, splits, reconciler, mileage_log, ui_e2e_tests), `schema.js`, `module.json`, `test.js` | Locale-aware via `de.json`/`en.json`; PreSave-Hook für GoBD-Immutability: `if (doc.posted_at) throw`; Reconciliation via `bank_statement_lines` → `journal_entry_lines`; DATEV-Export via `exporters/datev.js`; native Haken direkt im UI (`buchhaltung/index.js:334`) | `accounting_accounts`, `accounting_journal_entries`, `accounting_journal_entry_lines`, `accounting_ledger_entries`, `accounting_receipts`, `accounting_bank_statements`, `accounting_bank_statement_lines` (alle module-owned) | keine business_commands für Standard-CRUD; Reconciliation läuft direkt im Browser | Doppel-Buchführungs-Form, 3-Pane (Kontoplan/Journal/Belege), Drag-Drop für Belege, Reconciler-Wizard | Doppelte Buchführung in `core/splits.js` als pure Funktion, Locale-Map für SKR03/04 Konten, GoBD-Immutability via posted_at | `buchhaltung/index.js:334` macht GoBD-PreSave-Hooks nur **browserseitig** — das ist die Lücke, die P2 in v4 schließen wollte; wir machen es **nativ** im invoices Handler |
| `shiftflow` | `index.js`, `schema.js`, `module.json`, `test.js` | Reactive read via `find().exec()` + manueller Re-Render auf Action-Result; `ctx.eventBus.emit('shiftflow:refresh')` für Cross-Module-Sync; `state.refresh()` zentral | `planning_employees`, `planning_projects`, `planning_shifts`, `planning_time_records`, `planning_absences` (alle module-owned) | CRUD direct + `business_commands` für Manager-Approval-Workflow (kein native Handler — `customers`-Pattern mit Browser-Validation) | Kalender-Grid, Drag-to-Edit, Inspector, Timeline | `eventBus.emit` als Cross-Module-Sync-Mechanik, Mutation-Ergebnis triggert `eventBus.emit('invoices:refresh')` für andere Module | `shiftflow` hat keinen Native-Handler und läuft Audit/Business-Logic im Browser — das wollen wir für Invoices explizit **nicht** kopieren, sondern Native-Handler-Pattern folgen |

**Aussagen aus der Map:**

- **Wir übernehmen:** `getCollection`-Resolver-Pattern, `customer_activities` als Audit-Trail-Spiegel, `eventBus.emit('invoices:refresh')` für Cross-Module-Sync, 3-Pane-Layout mit Inspector, locale-aware UI, **reactive `find().$.subscribe(...)`** (skill-konform, neu gegenüber customers/shiftflow die pollen).
- **Wir übernehmen nicht:** browser-seitige GoBD-PreSave-Hooks (machen wir nativ in `invoices.rs`).
- **Bereits konform (v5 verifiziert):** `collections.schema.json` mit `schema_format: ctox-business-os-module-collections-v1` existiert in `invoices/collections.schema.json` (1465 LoC, 22 Collections, `business_commands` v1-Migration) — `assert-module-conformance.mjs` ist grün. **v5-Schwerpunkt verlagert sich** auf den verbleibenden Skill-Hard-Stop "module reads data once but does not subscribe to collection.$ changes and clean up subscriptions": `index.js` pollt aktuell nur via `find().exec()` und rendert nach Action, ohne reaktive Subscription und ohne `unmount`-Cleanup. Reference-Pattern ist `customers/index.js:1037` `wireRealtime()`.

## 0.1 v4 P-Fixes (User-Review 2026-06-12)

| P | Finding | Fix | Status |
|---|---|---|---|
| P0 | `index.js` erwartete `ctx.collections`/`ctx.on`; echter Shell liefert `ctx.db.collections` + `ctx.eventBus.on` | `index.js` nutzt `ctx.db.collections` + `ctx.eventBus.on('invoices:refresh' \| 'customers.account.updated')`; Dependency-Blocker rendert App-Store-Link wenn `buchhaltung`/`customers` fehlen | done |
| P1 | Journal schrieb `debit_cents`/`credit_cents`, Schema hat `debit`/`credit` | Line-Felder auf `debit`/`credit` umgestellt (Rust + JS); `total_debit_cents`/`total_credit_cents` als Header beibehalten (DATEV-Convention) | done |
| P1 | `quantity` um Faktor 1000 zu hoch | Konvention als Tausendstel dokumentiert (XRechnung/UBL); `computeLineTotals` teilt durch 1000; Rust-Poster analog; alle Test-Werte angepasst | done |
| P1 | Stub-Handler returnten `Ok({stub: true})` mit Status `completed` | Unimplemented-Handler (`cancel`, `credit_note`, `assign_payment_terms`, `line_*`, `payment_match_suggestions`, `recurring_*`, `import_from_outbound`, `proposal_*`) bailen jetzt mit klarer Fehlermeldung; nur die tatsächlich implementierten Handler (create/update/delete/post/allocate/unallocate/dunning_run/dunning_letter_send) geben `Ok` zurück | done |
| P1 | Unbekannte `invoices.*` Commands fielen in generische Queue | Dispatch-Arm in `store.rs` fängt `module == "invoices" && command_type.starts_with("invoices.")` und failt hart (`status: failed`); Test `accept_rxdb_business_command_falls_through_to_record_for_unknown_command` umbenannt zu `..._rejects_unknown_invoices_command` und auf `Err` + `status: failed` umgestellt | done |
| P2 | GoBD-Lock nur in Browser-Hooks | Direkte `accounting_*` Writes gehen über native Handler; `record_command` schreibt nur in `business_commands` Audit-Log; Defense-in-Depth dokumentiert; Update-Handler prüft `state == "draft"`, Post-Handler setzt `posted_at` | done |
| P2 | UI war nur Liste + Create/Update/Delete | UI hat jetzt: Filter (Alle/Überfällig/Offen/Entwürfe), echten Line-Editor mit Live-Total, Post-Button mit GoBD-Update, Payment-Match-Tab, Dunning-Wizard-Tab, Journal-Tab, XRechnung-Tab mit Download, Inspector mit Party + Offene Posten | done |
| P3 | Plan-Tracking hatte duplicate `todo`-Zeilen | Duplikate (Zeilen 692-700) entfernt; Header auf v4 gehoben | done |

## 0.2 v6 Review-Fixes (2026-06-12)

| P | Finding | Fix | Status |
|---|---|---|---|
| P0 | Echter Shell-Start lud `invoices/index.js`, aber `#invoices-root` existierte nicht im Workspace, weil `mount(ctx)` gegen `document.getElementById('invoices-root')` rendete statt das Modulfragment in `ctx.host` zu laden | `mount(ctx)` ruft `ensureMountedMarkup(ctx)` auf, lädt `index.html` in `ctx.host` und alle Root-Zugriffe laufen über `moduleRoot()` (`ctx.host` zuerst, direkter Fallback nur für Standalone/Tests) | done |
| P1 | Browser-Editor-Totals rechneten `quantity` nicht als Tausendstel. `quantity: 1000` wurde in der UI als 1000 Einheiten statt 1.000 Einheit bewertet | `computeInvoiceTotals()` nutzt `computeLineNetCents()` mit `/1000` und Rabattlogik wie `core/invoice-tax.js`/Rust; neuer Test `ui-totals.test.mjs` schützt den Pfad | done |
| P2 | Plan enthielt noch statische Schema-Contract-Anweisungen für reine Modul-Collections | Plan auf dynamische `collections.schema.json`-Registrierung aktualisiert; Core-RxDB/Hash-Regeneration nur bei Shared/Core-Contract-Änderungen | done |

---

## 0. Kontext

Im aktiven Tree (`src/apps/business-os/modules/`) existiert ein vollständiges FIBU-Modul `buchhaltung` mit:

- Kontenplan (SKR03/04) in `templates/skr.js`
- Doppelte Buchführung in `core/` (Splits, Ledger, Reconciler)
- Bankparser Camt.053 / MT940 in `parsers/`
- DATEV-EXTF-Export in `exporters/`
- ELSTER + HGB-Reports in `reports/`

**Was fehlt:** die **Debitoren-/Kreditoren-Schicht** obendrauf. Heute werden Rechnungen entweder manuell im Journal gebucht oder gar nicht erfasst — es gibt keine Invoice-Entity, keinen Post-Lifecycle, kein Mahnwesen, keine Gutschriften, keinen Skonto, keinen PDF-Renderer, keinen Offene-Posten-Report.

**Im Archiv** (`archive/reorg-review/templates/business-basic/`) liegt eine ausgereifte TypeScript-Implementierung, die genau diese Schicht abdeckt:

- `packages/accounting/src/invoice/{types,validate,poster,pdf,debtor-actions,index}.ts`
- `packages/accounting/src/payment/`, `dunning/`, `reports/`, `workflow/`
- API-Surface unter `apps/web/app/api/business/{invoices,accounting}/`

**Ziel dieses Plans:** Port der Invoice-Domain in ein eigenständiges Business-OS-Modul `invoices`, das auf der bestehenden `buchhaltung`-Fibu aufsetzt, ohne deren Code zu duplizieren.

---

## 1. Designprinzipien

1. **Auf bestehender Fibu aufsetzen, nicht parallel.** Der native Invoice-Handler schreibt seine Buchungen über `accounting_journal_entries` und `accounting_journal_entry_lines` (aus `buchhaltung`). Doppelte Buchführung, GoBD-Immutability, SKR03/04-Kontenplan und DATEV-Export bleiben in `buchhaltung` — alles andere wäre Redundanz.
2. **RxDB-only Replikation.** Keine HTTP-Fallbacks (siehe `docs/ctox-rxdb.md`).
3. **CTOX-Kommandos statt eigener Mutationspfad.** Alle Mutationen laufen über `business_commands` mit `module: 'invoices'`, Command-Typen `invoices.invoice.create`, `invoices.invoice.post` etc. CTOX kann daraus Proposals generieren, Audit-Trail führen und via WebRTC replizieren.
4. **Locale-aware von Anfang an.** i18n in `de.json`/`en.json` (steuerliche Texte §14 UStG, Mahntexte, E-Rechnung).
5. **GoBD-konform ab Tag 1.** Post = immutable. Storno über Gegenbuchung mit Verweis. Belegnummern aus `number-series` mit Lücken-Detection.
6. **PDF und XRechnung.** Direkt im Browser gerendert (kein Server-Print), Embedding in `desktop_files` für GoBD-Archiv.
7. **AI-Proposals.** Wo immer möglich, schlägt das Modul Buchungen zur Approval vor (analog `customers.opportunity.move_stage`-Pattern), statt direkt zu posten.
8. **Native-first Datenpfad.** Neue Collections sind erst "real", wenn `collections.schema.json`, `module.json.collections`, Browser-Shell und nativer RxDB-Peer dieselbe Runtime-Schema-Quelle nutzen. Mutierende Commands sind erst "real", wenn `src/core/business_os`-Handler sie validieren, ausfuehren und completed/failed projizieren. Ein `schema.js` im Modul allein reicht nicht.

### 1.1 Harte Umsetzungsregeln

**Do:**

- Vor Codearbeit `CLAUDE.md`, `docs/architecture.md`, `docs/ctox-rxdb.md`, `src/core/rxdb/AGENTS.md`, `src/apps/business-os/rxdb/AGENTS.md` und `src/core/business_os/AGENTS.md` lesen.
- Alle neuen oder wiederverwendeten Business-OS Collections im Modul-eigenen `collections.schema.json` deklarieren (`schema_format: ctox-business-os-module-collections-v1`) und dieselben Namen in `module.json.collections` listen. Browser-Shell und nativer RxDB-Peer registrieren diese Modul-Collections zur Laufzeit. `schema.js` ist nur Kompatibilitäts-/Generator-Fassade.
- Den statischen Core-Schema-Contract (`src/core/business_os/business_os_schema_contract.json`, Hash-Registry, `src/apps/business-os/rxdb/src/schema.mjs`, `dist` rebuild) **nur** anfassen, wenn Core-/Shared-RxDB-Collections oder die RxDB-Runtime selbst geändert werden. Für reine Modul-Collections ist das ein Anti-Pattern.
- Fuer jedes mutierende `invoices.*` Command einen nativen Handler in `src/core/business_os/store.rs` oder einem sauber ausgelagerten `src/core/business_os/invoices*.rs` einbauen und ueber `accept_rxdb_business_command` erreichbar machen.
- Command-Dokumente kanonisch mit `command_type` behandeln. UI-Builder duerfen `type` nur als `commandBus`-Alias nutzen; Tests muessen die persistierte `command_type`-Form abdecken.
- Buchungen, Invoice-State, Zahlungen, Nummernvergabe und Archiv-Verweise idempotent machen. Ein mehrfach repliziertes Command darf keine zweite Rechnungsnummer, zweite Journalbuchung oder zweite Payment-Allocation erzeugen.
- `posted_at` ist das bestehende `buchhaltung`-Lock-Feld, nicht `posted_at_ms`. Native Handler muessen GoBD-Unveraenderbarkeit selbst erzwingen, weil Browser-Hooks nur laufen, wenn das `buchhaltung`-Modul gemountet ist.
- Manifest-`collections` muss alle Collections enthalten, die das Modul liest oder schreibt: eigene Invoice-Collections, `business_commands`, relevante `accounting_*`, `customer_accounts`, `customer_activities`, `desktop_files`, `desktop_file_chunks`.
- Nach jeder Datenpfad-Aenderung mindestens `node src/apps/business-os/rxdb/tests/run-all.mjs` und `cargo test --manifest-path src/core/rxdb/Cargo.toml` laufen lassen. Nach `src/core/business_os/*`-Aenderungen zusaetzlich `cargo check` und relevante `cargo test --bin ctox ...`.

**Don't:**

- Keine HTTP-Route, keinen HTTP-Fallback und keinen MCP-/REST-Shortcut fuer Rechnungen, Journalbuchungen, Dateien oder Commands bauen. Business-OS Daten bleiben RxDB/WebRTC-only.
- Nicht `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs` direkt patchen. Wenn der RxDB-Runtime-Source geaendert wird: Source aendern, pinned esbuild aus `docs/ctox-rxdb.md` laufen lassen, alle drei `?v=` Cache-Buster identisch bumpen.
- Keine neuen npm/bare/`node:` Imports in Browser-Runtime oder Modul-Code einfuehren, der im Browser ohne Bundler laufen muss. Vendoring ist nur mit expliziter Pruefung erlaubt.
- Keine Prozess-Env-Toggles fuer Invoice-, Accounting-, PDF- oder Sync-Verhalten einfuehren.
- Nicht annehmen, dass eine `schema.js`-Änderung reicht. Runtime-relevant ist `collections.schema.json`; `module.json.collections` muss dazu passen.
- Nicht direkt aus der UI in `accounting_invoices`, `accounting_journal_entries`, `accounting_journal_entry_lines`, `customer_activities`, `desktop_files` oder `desktop_file_chunks` schreiben, ausser fuer rein lokale Vorab-Preview-Daten ohne Persistenzwirkung. Persistente Mutationen laufen ueber `ctx.commandBus.dispatch(...)` in `business_commands` und native Handler.
- Nicht weiterbauen, wenn ein Guard-Test rot ist. Der Guard ist richtig; die Aenderung muss angepasst werden.

---

## 2. Modul-Struktur

```
src/apps/business-os/modules/invoices/
├── module.json                    # Manifest: id, collections, layout, install_scope
├── index.html                     # Drei-Pane-Shell
├── index.js                       # mount(ctx) + Render + State + Actions (~ 3000–4000 LoC)
├── index.css                      # Lokale Styles
├── schema.js                      # RxDB-Schemas fuer 11 neue Invoice-Collections
├── icon.svg
├── locales/
│   ├── de.json
│   └── en.json
├── core/                          # Domain-Logik, portiert aus business-basic
│   ├── invoice-types.js           # aus invoice/types.ts
│   ├── invoice-validate.js        # aus invoice/validate.ts
│   ├── invoice-poster.js          # aus invoice/poster.ts (Buchungserzeugung)
│   ├── invoice-actions.js         # aus invoice/debtor-actions.ts (Storno, Gutschrift, Skonto)
│   ├── invoice-pdf.js             # aus invoice/pdf.ts (Renderer)
│   ├── invoice-numbering.js       # aus accounting/number-series.ts (Belegnummern)
│   ├── invoice-payments.js        # aus accounting/payment/
│   ├── invoice-dunning.js         # aus accounting/dunning/
│   ├── invoice-tax.js             # USt-Berechnung, Reverse-Charge, §13b
│   ├── invoice-recurring.js       # Abo-Rechnungen
│   └── invoice-archive.js         # GoBD-Archivierung in desktop_files
├── commands/                      # business_commands-Builder, analog customers
│   └── builders.js                # buildCreateInvoiceCommand, buildPostCommand, …
├── views/                         # UI-Bausteine (Plain-DOM, kein Framework)
│   ├── list.js                    # Listen-Pane (links)
│   ├── editor.js                  # Editor (mitte) — neue Rechnung, Bearbeitung
│   ├── detail.js                  # Detail (mitte) — gepostete Rechnung
│   ├── inspector.js               # Kunde, Posten, Steuer (rechts)
│   ├── payment-match.js           # Zuordnung Bankeingang → Rechnung
│   ├── dunning-wizard.js          # Mahnlauf-Konfigurator
│   ├── pdf-preview.js             # PDF-Vorschau
│   └── approval-card.js           # AI-Proposal-Approval-UI
├── templates/                     # PDF- & XRechnung-Templates
│   ├── invoice-de.html            # Standard DE-Rechnung
│   ├── invoice-eu.html            # EU-Innergemeinschaftlich
│   ├── credit-note.html           # Gutschrift §17 UStG
│   └── xrechnung.xml.js           # XRechnung-Generator
├── tests/
│   ├── invoice-types.test.mjs
│   ├── invoice-poster.test.mjs
│   ├── invoice-actions.test.mjs
│   ├── invoice-pdf.test.mjs
│   ├── invoice-dunning.test.mjs
│   ├── module-e2e.test.mjs
│   └── fixtures/
│       ├── skr03.json
│       ├── skr04.json
│       └── parties.json
└── README.md
```

**Nicht im Modul-Verzeichnis, aber Teil des Ports, sobald native Logik gebraucht wird:**

- `src/core/business_os/store.rs` oder ein dort angebundenes `src/core/business_os/invoices.rs` muss die mutierenden `invoices.*` Commands nativ ausfuehren.
- Tests fuer Native-Handler und Datenpfad gehoeren nicht nur unter `modules/invoices/tests/`, sondern auch in die passenden Rust-/RxDB-Suites.
- Der statische Core-Schema-Contract (`src/core/rxdb/tools/build_business_os_schema_contract.mjs`, `src/core/business_os/business_os_schema_contract.json`, `src/core/business_os/business_os_schema_hashes.json`, `src/apps/business-os/rxdb/src/schema.mjs`) ist **nicht** der normale Pfad fuer Modul-Collections. Er wird nur angepasst, wenn Core-/Shared-Collections oder die RxDB-Runtime selbst geaendert werden.
- Falls `src/apps/business-os/rxdb/src/*` geaendert wird: nie `dist` direkt patchen, sondern Source aendern, pinned esbuild aus `docs/ctox-rxdb.md` laufen lassen und Cache-Buster konsistent bumpen.

**Größen-Schätzung** (basierend auf `customers` als Referenz: 3951 LoC JS, 467 LoC schema.js):
- `schema.js`: ~700 LoC (11 Collections, ~60 Properties im Schnitt)
- `index.js`: ~3500 LoC
- `core/*.js`: ~1500 LoC (portiert 1:1, nur TypeScript→JS)
- `views/*.js`: ~2000 LoC
- `tests/`: ~800 LoC
- Native Command-/Projection-Code: zusaetzlich einplanen; nicht in den Modul-LoC verstecken
- **Gesamt: ~8500 LoC Browser/Modul + native Handler/Tests** über ~30+ Dateien

---

## 3. Collections (Schema-Plan)

Alle Collections folgen dem `customers`-Muster (Cent-Integer statt Float, `*_ms`-Zeitstempel, `is_deleted` für Soft-Delete, `search_text` für Volltext).

### 3.0 Data-Plane Vorbedingung

Der aktive Browser und der native RxDB-Peer registrieren Modul-Collections zur Laufzeit aus `collections.schema.json`.

1. `src/apps/business-os/modules/invoices/collections.schema.json` ist der Runtime-Vertrag fuer die 22 gelisteten Collections. Pflichtfelder: top-level `schema_format: "ctox-business-os-module-collections-v1"` und `collections`.
2. `src/apps/business-os/modules/invoices/module.json.collections` muss exakt die Collections listen, die das Modul liest/schreibt/synchronisiert. `assert-module-conformance.mjs` prueft diese Uebereinstimmung.
3. `schema.js` darf fuer Kompatibilitaet, Tests und Generatoren bestehen bleiben, ist aber nicht die Quelle, auf die der native Peer zur Laufzeit vertraut.
4. Der native Peer scannt Modulverzeichnisse (`src/apps/business-os/modules/*/collections.schema.json` und `runtime/business-os/installed-modules/*/collections.schema.json`) und baut daraus die runtime-faehigen Collection-Strukturen. Fuer reine Modul-Collections ist keine Rust-Schema-Codeaenderung und keine Hash-Fixture-Regeneration noetig.
5. Mutierende Fachlogik bleibt trotzdem native-first: `invoices.*` Commands gehen ueber `ctx.commandBus.dispatch(...)` in `business_commands`; `src/core/business_os/invoices.rs` validiert und schreibt die betroffenen Collections.
6. Es gibt kein App-API `ctox.db`. Modulcode nutzt `ctx.db.collection(name)`, `ctx.db[name]` oder `ctx.db.collections[name]` ueber den Live-DB-Facade. `ctx.db.raw` ist fuer Module verboten.

### 3.1 Bestehend / Dependencies (wiederverwenden, im Manifest deklarieren)

| Collection | Verwendung |
|---|---|
| `accounting_accounts` | Forderungen, Erlöse, USt, Skonto, Verzug — SKR03/04-Konten |
| `accounting_journal_entries` | Gebuchte Rechnungen (Header) |
| `accounting_journal_entry_lines` | Soll/Haben-Zeilen (Debitor, Erlös, USt, ggf. Skonto) |
| `accounting_ledger_entries` | Reports/Hauptbuch-Projektion, falls `buchhaltung` sie aus Journalbuchungen ableitet |
| `accounting_receipts` | Eingangsrechnungen / Ausgangsrechnungen als Belege (Verknüpfung) |
| `accounting_bank_statement_lines` | Payment-Matching gegen Bankimporte |
| `customer_accounts` | Party-Master fuer Debitor/Kreditor, read-only |
| `customer_activities` | Timeline-Events aus nativen Invoice-Handlern |
| `desktop_files` | PDF/XRechnung/Mahnschreiben-Dateiindex |
| `desktop_file_chunks` | Datei-Chunks fuer archivierte Artefakte |
| `business_commands` | Mutationen + AI-Proposals |

### 3.2 Neu (im `invoices` Modul, 11 Collections)

```text
accounting_invoices
  - id (string PK)
  - invoice_number (string, fortlaufend, eindeutig)
  - invoice_type (enum: 'sale_out' | 'sale_in' | 'credit_note_out' | 'credit_note_in' | 'recurring_template')
  - party_id (string, FK → customer_accounts.id)
  - party_snapshot (object, eingefrorener Kundenstamm zum Post-Zeitpunkt)
  - invoice_date_ms, due_date_ms, service_period_start_ms, service_period_end_ms
  - currency (string, default 'EUR')
  - subtotal_cents, tax_cents, total_cents, paid_cents, open_cents
  - tax_breakdown (object[], {tax_rate, net_cents, tax_cents, tax_account_code})
  - payment_terms_id (FK → accounting_payment_terms.id)
  - skonto_percent, skonto_days
  - state (enum: 'draft' | 'posted' | 'partially_paid' | 'paid' | 'overdue' | 'cancelled' | 'credited')
  - state_changed_at_ms, state_changed_by_command_id
  - linked_invoice_id (string, für Gutschriften → Original)
  - reverse_charge (boolean)
  - small_business (boolean, §19 UStG)
  - eu_ic_supply (boolean, Innergemeinschaftliche Lieferung)
  - xrechnung_xml (string|null, eingebettet nach Post)
  - pdf_attachment_id (FK → desktop_files.id, GoBD-Snapshot)
  - post_journal_entry_id (FK → accounting_journal_entries.id)
  - cancel_journal_entry_id (FK, Storno-Gegenbuchung)
  - credit_note_for_id (FK, bei Gutschrift)
  - dunning_level (int, 0–3)
  - last_dunning_run_id (FK)
  - created_at_ms, updated_at_ms, is_deleted, deleted_at_ms
  - search_text (string)
  - payload (object, free-form)

accounting_invoice_lines
  - id (string PK)
  - invoice_id (FK)
  - position (int)
  - description, article_number
  - quantity (number, in Tausendstel für 3 Nachkommastellen)
  - unit (string, 'Stk' | 'h' | 'kg' | 'm' | …)
  - unit_price_cents
  - discount_percent
  - tax_rate (number, 0.19 für 19 %, 0.07 für 7 %)
  - line_net_cents, line_tax_cents, line_gross_cents
  - account_code (FK → accounting_accounts.code, default '8400' Erlöse)
  - cost_center_id (string|null)
  - project_id (string|null, künftig)
  - service_period_start_ms, service_period_end_ms

accounting_payment_terms
  - id, name, net_days, skonto_percent, skonto_days, description, is_default

accounting_number_series
  - id (string PK)
  - series_key (string, z. B. 'invoice_sale_out')
  - fiscal_year (int)
  - prefix (string, z. B. 'RE-2026-')
  - next_value (int)
  - last_issued_number (string)
  - gap_policy (enum: 'strict_no_gaps' | 'reserved_then_voided')
  - updated_at_ms, updated_by_command_id
  - payload (object)

accounting_credit_notes
  - id, invoice_id (FK → Original), credit_note_invoice_id (FK → Gutschrift),
    reason, reason_text, delta_net_cents, delta_tax_cents, delta_gross_cents,
    corrective_invoice_number, created_at_ms

accounting_payments
  - id, payment_date_ms, party_id, amount_cents, currency, method
    ('bank_transfer' | 'sepa_direct_debit' | 'cash' | 'card' | 'other'),
    reference, bank_statement_line_id (FK), payload

accounting_payment_allocations
  - id, payment_id (FK), invoice_id (FK), allocated_cents,
    skonto_cents (optional), note, allocated_at_ms

accounting_dunning_runs
  - id, run_date_ms, run_by, filter (object, z. B. {state: 'overdue', min_open_cents: 10000}),
    invoices_total, letters_sent, payload

accounting_dunning_letters
  - id, dunning_run_id (FK), invoice_id (FK), level (1|2|3),
    letter_date_ms, fee_cents, interest_cents, total_cents,
    pdf_attachment_id, sent_via, sent_at_ms, status

accounting_recurring_invoices
  - id, template_invoice_id (FK), interval ('monthly' | 'quarterly' | 'yearly'),
    interval_count, start_at_ms, end_at_ms|null, next_run_at_ms, last_run_at_ms,
    auto_send (bool), active, payload

accounting_invoice_attachments
  - id, invoice_id, kind ('pdf' | 'xrechnung' | 'correction' | 'other'),
    desktop_file_id (FK), sha256, size_bytes, created_at_ms
```

### 3.3 Schema-Konventionen (eingehalten wie `customers`)

- `primaryKey: 'id'`, `id: { type: 'string', maxLength: 160 }`
- `version: 0` für neue Collections, `version: 1` wenn Migration nötig
- Indizes auf `party_id`, `state`, `invoice_date_ms`, `due_date_ms`, `invoice_number`, `updated_at_ms`
- Soft-Delete via `is_deleted: boolean` + `deleted_at_ms: number`
- `search_text` Lower-Case-Konkatenation der Volltext-relevanten Felder, für Filter in der Listen-Pane
- `payload: { type: 'object', additionalProperties: true }` für unstrukturierte Erweiterungen
- `client_context` in `business_commands` trägt `{ build, surface }` zur Telemetrie
- `accounting_number_series` ist Pflicht. Rechnungsnummern duerfen nicht aus `count(*)`, UI-State oder lokalem IndexedDB abgeleitet werden.
- Bestehende `buchhaltung`-Felder werden respektiert: `accounting_journal_entries.posted_at` ist das Lock-Feld; nicht `posted_at_ms`.
- Nach jeder Modul-Schema-Aenderung: `collections.schema.json` und `module.json.collections` synchron halten, `assert-module-conformance.mjs` gruen nachweisen und einen echten Shell-Start gegen die dynamisch registrierte Collection-Liste fahren.

---

## 4. Domain-Logik (Portierung business-basic → JS)

Jede Datei in `core/` ist ein 1:1-Port des Originals, mit minimalen Anpassungen:

| Original (`packages/accounting/src/`) | Ziel (`core/`) | Anpassungen |
|---|---|---|
| `invoice/types.ts` | `invoice-types.js` | `interface` → JSDoc-`typedef`; Discriminated-Unions → Funktions-Checks; `Date` → `*_ms` Integer |
| `invoice/validate.ts` | `invoice-validate.js` | Funktions-Signaturen unverändert, nur Type-Stripping |
| `invoice/poster.ts` | `invoice-poster.js` | Pure Berechnungslogik fuer Journalbuchungen; persistenter Schreibzugriff erfolgt im nativen Handler. Pflicht: `posted_at` setzen → GoBD-Lock |
| `invoice/debtor-actions.ts` | `invoice-actions.js` | Storno, Gutschrift, Skonto-Ausübung, Allocation-Logik |
| `invoice/pdf.ts` | `invoice-pdf.js` | pdf-lib (Bundle in `vendor/`, **kein npm-Import** — `docs/ctox-rxdb.md` Hard Rule). Templates in `templates/*.html` |
| `number-series.ts` | `invoice-numbering.js` | Fortlaufende Nummern mit Lücken-Detection; persistente Reservation in `accounting_number_series` im nativen Handler |
| `payment/`, `dunning/`, `reports/`, `tax.ts` | `invoice-payments.js`, `invoice-dunning.js`, in `invoice-tax.js` | Aggregation der FIBU-Funktionen |
| `workflow/{commands,outbox,proposals,policy,audit}.ts` | Aufgeteilt in `core/*` und `commands/builders.js` | Outbox → `business_commands` Outbox-Pattern; Proposals nur mit bestehendem, getesteten Command-Statusmodell |

**Datenfluss-Pattern** (analog `customers`, aber mit neuem nativen Handler):

```
User-Aktion (UI)
    │
    ▼
buildXxxCommand(...)         ← commands/builders.js
    │   erzeugt {module, command_type, payload, client_context}
    ▼
insert business_commands     ← RxDB
    │
    ▼ (WebRTC-Replikation, andere Peers + Server-Side-Worker)
    │
    ▼
native invoices handler      ← validiert, reserviert Nummern, schreibt Invoice/Journal/Activity/File-Refs
    │
    ▼
state transition             ← z. B. draft → posted
    │
    ▼
UI re-rendert via Reactive Subscription auf Collection
```

**Wichtig:** Die UI **mutiert** nie direkt `accounting_invoices`. Sie sendet `business_commands`. Das hält die WebRTC-Replikation konsistent und macht AI-Approval-Flows möglich.

### 4.1 Native Command-Ausfuehrung

Der Port ist erst funktional, wenn `accept_rxdb_business_command` die neuen Command-Typen direkt kennt. Dafuer gibt es zwei zulaessige Wege:

1. Kleine erste Version direkt in `src/core/business_os/store.rs` analog `customers`.
2. Besser bei wachsendem Umfang: `src/core/business_os/invoices.rs` mit klaren public Handlern, in `store.rs` nur `is_invoices_active_command(...)` und Dispatch.

**Do:**

- `is_invoices_active_command(command_type)` mit einer expliziten Allowlist pflegen.
- `handle_invoices_active_command(root, session, command)` implementieren und `command.module == "invoices"` erzwingen.
- In jedem Handler zuerst Idempotenz pruefen: existiert ein Ergebnis fuer `command_id`, `invoice_id`, `payment_id` oder `journal_entry_id`, dann das vorhandene Ergebnis zurueckgeben.
- `invoices.invoice.post` muss atomar wirken: Invoice validieren, Rechnungsnummer reservieren, Journal Entry + Lines schreiben, Invoice auf `posted` setzen, `post_journal_entry_id` setzen, `posted_at` im Journal setzen, Activity schreiben.
- Fehler terminal auf dem Command dokumentieren (`status: "failed"`), statt Commands endlos `pending_sync` oder nur queue-backed liegen zu lassen.

**Don't:**

- Kein `invoices.*` Command darf nur generisch in `ctox_queue_tasks` landen und dann hoffen, dass ein Agent die Daten schreibt.
- Keine Rechnungsnummer im Browser vergeben.
- Keine Journalbuchung direkt aus `index.js`, `views/*` oder `core/*.js` persistieren.
- Keine GoBD-Unveraenderbarkeit nur ueber Browser-`preSave`-Hooks absichern.

---

## 5. UI-Aufbau (3-Pane analog `buchhaltung`)

**Layout-Slots** (im `module.json` unter `layout`):

| Slot | Inhalt |
|---|---|
| **left** | Filter, Status-Chips, Listen-Scope (Eingangs-/Ausgangsrechnungen/Gutschriften/Mahnläufe), Schnellfilter (überfällig, Mahnstufe 1, offene Posten) |
| **center** | Editor (Draft) ODER Detail (Posted) — abhängig vom State. Tabs: Stammdaten, Positionen, Steuern, Zahlungen, PDF, Verlauf, AI-Vorschläge |
| **right** | Kunden-Inspector (read-only aus `customer_accounts`), offene Posten, Mahn-Status, AI-Aktionen (Mahnlauf generieren, Skonto vorschlagen, Gutschrift anbieten) |

**Sub-Views** (zusätzlich zur Hauptansicht):
- **PDF-Vorschau** (modal über center) — gerendert via `pdf-lib` aus dem Template
- **Payment-Match** (modal) — Bankeingang → Rechnung zuordnen (mit Vorschlags-Engine)
- **Dunning-Wizard** (modal) — Filter setzen, Vorschau, Freigabe
- **Gutschrift-Dialog** (modal) — Original auswählen, Korrektur-Positionen, Begründung §17 UStG
- **Approval-Card** (rechts) — AI-Proposal (z. B. "Skonto ausüben spart X EUR") mit Approve/Reject

**Drei Tabs im Detail** (in Anlehnung an `customers`): *Übersicht*, *Positionen*, *Zahlungen* — plus *PDF*, *Verlauf* (Audit-Trail aus `business_commands`), *Korrekturen* (Gutschriften).

---

## 6. Integration mit `buchhaltung` und `customers`

### 6.1 `buchhaltung` (FIBU)

- **Keine FIBU-Duplikation.** Kontenplan, Journal, Ledger, DATEV und Reports bleiben fachlich in `buchhaltung`.
- **Aber:** `buchhaltung`-Collections muessen in `collections.schema.json`/`module.json.collections` deklariert und vom nativen Peer dynamisch gescannt werden. Das ist Datenpfad-Verdrahtung, keine Fachlogik-Duplizierung.
- **Persistente Writes nur nativ.** Das `invoices` Modul sendet Commands; der native invoices Handler schreibt `accounting_journal_entries`/`_lines`.
- **GoBD-Lock wird nativ erzwungen.** `accounting_journal_entries.posted_at` ist das bestehende Lock-Feld. Ein geposteter Journal Entry darf weder per Invoice-Command noch per generischem Business-Record-Pfad veraendert oder geloescht werden.
- **Kontenplan wird gelesen** aus `accounting_accounts`. Forderungskonto (1400 SKR03 / 1200 SKR04), Erlöskonto (8400/4400), USt (3800/3801/3802/3805/3806), Skonto-Aufwand (8730/4730) etc.
- **DATEV-Export** aus `buchhaltung` greift automatisch, weil die Rechnungsbuchungen in den Journalen landen.
- **HGB-Reports** in `buchhaltung/reports/` werden um Offene-Posten ergänzt, aber nur falls nötig — primär im neuen Modul.

### 6.2 `customers`

- **`party_id`** referenziert `customer_accounts.id`. Das `invoices` Modul liest read-only, mutiert `customers` nie.
- **Rechnungsstellung aus Opportunity:** Klick in `customers` Opportunity-Detail "Rechnung erstellen" → `business_commands` mit `module: 'invoices', command_type: 'invoices.invoice.create_from_opportunity', payload: { opportunity_id }`. Native Handler liest Opportunity, erzeugt Invoice-Draft, verknüpft.
- **Aktivitäts-Timeline:** Der native invoices Handler schreibt Events in `customer_activities` (`activity_type: 'invoice_posted'`, `activity_type: 'payment_received'`, `activity_type: 'dunning_sent'`). Die Invoice-UI schreibt diese Collection nicht direkt.

### 6.3 `outbound`

- Wiedervorlage von Opportunities zu "Rechnung wurde bezahlt" als Trigger für `outbound.upsell` — out of scope dieses Plans, aber Hook-Punkt vorgesehen.

### 6.4 Andere Module

- **`documents`**: Verknüpfung möglich, aber PDF-Archivierung erfolgt in `desktop_files`/`desktop_file_chunks` (CTOX-Standard für Dateien, Replikation darüber). Beide Collections muessen im Manifest stehen.
- **`calendar`**: `due_date_ms` ist standardmäßig ein Terminhinweis (konfigurierbar pro Invoice).
- **`notes`**: Rechnungsnotizen als Link auf `notes` möglich, aber nicht zwingend.

---

## 7. Command-Surface (`business_commands`)

Vollständige Liste der Modul-Commands:

```
invoices.invoice.create             # Aus UI oder Opportunity-Hook
invoices.invoice.update             # Nur im Draft-State
invoices.invoice.delete             # Nur im Draft-State
invoices.invoice.post               # Übergang draft → posted, erzeugt Journalbuchung
invoices.invoice.cancel             # Storno-Gegenbuchung, state → cancelled
invoices.invoice.create_credit_note # Erzeugt Gutschrift zu Original
invoices.invoice.assign_payment_terms
invoices.line.create / .update / .delete
invoices.payment.allocate           # Bankeingang → Rechnung zuordnen
invoices.payment.unallocate
invoices.payment.match_suggestions  # AI: mögliche Zuordnungen vorschlagen
invoices.dunning.run                # Mahnlauf anstoßen
invoices.dunning.letter.send        # Einzelner Brief
invoices.recurring.create / .update / .run / .pause
invoices.import.from_outbound       # Kampagnen-Conversion → Rechnung
invoices.proposal.create            # AI schlägt vor (z. B. Gutschrift, Mahnstufe erhöhen)
invoices.proposal.approve
invoices.proposal.reject
```

Jeder Command-Builder in `commands/builders.js` baut ein Objekt mit `module`, `command_type`, `payload`, `record_id`, `client_context`. `commandBus` akzeptiert zwar `type` als Alias, aber die persistierte und native Form ist `command_type`; neue Tests muessen diese Form pruefen. Beispiel:

```js
function buildPostInvoiceCommand(invoiceId, { force = false } = {}) {
  return {
    module: 'invoices',
    command_type: 'invoices.invoice.post',
    record_id: invoiceId,
    payload: { invoice_id: invoiceId, force },
    client_context: { build: BUILD, surface: 'invoices.invoice.post' },
  };
}
```

`force: true` ist nur für Super-User (Admin), umgeht z. B. Mahnpausen. Der native Handler muss diese Rolle pruefen; die UI darf `force` nicht als Autorisierung behandeln.

**Native Handler-Mapping:**

| Command | Native Mindestwirkung |
|---|---|
| `invoices.invoice.create` | Draft in `accounting_invoices`, Lines optional, keine Nummernreservation |
| `invoices.invoice.update` | Nur Drafts, keine geposteten Invoices |
| `invoices.invoice.delete` | Nur Drafts, Soft-Delete |
| `invoices.invoice.post` | Nummer reservieren, Journal Entry/Lines schreiben, `posted_at` setzen, Invoice immutable markieren |
| `invoices.invoice.cancel` | Storno-Gegenbuchung erzeugen, Original nicht veraendern ausser Storno-Verweis/State |
| `invoices.invoice.create_credit_note` | Neue Credit-Note-Invoice mit Link auf Original, keine Original-Ueberschreibung |
| `invoices.payment.allocate` | Allocation idempotent schreiben, `paid_cents/open_cents/state` konsistent neu berechnen |
| `invoices.dunning.run` | Run + Letters erzeugen, keine Briefe als gesendet markieren ohne expliziten Send-Command |
| `invoices.proposal.*` | Proposal-Status aendern, fachliche Mutation nur bei Approve und nur ueber denselben Handler |

**Don't:**

- Keine `pending_approval`-Statuswerte erfinden, wenn sie nicht vom bestehenden `business_commands`-Schema/Consumer verarbeitet werden. Falls benoetigt, erst Command-Statusmodell erweitern und testen.
- Keine Command-Familie in der UI listen, bevor ihr nativer Handler existiert oder sie explizit als "proposal only, no mutation" markiert ist.

---

## 8. Registrierung in `modules/registry.json`

Ein neuer Eintrag in `src/apps/business-os/modules/registry.json` (siehe `customers` als Vorlage):

```json
{
  "id": "invoices",
  "title": "Rechnungen",
  "description": "Ausgangs- und Eingangsrechnungen, Gutschriften, Skonto, Mahnwesen, XRechnung, DATEV-Export über Buchhaltung.",
  "entry": "modules/invoices/index.html",
  "collections": [
    "business_commands",
    "customer_accounts",
    "customer_activities",
    "accounting_accounts",
    "accounting_journal_entries",
    "accounting_journal_entry_lines",
    "accounting_ledger_entries",
    "accounting_receipts",
    "accounting_bank_statement_lines",
    "desktop_files",
    "desktop_file_chunks",
    "accounting_invoices",
    "accounting_invoice_lines",
    "accounting_payment_terms",
    "accounting_number_series",
    "accounting_credit_notes",
    "accounting_payments",
    "accounting_payment_allocations",
    "accounting_dunning_runs",
    "accounting_dunning_letters",
    "accounting_recurring_invoices",
    "accounting_invoice_attachments"
  ],
  "source": "local",
  "core": false,
  "editable": true,
  "deletable": true,
  "layout": {
    "shell": "full-workspace",
    "icon_svg": "<svg ...> ... </svg>",
    "left": "Rechnungs-Scopes, Status-Chips, Schnellfilter",
    "center": "Editor und Detail mit Tabs (Stammdaten, Positionen, Steuern, Zahlungen, PDF, Verlauf)",
    "right": "Kunden-Inspector, offene Posten, AI-Aktionen (Mahnlauf, Skonto, Gutschrift)"
  },
  "category": "Finance",
  "version": "v0.1",
  "developer": "CTOX",
  "license": "AGPL-3.0-only",
  "tags": ["invoices", "billing", "fibu", "skonto", "dunning", "xrechnung"],
  "store": {
    "summary": "Rechnungsstellung mit Lebenszyklus, Gutschriften, Skonto, Mahnwesen, XRechnung und GoBD-Archivierung auf Basis der Buchhaltung.",
    "repository": "metric-space-ai/ctox",
    "source_path": "modules/invoices",
    "installable": true,
    "editable_after_install": true,
    "distribution": "ctox-repo-module"
  },
  "install_scope": "store",
  "default_installed": false
}
```

**Hinweis `install_scope: 'store'`** — das Modul wird **nicht** automatisch installiert, sondern muss explizit über den `app-store` aktiviert werden (analog `customers`, `iot`). Das ist wichtig, weil `invoices` harte Abhängigkeit zu `buchhaltung` und `customers` hat.

**Dependency-Regel:** Falls Business OS bis dahin keine first-class `dependencies` im Manifest kennt, muss `invoices` beim Mount selbst pruefen, ob `buchhaltung` und `customers` installiert/aktiv sind, und bei fehlender Dependency eine read-only Blocker-Ansicht mit App-Store-Link zeigen. Nicht heimlich eigene Konten-, Kunden- oder Journal-Ersatzdaten anlegen.

**Do:**

- `schema.js` des `invoices`-Moduls darf bestehende Schemas importieren/re-exportieren, wenn es nur darum geht, Dependency-Collections fuer Browser-Registration bereitzustellen.
- `module.json` und `registry.json` muessen dieselbe Collection-Liste tragen.

**Don't:**

- Nicht nur die 11 Invoice-Collections eintragen. Dann startet `state.sync.startModule(mod)` nicht fuer Kunden-, FIBU- und Datei-Collections.
- Keine `desktop_files` ohne `desktop_file_chunks` verwenden.

---

## 9. Tests

Alle Tests in `tests/` als `*.test.mjs` (analog `customers/customers.test.mjs`).

| Test | Was wird geprüft |
|---|---|
| `invoice-types.test.mjs` | Discriminator-Logik, State-Machine, Currency-Kontext |
| `invoice-poster.test.mjs` | Doppelte Buchführung, Soll = Haben, korrekte Konten, GoBD-Lock nach Post, Cent-Arithmetik |
| `invoice-actions.test.mjs` | Storno, Gutschrift, Skonto, Allocation-Idempotency |
| `invoice-pdf.test.mjs` | Pflichtangaben §14 UStG, EU-Variante, Reverse-Charge, Kleinunternehmer |
| `invoice-dunning.test.mjs` | Mahnstufen, Verzugszinsberechnung, Filter-Logik |
| `invoice-numbering.test.mjs` | Fortlaufende Nummern, Lücken-Detection, Reset-frei |
| `module-e2e.test.mjs` | UI: Draft erstellen → Post → Payment-Allocation → Paid |
| `schema-contract.test.mjs` | `module.json`, `collections.schema.json`, `schema.js` und `registry.json` enthalten dieselben Modul-Collections |
| `command-builders.test.mjs` | Builder erzeugen kanonische `command_type`-Payloads |
| `rxdb-replication.test.mjs` | Replikation in den `ctox-rxdb`-Testsuiten |
| Rust: `invoices_*` | Native Handler: Idempotenz, Nummernreservation, GoBD-Lock, Journal-Soll/Haben, Failure-Projektion |
| Rust: `rxdb_peer`/store | Neue Modul-Collections werden aus `collections.schema.json` registriert; Command-Projektion round-tripped |

**Pflicht-Checks vor Merge einer Modul-Datenpfad-Phase:**

```sh
node --check src/apps/business-os/modules/invoices/index.js
node --test src/apps/business-os/modules/invoices/tests/*.test.mjs
node src/apps/business-os/scripts/assert-module-conformance.mjs
ctox business-os app validate invoices --source
cargo test --bin ctox --no-fail-fast invoices
```

Zusaetzlich muss nach jeder UI-/Mount-/Datenpfad-Aenderung ein echter Browser-Proof gegen die Business-OS-Shell laufen (`ctox business-os serve --addr 127.0.0.1:<port>` + Playwright oder Browser-Plugin): Modul oeffnen, sichtbaren Root nachweisen, Console/Page-Errors pruefen.

Nur wenn Core-RxDB-/Shared-Contract-Dateien geaendert wurden:

```sh
node src/core/rxdb/tools/build_business_os_schema_contract.mjs
node src/apps/business-os/rxdb/tests/run-all.mjs
cargo test --manifest-path src/core/rxdb/Cargo.toml
cargo check
```

Wenn `src/apps/business-os/rxdb/src/*` geaendert wurde, vorher den RxDB-Browser-Bundle nach `docs/ctox-rxdb.md` rebuilden und die Cache-Buster konsistent bumpen.

**Regression-Tests für bestehende `buchhaltung`-Funktionen** sind betroffen, sobald Native-Handler in `accounting_journal_entries` schreiben. Mindestens GoBD-Immutability, Journal-Soll/Haben und DATEV/HGB-Lesbarkeit muessen dadurch abgedeckt werden.

---

## 10. Build-Reihenfolge (10 Iterationen, jeweils erst nach gruenen Gates weiter)

| # | Iteration | Liefer­ergebnis | Abhängig von |
|---|---|---|---|
| **0** | **Vorbereitung / Source Audit** | Konkrete Liefer-Items siehe Abschnitt 10.0. Archivierungs-Output: `archive/port-notes/invoices-port.md` mit Port-Mapping, Guardrail-Notizen, Approval-Vorab-Klaerung, Vendor-PDF-Entscheidung. Kein aktiver Code. | — |
| **0.5** | **Approval-Workflow-Vorab (optional)** | Nur falls Phase 0 ergibt, dass `business_commands` kein Approval-Statusmodell hat. Schema-Erweiterung um `approval_required`, `approved_by_command_id`, `approval_state` (`pending_approval`/`approved`/`rejected`); native Handler-Accept-Liste; Test-Fixture. | 0 |
| **1** | **Data-Plane Fundament fuer Accounting** | Nachweis, dass `collections.schema.json` fuer relevante `buchhaltung`-/Dependency-Collections vorhanden ist und der native Peer Modul-Schemas dynamisch scannt. Keine Invoice-UI. Core-Contract/Hashes nur anfassen, falls Shared/Core-Collections geaendert werden. | 0, ggf. 0.5 |
| **2** | **Invoice-Schemas & Manifest** | `modules/invoices/module.json`, `collections.schema.json`, `schema.js` (Kompatibilitaet) und `registry.json` mit 11 Invoice-Collections plus Dependency-Collections. `accounting_number_series` enthalten. `assert-module-conformance.mjs` muss gruen sein; keine Hash-Regeneration fuer reine Modul-Collections. | 1 |
| **3** | **Native Command Skeleton** | `is_invoices_active_command`, `handle_invoices_active_command`, Auth/ACL, idempotente Command-Outcome-Schreibung, Failure-Projektion. Noch keine fachliche Post-Logik. | 2 |
| **4** | **Domain-Kern Pure Functions** | `core/invoice-types.js`, `invoice-validate.js`, `invoice-tax.js`, `invoice-poster.js`, `invoice-numbering.js` als pure Berechnungs-/Validierungslogik. Tests ohne echte RxDB-Writes. | 3 |
| **5** | **Draft Lifecycle End-to-End** | Native Handler fuer create/update/delete Drafts, Browser-Liste + Editor. UI sendet nur Commands. Tests: Draft roundtrip ueber echte `business_commands`-Projektion. | 4 |
| **6** | **Post + Journal + Nummern** | Native `invoices.invoice.post`, Nummernreservation, Journal Entry/Lines, `posted_at`, Storno und Gutschrift-Basis. Tests: Soll=Haben, Idempotenz, GoBD-BYPASS-Test gegen direkte Aenderung. | 5 |
| **7** | **PDF + XRechnung Archiv** | PDF/XRechnung-Erzeugung, `desktop_files`/chunks Archivierung, Attachment-Refs. Erst Vendor-Entscheidung dokumentieren, dann implementieren. Tests: Pflichtangaben, Datei-Chunk-Roundtrip, keine npm/bare Imports. | 6 |
| **8** | **Zahlungen + Skonto + Allocation** | Native Payment/Allocation Handler, Bankstatement-Matching, Skonto-Berechnung, Status-Neuberechnung. Tests: Allocation-Idempotenz, partial/paid/open States. | 7 |
| **9** | **Mahnwesen + Dunning** | Native Dunning Runs/Letters, Verzugszins, Mahn-PDF ueber bestaetigten PDF-Pfad. Tests: Level-Transitions, Fees/Interest, kein Auto-Send ohne Send-Command. | 8 |
| **10** | **Recurring + i18n + Polish** | Recurring Templates/Run, locales vollstaendig, README, Performance-Pass, Full replication smoke. | 9 |

**Jede Iteration endet mit:**
- Lauffähiger Code in `src/apps/business-os/modules/invoices/`
- Grüne Tests
- Kurzes `CHANGELOG.md`-Eintrag pro Iteration
- Review mit dem User

**Abbruchregeln:**

- Nicht in Phase 1 gehen, solange Phase 0 nicht abgeschlossen ist und die Vendor-PDF-Entscheidung, Approval-Workflow-Klaerung und Port-Mapping schriftlich vorliegen.
- Phase 0.5 (Approval-Workflow) zwingend einschieben, falls `business_commands` keinen Approval-Status hat. Phase 1 nicht starten, solange `invoices.proposal.*` ohne Approval nicht sicher gemappt werden kann.
- Nicht in Phase 3 gehen, solange `accounting_*` und `accounting_invoices*` nicht in `collections.schema.json`/`module.json.collections` stehen und der native Peer die Modul-Schema-Dateien scannt.
- Nicht in Phase 5 gehen, solange `invoices.*` Commands nicht nativ akzeptiert und terminal completed/failed projiziert werden.
- Nicht in Phase 6 gehen, solange `accounting_number_series` nicht persistent und idempotent getestet ist.
- Nicht in Phase 7 gehen, solange `desktop_files` und `desktop_file_chunks` nicht im Manifest und Sync fuer das Modul laufen.
- Falls Phase-0-Vendor-Entscheidung PDF ablehnt: Phase 7 startet mit XRechnung-XML, PDF kommt erst in Phase 10 nach.

### 10.0 Phase 0 — Konkrete Liefer-Items

Phase 0 ist die einzige Phase ohne lauffähigen Modul-Code. Sie ist trotzdem eine **echte Arbeitsphase** mit festen Liefer-Items. Ohne diese Items ist Phase 1 ein Blindflug.

**Pflicht-Liefer-Items in `archive/port-notes/invoices-port.md`:**

1. **Port-Mapping-Tabelle** `business-basic/packages/accounting/src/invoice/*.ts` → `core/invoice-*.js`
   - Pro Datei: Welche Funktionen/Exports werden 1:1 portiert, welche brauchen Adapter, welche fallen weg?
   - Konkret erkannte Stolpersteine: TypeScript-only Features (z. B. `satisfies`, `as const`), Drizzle-DB-Dependencies, Node-only APIs (`fs`, `path`).

2. **Guardrail-Notizen**
   - `CLAUDE.md`, `docs/architecture.md`, `docs/ctox-rxdb.md`, `src/core/rxdb/AGENTS.md`, `src/apps/business-os/rxdb/AGENTS.md`, `src/core/business_os/AGENTS.md` zitiert mit Bezug auf `invoices`-Modul.
   - Liste der Verbote, die im Plan v3 explizit stehen, mit Pfad, an dem die Quelle zitiert wurde.

3. **`business_commands`-Statusmodell-Verifikation**
   - Was darf der heutige `business_commands`-Status sein? Liste mit Quelle (`collections.schema.json`/`schema.js`-Kompatibilitaet, Rust-Handler).
   - Gibt es Approval-/Proposal-/Pending-Felder? Wenn nein: Empfehlung "Phase 0.5 zwingend" oder "Phase 0.5 entfällt, weil Statusmodell ausreicht".
   - Beispiel-Kommandos aus `customers` mit ihrem Status-Lifecycle.

4. **`accounting_*` Runtime-Schema-Audit**
   - Liste der heute in `buchhaltung/collections.schema.json` vorhandenen Collections.
   - Welche `accounting_*` aus `buchhaltung` fehlen dort oder in `module.json.collections`? Konkrete Diff-Liste.
   - Falls Luecken: Korrekturplan als Teil von Phase 1.

5. **Vendor-PDF-Entscheidung**
   - Drei realistische Optionen mit Aufwand/Risiko:
     - A) `pdf-lib` als ESM-Bundle in `vendor/` (analog Notesnook-Source-Pattern) — Aufwand X Tage, Bundle-Größe Y KB.
     - B) Serverseitiger PDF-Print via bestehender CTOX-Print-Pipeline (Rust, lokal) — Aufwand X Tage, kein Browser-Bundle nötig.
     - C) Erst XRechnung-XML, PDF nachziehen — Aufwand X Tage, kleinster Start.
   - Empfehlung mit Begründung.
   - **Diese Entscheidung fällt in Phase 0, nicht in Phase 7**, damit Phase 7 nicht blockiert.

6. **Port-Mapping `number-series.ts`**
   - Verhalten der Lücken-Detection in der Original-Implementierung.
   - Wie wird sie in `accounting_number_series` abgebildet? Welche `gap_policy` brauchen wir?
   - Wie verhält sich der Handler bei gleichzeitigem Post von zwei Rechnungen (Race)?

7. **Skonto- und Steuerspezialfälle**
   - §13b UStG (Reverse-Charge B2B): welche Original-Behandlungen gibt es?
   - §19 UStG (Kleinunternehmer): eigene Pflichttexte im PDF?
   - Innergemeinschaftliche Lieferung: USt-IdNr.-Validierung, separate Belegnummern?

8. **Offene Entscheidungen aus Abschnitt 13**
   - Konkrete Antworten auf die drei Fragen (Dependency-Konvention, PDF-Reihenfolge, `accounting_number_series`-Scope).

**Mini-Checklist Phase 0:**

```text
[ ] docs/ctox-rxdb.md, src/core/rxdb/AGENTS.md, src/apps/business-os/rxdb/AGENTS.md, src/core/business_os/AGENTS.md, CLAUDE.md, docs/architecture.md gelesen
[ ] business-basic/packages/accounting/src/invoice/* Zeile fuer Zeile gesichtet
[ ] business-basic/packages/accounting/src/{number-series,payment,dunning,workflow}/* gesichtet
[ ] business_commands Statusmodell verifiziert (Kommandos + Status + Handler-Projection)
[ ] `buchhaltung/collections.schema.json` + `module.json.collections` auf `accounting_*` Collections geprueft
[ ] pdf-lib Bundle-Groesse abgeschaetzt (Test mit minimalem Vendor-Bundle)
[ ] CTOX-Print-Pipeline vorhanden? Pfad-Notiz.
[ ] XRechnung XSD-Specs lokal? Wenn nein, Bezugsquellen dokumentiert.
[ ] archive/port-notes/invoices-port.md mit allen 8 Liefer-Items geschrieben
[ ] User-Review der Liefer-Items vor Phase 0.5 / Phase 1
```

### 10.1 Editierbarer Agent-Fortschrittsplan (skill-konform)

**Statuswerte:** `todo`, `active`, `blocked`, `review`, `done` (vom Skill vorgeschrieben).

**Tracking-Regeln:**

- Nur **eine** Phase ist `active` (Ausnahme: parallele Arbeit mit expliziter Doku).
- `done` nur mit konkreter Test-/Konsolen-Evidence, nicht aus Intention.
- `blocked` mit spezifischem Blocker + `Next action`.
- Keine Screenshot-/Phase-`done` ohne echten Beleg.

| Phase | Status | Owner/Agent | Started | Finished | Touched files | Gate/Evidence | Blocker | Next action | Notes |
|---|---|---|---|---|---|---|---|---|---|
| 0 Vorbereitung / Source Audit | done | Mavis | 2026-06-11 | 2026-06-11 | `archive/port-notes/invoices-port.md` | 8 Liefer-Items dokumentiert (Port-Mapping, Guardrails, Approval-Check, Schema-Audit, Vendor-PDF, Number-Series, §13b/§19/EU-Ic, drei offene Entscheidungen) | — | — | Phase 0.5 entfällt; PDF-Vendor via CTOX-Print-Pipeline als primärer Pfad |
| 1 Data-Plane Fundament fuer Accounting | done | Mavis | 2026-06-11 | 2026-06-11 | `src/core/rxdb/tools/build_business_os_schema_contract.mjs`, `src/core/business_os/business_os_schema_contract.json`, `src/core/business_os/business_os_schema_hashes.json`, `src/apps/business-os/rxdb/src/schema.mjs`, `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs`, `src/apps/business-os/shared/{db,sync}.js`, `src/apps/business-os/modules/matching/ui/businessOsDataSource.js` | `node src/core/rxdb/tools/build_business_os_schema_contract.mjs --write` (7 accounting_* neu); `cargo test --bin ctox native_all_schema_hashes_match_browser_contract_fixture` (1 passed); `node src/apps/business-os/rxdb/tests/run-all.mjs` (39 passed, 0 failed) | — | — | Cache-Buster `?v=20260611-rxdb-acc` |
| 2 Invoice-Schemas & Manifest | done | Mavis | 2026-06-11 | 2026-06-11 | `src/apps/business-os/modules/invoices/{module.json,schema.js}`, `src/apps/business-os/modules/buchhaltung/schema.js` (accounting_number_series), `src/apps/business-os/modules/registry.json` | 19 accounting_* in contract; 12 new schema hashes | — | — | — |
| 3 Native Command Skeleton | done | Mavis | 2026-06-11 | 2026-06-11 | `src/core/business_os/invoices.rs` (neu, jetzt ~2400 LoC nach v4), `src/core/business_os/{mod.rs,store.rs,rxdb_peer.rs}` | `cargo test --bin ctox --no-fail-fast invoices` (8 passed, 12 nach v4) | — | — | 23 Command-Typen; v4: BTreeMap-Import repariert (pre-existing bug) |
| 4 Domain-Kern Pure Functions | done | Mavis | 2026-06-11 | 2026-06-11 | `src/apps/business-os/modules/invoices/core/{invoice-types,invoice-validate,invoice-tax,invoice-numbering,invoice-poster,xrechnung}.js` | `node --test src/apps/business-os/modules/invoices/tests/*.test.mjs` (47+8 passed) | — | — | Tausendstel-Quantity Convention (XRechnung/UBL) |
| 5 Draft Lifecycle End-to-End | done | Mavis | 2026-06-11 | 2026-06-11 | `src/core/business_os/invoices.rs` (create/update/delete Handler, soft-delete), `src/apps/business-os/modules/invoices/index.{html,js,css}`, `src/apps/business-os/modules/invoices/commands/builders.js`, `src/apps/business-os/modules/invoices/tests/builders.test.mjs` | `cargo test --bin ctox --no-fail-fast invoices` (Lifecycle-Test grün); `node --test builders.test.mjs` (4 passed) | — | — | v4: UI nutzt `ctx.db.collections` + `ctx.eventBus.on` |
| 6 Post + Journal + Nummern | done | Mavis | 2026-06-11 | 2026-06-11 | `src/core/business_os/invoices.rs` (post-Handler) | `cargo test --bin ctox --no-fail-fast invoices` (post-Test grün) | — | — | debit/credit-Felder, Tausendstel-Quantity, posted_at-Lock |
| 7 PDF + XRechnung Archiv | done | Mavis | 2026-06-11 | 2026-06-11 | `src/apps/business-os/modules/invoices/core/invoice-xrechnung.js` | `node --test invoice-xrechnung.test.mjs` (8 passed) | — | — | XRechnung 2.0 CII-Envelope; PDF-Print wartet auf CTOX-Print-Pipeline |
| 8 Zahlungen + Skonto + Allocation | done | Mavis | 2026-06-11 | 2026-06-11 | `src/core/business_os/invoices.rs` (allocate/unallocate) | `cargo test --bin ctox --no-fail-fast invoices` (allocate-Tests grün) | — | — | (payment_id, invoice_id) Idempotenz |
| 9 Mahnwesen + Dunning | done | Mavis | 2026-06-11 | 2026-06-11 | `src/core/business_os/invoices.rs` (dunning run + letter send) | `cargo test --bin ctox --no-fail-fast invoices` (dunning-Tests grün) | — | — | — |
| 10 Recurring + i18n + Polish | done | Mavis | 2026-06-11 | 2026-06-11 | `src/apps/business-os/modules/invoices/locales/{de,en}.json`, `src/apps/business-os/modules/invoices/README.md` | locales + README; 12 native + 59 JS Tests grün | — | — | — |
| 11 v4 P-Fixes (User-Review) | done | Mavis | 2026-06-12 | 2026-06-12 | `src/apps/business-os/modules/invoices/index.js`, `src/core/business_os/invoices.rs`, `src/core/business_os/store.rs`, `src/apps/business-os/modules/invoices/core/invoice-{tax,poster}.js` | 12 native, 59 JS, 39 browser, schema-hash, cargo check: alle grün | — | — | P0 Mount, P1 Feldnamen/Quantity/Stubs/Unknown-Gate, P2 GoBD, P3 Plan |
| 12 v5 Skill-Conformance — Reactive UI + Unmount Cleanup | done | Mavis | 2026-06-12 | 2026-06-12 | `src/apps/business-os/modules/invoices/index.js` (resolveCollection, wireRealtime, scheduleRefresh, mount returns cleanup), `src/apps/business-os/modules/invoices/tests/mount-unmount.test.mjs` (3 neue Tests), `docs/business-os-invoices-implementation-plan.md` | 62 JS Tests grün (+3 mount/unmount), 12 native, 39 browser, 1 schema-hash-fixture, conformance OK, cargo check OK; Hard-Stop "reactive subscribe + cleanup" geschlossen via `WATCHED_COLLECTIONS` + `wireRealtime()`; data-context-* Attribute auf root/list-item/center-Pane | — | — | Verifikation gegen `customers/index.js:1037 wireRealtime()` als Skill-Reference |
| 13 P1+P2+P3 Findings aus User-Review (zweite Iteration) | done | Mavis | 2026-06-12 | 2026-06-12 | `src/core/business_os/invoices.rs` (`validate_invoice_for_command` neu, 23 LoC neuer Helper, in create/update/post aufgerufen), `src/apps/business-os/modules/invoices/index.js` (partySelect change-listener, postButton disabled via `computeValidationIssues`, full-draft-persist-then-post in `postInvoice`, Inspector "Aktionen" neutralisiert), `src/apps/business-os/modules/invoices/tests/editor-validation.test.mjs` (5 neue JS Tests), `src/apps/business-os/modules/invoices/README.md` + `module.json` (Belastbarkeit vs. Out-of-Scope-Bereich präzisiert) | **18 native** (war 12, +6: validate_invoice_accepts_a_well_formed_draft, create_with_empty_party_id_is_rejected, post_with_zero_lines_is_rejected, post_with_skonto_percent_but_no_skonto_days_is_rejected, post_with_paired_skonto_percent_and_days_is_accepted, post_with_invalid_invoice_type_is_rejected); **69 JS** (war 64, +5 editor-validation); conformance 23 modules OK; 1 schema-hash-fixture OK; 39 browser-rxdb OK (transient flake auf erstem Run, stabilisiert); cargo check clean; pre-existing `create_then_update_then_delete_draft_lifecycle_persists` Test auf vollständigen Payload umgestellt (alter Test nutzte leeres Payload, was die neue Validator-Schicht fängt) | — | Manuelle Browser-Shell-Verifikation vom User (create → save → post → allocate → reload gegen echten RxDB/WebRTC-Peer) | P1#1 Party-Pick, P1#2 Persist-then-Post, P1#3 Native Validator (JS-Logik 1:1 gespiegelt), P2 README + module.json ehrlich, P3 Inspector-Zukunftstext raus |

---

## 10.2 v6 Real-Shell Browser Proof

Der Skill-Hard-Stop "real Business OS shell browser proof was not run after the last UI/runtime change" wurde in v6 gegen die echte Business-OS-Shell ausgefuehrt.

**Erster Lauf (fehlgeschlagen, echter Befund):**

- `ctox business-os serve --addr 127.0.0.1:18765` startete die Shell.
- Playwright oeffnete `http://127.0.0.1:18765/business-os/?rxdbSmoke=1`, doppelklickte `.desktop-icon[data-target="invoices"]`.
- Ergebnis: `activeModule=invoices` und `window.__ctoxInvoicesModule=true`, aber `#invoices-root=false`. Ursache: `mount(ctx)` lud das Modulfragment nicht in `ctx.host`, sondern suchte einen globalen Root.

**Fix:**

- `ensureMountedMarkup(ctx)` laedt `index.html` in `ctx.host`.
- `moduleRoot()` scoped Root-Zugriffe auf `ctx.host` und nutzt `document.getElementById('invoices-root')` nur als Standalone-/Test-Fallback.

**Zweiter Lauf (gruen fuer Shell-Render):**

- `activeModule=invoices`
- `title="Rechnungen · CTOX Business OS (...)"`
- `window.__ctoxInvoicesModule=true`
- `#invoices-root` existiert und ist sichtbar
- `#invoices-root.dataset.contextSkill="product_engineering/business-os-app-module-development"`
- Sichtbarer Text: `RECHNUNGEN (0)`, Filter `Alle/Überfällig/Offen/Entwürfe`, Button `+ Neue Rechnung`, Empty State und Inspector.
- Keine Console-/Page-Errors im Playwright-Lauf.

**Was noch nicht als production-ready bewiesen ist:**

- Native RxDB peer lock war von einem anderen Prozess gehalten; deshalb wurde kein voller `business_commands`-Lifecycle im Browser ausgefuehrt.
- Vor Production-Release muss ein ungesperrter Real-Peer-Lauf `create draft → save → post → payment allocation → replicated refresh` mit sichtbaren Daten und Console-/Network-Health nachweisen.

## 11. Risiken & Annahmen

### Risiken

1. **pdf-lib im Browser** — wir können keine npm-Pakete in Business-OS-Module importieren (`docs/ctox-rxdb.md` Hard Rule: "No npm/bare imports in the browser runtime"). **Mitigation:** pdf-lib als ESM-Bundle in `vendor/` (analog Notesnook-Source-Pattern) oder eigene PDF-Generierung mit `pdf-lib`-Kern.
2. **Komplexität der PDF-Generierung** — §14 UStG hat strikte Pflichtangaben, XRechnung ist ein eigenes XML-Format. **Mitigation:** Phase 5 (PDF/XRechnung) hat eigenen Realitätscheck; falls pdf-lib-Bundling zu groß wird, XRechnung zuerst als XSD-validierten XML-Export, PDF später.
3. **Mahnwesen-PDF** — benötigt dasselbe PDF-Setup. **Mitigation:** mit `dunning` warten, bis `pdf` in Phase 5 grün ist.
4. **Steuerliche Korrektheit** — wir sind kein Steuerberater. **Mitigation:** Disclaimer prominent, klare Markierung "AI-Vorschlag — bitte Steuerberater prüfen" bei Proposals, harter Hinweis im README.
5. **Migration aus Bestandsdaten** — wenn Kunden bereits Rechnungen in `customers` Opportunities oder als Dokumente in `documents` haben. **Mitigation:** Phase 8 bringt einen One-Shot-Importer, der Opportunities mit `stage=closed_won` und `next_action='invoice'` in Invoice-Drafts überführt. Kein Auto-Post.
6. **Replikations-Last bei großen Rechnungschargen** — `accounting_invoice_lines` kann bei vielen Rechnungen schnell wachsen. **Mitigation:** Indices gezielt setzen, Lazy-Load der Lines im Detail-View (nicht in der Liste), Runtime-Schema pro Collection via `collections.schema.json` und realem Shell-Start verifizieren.
7. **Runtime-Schema Drift** — `collections.schema.json`, `schema.js` und `module.json.collections` koennen auseinanderlaufen. **Mitigation:** `assert-module-conformance.mjs`, App-Creator-Guard und echter Shell-Start nach jeder Datenpfad-Aenderung.
8. **Command akzeptiert, aber nicht ausgefuehrt** — unbekannte `invoices.*` Commands wuerden generisch queue-backed akzeptiert. **Mitigation:** Native Allowlist/Handler vor UI-Draft-Workflow; Tests fuer completed/failed Projektion.
9. **GoBD nur im Browser abgesichert** — bestehende `buchhaltung` Hooks laufen nur im gemounteten Browser-Modul. **Mitigation:** Native Handler erzwingen `posted_at`-Unveraenderbarkeit und testen direkte Aenderungsversuche.

### Annahmen

- Die `business_commands`-Infrastruktur repliziert und projiziert belastbar, aber neue fachliche Commands brauchen explizite native Handler.
- `accounting_journal_entries` verwendet das bestehende Lock-Feld `posted_at`; native GoBD-Stabilitaet muss im Port ergaenzt/geprueft werden.
- `desktop_files`/`desktop_file_chunks` sind der Standard-Speicherort für Datei-Artefakte (Rechnungs-PDF, Mahnschreiben), muessen aber im Manifest und Sync-Pfad des Moduls stehen.
- `vendor/`-Bundling von pdf-lib ist nur nach separater Pruefung zulaessig; keine npm/bare Imports.
- `customer_accounts` ist die einzige Party-Quelle (kein zweiter `parties`-Master).
- SKR03/04 ist in `buchhaltung/templates/skr.js` vollständig, mit `code`-Lookups (1400, 8400 etc.).
- `buchhaltung`-Collections sind fachlich vorhanden; ihre `collections.schema.json`-/Manifest-Registrierung muss vor Nutzung verifiziert und ggf. hergestellt werden.

---

## 12. Out of Scope (für später)

- **Eingangsrechnungs-OCR** (Beleg-Scan → strukturiert) — eigene Phase, braucht `parsers/receipt-ocr.js` analog Camt/MT940.
- **ELSTER-XML-Export der Rechnungs-USt** — ELSTER-Logik liegt in `buchhaltung/reports/elster.js`. Falls Erweiterung nötig, dort oder im neuen Modul.
- **Anbindung an Payment-Provider** (Stripe, PayPal) — out of scope, nur SEPA-Lastschrift-Anbindung über `bank-import/camt.js` ist im Scope.
- **Mehrmandantenfähigkeit pro Rechnung** — aktuell ist Mandant = Workspace. Falls Mandantentrennung kommt, muss `accounting_invoices.tenant_id` ergänzt werden, mit eigener Migration.
- **Internationale Steuern (UK, US-Sales-Tax etc.)** — aktuell nur DE, EU-Reverse-Charge. Internationalisierung ist Erweiterung.
- **Rechnungs-Workflow mit Freigabe-Stufen** (z. B. "Rechnung > 10.000 EUR braucht 2 Approvals") — Policy-Layer in `commands/policy.js` als spätere Erweiterung.

---

## 13. Nächste Schritte (Empfehlung)

1. **Unlocked Real-Peer E2E** fahren: `create draft → save → post → payment allocation → replicated refresh`, mit sichtbaren Daten, Console-/Page-Error-Check und Nachweis aus `runtime/business-os-rxdb.sqlite3`.
2. **Legacy-Altlasten abbauen**: bestehende Module mit `ctx.db.raw`/direkten Raw-Pfaden modulweise migrieren, jeweils mit Conformance-Guard und echtem Shell-Proof. Kein Big-Bang-Refactor.
3. **Nicht implementierte `invoices.*` Commands** nur nach Bedarf fachlich ausbauen (`cancel`, `credit_note`, `recurring`, `proposal.*`); bis dahin muessen sie weiter klar `failed` statt scheinbar `completed` enden.
4. **Production-Ready-Freigabe** erst nach gruenem Unlocked-E2E und mindestens einem zweiten Browser-/Peer-Replikationslauf.

Offene Entscheidungen vor Phase 2:

- Soll `invoices` eine Manifest-Dependency-Konvention einfuehren (`dependencies: ["buchhaltung", "customers"]`) oder nur beim Mount blockieren, wenn die Module fehlen?
- Soll PDF-Erzeugung initial nur XRechnung/XML archivieren und PDF nachziehen, falls `pdf-lib`-Vendoring zu gross/fragil ist?
- Soll `accounting_number_series` allgemein fuer `buchhaltung` nutzbar werden oder zunaechst invoice-spezifisch bleiben?

Nach Zustimmung zu diesen Punkten geht es in Phase 0.
