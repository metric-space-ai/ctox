import { buildInvoiceDocument as buildAccountingInvoiceDocument, buildZugferdXml } from "@ctox-business/accounting/invoice";
import { getBusinessBundle, text, type BusinessInvoice } from "@/lib/business-seed";

type PdfLine = {
  description: string;
  quantity: string;
  title: string;
  total: string;
  unit: string;
  unitPrice: string;
};

type PdfDocument = {
  amountLabel: string;
  body: string;
  closingText: string;
  customerNumber?: string;
  dueDate?: string;
  footerLeft: string[];
  footerRight: string[];
  issueDate: string;
  lines: PdfLine[];
  number: string;
  paymentTerms: string;
  recipientLines: string[];
  senderLine: string;
  senderLines: string[];
  serviceDate?: string;
  subtotalAmount: string;
  subtotalLabel: string;
  taxAmount: string;
  taxLabel: string;
  title: string;
  totalLabel: string;
  typeLabel: string;
};

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
  const document = buildInvoiceDocument({
    customerName: customer?.name ?? invoice.customerId,
    customerPaymentTerms: customer?.paymentTerms,
    invoice,
    locale,
    productName: (productId: string) => data.products.find((item) => item.id === productId)?.name ?? productId,
    productDescription: (productId: string) => text(data.products.find((item) => item.id === productId)?.description ?? "", locale),
    productUnit: (productId: string) => data.products.find((item) => item.id === productId)?.type === "Service"
      ? (locale === "de" ? "Stunde" : "Hour")
      : (locale === "de" ? "Stück" : "Piece")
  });
  const zugferdXml = buildZugferdXml(invoice, buildInvoiceContext({
    customerName: customer?.name ?? invoice.customerId,
    customerPaymentTerms: customer?.paymentTerms,
    invoice,
    locale,
    productDescription: (productId: string) => text(data.products.find((item) => item.id === productId)?.description ?? "", locale),
    productName: (productId: string) => data.products.find((item) => item.id === productId)?.name ?? productId,
    productUnit: (productId: string) => data.products.find((item) => item.id === productId)?.type === "Service"
      ? (locale === "de" ? "Stunde" : "Hour")
      : (locale === "de" ? "Stück" : "Piece")
  }));
  const pdf = createInvoicePdf(document, {
    attachmentDescription: "ZUGFeRD invoice XML",
    attachmentFilename: "zugferd-invoice.xml",
    attachmentXml: zugferdXml
  });

  return new Response(pdf, {
    headers: {
      "Content-Disposition": `inline; filename="Rechnung_${safeFilename(invoice.number)}_${safeFilename(invoice.issueDate)}.pdf"`,
      "Content-Type": "application/pdf"
    }
  });
}

function buildInvoiceDocument({
  customerName,
  customerPaymentTerms,
  invoice,
  locale,
  productDescription,
  productName,
  productUnit
}: {
  customerName: string;
  customerPaymentTerms?: string;
  invoice: BusinessInvoice;
  locale: "en" | "de";
  productDescription: (productId: string) => string;
  productName: (productId: string) => string;
  productUnit: (productId: string) => string;
}): PdfDocument {
  const senderLines = [
    "Metric Space UG (haftungsbeschränkt)",
    "Lämmersieht 21",
    "22305 Hamburg",
    "Tel.: +49 176 23424399",
    "info@metric-space.ai",
    "metric-space.ai"
  ];
  const footerLeft = [
    "Metric Space UG (haftungsbeschränkt)",
    "Lämmersieht 21",
    "22305 Hamburg",
    "Tel.: +49 176 23424399",
    "info@metric-space.ai"
  ];
  const footerRight = [
    "Steuernummer: 43/743/02774",
    "Handelsregister B des Amtsgerichts",
    "Hamburg HRB 176693",
    "Geschäftsführer Michael Welsch"
  ];
  const baseDocument = buildAccountingInvoiceDocument(invoice, buildInvoiceContext({
    customerName,
    customerPaymentTerms,
    invoice,
    locale,
    productDescription,
    productName,
    productUnit
  }));

  return {
    ...baseDocument,
    footerLeft,
    footerRight,
    senderLine: "Metric Space UG (haftungsbeschränkt), Lämmersieht 21, 22305 Hamburg",
    senderLines
  };
}

