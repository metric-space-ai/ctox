import { chromium } from 'playwright';
const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
const page = browser.contexts()[0].pages().find((p) => p.url().startsWith('http://127.0.0.1:8765'));
const r = await page.evaluate(async () => {
  const app = window.CTOX_BUSINESS_OS_APP;
  const s = app.session || {};
  const boot = await fetch('/api/business-os/bootstrap').then((r) => r.status + ' ' + r.headers.get('content-type')).catch((e) => 'ERR ' + e.message);
  const auth = await fetch('/api/business-os/auth/session').then(async (r) => r.status + ' ' + (await r.text()).slice(0, 300)).catch((e) => 'ERR ' + e.message);
  return { sessionKeys: Object.keys(s).slice(0, 20), authenticated: s.authenticated, role: s.role, user: JSON.stringify(s.user || s.actor || null).slice(0, 200), boot, auth, cookies: document.cookie.slice(0, 120) };
});
console.log(JSON.stringify(r, null, 1));
await browser.close();
