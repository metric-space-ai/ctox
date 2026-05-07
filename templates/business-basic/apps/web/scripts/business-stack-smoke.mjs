const baseUrl = process.env.CTOX_BUSINESS_BASE_URL ?? "http://localhost:3001";
const authUser = process.env.CTOX_BUSINESS_USER ?? "admin";
const authPassword = process.env.CTOX_BUSINESS_PASSWORD ?? "ctox-business";

const moduleRoutes = [
  ["/app?locale=en&theme=light", "Projects"],
  ["/app/sales?locale=en&theme=light", "Pipeline"],
  ["/app/sales/campaigns?locale=en&theme=light", "Campaigns"],
  ["/app/sales/pipeline?locale=en&theme=light", "Sales"],
  ["/app/sales/pipeline?locale=en&theme=light&panel=sales-set&recordId=open-pipeline&drawer=right", "Ask CTOX to process this set"],
  ["/app/sales/leads?locale=en&theme=light", "Access map"],
  ["/app/sales/offers?locale=en&theme=light", "Offers"],
  ["/app/sales/offers?locale=en&theme=light&panel=sales-set&recordId=offers-open&drawer=right", "Open offers"],
  ["/app/sales/customers?locale=en&theme=light", "Customer handoff"],
  ["/app/marketing?locale=en&theme=light", "Website pages"],
  ["/app/marketing/website?locale=en&theme=light", "Marketing"],
  ["/app/marketing/website?locale=en&theme=light&panel=marketing-set&recordId=draft-pages&drawer=right", "Ask CTOX to process this set"],
  ["/app/marketing/assets?locale=en&theme=light", "Marketing"],
  ["/app/marketing/assets?locale=en&theme=light&panel=marketing-set&recordId=assets-ready&drawer=right", "Selected items"],
  ["/app/marketing/competitive-analysis?locale=en&theme=light", "Marketing"],
  ["/app/marketing/research?locale=en&theme=light", "Marketing"],
  ["/app/marketing/research?locale=en&theme=light&panel=marketing-set&recordId=research-collecting&drawer=right", "Collecting research"],
  ["/app/marketing/commerce?locale=en&theme=light", "Marketing"],
  ["/app/marketing/commerce?locale=en&theme=light&panel=marketing-set&recordId=commerce-listed&drawer=right", "Listed offers"],
  ["/app/operations?locale=en&theme=light", "Projects"],
  ["/app/operations/projects?locale=en&theme=light", "Operations"],
  ["/app/operations/work-items?locale=en&theme=light", "Operations"],
  ["/app/operations/boards?locale=en&theme=light", "Operations"],
  ["/app/operations/planning?locale=en&theme=light", "Operations"],
  ["/app/operations/knowledge?locale=en&theme=light", "Operations"],
  ["/app/operations/meetings?locale=en&theme=light", "Operations"],
  ["/app/business?locale=en&theme=light", "Customers"],
  ["/app/business/customers?locale=en&theme=light", "Business"],
  ["/app/business/customers?locale=en&theme=light&panel=business-set&recordId=receivables&drawer=right", "Ask CTOX to process this set"],
  ["/app/business/bookkeeping?locale=en&theme=light&panel=business-set&recordId=tax-review&drawer=right", "Selected items"],
  ["/app/business/reports?locale=en&theme=light&panel=business-set&recordId=open-reports&drawer=right", "Open reports"],
  ["/app/business/products?locale=en&theme=light", "Business"],
  ["/app/business/warehouse?locale=en&theme=light", "Warehouse"],
  ["/app/business/warehouse?locale=en&theme=light&panel=business-set&recordId=warehouse-replay&drawer=right", "Replay"],
  ["/app/business/fulfillment?locale=en&theme=light", "Orders"],
  ["/app/business/fulfillment?locale=en&theme=light&panel=reservation&recordId=res-7001&drawer=right", "Order work"],
  ["/app/business/invoices?locale=en&theme=light", "Business"],
  ["/app/business/ledger?locale=en&theme=light", "Ledger"],
  ["/app/business/fixed-assets?locale=en&theme=light", "Assets"],
  ["/app/business/receipts?locale=en&theme=light", "Receipts"],
  ["/app/business/payments?locale=en&theme=light", "Payments"],
  ["/app/business/bookkeeping?locale=en&theme=light", "Business"],
  ["/app/business/reports?locale=en&theme=light", "Business"],
  ["/app/ctox?locale=en&theme=light", "Agent runs"],
  ["/app/ctox/runs?locale=en&theme=light", "CTOX"],
  ["/app/ctox/harness?locale=en&theme=light", "Harness flow"],
  ["/app/ctox/queue?locale=en&theme=light", "CTOX"],
  ["/app/ctox/knowledge?locale=en&theme=light", "CTOX"],
  ["/app/ctox/bugs?locale=en&theme=light", "CTOX"],
  ["/app/ctox/sync?locale=en&theme=light", "CTOX"],
  ["/app/ctox/settings?locale=en&theme=light", "Company name"]
];

