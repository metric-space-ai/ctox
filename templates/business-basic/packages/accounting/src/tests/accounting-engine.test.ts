import assert from "node:assert/strict";
import test from "node:test";
import {
  bankMatchConfidence,
  buildAssetAcquisitionJournalDraft,
  buildAssetDepreciationJournalDraft,
  buildAssetDisposalJournalDraft,
  buildBankMatchJournalDraft,
  buildCancellationCreditNote,
  buildCreditNoteJournalDraft,
  buildDunningProposals,
  buildDunningFeeJournalDraft,
  buildDatevExtfCsv,
  buildDatevExtfExportBundle,
  buildDatevExtfLinesFromJournalDrafts,
  buildEmployeeExpenseJournalDraft,
  buildPartialCreditNote,
  buildStraightLineDepreciationSchedule,
  buildBalanceSheet,
  buildBusinessAnalysis,
  buildGeneralLedger,
  buildOpenItems,
  buildPeriodCloseChecklist,
  buildReverseJournalDraft,
  buildSupplierDiscountJournalDraft,
  buildSupplierPaymentJournalDraft,
  buildTrialBalanceFromEntries,
  buildVatStatement,
  checkPurchaseOrderReceiptMatch,
  createSeriesState,
  findDuplicateBankLines,
  findDuplicateReceipts,
  findVendorCandidates,
  parseBankCsv,
  parseCamt053,
  parseMt940,
  prepareCreateVendorFromReceiptCommand,
  prepareImportBankStatementCommand,
  preparePaymentRunCommand,
  buildInvoiceJournalDraft,
  buildProfitAndLoss,
  buildZugferdXml,
  buildReceiptJournalDraft,
  formatMoney,
  LedgerPosting,
  moneyFromMajor,
  moneyToMajor,
  prepareAcceptBankMatchCommand,
  prepareCreditNoteCommand,
  prepareDunningFeeCommand,
  preparePurchaseOrderMatchCommand,
  prepareQuoteCommand,
  prepareQuoteConversionCommand,
  preparePostReceiptCommand,
  prepareReceiptClarificationCommand,
  prepareReviewReceiptExtractionCommand,
  prepareSendInvoiceCommand,
  prepareSubmitEmployeeExpenseCommand,
  prepareSupplierDiscountCommand,
  prepareSupplierPaymentCommand,
  quoteToInvoiceLike,
  reviewReceiptOcr,
  selectPaymentRunCandidates,
  validateQuoteForSend,
  validateInvoiceForSend,
  validateDatevExtf,
  validateZugferdXml,
  allocateNumber,
  assertPeriodOpen,
  type BusinessInvoiceLike,
  type InvoiceContext,
  closeFiscalPeriod,
  createParty,
  germanTaxRatesForChart,
  resolveGermanTaxRate,
  seedChartAccounts
} from "../index";

const invoice: BusinessInvoiceLike = {
  currency: "EUR",
  customerId: "cust-1",
  dueDate: "2026-05-31",
  id: "inv-1",
  issueDate: "2026-05-07",
  lines: [
    { productId: "prod-saas", quantity: 2, taxRate: 19, unitPrice: 100 },
    { productId: "prod-support", quantity: 1, taxRate: 19, unitPrice: 50 }
  ],
  netAmount: 250,
  number: "RE-2026-001",
  serviceDate: "2026-05-07",
  status: "Draft",
  taxAmount: 47.5,
  total: 297.5
};

const invoiceContext: InvoiceContext = {
  companyId: "company-1",
  companyName: "Metric Space UG",
  customer: {
    country: "Germany",
    id: "cust-1",
    name: "Kunstmen GmbH",
    taxId: "DE123456789"
  },
  defaultReceivableAccountId: "acc-ar",
  defaultRevenueAccountId: "acc-revenue",
  defaultTaxAccountId: "acc-vat-output",
  issuerAddressLines: ["Metric Space UG", "Berlin"],
  issuerTaxId: "37/123/45678",
  issuerVatId: "DE123456789",
  products: [
    { id: "prod-saas", name: "SaaS", revenueAccount: "8400 SaaS subscriptions" },
    { id: "prod-support", name: "Support", revenueAccount: "8401 Support retainers" }
  ]
};

test("Money stores fixed minor units and formats with currency", () => {
  const amount = moneyFromMajor(12.34, "EUR");
  assert.equal(amount.minor, 1234);
  assert.equal(moneyToMajor(amount), 12.34);
  assert.match(formatMoney(amount), /12,34/);
});

test("LedgerPosting rejects unbalanced journals and emits balanced drafts", () => {
  assert.throws(
    () => new LedgerPosting("company-1", "manual", "bad-1", "2026-05-07").debit("acc-a", 10).credit("acc-b", 9).toJournalDraft("manual"),
    /posting_debit_credit_mismatch/
  );

  const draft = new LedgerPosting("company-1", "manual", "ok-1", "2026-05-07")
    .debit("acc-a", 10)
    .credit("acc-b", 10)
    .toJournalDraft("manual");

  assert.equal(draft.lines.reduce((sum, line) => sum + line.debit.minor, 0), 1000);
  assert.equal(draft.lines.reduce((sum, line) => sum + line.credit.minor, 0), 1000);
});

test("Sending an invoice creates an agent command and balanced journal draft", () => {
  const validation = validateInvoiceForSend(invoice, invoiceContext);
  const command = prepareSendInvoiceCommand(invoice, invoiceContext);
  const journal = buildInvoiceJournalDraft(invoice, invoiceContext);

  assert.deepEqual(validation.errors, []);
  assert.equal(command.type, "SendInvoice");
  assert.equal(command.payload.invoiceNumber, "RE-2026-001");
  assert.equal(journal.type, "invoice");
  assert.equal(journal.lines[0]?.accountId, "acc-ar");
  assert.equal(journal.lines.reduce((sum, line) => sum + line.debit.minor, 0), 29750);
  assert.equal(journal.lines.reduce((sum, line) => sum + line.credit.minor, 0), 29750);
});

