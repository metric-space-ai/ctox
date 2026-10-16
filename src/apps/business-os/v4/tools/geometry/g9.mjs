// G9 Pixelmessung (Strang PAL, Owner-Mangel M-17): die Kante setzt das Logo ohne Naht fort.
//
// Gemessen wird im Screenshot: ΔE (CIE76) zwischen einem DECKENDEN Logopixel am Rand und der Kante direkt daneben –
//   oben:  deckender Logopixel, der der oberen rechten Icon-Ecke am nächsten liegt ↔ obere Kante rechts neben dem Icon,
//   links: deckender Logopixel, der der unteren linken Icon-Ecke am nächsten liegt ↔ linke Kante unter dem Icon.
// „Deckend“ = Alpha ≥ 250 im Icon-Bitmap (in Rendergröße gerastert), dazu ein Pixel weiter nach innen, damit die
// Kantenglättung der Rundung nicht mitmisst. Bei vollflächigen Raster-Icons ist das der Eckpixel (wie bisher), bei
// Logos mit runden, durchsichtigen Ecken der Pixel direkt hinter der Rundung.
//
// Die frühere Messung nahm stets den Eckpixel des Icons (ir.right − 2, ir.top + 1). Bei durchsichtigen Ecken zeigt der
// den Hintergrund des Icon-Halters, eine dunkle Kante sah daneben „nahtlos“ aus (notes: ΔE 1,8 bestanden, tatsächlich
// 83,7). Die Eckpunkte werden zum Vergleich weiter mitgeliefert (cornerTop/cornerLeft).

export const G9_MAX_DE = 12;

// Läuft in der Seite (page.evaluate). sel = CSS-Selektor des Fensters. Liefert CSS-Pixel-Koordinaten.
export function g9ProbeInPage(sel) {
  const w = document.querySelector(sel);
  const img = w?.querySelector('[data-window-app-icon]');
  if (!w || !img) return { skip: 'kein Fenster-Icon' };
  if (!img.complete || !img.naturalWidth || img.style.visibility === 'hidden') return { skip: 'Icon nicht geladen' };
  const wr = w.getBoundingClientRect();
  const ir = img.getBoundingClientRect();
  const W = Math.max(2, Math.round(ir.width));
  const H = Math.max(2, Math.round(ir.height));
  const c = document.createElement('canvas');
  c.width = W; c.height = H;
  const g = c.getContext('2d', { willReadFrequently: true });
  let data;
  try { g.drawImage(img, 0, 0, W, H); data = g.getImageData(0, 0, W, H).data; } catch { return { skip: 'Icon nicht lesbar (fremde Herkunft)' }; }
  const A = (x, y) => data[(y * W + x) * 4 + 3];
  const OPAQUE = 250;
  // Nächster deckender Pixel zur Ecke (cx, cy) im Viertel nach innen (dx, dy), dann ein Pixel weiter hinein.
  const nearest = (cx, cy, dx, dy) => {
    let best = null; let bd = Infinity;
    for (let j = 0; j < Math.ceil(H / 2); j += 1) {
      for (let i = 0; i < Math.ceil(W / 2); i += 1) {
        const d = i * i + j * j;
        if (d >= bd) continue;
        if (A(cx + dx * i, cy + dy * j) >= OPAQUE) { bd = d; best = [i, j]; }
      }
    }
    if (!best) return null;
    const i = Math.min(best[0] + 1, Math.ceil(W / 2) - 1); const j = Math.min(best[1] + 1, Math.ceil(H / 2) - 1);
    return { x: ir.left + cx + dx * i + 0.5, y: ir.top + cy + dy * j + 0.5, inset: [best[0], best[1]] };
  };
  const top = nearest(W - 1, 0, -1, 1);
  const left = nearest(0, H - 1, 1, -1);
  return {
    maximized: w.classList.contains('is-maximized') || w.dataset.windowLayout === 'maximize' || w.dataset.windowState === 'maximized',
    logoTop: top, logoLeft: left,
    edgeTop: { x: ir.right + 2, y: wr.top + 1 },
    edgeLeft: { x: wr.left + 1, y: ir.bottom + 2 },
    cornerTop: { x: ir.right - 2, y: ir.top + 1 },
    cornerLeft: { x: ir.left + 1, y: ir.bottom - 2 },
    icon: { w: W, h: H, src: (img.currentSrc || img.src || '').slice(0, 60) },
  };
}

export function lab([r, g, b]) {
  const f = (v) => { v /= 255; return v > 0.04045 ? ((v + 0.055) / 1.055) ** 2.4 : v / 12.92; };
  const [R, G, B] = [f(r), f(g), f(b)];
  const X = (R * 0.4124 + G * 0.3576 + B * 0.1805) / 0.95047; const Y = R * 0.2126 + G * 0.7152 + B * 0.0722; const Z = (R * 0.0193 + G * 0.1192 + B * 0.9505) / 1.08883;
  const h = (t) => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116);
  return [116 * h(Y) - 16, 500 * (h(X) - h(Y)), 200 * (h(Y) - h(Z))];
}
export function deltaE(a, b) {
  const [p, q] = [lab(a), lab(b)];
  return +Math.hypot(p[0] - q[0], p[1] - q[1], p[2] - q[2]).toFixed(1);
}

// Pixel aus einem PNG-Screenshot lesen (Dekodierung in der Seite, kein PNG-Paket nötig). pts in CSS-Pixeln.
export async function pixelsFromPng(page, png, pts, dpr = 1) {
  return page.evaluate(async ([b64, pts, dpr]) => {
    const img = new Image(); img.src = `data:image/png;base64,${b64}`; await img.decode();
    const c = document.createElement('canvas'); c.width = img.naturalWidth; c.height = img.naturalHeight;
    const x = c.getContext('2d'); x.drawImage(img, 0, 0);
    return pts.map(([a, b]) => [...x.getImageData(Math.floor(a * dpr), Math.floor(b * dpr), 1, 1).data].slice(0, 3));
  }, [png.toString('base64'), pts, dpr]);
}

// G9 für ein Fenster: { fails: [...], row: {...} } bzw. { skip }.
export async function g9Pixel(page, sel, { dpr = 1, label = sel } = {}) {
  const p = await page.evaluate(g9ProbeInPage, sel);
  if (p.skip) return { skip: p.skip, fails: [] };
  if (!p.logoTop || !p.logoLeft) return { skip: 'Icon ohne deckende Pixel', fails: [] };
  const png = await page.screenshot();
  const pts = [p.logoTop, p.edgeTop, p.logoLeft, p.edgeLeft, p.cornerTop, p.cornerLeft].map((q) => [q.x, q.y]);
  const [logoTop, edgeTop, logoLeft, edgeLeft, cornerTop, cornerLeft] = await pixelsFromPng(page, png, pts, dpr);
  const row = {
    top: deltaE(logoTop, edgeTop), left: deltaE(logoLeft, edgeLeft),
    cornerTop: deltaE(cornerTop, edgeTop), cornerLeft: deltaE(cornerLeft, edgeLeft),
    logoTop, edgeTop, logoLeft, edgeLeft, insetTop: p.logoTop.inset, insetLeft: p.logoLeft.inset,
  };
  const fails = [];
  if (row.top >= G9_MAX_DE) fails.push(`${label}: Naht oben – deckendes Logopixel ${logoTop} ↔ Kante ${edgeTop} ΔE ${row.top}`);
  if (row.left >= G9_MAX_DE) fails.push(`${label}: Naht links – deckendes Logopixel ${logoLeft} ↔ Kante ${edgeLeft} ΔE ${row.left}`);
  return { row, fails };
}