function buildInvoiceContext({
  customerName,
  customerPaymentTerms,
  invoice,
  locale,
  productDescription,
  productName,
  productUnit
}: {
  customerName: string;
  customerPaymentTerms?: string;
  invoice: BusinessInvoice;
  locale: "en" | "de";
  productDescription: (productId: string) => string;
  productName: (productId: string) => string;
  productUnit: (productId: string) => string;
}) {
  return {
    companyId: "business-basic-company",
    companyName: "Metric Space UG (haftungsbeschränkt)",
    customer: {
      id: invoice.customerId,
      name: customerName,
      paymentTerms: customerPaymentTerms
    },
    defaultReceivableAccountId: "acc-ar",
    defaultRevenueAccountId: "acc-revenue-saas",
    defaultTaxAccountId: "acc-vat-output",
    issuerAddressLines: ["Metric Space UG (haftungsbeschraenkt)", "Laemmersieht 21", "22305 Hamburg", "Deutschland"],
    issuerTaxId: "43/743/02774",
    issuerVatId: "DE123456789",
    locale,
    products: invoice.lines.map((line) => ({
      description: productDescription(line.productId),
      id: line.productId,
      name: productName(line.productId),
      type: productUnit(line.productId) === (locale === "de" ? "Stunde" : "Hour") ? "Service" : "Product"
    }))
  };
}

function createInvoicePdf(document: PdfDocument, attachment?: { attachmentDescription: string; attachmentFilename: string; attachmentXml: string }) {
  const page = { width: 595, height: 842 };
  const margin = 48;
  const right = page.width - margin;
  const ops: string[] = [];
  const textOps = {
    large: (value: string, x: number, y: number) => drawText(ops, value, x, y, 22, "F2"),
    medium: (value: string, x: number, y: number) => drawText(ops, value, x, y, 12, "F2"),
    small: (value: string, x: number, y: number) => drawText(ops, value, x, y, 8),
    body: (value: string, x: number, y: number) => drawText(ops, value, x, y, 10),
    bodyBold: (value: string, x: number, y: number) => drawText(ops, value, x, y, 10, "F2")
  };

  textOps.large(document.title, 390, 790);
  textOps.small(document.senderLine, margin, 705);
  drawLine(ops, margin, 701, 250, 701, 0.5);
  drawWrappedText(ops, document.recipientLines.filter(Boolean), margin, 682, 240, 11, 13);

  let senderY = 735;
  document.senderLines.forEach((line, index) => {
    drawText(ops, line, right, senderY, index === 0 ? 10 : 8, index === 0 ? "F2" : "F1", "right");
    senderY -= index === 0 ? 13 : 10;
  });

  const factRows = [
    [`${document.typeLabel}snr.:`, document.number],
    ...(document.customerNumber ? [["Kundennr.:", document.customerNumber]] : []),
    ["Datum:", document.issueDate],
    ...(document.serviceDate ? [["Lieferdatum:", document.serviceDate]] : []),
    ...(document.dueDate ? [["Fällig:", document.dueDate]] : [])
  ];
  let factY = 650;
  factRows.forEach(([label, value]) => {
    textOps.bodyBold(label, 365, factY);
    drawText(ops, value, right, factY, 10, "F1", "right");
    factY -= 14;
  });

  textOps.medium(document.title, 390, 548);
  let y = drawParagraph(ops, document.body, margin, 520, 500, 10, 14) - 18;
  y = drawInvoiceTable(ops, document, margin, y, 500);
  y -= 16;
  y = drawParagraph(ops, document.paymentTerms, margin, y, 500, 10, 14) - 8;
  drawParagraph(ops, document.closingText, margin, y, 500, 10, 14);

  drawLine(ops, margin, 82, right, 82, 0.5);
  drawWrappedText(ops, document.footerLeft, margin, 68, 210, 7, 9);
  drawWrappedText(ops, document.footerRight, 320, 68, 200, 7, 9);
  drawText(ops, "Seite 1/1", 297, 24, 7, "F1", "center");

  return makePdf(ops.join("\n"), attachment);
}

