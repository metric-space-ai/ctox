import { moneyFromMajor } from "../money";
import { LedgerPosting, type JournalDraft } from "../posting/service";
import { createAccountingCommand, type AccountingCommand } from "../workflow/commands";
import { assertInvoiceCanSend, validateInvoiceForSend } from "./validate";
import type { BusinessInvoiceLike, InvoiceContext } from "./types";

export type SendInvoiceCommandPayload = {
  invoiceId: string;
  invoiceNumber: string;
  validationWarnings: string[];
};

export function prepareSendInvoiceCommand(
  invoice: BusinessInvoiceLike,
  context: InvoiceContext
): AccountingCommand<SendInvoiceCommandPayload> {
  const validation = validateInvoiceForSend(invoice, context);
  return createAccountingCommand({
    companyId: context.companyId,
    payload: {
      invoiceId: invoice.id,
      invoiceNumber: invoice.number,
      validationWarnings: validation.warnings
    },
    refId: invoice.id,
    refType: "invoice",
    requestedBy: context.requestedBy ?? "business-runtime",
    type: "SendInvoice"
  });
}

export function buildInvoiceJournalDraft(invoice: BusinessInvoiceLike, context: InvoiceContext): JournalDraft {
  assertInvoiceCanSend(invoice, context);

  const posting = new LedgerPosting(context.companyId, "invoice", invoice.id, invoice.issueDate, invoice.currency);
  posting.debit(context.defaultReceivableAccountId, moneyFromMajor(invoice.total, invoice.currency), invoice.customerId);

  for (const line of invoice.lines) {
    const product = context.products.find((item) => item.id === line.productId);
    posting.credit(revenueAccountId(product?.revenueAccount, context), moneyFromMajor(line.quantity * line.unitPrice, invoice.currency), invoice.customerId);
  }

  if (invoice.taxAmount > 0) {
    posting.credit(context.defaultTaxAccountId, moneyFromMajor(invoice.taxAmount, invoice.currency), invoice.customerId, { taxCode: "DE_19_OUTPUT" });
  }

  return posting.toJournalDraft("invoice", `Posted customer invoice ${invoice.number}.`);
}

function revenueAccountId(revenueAccount: string | undefined, context: InvoiceContext) {
  if (!revenueAccount) return context.defaultRevenueAccountId;
  const code = revenueAccount.trim().split(/\s+/)[0];
  const knownAccountId = knownRevenueAccountIds[code];
  if (knownAccountId) return knownAccountId;
  if (!code) return context.defaultRevenueAccountId;
  return `acc-${code}`;
}

const knownRevenueAccountIds: Record<string, string> = {
  "8337": "acc-revenue-implementation",
  "8338": "acc-revenue-research",
  "8400": "acc-revenue-saas",
  "8401": "acc-revenue-support"
};
