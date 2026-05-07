import { germanTaxRates, seedChartAccounts } from "@ctox-business/accounting";
import { saveAccountingSetupSnapshot } from "@ctox-business/db/accounting";
import { NextResponse } from "next/server";
import { getBusinessBundle } from "@/lib/business-seed";

const companyId = "business-basic-company";

export async function POST() {
  const data = await getBusinessBundle();
  const snapshot = {
    accounts: seedChartAccounts({ chart: "skr03", companyId }),
    fiscalPeriods: [{
      companyId,
      endDate: "2026-12-31",
      externalId: "fy-2026",
      startDate: "2026-01-01",
      status: "open"
    }],
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
    taxRates: germanTaxRates.map((taxRate) => ({
      accountId: taxRate.accountId,
      code: taxRate.code,
      companyId,
      externalId: `tax-${taxRate.code.toLowerCase()}`,
      rate: taxRate.rate,
      type: taxRate.type
    }))
  };

  if (!process.env.DATABASE_URL) {
    return NextResponse.json({
      persisted: false,
      reason: "DATABASE_URL not configured",
      snapshot
    });
  }

  await saveAccountingSetupSnapshot(snapshot);
  return NextResponse.json({ persisted: true, snapshot });
}

function vendorExternalId(vendorName: string) {
  return `vendor-${vendorName.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "")}`;
}