test("Quote commands validate the offer without touching the ledger, then convert to invoice posting", () => {
  const quote = {
    currency: "EUR" as const,
    customerId: "cust-1",
    id: "quote-1",
    issueDate: "2026-05-07",
    lines: [{ productId: "prod-saas", quantity: 1, taxRate: 19, unitPrice: 100 }],
    netAmount: 100,
    number: "ANG-2026-001",
    status: "Accepted",
    taxAmount: 19,
    total: 119,
    validUntil: "2026-05-21"
  };
  const validation = validateQuoteForSend(quote, invoiceContext);
  const quoteCommand = prepareQuoteCommand(quote, invoiceContext);
  const invoiceDraft = quoteToInvoiceLike(quote, {
    dueDate: "2026-06-06",
    invoiceId: "inv-from-quote-1",
    invoiceNumber: "RE-2026-101",
    issueDate: "2026-05-23"
  });
  const conversionCommand = prepareQuoteConversionCommand(quote, invoiceDraft, invoiceContext);
  const journal = buildInvoiceJournalDraft(invoiceDraft, invoiceContext);

  assert.deepEqual(validation.errors, []);
  assert.equal(quoteCommand.type, "PrepareQuote");
  assert.equal(conversionCommand.type, "ConvertQuoteToInvoice");
  assert.equal(journal.refId, "inv-from-quote-1");
  assert.equal(journal.lines.reduce((sum, line) => sum + line.debit.minor, 0), 11900);
  assert.equal(journal.lines.reduce((sum, line) => sum + line.credit.minor, 0), 11900);
});

test("Cancellation and partial credit notes reverse receivable, revenue, and VAT", () => {
  const cancellation = buildCancellationCreditNote({
    id: "cn-1",
    issueDate: "2026-05-08",
    number: "GS-2026-001",
    originalInvoice: invoice,
    reason: "Falscher Leistungszeitraum"
  });
  const cancellationCommand = prepareCreditNoteCommand(cancellation, invoice, invoiceContext);
  const cancellationJournal = buildCreditNoteJournalDraft(cancellation, invoice, invoiceContext);
  const partial = buildPartialCreditNote({
    currency: "EUR",
    id: "cn-2",
    issueDate: "2026-05-09",
    line: { productId: "prod-saas", quantity: 1, taxRate: 19, unitPrice: 50 },
    number: "GS-2026-002",
    originalInvoiceId: invoice.id,
    reason: "Nachlass"
  });
  const partialJournal = buildCreditNoteJournalDraft(partial, invoice, invoiceContext);

  assert.equal(cancellationCommand.type, "CreateCancellationCreditNote");
  assert.equal(cancellationJournal.type, "reverse");
  assert.equal(cancellationJournal.lines.find((line) => line.accountId === "acc-ar")?.credit.minor, 29750);
  assert.equal(cancellationJournal.lines.reduce((sum, line) => sum + line.debit.minor, 0), 29750);
  assert.equal(partialJournal.lines.find((line) => line.accountId === "acc-ar")?.credit.minor, 5950);
  assert.equal(partialJournal.lines.reduce((sum, line) => sum + line.debit.minor, 0), 5950);
});

test("Dunning fee command can create a VAT-bearing fee posting", () => {
  const command = prepareDunningFeeCommand(invoice, invoiceContext, { feeAmount: 12, level: 2 });
  const journal = buildDunningFeeJournalDraft(invoice, invoiceContext, {
    feeAmount: 12,
    feeRevenueAccountId: "acc-dunning-fees",
    issueDate: "2026-06-15",
    level: 2
  });

  assert.equal(command.type, "RunDunning");
  assert.equal(command.payload.level, 2);
  assert.ok(journal);
  assert.equal(journal.lines.find((line) => line.accountId === "acc-ar")?.debit.minor, 1200);
  assert.equal(journal.lines.reduce((sum, line) => sum + line.credit.minor, 0), 1200);
});

test("Invoice validator blocks Kleinunternehmer and reverse-charge tax misuse", () => {
  const ku = validateInvoiceForSend({
    ...invoice,
    kleinunternehmer: true,
    lines: [{ productId: "prod-saas", quantity: 1, taxRate: 0, unitPrice: 100 }],
    netAmount: 100,
    notes: "Gemäß § 19 UStG wird keine Umsatzsteuer berechnet.",
    taxAmount: 0,
    total: 100
  }, invoiceContext);
  const reverseCharge = validateInvoiceForSend({
    ...invoice,
    reverseCharge: true
  }, invoiceContext);
  const inconsistentTotals = validateInvoiceForSend({
    ...invoice,
    taxAmount: 1,
    total: 251
  }, invoiceContext);
  const invalidDates = validateInvoiceForSend({
    ...invoice,
    dueDate: "2026-05-01",
    issueDate: "2026-05-07"
  }, invoiceContext);
  const unsupportedTax = validateInvoiceForSend({
    ...invoice,
    lines: [{ productId: "prod-saas", quantity: 1, taxRate: 13, unitPrice: 100 }],
    netAmount: 100,
    taxAmount: 13,
    total: 113
  }, invoiceContext);

  assert.deepEqual(ku.errors, []);
  assert.match(reverseCharge.errors.join(","), /reverse_charge_invoice_must_not_have_tax/);
  assert.match(inconsistentTotals.errors.join(","), /invoice_tax_amount_mismatch/);
  assert.match(inconsistentTotals.errors.join(","), /invoice_total_amount_mismatch/);
  assert.match(invalidDates.errors.join(","), /due_date_before_issue_date/);
  assert.match(unsupportedTax.errors.join(","), /line_1_unsupported_tax_rate/);
});

test("Posting an inbound receipt debits expense and VAT, then credits payable", () => {
  const receipt = {
    currency: "EUR",
    expenseAccountId: "acc-expense-software",
    id: "rec-1",
    netAmount: 100,
    number: "EB-2026-001",
    payableAccountId: "acc-ap",
    receiptDate: "2026-05-07",
    status: "Needs review",
    taxAmount: 19,
    total: 119,
    vendorName: "Figma"
  };

  const command = preparePostReceiptCommand(receipt, "company-1");
  const journal = buildReceiptJournalDraft(receipt, "company-1");

  assert.equal(command.type, "PostReceipt");
  assert.equal(journal.lines.length, 3);
  assert.equal(journal.lines.reduce((sum, line) => sum + line.debit.minor, 0), 11900);
  assert.equal(journal.lines.reduce((sum, line) => sum + line.credit.minor, 0), 11900);

  const reducedRateReceipt = buildReceiptJournalDraft({
    ...receipt,
    id: "rec-7",
    netAmount: 100,
    number: "EB-2026-007",
    taxAmount: 7,
    taxCode: "DE_7_INPUT",
    total: 107
  }, "company-1");
  assert.ok(reducedRateReceipt.lines.some((line) => line.accountId === "acc-vat-input-7" && line.taxCode === "DE_7_INPUT" && moneyToMajor(line.debit) === 7));
});

