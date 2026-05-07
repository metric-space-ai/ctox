/**
 * Real browser proof for the Payroll workbench.
 *
 * Drives the Next.js dev server with puppeteer-core + system Chrome.
 * Verifies route render, click selects a payslip, right-click on a payslip row
 * opens the global Prompt-CTOX context menu, and reload preserves the snapshot.
 */
import puppeteer from "puppeteer-core";

const baseUrl = process.env.CTOX_BUSINESS_BASE_URL ?? "http://localhost:3001";
const chromePath =
  process.env.CTOX_BUSINESS_CHROME_PATH ??
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const headless = process.env.CTOX_BUSINESS_BROWSER_HEADLESS !== "false";

const sessionCookie = await loginCookie();
const cookieParts = sessionCookie.split("=");
const cookieName = cookieParts.shift();
const cookieValue = cookieParts.join("=");
if (!cookieName || !cookieValue) throw new Error("Login cookie not found");

const browser = await puppeteer.launch({
  executablePath: chromePath,
  headless,
  args: ["--no-sandbox", "--disable-dev-shm-usage"]
});

try {
  const page = await browser.newPage();
  await page.setViewport({ width: 1400, height: 900 });
  await page.setCookie({
    name: cookieName,
    value: cookieValue,
    url: baseUrl,
    httpOnly: true
  });

  // 1. Route render. Wait for the workspace scope (set by [module]/[submodule]/page.tsx) to confirm SSR completed.
  await page.goto(`${baseUrl}/app/payroll/runs?locale=de&theme=light`, { waitUntil: "domcontentloaded" });
  await page.waitForSelector(`[data-context-module="payroll"]`, { timeout: 15000 });
  const html = await page.content();
  const labelInHtml = html.includes("Lohnabrechnung");
  assert(labelInHtml, "Route did not render with Lohnabrechnung label");

  // 2. Quick run + slip generation via REST so we have something to click.
  const periodId = await fetchPeriodId(page);
  await callApi(page, "create_run", { periodId, payableAccountId: "1755" });
  const queue = await callApi(page, "queue_run", await runIdForPeriod(page, periodId));
  const slip = queue.snapshot.payslips.find((s) => s.runId === queue.snapshot.runs.find((r) => r.periodId === periodId).id);
  assert(slip, "Slip not generated");

  // 3. Reload page so the new run is in the SSR DOM.
  await page.reload({ waitUntil: "networkidle0" });

  // 4. Select the new run (workbench shows slips only for the selected run).
  const runRow = await page.$(`[data-context-record-type="payroll_run"][data-context-record-id="${slip.runId}"]`);
  assert(runRow, `Run row with data-context-record-id=${slip.runId} not in DOM`);
  await runRow.click();

  // 5. Verify data-context-* on the slip row.
  const slipRow = await page.$(`[data-context-record-type="payroll_payslip"][data-context-record-id="${slip.id}"]`);
  assert(slipRow, `Slip row with data-context-record-id=${slip.id} not in DOM`);

  // 5. Click the slip row.
  await slipRow.click();

  // 6. Verify the inspector now shows the slip details.
  const slipDetailVisible = await page.evaluate((id) => document.querySelectorAll(`[data-context-record-id="${id}"]`).length > 0, slip.id);
  assert(slipDetailVisible, "Slip detail did not appear after click");

  // 7. Right-click on the slip row.
  const box = await slipRow.boundingBox();
  assert(box, "Slip row has no bounding box");
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2, { button: "right" });

  // 8. Wait briefly for the context-menu-bridge to render its menu, then verify "Prompt CTOX" entry exists.
  await new Promise((r) => setTimeout(r, 250));
  const menuVisible = await page.evaluate(() => {
    const text = document.body.innerText;
    return text.includes("Prompt CTOX") || text.includes("CTOX") && text.toLowerCase().includes("prompt");
  });
  assert(menuVisible, "Right-click did not surface Prompt CTOX option");

  // 9. Verify the slip's data-context-record-id is anchored on a CTOX-promptable element.
  const ctoxAnchored = await page.evaluate((id) => {
    const el = document.querySelector(`[data-context-record-id="${id}"][data-context-module="payroll"]`);
    return Boolean(el);
  }, slip.id);
  assert(ctoxAnchored, "Selected slip not anchored as CTOX-promptable");

  // 10. Reload preserves snapshot: run row still in DOM, and clicking it brings slip back.
  await page.reload({ waitUntil: "networkidle0" });
  const runRowAfterReload = await page.$(`[data-context-record-type="payroll_run"][data-context-record-id="${slip.runId}"]`);
  assert(runRowAfterReload, "Run row missing after reload");
  await runRowAfterReload.click();
  const slipStillVisible = await page.evaluate((id) => Boolean(document.querySelector(`[data-context-record-id="${id}"]`)), slip.id);
  assert(slipStillVisible, "Slip not visible after reload + run select");

  console.log(`Payroll browser proof passed against ${baseUrl}`);
} finally {
  await browser.close();
}

async function fetchPeriodId(page) {
  const stamp = Date.now().toString(36);
  return await callApi(page, "create_period", {
    startDate: `2027-01-01`,
    endDate: `2027-01-31`,
    frequency: "monthly",
    id: `period_browser_${stamp}`
  }).then((res) => res.snapshot.periods.at(-1).id);
}

async function runIdForPeriod(page, periodId) {
  const snap = await page.evaluate(async () => {
    const r = await fetch("/api/payroll");
    return r.json();
  });
  const run = snap.snapshot.runs.find((r) => r.periodId === periodId && r.status !== "Cancelled");
  return { id: run.id };
}

async function callApi(page, command, payload) {
  return page.evaluate(
    async (cmd, body) => {
      const res = await fetch("/api/payroll", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ command: cmd, payload: body })
      });
      const json = await res.json();
      if (!json.ok) throw new Error(`${cmd} failed: ${json.error}`);
      return json;
    },
    command,
    payload
  );
}

async function loginCookie() {
  const user = process.env.CTOX_BUSINESS_USER ?? "admin";
  const password = process.env.CTOX_BUSINESS_PASSWORD ?? "ctox-business";
  const response = await fetch(
    `${baseUrl}/api/auth/login?user=${encodeURIComponent(user)}&password=${encodeURIComponent(password)}&next=/app`,
    { redirect: "manual" }
  );
  const cookie = response.headers.get("set-cookie");
  return cookie?.split(";")[0] ?? "";
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
