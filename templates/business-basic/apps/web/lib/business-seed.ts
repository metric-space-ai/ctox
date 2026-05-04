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

export type BusinessBundle = {
  customers: BusinessCustomer[];
  products: BusinessProduct[];
  invoices: BusinessInvoice[];
  bookkeeping: BusinessBookkeepingExport[];
  reports: BusinessReport[];
};

export const businessSeed: BusinessBundle = {
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
    const [customerRows, productRows, invoiceRows, bookkeepingRows, reportRows] = await Promise.all([
      db.listModuleRecords("business", "customers"),
      db.listModuleRecords("business", "products"),
      db.listModuleRecords("business", "invoices"),
      db.listModuleRecords("business", "bookkeeping"),
      db.listModuleRecords("business", "reports")
    ]);

    const shouldSeed = (customerRows?.length ?? 0) === 0 && (invoiceRows?.length ?? 0) === 0 && shouldAutoSeedPostgres();
    if (shouldSeed) {
      await db.seedModuleRecords("business", businessSeedRecords());
      return getBusinessBundle();
    }

    return {
      customers: rowsToPayload(customerRows, businessSeed.customers),
      products: rowsToPayload(productRows, businessSeed.products),
      invoices: rowsToPayload(invoiceRows, businessSeed.invoices),
      bookkeeping: rowsToPayload(bookkeepingRows, businessSeed.bookkeeping),
      reports: rowsToPayload(reportRows, businessSeed.reports)
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
  if (resource === "customers") return "customers";
  if (resource === "products" || resource === "services") return "products";
  if (resource === "invoices") return "invoices";
  if (resource === "bookkeeping" || resource === "exports") return "bookkeeping";
  if (resource === "reports") return "reports";
  return null;
}

export function text(value: LocalizedText, locale: SupportedLocale) {
  return typeof value === "string" ? value : value[locale] ?? value.en;
}

export function businessCurrency(amount: number, currency = "EUR", locale: SupportedLocale = "en") {
  return new Intl.NumberFormat(locale === "de" ? "de-DE" : "en-US", {
    currency,
    maximumFractionDigits: 0,
    style: "currency"
  }).format(amount);
}

function businessSeedRecords() {
  return {
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
    products: businessSeed.products.map((product) => ({
      id: product.id,
      label: product.name,
      status: product.status,
      ownerId: product.revenueAccount,
      payload: product
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