test("Receipt OCR review blocks unclear receipts and prepares vendor or duplicate commands", () => {
  const clearFields = [
    { confidence: 0.95, label: "vendor", value: "Hetzner Online GmbH" },
    { confidence: 0.92, label: "invoice_number", value: "HET-2026-0517" },
    { confidence: 0.9, label: "receipt_date", value: "2026-05-03" },
    { confidence: 0.94, label: "total_amount", value: "142.80" }
  ];
  const weakFields = [
    { confidence: 0.41, label: "vendor", value: "Restaurant" },
    { confidence: 0.38, label: "total_amount", value: "86.40" }
  ];
  const clearReview = reviewReceiptOcr({ companyId: "company-1", fields: clearFields, receiptId: "rec-ocr-1" });
  const weakReview = reviewReceiptOcr({ companyId: "company-1", fields: weakFields, receiptId: "rec-ocr-2" });
  const reviewCommand = prepareReviewReceiptExtractionCommand({ companyId: "company-1", fields: clearFields, receiptId: "rec-ocr-1" });
  const clarificationCommand = prepareReceiptClarificationCommand({ companyId: "company-1", fields: weakFields, receiptId: "rec-ocr-2" });
  const vendorCommand = prepareCreateVendorFromReceiptCommand({
    companyId: "company-1",
    defaultPayableAccountId: "acc-ap",
    iban: "DE123",
    receiptId: "rec-ocr-1",
    vendorName: "Hetzner Online GmbH"
  });
  const vendorCandidates = findVendorCandidates({
    companyId: "company-1",
    iban: "DE123",
    receiptId: "rec-ocr-1",
    vendorName: "Hetzner Online GmbH"
  }, [{ id: "vendor-1", iban: "DE123", name: "Hetzner Online GmbH" }]);
  const duplicates = findDuplicateReceipts({
    amount: 142.8,
    companyId: "company-1",
    currency: "EUR",
    id: "rec-new",
    vendorInvoiceNumber: "HET-2026-0517",
    vendorName: "Hetzner Online GmbH"
  }, [{
    amount: 142.8,
    currency: "EUR",
    id: "rec-existing",
    vendorInvoiceNumber: "HET-2026-0517",
    vendorName: "Hetzner Online GmbH"
  }]);

  assert.equal(clearReview.status, "reviewed");
  assert.equal(weakReview.status, "needs_clarification");
  assert.equal(reviewCommand.type, "ReviewReceiptExtraction");
  assert.equal(clarificationCommand.type, "RequestReceiptClarification");
  assert.equal(vendorCommand.type, "CreateVendorFromReceipt");
  assert.equal(vendorCandidates[0]?.vendor.id, "vendor-1");
  assert.equal(duplicates[0]?.receipt.id, "rec-existing");
});

test("Purchase-order matching reports exact matches and price variances", () => {
  const matched = checkPurchaseOrderReceiptMatch({
    companyId: "company-1",
    invoicedQuantity: 25,
    invoicedUnitPrice: 50,
    orderedQuantity: 25,
    orderedUnitPrice: 50,
    purchaseOrderId: "po-1",
    receiptId: "rec-po-1",
    receivedQuantity: 25
  });
  const variance = checkPurchaseOrderReceiptMatch({
    companyId: "company-1",
    invoicedQuantity: 25,
    invoicedUnitPrice: 60,
    orderedQuantity: 25,
    orderedUnitPrice: 50,
    purchaseOrderId: "po-2",
    receiptId: "rec-po-2",
    receivedQuantity: 25
  });
  const command = preparePurchaseOrderMatchCommand({
    companyId: "company-1",
    invoicedQuantity: 25,
    invoicedUnitPrice: 60,
    orderedQuantity: 25,
    orderedUnitPrice: 50,
    purchaseOrderId: "po-2",
    receiptId: "rec-po-2",
    receivedQuantity: 25
  });

  assert.equal(matched.status, "matched");
  assert.equal(variance.status, "variance");
  assert.equal(variance.totalVariance, 250);
  assert.equal(command.type, "CheckPurchaseOrderMatch");
});

test("Employee expenses create payable drafts without posting unreadable receipts", () => {
  const expense = {
    companyId: "company-1",
    currency: "EUR",
    employeeName: "Milena Ducic",
    employeePayableAccountId: "acc-employee-payables",
    expenseAccountId: "acc-travel",
    expenseDate: "2026-04-28",
    grossAmount: 38.6,
    id: "expense-1",
    netAmount: 38.6,
    projectId: "project-rem",
    taxAmount: 0,
    taxCode: "DE_0"
  };
  const command = prepareSubmitEmployeeExpenseCommand(expense);
  const journal = buildEmployeeExpenseJournalDraft(expense);

  assert.equal(command.type, "SubmitEmployeeExpense");
  assert.equal(journal.refType, "employee_expense");
  assert.equal(journal.lines[0]?.projectId, "project-rem");
  assert.equal(journal.lines.reduce((sum, line) => sum + line.debit.minor, 0), 3860);
  assert.equal(journal.lines.reduce((sum, line) => sum + line.credit.minor, 0), 3860);
});

test("Bank match commands preserve reconciler confidence context", () => {
  const transaction = {
    amount: 297.5,
    bookingDate: "2026-05-12",
    counterparty: "Kunstmen GmbH",
    currency: "EUR",
    id: "bank-1",
    matchedRecordId: "inv-1",
    matchType: "invoice" as const,
    purpose: "RE-2026-001",
    status: "Suggested"
  };

  const command = prepareAcceptBankMatchCommand(transaction, "company-1");

  assert.equal(bankMatchConfidence(transaction), 0.92);
  assert.equal(command.type, "AcceptBankMatch");
  assert.equal(command.payload.matchedRecordId, "inv-1");
});

