import { chromium } from 'playwright';
const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
const page = browser.contexts()[0].pages().find((p) => p.url().startsWith('http://127.0.0.1:8765'));
const r = await page.evaluate(() => {
  const app = window.CTOX_BUSINESS_OS_APP; const d = app.sync.diagnostics; const c = d.collections || {};
  const by = {}; for (const e of Object.values(c)) { const k = `${e.status}/${e.initialReplicationState}/${e.frameTransport?.collectionReadinessState}`; by[k] = (by[k] || 0) + 1; }
  const tr = c.ctox_bug_reports?.frameTransport || {};
  return { phase: d.phase, updatedAt: d.updatedAt, warn: app.recoveryWarning, dpr: app.dataPlaneReadyStatus, reason: app.dataPlaneReadyReason, by, peerKeys: Object.keys(d).filter(k => /peer|ice|channel|connection/i.test(k)).map(k => k + '=' + JSON.stringify(d[k]).slice(0, 80)), bug: { irs: c.ctox_bug_reports?.initialReplicationState, sent: tr.sentFrames, recv: tr.receivedFrames, apc: tr.activePeerCount, err: c.ctox_bug_reports?.lastError } };
});
console.log(JSON.stringify(r, null, 1).slice(0, 2500));
await browser.close();