const apiRoutes = [
  "/api/sales",
  "/api/sales/opportunities",
  "/api/sales/accounts",
  "/api/sales/contacts",
  "/api/sales/customers",
  "/api/sales/leads",
  "/api/sales/onboarding_projects",
  "/api/sales/offers",
  "/api/sales/tasks",
  "/api/marketing",
  "/api/marketing/website",
  "/api/marketing/assets",
  "/api/marketing/research",
  "/api/marketing/commerce",
  "/api/operations",
  "/api/operations/projects",
  "/api/operations/work-items",
  "/api/operations/knowledge",
  "/api/operations/meetings",
  "/api/business",
  "/api/business/accounts",
  "/api/business/bank-transactions",
  "/api/business/customers",
  "/api/business/products",
  "/api/business/warehouse",
  "/api/business/invoices",
  "/api/business/ledger",
  "/api/business/fixed-assets",
  "/api/business/receipts",
  "/api/business/bookkeeping",
  "/api/business/reports",
  "/api/business/accounting/reports",
  "/api/business/accounting/story-workflows?submodule=invoices",
  "/api/ctox/runs",
  "/api/ctox/queue",
  "/api/ctox/knowledge",
  "/api/ctox/bugs",
  "/api/ctox/sync",
  "/api/ctox/harness-flow",
  "/api/ctox/queue-tasks",
  "/api/ctox/bug-reports",
  "/api/ctox/events",
  "/api/ctox/navigation",
  "/api/ctox/locales",
  "/api/ctox/links?module=sales&submodule=pipeline&panel=opportunity&recordId=opp-northstar-pilot&drawer=right&locale=en&theme=light",
  "/api/ctox/links?module=marketing&submodule=website&panel=page&recordId=home&drawer=right&locale=en&theme=light",
  "/api/ctox/links?module=operations&submodule=work-items&panel=work-item&recordId=wp-1001&drawer=right&locale=en&theme=light",
  "/api/ctox/links?module=business&submodule=invoices&panel=invoice&recordId=inv-2026-001&drawer=right&locale=en&theme=light"
];

const publicResponse = await fetch(`${baseUrl}/?locale=en&theme=light`);
assert(publicResponse.ok, `/ returned ${publicResponse.status}`);
const publicHtml = await publicResponse.text();
assert(publicHtml.includes("Login"), "/ missing unauthenticated login link");
assert(!publicHtml.includes("Operational workspaces"), "/ leaked authenticated home content without a session cookie");

const authCookie = await getAuthCookie();

for (const [route, moduleName] of moduleRoutes) {
  const response = await fetch(`${baseUrl}${route}`, { headers: { cookie: authCookie } });
  assert(response.ok, `${route} returned ${response.status}`);
  const html = await response.text();
  assert(html.includes(moduleName), `${route} missing ${moduleName} shell`);
  assert(!html.includes("Current records, decisions, and queue-relevant work for this area."), `${route} still renders the generic fallback board`);
}