test("Bank match journal drafts post incoming payments to bank and receivables", () => {
  const journal = buildBankMatchJournalDraft({
    amount: 297.5,
    bookingDate: "2026-05-12",
    counterparty: "Kunstmen GmbH",
    currency: "EUR",
    id: "bank-1",
    matchedRecordId: "inv-1",
    matchType: "invoice",
    purpose: "RE-2026-001",
    status: "Suggested"
  }, {
    accountsPayableAccountId: "acc-ap",
    accountsReceivableAccountId: "acc-ar",
    bankAccountId: "acc-bank",
    bankFeeAccountId: "acc-fees",
    companyId: "company-1"
  });

  assert.equal(journal.type, "payment");
  assert.equal(journal.lines[0]?.accountId, "acc-bank");
  assert.equal(journal.lines[1]?.accountId, "acc-ar");
  assert.equal(journal.lines.reduce((sum, line) => sum + line.debit.minor, 0), 29750);
  assert.equal(journal.lines.reduce((sum, line) => sum + line.credit.minor, 0), 29750);
});

test("Payables commands handle supplier payments, cash discounts and payment runs", () => {
  const supplierPayment = {
    allocations: [
      { amount: 820, receiptId: "rec-tel-1" },
      { amount: 760, receiptId: "rec-tel-2" },
      { amount: 800, receiptId: "rec-tel-3" }
    ],
    bankAccountId: "acc-bank",
    companyId: "company-1",
    currency: "EUR",
    id: "pay-telekom",
    payableAccountId: "acc-ap",
    paymentDate: "2026-05-15",
    vendorName: "Telekom Deutschland GmbH"
  };
  const paymentCommand = prepareSupplierPaymentCommand(supplierPayment);
  const paymentJournal = buildSupplierPaymentJournalDraft(supplierPayment);
  const discountPayment = {
    allocations: [{ amount: 1190, receiptId: "rec-office-1" }],
    bankAccountId: "acc-bank",
    companyId: "company-1",
    currency: "EUR",
    discountAccountId: "acc-purchase-discounts",
    discountGrossAmount: 23.8,
    discountNetAmount: 20,
    id: "pay-office-discount",
    inputVatAccountId: "acc-vat-input",
    inputVatCorrectionAmount: 3.8,
    paidAmount: 1166.2,
    payableAccountId: "acc-ap",
    paymentDate: "2026-05-10",
    vendorName: "Buerobedarf Nord"
  };
  const discountCommand = prepareSupplierDiscountCommand(discountPayment);
  const discountJournal = buildSupplierDiscountJournalDraft(discountPayment);
  const runInput = {
    candidates: [
      { amount: 142.8, currency: "EUR", dueDate: "2026-05-12", receiptId: "rec-hosting", vendorName: "Hetzner" },
      { amount: 714, blocked: true, currency: "EUR", dueDate: "2026-05-10", receiptId: "rec-print", vendorName: "Printwerk Hamburg" },
      { amount: 416.5, currency: "EUR", dueDate: "2026-05-18", receiptId: "rec-insurance", vendorName: "Versicherung AG" }
    ],
    companyId: "company-1",
    dueBy: "2026-05-15",
    id: "run-2026-05-15"
  };
  const runCommand = preparePaymentRunCommand(runInput);
  const selected = selectPaymentRunCandidates(runInput);

  assert.equal(paymentCommand.type, "PostSupplierPayment");
  assert.equal(paymentJournal.lines.length, 4);
  assert.equal(paymentJournal.lines.reduce((sum, line) => sum + line.debit.minor, 0), 238000);
  assert.equal(paymentJournal.lines.reduce((sum, line) => sum + line.credit.minor, 0), 238000);
  assert.equal(discountCommand.type, "ApplySupplierDiscount");
  assert.equal(discountJournal.lines.reduce((sum, line) => sum + line.debit.minor, 0), 119000);
  assert.equal(discountJournal.lines.reduce((sum, line) => sum + line.credit.minor, 0), 119000);
  assert.equal(runCommand.type, "PreparePaymentRun");
  assert.equal(runCommand.payload.selectedCount, 1);
  assert.equal(selected[0]?.receiptId, "rec-hosting");
});

test("DATEV EXTF CSV quotes text and keeps German decimal comma", () => {
  const lines = [
    {
      accountCode: "8400",
      amount: 297.5,
      contraAccountCode: "1200",
      currency: "EUR",
      date: "2026-05-07",
      documentNumber: "RE-2026-001",
      side: "H" as const,
      taxCode: "19",
      text: "SaaS; Support"
    }
  ];
  const csv = buildDatevExtfCsv([
    ...lines
  ], {
    accountLength: 4,
    clientNumber: "67890",
    consultantNumber: "12345",
    fiscalYearStart: "20260101"
  });

  assert.match(csv.split("\n")[0] ?? "", /EXTF;700/);
  assert.match(csv, /297,50/);
  assert.match(csv, /"SaaS; Support"/);
  assert.deepEqual(validateDatevExtf(lines, {
    accountLength: 4,
    clientNumber: "67890",
    consultantNumber: "12345",
    fiscalYearStart: "20260101"
  }).errors, []);
  assert.match(validateDatevExtf([{ ...lines[0], accountCode: "84", amount: 0 }]).errors.join(","), /datev_line_1_amount_must_be_positive/);
});

test("ZUGFeRD XML includes buyer, totals, due date and tax category", () => {
  const xml = buildZugferdXml(invoice, invoiceContext);

  assert.match(xml, /CrossIndustryInvoice/);
  assert.match(xml, /Kunstmen GmbH/);
  assert.match(xml, /GrandTotalAmount>297\.50/);
  assert.match(xml, /DueDateDateTime/);
  assert.match(xml, /CategoryCode>S/);
  assert.deepEqual(validateZugferdXml(xml).errors, []);
  assert.match(validateZugferdXml("<xml />").errors.join(","), /cross_industry_invoice_missing/);
});

test("Number series allocates fiscal-year scoped gap-free numbers", () => {
  const first = allocateNumber(createSeriesState({ date: "2026-05-07", key: "invoice" }));
  const second = allocateNumber(first.state);

  assert.equal(first.number, "RE-2026-0001");
  assert.equal(second.number, "RE-2026-0002");
  assert.equal(second.state.nextValue, 3);
});

