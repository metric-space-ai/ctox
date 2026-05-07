import { readFile } from "node:fs/promises";
import { basename, join } from "node:path";
import pg from "pg";
import { closeBusinessDb } from "../src/client";
import {
  closeAccountingFiscalPeriod,
  decideAccountingProposal,
  loadAccountingBusinessRows,
  saveAccountingSetupSnapshot,
  saveAccountingWorkflowSnapshot,
  type AccountingSetupSnapshot,
  type AccountingWorkflowSnapshot
} from "../src/accounting";

const companyId = "smoke-company";

async function main() {
  const adminUrl = process.env.DATABASE_ADMIN_URL ?? defaultAdminUrl();
  const dbName = `ctox_business_accounting_smoke_${Date.now()}_${process.pid}`;
  const databaseUrl = databaseUrlFor(adminUrl, dbName);
  const admin = new pg.Client({ connectionString: adminUrl });

  await admin.connect();
  try {
    await admin.query(`CREATE DATABASE ${identifier(dbName)}`);
  } finally {
    await admin.end();
  }

  const client = new pg.Client({ connectionString: databaseUrl });
  await client.connect();
  try {
    await client.query("CREATE EXTENSION IF NOT EXISTS pgcrypto");
    await applyMigrations(client);
    await runAccountingSmoke(databaseUrl, client);
  } finally {
    await client.end();
    await closeBusinessDb();
    await dropDatabase(adminUrl, dbName);
  }
}

async function applyMigrations(client: pg.Client) {
  const migrationsDir = join(import.meta.dirname, "../drizzle");
  const migrations = [
    "0000_silent_alice.sql",
    "0001_violet_boomer.sql",
    "0002_tearful_hellion.sql",
    "0003_sales_offers.sql",
    "0004_ctox_bug_reports.sql",
    "0005_sales_campaigns_customers.sql",
    "0006_business_accounting.sql",
    "0007_business_accounting_engine.sql"
  ];

  for (const migration of migrations) {
    const sql = await readFile(join(migrationsDir, migration), "utf8");
    for (const statement of sql.split("--> statement-breakpoint").map((part) => part.trim()).filter(Boolean)) {
      await client.query(statement);
    }
    process.stdout.write(`applied ${basename(migration)}\n`);
  }
}