for (const route of apiRoutes) {
  const response = await fetch(`${baseUrl}${route}`, { headers: { cookie: authCookie } });
  assert(response.ok, `${route} returned ${response.status}`);
  const payload = await response.json();
  assert(payload.ok !== false, `${route} returned failed payload`);
}

await assertMutation({
  route: "/api/sales/opportunities",
  body: {
    action: "sync",
    recordId: "opp-northstar-pilot",
    title: "Sales smoke sync",
    instruction: "Smoke test Sales queue integration.",
    payload: { smoke: true }
  },
  deepLinkFragment: "/app/sales/pipeline"
});

await assertMutation({
  route: "/api/marketing/assets",
  body: {
    action: "sync",
    recordId: "intro-deck",
    title: "Marketing smoke sync",
    instruction: "Smoke test Marketing queue integration.",
    payload: { smoke: true }
  },
  deepLinkFragment: "/app/marketing/assets"
});

await assertMutation({
  route: "/api/operations/work-items",
  body: {
    action: "sync",
    recordId: "wp-1001",
    title: "Operations smoke sync",
    instruction: "Smoke test Operations queue integration.",
    payload: { smoke: true }
  },
  deepLinkFragment: "/app/operations/work-items"
});

await assertMutation({
  route: "/api/business/invoices",
  body: {
    action: "sync",
    recordId: "inv-2026-001",
    title: "Business smoke sync",
    instruction: "Smoke test Business queue integration.",
    payload: { smoke: true }
  },
  deepLinkFragment: "/app/business/invoices"
});

await assertMutation({
  route: "/api/business/warehouse",
  body: {
    action: "sync",
    recordId: "warehouse-replay",
    title: "Warehouse smoke sync",
    instruction: "Smoke test Warehouse queue integration.",
    payload: { smoke: true }
  },
  deepLinkFragment: "/app/business/warehouse"
});
await assertAccountingCommand({
  commandType: "PostDepreciation",
  route: "/api/business/fixed-assets",
  body: {
    action: "depreciate",
    recordId: "asset-macbook-2024",
    title: "Asset depreciation smoke",
    instruction: "Smoke test fixed asset depreciation workflow."
  }
});
await assertAccountingCommand({
  commandType: "DisposeAsset",
  route: "/api/business/fixed-assets",
  body: {
    action: "dispose",
    recordId: "asset-macbook-2024",
    title: "Asset disposal smoke",
    instruction: "Smoke test fixed asset disposal workflow."
  }
});
const capitalization = await assertAccountingCommand({
  commandType: "CapitalizeReceipt",
  route: "/api/business/receipts",
  body: {
    action: "capitalize",
    recordId: "rcpt-2026-018",
    title: "Receipt capitalization smoke",
    instruction: "Smoke test receipt-to-asset workflow."
  }
});
if (capitalization.accountingPersistence?.persisted !== false) {
  await assertAccountingCommand({
    commandType: "PostDepreciation",
    route: "/api/business/fixed-assets",
    body: {
      action: "depreciate",
      recordId: "asset-rcpt-2026-018",
      title: "Capitalized receipt depreciation smoke",
      instruction: "Smoke test depreciation for the receipt-created fixed asset."
    }
  });
}
await assertReceiptIngest();

await assertPeriodClose();
await assertPromptQueue();
await assertBugReportQueue();
await assertCoreEvent();
await assertRestLink({
  route: "/api/ctox/links/operations/work-items/wp-1001?locale=en&theme=light",
  expected: "/app/operations/work-items?recordId=wp-1001&panel=work-item&drawer=right&locale=en&theme=light"
});
await assertRestLink({
  route: "/api/ctox/links/business/invoices/inv-2026-001?locale=en&theme=light",
  expected: "/app/business/invoices?recordId=inv-2026-001&panel=invoice&drawer=right&locale=en&theme=light"
});
await assertRestLink({
  route: "/api/ctox/links/marketing/assets/intro-deck?locale=en&theme=light",
  expected: "/app/marketing/assets?recordId=intro-deck&panel=asset&drawer=right&locale=en&theme=light"
});
await assertBankImport();
await assertDatevExport();
await assertDunningRun();
await assertAccountingWorkflow();
await assertAccountingStoryWorkflows();
await assertAccountingReports();