test("Bank CSV import normalizes decimal commas and duplicate fingerprints", () => {
  const statement = parseBankCsv([
    "booking_date;value_date;amount;currency;remitter_name;remitter_iban;purpose;end_to_end_ref",
    "2026-05-07;2026-05-07;297,50;EUR;Kunstmen GmbH;DE123;RE-2026-001;E2E-1",
    "2026-05-07;2026-05-07;297,50;EUR;Kunstmen GmbH;DE123;RE-2026-001;E2E-1"
  ].join("\n"));
  const duplicates = findDuplicateBankLines(statement.lines);

  assert.equal(statement.lines[0]?.amount, 297.5);
  assert.equal(duplicates[0]?.duplicateOf, undefined);
  assert.equal(duplicates[1]?.duplicateOf?.lineNo, 1);
});

test("Bank statement import command carries source fingerprint", () => {
  const command = prepareImportBankStatementCommand({
    companyId: "company-1",
    format: "csv",
    sourceFilename: "statement.csv",
    sourceSha256: "abc123"
  });

  assert.equal(command.type, "ImportBankStatement");
  assert.equal(command.refType, "bank_statement");
  assert.equal(command.payload.sourceSha256, "abc123");
});

test("MT940 and camt.053 imports parse basic transaction lines", () => {
  const mt940 = parseMt940(":61:2605070507C297,50NTRFRE-2026-001");
  const camt = parseCamt053(`
    <Document xmlns:ns="urn:iso:std:iso:20022:tech:xsd:camt.053.001.08"><ns:BkToCstmrStmt><ns:Stmt><ns:Ntry>
      <ns:Amt Ccy="EUR">221.94</ns:Amt><ns:CdtDbtInd>DBIT</ns:CdtDbtInd>
      <ns:BookgDt><ns:Dt>2026-05-07</ns:Dt></ns:BookgDt>
      <ns:ValDt><ns:Dt>2026-05-07</ns:Dt></ns:ValDt>
      <ns:NtryDtls><ns:TxDtls><ns:Refs><ns:EndToEndId>E2E-2</ns:EndToEndId></ns:Refs><ns:RmtInf><ns:Ustrd>R-2026-017</ns:Ustrd></ns:RmtInf></ns:TxDtls></ns:NtryDtls>
    </ns:Ntry></ns:Stmt></ns:BkToCstmrStmt></Document>
  `);

  assert.equal(mt940.lines[0]?.amount, 297.5);
  assert.equal(mt940.lines[0]?.bookingDate, "2026-05-07");
  assert.equal(camt.lines[0]?.amount, -221.94);
  assert.equal(camt.lines[0]?.purpose, "R-2026-017");
});

test("Dunning runner proposes the next unpaid overdue level only", () => {
  const proposals = buildDunningProposals({
    asOf: "2026-05-20",
    companyId: "company-1",
    invoices: [
      { balanceDue: 100, customerId: "cust-1", dueDate: "2026-05-01", id: "inv-1", number: "RE-1", reminderLevel: 1, status: "Overdue" },
      { balanceDue: 0, customerId: "cust-2", dueDate: "2026-05-01", id: "inv-2", number: "RE-2", reminderLevel: 0, status: "Paid" }
    ]
  });

  assert.equal(proposals.length, 1);
  assert.equal(proposals[0]?.command.type, "RunDunning");
  assert.equal(proposals[0]?.command.payload.level, 2);
});

test("Chart seed, tax rates and parties expose German accounting defaults", () => {
  const accounts = seedChartAccounts({ companyId: "company-1", chart: "skr03" });
  const skr04Accounts = seedChartAccounts({ companyId: "company-1", chart: "skr04" });
  const tax = resolveGermanTaxRate({ taxRate: 19 });
  const skr04Tax = germanTaxRatesForChart("skr04");
  const party = createParty({
    defaultReceivableAccountId: "acc-ar",
    id: "cust-1",
    kind: "customer",
    name: "Kunstmen GmbH"
  });

  assert.equal(accounts.find((account) => account.code === "8400")?.externalId, "acc-revenue-saas");
  assert.equal(skr04Accounts.find((account) => account.code === "4400")?.externalId, "acc-revenue-saas");
  assert.equal(tax.accountId, "acc-vat-output");
  assert.equal(skr04Tax.find((rate) => rate.code === "DE_19")?.accountCode, "3806");
  assert.equal(skr04Tax.find((rate) => rate.code === "DE_19_INPUT")?.accountCode, "1406");
  assert.equal(party.defaultReceivableAccountId, "acc-ar");
});

test("Fiscal periods prevent postings into closed ranges", () => {
  const open = { endDate: "2026-12-31", id: "fy-2026", startDate: "2026-01-01", status: "open" as const };
  const may = { endDate: "2026-05-31", id: "fy-2026-05", startDate: "2026-05-01", status: "open" as const };
  assert.equal(assertPeriodOpen([open], "2026-05-07").id, "fy-2026");
  assert.equal(assertPeriodOpen([open, may], "2026-05-07").id, "fy-2026-05");
  assert.throws(() => assertPeriodOpen([closeFiscalPeriod(open)], "2026-05-07"), /fiscal_period_closed/);
  assert.throws(() => assertPeriodOpen([open, closeFiscalPeriod(may)], "2026-05-07"), /fiscal_period_closed/);
});

test("Reports derive GL, P&L and balance sheet from journal drafts", () => {
  const journal = buildInvoiceJournalDraft(invoice, invoiceContext);
  const accounts = seedChartAccounts({ companyId: "company-1", chart: "skr03" });
  const ledger = buildGeneralLedger({ accountId: "acc-ar", entries: [journal] });
  const pnl = buildProfitAndLoss({ accounts, entries: [journal] });
  const bwa = buildBusinessAnalysis({ accounts, entries: [journal] });
  const balanceSheet = buildBalanceSheet({ accounts, entries: [journal] });

  assert.equal(ledger[0]?.runningBalance, 297.5);
  assert.equal(pnl.income, 250);
  assert.equal(bwa.revenue, 250);
  assert.equal(bwa.ebit, 250);
  assert.equal(balanceSheet.assets, 297.5);
  assert.equal(balanceSheet.liabilities, 47.5);
  assert.equal(balanceSheet.retainedEarnings, 250);
  assert.equal(balanceSheet.balanced, true);
});

