import type { CurrencyCode } from "../money";

export type LocalizedValue = string | { de: string; en: string };

export type BusinessInvoiceLineLike = {
  productId: string;
  quantity: number;
  reverseCharge?: boolean;
  taxRate: number;
  unitPrice: number;
};

export type BusinessInvoiceLike = {
  id: string;
  addressLines?: string[];
  balanceDue?: number;
  closingText?: LocalizedValue;
  currency: CurrencyCode;
  customerId: string;
  customerNumber?: string;
  documentTitle?: string;
  dueDate: string;
  introText?: LocalizedValue;
  issueDate: string;
  lines: BusinessInvoiceLineLike[];
  netAmount?: number;
  notes?: LocalizedValue;
  number: string;
  paymentTermsText?: LocalizedValue;
  serviceDate?: string;
  status: string;
  taxAmount: number;
  total: number;
  kleinunternehmer?: boolean;
  reverseCharge?: boolean;
};

export type BusinessCustomerLike = {
  billingEmail?: string;
  country?: string;
  id: string;
  name: string;
  paymentTerms?: string;
  taxId?: string;
};

export type BusinessProductLike = {
  id: string;
  description?: LocalizedValue;
  name: string;
  revenueAccount?: string;
  taxRate?: number;
  type?: "Product" | "Service" | "Subscription" | string;
};

export type InvoiceContext = {
  companyId: string;
  companyName: string;
  customer?: BusinessCustomerLike;
  defaultReceivableAccountId: string;
  defaultRevenueAccountId: string;
  defaultTaxAccountId: string;
  issuerAddressLines?: string[];
  issuerTaxId?: string;
  issuerVatId?: string;
  kleinunternehmer?: boolean;
  locale?: "de" | "en";
  products: BusinessProductLike[];
  requestedBy?: string;
};

export type InvoiceValidationResult = {
  errors: string[];
  warnings: string[];
};

export type InvoiceDocumentLine = {
  description: string;
  quantity: string;
  title: string;
  total: string;
  unit: string;
  unitPrice: string;
};

export type InvoiceDocument = {
  amountLabel: string;
  body: string;
  closingText: string;
  customerNumber?: string;
  dueDate?: string;
  issueDate: string;
  lines: InvoiceDocumentLine[];
  number: string;
  paymentTerms: string;
  recipientLines: string[];
  serviceDate?: string;
  subtotalAmount: string;
  subtotalLabel: string;
  taxAmount: string;
  taxLabel: string;
  title: string;
  totalLabel: string;
  typeLabel: string;
};
