import { createAccountingCommand, type AccountingCommand } from "../workflow/commands";

export type PurchaseOrderReceiptCheckInput = {
  companyId: string;
  purchaseOrderId: string;
  receiptId: string;
  orderedQuantity: number;
  receivedQuantity: number;
  invoicedQuantity: number;
  orderedUnitPrice: number;
  invoicedUnitPrice: number;
  taxAmount?: number;
  requestedBy?: string;
};

export type PurchaseOrderMatchResult = {
  quantityVariance: number;
  status: "matched" | "variance";
  totalVariance: number;
  unitPriceVariance: number;
  warnings: string[];
};

export type CheckPurchaseOrderMatchPayload = {
  purchaseOrderId: string;
  quantityVariance: number;
  receiptId: string;
  totalVariance: number;
  unitPriceVariance: number;
};

export type ResolveReceiptVariancePayload = {
  action: "accept_difference" | "request_credit_note" | "request_supplier_clarification";
  purchaseOrderId: string;
  receiptId: string;
  reason: string;
  totalVariance: number;
};

export function checkPurchaseOrderReceiptMatch(input: PurchaseOrderReceiptCheckInput): PurchaseOrderMatchResult {
  const quantityVariance = input.invoicedQuantity - input.receivedQuantity;
  const unitPriceVariance = roundCurrency(input.invoicedUnitPrice - input.orderedUnitPrice);
  const totalVariance = roundCurrency(input.invoicedQuantity * input.invoicedUnitPrice - input.orderedQuantity * input.orderedUnitPrice);
  const warnings = [
    quantityVariance !== 0 ? "purchase_order_quantity_variance" : undefined,
    unitPriceVariance !== 0 ? "purchase_order_unit_price_variance" : undefined,
    totalVariance !== 0 ? "purchase_order_total_variance" : undefined
  ].filter(Boolean) as string[];

  return {
    quantityVariance,
    status: warnings.length ? "variance" : "matched",
    totalVariance,
    unitPriceVariance,
    warnings
  };
}

export function preparePurchaseOrderMatchCommand(input: PurchaseOrderReceiptCheckInput): AccountingCommand<CheckPurchaseOrderMatchPayload> {
  const result = checkPurchaseOrderReceiptMatch(input);
  return createAccountingCommand({
    companyId: input.companyId,
    payload: {
      purchaseOrderId: input.purchaseOrderId,
      quantityVariance: result.quantityVariance,
      receiptId: input.receiptId,
      totalVariance: result.totalVariance,
      unitPriceVariance: result.unitPriceVariance
    },
    refId: input.receiptId,
    refType: "receipt",
    requestedBy: input.requestedBy ?? "business-runtime",
    type: "CheckPurchaseOrderMatch"
  });
}

export function prepareResolveReceiptVarianceCommand(input: {
  action: ResolveReceiptVariancePayload["action"];
  companyId: string;
  purchaseOrderId: string;
  receiptId: string;
  reason: string;
  totalVariance: number;
  requestedBy?: string;
}): AccountingCommand<ResolveReceiptVariancePayload> {
  return createAccountingCommand({
    companyId: input.companyId,
    payload: {
      action: input.action,
      purchaseOrderId: input.purchaseOrderId,
      receiptId: input.receiptId,
      reason: input.reason,
      totalVariance: roundCurrency(input.totalVariance)
    },
    refId: input.receiptId,
    refType: "receipt",
    requestedBy: input.requestedBy ?? "business-runtime",
    type: "ResolveReceiptVariance"
  });
}

function roundCurrency(value: number) {
  return Math.round((value + Number.EPSILON) * 100) / 100;
}