test("Open items allocate invoice and receipt payments by matched record id", () => {
  const accounts = seedChartAccounts({ companyId: "company-1", chart: "skr03" });
  const customerInvoice = buildInvoiceJournalDraft(invoice, invoiceContext);
  const paymentContext = {
    accountsPayableAccountId: "acc-ap",
    accountsReceivableAccountId: "acc-ar",
    bankAccountId: "acc-bank",
    bankFeeAccountId: "acc-fees",
    companyId: "company-1"
  };
  const partialCustomerPayment = buildBankMatchJournalDraft({
    amount: 100,
    bookingDate: "2026-05-15",
    counterparty: "Kunstmen GmbH",
    currency: "EUR",
    id: "bank-in-partial",
    matchedRecordId: "inv-1",
    matchType: "invoice",
    purpose: "RE-2026-001",
    status: "Suggested"
  }, paymentContext);
  const receipt = buildReceiptJournalDraft({
    currency: "EUR",
    expenseAccountId: "acc-software",
    id: "rec-open-item",
    netAmount: 100,
    number: "EB-2026-002",
    payableAccountId: "acc-ap",
    receiptDate: "2026-05-01",
    status: "Posted",
    taxAmount: 19,
    total: 119,
    vendorName: "Figma"
  }, "company-1");
  const receiptPayment = buildBankMatchJournalDraft({
    amount: -119,
    bookingDate: "2026-05-12",
    counterparty: "Figma",
    currency: "EUR",
    id: "bank-out-receipt",
    matchedRecordId: "rec-open-item",
    matchType: "receipt",
    purpose: "EB-2026-002",
    status: "Suggested"
  }, paymentContext);

  const openItems = buildOpenItems({
    accounts,
    asOf: "2026-06-15",
    dueDatesByRef: {
      "inv-1": "2026-05-31",
      "rec-open-item": "2026-05-10"
    },
    entries: [customerInvoice, partialCustomerPayment, receipt, receiptPayment],
    includePaid: true
  });

  const receivable = openItems.rows.find((row) => row.refId === "inv-1");
  const payable = openItems.rows.find((row) => row.refId === "rec-open-item");
  assert.equal(receivable?.status, "partial");
  assert.equal(receivable?.paidAmount, 100);
  assert.equal(receivable?.outstandingAmount, 197.5);
  assert.equal(payable?.status, "paid");
  assert.equal(openItems.openReceivables, 197.5);
  assert.equal(openItems.openPayables, 0);
  assert.equal(openItems.buckets.overdue1To30, 197.5);
});

test("Period close checklist blocks unbalanced or incomplete accounting periods", () => {
  const period = { endDate: "2026-05-31", id: "fy-2026-05", startDate: "2026-05-01", status: "open" as const };
  const ready = buildPeriodCloseChecklist({
    datevExported: true,
    entries: [buildInvoiceJournalDraft(invoice, invoiceContext)],
    period,
    vatStatementReviewed: true
  });
  const blocked = buildPeriodCloseChecklist({
    entries: [{
      lines: [
        { credit: moneyFromMajor(0), debit: moneyFromMajor(10) },
        { credit: moneyFromMajor(9), debit: moneyFromMajor(0) }
      ],
      postingDate: "2026-05-07",
      refId: "bad-manual",
      type: "manual"
    }],
    openDraftCount: 1,
    period,
    unmatchedBankLineCount: 1,
    unpostedReceiptCount: 1,
    vatStatementReviewed: false
  });

  assert.equal(ready.ready, true);
  assert.equal(ready.status, "ready");
  assert.match(blocked.blockers.join(","), /unbalanced_journal:manual:bad-manual/);
  assert.match(blocked.blockers.join(","), /open_drafts_in_period/);
  assert.match(blocked.blockers.join(","), /vat_statement_not_reviewed/);
  assert.equal(blocked.ready, false);
});

test("GoBD reversal drafts keep original journal immutable and invert all lines", () => {
  const original = buildInvoiceJournalDraft(invoice, invoiceContext);
  const reverse = buildReverseJournalDraft(original, {
    postingDate: "2026-05-08",
    refId: "inv-1-storno"
  });

  assert.equal(reverse.type, "reverse");
  assert.equal(reverse.refType, original.refType);
  assert.equal(reverse.lines[0]?.credit.minor, original.lines[0]?.debit.minor);
  assert.equal(reverse.lines.reduce((sum, line) => sum + line.debit.minor, 0), 29750);
  assert.equal(reverse.lines.reduce((sum, line) => sum + line.credit.minor, 0), 29750);
  assert.equal(original.type, "invoice");
});

test("VAT statement derives 7 percent and reverse-charge bases from tagged journal lines", () => {
  const accounts = seedChartAccounts({ companyId: "company-1", chart: "skr03" });
  const reducedRateInvoice = buildInvoiceJournalDraft({
    ...invoice,
    id: "inv-reduced-rate",
    lines: [{ productId: "prod-support", quantity: 1, taxRate: 7, unitPrice: 100 }],
    netAmount: 100,
    number: "RE-2026-007",
    taxAmount: 7,
    total: 107
  }, invoiceContext);
  const reverseChargeInvoice = buildInvoiceJournalDraft({
    ...invoice,
    id: "inv-reverse-charge",
    lines: [{ productId: "prod-saas", quantity: 1, reverseCharge: true, taxRate: 0, unitPrice: 250 }],
    netAmount: 250,
    number: "RE-2026-RC",
    reverseCharge: true,
    taxAmount: 0,
    total: 250
  }, invoiceContext);

  const vatStatement = buildVatStatement({ accounts, entries: [reducedRateInvoice, reverseChargeInvoice] });

  assert.equal(vatStatement.outputVat, 7);
  assert.equal(vatStatement.boxes.find((box) => box.code === "86")?.amount, 100);
  assert.equal(vatStatement.boxes.find((box) => box.code === "RC")?.amount, 250);
  assert.equal(vatStatement.boxes.find((box) => box.code === "83")?.amount, 7);
  assert.ok(reducedRateInvoice.lines.some((line) => line.taxCode === "DE_7_OUTPUT" && moneyToMajor(line.credit) === 100));
  assert.ok(reducedRateInvoice.lines.some((line) => line.accountId === "acc-vat-output-7" && line.taxCode === "DE_7_OUTPUT" && moneyToMajor(line.credit) === 7));
  assert.ok(reverseChargeInvoice.lines.some((line) => line.taxCode === "DE_RC" && moneyToMajor(line.credit) === 250));
});

