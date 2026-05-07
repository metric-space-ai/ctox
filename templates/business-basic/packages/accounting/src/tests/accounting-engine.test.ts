import assert from "node:assert/strict";
import test from "node:test";
import {
  bankMatchConfidence,
  buildBankMatchJournalDraft,
  buildDunningProposals,
  buildDatevExtfCsv,
  buildBalanceSheet,
  buildGeneralLedger,
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
    notes: "Gemäß § 19 UStG wird keine Umsatzsteuer berechnet."
  }, invoiceContext);
  const reverseCharge = validateInvoiceForSend({
    ...invoice,
    reverseCharge: true
  }, invoiceContext);

  assert.match(ku.errors.join(","), /kleinunternehmer_invoice_must_not_have_tax/);
  assert.match(reverseCharge.errors.join(","), /reverse_charge_invoice_must_not_have_tax/);
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
  const csv = buildDatevExtfCsv([
    {
      accountCode: "8400",
      amount: 297.5,
      contraAccountCode: "1200",
      currency: "EUR",
      date: "2026-05-07",
      documentNumber: "RE-2026-001",
      side: "H",
      taxCode: "19",
      text: "SaaS; Support"
    }
  ], {
    accountLength: 4,
    clientNumber: "67890",
    consultantNumber: "12345",
    fiscalYearStart: "20260101"
  });

  assert.match(csv.split("\n")[0] ?? "", /EXTF;700/);
  assert.match(csv, /297,50/);
  assert.match(csv, /"SaaS; Support"/);
});

test("ZUGFeRD XML includes buyer, totals, due date and tax category", () => {
  const xml = buildZugferdXml(invoice, invoiceContext);

  assert.match(xml, /CrossIndustryInvoice/);
  assert.match(xml, /Kunstmen GmbH/);
  assert.match(xml, /GrandTotalAmount>297\.50/);
  assert.match(xml, /DueDateDateTime/);
  assert.match(xml, /CategoryCode>S/);
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
  assert.equal(assertPeriodOpen([open], "2026-05-07").id, "fy-2026");
  assert.throws(() => assertPeriodOpen([closeFiscalPeriod(open)], "2026-05-07"), /fiscal_period_closed/);
});

test("Reports derive GL, P&L and balance sheet from journal drafts", () => {
  const journal = buildInvoiceJournalDraft(invoice, invoiceContext);
  const accounts = seedChartAccounts({ companyId: "company-1", chart: "skr03" });
  const ledger = buildGeneralLedger({ accountId: "acc-ar", entries: [journal] });
  const pnl = buildProfitAndLoss({ accounts, entries: [journal] });
  const balanceSheet = buildBalanceSheet({ accounts, entries: [journal] });

  assert.equal(ledger[0]?.runningBalance, 297.5);
  assert.equal(pnl.income, 250);
  assert.equal(balanceSheet.assets, 297.5);
  assert.equal(balanceSheet.liabilities, 47.5);
});
