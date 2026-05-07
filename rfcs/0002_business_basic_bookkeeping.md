# RFC 0002: Business Basic — Double-Entry Bookkeeping, Invoices, Receipts

**Status:** Draft
**Author:** michaelwelsch
**Discussion:** see PR
**Affects:** `templates/business-basic/packages/db/src/schema.ts`,
`templates/business-basic/apps/web/lib/business-seed.ts`,
`templates/business-basic/apps/web/lib/business-runtime.ts`,
`templates/business-basic/apps/web/app/api/business/**`,
`templates/business-basic/apps/web/app/app/business/**`,
`templates/business-basic/apps/web/components/{business-workspace.tsx,invoice-*.tsx,business/business-actions.tsx}`,
new package `templates/business-basic/packages/accounting/**`.

## 1. Motivation

`templates/business-basic` ships a Business OS surface for sales /
marketing / operations / business with a deliberately separate
ownership boundary from CTOX core (see template `README.md`). The
**Business** module today has the surface of a billing/accounting
workspace but no engine:

- Invoices, customers, products, bookkeeping exports and reports exist
  as TypeScript types in `apps/web/lib/business-seed.ts` and as
  `payloadJson: text` blob tables in `packages/db/src/schema.ts`
  ([schema.ts:246–280](../templates/business-basic/packages/db/src/schema.ts:246)).
- The API queues every change as a CTOX task
  ([business-runtime.ts:37](../templates/business-basic/apps/web/lib/business-runtime.ts:37));
  no row ever transitions in the database itself.
- `revenueAccount` strings on products contain SKR03 numbers
  (`"8400 SaaS subscriptions"`, `"8337 Implementation services"`) but
  there is no chart of accounts, no debit/credit, no journal, no
  ledger.
- The invoice workspace, delivery actions, document preview and PDF
  route already exist and should remain the integration point. The
  current PDF generator is a hand-rolled minimal-PDF emitter
  ([invoices/[id]/pdf/route.ts:47](../templates/business-basic/apps/web/app/api/business/invoices/[id]/pdf/route.ts:47)).
- "Beleg" appears in the German UI strings only as a synonym for
  outgoing documents (Rechnung, Angebot). No model exists for inbound
  receipts (Eingangsbelege, Quittungen, Spesenbelege) with file
  attachment, OCR or bank reconciliation.

Concretely missing for any production B2B use:

1. Double-entry bookkeeping (account tree, debit/credit, journal,
   ledger, posting service, GL/TB/P&L/BS reports).
2. Inbound receipt handling (file + OCR + posting + bank
   reconciliation).
3. German legal compliance: SKR03/SKR04, DATEV EXTF export, §14 UStG
   field validation, §19 UStG Kleinunternehmer, ZUGFeRD/X-Rechnung
   embed, GoBD-grade immutability.
4. Bank statement import (camt.053, MT940) feeding receipt and
   payment posting.
5. Continuous invoice numbering, payment entity, dunning workflow.

This RFC defines the target shape so customer-owned generated business
repos have a real bookkeeping engine and a CTOX-native operating model:
deterministic posting stays in the repo, existing Business Basic invoice
surfaces are extended rather than rebuilt, and small custom agents handle
bounded automation steps around the books.

## 2. Non-Goals

- Inventory, batches, serial numbers, point-of-sale, loyalty
  programmes, pricing rules. These are present in upstream Frappe
  Books but out of scope for "basic". A later module
  (`business-advanced`) may add them.
- Personal-finance flat-account modelling (income/expense/transfer
  with one related account). The engine is strictly double-entry.
- Replacing the CTOX queue path or the Business Basic custom-agent
  pattern. Mutations still emit a CTOX task and a sync event; the
  difference is that deterministic accounting commands can also mutate
  the books transactionally. Agentic steps produce proposals, evidence
  and follow-up actions, not unreviewed ledger mutations.
- Multi-company / multi-tenant inside one business repo. Each
  generated repo is one company. Multi-tenant is out of scope.
