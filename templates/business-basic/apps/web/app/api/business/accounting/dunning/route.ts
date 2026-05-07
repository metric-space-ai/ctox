import { buildDunningProposals } from "@ctox-business/accounting/dunning";
import { NextResponse } from "next/server";
import { getBusinessBundle } from "@/lib/business-seed";

export async function POST() {
  const data = await getBusinessBundle();
  const proposals = buildDunningProposals({
    asOf: "2026-05-07",
    companyId: "business-basic-company",
    invoices: data.invoices.map((invoice) => ({
      balanceDue: invoice.balanceDue ?? invoice.total,
      customerId: invoice.customerId,
      dueDate: invoice.dueDate,
      id: invoice.id,
      number: invoice.number,
      reminderLevel: invoice.reminderLevel,
      status: invoice.status
    })),
    requestedBy: "dunning-assistant"
  });

  return NextResponse.json({ proposals });
}
