// Shell v4 – Pane-Kit bei schmaler Breite und Reiterüberlauf (Strang KIT1, Bedienkonzept-Tabelle Abschnitt 4).
//
// Zwei Kit-Regeln für ALLE Apps, ohne App-Mitwirkung (gleicher Lebenszyklus wie pane-heads.js, column-resizers.js):
//
// 1. Reiter einzeilig (Outlook/Salesforce): jede sichtbare `.ctox-pane-tabs` bleibt eine Zeile. Was nicht passt,
//    wandert in „Mehr ▾“ (Knopf am Zeilenende, Menü im Fenster). Der aktive Reiter bleibt immer sichtbar, Zähler bleiben
//    am Reiter (ganze Reiter werden ausgeblendet, nie gekürzt). Eine Menüauswahl klickt den Original-Reiter: die App
//    schaltet wie gewohnt um (ihr Routing bleibt die Quelle der Wahrheit).
//
// 2. Liste → Detail unter 600 px Fensterbreite (iOS Mail/Android Gmail): stehen in einem Workspace
//    (`.ctox-workspace` bzw. `[data-resize-frame]`) von Natur aus mindestens zwei Panes sichtbar, zeigt das Kit EINE
//    Spalte: zuerst die Liste (erste Pane im DOM). Hat die Liste keine Zeilen (leer, lädt noch), zeigt das Kit
//    zuerst das Detail (dort liegen „Neu“/Startinhalte), bis der Nutzer selbst wechselt. Ein Tipp auf eine Zeile der Liste, nach dem die App das Detail neu rendert,
//    öffnet das Detail (zweite Pane, weitere Panes darunter). „Zurück“ steht oben links im Detail-Kopf (pane-heads.js
//    rückt es aus der Icon-Zone, G8) und führt zur Liste, deren Scrollstand wiederhergestellt wird. Die Auswahl der App
//    bleibt unberührt; das Kit wechselt nur die Ansicht. Zeigt die App selbst nur eine Pane (eigene Liste→Detail-
//    Logik, z. B. mail), bleibt das Kit still.
//    Markierung: data-v4-md="list|detail" am Workspace, data-v4-md-role="list|detail|aside" an den Panes;
//    frame.css blendet per @container business-app-window (max-width: 599.98px) aus (Container-Query, kein Viewport).
//
//   const kit = createPaneNarrow();
//   kit.attach(appRoot)   nach mount
//   kit.detach()          vor unmount: Beobachter trennen, Markierungen und eingefügte Knöpfe entfernen
//   kit.measure()         sofort anwenden (Test)

export const NARROW_MAX = 600;
export const MD_ATTR = 'data-v4-md';
export const ROLE_ATTR = 'data-v4-md-role';
export const BACK_ATTR = 'data-v4-md-back';
export const TABS = '.ctox-pane-tabs';
export const TABS_ATTR = 'data-v4-tabs';
export const TAB_HIDDEN = 'data-v4-tab-overflow';
export const MORE_ATTR = 'data-v4-tabs-more';
const WORKSPACE = '.ctox-workspace, [data-resize-frame]';
const ACTIVE = '.is-active, .active, [aria-selected="true"], [aria-pressed="true"], [aria-current]:not([aria-current="false"])';
// Zeilen einer Liste (für die Auswahlmarke). Ein Klick irgendwo in der Liste (auch „+“ im Kopf) kann das Detail öffnen,
// entscheidend ist, ob die App danach das Detail neu rendert. Nie: Suche/Filter, Reiter, Eingaben, Spaltenteiler.
const ROW = '[data-id], [data-record-id], [data-key], [data-row-id], .ctox-list-item, [role="option"], [role="row"], [role="treeitem"], tr, li, [tabindex], a, button';
// Kontrollkästchen in Zeilen (z. B. Kalender ein-/ausblenden) zählen als Auswahl: ändert die App daraufhin das Detail, wird es gezeigt.
const NOT_ARM = '.ctox-filterbar, .ctox-filter-tray, .ctox-pane-tabs, .ctox-column-resizer, input:not([type="checkbox"]):not([type="radio"]), select, textarea, [contenteditable], [data-v4-md-back], [data-v4-tabs-more]';
const OPEN_WINDOW_MS = 2500;
const SELECT_SETTLE_MS = 250;
const SELECTED = '[aria-selected="true"], [aria-current]:not([aria-current="false"]), .is-selected, .is-active, .active, .selected';

