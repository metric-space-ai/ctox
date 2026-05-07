export type SupportedLocale = "en" | "de";

export type LocalizedText = string | {
  en: string;
  de: string;
};

export type BusinessCustomer = {
  id: string;
  name: string;
  segment: string;
  owner: string;
  taxId: string;
  billingEmail: string;
  paymentTerms: string;
  status: "Active" | "Onboarding" | "Review";
  country: string;
  mrr: number;
  arBalance: number;
  lastInvoiceId: string;
  notes: LocalizedText;
};

export type BusinessProduct = {
  id: string;
  sku: string;
  name: string;
  type: "Service" | "Subscription" | "Product";
  price: number;
  taxRate: number;
  revenueAccount: string;
  status: "Billable" | "Draft" | "Review";
  margin: number;
  description: LocalizedText;
};

export type BusinessInvoiceLine = {
  productId: string;
  quantity: number;
  unitPrice: number;
  taxRate: number;
};

export type BusinessPaymentEvent = {
  date: string;
  label: LocalizedText;
  amount: number;
};

export type BusinessAccount = {
  id: string;
  code: string;
  name: string;
  rootType: "asset" | "liability" | "equity" | "income" | "expense";
  accountType: "accumulated_depreciation" | "bank" | "depreciation" | "fixed_asset" | "receivable" | "payable" | "tax" | "income" | "expense" | "equity";
  currency: "EUR" | "USD";
  taxCode?: string;
  isPosting: boolean;
};

export type BusinessJournalLine = {
  accountId: string;
  debit: number;
  credit: number;
  partyId?: string;
  taxCode?: string;
  costCenter?: string;
  projectId?: string;
};

export type BusinessJournalEntry = {
  id: string;
  number: string;
  postingDate: string;
  type: "depreciation" | "invoice" | "payment" | "receipt" | "manual" | "fx" | "reverse";
  refType: "asset" | "invoice" | "payment" | "receipt" | "bank_transaction" | "manual";
  refId: string;
  status: "Posted" | "Draft" | "Reversed";
  narration: LocalizedText;
  lines: BusinessJournalLine[];
  postedAt?: string;
  exportId?: string;
};

export type BusinessInvoice = {
  id: string;
  number: string;
  customerId: string;
  customerNumber?: string;
  addressLines?: string[];
  documentTitle?: string;
  introText?: LocalizedText;
  paymentTermsText?: LocalizedText;
  closingText?: LocalizedText;
  issueDate: string;
  dueDate: string;
  serviceDate?: string;
  status: "Draft" | "Sent" | "Paid" | "Overdue" | "Export ready";
  currency: "EUR" | "USD";
  lines: BusinessInvoiceLine[];
  netAmount?: number;
  taxAmount: number;
  total: number;
  balanceDue?: number;
  collectionStatus?: "Clear" | "Due soon" | "Reminder due" | "Reminder sent" | "Final notice";
  reminderLevel?: 0 | 1 | 2 | 3;
  reminderDueDate?: string;
  createdAt?: string;
  printedAt?: string;
  deliveryChannel?: "Email" | "Print" | "Portal" | "Draft";
  paymentMethods?: string[];
  payments?: BusinessPaymentEvent[];
  tags?: string[];
  exportId?: string;
  notes: LocalizedText;
};

export type BusinessBookkeepingExport = {
  id: string;
  period: string;
  system: "DATEV" | "CSV" | "Lexoffice";
  status: "Ready" | "Queued" | "Needs review" | "Exported";
  invoiceIds: string[];
  netAmount: number;
  taxAmount: number;
  generatedAt: string;
  dueDate: string;
  reviewer: string;
  context: LocalizedText;
};

export type BusinessReceipt = {
  id: string;
  number: string;
  vendorName: string;
  receiptDate: string;
  dueDate: string;
  status: "Inbox" | "Needs review" | "Posted" | "Paid" | "Rejected";
  currency: "EUR" | "USD";
  netAmount: number;
  taxAmount: number;
  total: number;
  expenseAccountId: string;
  payableAccountId: string;
  taxCode: "DE_19_INPUT" | "DE_7_INPUT" | "DE_0" | "RC";
  documentType: "Invoice" | "Receipt" | "Credit note";
  source: "Upload" | "Email" | "Bank match" | "Manual";
  bankTransactionId?: string;
  journalEntryId?: string;
  attachmentName: string;
  extractedFields: Array<{ label: string; value: string; confidence: number }>;
  notes: LocalizedText;
};

export type BusinessBankTransaction = {
  id: string;
  bookingDate: string;
  valueDate: string;
  counterparty: string;
  purpose: string;
  amount: number;
  currency: "EUR" | "USD";
  status: "Matched" | "Suggested" | "Unmatched" | "Ignored";
  matchType?: "invoice" | "receipt" | "fee" | "manual";
  matchedRecordId?: string;
  confidence: number;
};

export type BusinessReport = {
  id: string;
  title: string;
  period: string;
  status: "Current" | "Draft" | "Needs data" | "Queued";
  amount: number;
  dueDate: string;
  taxContext: string;
  exportContext: string;
  summary: LocalizedText;
  linkedExportIds: string[];
};

export type BusinessFixedAsset = {
  accumulatedDepreciationAccountId: string;
  acquisitionCost: number;
  acquisitionDate: string;
  acquisitionJournalEntryId?: string;
  assetAccountId: string;
  category: string;
  currency: "EUR" | "USD";
  depreciationExpenseAccountId: string;
  depreciationMethod: "Straight line";
  id: string;
  name: string;
  notes: LocalizedText;
  receiptId?: string;
  salvageValue: number;
  serialNumber?: string;
  status: "Draft" | "Active" | "Fully depreciated" | "Disposed";
  supplier: string;
  usefulLifeMonths: number;
};

export type BusinessFiscalPeriod = {
  closedAt?: string;
  companyId: string;
  endDate: string;
  id: string;
  startDate: string;
  status: "closed" | "open";
};

export type BusinessBundle = {
  accounts: BusinessAccount[];
  bankTransactions: BusinessBankTransaction[];
  customers: BusinessCustomer[];
  fiscalPeriods: BusinessFiscalPeriod[];
  journalEntries: BusinessJournalEntry[];
  products: BusinessProduct[];
  invoices: BusinessInvoice[];
  bookkeeping: BusinessBookkeepingExport[];
  receipts: BusinessReceipt[];
  reports: BusinessReport[];
  fixedAssets: BusinessFixedAsset[];
  warehouse: Array<Record<string, unknown>>;
};

