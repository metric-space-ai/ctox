import { getBusinessBundle, businessCurrency, text, type BusinessInvoice } from "@/lib/business-seed";

export async function GET(
  request: Request,
  { params }: { params: Promise<{ id: string }> }
) {
  const { id } = await params;
  const locale = new URL(request.url).searchParams.get("locale") === "en" ? "en" : "de";
  const data = await getBusinessBundle();
  const invoice = data.invoices.find((item) => item.id === id);

  if (!invoice) {
    return new Response("Invoice not found", { status: 404 });
  }

  const customer = data.customers.find((item) => item.id === invoice.customerId);
  const lines = invoice.lines.map((line, index) => {
    const product = data.products.find((item) => item.id === line.productId);
    return `${index + 1}. ${product?.name ?? line.productId}  ${line.quantity} x ${businessCurrency(line.unitPrice, invoice.currency, locale)}  ${businessCurrency(line.quantity * line.unitPrice, invoice.currency, locale)}`;
  });
  const pdf = createInvoicePdf(invoice, [
    locale === "de" ? "Rechnung" : "Invoice",
    `${locale === "de" ? "Rechnungsnummer" : "Invoice number"}: ${invoice.number}`,
    `${locale === "de" ? "Datum" : "Date"}: ${invoice.issueDate}`,
    `${locale === "de" ? "Kunde" : "Customer"}: ${customer?.name ?? invoice.customerId}`,
    "",
    text(invoice.introText ?? invoice.notes, locale),
    "",
    ...lines,
    "",
    `${locale === "de" ? "Zwischensumme netto" : "Subtotal net"}: ${businessCurrency(invoice.netAmount ?? invoice.total - invoice.taxAmount, invoice.currency, locale)}`,
    `${locale === "de" ? "Umsatzsteuer" : "VAT"}: ${businessCurrency(invoice.taxAmount, invoice.currency, locale)}`,
    `${locale === "de" ? "Gesamtbetrag" : "Total"}: ${businessCurrency(invoice.total, invoice.currency, locale)}`,
    "",
    text(invoice.paymentTermsText ?? customer?.paymentTerms ?? "", locale),
    text(invoice.closingText ?? invoice.notes, locale)
  ]);

  return new Response(pdf, {
    headers: {
      "Content-Disposition": `inline; filename="Rechnung_${safeFilename(invoice.number)}_${safeFilename(invoice.issueDate)}.pdf"`,
      "Content-Type": "application/pdf"
    }
  });
}

function createInvoicePdf(invoice: BusinessInvoice, lines: string[]) {
  const content = [
    "BT",
    "/F1 22 Tf",
    "72 780 Td",
    `(${pdfText(invoice.documentTitle ?? "Rechnung")}) Tj`,
    "/F1 11 Tf",
    "0 -34 Td",
    ...lines.flatMap((line) => ["0 -18 Td", `(${pdfText(line)}) Tj`]),
    "ET"
  ].join("\n");
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    `<< /Length ${byteLength(content)} >>\nstream\n${content}\nendstream`
  ];
  let pdf = "%PDF-1.4\n";
  const offsets = [0];
  objects.forEach((object, index) => {
    offsets.push(byteLength(pdf));
    pdf += `${index + 1} 0 obj\n${object}\nendobj\n`;
  });
  const xrefOffset = byteLength(pdf);
  pdf += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  pdf += offsets.slice(1).map((offset) => `${String(offset).padStart(10, "0")} 00000 n \n`).join("");
  pdf += `trailer\n<< /Root 1 0 R /Size ${objects.length + 1} >>\nstartxref\n${xrefOffset}\n%%EOF`;
  return new Uint8Array(Buffer.from(pdf, "binary"));
}

function pdfText(value: string) {
  return value
    .normalize("NFKD")
    .replace(/[^\x20-\x7E]/g, "")
    .replace(/\\/g, "\\\\")
    .replace(/\(/g, "\\(")
    .replace(/\)/g, "\\)");
}

function safeFilename(value: string) {
  return value.replace(/[^a-z0-9.-]+/gi, "_");
}

function byteLength(value: string) {
  return Buffer.byteLength(value, "binary");
}