- A second invoice front-end or parallel PDF flow. The existing
  `InvoicesView`, invoice editor/preview, delivery actions and PDF API
  route stay the product surface. The engine is headless and plugs into
  those existing components.

## 3. Conceptual Sources (clean-room)

Every line of the engine is to be written from scratch in our stack
(TS, Drizzle, Postgres, Next.js). Two upstream projects were read for
**concept-level inspiration only**; no code, schemas, or fixtures are
copied:

- **frappe/books** (AGPLv3, Vue+Electron+SQLite, accounting domain):
  read for the standard double-entry data model
  (Account/AccountingLedgerEntry/JournalEntry/LedgerPosting), report
  shape (GL/TB/P&L/BS) and the
  reverted-not-deleted invariant. Domain concepts are
  GAAP/IFRS-standard and not copyrightable; their selection /
  arrangement / code is. We do not import their JSON schemas, COA
  fixtures, TypeScript code, or the `fyo` framework abstractions.
- **mayswind/ezbookkeeping** (MIT, Go+Vue, personal finance with
  strong importers): read for the receipt-attachment pattern, staging
  + duplicate-checker pattern and the bank-statement-converter
  architecture (camt.053 / MT940 / OFX / CSV). Their personal-finance
  account model is **not** adopted.

Because the work is clean-room, MIT vs AGPL of the source projects is
not a license issue. We do not redistribute their code. We do
acknowledge the conceptual reads in `NOTICE` of the generated
business repo.

## 4. Target Shape

### 4.1 Package layout

A new pnpm workspace package:

```
templates/business-basic/packages/accounting/
  package.json            // name: "@ctox-business/accounting"
  src/
    money.ts              // Decimal-backed Money type, currency-aware
    chart/
      types.ts            // RootType, AccountType enums
      skr03.ts            // SKR03 tree as TS constant
      skr04.ts            // SKR04 tree as TS constant
      seed.ts             // installs a chart into a fresh company
    accounts.ts           // CRUD over `accounts`, tree helpers
    parties.ts            // Customer/Vendor/Employee parties
    number-series.ts      // gap-free number generation per type/year
    posting/
      service.ts          // LedgerPosting accumulator + post/reverse
      validate.ts         // Σdebit = Σcredit; period not closed
    workflow/
      commands.ts         // SendInvoice, PostReceipt, MatchPayment, ...
      proposals.ts        // agent/user proposals before posting
      outbox.ts           // reliable CTOX sync/task emission after commit
      audit.ts            // immutable business/audit event writer
      policy.ts           // approval and auto-post rules
    invoice/
      validate.ts         // §14 UStG mandatory fields
      poster.ts           // existing BusinessInvoice → JournalEntry on send
      pdf.ts              // enrich existing PDF route with ZUGFeRD/PDF-A
      delivery.ts         // email / portal / print delivery hooks
    receipt/
      types.ts
      ingest.ts           // file + extract via CTOX OCR queue
      poster.ts           // Receipt → JournalEntry on review
    payment/
      poster.ts           // Payment → JournalEntry, settles invoice
      reconciler.ts       // bank line ↔ invoice / receipt match
    bank-import/
      types.ts
      camt053.ts          // ISO 20022 SEPA daily statement parser
      mt940.ts            // SWIFT MT940 parser
      csv.ts              // generic + bank-specific CSVs
      duplicate-checker.ts
    dunning/
      runner.ts           // levels 1..3, configurable letters/fees
    export/
      datev-extf.ts       // DATEV EXTF Buchungsstapel CSV
      lexoffice.ts        // existing enum, real impl deferred
    reports/
      general-ledger.ts   // SQL view + TS query helper
      trial-balance.ts
      profit-and-loss.ts
      balance-sheet.ts
    fiscal/
      period.ts           // open/closed periods, GoBD lock
    agents/
      manifest.ts         // micro custom agent contracts for this module
      receipt-extractor.ts
      invoice-checker.ts
      bank-reconciler.ts
      dunning-assistant.ts
    tests/
      ...                 // vitest, against an in-memory PG via testcontainers
```

