import { collections as accountingCollections } from '../buchhaltung/schema.js';
import { collections as customerCollections } from '../customers/schema.js';
import { collections as desktopCollections } from '../desktop/schema.js';

// invoices/schema.js — RxDB collection schemas for the invoices module.
// All schemas follow the customers convention (cent-integer, *_ms timestamps,
// is_deleted for soft-delete, search_text for full-text, payload for free-form
// extensions). Persistence happens via business_commands; UI never writes
// these collections directly.

const commandSchema = {
  version: 1,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    command_id: { type: 'string' },
    module: { type: 'string' },
    command_type: { type: 'string' },
    record_id: { type: 'string' },
    status: { type: 'string' },
    inbound_channel: { type: 'string' },
    payload: { type: 'object', additionalProperties: true },
    client_context: { type: 'object', additionalProperties: true },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'command_id', 'module', 'command_type', 'status', 'updated_at_ms'],
  indexes: ['module', 'command_type', 'status', 'updated_at_ms'],
  additionalProperties: true
};

const invoiceSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    invoice_number: { type: 'string' },
    invoice_type: { type: 'string' }, // 'sale_out' | 'sale_in' | 'credit_note_out' | 'credit_note_in' | 'recurring_template'
    party_id: { type: 'string' }, // FK -> customer_accounts.id
    party_snapshot: { type: 'object', additionalProperties: true },
    invoice_date_ms: { type: 'number' },
    due_date_ms: { type: 'number' },
    service_period_start_ms: { type: 'number' },
    service_period_end_ms: { type: 'number' },
    currency: { type: 'string' },
    subtotal_cents: { type: 'number' },
    tax_cents: { type: 'number' },
    total_cents: { type: 'number' },
    paid_cents: { type: 'number' },
    open_cents: { type: 'number' },
    tax_breakdown: { type: 'array', items: { type: 'object', additionalProperties: true } },
    payment_terms_id: { type: 'string' },
    skonto_percent: { type: 'number' },
    skonto_days: { type: 'number' },
    state: { type: 'string' }, // 'draft' | 'posted' | 'partially_paid' | 'paid' | 'overdue' | 'cancelled' | 'credited'
    state_changed_at_ms: { type: 'number' },
    state_changed_by_command_id: { type: 'string' },
    linked_invoice_id: { type: 'string' },
    reverse_charge: { type: 'boolean' },
    small_business: { type: 'boolean' },
    eu_ic_supply: { type: 'boolean' },
    xrechnung_xml: { type: 'string' },
    pdf_attachment_id: { type: 'string' },
    post_journal_entry_id: { type: 'string' },
    cancel_journal_entry_id: { type: 'string' },
    credit_note_for_id: { type: 'string' },
    dunning_level: { type: 'number' },
    last_dunning_run_id: { type: 'string' },
    proposal_status: { type: 'string' },
    draft_status: { type: 'string' },
    approval_status: { type: 'string' },
    approval_id: { type: 'string' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
    is_deleted: { type: 'boolean' },
    deleted_at_ms: { type: 'number' },
    search_text: { type: 'string' },
    payload: { type: 'object', additionalProperties: true }
  },
  required: ['id', 'invoice_type', 'state', 'currency', 'search_text', 'is_deleted', 'created_at_ms', 'updated_at_ms'],
  indexes: ['party_id', 'state', 'invoice_date_ms', 'due_date_ms', 'invoice_number', 'updated_at_ms'],
  additionalProperties: true
};

const invoiceLineSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    invoice_id: { type: 'string' },
    position: { type: 'number' },
    description: { type: 'string' },
    article_number: { type: 'string' },
    quantity: { type: 'number' },
    unit: { type: 'string' },
    unit_price_cents: { type: 'number' },
    discount_percent: { type: 'number' },
    tax_rate: { type: 'number' },
    line_net_cents: { type: 'number' },
    line_tax_cents: { type: 'number' },
    line_gross_cents: { type: 'number' },
    account_code: { type: 'string' },
    cost_center_id: { type: 'string' },
    project_id: { type: 'string' },
    service_period_start_ms: { type: 'number' },
    service_period_end_ms: { type: 'number' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'invoice_id', 'position', 'updated_at_ms'],
  indexes: ['invoice_id'],
  additionalProperties: true
};

const paymentTermsSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    name: { type: 'string' },
    net_days: { type: 'number' },
    skonto_percent: { type: 'number' },
    skonto_days: { type: 'number' },
    description: { type: 'string' },
    is_default: { type: 'boolean' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'name', 'net_days', 'is_default', 'updated_at_ms'],
  additionalProperties: true
};

const creditNoteSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    invoice_id: { type: 'string' },
    credit_note_invoice_id: { type: 'string' },
    reason: { type: 'string' },
    reason_text: { type: 'string' },
    delta_net_cents: { type: 'number' },
    delta_tax_cents: { type: 'number' },
    delta_gross_cents: { type: 'number' },
    corrective_invoice_number: { type: 'string' },
    created_at_ms: { type: 'number' }
  },
  required: ['id', 'invoice_id', 'credit_note_invoice_id', 'created_at_ms'],
  indexes: ['invoice_id'],
  additionalProperties: true
};

const paymentSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    payment_date_ms: { type: 'number' },
    party_id: { type: 'string' },
    amount_cents: { type: 'number' },
    currency: { type: 'string' },
    method: { type: 'string' }, // 'bank_transfer' | 'sepa_direct_debit' | 'cash' | 'card' | 'other'
    reference: { type: 'string' },
    bank_statement_line_id: { type: 'string' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
    payload: { type: 'object', additionalProperties: true }
  },
  required: ['id', 'payment_date_ms', 'amount_cents', 'currency', 'created_at_ms', 'updated_at_ms'],
  indexes: ['party_id', 'payment_date_ms'],
  additionalProperties: true
};

const paymentAllocationSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    payment_id: { type: 'string' },
    invoice_id: { type: 'string' },
    allocated_cents: { type: 'number' },
    skonto_cents: { type: 'number' },
    note: { type: 'string' },
    allocated_at_ms: { type: 'number' },
    allocated_by_command_id: { type: 'string' }
  },
  required: ['id', 'payment_id', 'invoice_id', 'allocated_cents', 'allocated_at_ms'],
  indexes: ['payment_id', 'invoice_id'],
  additionalProperties: true
};

const dunningRunSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    run_date_ms: { type: 'number' },
    run_by: { type: 'string' },
    filter: { type: 'object', additionalProperties: true },
    invoices_total: { type: 'number' },
    letters_sent: { type: 'number' },
    state: { type: 'string' }, // 'draft' | 'approved' | 'executed'
    created_at_ms: { type: 'number' },
    payload: { type: 'object', additionalProperties: true }
  },
  required: ['id', 'run_date_ms', 'invoices_total', 'state', 'created_at_ms'],
  additionalProperties: true
};

const dunningLetterSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    dunning_run_id: { type: 'string' },
    invoice_id: { type: 'string' },
    level: { type: 'number' }, // 1 | 2 | 3
    letter_date_ms: { type: 'number' },
    fee_cents: { type: 'number' },
    interest_cents: { type: 'number' },
    total_cents: { type: 'number' },
    pdf_attachment_id: { type: 'string' },
    sent_via: { type: 'string' }, // 'print' | 'email' | 'postal'
    sent_at_ms: { type: 'number' },
    status: { type: 'string' }, // 'draft' | 'sent' | 'delivered' | 'returned'
    created_at_ms: { type: 'number' }
  },
  required: ['id', 'dunning_run_id', 'invoice_id', 'level', 'letter_date_ms', 'total_cents', 'status', 'created_at_ms'],
  indexes: ['invoice_id', 'dunning_run_id'],
  additionalProperties: true
};

const recurringInvoiceSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    template_invoice_id: { type: 'string' },
    interval: { type: 'string' }, // 'monthly' | 'quarterly' | 'yearly'
    interval_count: { type: 'number' },
    start_at_ms: { type: 'number' },
    end_at_ms: { type: 'number' },
    next_run_at_ms: { type: 'number' },
    last_run_at_ms: { type: 'number' },
    auto_send: { type: 'boolean' },
    active: { type: 'boolean' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' },
    payload: { type: 'object', additionalProperties: true }
  },
  required: ['id', 'template_invoice_id', 'interval', 'interval_count', 'start_at_ms', 'auto_send', 'active', 'created_at_ms', 'updated_at_ms'],
  additionalProperties: true
};

const invoiceAttachmentSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    invoice_id: { type: 'string' },
    kind: { type: 'string' }, // 'pdf' | 'xrechnung' | 'correction' | 'other'
    desktop_file_id: { type: 'string' },
    sha256: { type: 'string' },
    size_bytes: { type: 'number' },
    created_at_ms: { type: 'number' }
  },
  required: ['id', 'invoice_id', 'kind', 'desktop_file_id', 'created_at_ms'],
  indexes: ['invoice_id'],
  additionalProperties: true
};

const invoiceApprovalSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 160 },
    invoice_id: { type: 'string' },
    revision_id: { type: 'string' },
    actor_user_id: { type: 'string' },
    decision: { type: 'string' }, // 'approved' | 'rejected' | 'request_changes'
    note: { type: 'string' },
    created_at_ms: { type: 'number' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'invoice_id', 'decision', 'created_at_ms', 'updated_at_ms'],
  indexes: ['invoice_id'],
  additionalProperties: true
};

export const collections = {
  business_commands: commandSchema,
  customer_accounts: customerCollections.customer_accounts,
  customer_activities: customerCollections.customer_activities,
  accounting_accounts: accountingCollections.accounting_accounts,
  accounting_journal_entries: accountingCollections.accounting_journal_entries,
  accounting_journal_entry_lines: accountingCollections.accounting_journal_entry_lines,
  accounting_ledger_entries: accountingCollections.accounting_ledger_entries,
  accounting_receipts: accountingCollections.accounting_receipts,
  accounting_bank_statement_lines: accountingCollections.accounting_bank_statement_lines,
  accounting_number_series: accountingCollections.accounting_number_series,
  desktop_files: desktopCollections.desktop_files,
  desktop_file_chunks: desktopCollections.desktop_file_chunks,
  accounting_invoices: invoiceSchema,
  accounting_invoice_lines: invoiceLineSchema,
  accounting_payment_terms: paymentTermsSchema,
  accounting_credit_notes: creditNoteSchema,
  accounting_payments: paymentSchema,
  accounting_payment_allocations: paymentAllocationSchema,
  accounting_dunning_runs: dunningRunSchema,
  accounting_dunning_letters: dunningLetterSchema,
  accounting_recurring_invoices: recurringInvoiceSchema,
  accounting_invoice_attachments: invoiceAttachmentSchema,
  accounting_invoice_approvals: invoiceApprovalSchema
};

export const migrationStrategies = {};
