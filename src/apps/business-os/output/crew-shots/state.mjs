import { chromium } from 'playwright';
const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
const page = browser.contexts()[0].pages().find((p) => p.url().startsWith('http://127.0.0.1:8765'));
const r = await page.evaluate(async () => {
  const app = window.CTOX_BUSINESS_OS_APP; const h = [...document.querySelectorAll('*')].find((e) => e.__ctoxState); const st = h?.__ctoxState;
  const d = app.sync.diagnostics; const c = d.collections || {};
  const errs = Object.values(c).filter((e) => e.lastError || e.status === 'error').map((e) => `${e.collection}: ${e.status} ${String(e.lastError?.message || e.lastError || e.reason || '').slice(0, 90)}`);
  const t = (p) => Promise.race([p, new Promise((r) => setTimeout(() => r(null), 3000))]);
  const n = async (x) => { const col = app?.db?.raw?.[x]; if (!col) return -1; const r = await t(col.find({ limit: 300 }).exec()); return Array.isArray(r) ? r.length : -2; };
  return { role: app.session?.role, user: app.session?.user?.name || app.session?.username, dataError: st?.dataError, crew: await n('ctox_crew_members'), tasks: await n('ctox_queue_tasks'), status: await n('ctox_harness_status'), events: await n('ctox_harness_events'), runs: await n('ctox_runs'), errs: errs.slice(0, 12), members: st?.crewMembers?.length, modelTasks: st?.model?.tasks?.length };
});
console.log(JSON.stringify(r, null, 1));
await browser.close();