The package is consumed by `apps/web` and exported as part of the
generated business repo (it is customer-owned code, like the rest of
the template).

### 4.2 Agentic operating model

Business Basic follows the same system shape as the Sales module: the
generated repo owns the business domain, and CTOX provides orchestration,
context, scheduling and user-facing automation. Accounting therefore has
three layers:

1. **Deterministic core.** The accounting package owns Money, accounts,
   invoices, receipts, payments, journals, reports and posting rules. It
   is the only layer allowed to create journal entries and ledger rows.
2. **Command API.** UI actions, API routes and agents call explicit
   commands (`SendInvoice`, `PostReceipt`, `ImportBankStatement`,
   `AcceptBankMatch`, `RunDunning`, `ExportDatev`). Commands are
   idempotent, validated and transaction-scoped.
3. **Micro custom agents.** CTOX tasks run narrow, replaceable agents
   that enrich or propose work: extract a receipt, validate an invoice,
   suggest a bank match, prepare a dunning letter, explain a report, or
   prepare a DATEV export checklist.

Agents do not post directly by default. They write
`accounting_proposals` with source evidence, confidence, diffs and a
recommended command. A posting policy decides whether the proposal can
auto-apply or needs human review. In early milestones the default is
review-before-post for anything that touches the ledger; later customer
repos may opt into auto-posting for low-risk classes such as exact bank
matches.

```ts
accounting_proposals (
  id, company_id, kind,                 -- "receipt_extraction" | "bank_match" | "invoice_check" | ...
  status,                               -- "open" | "accepted" | "rejected" | "superseded" | "auto_applied"
  ref_type, ref_id,
  proposed_command,                     -- JSON command payload
  evidence_json, confidence,
  created_by_agent, created_at,
  decided_by, decided_at,
  resulting_journal_entry_id
)

business_outbox_events (
  id, company_id, topic, payload_json,
  status, attempts, created_at, delivered_at
)

accounting_audit_events (
  id, company_id, actor_type, actor_id,
  action, ref_type, ref_id,
  before_json, after_json, created_at
)
```

`business_outbox_events` is written in the same transaction as a
successful command and delivered after commit by the existing Business
runtime. This avoids the failure mode where books are posted but the
CTOX task/sync event was never recorded.

### 4.3 Existing invoice and PDF integration

The current Business Basic invoice module is the starting point, not a
throwaway prototype. These pieces stay in place and are hardened:

- `apps/web/components/business-workspace.tsx::InvoicesView` remains the
  invoice workspace, including list, editor, preview and drawers.
- `invoice-customer-editor`, `invoice-lines-editor`,
  `invoice-document-selector`, `invoice-delivery-actions` remain the UI
  building blocks.
- `app/api/business/invoices/[id]/pdf/route.ts` remains the public PDF
  endpoint used by the app and by customers.
- The `business/invoices` resource namespace remains stable so CTOX
  context links, Sales-to-Business handoff and existing deep links keep
  working.

The accounting package adds a domain adapter around the existing
`BusinessInvoice` shape:

```ts
type InvoiceDraft = BusinessInvoice;

function validateInvoiceForSend(invoice: InvoiceDraft, context: InvoiceContext): ValidationResult;
function postBusinessInvoice(tx: DbTx, invoice: InvoiceDraft): JournalEntryId;
function buildInvoiceDocument(invoice: InvoiceDraft, context: InvoiceContext): InvoiceDocument;
function renderInvoicePdf(document: InvoiceDocument, options: PdfOptions): Uint8Array;
function buildZugferdXml(invoice: InvoiceDraft, context: InvoiceContext): string;
```

M2 therefore evolves the existing invoice send/PDF path:

1. Draft editing continues through the current invoice UI.
2. `SendInvoice` validates the existing invoice payload, allocates the
   official number when needed, writes first-class invoice rows, posts
   the journal entry and records outbox/audit events.
3. The existing PDF route calls the shared document builder and renderer.
   The renderer can move from the inline minimal emitter to a library
   implementation, but the route contract and UI entry points stay.
