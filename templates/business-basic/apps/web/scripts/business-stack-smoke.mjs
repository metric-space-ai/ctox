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
  ["/app/business/warehouse?locale=en&theme=light&panel=reservation&recordId=res-7001&drawer=right", "Implementation gates"],
  ["/app/business/invoices?locale=en&theme=light", "Business"],
  ["/app/business/ledger?locale=en&theme=light", "Ledger"],
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
  "/api/business/receipts",
  "/api/business/bookkeeping",
  "/api/business/reports",
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
