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
    "0007_business_accounting_engine.sql",
    "0012_business_accounting_artifacts.sql"
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
  await closeAccountingFiscalPeriod({ externalId: "fy-2026-04", status: "closed" }, databaseUrl);

  const workflow = buildWorkflowSnapshot();
  const receiptIngestWorkflow = buildReceiptIngestWorkflowSnapshot();
  const receiptWorkflow = buildReceiptWorkflowSnapshot();
  const bankWorkflow = buildBankMatchWorkflowSnapshot();
  const assetCapitalizationWorkflow = buildAssetCapitalizationWorkflowSnapshot();
  const assetDepreciationWorkflow = buildAssetDepreciationWorkflowSnapshot();
  const assetDisposalWorkflow = buildAssetDisposalWorkflowSnapshot();
  const datevWorkflow = buildDatevWorkflowSnapshot();
  const dunningWorkflow = buildDunningWorkflowSnapshot();
  const missingJournalWorkflow = buildMissingJournalWorkflowSnapshot();
  await saveAccountingWorkflowSnapshot(workflow, databaseUrl);
  await saveAccountingWorkflowSnapshot(workflow, databaseUrl);
  await saveAccountingWorkflowSnapshot(receiptIngestWorkflow, databaseUrl);
  await saveAccountingWorkflowSnapshot(receiptWorkflow, databaseUrl);
  await saveAccountingWorkflowSnapshot(bankWorkflow, databaseUrl);
  await saveAccountingWorkflowSnapshot(assetCapitalizationWorkflow, databaseUrl);
  await saveAccountingWorkflowSnapshot(assetDepreciationWorkflow, databaseUrl);
  await saveAccountingWorkflowSnapshot(assetDisposalWorkflow, databaseUrl);
  await saveAccountingWorkflowSnapshot(datevWorkflow, databaseUrl);
  await saveAccountingWorkflowSnapshot(dunningWorkflow, databaseUrl);
  await saveAccountingWorkflowSnapshot(missingJournalWorkflow, databaseUrl);
  await expectProposalDecisionBlocked(
    "proposal-smoke-missing-journal",
    "journal_entry_missing_for_proposal",
    databaseUrl
  );
  await decideAccountingProposal({
    actorId: "smoke-user",
    externalId: "proposal-smoke-receipt-ingest",
    resultingJournalEntryId: "receipt-ingest-rcpt-smoke-ingest",
    status: "accepted"
  }, databaseUrl);
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
  await decideAccountingProposal({
    actorId: "smoke-user",
    externalId: "proposal-smoke-asset-capitalization",
    status: "accepted"
  }, databaseUrl);
  await decideAccountingProposal({
    actorId: "smoke-user",
    externalId: "proposal-smoke-asset-depreciation",
    status: "accepted"
  }, databaseUrl);
  await decideAccountingProposal({
    actorId: "smoke-user",
    externalId: "proposal-smoke-asset-disposal",
    status: "accepted"
  }, databaseUrl);
  await decideAccountingProposal({
    actorId: "smoke-user",
    externalId: "proposal-smoke-datev-export",
    resultingJournalEntryId: "datev-export-exp-smoke-2026-05",
    status: "accepted"
  }, databaseUrl);
  await decideAccountingProposal({
    actorId: "smoke-user",
    externalId: "proposal-smoke-dunning",
    resultingJournalEntryId: "dunning-run-inv-smoke-001",
    status: "accepted"
  }, databaseUrl);
  await saveAccountingWorkflowSnapshot(buildClosedPeriodWorkflowSnapshot(), databaseUrl);
  await closeAccountingFiscalPeriod({ externalId: "fy-2026-05", status: "closed" }, databaseUrl);
  await expectProposalDecisionBlocked(
    "proposal-smoke-closed-period",
    "fiscal_period_closed",
    databaseUrl
  );
  await client.query("UPDATE business_outbox_events SET status = 'delivered', attempts = 2, delivered_at = now() WHERE external_id = 'outbox-smoke-invoice'");
  await saveAccountingWorkflowSnapshot(workflow, databaseUrl);

  await expectCount(client, "accounting_accounts", 10);
  await expectCount(client, "accounting_parties", 2);
  await expectCount(client, "accounting_tax_rates", 2);
  await expectCount(client, "accounting_invoices", 1);
  await expectCount(client, "accounting_invoice_lines", 2);
  await expectCount(client, "accounting_receipts", 3);
  await expectCount(client, "accounting_receipt_lines", 3);
  await expectCount(client, "accounting_receipt_files", 3);
  await expectCount(client, "accounting_payments", 1);
  await expectCount(client, "accounting_payment_allocations", 1);
  await expectCount(client, "accounting_journal_entries", 6);
  await expectCount(client, "accounting_journal_entry_lines", 16);
  await expectCount(client, "accounting_ledger_entries", 16);
  await expectCount(client, "accounting_bank_statements", 1);
  await expectCount(client, "accounting_bank_statement_lines", 2);
  await expectCount(client, "accounting_datev_exports", 1);
  await expectCount(client, "accounting_dunning_runs", 1);
  await expectCount(client, "business_accounting_proposals", 11);
  await expectCount(client, "business_outbox_events", 11);

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

  const invoice = await client.query("SELECT balance_due_minor, status, posted_journal_entry_external_id, sent_at FROM accounting_invoices WHERE external_id = 'inv-smoke-001'");
  if (
    invoice.rows[0]?.status !== "paid"
    || invoice.rows[0]?.balance_due_minor !== 0
    || invoice.rows[0]?.posted_journal_entry_external_id !== "je-invoice-invoice-inv-smoke-001"
    || !invoice.rows[0]?.sent_at
  ) {
    throw new Error("expected accepted invoice and bank match proposals to apply invoice send and payment side effects");
  }

  const receipt = await client.query("SELECT status, posted_journal_entry_external_id, posted_at FROM accounting_receipts WHERE external_id = 'rcpt-smoke-001'");
  if (
    receipt.rows[0]?.status !== "posted"
    || receipt.rows[0]?.posted_journal_entry_external_id !== "je-receipt-receipt-rcpt-smoke-001"
    || !receipt.rows[0]?.posted_at
  ) {
    throw new Error("expected accepted receipt proposal to apply receipt posting side effects");
  }

  const ingestedReceipt = await client.query("SELECT status, posted_journal_entry_external_id FROM accounting_receipts WHERE external_id = 'rcpt-smoke-ingest'");
  if (
    ingestedReceipt.rows[0]?.status !== "extracted"
    || ingestedReceipt.rows[0]?.posted_journal_entry_external_id !== null
  ) {
    throw new Error("expected accepted receipt ingest proposal to extract without creating a journal artifact");
  }

  const ingestProposal = await client.query("SELECT resulting_journal_entry_id FROM business_accounting_proposals WHERE external_id = 'proposal-smoke-receipt-ingest'");
  if (ingestProposal.rows[0]?.resulting_journal_entry_id !== null) {
    throw new Error("expected non-journal receipt ingest proposal to ignore stale journal artifact ids");
  }

  const payment = await client.query("SELECT posted_journal_entry_external_id FROM accounting_payments WHERE external_id = 'pay-smoke-bank-1'");
  if (payment.rows[0]?.posted_journal_entry_external_id !== "je-payment-bank_transaction-bank-statement-smoke-line-1") {
    throw new Error("expected accepted bank match proposal to apply payment side effects");
  }

  const assetReceipt = await client.query("SELECT status, posted_journal_entry_external_id, posted_at FROM accounting_receipts WHERE external_id = 'rcpt-smoke-asset'");
  if (
    assetReceipt.rows[0]?.status !== "posted"
    || assetReceipt.rows[0]?.posted_journal_entry_external_id !== "je-manual-asset-asset-rcpt-smoke-asset"
    || !assetReceipt.rows[0]?.posted_at
  ) {
    throw new Error("expected accepted asset capitalization proposal to post receipt-backed asset");
  }

  const assetJournals = await client.query("SELECT external_id FROM accounting_journal_entries WHERE external_id in ('je-manual-asset-asset-rcpt-smoke-asset', 'je-depreciation-asset-asset-rcpt-smoke-asset-2026', 'je-manual-asset-asset-rcpt-smoke-asset-disposal') ORDER BY external_id");
  if (assetJournals.rows.length !== 3) {
    throw new Error("expected asset capitalization, depreciation and disposal journals to persist");
  }

  const disposedAssetBalance = await client.query("SELECT coalesce(sum(debit_minor - credit_minor), 0)::int AS balance FROM accounting_journal_entry_lines WHERE account_external_id = 'acc-fixed-assets' AND journal_entry_external_id in ('je-manual-asset-asset-rcpt-smoke-asset', 'je-manual-asset-asset-rcpt-smoke-asset-disposal')");
  if (disposedAssetBalance.rows[0]?.balance !== 0) {
    throw new Error("expected asset disposal to clear the fixed asset acquisition balance");
  }

  const bankLine = await client.query("SELECT match_status, matched_journal_entry_external_id FROM accounting_bank_statement_lines WHERE external_id = 'bank-statement-smoke-line-1'");
  if (
    bankLine.rows[0]?.match_status !== "matched"
    || bankLine.rows[0]?.matched_journal_entry_external_id !== "je-payment-bank_transaction-bank-statement-smoke-line-1"
  ) {
    throw new Error("expected accepted bank match proposal to apply bank line side effects");
  }

  const datevExport = await client.query("SELECT status, exported_by, exported_at, source_proposal_external_id, csv_sha256 FROM accounting_datev_exports WHERE external_id = 'exp-smoke-2026-05'");
  if (
    datevExport.rows[0]?.status !== "exported"
    || datevExport.rows[0]?.exported_by !== "smoke-user"
    || !datevExport.rows[0]?.exported_at
    || datevExport.rows[0]?.source_proposal_external_id !== "proposal-smoke-datev-export"
    || datevExport.rows[0]?.csv_sha256 !== "sha256-datev-smoke"
  ) {
    throw new Error("expected accepted DATEV proposal to persist exported batch artifact");
  }

  const dunningRun = await client.query("SELECT status, created_by, delivered_at, level, fee_amount_minor, source_proposal_external_id FROM accounting_dunning_runs WHERE external_id = 'dunning-inv-smoke-001-level-2'");
  if (
    dunningRun.rows[0]?.status !== "delivered"
    || dunningRun.rows[0]?.created_by !== "smoke-user"
    || !dunningRun.rows[0]?.delivered_at
    || dunningRun.rows[0]?.level !== 2
    || dunningRun.rows[0]?.fee_amount_minor !== 500
    || dunningRun.rows[0]?.source_proposal_external_id !== "proposal-smoke-dunning"
  ) {
    throw new Error("expected accepted dunning proposal to persist dunning run artifact");
  }

  const businessRows = await loadAccountingBusinessRows(databaseUrl);
  if (
    businessRows.invoices.length !== 1
    || businessRows.receipts.length !== 3
    || businessRows.payments.length !== 1
    || businessRows.journalEntries.length !== 6
    || businessRows.datevExports.length !== 1
    || businessRows.dunningRuns.length !== 1
    || !businessRows.bankStatementLines.some((line) => line.matchStatus === "matched")
  ) {
    throw new Error("expected accounting business rows to hydrate persisted workspace data");
  }

  const outbox = await client.query("SELECT status, attempts FROM business_outbox_events WHERE external_id = 'outbox-smoke-invoice'");
  if (outbox.rows[0]?.status !== "delivered" || outbox.rows[0]?.attempts !== 2) {
    throw new Error("expected delivered outbox event to survive repeated snapshot");
  }

  const period = await client.query("SELECT status, closed_at FROM accounting_fiscal_periods WHERE external_id = 'fy-2026-04'");
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
    "DELETE FROM accounting_journal_entries WHERE external_id = 'je-invoice-invoice-inv-smoke-001'",
    "posted journal entries are immutable"
  );
  await expectMutationBlocked(
    client,
    "UPDATE accounting_journal_entry_lines SET debit_minor = 1 WHERE journal_entry_external_id = 'je-invoice-invoice-inv-smoke-001' AND line_no = 1",
    "posted journal entry lines are immutable"
  );
  await expectMutationBlocked(
    client,
    "DELETE FROM accounting_journal_entry_lines WHERE journal_entry_external_id = 'je-invoice-invoice-inv-smoke-001' AND line_no = 1",
    "posted journal entry lines are immutable"
  );
  await expectMutationBlocked(
    client,
    "UPDATE accounting_ledger_entries SET debit_minor = 1 WHERE external_id = 'je-invoice-invoice-inv-smoke-001-ledger-1'",
    "accounting ledger entries are append-only"
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
      account("acc-bank", "1200", "Bank", "asset", "bank"),
      account("acc-fixed-assets", "0480", "Office equipment", "asset", "fixed_asset"),
      account("acc-accumulated-depreciation", "0490", "Accumulated depreciation", "asset", "accumulated_depreciation"),
      account("acc-depreciation", "4830", "Depreciation", "expense", "depreciation")
    ],
    fiscalPeriods: [
      {
        companyId,
        endDate: "2026-12-31",
        externalId: "fy-2026",
        startDate: "2026-01-01",
        status: "open"
      },
      {
        companyId,
        endDate: "2026-04-30",
        externalId: "fy-2026-04",
        startDate: "2026-04-01",
        status: "open"
      },
      {
        companyId,
        endDate: "2026-05-31",
        externalId: "fy-2026-05",
        startDate: "2026-05-01",
        status: "open"
      }
    ],
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

