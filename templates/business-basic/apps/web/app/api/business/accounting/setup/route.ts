import {
  createAccountingAuditEvent,
  createBusinessOutboxEvent,
  germanTaxRatesForChart,
  seedChartAccounts
} from "@ctox-business/accounting";
import { saveAccountingSetupSnapshot, saveAccountingWorkflowSnapshot } from "@ctox-business/db/accounting";
import { NextResponse } from "next/server";
import { getBusinessBundle } from "@/lib/business-seed";

const companyId = "business-basic-company";

export async function POST(request: Request) {
  const body = await parseJsonBody(request);
  const chart = body?.chart === "skr04" ? "skr04" : "skr03";
  return prepareAccountingSetup(chart);
}

export async function PUT(request: Request) {
  const body = await parseJsonBody(request);
  const chart = body?.chart === "skr04" ? "skr04" : "skr03";
  return prepareAccountingSetup(chart);
}

async function prepareAccountingSetup(chart: "skr03" | "skr04") {
  const data = await getBusinessBundle();
  const snapshot = {
    accounts: seedChartAccounts({ chart, companyId }),
    chart,
    fiscalPeriods: fiscalPeriods2026(),
    parties: [
      ...data.customers.map((customer) => ({
        companyId,
        defaultReceivableAccountId: "acc-ar",
        externalId: customer.id,
        kind: "customer",
        name: customer.name,
        taxId: customer.taxId
      })),
      ...data.receipts.map((receipt) => ({
        companyId,
        defaultPayableAccountId: receipt.payableAccountId,
        externalId: vendorExternalId(receipt.vendorName),
        kind: "vendor",
        name: receipt.vendorName
      }))
    ],
    taxRates: germanTaxRatesForChart(chart).map((taxRate) => ({
      accountCode: taxRate.accountCode,
      accountId: taxRate.accountId,
      code: taxRate.code,
      companyId,
      externalId: `tax-${taxRate.code.toLowerCase()}`,
      rate: taxRate.rate,
      type: taxRate.type
    }))
  };
  const audit = createAccountingAuditEvent({
    action: "accounting.setup.prepare",
    actorId: "business-runtime",
    actorType: "system",
    after: {
      accountCount: snapshot.accounts.length,
      chart,
      fiscalPeriodCount: snapshot.fiscalPeriods.length,
      partyCount: snapshot.parties.length,
      taxRateCount: snapshot.taxRates.length
    },
    companyId,
    refId: companyId,
    refType: "company"
  });
  const outbox = createBusinessOutboxEvent({
    companyId,
    id: `outbox-business.accounting.setup-${companyId}`,
    payload: {
      accountCount: snapshot.accounts.length,
      chart,
      fiscalPeriodCount: snapshot.fiscalPeriods.length,
      partyCount: snapshot.parties.length,
      taxRateCount: snapshot.taxRates.length
    },
    topic: "business.accounting.setup"
  });
  const workflow = { audit, outbox };

  if (!process.env.DATABASE_URL) {
    return NextResponse.json({
      persisted: false,
      reason: "DATABASE_URL not configured",
      snapshot,
      workflow
    });
  }

  await saveAccountingSetupSnapshot(snapshot);
  await saveAccountingWorkflowSnapshot(workflow);
  return NextResponse.json({ persisted: true, snapshot, workflow });
}

async function parseJsonBody(request: Request) {
  try {
    return await request.json() as { chart?: unknown };
  } catch {
    return null;
  }
}

function vendorExternalId(vendorName: string) {
  const normalized = vendorName.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  return normalized.startsWith("vendor-") ? normalized.replace(/^(vendor-)+/, "vendor-") : `vendor-${normalized}`;
}

function fiscalPeriods2026() {
  const months = Array.from({ length: 12 }, (_, index) => {
    const month = index + 1;
    const start = new Date(Date.UTC(2026, index, 1));
    const end = new Date(Date.UTC(2026, month, 0));
    return {
      companyId,
      endDate: isoDate(end),
      externalId: `fy-2026-${String(month).padStart(2, "0")}`,
      startDate: isoDate(start),
      status: "open"
    };
  });

  return [
    {
      companyId,
      endDate: "2026-12-31",
      externalId: "fy-2026",
      startDate: "2026-01-01",
      status: "open"
    },
    ...months
  ];
}

function isoDate(date: Date) {
  return date.toISOString().slice(0, 10);
}
