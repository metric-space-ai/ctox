# invoices — Rechnungsstellung als Business-OS Modul

Sales-out / Sales-in / Credit-Note (Storno und Recurring: derzeit Stub-Handler — siehe "Out of Scope"). Aufgesetzt auf dem `buchhaltung`-Modul: SKR03/04-Kontenplan, doppelte Buchführung, DATEV-Export, ELSTER, GoBD-immutable Journal-Einträge.

## Status (Plan v5 + Phase 13)

Belastbar implementiert und getestet: Draft-Lifecycle, Post mit Journal, Skonto/Allocation, Mahnlauf, XRechnung-XML. Weitere Schritte siehe "Out of Scope".

| Phase | Inhalt | Status |
|---|---|---|
| 0 | Source Audit, Guardrails, Liefer-Items | done |
| 1 | Data-Plane Fundament (`buchhaltung` im nativen Schema-Contract) | done |
| 2 | 11 Invoice-Collections + `accounting_number_series` + Manifest | done |
| 3 | Native Command Skeleton (23 Command-Typen) | done |
| 4 | Domain-Kern (pure JS: types, validate, tax, numbering, poster) | done |
| 5 | Draft Lifecycle (create/update/delete) | done |
| 6 | Post + Journal + Nummern | done |
| 7 | XRechnung 2.0 XML | done |
| 8 | Zahlungen + Skonto + Allocation | done |
| 9 | Mahnwesen + Dunning (Level-1, ohne Auto-Send) | done |
| 10 | i18n + Polish | done |
| 12 | Reactive Shell (collection.$ subscribe + cleanup) | done |
| 13 | P1-P3 Findings (party-pick, persist-before-post, native validator, README-Honest) | done |

## Belastbar implementierte Funktionsfläche

- **Drafts:** `invoices.invoice.create` / `update` / `delete` (soft-delete).
- **Post:** `invoices.invoice.post` mit GoBD-Lock, Nummern-Reservation
  (`accounting_number_series`), balanciertem Journal-Eintrag.
- **XRechnung 2.0:** `core/invoice-xrechnung.js` erzeugt ein CII-Envelope aus
  dem gespeicherten Draft. XML-Preview + Download-Button im Editor.
- **Zahlungen:** `invoices.payment.allocate` / `unallocate` mit
  deterministischer `(payment_id, invoice_id)`-Allokation; überbuchen wird
  abgelehnt; `paid_cents` / `open_cents` werden neu berechnet.
- **Mahnen:** `invoices.dunning.run` erzeugt Letters für überfällige Drafts;
  `invoices.dunning.letter.send` setzt `letters_sent` und Run-`state:executed`.
  Es gibt **keinen** Auto-Send — Letters gehen erst nach explizitem
  `letter.send`-Command raus.

## Architektur

```
src/core/business_os/invoices.rs   native Handler, allowlist, idempotency,
                                   validate_invoice_for_command (JS-Spiegel)
src/apps/business-os/modules/invoices/
  schema.js                         11 RxDB-Collections (accounting_invoices, lines, ...)
  collections.schema.json           native-readable Schema-Contract
                                   (schema_format: ctox-business-os-module-collections-v1)
  core/                            pure JS: types, validate, tax, numbering,
                                          poster, xrechnung
  commands/builders.js              UI command builders
  index.{html,js,css}               3-pane mount (list / center / inspector),
                                   reactive collection.$ subscribe + cleanup,
                                   data-context-* attribute
  locales/{de,en}.json             i18n
  tests/                            69 JS unit tests + 18 native Rust tests
```

## Native Commands (23)

