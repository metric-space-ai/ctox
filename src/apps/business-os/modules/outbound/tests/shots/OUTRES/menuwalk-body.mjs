// ---------- OUTRES: Menü-Rundgang outbound (echte Maus + Tastatur im echten v4-Rahmen) ----------
const MODE = opt('--mode', 'walk');
const report = [];
const log = (...a) => { console.log(...a); };

async function openApp(browser, base, id, w) {
  const ctx = await browser.newContext({ viewport: { width: w, height: vh(w) }, deviceScaleFactor: 1, locale: 'de-DE', acceptDownloads: true });
  const page = await ctx.newPage();
  const errors = [];
  const downloads = [];
  page.on('pageerror', (e) => errors.push(`pageerror: ${String(e.message || e).slice(0, 220)}`));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(`console: ${m.text().slice(0, 220)}`); });
  page.on('download', (d) => downloads.push(d.suggestedFilename()));
  await page.goto(`${base}/__rg/page.html?app=${encodeURIComponent(id)}`);
  await page.waitForFunction(() => window.__rgReady === true, null, { timeout: 30000 });
  await page.waitForTimeout(300);
  const tile = page.locator(`[data-v4-desk-app="${id}"]`).first();
  await tile.scrollIntoViewIfNeeded();
  const tb = await tile.boundingBox();
  await realClick(page, tb.x + tb.width / 2, tb.y + tb.height / 2);
  await page.waitForSelector('.v4-app-root', { timeout: 15000 });
  await page.waitForTimeout(900);
  if (w > 480) {
    for (const sel of ['[data-window-layout-control="toggle"]', '[data-window-layout-control="maximize"]']) {
      const b = await page.locator(sel).first().boundingBox({ timeout: 2000 }).catch(() => null);
      if (b) { await realClick(page, b.x + b.width / 2, b.y + b.height / 2); await page.waitForTimeout(350); }
    }
    await page.waitForTimeout(500);
  }
  return { ctx, page, errors, downloads };
}

const R = '.v4-app-root';
async function box(page, sel) { return page.locator(sel).first().boundingBox({ timeout: 2500 }).catch(() => null); }
async function mouseClick(page, sel) {
  const loc = page.locator(sel).first();
  await loc.scrollIntoViewIfNeeded({ timeout: 2500 }).catch(() => {});
  const b = await box(page, sel);
  if (!b) return false;
  await realClick(page, b.x + b.width / 2, b.y + b.height / 2);
  return true;
}
const ops = (page) => page.evaluate(() => window.__rg.ops.map((o) => `${o.m} ${o.coll}${o.id ? '/' + o.id : ''} ${o.body.slice(0, 90)}`));
const menuItems = (page) => page.evaluate(() => [...document.querySelectorAll('[data-v4-menu] [data-v4-menu-item]')].map((b) => ({ id: b.dataset.v4MenuItem, label: b.textContent.trim(), disabled: b.getAttribute('aria-disabled') === 'true' })));
const menuOpen = (page) => page.evaluate(() => !!document.querySelector('[data-v4-menu]'));
const sheetOpen = (page) => page.evaluate(() => { const s = document.querySelector('[data-v4-sheet]'); return s ? s.innerText.replace(/\s+/g, ' ').slice(0, 160) : null; });
const active = (page) => page.evaluate(() => { const a = document.activeElement; return a ? `${a.tagName.toLowerCase()}[${a.dataset.action || a.dataset.v4MenuItem || ''}] ${(a.getAttribute('aria-label') || a.textContent || '').trim().slice(0, 40)}` : ''; });
const rowIds = (page) => page.evaluate((r) => [...new Set([...document.querySelectorAll(`${r} tr[data-company-id]`)].map((tr) => tr.dataset.companyId))], R);
const rowVisible = (page, cid) => page.evaluate(({ r, cid }) => [...document.querySelectorAll(`${r} tr[data-company-id="${cid}"] .outbound-cell-actions button`)].filter((b) => b.offsetParent).map((b) => (b.getAttribute('aria-label') || b.textContent).trim()), { r: R, cid });
const campaignVisible = (page, cid) => page.evaluate(({ r, cid }) => [...document.querySelectorAll(`${r} .outbound-campaign-item[data-campaign-id="${cid}"] .outbound-campaign-actions button`)].filter((b) => b.offsetParent).map((b) => (b.getAttribute('aria-label') || b.textContent).trim()), { r: R, cid });
const campaignIds = (page) => page.evaluate((r) => [...document.querySelectorAll(`${r} .outbound-campaign-item[data-campaign-id]`)].map((a) => ({ id: a.dataset.campaignId, name: a.querySelector('strong')?.textContent.trim(), selected: a.classList.contains('is-selected') })), R);