export const businessSeed: BusinessBundle = {
  accounts: [
    { id: "acc-bank", code: "1200", name: "Bank", rootType: "asset", accountType: "bank", currency: "EUR", isPosting: true },
    { id: "acc-ar", code: "1400", name: "Forderungen aus Lieferungen und Leistungen", rootType: "asset", accountType: "receivable", currency: "EUR", isPosting: true },
    { id: "acc-fixed-assets", code: "0480", name: "Betriebs- und Geschaeftsausstattung", rootType: "asset", accountType: "fixed_asset", currency: "EUR", isPosting: true },
    { id: "acc-accumulated-depreciation", code: "0490", name: "Kumulierte Abschreibungen auf Sachanlagen", rootType: "asset", accountType: "accumulated_depreciation", currency: "EUR", isPosting: true },
    { id: "acc-ap", code: "1600", name: "Verbindlichkeiten aus Lieferungen und Leistungen", rootType: "liability", accountType: "payable", currency: "EUR", isPosting: true },
    { id: "acc-vat-output", code: "1776", name: "Umsatzsteuer 19%", rootType: "liability", accountType: "tax", currency: "EUR", taxCode: "DE_19_OUTPUT", isPosting: true },
    { id: "acc-vat-output-7", code: "1771", name: "Umsatzsteuer 7%", rootType: "liability", accountType: "tax", currency: "EUR", taxCode: "DE_7_OUTPUT", isPosting: true },
    { id: "acc-vat-input", code: "1576", name: "Abziehbare Vorsteuer 19%", rootType: "asset", accountType: "tax", currency: "EUR", taxCode: "DE_19_INPUT", isPosting: true },
    { id: "acc-vat-input-7", code: "1571", name: "Abziehbare Vorsteuer 7%", rootType: "asset", accountType: "tax", currency: "EUR", taxCode: "DE_7_INPUT", isPosting: true },
    { id: "acc-revenue-saas", code: "8400", name: "SaaS subscriptions", rootType: "income", accountType: "income", currency: "EUR", isPosting: true },
    { id: "acc-revenue-implementation", code: "8337", name: "Implementation services", rootType: "income", accountType: "income", currency: "EUR", isPosting: true },
    { id: "acc-revenue-research", code: "8338", name: "Research services", rootType: "income", accountType: "income", currency: "EUR", isPosting: true },
    { id: "acc-revenue-support", code: "8401", name: "Support subscriptions", rootType: "income", accountType: "income", currency: "EUR", isPosting: true },
    { id: "acc-software", code: "4920", name: "Software und Cloud", rootType: "expense", accountType: "expense", currency: "EUR", isPosting: true },
    { id: "acc-depreciation", code: "4830", name: "Abschreibungen auf Sachanlagen", rootType: "expense", accountType: "depreciation", currency: "EUR", isPosting: true },
    { id: "acc-contractor", code: "3125", name: "Fremdleistungen", rootType: "expense", accountType: "expense", currency: "EUR", isPosting: true },
    { id: "acc-fees", code: "4970", name: "Nebenkosten des Geldverkehrs", rootType: "expense", accountType: "expense", currency: "EUR", isPosting: true }
  ],
  bankTransactions: [
    {
      id: "bank-2026-05-001",
      bookingDate: "2026-05-01",
      valueDate: "2026-05-01",
      counterparty: "Stripe Payments Europe",
      purpose: "Qualified Piper Inc. US-2026-003",
      amount: 3300,
      currency: "USD",
      status: "Matched",
      matchType: "invoice",
      matchedRecordId: "inv-2026-003",
      confidence: 98
    },
    {
      id: "bank-2026-05-002",
      bookingDate: "2026-05-02",
      valueDate: "2026-05-02",
      counterparty: "Cloud Harbor GmbH",
      purpose: "R-2026-017 Hosting Business OS",
      amount: -221.94,
      currency: "EUR",
      status: "Matched",
      matchType: "receipt",
      matchedRecordId: "rcpt-2026-017",
      confidence: 96
    },
    {
      id: "bank-2026-05-003",
      bookingDate: "2026-05-03",
      valueDate: "2026-05-03",
      counterparty: "Stripe Payments Europe",
      purpose: "Processing fees April",
      amount: -39.22,
      currency: "EUR",
      status: "Suggested",
      matchType: "fee",
      confidence: 81
    },
    {
      id: "bank-2026-05-004",
      bookingDate: "2026-05-04",
      valueDate: "2026-05-04",
      counterparty: "Nova Logistics GmbH",
      purpose: "RE-2026-004 Teilzahlung",
      amount: 1200,
      currency: "EUR",
      status: "Suggested",
      matchType: "invoice",
      matchedRecordId: "inv-2026-004",
      confidence: 73
    },
    {
      id: "bank-2026-05-005",
      bookingDate: "2026-05-04",
      valueDate: "2026-05-04",
      counterparty: "Unknown SEPA",
      purpose: "Invoice May",
      amount: -148.75,
      currency: "EUR",
      status: "Unmatched",
      confidence: 32
    }
  ],
  customers: [
    {
      id: "cust-nova",
      name: "Nova Logistics GmbH",
      segment: "Mid-market operations",
      owner: "Maya Chen",
      taxId: "DE318455210",
      billingEmail: "finance@nova-logistics.example",
      paymentTerms: "14 days",
      status: "Active",
      country: "DE",
      mrr: 6200,
      arBalance: 7378,
      lastInvoiceId: "inv-2026-001",
      notes: {
        en: "Launch customer for the Operations workspace. Invoices need project references for purchasing.",
        de: "Launch-Kunde fuer den Operations Workspace. Rechnungen brauchen Projektreferenzen fuer Einkauf."
      }
    },
    {
      id: "cust-atelier",
      name: "Atelier North Studio",
      segment: "Creative services",
      owner: "Jonas Weber",
      taxId: "FR44920018311",
      billingEmail: "ap@atelier-north.example",
      paymentTerms: "30 days",
      status: "Onboarding",
      country: "FR",
      mrr: 2800,
      arBalance: 3332,
      lastInvoiceId: "inv-2026-002",
      notes: {
        en: "Cross-border VAT review required before recurring billing is activated.",
        de: "Grenzueberschreitende Umsatzsteuer muss vor dem wiederkehrenden Billing geprueft werden."
      }
    },
    {
      id: "cust-piper",
      name: "Qualified Piper Inc.",
      segment: "AI sales tooling",
      owner: "Sara Malik",
      taxId: "US-92-1845509",
      billingEmail: "billing@qualifiedpiper.example",
      paymentTerms: "Due on receipt",
      status: "Review",
      country: "US",
      mrr: 4100,
      arBalance: 0,
      lastInvoiceId: "inv-2026-003",
      notes: {
        en: "USD billing is active; revenue report converts to EUR for management reporting.",
        de: "USD Billing ist aktiv; Revenue Report wird fuer Management Reporting in EUR umgerechnet."
      }
    }
  ],
  products: [
    {
      id: "prod-core",
      sku: "CTOX-CORE-M",
      name: "CTOX Core Managed",
      type: "Subscription",
      price: 2200,
      taxRate: 19,
      revenueAccount: "8400 SaaS subscriptions",
      status: "Billable",
      margin: 82,
      description: {
        en: "Managed CTOX core with queue, knowledge store, prompts, and business stack integration.",
        de: "Managed CTOX Core mit Queue, Knowledge Store, Prompts und Business-Stack-Integration."
      }
    },
    {
      id: "prod-business",
      sku: "CTOX-BOS-SETUP",
      name: "Business OS Setup",
      type: "Service",
      price: 4800,
      taxRate: 19,
      revenueAccount: "8337 Implementation services",
      status: "Billable",
      margin: 64,
      description: {
        en: "Initial configuration of Sales, Marketing, Operations, and Business modules.",
        de: "Initiale Einrichtung von Sales, Marketing, Operations und Business Modulen."
      }
    },
    {
      id: "prod-market",
      sku: "CTOX-MKT-INTEL",
      name: "Market Intelligence Run",
      type: "Service",
      price: 1200,
      taxRate: 19,
      revenueAccount: "8338 Research services",
      status: "Review",
      margin: 58,
      description: {
        en: "Competitive analysis scrape, benchmark model update, and CTOX task queue handoff.",
        de: "Wettbewerbsanalyse-Scrape, Benchmark-Modell-Update und CTOX Task-Queue-Uebergabe."
      }
    },
    {
      id: "prod-support",
      sku: "CTOX-SUPPORT",
      name: "Priority Support",
      type: "Subscription",
      price: 900,
      taxRate: 19,
      revenueAccount: "8401 Support subscriptions",
      status: "Draft",
      margin: 70,
      description: {
        en: "Monthly response window for bug reports, prompts, and business stack adjustments.",
        de: "Monatliches Reaktionsfenster fuer Bug Reports, Prompts und Business-Stack-Anpassungen."
      }
    }
  ],
  invoices: [
    {
      id: "inv-2026-001",
      number: "RE-2026-001",
      customerId: "cust-nova",
      customerNumber: "10001",
      addressLines: ["Nova Logistics GmbH", "Finance Operations", "Hafenstr. 14", "20457 Hamburg", "Deutschland"],
      documentTitle: "Rechnung",
      introText: {
        en: "We invoice the accepted Operations setup milestone as follows.",
        de: "Unsere Leistungen fuer den abgenommenen Operations Setup Meilenstein stellen wir wie folgt in Rechnung."
      },
      issueDate: "2026-04-18",
      dueDate: "2026-05-02",
      serviceDate: "2026-04-18",
      status: "Overdue",
      currency: "EUR",
      lines: [
        { productId: "prod-core", quantity: 1, unitPrice: 2200, taxRate: 19 },
        { productId: "prod-business", quantity: 1, unitPrice: 4000, taxRate: 19 }
      ],
      netAmount: 6200,
      taxAmount: 1178,
      total: 7378,
      balanceDue: 7378,
      collectionStatus: "Reminder due",
      reminderLevel: 1,
      reminderDueDate: "2026-05-04",
      createdAt: "2026-04-18",
      printedAt: "2026-04-18",
      deliveryChannel: "Email",
      paymentTermsText: {
        en: "Payment target 14 days. Please reference RE-2026-001.",
        de: "Zahlungsziel 14 Tage. Bitte RE-2026-001 als Referenz angeben."
      },
      closingText: {
        en: "Thank you for the good collaboration.",
        de: "Vielen Dank fuer die gute Zusammenarbeit."
      },
      paymentMethods: ["Bank transfer", "PayPal.Me"],
      payments: [],
      tags: ["overdue", "operations", "collection"],
      exportId: "exp-2026-04",
      notes: {
        en: "Operations setup milestone accepted. Follow-up collection task should be queued.",
        de: "Operations Setup Meilenstein abgenommen. Collection-Follow-up sollte in die Queue."
      }
    },
    {
      id: "inv-2026-002",
      number: "RE-2026-002",
      customerId: "cust-atelier",
      customerNumber: "10002",
      addressLines: ["Atelier North Studio", "Accounts Payable", "12 Rue du Canal", "75010 Paris", "Frankreich"],
      documentTitle: "Rechnung",
      introText: {
        en: "We invoice the April workspace and market intelligence services as follows.",
        de: "Unsere Leistungen fuer den April Workspace und Market Intelligence stellen wir wie folgt in Rechnung."
      },
      issueDate: "2026-04-26",
      dueDate: "2026-05-26",
      serviceDate: "2026-04-26",
      status: "Sent",
      currency: "EUR",
      lines: [
        { productId: "prod-core", quantity: 1, unitPrice: 2200, taxRate: 19 },
        { productId: "prod-market", quantity: 1, unitPrice: 600, taxRate: 19 }
      ],
      netAmount: 2800,
      taxAmount: 532,
      total: 3332,
      balanceDue: 3332,
      collectionStatus: "Due soon",
      reminderLevel: 0,
      reminderDueDate: "2026-05-27",
      createdAt: "2026-04-26",
      printedAt: "2026-04-26",
      deliveryChannel: "Email",
      paymentTermsText: {
        en: "Payment target 30 days.",
        de: "Zahlungsziel 30 Tage."
      },
      closingText: {
        en: "Please contact us if the VAT treatment needs another document split.",
        de: "Bitte melden Sie sich, wenn die Umsatzsteuerbehandlung eine weitere Belegaufteilung braucht."
      },
      paymentMethods: ["Bank transfer"],
      payments: [],
      tags: ["vat-review", "cross-border"],
      exportId: "exp-2026-04",
      notes: {
        en: "French VAT treatment needs bookkeeping confirmation before recurring run.",
        de: "Franzoesische Umsatzsteuerbehandlung braucht Buchhaltungsfreigabe vor Recurring Run."
      }
    },
    {
      id: "inv-2026-003",
      number: "US-2026-003",
      customerId: "cust-piper",
      customerNumber: "10003",
      addressLines: ["Qualified Piper Inc.", "Billing", "500 Market Street", "San Francisco, CA 94105", "USA"],
      documentTitle: "Invoice",
      introText: {
        en: "We invoice the May CTOX Core and Priority Support subscription as follows.",
        de: "CTOX Core und Priority Support Subscription fuer Mai stellen wir wie folgt in Rechnung."
      },
      issueDate: "2026-05-01",
      dueDate: "2026-05-01",
      serviceDate: "2026-05-01",
      status: "Paid",
      currency: "USD",
      lines: [
        { productId: "prod-core", quantity: 1, unitPrice: 2400, taxRate: 0 },
        { productId: "prod-support", quantity: 1, unitPrice: 900, taxRate: 0 }
      ],
      netAmount: 3300,
      taxAmount: 0,
      total: 3300,
      balanceDue: 0,
      collectionStatus: "Clear",
      reminderLevel: 0,
      createdAt: "2026-05-01",
      printedAt: "2026-05-01",
      deliveryChannel: "Portal",
      paymentTermsText: {
        en: "Due on receipt.",
        de: "Faellig bei Erhalt."
      },
      closingText: {
        en: "Settlement received through Stripe.",
        de: "Settlement ueber Stripe erhalten."
      },
      paymentMethods: ["Stripe"],
      payments: [
        {
          date: "2026-05-01",
          label: { en: "Stripe settlement", de: "Stripe Settlement" },
          amount: 3300
        }
      ],
      tags: ["paid", "usd"],
      notes: {
        en: "Stripe settlement received; management report uses EUR conversion estimate.",
        de: "Stripe Settlement erhalten; Management Report nutzt EUR-Umrechnungsschaetzung."
      }
    },
    {
      id: "inv-2026-004",
      number: "RE-2026-004",
      customerId: "cust-nova",
      customerNumber: "10001",
      addressLines: ["Nova Logistics GmbH", "Finance Operations", "Hafenstr. 14", "20457 Hamburg", "Deutschland"],
      documentTitle: "Rechnung",
      introText: {
        en: "We invoice the April priority support and CTOX Core subscription as follows.",
        de: "Priority Support und CTOX Core Subscription fuer April stellen wir wie folgt in Rechnung."
      },
      issueDate: "2026-04-01",
      dueDate: "2026-04-15",
      serviceDate: "2026-04-01",
      status: "Overdue",
      currency: "EUR",
      lines: [
        { productId: "prod-core", quantity: 1, unitPrice: 2200, taxRate: 19 },
        { productId: "prod-support", quantity: 1, unitPrice: 900, taxRate: 19 }
      ],
      netAmount: 3100,
      taxAmount: 589,
      total: 3689,
      balanceDue: 3689,
      collectionStatus: "Reminder sent",
      reminderLevel: 2,
      reminderDueDate: "2026-05-05",
      createdAt: "2026-04-01",
      printedAt: "2026-04-01",
      deliveryChannel: "Email",
      paymentTermsText: {
        en: "Payment target 14 days.",
        de: "Zahlungsziel 14 Tage."
      },
      closingText: {
        en: "Please settle the open amount before the next subscription cycle.",
        de: "Bitte gleichen Sie den offenen Betrag vor dem naechsten Subscription-Zyklus aus."
      },
      paymentMethods: ["Bank transfer"],
      payments: [],
      tags: ["mahnung-2", "subscription"],
      exportId: "exp-2026-04",
      notes: {
        en: "Second reminder was sent; next action is final notice if no payment is registered.",
        de: "Zweite Mahnung wurde versendet; naechster Schritt ist letzte Mahnung ohne Zahlungseingang."
      }
    },
    {
      id: "inv-2026-005",
      number: "RE-2026-005",
      customerId: "cust-atelier",
      customerNumber: "10002",
      addressLines: ["Atelier North Studio", "Accounts Payable", "12 Rue du Canal", "75010 Paris", "Frankreich"],
      documentTitle: "Rechnung",
      introText: {
        en: "We invoice the accepted market intelligence run as follows.",
        de: "Den abgenommenen Market Intelligence Run stellen wir wie folgt in Rechnung."
      },
      issueDate: "2026-05-02",
      dueDate: "2026-06-01",
      serviceDate: "2026-05-02",
      status: "Draft",
      currency: "EUR",
      lines: [
        { productId: "prod-market", quantity: 2, unitPrice: 1200, taxRate: 19 }
      ],
      netAmount: 2400,
      taxAmount: 456,
      total: 2856,
      balanceDue: 2856,
      collectionStatus: "Clear",
      reminderLevel: 0,
      createdAt: "2026-05-02",
      deliveryChannel: "Draft",
      paymentTermsText: {
        en: "Payment target 30 days after sending.",
        de: "Zahlungsziel 30 Tage nach Versand."
      },
      closingText: {
        en: "Draft waits for tax review before sending.",
        de: "Entwurf wartet vor Versand auf Steuer-Review."
      },
      paymentMethods: ["Bank transfer"],
      payments: [],
      tags: ["draft", "tax-review"],
      notes: {
        en: "Draft invoice should stay blocked until cross-border VAT note is approved.",
        de: "Rechnungsentwurf bleibt blockiert, bis die Cross-Border-USt-Notiz freigegeben ist."
      }
    },
    {
      id: "inv-2026-006",
      number: "RE-2026-006",
      customerId: "cust-piper",
      customerNumber: "10003",
      addressLines: ["Qualified Piper Inc.", "Billing", "500 Market Street", "San Francisco, CA 94105", "USA"],
      documentTitle: "Invoice",
      introText: {
        en: "We invoice the implementation support block as follows.",
        de: "Den Implementation-Support-Block stellen wir wie folgt in Rechnung."
      },
      issueDate: "2026-04-20",
      dueDate: "2026-04-20",
      serviceDate: "2026-04-20",
      status: "Export ready",
      currency: "USD",
      lines: [
        { productId: "prod-support", quantity: 5, unitPrice: 500, taxRate: 0 }
      ],
      netAmount: 2500,
      taxAmount: 0,
      total: 2500,
      balanceDue: 0,
      collectionStatus: "Clear",
      reminderLevel: 0,
      createdAt: "2026-04-20",
      printedAt: "2026-04-20",
      deliveryChannel: "Portal",
      paymentTermsText: {
        en: "Paid by card, export pending.",
        de: "Per Karte bezahlt, Export steht aus."
      },
      closingText: {
        en: "Ready for bookkeeping export.",
        de: "Bereit fuer Buchhaltungsexport."
      },
      paymentMethods: ["Stripe"],
      payments: [
        {
          date: "2026-04-20",
          label: { en: "Card payment", de: "Kartenzahlung" },
          amount: 2500
        }
      ],
      tags: ["export-ready", "usd"],
      exportId: "exp-2026-05-open",
      notes: {
        en: "Paid document is ready for export and exchange-rate review.",
        de: "Bezahlter Beleg ist bereit fuer Export und Wechselkurs-Review."
      }
    }
  ],
  journalEntries: [
    {
      id: "je-inv-2026-001",
      number: "B-2026-0001",
      postingDate: "2026-04-18",
      type: "invoice",
      refType: "invoice",
      refId: "inv-2026-001",
      status: "Posted",
      narration: {
        en: "Posted customer invoice RE-2026-001 for Operations setup and CTOX Core.",
        de: "Gebuchte Ausgangsrechnung RE-2026-001 fuer Operations Setup und CTOX Core."
      },
      lines: [
        { accountId: "acc-ar", debit: 7378, credit: 0, partyId: "cust-nova" },
        { accountId: "acc-revenue-saas", debit: 0, credit: 2200, partyId: "cust-nova" },
        { accountId: "acc-revenue-implementation", debit: 0, credit: 4000, partyId: "cust-nova" },
        { accountId: "acc-vat-output", debit: 0, credit: 1178, partyId: "cust-nova", taxCode: "DE_19_OUTPUT" }
      ],
      postedAt: "2026-04-18T10:20:00.000Z",
      exportId: "exp-2026-04"
    },
    {
      id: "je-inv-2026-002",
      number: "B-2026-0002",
      postingDate: "2026-04-26",
      type: "invoice",
      refType: "invoice",
      refId: "inv-2026-002",
      status: "Posted",
      narration: {
        en: "Posted customer invoice RE-2026-002 with cross-border VAT review flag.",
        de: "Gebuchte Ausgangsrechnung RE-2026-002 mit Cross-Border-USt-Review."
      },
      lines: [
        { accountId: "acc-ar", debit: 3332, credit: 0, partyId: "cust-atelier" },
        { accountId: "acc-revenue-saas", debit: 0, credit: 2200, partyId: "cust-atelier" },
        { accountId: "acc-revenue-research", debit: 0, credit: 600, partyId: "cust-atelier" },
        { accountId: "acc-vat-output", debit: 0, credit: 532, partyId: "cust-atelier", taxCode: "DE_19_OUTPUT" }
      ],
      postedAt: "2026-04-26T12:45:00.000Z",
      exportId: "exp-2026-04"
    },
    {
      id: "je-inv-2026-003",
      number: "B-2026-0003",
      postingDate: "2026-05-01",
      type: "invoice",
      refType: "invoice",
      refId: "inv-2026-003",
      status: "Posted",
      narration: {
        en: "Posted reverse-charge USD subscription invoice US-2026-003.",
        de: "Gebuchte Reverse-Charge USD Subscription Rechnung US-2026-003."
      },
      lines: [
        { accountId: "acc-ar", debit: 3300, credit: 0, partyId: "cust-piper", taxCode: "DE_0" },
        { accountId: "acc-revenue-saas", debit: 0, credit: 2400, partyId: "cust-piper" },
        { accountId: "acc-revenue-support", debit: 0, credit: 900, partyId: "cust-piper" }
      ],
      postedAt: "2026-05-01T08:15:00.000Z",
      exportId: "exp-2026-05-open"
    },
    {
      id: "je-pay-2026-003",
      number: "B-2026-0004",
      postingDate: "2026-05-01",
      type: "payment",
      refType: "bank_transaction",
      refId: "bank-2026-05-001",
      status: "Posted",
      narration: {
        en: "Matched Stripe settlement against US-2026-003.",
        de: "Stripe Settlement gegen US-2026-003 ausgeglichen."
      },
      lines: [
        { accountId: "acc-bank", debit: 3300, credit: 0, partyId: "cust-piper" },
        { accountId: "acc-ar", debit: 0, credit: 3300, partyId: "cust-piper" }
      ],
      postedAt: "2026-05-01T09:10:00.000Z",
      exportId: "exp-2026-05-open"
    },
    {
      id: "je-inv-2026-004",
      number: "B-2026-0005",
      postingDate: "2026-04-01",
      type: "invoice",
      refType: "invoice",
      refId: "inv-2026-004",
      status: "Posted",
      narration: {
        en: "Posted April subscription invoice RE-2026-004.",
        de: "Gebuchte April Subscription Rechnung RE-2026-004."
      },
      lines: [
        { accountId: "acc-ar", debit: 3689, credit: 0, partyId: "cust-nova" },
        { accountId: "acc-revenue-saas", debit: 0, credit: 2200, partyId: "cust-nova" },
        { accountId: "acc-revenue-support", debit: 0, credit: 900, partyId: "cust-nova" },
        { accountId: "acc-vat-output", debit: 0, credit: 589, partyId: "cust-nova", taxCode: "DE_19_OUTPUT" }
      ],
      postedAt: "2026-04-01T07:45:00.000Z",
      exportId: "exp-2026-04"
    },
    {
      id: "je-rcpt-2026-017",
      number: "B-2026-0006",
      postingDate: "2026-05-02",
      type: "receipt",
      refType: "receipt",
      refId: "rcpt-2026-017",
      status: "Posted",
      narration: {
        en: "Posted Cloud Harbor hosting receipt with input VAT.",
        de: "Cloud Harbor Hosting-Eingangsbeleg mit Vorsteuer gebucht."
      },
      lines: [
        { accountId: "acc-software", debit: 186.5, credit: 0, taxCode: "DE_19_INPUT" },
        { accountId: "acc-vat-input", debit: 35.44, credit: 0, taxCode: "DE_19_INPUT" },
        { accountId: "acc-ap", debit: 0, credit: 221.94 }
      ],
      postedAt: "2026-05-02T11:15:00.000Z",
      exportId: "exp-2026-05-open"
    },
    {
      id: "je-pay-rcpt-2026-017",
      number: "B-2026-0007",
      postingDate: "2026-05-02",
      type: "payment",
      refType: "bank_transaction",
      refId: "bank-2026-05-002",
      status: "Posted",
      narration: {
        en: "Matched bank payment for Cloud Harbor hosting receipt.",
        de: "Bankzahlung fuer Cloud Harbor Hosting-Eingangsbeleg ausgeglichen."
      },
      lines: [
        { accountId: "acc-ap", debit: 221.94, credit: 0 },
        { accountId: "acc-bank", debit: 0, credit: 221.94 }
      ],
      postedAt: "2026-05-02T11:20:00.000Z",
      exportId: "exp-2026-05-open"
    },
    {
      id: "je-bank-fee-2026-05",
      number: "B-2026-0008",
      postingDate: "2026-05-03",
      type: "manual",
      refType: "bank_transaction",
      refId: "bank-2026-05-003",
      status: "Draft",
      narration: {
        en: "Suggested posting for Stripe processing fees, waiting for review.",
        de: "Buchungsvorschlag fuer Stripe-Gebuehren, wartet auf Review."
      },
      lines: [
        { accountId: "acc-fees", debit: 39.22, credit: 0 },
        { accountId: "acc-bank", debit: 0, credit: 39.22 }
      ]
    },
    {
      id: "je-asset-acq-2024-001",
      number: "B-2024-0001",
      postingDate: "2024-01-23",
      type: "manual",
      refType: "asset",
      refId: "asset-macbook-2024",
      status: "Posted",
      narration: {
        en: "Capitalized fixed asset from supplier invoice RE2401186.",
        de: "Anlage aus Lieferantenrechnung RE2401186 aktiviert."
      },
      lines: [
        { accountId: "acc-fixed-assets", debit: 1236.97, credit: 0 },
        { accountId: "acc-vat-input", debit: 235.03, credit: 0, taxCode: "DE_19_INPUT" },
        { accountId: "acc-ap", debit: 0, credit: 1472 }
      ],
      postedAt: "2024-01-23T14:11:00.000Z",
      exportId: "exp-2024-01"
    },
    {
      id: "je-asset-pay-2024-001",
      number: "B-2024-0002",
      postingDate: "2024-01-23",
      type: "payment",
      refType: "asset",
      refId: "asset-macbook-2024",
      status: "Posted",
      narration: {
        en: "Paid supplier invoice for fixed asset RE2401186.",
        de: "Lieferantenrechnung fuer Anlage RE2401186 bezahlt."
      },
      lines: [
        { accountId: "acc-ap", debit: 1472, credit: 0 },
        { accountId: "acc-bank", debit: 0, credit: 1472 }
      ],
      postedAt: "2024-01-23T14:20:00.000Z",
      exportId: "exp-2024-01"
    },
    {
      id: "je-asset-depr-2024-001",
      number: "B-2024-0012",
      postingDate: "2024-12-31",
      type: "depreciation",
      refType: "asset",
      refId: "asset-macbook-2024",
      status: "Posted",
      narration: {
        en: "Annual depreciation 2024 for MacBook Pro asset.",
        de: "Jahresabschreibung 2024 fuer MacBook Pro Anlage."
      },
      lines: [
        { accountId: "acc-depreciation", debit: 247.97, credit: 0 },
        { accountId: "acc-accumulated-depreciation", debit: 0, credit: 247.97 }
      ],
      postedAt: "2024-12-31T18:00:00.000Z",
      exportId: "exp-2024-12"
    },
    {
      id: "je-asset-depr-2025-001",
      number: "B-2025-0012",
      postingDate: "2025-12-31",
      type: "depreciation",
      refType: "asset",
      refId: "asset-macbook-2024",
      status: "Posted",
      narration: {
        en: "Annual depreciation 2025 for MacBook Pro asset.",
        de: "Jahresabschreibung 2025 fuer MacBook Pro Anlage."
      },
      lines: [
        { accountId: "acc-depreciation", debit: 248, credit: 0 },
        { accountId: "acc-accumulated-depreciation", debit: 0, credit: 248 }
      ],
      postedAt: "2025-12-31T18:00:00.000Z",
      exportId: "exp-2025-12"
    },
    {
      id: "je-asset-depr-2026-001",
      number: "B-2026-0009",
      postingDate: "2026-12-31",
      type: "depreciation",
      refType: "asset",
      refId: "asset-macbook-2024",
      status: "Draft",
      narration: {
        en: "Prepared annual depreciation 2026 for MacBook Pro asset.",
        de: "Vorbereitete Jahresabschreibung 2026 fuer MacBook Pro Anlage."
      },
      lines: [
        { accountId: "acc-depreciation", debit: 248, credit: 0 },
        { accountId: "acc-accumulated-depreciation", debit: 0, credit: 248 }
      ]
    }
  ],
  receipts: [
    {
      id: "rcpt-2026-017",
      number: "R-2026-017",
      vendorName: "Cloud Harbor GmbH",
      receiptDate: "2026-05-02",
      dueDate: "2026-05-02",
      status: "Paid",
      currency: "EUR",
      netAmount: 186.5,
      taxAmount: 35.44,
      total: 221.94,
      expenseAccountId: "acc-software",
      payableAccountId: "acc-ap",
      taxCode: "DE_19_INPUT",
      documentType: "Invoice",
      source: "Email",
      bankTransactionId: "bank-2026-05-002",
      journalEntryId: "je-rcpt-2026-017",
      attachmentName: "cloud-harbor-r-2026-017.pdf",
      extractedFields: [
        { label: "Vendor", value: "Cloud Harbor GmbH", confidence: 99 },
        { label: "Gross", value: "221.94 EUR", confidence: 98 },
        { label: "VAT", value: "35.44 EUR", confidence: 96 }
      ],
      notes: {
        en: "Hosting receipt is posted, paid, and matched against the bank feed.",
        de: "Hosting-Eingangsbeleg ist gebucht, bezahlt und mit dem Bankfeed abgeglichen."
      }
    },
    {
      id: "rcpt-2026-018",
      number: "R-2026-018",
      vendorName: "Independent Design Partner",
      receiptDate: "2026-05-03",
      dueDate: "2026-05-17",
      status: "Needs review",
      currency: "EUR",
      netAmount: 780,
      taxAmount: 148.2,
      total: 928.2,
      expenseAccountId: "acc-contractor",
      payableAccountId: "acc-ap",
      taxCode: "DE_19_INPUT",
      documentType: "Invoice",
      source: "Upload",
      attachmentName: "design-partner-r-2026-018.pdf",
      extractedFields: [
        { label: "Vendor", value: "Independent Design Partner", confidence: 91 },
        { label: "Service date", value: "2026-04-30", confidence: 76 },
        { label: "VAT ID", value: "missing", confidence: 52 }
      ],
      notes: {
        en: "OCR found the amount, but VAT ID and service period need human review before posting.",
        de: "OCR hat den Betrag erkannt, aber USt-ID und Leistungszeitraum brauchen Review vor Buchung."
      }
    },
    {
      id: "rcpt-2026-019",
      number: "R-2026-019",
      vendorName: "OpenAI API Platform",
      receiptDate: "2026-05-04",
      dueDate: "2026-05-04",
      status: "Inbox",
      currency: "USD",
      netAmount: 148.75,
      taxAmount: 0,
      total: 148.75,
      expenseAccountId: "acc-software",
      payableAccountId: "acc-ap",
      taxCode: "RC",
      documentType: "Receipt",
      source: "Bank match",
      bankTransactionId: "bank-2026-05-005",
      attachmentName: "openai-usage-2026-05.html",
      extractedFields: [
        { label: "Vendor", value: "OpenAI API Platform", confidence: 88 },
        { label: "Gross", value: "148.75 USD", confidence: 84 },
        { label: "Tax treatment", value: "reverse charge", confidence: 61 }
      ],
      notes: {
        en: "Bank line is unmatched until the imported receipt is reviewed for reverse-charge treatment.",
        de: "Bankzeile bleibt ungeklärt, bis der importierte Beleg fuer Reverse Charge geprueft ist."
      }
    }
  ],
  bookkeeping: [
    {
      id: "exp-2026-04",
      period: "2026-04",
      system: "DATEV",
      status: "Needs review",
      invoiceIds: ["inv-2026-001", "inv-2026-002", "inv-2026-004"],
      netAmount: 12100,
      taxAmount: 2299,
      generatedAt: "2026-05-01T08:40:00.000Z",
      dueDate: "2026-05-05",
      reviewer: "Lea Hoffmann",
      context: {
        en: "Cross-border invoice needs tax code confirmation before DATEV export.",
        de: "Grenzueberschreitende Rechnung braucht Steuerkennzeichen-Freigabe vor DATEV Export."
      }
    },
    {
      id: "exp-2026-05-open",
      period: "2026-05",
      system: "CSV",
      status: "Queued",
      invoiceIds: ["inv-2026-003", "inv-2026-006"],
      netAmount: 5800,
      taxAmount: 0,
      generatedAt: "2026-05-02T06:15:00.000Z",
      dueDate: "2026-05-31",
      reviewer: "CTOX Agent",
      context: {
        en: "Open May export collecting paid USD invoices and draft recurring invoices.",
        de: "Offener Mai-Export sammelt bezahlte USD-Rechnungen und Recurring-Drafts."
      }
    }
  ],
  fiscalPeriods: fiscalPeriodsForYear(2026),
  fixedAssets: [
    {
      id: "asset-macbook-2024",
      name: "MacBook Pro 14 M3",
      category: "Computer und Zubehoer",
      supplier: "Schneiderladen GmbH",
      receiptId: "RE2401186",
      acquisitionDate: "2024-01-23",
      acquisitionCost: 1236.97,
      currency: "EUR",
      usefulLifeMonths: 60,
      salvageValue: 1,
      status: "Active",
      assetAccountId: "acc-fixed-assets",
      accumulatedDepreciationAccountId: "acc-accumulated-depreciation",
      depreciationExpenseAccountId: "acc-depreciation",
      acquisitionJournalEntryId: "je-asset-acq-2024-001",
      depreciationMethod: "Straight line",
      serialNumber: "WS101009",
      notes: {
        en: "Capitalized office hardware. Depreciation schedule posts to accumulated depreciation and flows into the balance sheet.",
        de: "Aktivierte Buero-Hardware. Abschreibungsplan bucht gegen kumulierte Abschreibung und fliesst in die Bilanz."
      }
    }
  ],
  warehouse: [],
  reports: [
    {
      id: "rep-runway",
      title: "Runway and receivables",
      period: "May 2026",
      status: "Current",
      amount: 10710,
      dueDate: "2026-05-03",
      taxContext: "VAT split: DE 1,178 EUR / FR review 532 EUR / US reverse charge",
      exportContext: "DATEV April export pending review",
      summary: {
        en: "Receivables are concentrated in Nova; one overdue invoice should trigger a CTOX collection task.",
        de: "Forderungen konzentrieren sich auf Nova; eine ueberfaellige Rechnung sollte einen CTOX Collection Task ausloesen."
      },
      linkedExportIds: ["exp-2026-04"]
    },
    {
      id: "rep-tax",
      title: "VAT and export readiness",
      period: "Q2 2026",
      status: "Needs data",
      amount: 1710,
      dueDate: "2026-05-05",
      taxContext: "Cross-border customer tax validation missing",
      exportContext: "Waiting for DATEV mapping approval",
      summary: {
        en: "The report is blocked by one French customer tax decision and account mapping confirmation.",
        de: "Der Report ist durch eine franzoesische Steuerentscheidung und Kontenmapping-Freigabe blockiert."
      },
      linkedExportIds: ["exp-2026-04", "exp-2026-05-open"]
    },
    {
      id: "rep-product-mix",
      title: "Product revenue mix",
      period: "April 2026",
      status: "Draft",
      amount: 12300,
      dueDate: "2026-05-04",
      taxContext: "Uses invoice line tax rates",
      exportContext: "Can be exported as management CSV",
      summary: {
        en: "Core subscription is the stable base; implementation services still carry the largest cash movement.",
        de: "Core Subscription ist die stabile Basis; Implementation Services tragen weiter die groesste Cash-Bewegung."
      },
      linkedExportIds: ["exp-2026-04"]
    }
  ]
};

