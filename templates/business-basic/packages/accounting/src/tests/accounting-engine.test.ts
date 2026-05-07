import assert from "node:assert/strict";
import test from "node:test";
import {
  bankMatchConfidence,
  buildAssetAcquisitionJournalDraft,
  buildAssetDepreciationJournalDraft,
  buildAssetDisposalJournalDraft,
  buildBankMatchJournalDraft,
  buildDunningProposals,
  buildDatevExtfCsv,
  buildDatevExtfLinesFromJournalDrafts,
  buildStraightLineDepreciationSchedule,
  buildBalanceSheet,
  buildBusinessAnalysis,
  buildGeneralLedger,
  buildTrialBalanceFromEntries,
  buildVatStatement,
  createSeriesState,
  findDuplicateBankLines,
  parseBankCsv,
  parseCamt053,
  parseMt940,
  prepareImportBankStatementCommand,
  buildInvoiceJournalDraft,
  buildProfitAndLoss,
  buildZugferdXml,
  buildReceiptJournalDraft,
  formatMoney,
  LedgerPosting,
  moneyFromMajor,
  moneyToMajor,
  prepareAcceptBankMatchCommand,
  preparePostReceiptCommand,
  prepareSendInvoiceCommand,
  validateInvoiceForSend,
  validateDatevExtf,
  validateZugferdXml,
  allocateNumber,
  assertPeriodOpen,
  type BusinessInvoiceLike,
  type InvoiceContext,
  closeFiscalPeriod,
  createParty,
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
  const tax = resolveGermanTaxRate({ taxRate: 19 });
  const party = createParty({
    defaultReceivableAccountId: "acc-ar",
    id: "cust-1",
    kind: "customer",
    name: "Kunstmen GmbH"
  });

  assert.equal(accounts.find((account) => account.code === "8400")?.externalId, "acc-revenue-saas");
  assert.equal(tax.accountId, "acc-vat-output");
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
