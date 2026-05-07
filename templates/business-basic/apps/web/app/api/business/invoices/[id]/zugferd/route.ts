import { NextResponse } from "next/server";
import { getBusinessBundle } from "@/lib/business-seed";
import { getDatabaseBackedBusinessBundle } from "@/lib/business-db-bundle";
import { prepareExistingInvoiceForAccounting } from "@/lib/business-accounting";

export async function GET(
  request: Request,
  { params }: { params: Promise<{ id: string }> }
) {
  const { id } = await params;
  const locale = new URL(request.url).searchParams.get("locale") === "en" ? "en" : "de";
  const data = await getDatabaseBackedBusinessBundle(await getBusinessBundle());
  const invoice = data.invoices.find((item) => item.id === id);

  if (!invoice) {
    return NextResponse.json({ error: "invoice_not_found" }, { status: 404 });
  }

  const accounting = prepareExistingInvoiceForAccounting({ data, invoice, locale });
  return new Response(accounting.zugferdXml, {
    headers: {
      "content-disposition": `attachment; filename="${invoice.number}-zugferd.xml"`,
      "content-type": "application/xml; charset=utf-8"
    }
  });
}
