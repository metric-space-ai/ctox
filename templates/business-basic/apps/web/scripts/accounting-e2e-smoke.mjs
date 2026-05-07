const baseUrl = process.env.CTOX_BUSINESS_BASE_URL ?? "http://localhost:3001";
const authUser = process.env.CTOX_BUSINESS_USER ?? "admin";
const authPassword = process.env.CTOX_BUSINESS_PASSWORD ?? "ctox-business";

const authCookie = await getAuthCookie();

await assertAccountingSetup();
await assertInvoiceDocumentFlow();
await assertOutgoingInvoicePosting();
await assertReceiptPostingAndIngest();
await assertBankReconciliationFlow();
await assertFixedAssetLifecycle();
await assertDatevAndDunningFlow();
await assertDerivedReports();

console.log(`Accounting E2E smoke passed against ${baseUrl}`);

async function getAuthCookie() {
  const response = await fetch(`${baseUrl}/api/auth/login?user=${encodeURIComponent(authUser)}&password=${encodeURIComponent(authPassword)}&next=/app`, {
    redirect: "manual"
  });
  assert(response.status === 307 || response.status === 302, `/api/auth/login returned ${response.status}`);
  const setCookie = response.headers.get("set-cookie");
  assert(setCookie?.includes("ctox_business_session"), "/api/auth/login did not set a session cookie");
  return setCookie.split(";")[0];
}

async function assertAccountingSetup() {
  const response = await fetch(`${baseUrl}/api/business/accounting/setup`, {
    headers: { cookie: authCookie },
    method: "POST"
  });
  assert(response.ok, `/api/business/accounting/setup returned ${response.status}`);
  const payload = await response.json();
  assert(payload.snapshot?.accounts?.some((account) => account.code === "1771" && account.accountType === "tax"), "Accounting setup missing SKR03 output VAT 7% account 1771");
  assert(payload.snapshot?.accounts?.some((account) => account.code === "1571" && account.accountType === "tax"), "Accounting setup missing SKR03 input VAT 7% account 1571");
  assert(payload.snapshot?.taxRates?.some((taxRate) => taxRate.code === "DE_7" && taxRate.accountId === "acc-vat-output-7"), "Accounting setup missing DE_7 tax rate account mapping");
  assert(payload.workflow?.audit?.action === "accounting.setup.prepare", "Accounting setup missing workflow audit");
}

async function assertInvoiceDocumentFlow() {
  const zugferdResponse = await fetch(`${baseUrl}/api/business/invoices/inv-2026-001/zugferd?locale=de`, {
    headers: { cookie: authCookie }
  });
  assert(zugferdResponse.ok, `/api/business/invoices/[id]/zugferd returned ${zugferdResponse.status}`);
  const xml = await zugferdResponse.text();
  assert(xml.includes("CrossIndustryInvoice"), "ZUGFeRD XML missing CrossIndustryInvoice root");
  assert(xml.includes("RE-2026-001"), "ZUGFeRD XML missing invoice number");
  assert(xml.includes("Metric Space UG"), "ZUGFeRD XML missing seller context");

  const pdfResponse = await fetch(`${baseUrl}/api/business/invoices/inv-2026-001/pdf?locale=de`, {
    headers: { cookie: authCookie }
  });
  assert(pdfResponse.ok, `/api/business/invoices/[id]/pdf returned ${pdfResponse.status}`);
  assert(pdfResponse.headers.get("content-type")?.includes("application/pdf"), "Invoice PDF missing application/pdf content type");
  const pdfBytes = new Uint8Array(await pdfResponse.arrayBuffer());
  const header = new TextDecoder().decode(pdfBytes.slice(0, 8));
  assert(header.startsWith("%PDF-"), "Invoice PDF does not start with a PDF header");
  assert(pdfBytes.length > 1500, "Invoice PDF is unexpectedly small");
}

async function assertOutgoingInvoicePosting() {
  const payload = await postBusinessMutation("/api/business/invoices", {
    action: "send",
    instruction: "Accounting E2E: send and post invoice.",
    recordId: "inv-2026-001",
    title: "Accounting E2E invoice send"
  });
  assert(payload.accounting?.command?.type === "SendInvoice", "Invoice send missing SendInvoice command");
  assert(payload.accounting?.journalDraft?.type === "invoice", "Invoice send missing invoice journal draft");
  assertBalancedJournal(payload.accounting.journalDraft, "invoice send journal");
  assert(payload.accounting?.invoiceProjection?.postedJournalEntryExternalId, "Invoice projection missing posted journal entry reference");
  assert(payload.accounting?.zugferdXml?.includes("RE-2026-001"), "Invoice send missing ZUGFeRD XML in accounting payload");
  assert(payload.accounting?.journalDraft?.lines?.some((line) => line.taxCode === "DE_19_OUTPUT" && line.credit?.minor > 0), "Invoice journal missing DE_19_OUTPUT tagged lines");
}