4. ZUGFeRD/X-Rechnung is an attachment/enrichment step on that existing
   PDF flow, not a second document generator.

### 4.4 Money

All amounts are `Money` values, never JS `number`. Internally a
fixed-point integer in the smallest unit of the currency (Euro
cents). Arithmetic via a Decimal library (e.g. `dinero.js`,
`decimal.js`, both MIT). Never floor/round implicitly; rounding only
in display formatters.

### 4.5 Chart of accounts

Two enums (closed sets):

```ts
export type RootType = "asset" | "liability" | "equity" | "income" | "expense";

export type AccountType =
  | "bank" | "cash" | "receivable" | "payable" | "tax"
  | "fixed_asset" | "depreciation" | "accumulated_depreciation"
  | "cogs" | "income" | "expense" | "round_off"
  | "stock" | "stock_adjustment" | "temporary";
```

Tree shape: `accounts(id, code, name, root_type, account_type,
parent_id, is_group, currency, company_id)`. Leaves take ledger
entries; groups do not. `code` is the SKR account number for SKR
charts.

Two ship-with charts: SKR03 (default; small business / GmbH) and
SKR04 (alternative). Account titles and codes are public domain.

### 4.6 Ledger and journal

```ts
journal_entries (
  id, company_id, posting_date, type,           -- "invoice" | "payment" | "receipt" | "manual" | "fx" | "depreciation" | "reverse"
  ref_type, ref_id,                             -- e.g. "invoice", inv-id
  number,                                       -- gap-free number from number_series
  narration, created_by, created_at,
  reversed_by_id,                               -- if non-null, this entry was reversed by another
  posted_at                                     -- non-null = GoBD-locked
)

journal_entry_lines (
  id, journal_entry_id,
  account_id, party_id,
  debit, credit,                                -- Money; exactly one is zero
  cost_center_id, project_id,                   -- optional analytics dims
  line_no
)

accounting_ledger_entries (
  id, company_id, posting_date,
  account_id, party_id,
  debit, credit,                                -- denormalised projection of journal_entry_lines
  ref_type, ref_id,
  journal_entry_id,
  reverted, reverts_id,                         -- storno backlink
  created_at
)
```

`accounting_ledger_entries` is **append-only**. Reversal does not
delete; it inserts a counter-entry with `reverts_id` set. Once
`posted_at` is set on a `journal_entries` row, it and its lines are
read-only. This implements GoBD-Festschreibung at the database level.

### 4.7 Posting service

```ts
class LedgerPosting {
  constructor(company: CompanyId, refType: string, refId: string, postingDate: Date);
  debit(account: AccountId, amount: Money, party?: PartyId): this;
  credit(account: AccountId, amount: Money, party?: PartyId): this;
  validate(): void;                  // throws if Σdebit ≠ Σcredit
  post(tx: DbTx, narration?: string): JournalEntryId;
  postReverse(tx: DbTx, originalEntryId: JournalEntryId): JournalEntryId;
}
```

Always called inside a Drizzle transaction. Validates the period is
open. Computes the gap-free entry number from `number_series` inside
the same transaction with `SELECT … FOR UPDATE` on the series row.

### 4.8 Parties, taxes, number series

```ts
parties (
  id, company_id, kind,             -- "customer" | "vendor" | "employee"
  name, tax_id, vat_id,             -- Steuernummer, USt-IdNr.
  default_receivable_account_id,
  default_payable_account_id,
  ...
)

tax_rates (
  id, company_id, code,             -- "DE_19", "DE_7", "DE_0", "DE_RC", "DE_KU"
  rate, account_id,                 -- 1776 / 3806 etc.
  type                              -- "output" | "input" | "reverse_charge" | "kleinunternehmer"
)

number_series (
  id, company_id, key,              -- "invoice", "receipt", "credit_note", "journal", "dunning"
  fiscal_year, prefix, next_value
)
```

§14 UStG and §22 UStG mandate gap-free, immutable numbering on
outgoing invoices. Fiscal year is part of the series key so resets
are explicit at year-end.

### 4.9 Invoices (outgoing)

