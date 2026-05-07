import { formatMoney, moneyFromMajor } from "../money";
import type { BusinessInvoiceLike, InvoiceContext, InvoiceDocument, LocalizedValue } from "./types";

export function buildInvoiceDocument(invoice: BusinessInvoiceLike, context: InvoiceContext): InvoiceDocument {
  const locale = context.locale ?? "de";
  const formatterLocale = locale === "de" ? "de-DE" : "en-US";
  const customer = context.customer;
  const lines = invoice.lines.map((line) => {
    const product = context.products.find((item) => item.id === line.productId);
    return {
      description: localized(product?.description ?? "", locale),
      quantity: line.quantity.toLocaleString(formatterLocale, { maximumFractionDigits: 2 }),
      title: product?.name ?? line.productId,
      total: formatMoney(moneyFromMajor(line.quantity * line.unitPrice, invoice.currency), formatterLocale),
      unit: product?.type === "Service" ? (locale === "de" ? "Stunde" : "Hour") : (locale === "de" ? "Stück" : "Piece"),
      unitPrice: formatMoney(moneyFromMajor(line.unitPrice, invoice.currency), formatterLocale)
    };
  });
  const netAmount = invoice.netAmount ?? invoice.lines.reduce((sum, line) => sum + line.quantity * line.unitPrice, 0);

  return {
    amountLabel: formatMoney(moneyFromMajor(invoice.total, invoice.currency), formatterLocale),
    body: localized(invoice.introText ?? invoice.notes ?? "", locale),
    closingText: localized(invoice.closingText ?? invoice.notes ?? "", locale),
    customerNumber: invoice.customerNumber,
    dueDate: invoice.dueDate,
    issueDate: invoice.issueDate,
    lines,
    number: invoice.number,
    paymentTerms: localized(invoice.paymentTermsText ?? customer?.paymentTerms ?? "", locale),
    recipientLines: invoice.addressLines ?? [customer?.name ?? invoice.customerId, customer?.country ?? ""].filter(Boolean),
    serviceDate: invoice.serviceDate,
    subtotalAmount: formatMoney(moneyFromMajor(netAmount, invoice.currency), formatterLocale),
    subtotalLabel: locale === "de" ? "Zwischensumme (netto)" : "Subtotal (net)",
    taxAmount: formatMoney(moneyFromMajor(invoice.taxAmount, invoice.currency), formatterLocale),
    taxLabel: locale === "de" ? "Umsatzsteuer" : "VAT",
    title: invoice.documentTitle ?? (locale === "de" ? "Rechnung" : "Invoice"),
    totalLabel: locale === "de" ? "Gesamtbetrag" : "Total",
    typeLabel: locale === "de" ? "Rechnung" : "Invoice"
  };
}