console.log(`Business stack smoke passed against ${baseUrl}`);

async function getAuthCookie() {
  const response = await fetch(`${baseUrl}/api/auth/login?user=${encodeURIComponent(authUser)}&password=${encodeURIComponent(authPassword)}&next=/app`, {
    redirect: "manual"
  });
  assert(response.status === 307 || response.status === 302, `/api/auth/login returned ${response.status}`);
  const setCookie = response.headers.get("set-cookie");
  assert(setCookie?.includes("ctox_business_session"), "/api/auth/login did not set a session cookie");
  return setCookie.split(";")[0];
}

async function assertMutation({ route, body, deepLinkFragment }) {
  const response = await fetch(`${baseUrl}${route}`, {
    method: "POST",
    headers: { "content-type": "application/json", cookie: authCookie },
    body: JSON.stringify(body)
  });
  assert(response.ok, `POST ${route} returned ${response.status}`);
  const payload = await response.json();
  assert(payload.ok === true && payload.queued === true, `${route} did not queue`);
  assert(payload.mutation?.deepLink?.href?.includes(deepLinkFragment), `${route} missing ${deepLinkFragment} deep link`);
  assert(payload.core?.mode, `${route} missing CTOX core mode`);
}

async function assertAccountingCommand({ route, body, commandType }) {
  const response = await fetch(`${baseUrl}${route}`, {
    method: "POST",
    headers: { "content-type": "application/json", cookie: authCookie },
    body: JSON.stringify(body)
  });
  assert(response.ok, `POST ${route} returned ${response.status}`);
  const payload = await response.json();
  assert(payload.ok === true && payload.queued === true, `${route} did not queue accounting command`);
  assert(payload.accounting?.command?.type === commandType, `${route} returned ${payload.accounting?.command?.type}, expected ${commandType}`);
  assert(payload.accounting?.journalDraft?.lines?.length >= 2, `${route} did not return a balanced journal draft`);
  return payload;
}