async function runAccountingSmoke(databaseUrl: string, client: pg.Client) {
  const setup = buildSetupSnapshot();
  await saveAccountingSetupSnapshot(setup, databaseUrl);
  await closeAccountingFiscalPeriod({ externalId: "fy-2026", status: "closed" }, databaseUrl);

  const workflow = buildWorkflowSnapshot();
  const receiptWorkflow = buildReceiptWorkflowSnapshot();
  const bankWorkflow = buildBankMatchWorkflowSnapshot();
  await saveAccountingWorkflowSnapshot(workflow, databaseUrl);
  await saveAccountingWorkflowSnapshot(workflow, databaseUrl);
  await saveAccountingWorkflowSnapshot(receiptWorkflow, databaseUrl);
  await saveAccountingWorkflowSnapshot(bankWorkflow, databaseUrl);
  await decideAccountingProposal({
    actorId: "smoke-user",
    externalId: "proposal-smoke-invoice",
    status: "accepted"
  }, databaseUrl);
  await decideAccountingProposal({
    actorId: "smoke-user",
    externalId: "proposal-smoke-receipt",
    status: "accepted"
  }, databaseUrl);
  await decideAccountingProposal({
    actorId: "smoke-user",
    externalId: "proposal-smoke-bank-match",
    status: "accepted"
  }, databaseUrl);
  await client.query("UPDATE business_outbox_events SET status = 'delivered', attempts = 2, delivered_at = now() WHERE external_id = 'outbox-smoke-invoice'");
  await saveAccountingWorkflowSnapshot(workflow, databaseUrl);

  await expectCount(client, "accounting_accounts", 7);
  await expectCount(client, "accounting_parties", 2);
  await expectCount(client, "accounting_tax_rates", 2);
  await expectCount(client, "accounting_invoices", 1);
  await expectCount(client, "accounting_invoice_lines", 2);
  await expectCount(client, "accounting_receipts", 1);
  await expectCount(client, "accounting_receipt_lines", 1);
  await expectCount(client, "accounting_receipt_files", 1);
  await expectCount(client, "accounting_payments", 1);
  await expectCount(client, "accounting_payment_allocations", 1);
  await expectCount(client, "accounting_journal_entries", 3);
  await expectCount(client, "accounting_journal_entry_lines", 8);
  await expectCount(client, "accounting_ledger_entries", 8);
  await expectCount(client, "accounting_bank_statements", 1);
  await expectCount(client, "accounting_bank_statement_lines", 2);
  await expectCount(client, "business_accounting_proposals", 3);
  await expectCount(client, "business_outbox_events", 3);

  const auditCount = Number((await client.query("SELECT count(*)::int AS count FROM business_accounting_audit_events")).rows[0]?.count ?? 0);
  if (auditCount < 8) throw new Error(`expected repeated audit and decision rows, got ${auditCount}`);

  const proposal = await client.query("SELECT status, decided_by, resulting_journal_entry_id FROM business_accounting_proposals WHERE external_id = 'proposal-smoke-invoice'");
  if (
    proposal.rows[0]?.status !== "accepted"
    || proposal.rows[0]?.decided_by !== "smoke-user"
    || proposal.rows[0]?.resulting_journal_entry_id !== "je-invoice-invoice-inv-smoke-001"
  ) {
    throw new Error("expected accepted proposal decision to persist");
  }

  const invoice = await client.query("SELECT status, posted_journal_entry_external_id, sent_at FROM accounting_invoices WHERE external_id = 'inv-smoke-001'");
  if (
    invoice.rows[0]?.status !== "sent"
    || invoice.rows[0]?.posted_journal_entry_external_id !== "je-invoice-invoice-inv-smoke-001"
    || !invoice.rows[0]?.sent_at
  ) {
    throw new Error("expected accepted invoice proposal to apply invoice send side effects");
  }

  const receipt = await client.query("SELECT status, posted_journal_entry_external_id, posted_at FROM accounting_receipts WHERE external_id = 'rcpt-smoke-001'");
  if (
    receipt.rows[0]?.status !== "posted"
    || receipt.rows[0]?.posted_journal_entry_external_id !== "je-receipt-receipt-rcpt-smoke-001"
    || !receipt.rows[0]?.posted_at
  ) {
    throw new Error("expected accepted receipt proposal to apply receipt posting side effects");
  }

  const payment = await client.query("SELECT posted_journal_entry_external_id FROM accounting_payments WHERE external_id = 'pay-smoke-bank-1'");
  if (payment.rows[0]?.posted_journal_entry_external_id !== "je-payment-bank_transaction-bank-statement-smoke-line-1") {
    throw new Error("expected accepted bank match proposal to apply payment side effects");
  }

  const bankLine = await client.query("SELECT match_status, matched_journal_entry_external_id FROM accounting_bank_statement_lines WHERE external_id = 'bank-statement-smoke-line-1'");
  if (
    bankLine.rows[0]?.match_status !== "matched"
    || bankLine.rows[0]?.matched_journal_entry_external_id !== "je-payment-bank_transaction-bank-statement-smoke-line-1"
  ) {
    throw new Error("expected accepted bank match proposal to apply bank line side effects");
  }

  const businessRows = await loadAccountingBusinessRows(databaseUrl);
  if (
    businessRows.invoices.length !== 1
    || businessRows.receipts.length !== 1
    || businessRows.payments.length !== 1
    || businessRows.journalEntries.length !== 3
    || !businessRows.bankStatementLines.some((line) => line.matchStatus === "matched")
  ) {
    throw new Error("expected accounting business rows to hydrate persisted workspace data");
  }

  const outbox = await client.query("SELECT status, attempts FROM business_outbox_events WHERE external_id = 'outbox-smoke-invoice'");
  if (outbox.rows[0]?.status !== "delivered" || outbox.rows[0]?.attempts !== 2) {
    throw new Error("expected delivered outbox event to survive repeated snapshot");
  }

  const period = await client.query("SELECT status, closed_at FROM accounting_fiscal_periods WHERE external_id = 'fy-2026'");
  if (period.rows[0]?.status !== "closed" || !period.rows[0]?.closed_at) {
    throw new Error("expected fiscal period to be closed");
  }

  await expectMutationBlocked(
    client,
    "UPDATE accounting_journal_entries SET narration = 'mutated' WHERE external_id = 'je-invoice-invoice-inv-smoke-001'",
    "posted journal entries are immutable"
  );
  await expectMutationBlocked(
    client,
    "DELETE FROM accounting_ledger_entries WHERE external_id = 'je-invoice-invoice-inv-smoke-001-ledger-1'",
    "accounting ledger entries are append-only"
  );

  process.stdout.write("accounting db smoke ok\n");
}

