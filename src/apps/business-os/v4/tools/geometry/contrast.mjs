#!/usr/bin/env node
// Shell v4 – Kontrast-Kriterium „G10 Kontrast“ (Strang DARK).
//
// Öffnet je App und Theme (hell/dunkel) den echten v4-Rahmen über den Labor-Server des Rundgangs (rundgang.mjs:
// serve/OTHERS), startet die App über ihr Desktop-Symbol ([data-v4-desk-app], echter Klick wie rundgang2/check-raw-ids),
// maximiert sie und misst den Kontrast jedes sichtbaren Textes und jedes Symbols (SVG) gegen seinen tatsächlichen
// Hintergrund: im geöffneten Zustand und nach Klick auf den ersten Datensatz ([data-context-record-id]).
//
//   WCAG 2.x: Fließtext ≥ 4,5 · große Schrift (≥ 24px oder ≥ 18,66px fett) ≥ 3 · Symbole ≥ 3.
//   Nicht gemessen (WCAG-Ausnahmen): deaktivierte Bedienelemente, unsichtbare/abgeschnittene Knoten.
//   Bereich je Fund: Kopf (Pane-Kopf/Kopfzeilen), Liste (erste Pane), Detail (weitere Panes), Inhalt (ohne Panes).
//
//   node v4/tools/geometry/contrast.mjs [--app a,b] [--width 1280] [--themes light,dark] [--json out.json]
//        [--shots <dir>] [--strict]   ohne --app: alle Apps mit v4-Manifest. Exit 1 bei App-Funden.
//   Funde, deren Vorder- und Hintergrund exakt Theme-Tokens des Rahmens sind (z. B. --muted auf --surface-2), sind
//   „Rahmen-Token“: gemeldet (TOKEN-Zeilen, JSON .token), aber kein App-Fund; --strict zählt sie mit.
//
// Als Modul: contrastRatio(fg, bg), minRatio({fontSize, fontWeight, symbol}) – reine Funktionen (contrast.test.mjs)
// und probeContrast (läuft in der Seite, Argument {min:{body,large,symbol}}).
import { writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

export const MIN = { body: 4.5, large: 3, symbol: 3 };

const lin = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
export const luminance = ({ r, g, b }) => 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
/** Kontrastverhältnis zweier deckender Farben {r,g,b} (0–255). */
export function contrastRatio(fg, bg) {
  const [a, b] = [luminance(fg), luminance(bg)].sort((x, y) => y - x);
  return (a + 0.05) / (b + 0.05);
}
/** Mindestkontrast je Knoten nach WCAG (große Schrift: ≥ 24px oder ≥ 18,66px ab 700). */
export function minRatio({ fontSize = 16, fontWeight = 400, symbol = false } = {}, min = MIN) {
  if (symbol) return min.symbol;
  return fontSize >= 24 || (fontSize >= 18.66 && Number(fontWeight) >= 700) ? min.large : min.body;
}

/** In der Seite: alle Kontrastfunde unter .v4-app-root. Muss eigenständig sein (wird serialisiert). */
export function probeContrast({ min }) {
  const root = document.querySelector('.v4-app-root');
  if (!root) return { open: false, checked: 0, low: [] };
  const win = root.closest('.shell-window') || root;
  const W = win.getBoundingClientRect();
  const cv = document.createElement('canvas'); cv.width = cv.height = 1;
  const cx = cv.getContext('2d', { willReadFrequently: true });
  // Farben über ein Canvas normalisieren (color-mix/color(srgb …)/oklch inklusive).
  const rgb = (c) => { cx.clearRect(0, 0, 1, 1); cx.fillStyle = '#000'; cx.fillStyle = c; cx.fillRect(0, 0, 1, 1); const [r, g, b, a] = cx.getImageData(0, 0, 1, 1).data; return { r, g, b, a: a / 255 }; };
  const over = (top, under) => ({ r: top.r * top.a + under.r * (1 - top.a), g: top.g * top.a + under.g * (1 - top.a), b: top.b * top.a + under.b * (1 - top.a), a: 1 });
  const lin = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
  const lum = ({ r, g, b }) => 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
  const ratioOf = (f, b) => { const [x, y] = [lum(f), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
  // Hintergrund: Farbschichten von innen nach außen bis zur ersten deckenden; Verläufe (z. B. die Shell-v2-Fläche als
  // linear-gradient(...) padding-box) zählen mit ihrer ersten Farbe.
  const layerOf = (n) => {
    const s = getComputedStyle(n);
    const c = rgb(s.backgroundColor);
    if (c.a > 0) return c;
    const img = s.backgroundImage;
    if (img && img !== 'none') { const m = /(rgba?\([^)]*\)|#[0-9a-f]{3,8}|color\([^)]*\))/i.exec(img); if (m) return rgb(m[1]); }
    return null;
  };
  const bgCache = new Map();
  const bgOf = (el) => {
    if (bgCache.has(el)) return bgCache.get(el);
    const layers = [];
    let from = null;
    for (let n = el; n; n = n.parentElement) { const c = layerOf(n); if (c) { layers.push(c); from = n; if (c.a >= 0.99) break; } }
    const v = layers.reverse().reduce((acc, c) => over(c, acc), { r: 255, g: 255, b: 255, a: 1 });
    v.from = from ? `${from.tagName.toLowerCase()}.${String(from.className?.baseVal ?? from.className ?? '').trim().split(/\s+/).slice(0, 2).join('.')}` : 'document';
    bgCache.set(el, v);
    return v;
  };
  const opacityOf = (el) => { let o = 1; for (let n = el; n && n !== document.documentElement; n = n.parentElement) o *= Number(getComputedStyle(n).opacity || 1); return o; };
  const panes = [...root.querySelectorAll('.ctox-pane')].filter((p) => !p.parentElement.closest('.ctox-pane'));
  const regionOf = (el) => {
    if (el.closest('.ctox-pane-header, [data-shell-v2-header-row], header, thead, [role=toolbar]')) return 'Kopf';
    const i = panes.findIndex((p) => p.contains(el));
    return i < 0 ? 'Inhalt' : i === 0 ? 'Liste' : 'Detail';
  };
  const visible = (el) => {
    if (!el.checkVisibility?.({ checkOpacity: true, checkVisibilityCSS: true })) return false;
    const r = el.getBoundingClientRect();
    if (r.width < 1 || r.height < 1) return false;
    // nur im Fenster und im Viewport (abgeschnittene/verdeckte Bereiche zählen nicht)
    return r.right > W.left && r.left < W.right && r.bottom > W.top && r.top < Math.min(W.bottom, innerHeight);
  };
  // Theme-Tokens des Rahmens (theme.css/shell-v2-desktop.css) und die Kit-Mulde. Ein Fund, dessen Vorder- UND
  // Hintergrund exakt ein Token-Paar ist (z. B. --muted auf .ctox-well), liegt an der Palette von Rahmen/Kit, nicht an der App.
  const rs = getComputedStyle(root);
  const tok = (names) => names.map((n) => rs.getPropertyValue(n).trim()).filter(Boolean).map(rgb).filter((c) => c.a >= 0.99);
  const FG_TOK = tok(['--text', '--text-strong', '--muted', '--accent', '--accent-foreground', '--danger']);
  const BG_TOK = tok(['--bg', '--surface', '--surface-2', '--surface-3', '--accent', '--accent-soft']);
  // Kit-Mulde .ctox-well (shared/base.css): color-mix(in srgb, var(--text) 6%, var(--surface)).
  { const [t, f] = tok(['--text', '--surface']); if (t && f) BG_TOK.push({ r: t.r * 0.06 + f.r * 0.94, g: t.g * 0.06 + f.g * 0.94, b: t.b * 0.06 + f.b * 0.94, a: 1 }); }
  // Paare aus der Palette (auch Text in Flächenfarbe auf Akzent, wie der Kit-Primärknopf) zählen als Token-Paar –
  // aber nur ab 3: darunter ist es keine knappe Palettenentscheidung, sondern ein falsch kombiniertes Paar der App.
  const PAL = [...FG_TOK, ...BG_TOK];
  const near = (a, list) => list.some((c) => Math.abs(a.r - c.r) <= 2 && Math.abs(a.g - c.g) <= 2 && Math.abs(a.b - c.b) <= 2);
  const low = [];
  let checked = 0;
  for (const el of root.querySelectorAll('*')) {
    const isSvg = el.tagName.toLowerCase() === 'svg';
    const ownText = !isSvg && [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
    if (!ownText && !isSvg) continue;
    if (el instanceof SVGElement && !isSvg) continue;
    if (el.closest(':disabled, [aria-disabled="true"], [inert]')) continue;
    if (!visible(el)) continue;
    const s = getComputedStyle(el);
    let fg;
    if (isSvg) {
      const r = el.getBoundingClientRect();
      if (r.width < 8 || r.height < 8) continue;
      // Symbolfarbe aus den gezeichneten Formen (Strich vor Füllung). Mehrfarbige SVGs sind Illustrationen
      // (Porträts, Diagramme), keine Symbole, und werden nicht gemessen; ebenso SVGs über 48px.
      if (r.width > 48 || r.height > 48) continue;
      const paints = new Set();
      for (const c of el.querySelectorAll('path, circle, rect, line, polyline, polygon, ellipse')) {
        const st = getComputedStyle(c);
        if (st.display === 'none' || st.visibility === 'hidden' || Number(st.opacity) === 0) continue;
        const k = st.stroke && st.stroke !== 'none' ? st.stroke : (st.fill && st.fill !== 'none' ? st.fill : '');
        if (k && !/^url\(/.test(k)) paints.add(k);
      }
      if (paints.size !== 1) continue;
      const k = [...paints][0];
      fg = rgb(k);
      if (fg.a === 0) continue;
    } else {
      fg = rgb(s.color);
    }
    const bg = bgOf(el.parentElement && isSvg ? el.parentElement : el);
    const fgEff = over({ ...fg, a: fg.a * opacityOf(el) }, bg);
    const fontSize = parseFloat(s.fontSize) || 16;
    const fontWeight = Number(s.fontWeight) || 400;
    const need = isSvg ? min.symbol : (fontSize >= 24 || (fontSize >= 18.66 && fontWeight >= 700) ? min.large : min.body);
    const ratio = ratioOf(fgEff, bg);
    checked += 1;
    if (ratio + 1e-6 < need) {
      const text = isSvg ? `[Symbol] ${(el.closest('[aria-label],[title]')?.getAttribute('aria-label') || el.closest('[title]')?.title || '').slice(0, 30)}` : el.textContent.replace(/\s+/g, ' ').trim().slice(0, 40);
      const cls = String(el.className?.baseVal ?? el.className ?? '').trim().split(/\s+/).slice(0, 2).join('.');
      const hex = (c) => '#' + [c.r, c.g, c.b].map((v) => Math.round(v).toString(16).padStart(2, '0')).join('');
      low.push({ kind: near(fgEff, PAL) && near(bg, PAL) && ratio >= 3 ? 'token' : 'app', region: regionOf(el), sel: `${el.tagName.toLowerCase()}${cls ? '.' + cls : ''}`, text, ratio: Math.round(ratio * 100) / 100, need, fg: hex(fgEff), bg: hex(bg), bgFrom: bg.from, ...(isSvg ? { host: (el.parentElement?.outerHTML || '').replace(/<svg[\s\S]*$/, '').slice(0, 160) } : {}) });
    }
  }
  return { open: true, checked, low };
}

// ---------------------------------------------------------------- Browser-Lauf
async function measureApp(browser, base, id, w, theme, shots) {
  const ctx = await browser.newContext({ viewport: { width: w, height: w <= 480 ? 860 : 800 }, deviceScaleFactor: 1, locale: 'de-DE', colorScheme: theme });
  const page = await ctx.newPage();
  await page.route((u) => !u.href.startsWith(base) && !u.href.startsWith('blob:') && !u.href.startsWith('data:'), (r) => r.abort());
  const settle = async (ms) => {
    await page.waitForTimeout(ms);
    await page.evaluate(async () => { try { await window.__rg.kernel.sim.settle(); } catch {} await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))); });
  };
  const states = [];
  try {
    await page.goto(`${base}/__rg/page.html?app=${encodeURIComponent(id)}`);
    await page.waitForFunction(() => window.__rgReady === true, null, { timeout: 30000 });
    await page.evaluate((t) => { document.documentElement.dataset.theme = t; delete document.documentElement.dataset.v4ThemeAuto; }, theme);
    await settle(300);
    const tile = page.locator(`[data-v4-desk-app="${id}"]`).first();
    await tile.scrollIntoViewIfNeeded({ timeout: 5000 });
    await tile.click();
    await page.waitForSelector('.v4-app-root', { timeout: 12000 });
    await settle(900);
    const max = page.locator(`[data-window-app="${id}"] [data-window-layout-control="maximize"], [data-window-app="${id}"] [data-window-control="maximize"]`).first();
    if (await max.isVisible().catch(() => false)) { await max.click().catch(() => {}); await settle(300); }
    // Transitionen (Hover/Fokus/Theme-Wechsel) ausklingen lassen, Zeiger aus dem Fenster nehmen.
    await page.mouse.move(1, 1);
    await settle(400);
    states.push({ state: 'offen', ...(await page.evaluate(probeContrast, { min: MIN })) });
    if (shots) await page.screenshot({ path: join(shots, `${id}-${theme}-${w}.png`) });
    const row = page.locator('.v4-app-root [data-context-record-id]').first();
    if (await row.isVisible().catch(() => false)) {
      await row.click({ timeout: 3000 }).catch(() => {});
      await page.mouse.move(1, 1);
      await settle(700);
      states.push({ state: 'Datensatz', ...(await page.evaluate(probeContrast, { min: MIN })) });
      if (shots) await page.screenshot({ path: join(shots, `${id}-${theme}-${w}-detail.png`) });
    }
    return { id, theme, ok: true, states };
  } catch (error) {
    return { id, theme, ok: false, error: String(error?.message || error).split('\n')[0].slice(0, 160), states };
  } finally {
    await ctx.close();
  }
}

async function main() {
  const args = process.argv.slice(2);
  const opt = (name, dflt) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : dflt; };
  const mod = await import('../rundgang/rundgang.mjs');
  const geo = await import('./geometry-v4.mjs');
  const apps = String(opt('--app', '')).split(',').filter(Boolean);
  const ids = apps.length ? apps : mod.OTHERS.map((o) => o.id);
  const w = Number(opt('--width', '1280'));
  const themes = String(opt('--themes', 'light,dark')).split(',').filter(Boolean);
  const shots = opt('--shots', '');
  if (shots) mkdirSync(shots, { recursive: true });
  const server = await mod.serve();
  const base = `http://127.0.0.1:${server.address().port}`;
  const browser = await mod.chromium.launch({ headless: true, ...(existsSync(mod.EXE) ? { executablePath: mod.EXE } : {}) });
  const strict = args.includes('--strict');
  const tokenPairs = new Map();
  const report = [];
  let bad = 0;
  for (const id of ids) {
    try {
      geo.configs.set(id, { ...geo.appConfig(id), title: mod.OTHERS.find((o) => o.id === id)?.title });
    } catch (error) {
      // wie check-raw-ids: Apps ohne Laborkonfiguration (customer-modules) werden übersprungen und gemeldet.
      report.push({ id, ok: false, skipped: true, error: `keine Laborkonfiguration: ${String(error?.message || error).slice(0, 120)}` });
      console.log(`SKIP ${id.padEnd(26)} keine Laborkonfiguration`);
      continue;
    }
    for (const theme of themes) {
      const r = await measureApp(browser, base, id, w, theme, shots);
      const seen = new Set();
      const low = [];
      for (const st of r.states) for (const l of st.low) { const k = `${l.region}|${l.sel}|${l.text}|${l.ratio}`; if (seen.has(k)) continue; seen.add(k); low.push({ ...l, state: st.state }); }
      const checked = r.states.reduce((n, s) => n + (s.checked || 0), 0);
      const appLow = low.filter((l) => strict || l.kind === 'app');
      const tokLow = low.filter((l) => l.kind === 'token');
      report.push({ id, theme, ok: r.ok, error: r.error, checked, low: appLow, token: tokLow });
      if (!r.ok || appLow.length || !checked) bad += 1;
      const tag = !r.ok ? 'FEHL' : appLow.length ? 'LOW ' : !checked ? 'LEER' : 'OK  ';
      console.log(`${tag} ${id.padEnd(26)} ${theme.padEnd(5)} ${String(checked).padStart(4)} geprüft, ${appLow.length} zu schwach${tokLow.length ? ` (+${tokLow.length} Rahmen-Token)` : ''}${appLow.length ? ': ' + appLow.slice(0, 4).map((l) => `${l.region} ${l.sel} "${l.text}" ${l.ratio}<${l.need} ${l.fg}/${l.bg}`).join(' · ') : ''}${r.error ? ' · ' + r.error : ''}`);
      for (const l of tokLow) { const k = `${theme} ${l.fg} auf ${l.bg}`; const t = tokenPairs.get(k) || { ratio: l.ratio, need: l.need, n: 0, apps: new Set() }; t.n += 1; t.apps.add(id); tokenPairs.set(k, t); }
    }
  }
  await browser.close();
  server.close();
  const json = opt('--json', '');
  if (json) writeFileSync(json, JSON.stringify(report, null, 1));
  for (const [k, t] of tokenPairs) console.log(`TOKEN ${k}: ${t.ratio} < ${t.need} · ${t.n}× in ${t.apps.size} Apps (Rahmen-Palette, nicht App)`);
  const skipped = report.filter((r) => r.skipped).map((r) => r.id);
  console.log((bad ? `G10 Kontrast: ${bad} App/Theme-Läufe mit Funden` : `G10 Kontrast: OK (${report.length - skipped.length} App/Theme-Läufe)`) + (skipped.length ? ` · übersprungen: ${skipped.join(', ')}` : ''));
  process.exit(bad ? 1 : 0);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main().catch((e) => { console.error(e); process.exit(1); });