export async function getBusinessBundle() {
  if (!shouldUsePostgres()) return businessSeed;

  try {
    const db = await import("@ctox-business/db/modules");
    const [
      accountRows,
      bankTransactionRows,
      customerRows,
      journalEntryRows,
      productRows,
      invoiceRows,
      bookkeepingRows,
      receiptRows,
      fixedAssetRows,
      reportRows
    ] = await Promise.all([
      db.listModuleRecords("business", "accounts"),
      db.listModuleRecords("business", "bank-transactions"),
      db.listModuleRecords("business", "customers"),
      db.listModuleRecords("business", "ledger"),
      db.listModuleRecords("business", "products"),
      db.listModuleRecords("business", "invoices"),
      db.listModuleRecords("business", "bookkeeping"),
      db.listModuleRecords("business", "receipts"),
      db.listModuleRecords("business", "fixed-assets"),
      db.listModuleRecords("business", "reports")
    ]);

    const shouldSeed = (customerRows?.length ?? 0) === 0 && (invoiceRows?.length ?? 0) === 0 && shouldAutoSeedPostgres();
    if (shouldSeed) {
      await db.seedModuleRecords("business", businessSeedRecords());
      return getBusinessBundle();
    }

    return {
      accounts: rowsToPayload(accountRows, businessSeed.accounts),
      bankTransactions: rowsToPayload(bankTransactionRows, businessSeed.bankTransactions),
      customers: rowsToPayload(customerRows, businessSeed.customers),
      journalEntries: rowsToPayload(journalEntryRows, businessSeed.journalEntries),
      fiscalPeriods: businessSeed.fiscalPeriods,
      products: rowsToPayload(productRows, businessSeed.products),
      invoices: rowsToPayload(invoiceRows, businessSeed.invoices),
      bookkeeping: rowsToPayload(bookkeepingRows, businessSeed.bookkeeping),
      receipts: rowsToPayload(receiptRows, businessSeed.receipts),
      fixedAssets: rowsToPayload(fixedAssetRows, businessSeed.fixedAssets),
      reports: rowsToPayload(reportRows, businessSeed.reports),
      warehouse: []
    };
  } catch (error) {
    console.warn("Falling back to Business seed data.", error);
    return businessSeed;
  }
}

