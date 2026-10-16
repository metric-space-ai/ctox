#!/usr/bin/env node
// Shell v4 – Roh-ID-Prüfer (Textdiät Regel 4: keine technischen IDs/Rohwerte in der Oberfläche).
//
// Öffnet je App den echten v4-Rahmen über den Labor-Server des Rundgangs (rundgang.mjs: serve/configs, Fixtures aus
// geometry/fixtures.mjs), klickt die Launcher-Kachel, maximiert, liest den sichtbaren Text (innerText von
// .v4-app-root) im geöffneten Zustand und nach Klick auf den ersten Datensatz ([data-context-record-id]) und meldet
// Roh-IDs je App:
//   slug-id  kleingeschriebener Bezeichner mit Ziffernfolge: party-001, cand_12, manualreq_1729…   /\b[a-z]+(?:[-_][a-z]+)*[-_]{1,2}\d{2,}\w*\b/
//   uuid     8-4-4-4-12 Hex
//   hex      lange Hex-Werte (≥ 16 Zeichen, Ziffern und Buchstaben gemischt)
//   hash     Hash-Kürzel neben Text (7–12 Hex-Zeichen mit ≥ 2 Ziffern und ≥ 1 Buchstaben a–f), z. B. a65ff419f
//   snake    snake_case-Schlüssel/Rohwerte (partially_signed, customer_accounts)
// Nicht gemeldet: Rechnungs-/Belegnummern in Großschrift (RE-2026-0001), Datum/Uhrzeit, Beträge, Normen (ISO 9001).
//
//   node v4/tools/rundgang/check-raw-ids.mjs [--app a,b] [--width 1280] [--json out.json] [--md out.md]
//   ohne --app: alle Apps mit v4-Manifest. Stub über CTOX_APP_API_STUB wie Rundgang/Geometrie-Labor.
//
// Als Modul: findRawIds(text) → [{kind, value, context}] (reine Funktion, siehe check-raw-ids.test.mjs).
import { writeFileSync, existsSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const PATTERNS = [
  ['uuid', /\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/gi],
  ['slug-id', /(?<![\w.-])[a-z]+(?:[-_][a-z]+)*[-_]{1,2}\d{2,}[a-z0-9_-]*(?![\w-])/g],
  ['hex', /\b[0-9a-f]{16,}\b/gi],
  ['hash', /\b[0-9a-f]{7,12}\b/g],
  ['snake', /(?<![\w.-])[a-z]+(?:_[a-z0-9]+)+(?![\w-])/g],
];

function mixedHex(v, minDigits = 1) {
  const digits = (v.match(/\d/g) || []).length;
  return digits >= minDigits && /[a-f]/i.test(v);
}

/** Roh-IDs in einem sichtbaren Text. Jede Fundstelle nur einmal (erste passende Art gewinnt). */
export function findRawIds(text) {
  const src = String(text || '');
  const hits = [];
  const taken = [];
  const overlaps = (a, b) => taken.some(([s, e]) => a < e && b > s);
  for (const [kind, re] of PATTERNS) {
    re.lastIndex = 0;
    let m;
    while ((m = re.exec(src))) {
      const value = m[0];
      const start = m.index;
      const end = start + value.length;
      if (overlaps(start, end)) continue;
      if (kind === 'hex' && !mixedHex(value)) continue;
      if (kind === 'hash' && !mixedHex(value, 2)) continue;
      taken.push([start, end]);
      hits.push({ kind, value, context: src.slice(Math.max(0, start - 30), Math.min(src.length, end + 30)).replace(/\s+/g, ' ').trim() });
    }
  }
  return hits;
}

// ---------------------------------------------------------------- Browser-Lauf
async function readApp(browser, base, id, w) {
  const ctx = await browser.newContext({ viewport: { width: w, height: w <= 480 ? 860 : 800 }, deviceScaleFactor: 1, locale: 'de-DE' });
  const page = await ctx.newPage();
  await page.route((u) => !u.href.startsWith(base) && !u.href.startsWith('blob:') && !u.href.startsWith('data:'), (r) => r.abort());
  const settle = async (ms) => {
    await page.waitForTimeout(ms);
    await page.evaluate(async () => { try { await window.__rg.kernel.sim.settle(); } catch {} await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))); });
  };
  const texts = [];
  try {
    await page.goto(`${base}/__rg/page.html?app=${encodeURIComponent(id)}`);
    await page.waitForFunction(() => window.__rgReady === true, null, { timeout: 30000 });
    await settle(300);
    const tile = page.locator(`[data-v4-desk-app="${id}"]`).first(); // FR6d: Apps starten vom Desktop (wie rundgang2)
    await tile.scrollIntoViewIfNeeded({ timeout: 5000 });
    await tile.click();
    await page.waitForSelector('.v4-app-root', { timeout: 12000 });
    await settle(900);
    const max = page.locator(`[data-window-app="${id}"] [data-window-layout-control="maximize"], [data-window-app="${id}"] [data-window-control="maximize"]`).first();
    if (await max.isVisible().catch(() => false)) { await max.click().catch(() => {}); await settle(300); }
    texts.push(await page.evaluate(() => document.querySelector('.v4-app-root')?.innerText || ''));
    const row = page.locator('.v4-app-root [data-context-record-id]').first();
    if (await row.isVisible().catch(() => false)) {
      await row.click({ timeout: 3000 }).catch(() => {});
      await settle(600);
      texts.push(await page.evaluate(() => document.querySelector('.v4-app-root')?.innerText || ''));
    }
    return { id, ok: true, texts };
  } catch (error) {
    return { id, ok: false, error: String(error?.message || error).split('\n')[0].slice(0, 160), texts };
  } finally {
    await ctx.close();
  }
}

