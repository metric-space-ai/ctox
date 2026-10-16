// Shell v4 – Geometrie-Labor: Kriterium G11 „schmal“ (Strang KIT1). Seitenfunktionen ohne Importe (page.evaluate).
//
// Bei schmaler Fensterbreite (420) gilt je App:
//   a) höchstens EINE Pane-Spalte sichtbar: in jedem Workspace (.ctox-workspace / [data-resize-frame], äußerster mit
//      ≥ 2 Panes) ist im Grundzustand höchstens eine Pane sichtbar (keine nebeneinander, keine gestapelten Liste+Detail);
//   b) Reiter einzeilig: alle sichtbaren Reiter einer .ctox-pane-tabs bzw. [role=tablist] auf einer Oberkante (±3 px)
//      und keiner ragt über den sichtbaren Bereich (root bzw. abschneidender Vorfahr) hinaus;
//   c) kein horizontaler Überlauf von root;
//   d) Detail erreichbar und Zurück funktioniert (interaktiv, echter Mausklick, geometry-v4.mjs runG11): Zeile der
//      Liste antippen → Detail-Pane sichtbar, Liste verborgen, „Zurück“ sichtbar und außerhalb der reservierten Zonen;
//      „Zurück“ antippen → Liste sichtbar, Scrollstand der Liste wiederhergestellt (±2 px).
export function g11Static({ owner }) {
  const rec = window.__geo.frame.shellHandle().win(owner);
  const root = rec?.handle?.element?.querySelector('.v4-app-root');
  const out = { fails: [], info: {} };
  if (!root) { out.fails.push('kein .v4-app-root'); return out; }
  const R = root.getBoundingClientRect();
  const shown = (el) => !!el && el.getClientRects().length > 0 && el.getBoundingClientRect().width > 1 && el.getBoundingClientRect().height > 1 && el.checkVisibility({ checkVisibilityCSS: true });
  const desc = (el) => `${el.tagName.toLowerCase()}${typeof el.className === 'string' && el.className.trim() ? `.${el.className.trim().split(/\s+/).slice(0, 2).join('.')}` : ''}`;
  const paneLike = (k) => k.matches('.ctox-pane') || !!k.querySelector(':scope > .ctox-pane-header, :scope > .ctox-pane');
  // a) Pane-Spalten
  const wss = [];
  for (const ws of root.querySelectorAll('.ctox-workspace, [data-resize-frame]')) {
    if (wss.some((o) => o.contains(ws))) continue;
    if ([...ws.children].filter(paneLike).length >= 2) wss.push(ws);
  }
  out.info.workspaces = wss.map((ws) => ({ ws: desc(ws), mode: ws.getAttribute('data-v4-md'), visible: [...ws.children].filter((k) => paneLike(k) && shown(k)).map(desc) }));

  // Sichtbar = im sichtbaren Bereich des Fensters (root). Weitere Panes unter dem Detail (Inspektor) erreicht man durch
  // Scrollen derselben Spalte; sie zählen erst, wenn sie im Bild sind.
  const inView = (el) => { const r = el.getBoundingClientRect(); const ws = el.parentElement.getBoundingClientRect(); const top = Math.max(r.top, R.top, ws.top); const bottom = Math.min(r.bottom, R.bottom, ws.bottom); return bottom - top > 4; };
  for (const ws of wss) {
    const vis = [...ws.children].filter((k) => paneLike(k) && shown(k) && inView(k));
    if (vis.length > 1) out.fails.push(`${vis.length} Panes zugleich sichtbar in ${desc(ws)}: ${vis.map(desc).join(', ')}`);
  }
  // b) Reiter einzeilig
  const clipRight = (el) => {
    let lim = R.right;
    for (let a = el.parentElement; a && a !== root && root.contains(a); a = a.parentElement) {
      if (getComputedStyle(a).overflowX !== 'visible') lim = Math.min(lim, a.getBoundingClientRect().right);
    }
    return lim;
  };
  for (const t of root.querySelectorAll('.ctox-pane-tabs, [role="tablist"]')) {
    if (!shown(t)) continue;
    const kids = [...t.children].filter(shown);
    if (kids.length < 2) continue;
    const tops = kids.map((k) => Math.round(k.getBoundingClientRect().top));
    if (Math.max(...tops) - Math.min(...tops) > 3) out.fails.push(`Reiter mehrzeilig: ${desc(t.parentElement || t)}>${desc(t)} (${[...new Set(tops)].length} Zeilen)`);
    const lim = clipRight(t);
    const cut = kids.filter((k) => k.getBoundingClientRect().right > lim + 1);
    if (cut.length) out.fails.push(`Reiter abgeschnitten: ${desc(t)} ${cut.length} über dem Rand (${cut.slice(0, 3).map((k) => (k.textContent || '').trim().slice(0, 20)).join(', ')})`);
  }
  // c) kein horizontaler Überlauf
  if (root.scrollWidth > root.clientWidth + 1) out.fails.push(`root scrollt horizontal (${root.scrollWidth} > ${root.clientWidth})`);
  return out;
}