function buildSetupSnapshot(): AccountingSetupSnapshot {
  return {
    accounts: [
      account("acc-ar", "1400", "Receivables", "asset", "receivable"),
      account("acc-ap", "1600", "Payables", "liability", "payable"),
      account("acc-expense", "4930", "Office supplies", "expense", "expense"),
      account("acc-revenue", "8400", "SaaS subscriptions", "income", "income"),
      account("acc-vat-input", "1576", "Input VAT 19%", "asset", "tax"),
      account("acc-vat-output", "1776", "VAT 19%", "liability", "tax"),
      account("acc-bank", "1200", "Bank", "asset", "bank")
    ],
    fiscalPeriods: [{
      companyId,
      endDate: "2026-12-31",
      externalId: "fy-2026",
      startDate: "2026-01-01",
      status: "open"
    }],
    parties: [{
      companyId,
      defaultReceivableAccountId: "acc-ar",
      externalId: "cust-smoke",
      kind: "customer",
      name: "Smoke Customer GmbH",
      vatId: "DE123456789"
    }, {
      companyId,
      defaultPayableAccountId: "acc-ap",
      externalId: "vendor-smoke",
      kind: "vendor",
      name: "Smoke Vendor GmbH"
    }],
    taxRates: [
      {
        accountId: "acc-vat-output",
        code: "DE_19",
        companyId,
        externalId: "tax-de-19",
        rate: 19,
        type: "output"
      },
      {
        accountId: "acc-vat-input",
        code: "DE_19_INPUT",
        companyId,
        externalId: "tax-de-19-input",
        rate: 19,
        type: "input"
      }
    ]
  };
}

function buildWorkflowSnapshot(): AccountingWorkflowSnapshot {
  return {
    audit: {
      action: "invoice.prepare_send",
      actorId: "smoke",
      actorType: "system",
      after: { status: "sent" },
      companyId,
      refId: "inv-smoke-001",
      refType: "invoice"
    },
    invoice: {
      balanceDueMinor: 11900,
      companyId,
      currency: "EUR",
      customerExternalId: "cust-smoke",
      dueDate: "2026-05-21",
      externalId: "inv-smoke-001",
      issueDate: "2026-05-07",
      lines: [
        {
          description: "SaaS subscription",
          lineNetMinor: 8000,
          lineNo: 1,
          lineTotalMinor: 9520,
          productExternalId: "prod-saas",
          quantity: 1,
          revenueAccountExternalId: "acc-revenue",
          taxAmountMinor: 1520,
          taxRate: 19,
          unitPriceMinor: 8000
        },
        {
          description: "Operations setup",
          lineNetMinor: 2000,
          lineNo: 2,
          lineTotalMinor: 2380,
          productExternalId: "prod-setup",
          quantity: 1,
          revenueAccountExternalId: "acc-revenue",
          taxAmountMinor: 380,
          taxRate: 19,
          unitPriceMinor: 2000
        }
      ],
      netAmountMinor: 10000,
      number: "RE-SMOKE-001",
      pdfBlobRef: "invoice-pdf:inv-smoke-001",
      postedJournalEntryExternalId: "je-invoice-invoice-inv-smoke-001",
      sentAt: new Date("2026-05-07T00:00:00.000Z"),
      serviceDate: "2026-05-07",
      status: "sent",
      taxAmountMinor: 1900,
      totalAmountMinor: 11900,
      zugferdXml: "<rsm:CrossIndustryInvoice />"
    },
    bankStatement: {
      accountExternalId: "acc-bank",
      closingBalanceMinor: 11900,
      companyId,
      currency: "EUR",
      endDate: "2026-05-08",
      externalId: "bank-statement-smoke",
      format: "csv",
      importedBy: "smoke",
      lines: [
        {
          amountMinor: 11900,
          bookingDate: "2026-05-07",
          currency: "EUR",
          externalId: "bank-statement-smoke-line-1",
          lineNo: 1,
          matchStatus: "suggested",
          purpose: "RE-SMOKE-001",
          remitterName: "Smoke Customer GmbH",
          valueDate: "2026-05-07"
        },
        {
          amountMinor: 0,
          bookingDate: "2026-05-08",
          currency: "EUR",
          duplicateOfLineExternalId: "bank-statement-smoke-line-1",
          externalId: "bank-statement-smoke-line-2",
          lineNo: 2,
          matchStatus: "ignored",
          purpose: "duplicate test"
        }
      ],
      openingBalanceMinor: 0,
      sourceFilename: "smoke.csv",
      sourceSha256: "smoke-sha256",
      startDate: "2026-05-07"
    },
    journalDraft: {
      companyId,
      lines: [
        { accountId: "acc-ar", debit: { minor: 11900 }, credit: { minor: 0 }, partyId: "cust-smoke" },
        { accountId: "acc-revenue", debit: { minor: 0 }, credit: { minor: 10000 } },
        { accountId: "acc-vat-output", debit: { minor: 0 }, credit: { minor: 1900 } }
      ],
      narration: "Smoke invoice posting",
      postingDate: "2026-05-07",
      refId: "inv-smoke-001",
      refType: "invoice",
      type: "invoice"
    },
    outbox: {
      companyId,
      id: "outbox-smoke-invoice",
      payload: { invoiceId: "inv-smoke-001", proposalId: "proposal-smoke-invoice" },
      status: "pending",
      topic: "business.invoice.prepare_send"
    },
    proposal: {
      companyId,
      confidence: 0.98,
      createdByAgent: "invoice-checker",
      evidence: { smoke: true },
      id: "proposal-smoke-invoice",
      kind: "invoice_check",
      proposedCommand: {
        companyId,
        idempotencyKey: `${companyId}:SendInvoice:invoice:inv-smoke-001`,
        payload: { invoiceId: "inv-smoke-001", invoiceNumber: "RE-SMOKE-001" },
        refId: "inv-smoke-001",
        refType: "invoice",
        requestedAt: "2026-05-07T00:00:00.000Z",
        requestedBy: "smoke",
        type: "SendInvoice"
      },
      refId: "inv-smoke-001",
      refType: "invoice",
      status: "open"
    }
  };
}