This section describes the durable accounting-backed invoice model that
the existing Business Basic invoice module writes to and reads from. It
does not introduce a separate invoice product surface.

```ts
invoices (
  id, company_id, customer_id,
  number,                               -- from number_series, immutable once "sent"
  status,                               -- "draft" | "sent" | "paid" | "partially_paid" | "void" | "cancelled"
  issue_date, service_date, due_date,
  currency, fx_rate_to_eur,
  net_amount, tax_amount, total_amount, balance_due,
  delivery_channel, delivered_at,
  zugferd_xml,                          -- inline XML for embedding
  pdf_blob_ref,
  payment_terms_text, intro_text, closing_text,
  reverse_charge, kleinunternehmer,
  created_at, sent_at, posted_journal_entry_id
)

invoice_lines (
  id, invoice_id, line_no,
  product_id, description,
  quantity, unit_price, line_net,
  tax_rate_id, tax_amount, line_total,
  revenue_account_id                    -- defaults from product, override per line
)
```

Lifecycle:

1. **Draft.** Editable, no number assigned, no posting.
2. **Send.** Validate §14 UStG fields (`invoice/validate.ts`).
   Allocate number from `number_series`. Generate ZUGFeRD/X-Rechnung
   XML and embed in PDF/A-3. Post the journal entry:
   - debit `customer.default_receivable_account_id` total
   - credit each `line.revenue_account_id` line_net
   - credit `tax_rate.account_id` tax_amount per rate
3. **Pay** (one or many). Posted via `payment/poster.ts` (§4.12).
4. **Cancel.** Allowed only via Storno-Gutschrift (cancellation credit
   note): a separate `credit_notes` row of type cancellation that
   reverses the original journal entry and emits its own number.

§19 UStG Kleinunternehmer is a per-company flag. When set, no tax is
calculated and the PDF carries the mandated note; an empty
`tax_amount` is allowed.

### 4.10 Receipts (incoming)

New domain. Distinct concept from "Beleg" in the existing UI strings,
which means outgoing document; we therefore use `receipts` and
"Eingangsbeleg" in the UI to avoid collision.

```ts
receipts (
  id, company_id, vendor_party_id,
  number,                               -- our internal number from number_series "receipt"
  vendor_invoice_number,                -- as printed on the receipt
  status,                               -- "scanned" | "extracted" | "reviewed" | "posted" | "rejected"
  receipt_date, due_date,
  currency, net_amount, tax_amount, total_amount,
  expense_account_id,                   -- assigned during review
  tax_rate_id,
  ocr_text, extracted_json,             -- whatever the OCR step returned
  posted_journal_entry_id,
  created_at, reviewed_at, posted_at
)

receipt_files (
  id, receipt_id,
  blob_ref, mime, original_filename, sha256,
  uploaded_at
)

receipt_lines (
  id, receipt_id, line_no,
  description, expense_account_id,
  net_amount, tax_rate_id, tax_amount, total_amount
)
```

OCR / extraction runs through CTOX as a queued task
(`receipt/ingest.ts` enqueues, the result populates
`extracted_json`). The user reviews proposed lines, picks expense
account(s) and tax rate(s), and confirms. On post:

- debit each `line.expense_account_id` line_net
- debit `tax_rate.account_id` (input VAT) tax_amount
- credit `vendor.default_payable_account_id` total

Files live in CTOX storage. Hash + original filename are kept for
GoBD audit.

### 4.11 Bank statement import

```ts
bank_statements (
  id, company_id, account_id,           -- our bank account in the chart
  format,                               -- "camt053" | "mt940" | "csv"
  imported_at, imported_by,
  source_filename, source_sha256,
  start_date, end_date, opening_balance, closing_balance
)

bank_statement_lines (
  id, statement_id, line_no,
  booking_date, value_date, amount, currency,
  remitter_name, remitter_iban, purpose, end_to_end_ref,
  match_status,                         -- "unmatched" | "suggested" | "matched" | "ignored"
  matched_journal_entry_id,
  duplicate_of_line_id
)
```