async function main() {
  const args = process.argv.slice(2);
  const opt = (name, dflt) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : dflt; };
  const mod = await import('./rundgang.mjs');
  const geo = await import('../geometry/geometry-v4.mjs');
  const apps = String(opt('--app', '')).split(',').filter(Boolean);
  const ids = apps.length ? apps : mod.OTHERS.map((o) => o.id);
  const w = Number(opt('--width', '1280'));
  const server = await mod.serve();
  const base = `http://127.0.0.1:${server.address().port}`;
  const browser = await mod.chromium.launch({ headless: true, ...(existsSync(mod.EXE) ? { executablePath: mod.EXE } : {}) });
  const report = [];
  for (const id of ids) {
    try {
      geo.configs.set(id, { ...geo.appConfig(id), title: mod.OTHERS.find((o) => o.id === id)?.title });
    } catch (error) {
      report.push({ id, ok: false, error: `keine Laborkonfiguration: ${String(error?.message || error).slice(0, 120)}`, hits: [] });
      console.log(`SKIP ${id.padEnd(24)} keine Laborkonfiguration`);
      continue;
    }
    const r = await readApp(browser, base, id, w);
    const seen = new Set();
    const hits = [];
    for (const t of r.texts) for (const h of findRawIds(t)) { if (seen.has(h.value)) continue; seen.add(h.value); hits.push(h); }
    report.push({ id, ok: r.ok, error: r.error, hits });
    console.log(`${r.ok ? (hits.length ? 'IDS ' : 'OK  ') : 'FEHL'} ${id.padEnd(24)} ${hits.length} ${hits.slice(0, 6).map((h) => h.value).join(', ')}${r.error ? ' · ' + r.error : ''}`);
  }
  await browser.close();
  server.close();
  const json = opt('--json', '');
  if (json) writeFileSync(json, JSON.stringify(report, null, 1));
  const md = opt('--md', '');
  if (md) {
    const esc = (s) => String(s).replace(/\|/g, '\\|');
    const lines = [`| App | Treffer | Roh-IDs (Art: Wert – Umfeld) |`, `|---|---|---|`];
    for (const r of report) {
      const cell = r.ok === false && !r.hits.length ? `nicht geprüft: ${esc(r.error || '')}` : r.hits.map((h) => `${h.kind}: \`${esc(h.value)}\` – ${esc(h.context)}`).join('<br>') || '—';
      lines.push(`| ${r.id} | ${r.hits.length} | ${cell} |`);
    }
    writeFileSync(md, lines.join('\n') + '\n');
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main().catch((e) => { console.error(e); process.exit(1); });