export function buildZugferdXml(invoice: BusinessInvoiceLike, context: InvoiceContext) {
  const customerName = context.customer?.name ?? invoice.customerId;
  const netAmount = invoice.netAmount ?? invoice.lines.reduce((sum, line) => sum + line.quantity * line.unitPrice, 0);
  const taxCategory = invoice.kleinunternehmer || context.kleinunternehmer ? "E" : invoice.reverseCharge ? "AE" : "S";
  const taxReason = invoice.kleinunternehmer || context.kleinunternehmer
    ? "Nicht steuerbar wegen Anwendung der Kleinunternehmerregelung (§ 19 UStG)."
    : invoice.reverseCharge
      ? "Reverse Charge - Steuerschuldnerschaft des Leistungsempfängers."
      : "";
  const lineXml = invoice.lines.map((line, index) => {
    const product = context.products.find((item) => item.id === line.productId);
    const lineNet = line.quantity * line.unitPrice;
    return [
      "    <ram:IncludedSupplyChainTradeLineItem>",
      `      <ram:AssociatedDocumentLineDocument><ram:LineID>${index + 1}</ram:LineID></ram:AssociatedDocumentLineDocument>`,
      `      <ram:SpecifiedTradeProduct><ram:Name>${escapeXml(product?.name ?? line.productId)}</ram:Name></ram:SpecifiedTradeProduct>`,
      "      <ram:SpecifiedLineTradeAgreement>",
      `        <ram:NetPriceProductTradePrice><ram:ChargeAmount>${money(line.unitPrice)}</ram:ChargeAmount></ram:NetPriceProductTradePrice>`,
      "      </ram:SpecifiedLineTradeAgreement>",
      "      <ram:SpecifiedLineTradeDelivery>",
      `        <ram:BilledQuantity unitCode="C62">${line.quantity}</ram:BilledQuantity>`,
      "      </ram:SpecifiedLineTradeDelivery>",
      "      <ram:SpecifiedLineTradeSettlement>",
      `        <ram:ApplicableTradeTax><ram:TypeCode>VAT</ram:TypeCode><ram:CategoryCode>${taxCategory}</ram:CategoryCode><ram:RateApplicablePercent>${line.taxRate}</ram:RateApplicablePercent></ram:ApplicableTradeTax>`,
      `        <ram:SpecifiedTradeSettlementLineMonetarySummation><ram:LineTotalAmount>${money(lineNet)}</ram:LineTotalAmount></ram:SpecifiedTradeSettlementLineMonetarySummation>`,
      "      </ram:SpecifiedLineTradeSettlement>",
      "    </ram:IncludedSupplyChainTradeLineItem>"
    ].join("\n");
  }).join("\n");

  return [
    '<?xml version="1.0" encoding="UTF-8"?>',
    '<rsm:CrossIndustryInvoice xmlns:rsm="urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100" xmlns:ram="urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100" xmlns:udt="urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100">',
    "  <rsm:ExchangedDocument>",
    `    <ram:ID>${escapeXml(invoice.number)}</ram:ID>`,
    "    <ram:TypeCode>380</ram:TypeCode>",
    `    <ram:IssueDateTime><udt:DateTimeString format="102">${invoice.issueDate.replace(/-/g, "")}</udt:DateTimeString></ram:IssueDateTime>`,
    "  </rsm:ExchangedDocument>",
    "  <rsm:SupplyChainTradeTransaction>",
    lineXml,
    "    <ram:ApplicableHeaderTradeAgreement>",
    `      <ram:SellerTradeParty><ram:Name>${escapeXml(context.companyName)}</ram:Name></ram:SellerTradeParty>`,
    `      <ram:BuyerTradeParty><ram:Name>${escapeXml(customerName)}</ram:Name></ram:BuyerTradeParty>`,
    "    </ram:ApplicableHeaderTradeAgreement>",
    "    <ram:ApplicableHeaderTradeDelivery>",
    `      <ram:ActualDeliverySupplyChainEvent><ram:OccurrenceDateTime><udt:DateTimeString format="102">${(invoice.serviceDate ?? invoice.issueDate).replace(/-/g, "")}</udt:DateTimeString></ram:OccurrenceDateTime></ram:ActualDeliverySupplyChainEvent>`,
    "    </ram:ApplicableHeaderTradeDelivery>",
    "    <ram:ApplicableHeaderTradeSettlement>",
    `      <ram:InvoiceCurrencyCode>${escapeXml(invoice.currency)}</ram:InvoiceCurrencyCode>`,
    `      <ram:ApplicableTradeTax><ram:CalculatedAmount>${money(invoice.taxAmount)}</ram:CalculatedAmount><ram:TypeCode>VAT</ram:TypeCode><ram:CategoryCode>${taxCategory}</ram:CategoryCode><ram:BasisAmount>${money(netAmount)}</ram:BasisAmount><ram:RateApplicablePercent>${invoice.lines[0]?.taxRate ?? 0}</ram:RateApplicablePercent>${taxReason ? `<ram:ExemptionReason>${escapeXml(taxReason)}</ram:ExemptionReason>` : ""}</ram:ApplicableTradeTax>`,
    `      <ram:SpecifiedTradePaymentTerms><ram:DueDateDateTime><udt:DateTimeString format="102">${invoice.dueDate.replace(/-/g, "")}</udt:DateTimeString></ram:DueDateDateTime></ram:SpecifiedTradePaymentTerms>`,
    "      <ram:SpecifiedTradeSettlementHeaderMonetarySummation>",
    `        <ram:LineTotalAmount>${money(netAmount)}</ram:LineTotalAmount>`,
    `        <ram:TaxBasisTotalAmount>${money(netAmount)}</ram:TaxBasisTotalAmount>`,
    `        <ram:TaxTotalAmount currencyID="${escapeXml(invoice.currency)}">${money(invoice.taxAmount)}</ram:TaxTotalAmount>`,
    `        <ram:GrandTotalAmount>${money(invoice.total)}</ram:GrandTotalAmount>`,
    `        <ram:DuePayableAmount>${money(invoice.balanceDue ?? invoice.total)}</ram:DuePayableAmount>`,
    "      </ram:SpecifiedTradeSettlementHeaderMonetarySummation>",
    "    </ram:ApplicableHeaderTradeSettlement>",
    "  </rsm:SupplyChainTradeTransaction>",
    '</rsm:CrossIndustryInvoice>'
  ].join("\n");
}

function localized(value: LocalizedValue, locale: "de" | "en") {
  return typeof value === "string" ? value : value[locale] ?? value.de ?? value.en;
}

function escapeXml(value: string) {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function money(value: number) {
  return value.toFixed(2);
}