Importers in `bank-import/{camt053,mt940,csv}.ts` parse to a common
`BankStatementLine[]` shape. The duplicate-checker hashes
(booking_date, amount, end_to_end_ref || (remitter_iban + purpose))
and rejects re-imports.

The reconciler proposes matches:

- exact: amount equals an open invoice's `balance_due` and purpose
  contains the invoice number → propose a `payment` posting.
- partial: amount ≤ balance_due → propose partial payment.
- unknown debit → propose a posting against a holding account, leave
  for manual review.

Confirmation triggers `payment/poster.ts`.

### 4.12 Payments

```ts
payments (
  id, company_id, party_id,
  kind,                                 -- "incoming" | "outgoing"
  payment_date, amount, currency,
  bank_account_id,                      -- account in chart
  bank_statement_line_id,
  posted_journal_entry_id,
  created_at
)

payment_allocations (
  id, payment_id,
  invoice_id, receipt_id,               -- exactly one set
  amount
)
```

Posting an incoming payment that settles invoice `I`:

- debit bank account total
- credit `customer.default_receivable_account_id` total

Posting an outgoing payment that settles receipt `R`:

- debit `vendor.default_payable_account_id` total
- credit bank account total

Allocations split across multiple invoices/receipts when needed. The
invoice/receipt status follows from `Σ allocations vs total_amount`.

### 4.13 Dunning

```ts
dunning_runs (
  id, company_id, run_date, level,      -- 1 | 2 | 3
  invoice_id, fee_amount, letter_blob_ref,
  posted_journal_entry_id,              -- if fee was booked
  delivered_at
)
```

Triggered manually or by a CTOX scheduled task. Level escalates with
days-overdue thresholds configured per company. Optional fee booking
posts a small entry against a dunning-fee revenue account.

### 4.14 Reports

All four reports are stateless TS queries against
`accounting_ledger_entries`, with optional date range and company
filter. Implemented as plain `drizzle-orm` queries + small
post-processing for the tree layout:

- **General Ledger** — for an account, all entries in date range
  with running balance.
- **Trial Balance** — Σdebit and Σcredit per account in date range.
- **Profit and Loss** — sum income vs sum expense, grouped by parent
  → root_type subtree.
- **Balance Sheet** — assets vs liabilities + equity at a date,
  grouped by tree.

Caching: not in v1. If query times become an issue, add a materialised
monthly-balance table later.

### 4.15 DATEV EXTF export

DATEV EXTF "Buchungsstapel" v7.0 is a public CSV format
(documentation at `https://developer.datev.de`). `export/datev-extf.ts`
streams `accounting_ledger_entries` for a period to a CSV that DATEV
imports as journal entries. Required header fields (consultant
number, client number, fiscal year start, account length, etc.)
become `accounting_settings(company_id, …)` configuration.

### 4.16 ZUGFeRD / X-Rechnung

Outgoing invoices PDF/A-3 with embedded XML per ZUGFeRD 2.x
(EN 16931). For each invoice we render the visual PDF, build the XML
from the line data, and embed via PDF/A-3 attachment. Library choice:
`node-zugferd` or equivalent MIT package; selection deferred to
implementation. The existing PDF endpoint and document-building flow in
[invoices/[id]/pdf/route.ts:47](../templates/business-basic/apps/web/app/api/business/invoices/[id]/pdf/route.ts:47)
is retained; its internals are upgraded so the same route can emit a
compliant visual PDF and embedded XML.

## 5. Schema migration from today's `business-basic`

The existing invoice module is migrated in place. The `business/invoices`
resource, route names, UI components and PDF endpoint remain stable.
The JSON-blob `business_invoices` table becomes a compatibility/staging
surface during rollout, while first-class invoice, line, journal and
ledger tables become the accounting source of truth after send/post.
Migration steps in the customer repo:

1. New tables under §4.2 and §4.5–4.13 added via Drizzle migration.
2. Existing invoice payloads are backfilled into the first-class invoice
   tables where possible; drafts that cannot be posted remain editable
   through the current invoice UI until the user fixes validation
   findings.
