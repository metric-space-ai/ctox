import type { BusinessInvoiceLike, InvoiceContext, InvoiceValidationResult } from "./types";

export function validateInvoiceForSend(invoice: BusinessInvoiceLike, context: InvoiceContext): InvoiceValidationResult {
  const errors: string[] = [];
  const warnings: string[] = [];

  if (!invoice.customerId) errors.push("customer_required");
  if (!context.customer?.name) errors.push("customer_name_required");
  if (!context.companyName) errors.push("issuer_name_required");
  if (!context.issuerAddressLines?.length) warnings.push("issuer_address_missing");
  if (!context.issuerTaxId && !context.issuerVatId) warnings.push("issuer_tax_or_vat_id_missing");
  if (!invoice.addressLines?.length && !context.customer?.country) warnings.push("customer_address_missing");
  if (!invoice.issueDate) errors.push("issue_date_required");
  if (!invoice.dueDate) errors.push("due_date_required");
  if (!invoice.serviceDate) warnings.push("service_date_missing");
  if (invoice.issueDate && !isIsoDate(invoice.issueDate)) errors.push("issue_date_invalid");
  if (invoice.dueDate && !isIsoDate(invoice.dueDate)) errors.push("due_date_invalid");
  if (invoice.serviceDate && !isIsoDate(invoice.serviceDate)) errors.push("service_date_invalid");
  if (invoice.issueDate && invoice.dueDate && isIsoDate(invoice.issueDate) && isIsoDate(invoice.dueDate) && invoice.dueDate < invoice.issueDate) {
    errors.push("due_date_before_issue_date");
  }
  if (!invoice.lines.length) errors.push("invoice_lines_required");
  if (invoice.total <= 0) errors.push("invoice_total_must_be_positive");
  if (!invoice.number) errors.push("invoice_number_required");

  for (const [index, line] of invoice.lines.entries()) {
    if (!line.productId) errors.push(`line_${index + 1}_product_required`);
    if (line.quantity <= 0) errors.push(`line_${index + 1}_quantity_must_be_positive`);
    if (line.unitPrice < 0) errors.push(`line_${index + 1}_unit_price_must_not_be_negative`);
    if (!supportedTaxRates.has(line.taxRate)) errors.push(`line_${index + 1}_unsupported_tax_rate`);
    if ((context.kleinunternehmer || invoice.kleinunternehmer) && line.taxRate !== 0) errors.push(`line_${index + 1}_kleinunternehmer_tax_rate_must_be_zero`);
    if ((invoice.reverseCharge || line.reverseCharge) && line.taxRate !== 0) errors.push(`line_${index + 1}_reverse_charge_tax_rate_must_be_zero`);
  }

  if (invoice.taxAmount < 0) errors.push("tax_amount_must_not_be_negative");
  if ((context.kleinunternehmer || invoice.kleinunternehmer) && invoice.taxAmount > 0) {
    errors.push("kleinunternehmer_invoice_must_not_have_tax");
  }
  if ((context.kleinunternehmer || invoice.kleinunternehmer) && !hasKleinunternehmerNote(invoice)) {
    warnings.push("kleinunternehmer_note_missing");
  }
  if (invoice.reverseCharge && invoice.taxAmount > 0) {
    errors.push("reverse_charge_invoice_must_not_have_tax");
  }
  if (context.customer?.country && context.customer.country !== "Germany" && invoice.taxAmount > 0) {
    warnings.push("cross_border_tax_review_recommended");
  }
  if (invoice.reverseCharge && !context.customer?.taxId) {
    warnings.push("reverse_charge_customer_vat_id_missing");
  }
  if (invoice.lines.some((line) => line.reverseCharge) && !invoice.reverseCharge) {
    warnings.push("line_level_reverse_charge_requires_invoice_review");
  }

  const expectedNet = round(invoice.lines.reduce((sum, line) => sum + line.quantity * line.unitPrice, 0));
  const declaredNet = round(invoice.netAmount ?? expectedNet);
  if (Math.abs(declaredNet - expectedNet) > 0.01) errors.push("invoice_net_amount_mismatch");

  const expectedTax = round(invoice.lines.reduce((sum, line) => {
    if (context.kleinunternehmer || invoice.kleinunternehmer || invoice.reverseCharge || line.reverseCharge) return sum;
    return sum + line.quantity * line.unitPrice * (line.taxRate / 100);
  }, 0));
  if (Math.abs(round(invoice.taxAmount) - expectedTax) > 0.01) errors.push("invoice_tax_amount_mismatch");

  const expectedTotal = round(expectedNet + expectedTax);
  if (Math.abs(round(invoice.total) - expectedTotal) > 0.01) errors.push("invoice_total_amount_mismatch");

  return { errors, warnings };
}

const supportedTaxRates = new Set([0, 7, 19]);

function isIsoDate(value: string) {
  return /^\d{4}-\d{2}-\d{2}$/.test(value) && !Number.isNaN(new Date(`${value}T00:00:00.000Z`).getTime());
}

function hasKleinunternehmerNote(invoice: BusinessInvoiceLike) {
  const joined = [invoice.notes, invoice.closingText, invoice.paymentTermsText].map((value) => {
    if (!value) return "";
    return typeof value === "string" ? value : `${value.de} ${value.en}`;
  }).join(" ").toLowerCase();
  return joined.includes("19 ustg") || joined.includes("kleinunternehmer");
}

function round(value: number) {
  return Math.round((value + Number.EPSILON) * 100) / 100;
}

export function assertInvoiceCanSend(invoice: BusinessInvoiceLike, context: InvoiceContext) {
  const result = validateInvoiceForSend(invoice, context);
  if (result.errors.length) {
    throw new Error(`invoice_validation_failed:${result.errors.join(",")}`);
  }
  return result;
}
