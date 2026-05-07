const baseUrl = process.env.CTOX_BUSINESS_BASE_URL ?? "http://localhost:3001";
const sessionCookie = await loginCookie();

const routes = [
  "/app/operations/projects?locale=en&theme=light",
  "/app/operations/projects?locale=en&theme=light&panel=operations-set&recordId=projects-at-risk&drawer=right",
  "/app/operations/projects?locale=en&theme=light&panel=operations-set&recordId=customer-projects&drawer=right",
  "/app/operations/work-items?locale=en&theme=light",
  "/app/operations/work-items?locale=en&theme=light&panel=work-set&recordId=open&drawer=right",
  "/app/operations/work-items?locale=en&theme=light&panel=work-set&recordId=status-ready&drawer=right",
  "/app/operations/work-items?locale=en&theme=light&panel=work-set&recordId=priority-urgent&drawer=right",
  "/app/operations/boards?locale=en&theme=light",
  "/app/operations/boards?locale=en&theme=light&panel=work-set&recordId=status-in-progress&drawer=right",
  "/app/operations/planning?locale=en&theme=light",
  "/app/operations/planning?locale=en&theme=light&panel=operations-set&recordId=milestones-at-risk&drawer=right",
  "/app/operations/planning?locale=en&theme=light&panel=operations-set&recordId=open-actions&drawer=right",
  "/app/operations/knowledge?locale=en&theme=light",
  "/app/operations/knowledge?locale=en&theme=light&panel=operations-set&recordId=knowledge&drawer=right",
  "/app/operations/knowledge?locale=en&theme=light&panel=operations-set&recordId=knowledge-linked&drawer=right",
  "/app/operations/knowledge?locale=en&theme=light&panel=operations-set&recordId=knowledge-ctox&drawer=right",
  "/app/operations/meetings?locale=en&theme=light",
  "/app/operations/meetings?locale=en&theme=light&panel=operations-set&recordId=meetings&drawer=right",
  "/app/operations/meetings?locale=en&theme=light&panel=operations-set&recordId=meetings-actions&drawer=right",
  "/app/operations/meetings?locale=en&theme=light&panel=operations-set&recordId=meetings-decisions&drawer=right"
];

const apiRoutes = [
  "/api/operations",
  "/api/operations/projects",
  "/api/operations/work-items",
  "/api/operations/knowledge",
  "/api/operations/documents",
  "/api/operations/document-templates",
  "/api/operations/meetings",
  "/api/ctox/links?module=operations&submodule=work-items&panel=work-item&recordId=wp-1001&drawer=right&locale=en&theme=light"
];

for (const route of routes) {
  const response = await fetch(`${baseUrl}${route}`, sessionCookie ? { headers: { cookie: sessionCookie } } : undefined);
  assert(response.ok, `${route} returned ${response.status}`);
  const html = await response.text();
  assert(html.includes("Operations"), `${route} missing Operations shell`);
}

for (const route of apiRoutes) {
  const response = await fetch(`${baseUrl}${route}`, { headers: { cookie: sessionCookie } });
  assert(response.ok, `${route} returned ${response.status}`);
  const payload = await response.json();
  assert(payload.ok !== false, `${route} returned failed payload`);
}

const mutation = await fetch(`${baseUrl}/api/operations/work-items`, {
  method: "POST",
  headers: { "content-type": "application/json", cookie: sessionCookie },
  body: JSON.stringify({
    action: "sync",
    recordId: "wp-1001",
    title: "Operations smoke sync",
    instruction: "Smoke test Operations queue integration.",
    payload: { smoke: true }
  })
});
assert(mutation.ok, `POST /api/operations/work-items returned ${mutation.status}`);
const mutationPayload = await mutation.json();
assert(mutationPayload.ok === true && mutationPayload.queued === true, "Operations mutation did not queue");
assert(mutationPayload.mutation?.deepLink?.href?.includes("/app/operations/work-items"), "Operations mutation missing work-item deep link");
assert(mutationPayload.core?.mode, "Operations mutation missing CTOX core mode");

const createMutation = await fetch(`${baseUrl}/api/operations/projects`, {
  method: "POST",
  headers: { "content-type": "application/json", cookie: sessionCookie },
  body: JSON.stringify({
    action: "create",
    title: "Operations smoke create project",
    payload: {
      subject: "Smoke-test project",
      owner: "operations-lead",
      due: "2026-05-30",
      details: "Created by Operations smoke test."
    }
  })
});
assert(createMutation.ok, `POST /api/operations/projects returned ${createMutation.status}`);
const createPayload = await createMutation.json();
assert(createPayload.ok === true && createPayload.queued === true, "Operations create mutation did not queue");

const exportMutation = await fetch(`${baseUrl}/api/operations/documents/export`, {
  method: "POST",
  headers: { "content-type": "application/json", cookie: sessionCookie },
  body: JSON.stringify({
    documentId: "doc-operating-model",
    format: "docx",
    locale: "en",
    includeCtoxContext: true
  })
});
assert(exportMutation.ok, `POST /api/operations/documents/export returned ${exportMutation.status}`);
const exportPayload = await exportMutation.json();
assert(exportPayload.ok === true && exportPayload.export?.mode === "queued-conversion", "Document DOCX export did not queue conversion");

const importMutation = await fetch(`${baseUrl}/api/operations/documents/import`, {
  method: "POST",
  headers: { "content-type": "application/json", cookie: sessionCookie },
  body: JSON.stringify({
    filename: "customer-proposal.docx",
    format: "docx",
    templateId: "tpl-proposal-standard",
    targetKnowledgeId: "kb-customer-onboarding"
  })
});
assert(importMutation.ok, `POST /api/operations/documents/import returned ${importMutation.status}`);
const importPayload = await importMutation.json();
assert(importPayload.ok === true && importPayload.import?.mode === "queued-conversion", "Document import did not queue conversion");

console.log(`Operations smoke passed against ${baseUrl}`);

async function loginCookie() {
  const user = process.env.CTOX_BUSINESS_USER ?? "admin";
  const password = process.env.CTOX_BUSINESS_PASSWORD ?? "ctox-business";
  const response = await fetch(`${baseUrl}/api/auth/login?user=${encodeURIComponent(user)}&password=${encodeURIComponent(password)}&next=/app`, {
    redirect: "manual"
  });
  const cookie = response.headers.get("set-cookie");
  return cookie?.split(";")[0] ?? "";
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
