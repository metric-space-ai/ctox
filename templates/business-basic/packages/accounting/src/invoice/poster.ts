import { moneyFromMajor } from "../money";
import { LedgerPosting, type JournalDraft } from "../posting/service";
import { resolveGermanTaxRate } from "../tax";
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
  const taxByCode = new Map<string, number>();

  for (const line of invoice.lines) {
    const product = context.products.find((item) => item.id === line.productId);
    const lineNet = line.quantity * line.unitPrice;
    const tax = resolveGermanTaxRate({
      kleinunternehmer: context.kleinunternehmer || invoice.kleinunternehmer,
      reverseCharge: invoice.reverseCharge || line.reverseCharge,
      taxRate: line.taxRate
    });
    const taxCode = taxCodeForTaxRate(tax.code);
    posting.credit(revenueAccountId(product?.revenueAccount, context), moneyFromMajor(lineNet, invoice.currency), invoice.customerId, { taxCode });
    if (line.taxRate > 0 && taxCode.endsWith("_OUTPUT")) {
      const accountId = tax.accountId ?? context.defaultTaxAccountId;
      taxByCode.set(`${taxCode}:${accountId}`, round(taxByCode.get(`${taxCode}:${accountId}`) ?? 0) + round(lineNet * (line.taxRate / 100)));
    }
  }

  for (const [key, amount] of taxByCode) {
    if (amount > 0) {
      const [taxCode, accountId] = key.split(":");
      posting.credit(accountId ?? context.defaultTaxAccountId, moneyFromMajor(round(amount), invoice.currency), invoice.customerId, { taxCode });
    }
  }

  return posting.toJournalDraft("invoice", `Posted customer invoice ${invoice.number}.`);
}

function taxCodeForTaxRate(code: ReturnType<typeof resolveGermanTaxRate>["code"]) {
  if (code === "DE_19") return "DE_19_OUTPUT";
  if (code === "DE_7") return "DE_7_OUTPUT";
  return code;
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

function round(value: number) {
  return Math.round((value + Number.EPSILON) * 100) / 100;
}