function buildReceiptWorkflowSnapshot(): AccountingWorkflowSnapshot {
  return {
    audit: {
      action: "receipt.prepare_post",
      actorId: "smoke",
      actorType: "system",
      after: { status: "posted" },
      companyId,
      refId: "rcpt-smoke-001",
      refType: "receipt"
    },
    journalDraft: {
      companyId,
      lines: [
        { accountId: "acc-expense", debit: { minor: 10000 }, credit: { minor: 0 } },
        { accountId: "acc-vat-input", debit: { minor: 1900 }, credit: { minor: 0 } },
        { accountId: "acc-ap", debit: { minor: 0 }, credit: { minor: 11900 }, partyId: "vendor-smoke" }
      ],
      narration: "Smoke receipt posting",
      postingDate: "2026-05-07",
      refId: "rcpt-smoke-001",
      refType: "receipt",
      type: "receipt"
    },
    outbox: {
      companyId,
      id: "outbox-smoke-receipt",
      payload: { proposalId: "proposal-smoke-receipt", receiptId: "rcpt-smoke-001" },
      status: "pending",
      topic: "business.receipt.prepare_post"
    },
    proposal: {
      companyId,
      confidence: 0.91,
      createdByAgent: "receipt-extractor",
      evidence: { smoke: true },
      id: "proposal-smoke-receipt",
      kind: "receipt_extraction",
      proposedCommand: {
        companyId,
        idempotencyKey: `${companyId}:PostReceipt:receipt:rcpt-smoke-001`,
        payload: { receiptId: "rcpt-smoke-001", receiptNumber: "ER-SMOKE-001", vendorName: "Smoke Vendor GmbH" },
        refId: "rcpt-smoke-001",
        refType: "receipt",
        requestedAt: "2026-05-07T00:00:00.000Z",
        requestedBy: "smoke",
        type: "PostReceipt"
      },
      refId: "rcpt-smoke-001",
      refType: "receipt",
      status: "open"
    },
    receipt: {
      companyId,
      currency: "EUR",
      dueDate: "2026-05-21",
      expenseAccountExternalId: "acc-expense",
      externalId: "rcpt-smoke-001",
      extractedJson: { vendorName: "Smoke Vendor GmbH" },
      files: [{
        blobRef: "receipt-file:rcpt-smoke-001",
        mime: "application/pdf",
        originalFilename: "smoke-receipt.pdf",
        sha256: "sha256-smoke-receipt"
      }],
      lines: [{
        description: "Office supplies",
        expenseAccountExternalId: "acc-expense",
        lineNo: 1,
        netAmountMinor: 10000,
        taxAmountMinor: 1900,
        taxCode: "DE_19_INPUT",
        totalAmountMinor: 11900
      }],
      netAmountMinor: 10000,
      number: "ER-SMOKE-001",
      payableAccountExternalId: "acc-ap",
      receiptDate: "2026-05-07",
      status: "reviewed",
      taxAmountMinor: 1900,
      taxCode: "DE_19_INPUT",
      totalAmountMinor: 11900,
      vendorExternalId: "vendor-smoke",
      vendorInvoiceNumber: "SMOKE-V-001"
    }
  };
}