test("German bookkeeping acceptance scenario derives ledger, DATEV, P&L and balance sheet values", () => {
  const accounts = seedChartAccounts({ companyId: "company-1", chart: "skr03" });
  const paymentContext = {
    accountsPayableAccountId: "acc-ap",
    accountsReceivableAccountId: "acc-ar",
    bankAccountId: "acc-bank",
    bankFeeAccountId: "acc-fees",
    companyId: "company-1"
  };
  const opening = new LedgerPosting("company-1", "manual", "opening-capital-2026", "2026-01-01")
    .debit("acc-bank", 10000)
    .credit("acc-equity", 10000)
    .toJournalDraft("manual", "Opening capital contribution.");
  const serviceInvoice = buildInvoiceJournalDraft({
    ...invoice,
    id: "inv-acceptance-1",
    issueDate: "2026-02-01",
    lines: [{ productId: "prod-saas", quantity: 1, taxRate: 19, unitPrice: 1000 }],
    netAmount: 1000,
    number: "RE-2026-0001",
    taxAmount: 190,
    total: 1190
  }, invoiceContext);
  const servicePayment = buildBankMatchJournalDraft({
    amount: 1190,
    bookingDate: "2026-02-14",
    counterparty: "Kunstmen GmbH",
    currency: "EUR",
    id: "bank-in-1",
    matchedRecordId: "inv-acceptance-1",
    matchType: "invoice",
    purpose: "RE-2026-0001",
    status: "Suggested"
  }, paymentContext);
  const cloudReceipt = buildReceiptJournalDraft({
    currency: "EUR",
    expenseAccountId: "acc-software",
    id: "rec-cloud-1",
    netAmount: 200,
    number: "EB-2026-0001",
    payableAccountId: "acc-ap",
    receiptDate: "2026-03-05",
    status: "Reviewed",
    taxAmount: 38,
    total: 238,
    vendorName: "Figma"
  }, "company-1");
  const cloudPayment = buildBankMatchJournalDraft({
    amount: -238,
    bookingDate: "2026-03-10",
    counterparty: "Figma",
    currency: "EUR",
    id: "bank-out-cloud-1",
    matchedRecordId: "rec-cloud-1",
    matchType: "receipt",
    purpose: "EB-2026-0001",
    status: "Suggested"
  }, paymentContext);
  const asset = {
    accumulatedDepreciationAccountId: "acc-accumulated-depreciation",
    acquisitionAccountId: "acc-ap",
    acquisitionCost: 1200,
    acquisitionDate: "2025-12-01",
    assetAccountId: "acc-fixed-assets",
    currency: "EUR" as const,
    depreciationExpenseAccountId: "acc-depreciation",
    id: "asset-acceptance-notebook",
    name: "Notebook",
    salvageValue: 0,
    usefulLifeMonths: 60
  };
  const assetAcquisition = buildAssetAcquisitionJournalDraft({
    asset,
    companyId: "company-1",
    inputVatAccountId: "acc-vat-input",
    inputVatAmount: 228,
    payableAccountId: "acc-ap"
  });
  const assetPayment = buildBankMatchJournalDraft({
    amount: -1428,
    bookingDate: "2026-01-15",
    counterparty: "Hardware GmbH",
    currency: "EUR",
    id: "bank-out-asset-1",
    matchedRecordId: "asset-acceptance-notebook",
    matchType: "receipt",
    purpose: "Asset acquisition asset-acceptance-notebook",
    status: "Suggested"
  }, paymentContext);
  const depreciationEntries = buildStraightLineDepreciationSchedule(asset)
    .filter((line) => line.fiscalYear === 2026)
    .map((line) => buildAssetDepreciationJournalDraft({ asset, companyId: "company-1", line }));
  const entries = [opening, assetAcquisition, assetPayment, serviceInvoice, servicePayment, cloudReceipt, cloudPayment, ...depreciationEntries];
  const trialBalance = buildTrialBalanceFromEntries({ accounts, entries });
  const generalLedger = buildGeneralLedger({ accountId: "acc-bank", entries });
  const pnl = buildProfitAndLoss({ accounts, entries });
  const bwa = buildBusinessAnalysis({ accounts, entries });
  const balanceSheet = buildBalanceSheet({ accounts, entries });
  const vatStatement = buildVatStatement({ accounts, entries });
  const datevLines = buildDatevExtfLinesFromJournalDrafts({ accounts, entries });
  const datevBundle = buildDatevExtfExportBundle({
    accounts,
    entries,
    period: { endDate: "2026-12-31", startDate: "2026-01-01" },
    settings: {
      accountLength: 4,
      clientNumber: "67890",
      consultantNumber: "12345",
      fiscalYearStart: "20260101"
    }
  });
  const datevCsv = buildDatevExtfCsv(datevLines, {
    accountLength: 4,
    clientNumber: "67890",
    consultantNumber: "12345",
    fiscalYearStart: "20260101"
  });

  assert.equal(depreciationEntries.length, 12);
  assert.equal(journalSideTotal(entries, "debit"), 15952);
  assert.equal(journalSideTotal(entries, "credit"), 15952);
  assert.equal(generalLedger.at(-1)?.runningBalance, 9524);
  assert.equal(accountBalance(trialBalance, "acc-bank"), 9524);
  assert.equal(accountBalance(trialBalance, "acc-ar"), 0);
  assert.equal(accountBalance(trialBalance, "acc-fixed-assets"), 1200);
  assert.equal(accountBalance(trialBalance, "acc-accumulated-depreciation"), -240);
  assert.equal(accountBalance(trialBalance, "acc-vat-input"), 266);
  assert.equal(accountBalance(trialBalance, "acc-ap"), 0);
  assert.equal(accountBalance(trialBalance, "acc-vat-output"), 190);
  assert.equal(accountBalance(trialBalance, "acc-equity"), 10000);
  assert.equal(accountBalance(trialBalance, "acc-software"), 200);
  assert.equal(accountBalance(trialBalance, "acc-depreciation"), 240);
  assert.equal(accountBalance(trialBalance, "acc-revenue-saas"), 1000);
  assert.equal(pnl.income, 1000);
  assert.equal(pnl.expense, 440);
  assert.equal(pnl.netIncome, 560);
  assert.equal(bwa.revenue, 1000);
  assert.equal(bwa.operatingExpenses, 200);
  assert.equal(bwa.depreciation, 240);
  assert.equal(bwa.ebit, 560);
  assert.equal(vatStatement.outputVat, 190);
  assert.equal(vatStatement.inputVat, 266);
  assert.equal(vatStatement.netPosition, -76);
  assert.equal(vatStatement.payable, 0);
  assert.equal(vatStatement.refundable, 76);
  assert.deepEqual(vatStatement.boxes.find((box) => box.code === "81"), {
    amount: 1000,
    amountKind: "base",
    code: "81",
    label: "Steuerpflichtige Umsaetze 19%",
    source: "invoice_revenue_lines_de_19_output",
    taxRate: 19
  });
  assert.deepEqual(vatStatement.boxes.find((box) => box.code === "66"), {
    amount: 266,
    amountKind: "tax",
    code: "66",
    label: "Abziehbare Vorsteuer",
    source: "input_vat_tax_accounts"
  });
  assert.equal(vatStatement.boxes.find((box) => box.code === "83")?.amount, -76);
  assert.equal(balanceSheet.assets, 10750);
  assert.equal(balanceSheet.liabilities, 190);
  assert.equal(balanceSheet.equity, 10560);
  assert.equal(balanceSheet.retainedEarnings, 560);
  assert.equal(balanceSheet.difference, 0);
  assert.equal(balanceSheet.balanced, true);
  assert.ok(datevLines.some((line) => line.accountCode === "0480" && line.side === "S" && line.amount === 1200));
  assert.ok(datevLines.some((line) => line.accountCode === "0490" && line.side === "H" && line.amount === 20));
  assert.ok(datevLines.some((line) => line.accountCode === "4830" && line.side === "S" && line.amount === 20));
  assert.match(datevCsv, /1190,00/);
  assert.match(datevCsv, /DE_19_INPUT/);
  assert.match(datevCsv, /DE_19_OUTPUT/);
  assert.equal(datevBundle.lineCount, buildDatevExtfLinesFromJournalDrafts({
    accounts,
    entries: entries.filter((entry) => entry.postingDate >= "2026-01-01" && entry.postingDate <= "2026-12-31")
  }).length);
  assert.equal(datevBundle.totals.debit, datevBundle.totals.credit);
});