async function toTable(page, w, shot, dir) {
  if (w <= 480) {
    const sel = `${R} .outbound-campaign-item.is-selected [data-action="select-campaign"]`;
    await mouseClick(page, sel);
    await page.waitForTimeout(900);
  }
  await page.locator(`${R} tr[data-company-id]`).first().scrollIntoViewIfNeeded({ timeout: 4000 }).catch(() => {});
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(dir, `${w}-${shot}.png`) });
}

async function scenario(browser, base, w, name, fn) {
  const dir = join(OUT, 'outbound');
  mkdirSync(dir, { recursive: true });
  const env = await openApp(browser, base, 'outbound', w);
  const entry = { width: w, scenario: name, steps: [] };
  try { await fn({ ...env, dir, entry }); } catch (e) { entry.steps.push({ ABBRUCH: String(e.message || e).split('\n')[0] }); }
  entry.errors = env.errors.splice(0);
  report.push(entry);
  await env.ctx.close();
}

// Ein Menüeintrag per echter Maus: „…“ klicken, Eintrag klicken, Wirkung messen.
async function viaMenu(page, env, moreSel, itemId) {
  const o0 = (await ops(page)).length;
  const d0 = env.downloads.length;
  if (!(await mouseClick(page, moreSel))) return { itemId, error: '„…“ nicht gefunden' };
  await page.waitForSelector('[data-v4-menu]', { timeout: 3000 });
  const items = await menuItems(page);
  const hit = items.find((i) => i.id === itemId);
  if (!hit) { await page.keyboard.press('Escape'); return { itemId, error: `Eintrag fehlt, Menü: ${items.map((i) => i.id).join(',')}` }; }
  if (hit.disabled) { await page.keyboard.press('Escape'); return { itemId, label: hit.label, disabled: true, clicks: 2 }; }
  await mouseClick(page, `[data-v4-menu] [data-v4-menu-item="${itemId}"]`);
  await page.waitForTimeout(800);
  const sheet = await sheetOpen(page);
  const res = { itemId, label: hit.label, clicks: 2, ops: (await ops(page)).slice(o0), downloads: env.downloads.slice(d0), sheet };
  return res;
}
async function viaButton(page, env, sel) {
  const o0 = (await ops(page)).length;
  if (!(await mouseClick(page, sel))) return { sel, error: 'Knopf nicht gefunden' };
  await page.waitForTimeout(800);
  return { sel, clicks: 1, ops: (await ops(page)).slice(o0), sheet: await sheetOpen(page) };
}