export function getBusinessSeedBundle() {
  return businessSeed;
}

export async function getBusinessResource(resource: string) {
  const normalized = normalizeBusinessResource(resource);
  if (!normalized) return null;
  const bundle = await getBusinessBundle();
  return bundle[normalized];
}

export function normalizeBusinessResource(resource: string): keyof BusinessBundle | null {
  if (resource === "accounts") return "accounts";
  if (resource === "bank-transactions" || resource === "banking" || resource === "payments") return "bankTransactions";
  if (resource === "customers") return "customers";
  if (resource === "journal" || resource === "ledger" || resource === "journal-entries") return "journalEntries";
  if (resource === "products" || resource === "services") return "products";
  if (resource === "warehouse" || resource === "inventory" || resource === "stock") return "warehouse";
  if (resource === "invoices") return "invoices";
  if (resource === "bookkeeping" || resource === "exports") return "bookkeeping";
  if (resource === "receipts" || resource === "inbound-receipts") return "receipts";
  if (resource === "fixed-assets" || resource === "assets" || resource === "anlagen") return "fixedAssets";
  if (resource === "fiscal-periods" || resource === "periods" || resource === "perioden") return "fiscalPeriods";
  if (resource === "reports") return "reports";
  return null;
}

export function text(value: LocalizedText, locale: SupportedLocale) {
  return typeof value === "string" ? value : value[locale] ?? value.en;
}