// Interaktiv: Ziel für den ersten Tipp (Zeile der Liste) bzw. Zustand nach dem Tipp / nach Zurück.
export function g11Probe({ owner, phase, scrollWant = 60 }) {
  const rec = window.__geo.frame.shellHandle().win(owner);
  const win = rec.handle.element;
  const root = win.querySelector('.v4-app-root');
  const content = win.querySelector('.shell-window-content');
  const shown = (el) => !!el && el.getClientRects().length > 0 && el.getBoundingClientRect().width > 1 && el.getBoundingClientRect().height > 1 && el.checkVisibility({ checkVisibilityCSS: true });
  const ws = root.querySelector('[data-v4-md]');
  const center = (el) => { const r = el.getBoundingClientRect(); return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + Math.min(r.height / 2, 14)) }; };
  if (!ws) return { kit: false };
  const role = (r) => ws.querySelector(`:scope > [data-v4-md-role="${r}"]`);
  const list = role('list'); const detail = role('detail');
  const mode = ws.getAttribute('data-v4-md');
  if (phase === 'list') {
    // Liste leer → das Kit zeigt zuerst das Detail; die Probe misst die Liste dann bei ausgeblendeter Liste nicht.
    // Scrollbarer Bereich der Liste: ein Stück scrollen, damit Zurück den Stand beweisen muss.
    const sc = [list, ...list.querySelectorAll('*')].find((el) => el.scrollHeight > el.clientHeight + 40 && /auto|scroll/.test(getComputedStyle(el).overflowY));
    let scrolled = null;
    if (sc) { sc.scrollTop = Math.min(scrollWant, sc.scrollHeight - sc.clientHeight); sc.setAttribute('data-g11-scroller', ''); scrolled = Math.round(sc.scrollTop); }
    const NOT = '.ctox-pane-header, .ctox-filterbar, .ctox-filter-tray, .ctox-pane-tabs, .ctox-pane-footer, input, select, textarea, label, [data-v4-md-back], [data-v4-tabs-more]';
    // Zeilen bevorzugt nach Kit-Klassen und Rollen, zuletzt alles, was die App als klickbar zeigt (cursor: pointer).
    const PREF = ['.ctox-list-item', '[data-record-id]', '[data-id]', '[role="option"]', 'tbody tr', '[role="row"]', 'li', '[data-key]', '[tabindex]:not([tabindex="-1"])', 'button', 'a', '*'];
    const L = list.getBoundingClientRect();
    let row = null;
    for (const sel of PREF) {
      row = [...list.querySelectorAll(sel)].find((el) => shown(el) && (sel !== '*' || getComputedStyle(el).cursor === 'pointer') && !el.closest(NOT) && !el.querySelector('input[type="checkbox"]:only-child') && el.getBoundingClientRect().top >= L.top && el.getBoundingClientRect().bottom <= L.bottom + 1 && el.getBoundingClientRect().height < 200 && (() => { const c = center(el); const hit = document.elementFromPoint(c.x, c.y); return !!hit && el.contains(hit); })());
      if (row) break;
    }
    // Zweiter Versuch an derselben Zeile: ihr erstes Bedienelement (Kontrollkästchen, Knopf), falls der Tipp auf den Text nichts tut.
    const inner = row && [...row.querySelectorAll('input[type="checkbox"], input[type="radio"], [role="checkbox"], button, a')].find(shown);
    const listEmpty = ![...list.querySelectorAll('.ctox-list-item, [role="option"], [role="row"], [role="treeitem"], tbody tr, li, [data-id], [data-record-id], button, a[href], [role="button"], [tabindex]:not([tabindex="-1"])')].some((el) => !el.closest('.ctox-pane-header, .ctox-filterbar, .ctox-filter-tray, .ctox-pane-tabs, .ctox-pane-footer') && el.getClientRects().length > 0);
    return { kit: true, mode, listEmpty, listShown: shown(list), detailShown: shown(detail), scrolled, row: row ? { ...center(row), text: (row.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 40) } : null, inner: inner ? center(inner) : null };
  }
  const back = ws.querySelector('[data-v4-md-back]');
  const C = content.getBoundingClientRect();
  const cs = getComputedStyle(content);
  const num = (n) => parseFloat(cs.getPropertyValue(n)) || 0;
  let backInZone = false;
  if (shown(back)) {
    const r = back.getBoundingClientRect();
    const iw = num('--shell-icon-inset'), ih = num('--shell-icon-zone-height'), cw = num('--shell-controls-inset'), ch = num('--shell-controls-zone-height');
    backInZone = (r.left < C.left + iw && r.top < C.top + ih) || (r.right > C.right - cw && r.top < C.top + ch);
  }
  const sc = list.querySelector('[data-g11-scroller]') || (list.hasAttribute('data-g11-scroller') ? list : null);
  // Im Detail (Pane jetzt am linken Rand) liegt kein Bedienelement unter Icon-Header oder Knopfcluster (G8 im Detail).
  const zoneHits = [];
  if (phase === 'detail' && shown(detail)) {
    const iw = num('--shell-icon-inset'), ih = num('--shell-icon-zone-height'), cw = num('--shell-controls-inset'), ch = num('--shell-controls-zone-height');
    for (const el of detail.querySelectorAll('button, a[href], input, select, textarea, [role="button"], [tabindex]:not([tabindex="-1"])')) {
      if (!shown(el)) continue;
      const r = el.getBoundingClientRect();
      const inIcon = r.left < C.left + iw - 0.5 && r.right > C.left + 0.5 && r.top < C.top + ih - 0.5 && r.bottom > C.top + 0.5;
      const inCtrl = r.right > C.right - cw + 0.5 && r.left < C.right - 0.5 && r.top < C.top + ch - 0.5 && r.bottom > C.top + 0.5;
      if (inIcon || inCtrl) zoneHits.push(`${el.tagName.toLowerCase()}${el.className && typeof el.className === 'string' ? '.' + el.className.trim().split(/\s+/)[0] : ''} „${(el.textContent || el.getAttribute('aria-label') || '').trim().slice(0, 20)}“ unter ${inIcon ? 'Icon-Header' : 'Knopfcluster'}`);
    }
  }
  // Zurück muss an seinem Mittelpunkt selbst getroffen werden (nichts darüber).
  let backHit = null;
  if (shown(back)) { const c = center(back); const h = document.elementFromPoint(c.x, c.y); backHit = !!h && back.contains(h) ? 'ok' : (h ? `${h.tagName.toLowerCase()}.${String(h.className).split(' ')[0]}` : 'nichts'); }
  return { kit: true, mode, listShown: shown(list), detailShown: shown(detail), backShown: shown(back), backInZone, backHit, back: shown(back) ? center(back) : null, scrollTop: sc ? Math.round(sc.scrollTop) : null, zoneHits };
}