function drawInvoiceTable(ops: string[], document: PdfDocument, x: number, y: number, width: number) {
  const cols = [28, 214, 46, 54, 75, 83];
  const headers = ["Pos.", "Bezeichnung", "Menge", "Einheit", "Einzel", "Gesamt"];
  const rowMinHeight = 34;
  const tableTop = y;
  const tableBottoms: number[] = [];
  drawRect(ops, x, y - 22, width, 22, "0.93 0.95 0.96");
  let cursorX = x;
  headers.forEach((header, index) => {
    drawText(ops, header, index >= 2 ? cursorX + cols[index] - 5 : cursorX + 5, y - 14, 8, "F2", index >= 2 ? "right" : "left", cols[index] - 10);
    cursorX += cols[index];
  });
  drawGridLine(ops, x, y, width, 0);
  drawGridLine(ops, x, y - 22, width, 0);
  y -= 22;

  document.lines.forEach((line, index) => {
    const descriptionLines = wrapText(line.description, 32);
    const rowHeight = Math.max(rowMinHeight, 24 + descriptionLines.length * 10);
    drawGridLine(ops, x, y - rowHeight, width, 0);
    cursorX = x;
    drawText(ops, String(index + 1), cursorX + cols[0] - 5, y - 17, 9, "F1", "right");
    cursorX += cols[0];
    drawText(ops, line.title, cursorX + 5, y - 17, 9, "F2");
    descriptionLines.slice(0, 2).forEach((description, descriptionIndex) => {
      drawText(ops, description, cursorX + 5, y - 30 - descriptionIndex * 10, 7);
    });
    cursorX += cols[1];
    drawText(ops, line.quantity, cursorX + cols[2] - 5, y - 17, 9, "F1", "right");
    cursorX += cols[2];
    drawText(ops, line.unit, cursorX + cols[3] - 5, y - 17, 9, "F1", "right");
    cursorX += cols[3];
    drawText(ops, line.unitPrice, cursorX + cols[4] - 5, y - 17, 9, "F1", "right");
    cursorX += cols[4];
    drawText(ops, line.total, cursorX + cols[5] - 5, y - 17, 9, "F1", "right");
    y -= rowHeight;
    tableBottoms.push(y);
  });

  const totalRows = [
    [document.subtotalLabel, document.subtotalAmount, false],
    [document.taxLabel, document.taxAmount, false],
    [document.totalLabel, document.amountLabel, true]
  ] as const;
  totalRows.forEach(([label, value, bold]) => {
    drawGridLine(ops, x + cols[0] + cols[1] + cols[2], y - 24, cols[3] + cols[4] + cols[5], 0);
    drawText(ops, label, x + width - cols[5] - 10, y - 15, 9, bold ? "F2" : "F1", "right");
    drawText(ops, value, x + width - 5, y - 15, 9, bold ? "F2" : "F1", "right");
    y -= 24;
    tableBottoms.push(y);
  });

  let lineX = x;
  cols.slice(0, -1).forEach((colWidth) => {
    lineX += colWidth;
    drawLine(ops, lineX, y, lineX, tableTop, 0.35);
  });
  drawLine(ops, x, y, x, tableTop, 0.35);
  drawLine(ops, x + width, y, x + width, tableTop, 0.35);
  tableBottoms.forEach((bottom) => {
    drawLine(ops, x, bottom, x + width, bottom, 0.35);
  });

  return y;
}

function drawParagraph(ops: string[], value: string, x: number, y: number, width: number, size: number, lineHeight: number) {
  const lines = wrapText(value, Math.max(20, Math.floor(width / (size * 0.52))));
  lines.forEach((line, index) => drawText(ops, line, x, y - index * lineHeight, size));
  return y - lines.length * lineHeight;
}

function drawWrappedText(ops: string[], lines: string[], x: number, y: number, width: number, size: number, lineHeight: number) {
  let currentY = y;
  lines.forEach((line) => {
    wrapText(line, Math.max(18, Math.floor(width / (size * 0.52)))).forEach((wrapped) => {
      drawText(ops, wrapped, x, currentY, size);
      currentY -= lineHeight;
    });
  });
  return currentY;
}