23 Command-Typen sind in `is_invoices_active_command` erfasst. Davon
implementiert (mit `Ok(...)`-Pfad): `create`, `update`, `delete`, `post`,
`allocate`, `unallocate`, `dunning.run`, `dunning.letter.send`. Alle
übrigen (`cancel`, `create_credit_note`, `assign_payment_terms`,
`line.create`/`update`/`delete`, `match_suggestions`, `recurring.*`,
`import.from_outbound`, `proposal.*`) fallen aktuell in den
nicht-implementierten Pfad und bailen mit `anyhow!("invoices.<x> not yet
implemented")`. Das verhindert stillen `Ok({stub:true})`-Erfolg
(siehe "Validation Gate" unten).

## Datenfluss

```
UI  →  business_commands  →  Native Handler  →  business_records (accounting_invoices, accounting_journal_entries, ...)
                                          →  accounting_number_series (Lücken-Detection)
```

Alle Mutationen gehen über `business_commands` (RxDB-only). UI schreibt nie direkt.

## Validation Gate (nativ dupliziert, P1#3)

`validate_invoice_for_command(invoice, strict_post)` in
`src/core/business_os/invoices.rs` spiegelt 1:1 die Regeln aus
`modules/invoices/core/invoice-validate.js` und wird von `create`,
`update` und `post` aufgerufen. Pflichtfelder: `invoice_type`,
`party_id`, `currency`, `invoice_date_ms`. Konsistenz: `small_business`
vs. `tax_breakdown`, `reverse_charge` nur für `sale_out`/`sale_in`,
`eu_ic_supply` nur für `sale_out`, `credit_note_*` braucht
`credit_note_for_id`, `skonto_percent` braucht `skonto_days > 0`.
Strict-Post-Gate: mindestens eine valide Position (Beschreibung, Menge
in Tausendsteln, `unit_price_cents` ≥ 0, `tax_rate` in `[0,1]`,
`account_code`).

Damit kann weder ein veralteter Client noch ein App-Creator-Hardener
einen ungültigen Draft posten: der Dispatch-Arm in `store.rs` failt
hard und projiziert `status: failed` auf `business_commands`.

## Idempotenz

| Command | Idempotenz-Mechanismus |
|---|---|
| `invoices.invoice.create` | (1) dispatch-layer dedupliziert `command_id`; (2) `existing` invoice liefert `idempotent: true` |
| `invoices.invoice.update` | merge auf bestehendem Draft, dann Re-Validierung |
| `invoices.invoice.post` | deterministisch via `find_journal_entry_for_invoice(invoice_id)` — zweiter Post returnt gecachten JE |
| `invoices.payment.allocate` | deterministische `allocation_id = alloc_{payment_id}_{invoice_id}` |
| `invoices.dunning.run` | deterministische `run_id` aus `command.payload.run_id` |
| Unbekannte `invoices.*` | Dispatch-Arm failt mit `status: failed`, keine Queue-Only-Akzeptanz |

## Tests

- `cargo test --bin ctox --no-fail-fast invoices` → 18 Tests
- `node --test src/apps/business-os/modules/invoices/tests/*.test.mjs` → 69 Tests
- `cargo test --bin ctox native_all_schema_hashes_match_browser_contract_fixture` → grün
- `node src/apps/business-os/rxdb/tests/run-all.mjs` → 39 Tests grün
- `node src/apps/business-os/scripts/assert-module-conformance.mjs` → 23 modules OK
- `cargo check` → clean

## Dependencies (install_scope: store)

Modul ist **nicht** default-installed. Harte Abhängigkeiten:
- `buchhaltung` (für Kontenplan, doppelte Buchführung, DATEV-Export, ELSTER)
- `customers` (für Party-Snapshot, Activity-Timeline)

Mount-Time-Prüfung in `index.js` rendert eine read-only Blocker-Ansicht mit App-Store-Link, wenn Dependencies fehlen.

## GoBD-Invariants (nativ erzwungen)