3. `business-seed.ts` keeps its `BusinessBundle` type as **demo-data
   generator and compatibility fixture only**, no longer the source of
   truth for posted accounting state. The seed loader inserts demo
   customers + products + a handful of invoices and receipts through the
   existing invoice module and then sends/posts them via the real engine,
   so the seed exercises the same paths as production.
4. `business-runtime.ts::queueBusinessMutation` becomes the delivery
   side of the new business outbox. Write paths execute an accounting
   command inside a transaction, append audit/outbox rows, then deliver
   CTOX tasks and sync events after commit. Agent-originated work lands
   as an `accounting_proposals` row unless the posting policy explicitly
   permits auto-apply.
5. The existing PDF route remains at
   `/api/business/invoices/[id]/pdf` and imports shared helpers from
   `accounting/invoice/pdf.ts`.

`apps/web/app/app/business/` views are extended in place:
`/invoices` remains the invoice workspace, `/receipts` is added for
incoming documents, and `/bookkeeping` hosts journals, ledger, reports
and DATEV export. The existing top-level `/customers`, `/products` and
`/reports` stay.

## 6. CTOX integration

The engine is a customer-owned package. CTOX interacts with it the
same way Sales does: through record context, queue tasks, scheduled
tasks, sync events and small custom agents that live with the generated
business repo.

The important boundary is:

- CTOX can inspect records, run skills, schedule work, explain state,
  draft communications and submit proposals.
- The accounting package alone validates and posts accounting commands.
- The Business runtime owns reliable delivery between committed
  accounting state and CTOX-visible tasks/events.

Micro-agent contracts are declared in `accounting/src/agents/manifest.ts`
and are intentionally narrow:

| Agent | Trigger | Writes | Human gate |
| --- | --- | --- | --- |
| `receipt-extractor` | receipt file uploaded | extracted fields + proposal | yes |
| `invoice-checker` | draft invoice before send | validation findings + proposal | yes |
| `bank-reconciler` | bank statement imported | match proposals | configurable |
| `dunning-assistant` | scheduled overdue scan | dunning-run proposal + letter draft | yes |
| `datev-exporter` | month-end schedule | export checklist + outbox event | yes |
| `report-explainer` | user asks CTOX on a report | narrative answer, no mutation | no |

Concrete flows:

1. **Context prompt.** Right-click "Prompt CTOX" on an invoice /
   receipt / journal entry generates a queue task with
   `data-context-*` set to that record. If the agent suggests a change,
   it writes an `accounting_proposals` row.
2. **Receipt OCR.** `receipt/ingest.ts` stores file metadata and emits
   an outbox event. The CTOX OCR task runs `receipt-extractor`; the
   result is accepted through `/api/business/receipts/[id]/extracted`
   and transitions `scanned → extracted`.
3. **Bank reconciliation.** Import is deterministic. The
   `bank-reconciler` agent ranks ambiguous matches and writes proposals.
   Exact matches may auto-apply only when `posting_policy` permits it.
4. **Dunning.** A scheduled CTOX task runs `dunning-assistant`, which
   drafts the letter and proposes `RunDunning`; posting fees and sending
   letters remain command/API operations.
5. **Exports.** Month-end DATEV automation prepares a checklist and
   export proposal. The user accepts, then `ExportDatev` writes the
   immutable export record and outbox event.

No CTOX-core change is required by this RFC. The new code belongs to
the generated business repo's accounting package, API routes and
Business runtime.

## 7. Testing

- Unit tests in `packages/accounting/src/tests/` against an
  ephemeral Postgres (`testcontainers`). Each domain module ships
  worked examples (a B2B services company, a Kleinunternehmer, a
  reverse-charge EU invoice).
- Property tests for posting: for any random sequence of valid
  invoice/payment/receipt/reverse calls, Σdebit ≡ Σcredit on the
  ledger and every report sums correctly.
- Workflow tests cover command idempotency, proposal accept/reject,
  outbox delivery retry and the rule that agents cannot create ledger
  rows without going through an accepted command.