async function assertReceiptPostingAndIngest() {
  const ingestResponse = await fetch(`${baseUrl}/api/business/receipts/rcpt-2026-018/ingest`, {
    body: JSON.stringify({
      mime: "text/plain",
      originalFilename: "accounting-e2e-receipt.txt",
      sha256: "sha256-accounting-e2e-receipt",
      sourceText: "Independent Design Partner invoice 928.20 EUR VAT 148.20"
    }),
    headers: {
      "content-type": "application/json",
      cookie: authCookie
    },
    method: "POST"
  });
  assert(ingestResponse.ok, `/api/business/receipts/[id]/ingest returned ${ingestResponse.status}`);
  const ingest = await ingestResponse.json();
  assert(ingest.command?.type === "IngestReceipt", "Receipt ingest missing IngestReceipt command");
  assert(ingest.receiptProjection?.files?.[0]?.sha256 === "sha256-accounting-e2e-receipt", "Receipt ingest missing retained file hash");

  const payload = await postBusinessMutation("/api/business/receipts", {
    action: "post",
    instruction: "Accounting E2E: post reviewed inbound receipt.",
    recordId: "rcpt-2026-018",
    title: "Accounting E2E receipt post"
  });
  assert(payload.accounting?.command?.type === "PostReceipt", "Receipt post missing PostReceipt command");
  assert(payload.accounting?.journalDraft?.type === "receipt", "Receipt post missing receipt journal draft");
  assertBalancedJournal(payload.accounting.journalDraft, "receipt post journal");
  assert(payload.accounting?.journalDraft?.lines?.some((line) => line.accountId === "acc-vat-input" && line.taxCode === "DE_19_INPUT"), "Receipt journal missing input VAT line");
}

async function assertBankReconciliationFlow() {
  const importResponse = await fetch(`${baseUrl}/api/business/accounting/bank-import`, {
    body: JSON.stringify({ format: "csv", sourceFilename: "accounting-e2e-bank.csv" }),
    headers: {
      "content-type": "application/json",
      cookie: authCookie
    },
    method: "POST"
  });
  assert(importResponse.ok, `/api/business/accounting/bank-import returned ${importResponse.status}`);
  const imported = await importResponse.json();
  assert(imported.workflow?.matchedProposal?.proposedCommand?.type === "AcceptBankMatch", "Bank import missing AcceptBankMatch proposal");
  if (imported.persisted === false) return;
  const bankTransactionId = imported.workflow.matchedProposal.proposedCommand.payload?.bankTransactionId ?? imported.workflow.matchedProposal.refId;
  assert(bankTransactionId, "Bank import missing matched bank transaction id");

  const payload = await postBusinessMutation("/api/business/payments", {
    action: "match",
    instruction: "Accounting E2E: accept bank match.",
    recordId: bankTransactionId,
    title: "Accounting E2E bank match"
  });
  assert(payload.accounting?.command?.type === "AcceptBankMatch", "Payment match missing AcceptBankMatch command");
  assertBalancedJournal(payload.accounting.journalDraft, "bank match journal");
  assert(payload.accounting?.paymentProjection?.allocations?.length > 0, "Payment match missing allocations");
}

async function assertFixedAssetLifecycle() {
  const depreciation = await postBusinessMutation("/api/business/fixed-assets", {
    action: "depreciate",
    instruction: "Accounting E2E: post annual depreciation.",
    recordId: "asset-macbook-2024",
    title: "Accounting E2E asset depreciation"
  });
  assert(depreciation.accounting?.command?.type === "PostDepreciation", "Asset depreciation missing PostDepreciation command");
  assertBalancedJournal(depreciation.accounting.journalDraft, "asset depreciation journal");
  assert(depreciation.accounting?.journalDraft?.lines?.some((line) => line.accountId === "acc-depreciation" && line.debit?.minor > 0), "Asset depreciation missing expense debit");

  const disposal = await postBusinessMutation("/api/business/fixed-assets", {
    action: "dispose",
    instruction: "Accounting E2E: prepare asset disposal.",
    recordId: "asset-macbook-2024",
    title: "Accounting E2E asset disposal"
  });
  assert(disposal.accounting?.command?.type === "DisposeAsset", "Asset disposal missing DisposeAsset command");
  assertBalancedJournal(disposal.accounting.journalDraft, "asset disposal journal");
  assert(disposal.accounting?.journalDraft?.lines?.some((line) => line.accountId === "acc-fixed-assets" && line.credit?.minor > 0), "Asset disposal missing fixed asset credit");
}