async function assertReceiptIngest() {
  const response = await fetch(`${baseUrl}/api/business/receipts/rcpt-2026-018/ingest`, {
    body: JSON.stringify({
      mime: "text/plain",
      originalFilename: "receipt-smoke.txt",
      sha256: "sha256-smoke-receipt",
      sourceText: "Schneiderladen GmbH Rechnung 1190,00 EUR"
    }),
    headers: {
      "content-type": "application/json",
      cookie: authCookie
    },
    method: "POST"
  });
  assert(response.ok, `/api/business/receipts/[id]/ingest returned ${response.status}`);
  const payload = await response.json();
  assert(payload.command?.type === "IngestReceipt", "Receipt ingest missing IngestReceipt command");
  assert(payload.receiptProjection?.status === "extracted", "Receipt ingest did not move receipt projection to extracted");
  assert(payload.receiptProjection?.files?.[0]?.sha256 === "sha256-smoke-receipt", "Receipt ingest did not retain file hash");
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

async function assertPromptQueue() {
  const response = await fetch(`${baseUrl}/api/ctox/queue-tasks`, {
    method: "POST",
    headers: { "content-type": "application/json", cookie: authCookie },
    body: JSON.stringify({
      instruction: "Smoke test context prompt queue.",
      context: {
        source: "business-stack-smoke",
        items: [
          {
            moduleId: "operations",
            submoduleId: "projects",
            recordType: "project",
            recordId: "company-setup",
            label: "Company setup"
          }
        ]
      }
    })
  });
  assert(response.ok, "POST /api/ctox/queue-tasks failed");
  const payload = await response.json();
  assert(payload.ok === true && payload.queued === true, "/api/ctox/queue-tasks did not queue");
  assert(payload.core?.mode, "/api/ctox/queue-tasks missing CTOX core mode");
}

async function assertBugReportQueue() {
  const response = await fetch(`${baseUrl}/api/ctox/bug-reports`, {
    method: "POST",
    headers: { "content-type": "application/json", cookie: authCookie },
    body: JSON.stringify({
      summary: "Smoke test annotated bug report.",
      expected: "Bug report should queue into CTOX.",
      pageUrl: `${baseUrl}/app/operations/projects?locale=en&theme=light`,
      moduleId: "operations",
      submoduleId: "projects",
      annotation: { mode: "area", rect: { x: 10, y: 10, width: 100, height: 80 } }
    })
  });
  assert(response.ok, "POST /api/ctox/bug-reports failed");
  const payload = await response.json();
  assert(payload.ok === true && payload.accepted === true, "/api/ctox/bug-reports did not accept");
  assert(payload.core?.mode, "/api/ctox/bug-reports missing CTOX core mode");
}

async function assertCoreEvent() {
  const response = await fetch(`${baseUrl}/api/ctox/events`, {
    method: "POST",
    headers: { "content-type": "application/json", cookie: authCookie },
    body: JSON.stringify({
      type: "business.stack.smoke",
      module: "ctox",
      recordType: "stack",
      recordId: "business-basic",
      payload: { smoke: true }
    })
  });
  assert(response.ok, "POST /api/ctox/events failed");
  const payload = await response.json();
  assert(payload.ok === true && payload.accepted === true, "/api/ctox/events did not accept");
  assert(payload.core?.mode, "/api/ctox/events missing CTOX core mode");
}

async function assertRestLink({ route, expected }) {
  const response = await fetch(`${baseUrl}${route}`, { headers: { cookie: authCookie } });
  assert(response.ok, `${route} returned ${response.status}`);
  const payload = await response.json();
  assert(payload.ok === true, `${route} did not return an ok link`);
  assert(payload.link?.href === expected, `${route} returned ${payload.link?.href}, expected ${expected}`);
}

async function assertAccountingReports() {
  const response = await fetch(`${baseUrl}/api/business/accounting/reports`, { headers: { cookie: authCookie } });
  assert(response.ok, `/api/business/accounting/reports returned ${response.status}`);
  const payload = await response.json();
  assert(payload.balanceSheet?.balanced === true, "Accounting balance sheet is not balanced");
  assert(payload.balanceSheet?.difference === 0, `Accounting balance sheet difference is ${payload.balanceSheet?.difference}`);
  assert(payload.balanceSheet?.rows?.some((row) => row.account?.code === "0480" && row.balance > 0), "Accounting reports missing fixed asset account 0480");
  assert(payload.balanceSheet?.rows?.some((row) => row.account?.code === "0490" && row.balance < 0), "Accounting reports missing accumulated depreciation account 0490");
  assert(payload.profitAndLoss?.netIncome !== undefined, "Accounting reports missing P&L net income");
  assert(typeof payload.businessAnalysis?.ebit === "number", "Accounting reports missing BWA EBIT");
  assert(payload.businessAnalysis?.rows?.some((row) => row.bwaGroup === "depreciation"), "Accounting reports missing BWA depreciation grouping");
  assert(payload.vatStatement?.outputVat >= 0, "Accounting reports missing VAT output");
  assert(payload.vatStatement?.inputVat >= 0, "Accounting reports missing VAT input");
  assert(typeof payload.vatStatement?.netPosition === "number", "Accounting reports missing VAT net position");
  assert(payload.vatStatement?.boxes?.some((box) => box.code === "81" && box.amountKind === "base"), "Accounting reports missing UStVA box 81 base");
  assert(payload.vatStatement?.boxes?.some((box) => box.code === "86" && box.amountKind === "base"), "Accounting reports missing UStVA box 86 base");
  assert(payload.vatStatement?.boxes?.some((box) => box.code === "66" && box.amountKind === "tax"), "Accounting reports missing UStVA box 66 input tax");
  assert(payload.vatStatement?.boxes?.some((box) => box.code === "83" && box.amountKind === "settlement"), "Accounting reports missing UStVA box 83 settlement");
  assert(payload.vatStatement?.boxes?.some((box) => box.code === "RC" && box.amountKind === "base"), "Accounting reports missing reverse-charge/0% VAT base");
  assert(payload.fiscalPeriods?.nextClosablePeriod?.id || payload.fiscalPeriods?.closedCount >= 0, "Accounting reports missing fiscal period state");
  assert(payload.fixedAssets?.some((asset) => asset.bookValue > 0 && asset.accumulatedDepreciation > 0), "Accounting reports missing depreciated fixed asset register row");
  assert(Array.isArray(payload.datevExports) && payload.datevExports.length > 0, "Accounting reports missing DATEV export artifacts");
  assert(Array.isArray(payload.dunningRuns), "Accounting reports missing dunning run artifacts");
  assert(Array.isArray(payload.ledger) && payload.ledger.every((row) => row.entry?.status === "Posted"), "Accounting reports ledger includes non-posted entries");
  assert(Array.isArray(payload.datevPreview) && payload.datevPreview.every((line) => line.entry?.status === "Posted"), "Accounting reports DATEV preview includes non-posted entries");
  assert(!payload.ledger.some((row) => row.entry?.id === "je-bank-fee-2026-05" || row.entry?.id === "je-asset-depr-2026-001"), "Accounting reports ledger leaked known draft entries");
  assert(!payload.datevPreview.some((line) => line.entry?.id === "je-bank-fee-2026-05" || line.entry?.id === "je-asset-depr-2026-001"), "Accounting reports DATEV preview leaked known draft entries");
}

async function assertBankImport() {
  const response = await fetch(`${baseUrl}/api/business/accounting/bank-import`, {
    body: JSON.stringify({ format: "csv", sourceFilename: "smoke-bank.csv" }),
    headers: {
      "content-type": "application/json",
      cookie: authCookie
    },
    method: "POST"
  });
  assert(response.ok, `/api/business/accounting/bank-import returned ${response.status}`);
  const payload = await response.json();
  const statementId = payload.workflow?.statementExternalId;
  const matchedProposal = payload.workflow?.matchedProposal;
  assert(statementId?.startsWith("bank-statement-csv-"), "Bank import missing stable statement id");
  assert(matchedProposal?.proposedCommand?.type === "AcceptBankMatch", "Bank import missing bank match proposal");
  assert(matchedProposal?.proposedCommand?.payload?.matchedRecordId, "Bank import proposal missing matched record id");
  assert(matchedProposal?.proposedCommand?.payload?.matchType === "invoice", "Bank import proposal missing invoice match type");
  assert(matchedProposal?.refId?.startsWith(`${statementId}-line-`), `Bank match proposal ref ${matchedProposal?.refId} does not point to imported statement ${statementId}`);
}

async function assertDatevExport() {
  const response = await fetch(`${baseUrl}/api/business/accounting/datev-export?workflow=json`, { headers: { cookie: authCookie } });
  assert(response.ok, `/api/business/accounting/datev-export returned ${response.status}`);
  const payload = await response.json();
  assert(payload.filename?.endsWith("-datev.csv"), "DATEV export missing filename");
  assert(payload.csv?.includes("EXTF;700"), "DATEV export missing EXTF header");
  assert(payload.csv?.includes("DE_19_OUTPUT"), "DATEV export missing output VAT tax code");
  assert(payload.workflow?.proposal?.kind === "datev_export", "DATEV export missing workflow proposal");
  assert(payload.workflow?.datevExport?.lineCount > 0, "DATEV export workflow missing line count");
}

async function assertDunningRun() {
  const response = await fetch(`${baseUrl}/api/business/accounting/dunning`, {
    headers: { cookie: authCookie },
    method: "POST"
  });
  assert(response.ok, `/api/business/accounting/dunning returned ${response.status}`);
  const payload = await response.json();
  assert(Array.isArray(payload.workflow), "Dunning run missing workflow array");
  assert(payload.workflow.some((item) => item.audit?.action === "dunning.scan"), "Dunning run missing scan audit");
  assert(payload.proposals.every((item) => item.command?.type === "RunDunning"), "Dunning proposals contain unexpected command type");
}

async function assertAccountingWorkflow() {
  const response = await fetch(`${baseUrl}/api/business/accounting/workflow`, { headers: { cookie: authCookie } });
  assert(response.ok, `/api/business/accounting/workflow returned ${response.status}`);
  const payload = await response.json();
  const proposals = payload.proposals ?? [];
  const activation = proposals.find((proposal) => proposal.kind === "asset_activation");
  const depreciation = proposals.find((proposal) => proposal.kind === "asset_depreciation");
  assert(activation?.createdByAgent === "asset-accountant", "Workflow missing asset activation proposal from asset-accountant");
  assert(depreciation?.createdByAgent === "asset-accountant", "Workflow missing asset depreciation proposal from asset-accountant");
  assert(activation?.proposedCommand?.type === "CapitalizeReceipt", "Workflow asset activation proposal missing CapitalizeReceipt command");
  await assertProposalDecision({
    expectedResultingJournalEntryId: `je-manual-asset-asset-${activation.refId}`,
    id: activation.externalId ?? activation.id,
    proposedCommand: activation.proposedCommand
  });
}

async function assertAccountingStoryWorkflows() {
  const response = await fetch(`${baseUrl}/api/business/accounting/story-workflows`, {
    body: JSON.stringify({ locale: "en", storyId: "story-50" }),
    headers: {
      "content-type": "application/json",
      cookie: authCookie
    },
    method: "POST"
  });
  assert(response.ok, `/api/business/accounting/story-workflows returned ${response.status}`);
  const payload = await response.json();
  assert(payload.workflow?.story?.id === "story-50", "Story workflow missing selected story");
  assert(payload.workflow?.command?.type === "PrepareAccountingApprovalBatch", "Story workflow missing concrete approval batch command");
  assert(payload.workflow?.command?.payload?.implementationLevel === "direct", "Story workflow missing direct implementation marker");
  assert(payload.workflow?.proposal?.kind === "story_workflow", "Story workflow missing proposal");
  assert(payload.workflow?.outbox?.topic === "business.accounting.story-50.prepare", "Story workflow missing CTOX outbox event");
}

async function assertProposalDecision({ expectedResultingJournalEntryId, id, proposedCommand }) {
  assert(id, "Workflow proposal missing id");
  const response = await fetch(`${baseUrl}/api/business/accounting/workflow/proposals/${encodeURIComponent(id)}`, {
    body: JSON.stringify({ decision: "accept", proposedCommand }),
    headers: {
      "content-type": "application/json",
      cookie: authCookie
    },
    method: "POST"
  });
  assert(response.ok, `Proposal decision returned ${response.status}`);
  const payload = await response.json();
  assert(payload.proposal?.status === "accepted", "Proposal decision did not accept");
  assert(payload.proposal?.resultingJournalEntryId === expectedResultingJournalEntryId, `Proposal decision returned ${payload.proposal?.resultingJournalEntryId}, expected ${expectedResultingJournalEntryId}`);
}

async function assertPeriodClose() {
  const response = await fetch(`${baseUrl}/api/business/accounting/period-close`, {
    headers: {
      "content-type": "application/json",
      cookie: authCookie
    },
    method: "POST",
    body: JSON.stringify({ periodId: "fy-2026-04" })
  });
  assert(response.ok || response.status === 404, `/api/business/accounting/period-close returned ${response.status}`);
  const payload = await response.json();
  assert(payload.period?.status === "closed" || payload.error === "fiscal_period_not_found", "Period close did not return a closed period or setup hint");
}