- A smoke test extends the existing
  `pnpm test:business-stack` to import a sample camt.053 file, post
  a sample receipt with a fixture image, generate a ZUGFeRD invoice
  PDF, and emit a DATEV EXTF CSV.

## 8. Rollout

1. **M0 — Workflow spine.** Commands, proposals, audit events,
   business outbox, posting policy and agent manifest. No accounting
   UI changes, but the architecture matches the Sales-style CTOX
   custom-agent model from the start.
2. **M1 — Engine.** Money, chart, accounts, parties, number series,
   posting service, journal/ledger tables. SKR03 ships, SKR04 stub.
   No UI changes. Fully tested headlessly.
3. **M2 — Outgoing.** Existing invoice workspace connected to
   `SendInvoice`, invoice posting on send, ZUGFeRD/X-Rechnung
   enrichment for the current PDF route, payment posting,
   GL/TB/P&L/BS reports, DATEV EXTF export. `/bookkeeping` is connected
   to the engine.
4. **M3 — Incoming.** Receipts (file + OCR + post), payments outgoing,
   `receipt-extractor` agent, `/app/business/receipts` UI.
5. **M4 — Bank import.** camt.053 + MT940 parsers, reconciler,
   `bank-reconciler` agent and match-suggest UI inside
   `/app/business/bookkeeping`.
6. **M5 — Dunning + UX polish.** Levels 1–3, fee posting, letters,
   `dunning-assistant`, period close UI, GoBD-Festschreibung audit log.

Each milestone is independently shippable and ships behind a
`CTOX_BUSINESS_BOOKKEEPING_ENABLED` flag in the generated repo's
`.env` (template default off until M2). The flag belongs to the
generated repo's app config, not to CTOX core's `runtime_env_kv`.

## 9. Compliance notes (Germany-first)

- **§14 UStG**: invoice validator enforces full set of mandatory
  fields before number allocation.
- **§19 UStG (Kleinunternehmer)**: company flag suppresses tax
  calculation and adds the mandated note on the PDF; tax accounts
  are not touched on those invoices.
- **§14a UStG / Reverse Charge**: per-line flag plus EU-customer
  detection drives the dedicated `DE_RC` tax code.
- **§14b UStG (8-year invoice retention as of the current statute)**:
  PDF blobs are content-addressed via sha256 in `pdf_blob_ref`; the
  storage layer must not garbage-collect referenced blobs. Other
  commercial/tax retention duties can still require longer retention
  for adjacent accounting records, so customer policy may choose a
  longer storage horizon.
- **§22 UStG / GoBD**: number series are gap-free per fiscal year and
  type, posted journal entries are read-only at the DB level
  (implemented with database triggers / permissions, not application
  convention), reversal is via storno entry with `reverts_id` backlink.
- **GoBD Verfahrensdokumentation**: out of scope for the engine;
  belongs in the customer's compliance documentation.

## 10. Open questions

1. Library choices: Decimal (`dinero.js` vs `decimal.js`), ZUGFeRD
   (`node-zugferd` vs hand-rolled XML + PDF/A-3 embed), PDF internals
   for the existing route (`pdf-lib` vs `@react-pdf/renderer` vs
   hardening the current minimal emitter). Decision in M2 prep.
2. Storage of receipt files and PDF blobs: reuse CTOX's existing
   blob storage, or keep in Postgres `bytea`, or push to S3-like
   external. Default: CTOX storage (consistent with other
   customer-owned blobs); revisit at M3.
3. Cost-center / project analytic dimensions: shipped in the line
   schema (§4.5) but unused in v1 reports. Decide at M2 whether to
   expose them in the GL UI or defer.
4. Multi-currency P&L. v1 reports in company base currency only;
   conversion happens at posting time via `fx_rate_to_eur` snapshot.
   Confirm acceptable for typical SaaS use.
5. SKR04 ship-along: ship in M1 as a stub or wait until first
   customer asks.
6. Posting policy defaults: decide which proposal classes may auto-apply
   in template defaults. Initial recommendation: none for ledger-touching
   commands; exact bank matches can be enabled per customer repo after
   pilot use.