const shown = (el) => !!el && el.getClientRects().length > 0 && el.getBoundingClientRect().width > 0;
const paneLike = (k) => k.matches('.ctox-pane') || !!k.querySelector(':scope > .ctox-pane-header, :scope > .ctox-pane');
// Zeilen einer Liste (außerhalb von Kopf, Such-/Filterzeile und Reitern), gemessen bei natürlicher Sichtbarkeit.
const LIST_ROWS = '.ctox-list-item, [role="option"], [role="row"], [role="treeitem"], tbody tr, li, [data-id], [data-record-id]';
// Dazu jedes Bedienelement im Listenkörper (Karten mit Klick, „Anlegen“ im Leerzustand): dann bleibt die Liste zuerst.
const LIST_CONTROLS = `${LIST_ROWS}, button, a[href], [role="button"], [tabindex]:not([tabindex="-1"])`;
const hasRows = (pane) => [...pane.querySelectorAll(LIST_CONTROLS)].some((el) => !el.closest('.ctox-pane-header, .ctox-filterbar, .ctox-filter-tray, .ctox-pane-tabs, .ctox-pane-footer') && shown(el));

export function createPaneNarrow({ raf = (fn) => requestAnimationFrame(fn), caf = (id) => cancelAnimationFrame(id), now = () => performance.now() } = {}) {
  let root = null;
  let ro = null;
  let mo = null;
  let frame = 0;
  let writing = false;
  let menu = null; // {el, close}
  // Zustand je Workspace über seinen Pfad ab root: rendert die App den ganzen Workspace neu, bleibt der Modus erhalten.
  const state = new Map(); // key → {mode, scroll: [{el, path, top}]}
  let armed = null; // {key, ws, at, scroll}

  // ---------------- Reiter ----------------
  // Rechte Grenze für die Reiter: der nächste abschneidende Vorfahr (Pane, Scroller, root) minus der Innenabstände und
  // Rahmen aller Vorfahren dazwischen. Nicht die eigene Box der Reiter: deren Vorfahren können durch die (noch nicht
  // ausgeblendeten) Reiter selbst breiter geworden sein und über die Pane hinausragen.
  function clipRight(el) {
    const edge = (s) => parseFloat(s.paddingRight || 0) + parseFloat(s.borderRightWidth || 0);
    let inset = edge(getComputedStyle(el));
    let limit = Infinity;
    for (let a = el.parentElement; a && root.contains(a); a = a.parentElement) {
      const s = getComputedStyle(a);
      inset += edge(s);
      if (s.overflowX !== 'visible' || a === root || a.matches('.ctox-pane')) limit = Math.min(limit, a.getBoundingClientRect().right - inset);
      if (a === root || a.matches('.ctox-pane')) break;
    }
    return limit;
  }

  function moreButton(tabs) {
    let b = [...tabs.children].find((k) => k.hasAttribute(MORE_ATTR));
    if (!b) {
      b = document.createElement('button');
      b.type = 'button';
      b.className = 'v4-tabs-more';
      b.setAttribute(MORE_ATTR, '');
      b.setAttribute('aria-haspopup', 'menu');
      b.textContent = 'Mehr ▾';
      b.addEventListener('click', (ev) => { ev.preventDefault(); ev.stopPropagation(); openMenu(tabs, b); });
      tabs.append(b);
    } else if (b !== tabs.lastElementChild) tabs.append(b);
    return b;
  }

  function fitTabs(tabs) {
    const tabsOf = () => [...tabs.children].filter((k) => !k.hasAttribute(MORE_ATTR));
    tabs.setAttribute(TABS_ATTR, '');
    for (const k of tabsOf()) k.removeAttribute(TAB_HIDDEN);
    const more = [...tabs.children].find((k) => k.hasAttribute(MORE_ATTR));
    if (more) more.hidden = true;
    if (!shown(tabs)) return;
    const items = tabsOf().filter(shown);
    if (items.length < 2) return;
    const limit = clipRight(tabs);
    const left = tabs.getBoundingClientRect().left;
    const rects = items.map((k) => k.getBoundingClientRect());
    const top0 = rects[0].top;
    if (rects.every((r) => r.right <= limit + 0.5 && Math.abs(r.top - top0) <= 2)) return;
    const b = moreButton(tabs);
    b.hidden = false;
    const gap = parseFloat(getComputedStyle(tabs).columnGap) || 0;
    const moreW = b.getBoundingClientRect().width + gap;
    const widths = rects.map((r) => r.width + gap);
    const startPad = (rects[0].left - left);
    const active = items.findIndex((k) => k.matches(ACTIVE));
    const room = limit - left - startPad - moreW;
    let used = active >= 0 ? widths[active] : 0;
    const keep = new Set(active >= 0 ? [active] : []);
    for (let i = 0; i < items.length; i += 1) {
      if (i === active) continue;
      if (used + widths[i] <= room) { keep.add(i); used += widths[i]; } else break;
    }
    items.forEach((k, i) => { if (!keep.has(i)) k.setAttribute(TAB_HIDDEN, ''); });
  }

  function closeMenu() { menu?.close(); }
  function openMenu(tabs, btn) {
    closeMenu();
    const win = root?.closest('.shell-window') || root;
    if (!win) return;
    const hidden = [...tabs.children].filter((k) => k.hasAttribute(TAB_HIDDEN));
    if (!hidden.length) return;
    const el = document.createElement('div');
    el.className = 'v4-menu v4-tabs-menu';
    el.setAttribute('role', 'menu');
    el.setAttribute('data-v4-tabs-menu', '');
    for (const tab of hidden) {
      const it = document.createElement('button');
      it.type = 'button';
      it.className = 'v4-menu-item';
      it.setAttribute('role', 'menuitem');
      const label = document.createElement('span');
      label.className = 'v4-menu-label';
      label.textContent = (tab.textContent || tab.getAttribute('aria-label') || '').replace(/\s+/g, ' ').trim();
      it.append(label);
      if (tab.disabled) { it.disabled = true; it.classList.add('is-disabled'); }
      it.addEventListener('click', (ev) => { ev.preventDefault(); close(); tab.click(); });
      el.append(it);
    }
    const w = win.getBoundingClientRect();
    const r = btn.getBoundingClientRect();
    el.style.top = `${Math.round(r.bottom - w.top + 4)}px`;
    el.style.right = `${Math.max(4, Math.round(w.right - r.right))}px`;
    win.append(el);
    btn.setAttribute('aria-expanded', 'true');
    const onDown = (ev) => { if (!el.contains(ev.target) && ev.target !== btn) close(); };
    const onKey = (ev) => { if (ev.key === 'Escape') { ev.preventDefault(); close(); btn.focus(); } };
    document.addEventListener('pointerdown', onDown, true);
    el.addEventListener('keydown', onKey);
    function close() {
      document.removeEventListener('pointerdown', onDown, true);
      el.remove();
      btn.setAttribute('aria-expanded', 'false');
      if (menu?.el === el) menu = null;
    }
    menu = { el, close };
    el.querySelector('button:not(:disabled)')?.focus();
  }

  // ---------------- Liste → Detail ----------------
  const narrowNow = () => {
    const box = root.closest('.shell-window-content') || root;
    const w = box.getBoundingClientRect().width;
    return w > 0 && w < NARROW_MAX;
  };
  function workspaces() {
    const out = [];
    for (const ws of root.querySelectorAll(WORKSPACE)) {
      if (out.some((o) => o.contains(ws))) continue;
      if ([...ws.children].filter(paneLike).length >= 2) out.push(ws);
    }
    return out;
  }
  function clearWs(ws) {
    ws.removeAttribute(MD_ATTR);
    for (const k of ws.children) k.removeAttribute(ROLE_ATTR);
  }
  function backButton(pane) {
    const head = pane.matches('.ctox-pane-header') ? pane : pane.querySelector('.ctox-pane-header');
    const host = head?.querySelector(':scope > .ctox-pane-title-row') || head || pane;
    let b = host.querySelector(`:scope > [${BACK_ATTR}]`);
    if (b && b === host.firstElementChild) return b;
    for (const old of pane.querySelectorAll(`[${BACK_ATTR}]`)) old.remove();
    b = document.createElement('button');
    b.type = 'button';
    b.className = 'v4-md-back';
    b.setAttribute(BACK_ATTR, '');
    b.setAttribute('aria-label', 'Zurück zur Liste');
    b.textContent = '‹ Zurück';
    b.addEventListener('click', (ev) => {
      ev.preventDefault(); ev.stopPropagation();
      const ws = pane.parentElement;
      if (ws) setMode(ws, 'list');
    });
    host.prepend(b);
    return b;
  }
  // Scrollstand der Liste: Element und sein Pfad ab der Pane (Kindindizes). Rendert die App die Liste neu, findet der
  // Pfad das neue Element.
  function pathOf(pane, el) {
    const path = [];
    for (let e = el; e && e !== pane; e = e.parentElement) path.unshift([...e.parentElement.children].indexOf(e));
    return path;
  }
  function byPath(pane, path) {
    let e = pane;
    for (const i of path) { e = e?.children[i]; if (!e) return null; }
    return e;
  }
  function saveScroll(pane) {
    const out = [];
    for (const el of [pane, ...pane.querySelectorAll('*')]) if (el.scrollTop > 0) out.push({ el, path: pathOf(pane, el), top: el.scrollTop });
    return out;
  }
  const keyOf = (ws) => pathOf(root, ws).join('/');
  function setMode(ws, mode, snapshot = null) {
    const key = keyOf(ws);
    const st = state.get(key) || { mode: 'list', scroll: [], auto: true };
    if (st.mode === mode && ws.getAttribute(MD_ATTR) === mode) return;
    const list = [...ws.children].find((k) => k.getAttribute(ROLE_ATTR) === 'list');
    if (mode === 'detail' && list) st.scroll = snapshot || saveScroll(list);
    st.mode = mode;
    st.auto = false;
    state.set(key, st);
    writing = true;
    ws.setAttribute(MD_ATTR, mode);
    writing = false;
    if (mode === 'list') {
      const restore = () => { for (const x of st.scroll) { const el = list && (list.contains(x.el) ? x.el : byPath(list, x.path)); if (el) el.scrollTop = x.top; } };
      restore();
      raf(restore);
    } else {
      const detail = [...ws.children].find((k) => k.getAttribute(ROLE_ATTR) === 'detail');
      if (detail) detail.scrollTop = 0;
    }
  }
  function applyWs(ws, narrow) {
    if (!narrow) { clearWs(ws); return; }
    // Natürliche Sichtbarkeit (ohne Kit-Ausblendung): Markierung kurz entfernen, in EINEM Layout lesen.
    writing = true;
    ws.removeAttribute(MD_ATTR);
    // Reihenfolge im DOM: die erste Pane ist die Liste (aside/rail), auch wenn die App gestapelte Panes umordnet.
    const panes = [...ws.children].filter((k) => paneLike(k) && shown(k));
    writing = false;
    if (panes.length < 2) {
      writing = true; clearWs(ws); writing = false;
      for (const b of ws.querySelectorAll(`[${BACK_ATTR}]`)) b.remove();
      return;
    }
    const key = keyOf(ws);
    // auto: der Nutzer hat noch nicht gewechselt → Liste, wenn sie Zeilen hat, sonst Detail.
    const st = state.get(key) || { mode: 'list', scroll: [], auto: true };
    state.set(key, st);
    if (st.auto) st.mode = hasRows(panes[0]) ? 'list' : 'detail';
    writing = true;
    for (const k of ws.children) {
      const i = panes.indexOf(k);
      const role = i === 0 ? 'list' : i === 1 ? 'detail' : i > 1 ? 'aside' : null;
      if (role) { if (k.getAttribute(ROLE_ATTR) !== role) k.setAttribute(ROLE_ATTR, role); } else k.removeAttribute(ROLE_ATTR);
    }
    backButton(panes[1]);
    ws.setAttribute(MD_ATTR, st.mode);
    writing = false;
    // Die App hat den ganzen Workspace nach dem Tipp neu gerendert: das ist ihre Antwort auf die Auswahl.
    if (armed && armed.key === key && armed.ws !== ws && !armed.ws.isConnected && now() - armed.at <= OPEN_WINDOW_MS) {
      const { scroll } = armed; armed = null;
      setMode(ws, 'detail', scroll);
    }
  }

  function measure() {
    frame = 0;
    if (!root) return;
    for (const t of root.querySelectorAll(TABS)) fitTabs(t);
    const narrow = narrowNow();
    const wss = workspaces();
    for (const ws of wss) applyWs(ws, narrow);
    for (const ws of root.querySelectorAll(`[${MD_ATTR}]`)) if (!wss.includes(ws)) clearWs(ws);
  }
  function schedule() { if (!frame && root) frame = raf(measure); }

  function onClick(ev) {
    if (!narrowNow()) return;
    const t = ev.target;
    if (!(t instanceof Element)) return;
    const pane = t.closest(`[${ROLE_ATTR}="list"]`);
    const ws = pane?.parentElement;
    if (!pane || ws?.getAttribute(MD_ATTR) !== 'list' || t.closest(NOT_ARM)) return;
    // Scrollstand VOR der Verarbeitung durch die App sichern: rendert sie die Liste neu, ist er danach weg.
    const mine = { key: keyOf(ws), ws, at: now(), scroll: saveScroll(pane) };
    armed = mine;
    // Tipp auf eine Zeile, die (schon) ausgewählt ist, ohne dass sich das Detail ändert (z. B. die vorausgewählte
    // Zeile): die Auswahlmarke der App an der Zeile entscheidet, kurz nachdem die App den Klick verarbeitet hat.
    const row = t.closest('.ctox-pane-header') ? null : t.closest(ROW);
    if (!row || !pane.contains(row) || row === pane) return;
    const rowPath = pathOf(pane, row);
    setTimeout(() => {
      if (armed !== mine || !ws.isConnected || ws.getAttribute(MD_ATTR) !== 'list') return;
      const cur = row.isConnected ? row : byPath(pane, rowPath);
      if (cur?.matches(SELECTED)) { armed = null; setMode(ws, 'detail', mine.scroll); }
    }, SELECT_SETTLE_MS);
  }
  function onMutations(records) {
    if (writing) return;
    let relevant = false;
    for (const rec of records) {
      const n = rec.target.nodeType === 1 ? rec.target : rec.target.parentElement;
      if (!n) continue;
      if (n.closest?.('[data-v4-tabs-menu]')) continue;
      // Eigene Schreibvorgänge (Markierungen, Knöpfe) lösen nichts aus.
      if (rec.type === 'attributes' && (n.hasAttribute(MORE_ATTR) || n.hasAttribute(BACK_ATTR))) continue;
      const own = [...rec.addedNodes, ...rec.removedNodes].every((x) => x.nodeType === 1 && (x.hasAttribute?.(BACK_ATTR) || x.hasAttribute?.(MORE_ATTR)));
      if (rec.type === 'childList' && (rec.addedNodes.length || rec.removedNodes.length) && own) continue;
      relevant = true;
      // Inhalt des Details geändert (nicht nur Stil, den z. B. pane-heads.js setzt) → Detail zeigen.
      if (armed && rec.attributeName !== 'style' && now() - armed.at <= OPEN_WINDOW_MS && armed.ws.isConnected) {
        const detail = n.closest?.(`[${ROLE_ATTR}="detail"], [${ROLE_ATTR}="aside"]`);
        if (detail && detail.parentElement === armed.ws) { const { ws, scroll } = armed; armed = null; setMode(ws, 'detail', scroll); }
      }
    }
    if (relevant) schedule();
  }

  return {
    attach(appRoot) {
      this.detach();
      root = appRoot;
      if (!root) return;
      if (typeof ResizeObserver === 'function') {
        ro = new ResizeObserver(schedule);
        ro.observe(root.closest('.shell-window-content') || root);
        ro.observe(root);
      }
      mo = new MutationObserver(onMutations);
      mo.observe(root, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ['class', 'hidden', 'style', 'aria-selected', 'aria-pressed', 'aria-current', 'open'] });
      root.addEventListener('click', onClick, true);
      measure();
    },
    detach() {
      closeMenu();
      ro?.disconnect(); mo?.disconnect();
      ro = null; mo = null;
      if (frame) caf(frame);
      frame = 0;
      armed = null;
      if (root) {
        root.removeEventListener('click', onClick, true);
        for (const el of root.querySelectorAll(`[${BACK_ATTR}], [${MORE_ATTR}]`)) el.remove();
        for (const el of root.querySelectorAll(`[${TAB_HIDDEN}]`)) el.removeAttribute(TAB_HIDDEN);
        for (const el of root.querySelectorAll(`[${TABS_ATTR}]`)) el.removeAttribute(TABS_ATTR);
        for (const el of root.querySelectorAll(`[${MD_ATTR}]`)) clearWs(el);
      }
      root = null;
    },
    measure,
    mode: (ws) => (root ? state.get(keyOf(ws))?.mode : null) || null,
  };
}