function buildBankMatchWorkflowSnapshot(): AccountingWorkflowSnapshot {
  return {
    audit: {
      action: "bank_match.prepare_accept",
      actorId: "smoke",
      actorType: "system",
      after: { status: "matched" },
      companyId,
      refId: "bank-statement-smoke-line-1",
      refType: "bank_transaction"
    },
    journalDraft: {
      companyId,
      lines: [
        { accountId: "acc-bank", debit: { minor: 11900 }, credit: { minor: 0 } },
        { accountId: "acc-ar", debit: { minor: 0 }, credit: { minor: 11900 }, partyId: "cust-smoke" }
      ],
      narration: "Smoke bank match",
      postingDate: "2026-05-07",
      refId: "bank-statement-smoke-line-1",
      refType: "bank_transaction",
      type: "payment"
    },
    outbox: {
      companyId,
      id: "outbox-smoke-bank-match",
      payload: { bankTransactionId: "bank-statement-smoke-line-1", proposalId: "proposal-smoke-bank-match" },
      status: "pending",
      topic: "business.bank_match.prepare_accept"
    },
    payment: {
      allocation: {
        amountMinor: 11900,
        invoiceExternalId: "inv-smoke-001"
      },
      amountMinor: 11900,
      bankAccountExternalId: "acc-bank",
      bankStatementLineExternalId: "bank-statement-smoke-line-1",
      companyId,
      currency: "EUR",
      externalId: "pay-smoke-bank-1",
      kind: "incoming",
      partyExternalId: "cust-smoke",
      paymentDate: "2026-05-07"
    },
    proposal: {
      companyId,
      confidence: 0.99,
      createdByAgent: "bank-reconciler",
      evidence: { smoke: true },
      id: "proposal-smoke-bank-match",
      kind: "bank_match",
      proposedCommand: {
        companyId,
        idempotencyKey: `${companyId}:AcceptBankMatch:bank_transaction:bank-statement-smoke-line-1`,
        payload: {
          amount: 119,
          bankTransactionId: "bank-statement-smoke-line-1",
          matchedRecordId: "inv-smoke-001",
          matchType: "invoice"
        },
        refId: "bank-statement-smoke-line-1",
        refType: "bank_transaction",
        requestedAt: "2026-05-07T00:00:00.000Z",
        requestedBy: "smoke",
        type: "AcceptBankMatch"
      },
      refId: "bank-statement-smoke-line-1",
      refType: "bank_transaction",
      status: "open"
    }
  };
}

function account(externalId: string, code: string, name: string, rootType: string, accountType: string) {
  return {
    accountType,
    code,
    companyId,
    currency: "EUR",
    externalId,
    isGroup: false,
    name,
    rootType
  };
}

async function expectCount(client: pg.Client, table: string, expected: number) {
  const result = await client.query(`SELECT count(*)::int AS count FROM ${identifier(table)}`);
  const actual = Number(result.rows[0]?.count ?? 0);
  if (actual !== expected) throw new Error(`expected ${expected} rows in ${table}, got ${actual}`);
}

async function expectMutationBlocked(client: pg.Client, sql: string, expectedMessage: string) {
  try {
    await client.query(sql);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (message.includes(expectedMessage)) return;
    throw error;
  }
  throw new Error(`expected mutation to be blocked: ${sql}`);
}

async function dropDatabase(adminUrl: string, dbName: string) {
  const admin = new pg.Client({ connectionString: adminUrl });
  await admin.connect();
  try {
    await admin.query(`DROP DATABASE IF EXISTS ${identifier(dbName)} WITH (FORCE)`);
  } finally {
    await admin.end();
  }
}

function defaultAdminUrl() {
  const user = encodeURIComponent(process.env.USER ?? "postgres");
  return `postgres://${user}@localhost:5432/postgres`;
}

function databaseUrlFor(adminUrl: string, dbName: string) {
  const url = new URL(adminUrl);
  url.pathname = `/${dbName}`;
  return url.toString();
}

function identifier(value: string) {
  return `"${value.replace(/"/g, "\"\"")}"`;
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