- `accounting_journal_entries.posted_at` ist Lock-Feld
- `accounting_invoice.posted_at` und `post_journal_entry_id` werden auf Post gesetzt
- `invoices.invoice.update` bailt, wenn `state != "draft"`
- Native Validator in `validate_invoice_for_command` (strict_post=true) lehnt
  Posts mit fehlendem `party_id`, leeren `lines`, inkonsistenten
  `tax_breakdown`/`small_business`/`reverse_charge`-Kombinationen ab.
- Storno ist als `invoices.invoice.cancel` (Gegenbuchung mit
  `reversed_by_id`) in der Allowlist, aber aktuell **nicht implementiert** —
  der Handler bailt; siehe "Out of Scope".

## Out of Scope (post-MVP, derzeit nicht implementiert)

- **Storno / `invoices.invoice.cancel`:** Handler bailt, kein GoBD-Reversal.
- **Gutschrift / `invoices.invoice.create_credit_note`:** Handler bailt.
  Gutschriften können aktuell nicht erzeugt werden.
- **Recurring (`invoices.recurring.*`):** alle 4 Handler bailen. Periodische
  Instantiierung muss extern erfolgen.
- **Line-CRUD (`invoices.line.create`/`update`/`delete`):** Handler bailen.
  Lines werden über `invoices.invoice.update` mit dem `lines`-Array
  aktualisiert.
- **Proposals (`invoices.proposal.*`):** Handler bailen.
  Approval-Workflow ist nicht implementiert.
- **Payment-Terms / `invoices.invoice.assign_payment_terms`:** Handler bailt.
- **PDF-Rendering:** XRechnung XML funktioniert, PDF-Print wartet auf
  CTOX-Print-Pipeline. Auch Mahn-PDF-Briefe sind blockiert, bis die
  Pipeline steht.
- **OCR für Eingangsrechnungen**
- **ELSTER-XML-Export der UStVA**
- **Stripe / Payment-Provider Integration**
- **Mehrmandantenfähigkeit**
- **International Sales Tax (UK/US)**

## Bekannte Einschränkungen

- `accounting_payments` werden vom Handler nicht direkt persistiert — Allocation
  verweist nur via `payment_id` String. Volle Payment-Entity-Implementierung
  in einer späteren Phase.
- `customer_activities`-Schreiben ist im Plan v3 vorgesehen, aber im
  aktuellen Native Handler nicht implementiert. Wird in einer Folge-Iteration
  ergänzt.
- Echte Recurring-Cron-Logik: nicht implementiert; periodische Instantiierung
  muss extern erfolgen (z. B. via CTOX-Job-Scheduler).

## Browser-Shell-Proof (Hard-Stop)

Der Skill verlangt eine echte App-Shell-Verifikation
(`mount` → `edit` → `save` → `post` → `payment allocation` → `reload`
gegen den realen RxDB/WebRTC-Peer). Diese ist **nicht** in der
CI-Suite abgedeckt, weil der Sandbox-Browser nicht den App-Shell-Bundle
laden kann. Substitut in Tests: `mount-unmount.test.mjs` (Subscriptions
+ Cleanup) und `editor-validation.test.mjs` (Validator-Gate) decken
die kritischen Mount- und Validation-Verträge ab. **Manuelle
Verifikation in der echten App-Shell ist Voraussetzung für die
Produktionsfreigabe.**

## Run-All Gates (jede Phase mit Datenpfad-Änderung)

```sh
node src/core/rxdb/tools/build_business_os_schema_contract.mjs --write  # Schema-Contract
node src/apps/business-os/rxdb/tests/run-all.mjs                        # Browser-Suite
cargo test --manifest-path src/core/rxdb/Cargo.toml                      # rxdb-rs Crate
cargo test --bin ctox --no-fail-fast invoices                             # Native Handler
cargo test --bin ctox native_all_schema_hashes_match_browser_contract_fixture
node src/apps/business-os/scripts/assert-module-conformance.mjs          # 23-Module-Conformance
cargo check                                                                # Workspace
```

Alle sieben Gates müssen nach jeder Phase mit Datenpfad-Änderung grün sein.