export function businessCurrency(amount: number, currency = "EUR", locale: SupportedLocale = "en") {
  const hasCents = Math.abs(amount % 1) > 0.001;
  return new Intl.NumberFormat(locale === "de" ? "de-DE" : "en-US", {
    currency,
    maximumFractionDigits: hasCents ? 2 : 0,
    minimumFractionDigits: hasCents ? 2 : 0,
    style: "currency"
  }).format(amount);
}

function businessSeedRecords() {
  return {
    accounts: businessSeed.accounts.map((account) => ({
      id: account.id,
      label: `${account.code} ${account.name}`,
      status: account.isPosting ? "Posting" : "Group",
      ownerId: account.rootType,
      payload: account
    })),
    "bank-transactions": businessSeed.bankTransactions.map((transaction) => ({
      id: transaction.id,
      label: `${transaction.counterparty} ${transaction.amount}`,
      status: transaction.status,
      ownerId: transaction.matchedRecordId ?? transaction.matchType,
      payload: transaction
    })),
    bookkeeping: businessSeed.bookkeeping.map((item) => ({
      id: item.id,
      label: item.period,
      status: item.status,
      ownerId: item.reviewer,
      payload: item
    })),
    customers: businessSeed.customers.map((customer) => ({
      id: customer.id,
      label: customer.name,
      status: customer.status,
      ownerId: customer.owner,
      payload: customer
    })),
    invoices: businessSeed.invoices.map((invoice) => ({
      id: invoice.id,
      label: invoice.number,
      status: invoice.status,
      ownerId: invoice.customerId,
      payload: invoice
    })),
    "fixed-assets": businessSeed.fixedAssets.map((asset) => ({
      id: asset.id,
      label: asset.name,
      status: asset.status,
      ownerId: asset.category,
      payload: asset
    })),
    "fiscal-periods": businessSeed.fiscalPeriods.map((period) => ({
      id: period.id,
      label: `${period.startDate} - ${period.endDate}`,
      status: period.status,
      ownerId: period.companyId,
      payload: period
    })),
    ledger: businessSeed.journalEntries.map((entry) => ({
      id: entry.id,
      label: entry.number,
      status: entry.status,
      ownerId: entry.refId,
      payload: entry
    })),
    products: businessSeed.products.map((product) => ({
      id: product.id,
      label: product.name,
      status: product.status,
      ownerId: product.revenueAccount,
      payload: product
    })),
    receipts: businessSeed.receipts.map((receipt) => ({
      id: receipt.id,
      label: receipt.number,
      status: receipt.status,
      ownerId: receipt.vendorName,
      payload: receipt
    })),
    reports: businessSeed.reports.map((report) => ({
      id: report.id,
      label: report.title,
      status: report.status,
      ownerId: report.period,
      payload: report
    }))
  };
}