function drawText(ops: string[], value: string, x: number, y: number, size: number, font = "F1", align: "left" | "right" | "center" = "left", width = 0) {
  const safe = pdfText(value);
  const estimatedWidth = width || estimateTextWidth(safe, size);
  const tx = align === "right" ? x - estimatedWidth : align === "center" ? x - estimatedWidth / 2 : x;
  ops.push(`BT /${font} ${size} Tf 1 0 0 1 ${round(tx)} ${round(y)} Tm (${safe}) Tj ET`);
}

function drawLine(ops: string[], x1: number, y1: number, x2: number, y2: number, width: number) {
  ops.push(`q ${width} w 0.72 0.76 0.78 RG ${round(x1)} ${round(y1)} m ${round(x2)} ${round(y2)} l S Q`);
}

function drawGridLine(ops: string[], x: number, y: number, width: number, _unused: number) {
  drawLine(ops, x, y, x + width, y, 0.35);
}

function drawRect(ops: string[], x: number, y: number, width: number, height: number, fill: string) {
  ops.push(`q ${fill} rg ${round(x)} ${round(y)} ${round(width)} ${round(height)} re f Q`);
}

function makePdf(content: string, attachment?: { attachmentDescription: string; attachmentFilename: string; attachmentXml: string }) {
  const catalog = attachment
    ? "<< /Type /Catalog /Pages 2 0 R /Names << /EmbeddedFiles << /Names [(zugferd-invoice.xml) 8 0 R] >> >> /AF [8 0 R] >>"
    : "<< /Type /Catalog /Pages 2 0 R >>";
  const objects = [
    catalog,
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 4 0 R /F2 5 0 R >> >> /Contents 6 0 R >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>",
    `<< /Length ${byteLength(content)} >>\nstream\n${content}\nendstream`
  ];
  if (attachment) {
    const filename = pdfString(attachment.attachmentFilename);
    const description = pdfString(attachment.attachmentDescription);
    objects.push(
      `<< /Type /EmbeddedFile /Subtype /text#2Fxml /Params << /Size ${byteLength(attachment.attachmentXml)} >> /Length ${byteLength(attachment.attachmentXml)} >>\nstream\n${attachment.attachmentXml}\nendstream`,
      `<< /Type /Filespec /F (${filename}) /UF (${filename}) /AFRelationship /Alternative /Desc (${description}) /EF << /F 7 0 R /UF 7 0 R >> >>`
    );
  }
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
  return new Uint8Array(Buffer.from(pdf, "utf8"));
}

function pdfText(value: string) {
  return value
    .replace(/\u00a0/g, " ")
    .replace(/Ä/g, "Ae")
    .replace(/Ö/g, "Oe")
    .replace(/Ü/g, "Ue")
    .replace(/ä/g, "ae")
    .replace(/ö/g, "oe")
    .replace(/ü/g, "ue")
    .replace(/ß/g, "ss")
    .replace(/€/g, "EUR")
    .replace(/\$/g, "USD")
    .replace(/[“”]/g, "\"")
    .replace(/[‘’]/g, "'")
    .replace(/[–—]/g, "-")
    .replace(/[^\x20-\x7E]/g, "")
    .replace(/\\/g, "\\\\")
    .replace(/\(/g, "\\(")
    .replace(/\)/g, "\\)");
}

function pdfString(value: string) {
  return value
    .replace(/[^\x20-\x7E]/g, "")
    .replace(/\\/g, "\\\\")
    .replace(/\(/g, "\\(")
    .replace(/\)/g, "\\)");
}

function wrapText(value: string, maxChars: number) {
  const words = value.replace(/\s+/g, " ").trim().split(" ").filter(Boolean);
  const lines: string[] = [];
  let current = "";
  words.forEach((word) => {
    const next = current ? `${current} ${word}` : word;
    if (next.length <= maxChars) {
      current = next;
      return;
    }
    if (current) lines.push(current);
    current = word;
  });
  if (current) lines.push(current);
  return lines.length ? lines : [""];
}

function estimateTextWidth(value: string, size: number) {
  return value.replace(/\\[()\\]/g, "x").length * size * 0.52;
}

function safeFilename(value: string) {
  return value.replace(/[^a-z0-9.-]+/gi, "_");
}

function byteLength(value: string) {
  return Buffer.byteLength(value, "utf8");
}

function round(value: number) {
  return Number(value.toFixed(2));
}
