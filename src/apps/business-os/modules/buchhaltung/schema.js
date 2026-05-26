const accountSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    code: { type: 'string' },
    name: { type: 'string' },
    root_type: { type: 'string' }, // asset, liability, equity, revenue, expense
    account_type: { type: 'string' }, // bank, cash, receivable, payable, expense, revenue, tax, regular
    parent_id: { type: 'string' },
    is_group: { type: 'boolean' },
    tax_rate_id: { type: 'string' },
    skr: { type: 'string' }, // SKR03 or SKR04
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'code', 'name', 'root_type', 'account_type', 'is_group', 'skr', 'updated_at_ms'],
  additionalProperties: true
};

const journalEntrySchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    posting_date: { type: 'string' },
    type: { type: 'string' }, // invoice, bank, journal, depreciation, storno
    ref_type: { type: 'string' },
    ref_id: { type: 'string' },
    number: { type: 'string' }, // standard transaction reference number
    narration: { type: 'string' },
    posted_at: { type: 'number' }, // ms timestamp: when set, doc is strictly GoBD-immutable
    reversed_by_id: { type: 'string' }, // points to storno document, if any
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'posting_date', 'type', 'number', 'updated_at_ms'],
  additionalProperties: true
};

const journalEntryLineSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    journal_entry_id: { type: 'string' },
    account_id: { type: 'string' },
    debit: { type: 'number' }, // in cents
    credit: { type: 'number' }, // in cents
    party_id: { type: 'string' },
    tax_rate_id: { type: 'string' },
    line_no: { type: 'number' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'journal_entry_id', 'account_id', 'debit', 'credit', 'line_no', 'updated_at_ms'],
  additionalProperties: true
};

const ledgerEntrySchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    journal_entry_line_id: { type: 'string' },
    journal_entry_id: { type: 'string' },
    posting_date: { type: 'string' },
    account_id: { type: 'string' },
    debit: { type: 'number' },
    credit: { type: 'number' },
    narration: { type: 'string' },
    posted_at: { type: 'number' },
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'journal_entry_line_id', 'journal_entry_id', 'posting_date', 'account_id', 'debit', 'credit', 'updated_at_ms'],
  additionalProperties: true
};

const receiptSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    file_storage_url: { type: 'string' }, // CTOX File-Storage path
    filename: { type: 'string' },
    supplier_name: { type: 'string' },
    invoice_date: { type: 'string' },
    invoice_number: { type: 'string' },
    vat_id: { type: 'string' },
    net_amount: { type: 'number' }, // cents
    tax_amount: { type: 'number' }, // cents
    gross_amount: { type: 'number' }, // cents
    suggested_account_id: { type: 'string' },
    status: { type: 'string' }, // draft, proposed, posted, ignored
    ocr_raw: { type: 'string' }, // serialized raw json details
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'file_storage_url', 'filename', 'status', 'updated_at_ms'],
  additionalProperties: true
};

const bankStatementSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    account_number: { type: 'string' }, // IBAN
    bank_code: { type: 'string' }, // BIC
    statement_number: { type: 'string' },
    start_date: { type: 'string' },
    end_date: { type: 'string' },
    start_balance: { type: 'number' }, // cents
    end_balance: { type: 'number' }, // cents
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'account_number', 'updated_at_ms'],
  additionalProperties: true
};

const bankStatementLineSchema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: {
    id: { type: 'string', maxLength: 128 },
    statement_id: { type: 'string' },
    value_date: { type: 'string' },
    narration: { type: 'string' }, // Verwendungszweck
    amount: { type: 'number' }, // in cents, + or -
    counterparty_name: { type: 'string' },
    counterparty_iban: { type: 'string' },
    reconciled_entry_id: { type: 'string' }, // links to posted journal entry
    match_status: { type: 'string' }, // unmatched, proposed, matched
    updated_at_ms: { type: 'number' }
  },
  required: ['id', 'statement_id', 'value_date', 'amount', 'narration', 'match_status', 'updated_at_ms'],
  additionalProperties: true
};

export const collections = {
  accounting_accounts: accountSchema,
  accounting_journal_entries: journalEntrySchema,
  accounting_journal_entry_lines: journalEntryLineSchema,
  accounting_ledger_entries: ledgerEntrySchema,
  accounting_receipts: receiptSchema,
  accounting_bank_statements: bankStatementSchema,
  accounting_bank_statement_lines: bankStatementLineSchema
};

export const migrationStrategies = {};