function fiscalPeriodsForYear(year: number): BusinessFiscalPeriod[] {
  const companyId = "business-basic-company";
  const months = Array.from({ length: 12 }, (_, index) => {
    const month = index + 1;
    const start = new Date(Date.UTC(year, index, 1));
    const end = new Date(Date.UTC(year, month, 0));
    return {
      companyId,
      endDate: isoDate(end),
      id: `fy-${year}-${String(month).padStart(2, "0")}`,
      startDate: isoDate(start),
      status: "open" as const
    };
  });

  return [
    {
      companyId,
      endDate: `${year}-12-31`,
      id: `fy-${year}`,
      startDate: `${year}-01-01`,
      status: "open"
    },
    ...months
  ];
}

function isoDate(date: Date) {
  return date.toISOString().slice(0, 10);
}

function rowsToPayload<T>(rows: Array<{ payloadJson: string }> | null | undefined, fallback: T[]): T[] {
  if (!rows || rows.length === 0) return fallback;
  return rows.map((row) => parseJson(row.payloadJson)).filter(Boolean) as T[];
}

function parseJson(value: string) {
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return null;
  }
}

function shouldUsePostgres() {
  const value = process.env.DATABASE_URL;
  return Boolean(value && !value.includes("user:password@localhost"));
}

function shouldAutoSeedPostgres() {
  return process.env.CTOX_BUSINESS_AUTO_SEED !== "false";
}