async function walkMain() {
  mkdirSync(OUT, { recursive: true });
  const server = await serve();
  const base = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium.launch({ headless: true, ...(existsSync(EXE) ? { executablePath: EXE } : {}) });
  const cfg = appConfig('outbound');
  for (const rows of Object.values(cfg.seed)) for (const r of rows) delete r._deleted;
  for (const rows of Object.values(cfg.seed)) for (const r of rows) for (const k of Object.keys(r)) {
    if (r[k] === 'user-001' && /(^|_)(user_id|owner_user_id|assigned_user_id|assignee_id|owner_id|author_user_id|actor_user_id|recipient_user_id)$/.test(k)) r[k] = 'owner';
  }
  configs.set('outbound', cfg);
  for (const w of WIDTHS) {
    if (MODE === 'before') {
      await scenario(browser, base, w, 'vorher', async ({ page, dir, entry }) => {
        await page.screenshot({ path: join(dir, `${w}-01-open.png`) });
        const camps = await campaignIds(page);
        for (const c of camps) entry.steps.push({ campaign: c.name, visible: await campaignVisible(page, c.id) });
        await toTable(page, w, '02-table', dir);
        for (const cid of await rowIds(page)) entry.steps.push({ company: cid, visible: await rowVisible(page, cid) });
      });
      continue;
    }
    // A: Sicht + Tastatur + Ausschließen (Menü)
    await scenario(browser, base, w, 'A Sicht, Tastatur, Ausschließen', async ({ page, dir, entry, downloads }) => {
      const env = { downloads };
      await page.screenshot({ path: join(dir, `${w}-01-open.png`) });
      const camps = await campaignIds(page);
      for (const c of camps) entry.steps.push({ campaign: c.name, visible: await campaignVisible(page, c.id) });
      await toTable(page, w, '02-table', dir);
      const ids = await rowIds(page);
      for (const cid of ids) {
        const more = `${R} tr[data-company-id="${cid}"] [data-action="company-more"]`;
        const visible = await rowVisible(page, cid);
        await mouseClick(page, more);
        await page.waitForSelector('[data-v4-menu]', { timeout: 3000 });
        const items = await menuItems(page);
        await page.screenshot({ path: join(dir, `${w}-03-menu-${cid}.png`) });
        await page.keyboard.press('Escape');
        await page.waitForTimeout(200);
        entry.steps.push({ company: cid, visible, menu: items.map((i) => `${i.id}${i.disabled ? '(aus)' : ''}=${i.label}`), escCloses: !(await menuOpen(page)), focusAfterEsc: await active(page) });
      }
      // Tastatur am ersten Zeilenmenü
      const cid = ids[0];
      const more = `${R} tr[data-company-id="${cid}"] [data-action="company-more"]`;
      await page.focus(more);
      const kb = { focus: await active(page) };
      await page.keyboard.press('Enter'); await page.waitForTimeout(200);
      kb.enterOpens = await menuOpen(page); kb.firstFocus = await active(page);
      await page.keyboard.press('ArrowDown'); await page.waitForTimeout(80); kb.afterDown = await active(page);
      await page.keyboard.press('ArrowUp'); await page.waitForTimeout(80); kb.afterUp = await active(page);
      await page.keyboard.press('End'); await page.waitForTimeout(80); kb.afterEnd = await active(page);
      await page.screenshot({ path: join(dir, `${w}-04-keyboard-menu.png`) });
      await page.keyboard.press('Escape'); await page.waitForTimeout(150);
      kb.escCloses = !(await menuOpen(page)); kb.focusBack = await active(page);
      await page.keyboard.press(' '); await page.waitForTimeout(200);
      kb.spaceOpens = await menuOpen(page);
      // Eintrag per Tastatur wählen: ArrowDown bis „research-company“, Enter
      const items = await menuItems(page);
      const target = items.findIndex((i) => i.id === 'research-company');
      for (let i = 0; i < target; i += 1) await page.keyboard.press('ArrowDown');
      const o0 = (await ops(page)).length;
      kb.chosen = await active(page);
      await page.keyboard.press('Enter'); await page.waitForTimeout(800);
      kb.enterRuns = (await ops(page)).slice(o0);
      kb.menuClosed = !(await menuOpen(page));
      entry.steps.push({ keyboard: kb });
      // Ausschließen per Maus über das Menü
      const r = await viaMenu(page, env, more, 'reject-company');
      entry.steps.push({ company: cid, action: r });
      entry.steps.push({ company: cid, visibleAfterReject: await rowVisible(page, cid) });
      await page.screenshot({ path: join(dir, `${w}-05-after-reject.png`) });
    });
    // B: übrige Zeilenaktionen per echter Maus
    await scenario(browser, base, w, 'B Zeilenaktionen', async ({ page, dir, entry, downloads }) => {
      const env = { downloads };
      await toTable(page, w, '06-table-b', dir);
      const cid = (await rowIds(page))[0];
      const more = `${R} tr[data-company-id="${cid}"] [data-action="company-more"]`;
      for (const it of ['download-company-research-log', 'validate-company-research', 'research-company']) entry.steps.push({ company: cid, action: await viaMenu(page, env, more, it) });
      entry.steps.push({ company: cid, primary: 'Qualifizieren', action: await viaButton(page, env, `${R} tr[data-company-id="${cid}"] .outbound-cell-actions [data-action="qualify-company"]`), visibleAfter: await rowVisible(page, cid) });
      await page.screenshot({ path: join(dir, `${w}-07-after-qualify.png`) });
      entry.steps.push({ company: cid, primary: 'In Pipeline', action: await viaButton(page, env, `${R} tr[data-company-id="${cid}"] .outbound-cell-actions [data-action="send-pipeline"]`), visibleAfter: await rowVisible(page, cid) });
      const before = await rowIds(page);
      entry.steps.push({ company: cid, action: await viaMenu(page, env, more, 'hide-company'), rowsBefore: before.length, rowsAfter: (await rowIds(page)).length });
      await page.screenshot({ path: join(dir, `${w}-08-after-hide.png`) });
    });
    // C: Kampagnen-Menü
    await scenario(browser, base, w, 'C Kampagnen', async ({ page, dir, entry, downloads }) => {
      const env = { downloads };
      const camps = await campaignIds(page);
      const c = camps.find((x) => x.selected) || camps[0];
      const more = `${R} .outbound-campaign-item[data-campaign-id="${c.id}"] [data-action="campaign-more"]`;
      await mouseClick(page, more);
      await page.waitForSelector('[data-v4-menu]', { timeout: 3000 });
      const items = await menuItems(page);
      await page.screenshot({ path: join(dir, `${w}-09-campaign-menu.png`) });
      await page.keyboard.press('Escape');
      entry.steps.push({ campaign: c.name, visible: await campaignVisible(page, c.id), menu: items.map((i) => `${i.id}${i.disabled ? '(aus)' : ''}=${i.label}`) });
      // Importjob anlegen
      const imp = await viaMenu(page, env, more, 'import-source');
      imp.importer = await page.evaluate(() => !!document.querySelector('.v4-app-root [data-universal-importer], .v4-app-root .universal-importer, .v4-app-root [class*="importer"]'));
      await page.screenshot({ path: join(dir, `${w}-10-import.png`) });
      entry.steps.push({ campaign: c.name, action: imp });
      imp.closedBy = (await mouseClick(page, `${R} [data-action="close-importer"]`)) ? 'Schließen-Knopf' : 'nicht gefunden';
      await page.waitForTimeout(400);
      const rb = await viaMenu(page, env, more, 'open-campaign-runbook');
      rb.navigate = await page.evaluate(() => window.__rg.nav || []);
      await page.screenshot({ path: join(dir, `${w}-10b-runbook.png`) });
      entry.steps.push({ campaign: c.name, action: rb });
      // Bearbeiten (sichtbares Symbol)
      const ed = await viaButton(page, env, `${R} .outbound-campaign-item[data-campaign-id="${c.id}"] [data-action="edit-campaign"]`);
      ed.editor = await page.evaluate(() => !!document.querySelector('.v4-app-root [data-action="save-campaign-edit"]'));
      await page.screenshot({ path: join(dir, `${w}-11-edit.png`) });
      entry.steps.push({ campaign: c.name, primary: 'Bearbeiten', action: ed });
      await mouseClick(page, `${R} [data-action="cancel-campaign-edit"]`);
      await page.waitForTimeout(400);
      // Löschen: andere Kampagne, Blatt bestätigen per Enter
      const camps2 = await campaignIds(page);
      const victim = camps2.find((x) => !x.selected && x.id !== c.id && x.name !== 'Outbound Firmenqualifizierung') || camps2[0];
      const moreV = `${R} .outbound-campaign-item[data-campaign-id="${victim.id}"] [data-action="campaign-more"]`;
      const del = await viaMenu(page, env, moreV, 'delete-campaign');
      await page.screenshot({ path: join(dir, `${w}-12-delete-sheet.png`) });
      const oDel = (await ops(page)).length;
      await page.keyboard.press('Enter'); await page.waitForTimeout(1200);
      del.opsAfterEnter = (await ops(page)).slice(oDel).filter((o) => /^remove/.test(o));
      del.campaignsBefore = camps2.length;
      del.campaignsAfter = (await campaignIds(page)).length;
      del.victimGone = !(await campaignIds(page)).some((x) => x.id === victim.id);
      entry.steps.push({ campaign: victim.name, action: del });
    });
  }
  writeFileSync(join(OUT, 'outbound', `menuwalk-${MODE}.json`), JSON.stringify(report, null, 1));
  for (const e of report) {
    log(`\n=== ${e.width} ${e.scenario}${e.errors.length ? '  FEHLER: ' + e.errors.join(' | ') : ''}`);
    for (const s of e.steps) log(JSON.stringify(s));
  }
  await browser.close();
  server.close();
}
walkMain().catch((e) => { console.error(e); process.exit(1); });