function buildReceiptIngestWorkflowSnapshot(): AccountingWorkflowSnapshot {
  return {
    audit: {
      action: "receipt.prepare_ingest",
      actorId: "smoke",
      actorType: "system",
      after: { status: "extracted" },
      companyId,
      refId: "rcpt-smoke-ingest",
      refType: "receipt"
    },
    outbox: {
      companyId,
      id: "outbox-smoke-receipt-ingest",
      payload: { proposalId: "proposal-smoke-receipt-ingest", receiptId: "rcpt-smoke-ingest" },
      status: "pending",
      topic: "business.receipt.prepare_ingest"
    },
    proposal: {
      companyId,
      confidence: 0.87,
      createdByAgent: "receipt-extractor",
      evidence: { smoke: true, source: "ocr" },
      id: "proposal-smoke-receipt-ingest",
      kind: "receipt_extraction",
      proposedCommand: {
        companyId,
        idempotencyKey: `${companyId}:IngestReceipt:receipt:rcpt-smoke-ingest`,
        payload: { fileName: "smoke-ingest.pdf", receiptId: "rcpt-smoke-ingest" },
        refId: "rcpt-smoke-ingest",
        refType: "receipt",
        requestedAt: "2026-05-07T00:00:00.000Z",
        requestedBy: "smoke",
        type: "IngestReceipt"
      },
      refId: "rcpt-smoke-ingest",
      refType: "receipt",
      status: "open"
    },
    receipt: {
      companyId,
      currency: "EUR",
      dueDate: "2026-05-21",
      expenseAccountExternalId: "acc-expense",
      externalId: "rcpt-smoke-ingest",
      extractedJson: { vendorName: "Smoke Vendor GmbH", confidence: 0.87 },
      files: [{
        blobRef: "receipt-file:rcpt-smoke-ingest",
        mime: "application/pdf",
        originalFilename: "smoke-ingest.pdf",
        sha256: "sha256-smoke-ingest"
      }],
      lines: [{
        description: "OCR staged expense",
        expenseAccountExternalId: "acc-expense",
        lineNo: 1,
        netAmountMinor: 5000,
        taxAmountMinor: 950,
        taxCode: "DE_19_INPUT",
        totalAmountMinor: 5950
      }],
      netAmountMinor: 5000,
      number: "ER-SMOKE-INGEST",
      ocrText: "Smoke Vendor GmbH 59,50 EUR",
      payableAccountExternalId: "acc-ap",
      receiptDate: "2026-05-07",
      status: "scanned",
      taxAmountMinor: 950,
      taxCode: "DE_19_INPUT",
      totalAmountMinor: 5950,
      vendorExternalId: "vendor-smoke",
      vendorInvoiceNumber: "SMOKE-INGEST-001"
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

function buildAssetCapitalizationWorkflowSnapshot(): AccountingWorkflowSnapshot {
  return {
    audit: {
      action: "asset.prepare_capitalization",
      actorId: "smoke",
      actorType: "system",
      after: { status: "asset_capitalized" },
      companyId,
      refId: "rcpt-smoke-asset",
      refType: "receipt"
    },
    journalDraft: {
      companyId,
      lines: [
        { accountId: "acc-fixed-assets", debit: { minor: 100000 }, credit: { minor: 0 } },
        { accountId: "acc-vat-input", debit: { minor: 19000 }, credit: { minor: 0 } },
        { accountId: "acc-ap", debit: { minor: 0 }, credit: { minor: 119000 }, partyId: "vendor-smoke" }
      ],
      narration: "Smoke asset capitalization",
      postingDate: "2026-05-07",
      refId: "asset-rcpt-smoke-asset",
      refType: "asset",
      type: "manual"
    },
    outbox: {
      companyId,
      id: "outbox-smoke-asset-capitalization",
      payload: { proposalId: "proposal-smoke-asset-capitalization", receiptId: "rcpt-smoke-asset" },
      status: "pending",
      topic: "business.asset.prepare_capitalization"
    },
    proposal: {
      companyId,
      confidence: 0.91,
      createdByAgent: "asset-accountant",
      evidence: { smoke: true },
      id: "proposal-smoke-asset-capitalization",
      kind: "asset_activation",
      proposedCommand: {
        companyId,
        idempotencyKey: `${companyId}:CapitalizeReceipt:receipt:rcpt-smoke-asset`,
        payload: { assetId: "asset-rcpt-smoke-asset", receiptId: "rcpt-smoke-asset" },
        refId: "rcpt-smoke-asset",
        refType: "receipt",
        requestedAt: "2026-05-07T00:00:00.000Z",
        requestedBy: "smoke",
        type: "CapitalizeReceipt"
      },
      refId: "rcpt-smoke-asset",
      refType: "receipt",
      status: "open"
    },
    receipt: {
      companyId,
      currency: "EUR",
      dueDate: "2026-05-21",
      expenseAccountExternalId: "acc-fixed-assets",
      externalId: "rcpt-smoke-asset",
      extractedJson: { vendorName: "Smoke Vendor GmbH" },
      files: [{
        blobRef: "receipt-file:rcpt-smoke-asset",
        mime: "application/pdf",
        originalFilename: "smoke-asset.pdf",
        sha256: "sha256-smoke-asset"
      }],
      lines: [{
        description: "Office notebook",
        expenseAccountExternalId: "acc-fixed-assets",
        lineNo: 1,
        netAmountMinor: 100000,
        taxAmountMinor: 19000,
        taxCode: "DE_19_INPUT",
        totalAmountMinor: 119000
      }],
      netAmountMinor: 100000,
      number: "ER-SMOKE-ASSET",
      payableAccountExternalId: "acc-ap",
      receiptDate: "2026-05-07",
      status: "reviewed",
      taxAmountMinor: 19000,
      taxCode: "DE_19_INPUT",
      totalAmountMinor: 119000,
      vendorExternalId: "vendor-smoke",
      vendorInvoiceNumber: "SMOKE-ASSET-001"
    }
  };
}

function buildAssetDepreciationWorkflowSnapshot(): AccountingWorkflowSnapshot {
  return {
    audit: {
      action: "asset.prepare_depreciation",
      actorId: "smoke",
      actorType: "system",
      after: { status: "depreciation_posted" },
      companyId,
      refId: "asset-rcpt-smoke-asset",
      refType: "asset"
    },
    journalDraft: {
      companyId,
      lines: [
        { accountId: "acc-depreciation", debit: { minor: 10000 }, credit: { minor: 0 } },
        { accountId: "acc-accumulated-depreciation", debit: { minor: 0 }, credit: { minor: 10000 } }
      ],
      narration: "Smoke asset depreciation",
      postingDate: "2026-06-30",
      refId: "asset-rcpt-smoke-asset-2026",
      refType: "asset",
      type: "depreciation"
    },
    outbox: {
      companyId,
      id: "outbox-smoke-asset-depreciation",
      payload: { assetId: "asset-rcpt-smoke-asset", proposalId: "proposal-smoke-asset-depreciation" },
      status: "pending",
      topic: "business.asset.prepare_depreciation"
    },
    proposal: {
      companyId,
      confidence: 0.9,
      createdByAgent: "asset-accountant",
      evidence: { smoke: true },
      id: "proposal-smoke-asset-depreciation",
      kind: "asset_depreciation",
      proposedCommand: {
        companyId,
        idempotencyKey: `${companyId}:PostDepreciation:asset:asset-rcpt-smoke-asset-2026`,
        payload: { amountMinor: 10000, assetId: "asset-rcpt-smoke-asset", fiscalYear: 2026 },
        refId: "asset-rcpt-smoke-asset-2026",
        refType: "asset",
        requestedAt: "2026-06-30T00:00:00.000Z",
        requestedBy: "smoke",
        type: "PostDepreciation"
      },
      refId: "asset-rcpt-smoke-asset",
      refType: "asset",
      status: "open"
    }
  };
}

function buildAssetDisposalWorkflowSnapshot(): AccountingWorkflowSnapshot {
  return {
    audit: {
      action: "asset.prepare_disposal",
      actorId: "smoke",
      actorType: "system",
      after: { status: "asset_disposed" },
      companyId,
      refId: "asset-rcpt-smoke-asset",
      refType: "asset"
    },
    journalDraft: {
      companyId,
      lines: [
        { accountId: "acc-accumulated-depreciation", debit: { minor: 10000 }, credit: { minor: 0 } },
        { accountId: "acc-depreciation", debit: { minor: 90000 }, credit: { minor: 0 } },
        { accountId: "acc-fixed-assets", debit: { minor: 0 }, credit: { minor: 100000 } }
      ],
      narration: "Smoke asset disposal",
      postingDate: "2026-12-31",
      refId: "asset-rcpt-smoke-asset-disposal",
      refType: "asset",
      type: "manual"
    },
    outbox: {
      companyId,
      id: "outbox-smoke-asset-disposal",
      payload: { assetId: "asset-rcpt-smoke-asset", proposalId: "proposal-smoke-asset-disposal" },
      status: "pending",
      topic: "business.asset.prepare_disposal"
    },
    proposal: {
      companyId,
      confidence: 0.84,
      createdByAgent: "asset-accountant",
      evidence: { accumulatedDepreciation: 100, bookValue: 900, disposalDate: "2026-12-31", proceeds: 0 },
      id: "proposal-smoke-asset-disposal",
      kind: "asset_disposal",
      proposedCommand: {
        companyId,
        idempotencyKey: `${companyId}:DisposeAsset:asset:asset-rcpt-smoke-asset-disposal`,
        payload: {
          accumulatedDepreciationMinor: 10000,
          assetId: "asset-rcpt-smoke-asset",
          disposalDate: "2026-12-31",
          proceedsMinor: 0
        },
        refId: "asset-rcpt-smoke-asset-disposal",
        refType: "asset",
        requestedAt: "2026-12-31T00:00:00.000Z",
        requestedBy: "smoke",
        type: "DisposeAsset"
      },
      refId: "asset-rcpt-smoke-asset",
      refType: "asset",
      status: "open"
    }
  };
}

function buildDatevWorkflowSnapshot(): AccountingWorkflowSnapshot {
  return {
    audit: {
      action: "datev.prepare_export",
      actorId: "smoke",
      actorType: "system",
      after: { lineCount: 3, period: "2026-05" },
      companyId,
      refId: "exp-smoke-2026-05",
      refType: "bookkeeping_export"
    },
    datevExport: {
      companyId,
      csvBlobRef: "datev-export:exp-smoke-2026-05",
      csvSha256: "sha256-datev-smoke",
      externalId: "exp-smoke-2026-05",
      lineCount: 3,
      netAmountMinor: 10000,
      payload: { filename: "2026-05-datev.csv" },
      period: "2026-05",
      sourceProposalExternalId: "proposal-smoke-datev-export",
      status: "prepared",
      system: "DATEV",
      taxAmountMinor: 1900
    },
    outbox: {
      companyId,
      id: "outbox-smoke-datev-export",
      payload: { exportId: "exp-smoke-2026-05", proposalId: "proposal-smoke-datev-export" },
      status: "pending",
      topic: "business.datev.prepare_export"
    },
    proposal: {
      companyId,
      confidence: 0.94,
      createdByAgent: "datev-exporter",
      evidence: { lineCount: 3, period: "2026-05", system: "DATEV" },
      id: "proposal-smoke-datev-export",
      kind: "datev_export",
      proposedCommand: {
        companyId,
        idempotencyKey: `${companyId}:ExportDatev:bookkeeping:exp-smoke-2026-05`,
        payload: { exportId: "exp-smoke-2026-05", period: "2026-05", system: "DATEV" },
        refId: "exp-smoke-2026-05",
        refType: "bookkeeping_export",
        requestedAt: "2026-05-07T00:00:00.000Z",
        requestedBy: "smoke",
        type: "ExportDatev"
      },
      refId: "exp-smoke-2026-05",
      refType: "bookkeeping_export",
      status: "open"
    }
  };
}

function buildDunningWorkflowSnapshot(): AccountingWorkflowSnapshot {
  return {
    audit: {
      action: "dunning.prepare_run",
      actorId: "smoke",
      actorType: "system",
      after: { daysOverdue: 16, level: 2 },
      companyId,
      refId: "inv-smoke-001",
      refType: "invoice"
    },
    outbox: {
      companyId,
      id: "outbox-smoke-dunning",
      payload: { invoiceId: "inv-smoke-001", proposalId: "proposal-smoke-dunning" },
      status: "pending",
      topic: "business.dunning.prepare_run"
    },
    proposal: {
      companyId,
      confidence: 0.9,
      createdByAgent: "dunning-assistant",
      evidence: { daysOverdue: 16, feeAmount: 5, invoiceNumber: "RE-SMOKE-001", level: 2 },
      id: "proposal-smoke-dunning",
      kind: "dunning_run",
      proposedCommand: {
        companyId,
        idempotencyKey: `${companyId}:RunDunning:invoice:inv-smoke-001`,
        payload: { feeAmount: 5, invoiceId: "inv-smoke-001", invoiceNumber: "RE-SMOKE-001", level: 2 },
        refId: "inv-smoke-001",
        refType: "invoice",
        requestedAt: "2026-05-07T00:00:00.000Z",
        requestedBy: "smoke",
        type: "RunDunning"
      },
      refId: "inv-smoke-001",
      refType: "invoice",
      status: "open"
    }
  };
}

function buildMissingJournalWorkflowSnapshot(): AccountingWorkflowSnapshot {
  return {
    audit: {
      action: "invoice.prepare_send",
      actorId: "smoke",
      actorType: "system",
      after: { status: "sent_without_journal" },
      companyId,
      refId: "inv-smoke-missing-journal",
      refType: "invoice"
    },
    outbox: {
      companyId,
      id: "outbox-smoke-missing-journal",
      payload: { proposalId: "proposal-smoke-missing-journal" },
      status: "pending",
      topic: "business.invoice.prepare_send"
    },
    proposal: {
      companyId,
      confidence: 0.98,
      createdByAgent: "invoice-checker",
      evidence: { smoke: true },
      id: "proposal-smoke-missing-journal",
      kind: "invoice_check",
      proposedCommand: {
        companyId,
        idempotencyKey: `${companyId}:SendInvoice:invoice:inv-smoke-missing-journal`,
        payload: { invoiceId: "inv-smoke-missing-journal", invoiceNumber: "RE-SMOKE-MISSING" },
        refId: "inv-smoke-missing-journal",
        refType: "invoice",
        requestedAt: "2026-05-07T00:00:00.000Z",
        requestedBy: "smoke",
        type: "SendInvoice"
      },
      refId: "inv-smoke-missing-journal",
      refType: "invoice",
      status: "open"
    }
  };
}

function buildClosedPeriodWorkflowSnapshot(): AccountingWorkflowSnapshot {
  return {
    audit: {
      action: "invoice.prepare_send",
      actorId: "smoke",
      actorType: "system",
      after: { status: "closed_period_attempt" },
      companyId,
      refId: "inv-smoke-001",
      refType: "invoice"
    },
    outbox: {
      companyId,
      id: "outbox-smoke-closed-period",
      payload: { proposalId: "proposal-smoke-closed-period" },
      status: "pending",
      topic: "business.invoice.prepare_send"
    },
    proposal: {
      companyId,
      confidence: 0.98,
      createdByAgent: "invoice-checker",
      evidence: { smoke: true },
      id: "proposal-smoke-closed-period",
      kind: "invoice_check",
      proposedCommand: {
        companyId,
        idempotencyKey: `${companyId}:SendInvoice:invoice:inv-smoke-001:closed-period`,
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

async function expectProposalDecisionBlocked(externalId: string, expectedMessage: string, databaseUrl: string) {
  try {
    await decideAccountingProposal({
      actorId: "smoke-user",
      externalId,
      status: "accepted"
    }, databaseUrl);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (message.includes(expectedMessage)) return;
    throw error;
  }
  throw new Error(`expected proposal decision to be blocked: ${externalId}`);
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
