import { readFile } from "node:fs/promises";
import { basename, join } from "node:path";
import pg from "pg";
import { closeBusinessDb } from "../src/client";
import {
  closeAccountingFiscalPeriod,
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
  await saveAccountingWorkflowSnapshot(workflow, databaseUrl);
  await saveAccountingWorkflowSnapshot(workflow, databaseUrl);

  await expectCount(client, "accounting_accounts", 4);
  await expectCount(client, "accounting_parties", 1);
  await expectCount(client, "accounting_tax_rates", 1);
  await expectCount(client, "accounting_invoices", 1);
  await expectCount(client, "accounting_invoice_lines", 2);
  await expectCount(client, "accounting_journal_entries", 1);
  await expectCount(client, "accounting_journal_entry_lines", 3);
  await expectCount(client, "accounting_ledger_entries", 3);
  await expectCount(client, "business_accounting_proposals", 1);
  await expectCount(client, "business_outbox_events", 1);

  const auditCount = Number((await client.query("SELECT count(*)::int AS count FROM business_accounting_audit_events")).rows[0]?.count ?? 0);
  if (auditCount < 2) throw new Error(`expected repeated audit rows, got ${auditCount}`);

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
      account("acc-revenue", "8400", "SaaS subscriptions", "income", "income"),
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
    }],
    taxRates: [{
      accountId: "acc-vat-output",
      code: "DE_19",
      companyId,
      externalId: "tax-de-19",
      rate: 19,
      type: "output"
    }]
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
      payload: { invoiceId: "inv-smoke-001" },
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
      proposedCommand: { type: "SendInvoice", invoiceId: "inv-smoke-001" },
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