test("Fixed assets create acquisition and depreciation journals for the balance sheet", () => {
  const asset = {
    accumulatedDepreciationAccountId: "acc-accumulated-depreciation",
    acquisitionAccountId: "acc-ap",
    acquisitionCost: 1200,
    acquisitionDate: "2026-01-15",
    assetAccountId: "acc-fixed-assets",
    currency: "EUR" as const,
    depreciationExpenseAccountId: "acc-depreciation",
    id: "asset-1",
    name: "Notebook",
    salvageValue: 0,
    usefulLifeMonths: 60
  };
  const schedule = buildStraightLineDepreciationSchedule(asset);
  const acquisition = buildAssetAcquisitionJournalDraft({ asset, companyId: "company-1", payableAccountId: "acc-ap" });
  const depreciation = buildAssetDepreciationJournalDraft({ asset, companyId: "company-1", line: schedule[0]! });
  const accounts = seedChartAccounts({ companyId: "company-1", chart: "skr03" });
  const balanceSheet = buildBalanceSheet({ accounts, entries: [acquisition, depreciation] });

  assert.equal(schedule[0]?.amount, 20);
  assert.equal(balanceSheet.rows.find((row) => row.account.id === "acc-fixed-assets")?.balance, 1200);
  assert.equal(balanceSheet.rows.find((row) => row.account.id === "acc-accumulated-depreciation")?.balance, -20);
  assert.equal(balanceSheet.assets, 1180);
});

test("Fixed asset disposal removes cost and accumulated depreciation from the balance sheet", () => {
  const asset = {
    accumulatedDepreciationAccountId: "acc-accumulated-depreciation",
    acquisitionAccountId: "acc-ap",
    acquisitionCost: 1200,
    acquisitionDate: "2026-01-15",
    assetAccountId: "acc-fixed-assets",
    currency: "EUR" as const,
    depreciationExpenseAccountId: "acc-depreciation",
    id: "asset-disposal-1",
    name: "Notebook",
    salvageValue: 0,
    usefulLifeMonths: 60
  };
  const acquisition = buildAssetAcquisitionJournalDraft({ asset, companyId: "company-1", payableAccountId: "acc-ap" });
  const depreciation = buildAssetDepreciationJournalDraft({ asset, companyId: "company-1", line: buildStraightLineDepreciationSchedule(asset)[0]! });
  const disposal = buildAssetDisposalJournalDraft({
    accumulatedDepreciation: 20,
    asset,
    companyId: "company-1",
    disposalDate: "2026-04-30",
    gainAccountId: "acc-revenue-saas",
    lossAccountId: "acc-depreciation",
    proceeds: 1000,
    proceedsAccountId: "acc-bank"
  });
  const accounts = seedChartAccounts({ companyId: "company-1", chart: "skr03" });
  const balanceSheet = buildBalanceSheet({ accounts, entries: [acquisition, depreciation, disposal] });

  assert.equal(disposal.lines.reduce((sum, line) => sum + line.debit.minor, 0), 120000);
  assert.equal(disposal.lines.reduce((sum, line) => sum + line.credit.minor, 0), 120000);
  assert.equal(balanceSheet.rows.find((row) => row.account.id === "acc-fixed-assets")?.balance ?? 0, 0);
  assert.equal(balanceSheet.rows.find((row) => row.account.id === "acc-accumulated-depreciation")?.balance ?? 0, 0);
  assert.equal(balanceSheet.assets, 1000);
  assert.equal(balanceSheet.balanced, true);
});

function accountBalance(rows: ReturnType<typeof buildTrialBalanceFromEntries>, accountId: string) {
  const row = rows.find((candidate) => candidate.account.id === accountId);
  assert.ok(row, `expected trial balance row for ${accountId}`);
  return row.balance;
}

function journalSideTotal(entries: ReturnType<typeof buildInvoiceJournalDraft>[], side: "credit" | "debit") {
  return entries.reduce((entrySum, entry) => entrySum + entry.lines.reduce((lineSum, line) => lineSum + moneyToMajor(line[side]), 0), 0);
}
