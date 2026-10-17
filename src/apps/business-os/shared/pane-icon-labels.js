// Icon + Wort (UI-Regel „Icons tragen ein Wort“, workjet#801): Kopf- und Werkzeugknöpfe
// mit `data-short-de` / `data-short-en` bekommen ein sichtbares Kurzwort neben dem Icon.
// Der ausführliche Text bleibt in aria-label/title.
//
// Wörter nur, wo Platz ist: Je Kopf-/Werkzeugzeile mit Wortknöpfen misst ein Beobachter, ob die Wörter passen. Läuft
// die Zeile über, wird ein Titel abgeschnitten oder fällt ein Suchfeld unter SEARCH_MIN, bekommt die Zeile
// `data-icons-only` (base.css: nur Icons). Neu entschieden bei jeder Größenänderung, höchstens einmal je Bild; der
// Beobachter reagiert nicht auf das eigene Attribut (keine Rückkopplung). Der v4-Rahmen prüft zusätzlich die
// reservierten Zonen (v4/frame/pane-heads.js, data-v4-icons-only).
//
//   const labels = applyPaneIconLabels(root, lang);   // mehrfach aufrufbar (Sprachwechsel), ein Beobachter je root
//   labels.dispose();                                  // beim Abbau: Beobachter trennen, Markierungen entfernen

export const ICONS_ONLY_ATTR = 'data-icons-only';
const ROWS = '.ctox-pane-header, .ctox-filterbar';
const LABELLED = '.ctox-pane-icon.has-label';
const SEARCH_MIN = 120;
const fitters = new WeakMap();

export function applyPaneIconLabels(root, lang = 'de') {
  if (!root?.querySelectorAll) return { dispose() {}, fit() {} };
  const key = lang === 'en' ? 'shortEn' : 'shortDe';
  for (const button of root.querySelectorAll('[data-short-de]')) {
    const word = button.dataset[key] || button.dataset.shortDe;
    let label = button.querySelector(':scope > .ctox-pane-icon-label');
    if (!label) {
      label = button.ownerDocument.createElement('span');
      label.className = 'ctox-pane-icon-label';
      button.append(label);
    }
    if (label.textContent !== word) label.textContent = word;
    button.classList.add('has-label');
  }
  let fitter = fitters.get(root);
  if (!fitter) { fitter = createFitter(root); fitters.set(root, fitter); }
  fitter.fit();
  return fitter;
}

function rowIsTight(row) {
  const r = row.getBoundingClientRect();
  if (!(r.width > 0 && r.height > 0)) return false;
  const cut = (el) => el.scrollWidth > el.clientWidth + 1;
  if (cut(row) || [...row.children].some(cut)) return true;
  for (const b of row.querySelectorAll(LABELLED)) {
    const br = b.getBoundingClientRect();
    if (br.width > 0 && (br.right > r.right + 0.5 || br.left < r.left - 0.5)) return true;
  }
  for (const t of row.querySelectorAll('.ctox-pane-title')) if (cut(t)) return true;
  for (const i of row.querySelectorAll('input[type="search"], input[type="text"], input:not([type])')) {
    const w = i.getBoundingClientRect().width;
    if (w > 0 && w < SEARCH_MIN) return true;
  }
  return false;
}

function createFitter(root) {
  const win = root.ownerDocument?.defaultView;
  const raf = win?.requestAnimationFrame?.bind(win) || ((fn) => setTimeout(fn, 16));
  const caf = win?.cancelAnimationFrame?.bind(win) || clearTimeout;
  let frame = 0;
  let disposed = false;
  const observed = new Set();
  const ro = typeof win?.ResizeObserver === 'function' ? new win.ResizeObserver(() => schedule()) : null;
  ro?.observe(root);

  function fit() {
    frame = 0;
    if (disposed) return;
    const rows = [...root.querySelectorAll(ROWS)].filter((el) => el.querySelector(LABELLED));
    for (const el of observed) if (!rows.includes(el)) { ro?.unobserve(el); observed.delete(el); }
    for (const el of rows) if (!observed.has(el)) { ro?.observe(el); observed.add(el); }
    // Mit Wörtern messen (alle Markierungen entfernen, ein Layout lesen), dann gezielt setzen.
    for (const el of rows) el.removeAttribute(ICONS_ONLY_ATTR);
    const tight = rows.filter(rowIsTight);
    for (const el of tight) el.setAttribute(ICONS_ONLY_ATTR, '');
  }
  function schedule() {
    if (!frame && !disposed) frame = raf(fit);
  }
  return {
    fit,
    dispose() {
      disposed = true;
      if (frame) caf(frame);
      frame = 0;
      ro?.disconnect();
      for (const el of root.querySelectorAll(`[${ICONS_ONLY_ATTR}]`)) el.removeAttribute(ICONS_ONLY_ATTR);
      observed.clear();
      fitters.delete(root);
    },
  };
}