async function assertDatevAndDunningFlow() {
  const datevResponse = await fetch(`${baseUrl}/api/business/accounting/datev-export?workflow=json`, {
    headers: { cookie: authCookie }
  });
  assert(datevResponse.ok, `/api/business/accounting/datev-export returned ${datevResponse.status}`);
  const datev = await datevResponse.json();
  assert(datev.csv?.includes("EXTF;700"), "DATEV export missing EXTF header");
  assert(datev.csv?.includes("DE_19_OUTPUT"), "DATEV export missing output VAT tax code");
  assert(datev.workflow?.datevExport?.csvSha256, "DATEV export workflow missing CSV artifact hash");
  assert(datev.workflow?.datevExport?.lineCount > 0, "DATEV export workflow missing artifact line count");
  assert(["exported", "prepared"].includes(datev.workflow?.datevExport?.status), "DATEV export workflow missing prepared/exported artifact status");

  const dunningResponse = await fetch(`${baseUrl}/api/business/accounting/dunning`, {
    headers: { cookie: authCookie },
    method: "POST"
  });
  assert(dunningResponse.ok, `/api/business/accounting/dunning returned ${dunningResponse.status}`);
  const dunning = await dunningResponse.json();
  assert(Array.isArray(dunning.workflow) && dunning.workflow.some((item) => item.audit?.action === "dunning.scan"), "Dunning run missing scan workflow");
  if ((dunning.proposals?.length ?? 0) > 0) {
    assert(dunning.proposals.every((proposal) => proposal.command?.type === "RunDunning"), "Dunning run returned unexpected command types");
  }
}

async function assertDerivedReports() {
  const response = await fetch(`${baseUrl}/api/business/accounting/reports`, {
    headers: { cookie: authCookie }
  });
  assert(response.ok, `/api/business/accounting/reports returned ${response.status}`);
  const payload = await response.json();
  assert(payload.balanceSheet?.balanced === true, "Balance sheet is not balanced");
  assert(payload.balanceSheet?.rows?.some((row) => row.account?.code === "0480"), "Balance sheet missing fixed asset account");
  assert(payload.balanceSheet?.rows?.some((row) => row.account?.code === "0490"), "Balance sheet missing accumulated depreciation account");
  assert(typeof payload.profitAndLoss?.netIncome === "number", "P&L missing net income");
  assert(payload.businessAnalysis?.rows?.some((row) => row.bwaGroup === "depreciation"), "BWA missing depreciation grouping");
  assert(payload.vatStatement?.boxes?.some((box) => box.code === "81" && box.amountKind === "base"), "UStVA missing Kz 81");
  assert(payload.vatStatement?.boxes?.some((box) => box.code === "86" && box.amountKind === "base"), "UStVA missing Kz 86 structure");
  assert(payload.vatStatement?.boxes?.some((box) => box.code === "66" && box.amountKind === "tax"), "UStVA missing Kz 66");
  assert(payload.vatStatement?.boxes?.some((box) => box.code === "83" && box.amountKind === "settlement"), "UStVA missing Kz 83");
  assert(payload.fixedAssets?.some((asset) => asset.id === "asset-macbook-2024" && asset.bookValue >= 0), "Fixed asset register missing MacBook asset");
  assert(Array.isArray(payload.datevExports), "Reports missing DATEV export artifacts");
  assert(Array.isArray(payload.dunningRuns), "Reports missing dunning artifacts");
  assert(Array.isArray(payload.ledger) && payload.ledger.every((row) => row.entry?.status === "Posted"), "Ledger report includes non-posted journal entries");
  assert(Array.isArray(payload.datevPreview) && payload.datevPreview.every((line) => line.entry?.status === "Posted"), "DATEV preview includes non-posted journal entries");
  assert(!payload.ledger.some((row) => row.entry?.id === "je-bank-fee-2026-05" || row.entry?.id === "je-asset-depr-2026-001"), "Ledger report leaked known draft entries");
  assert(!payload.datevPreview.some((line) => line.entry?.id === "je-bank-fee-2026-05" || line.entry?.id === "je-asset-depr-2026-001"), "DATEV preview leaked known draft entries");
}

async function postBusinessMutation(route, body) {
  const response = await fetch(`${baseUrl}${route}`, {
    body: JSON.stringify(body),
    headers: {
      "content-type": "application/json",
      cookie: authCookie
    },
    method: "POST"
  });
  assert(response.ok, `POST ${route} returned ${response.status}`);
  const payload = await response.json();
  assert(payload.ok === true && payload.queued === true, `${route} did not queue`);
  return payload;
}

function assertBalancedJournal(journal, label) {
  assert(journal?.lines?.length >= 2, `${label} missing journal lines`);
  const debit = journal.lines.reduce((sum, line) => sum + (line.debit?.minor ?? 0), 0);
  const credit = journal.lines.reduce((sum, line) => sum + (line.credit?.minor ?? 0), 0);
  assert(debit === credit, `${label} is unbalanced: debit ${debit}, credit ${credit}`);
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
