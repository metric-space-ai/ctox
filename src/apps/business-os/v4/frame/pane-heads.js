// Shell v4 – Spaltenköpfe eines Fensters auf gleicher Höhe (Strang VIS4, Shell-V2-Regel).
//
// Pane-Köpfe (.ctox-pane-header) dürfen mit langem Titel wachsen (Umbruch statt Ellipse, G4). Damit die Kopfbänder
// der Spalten EINES Fensters trotzdem auf einer Linie enden, bestimmt der höchste Kopf die Höhe seiner Geschwister:
// Geschwister sind alle sichtbaren Köpfe im App-Inhalt, deren Oberkante auf derselben Höhe liegt (±2 px, also
// nebeneinanderstehende Spalten). Der Rahmen misst die natürliche Höhe jedes Kopfes (ohne eigene Angleichung), setzt
// je Gruppe mit mindestens zwei Köpfen `--v4-pane-head-h` am Kopf plus `data-v4-pane-row`; theme.css (Kit-Regel aus
// build-theme.mjs) macht daraus `min-height`. Generisch, keine App-Sonderfälle, keine App-Mitwirkung.
//
//   const sync = createPaneHeadSync();
//   sync.attach(appRoot)   nach mount: messen, Größen- und Inhaltsänderungen beobachten
//   sync.detach()          vor unmount: Beobachter trennen, Markierungen entfernen
//   sync.measure()         sofort angleichen (Test)
//
// Auslöser: ResizeObserver auf root und jeden Kopf (Breitenänderung → anderer Umbruch), MutationObserver für neue
// Köpfe und Textänderungen in Köpfen (ein kürzerer Titel schrumpft nicht von selbst unter die gesetzte Mindesthöhe).
// Gemessen wird gebündelt im nächsten Animationsbild; ändert sich nichts, wird nichts geschrieben (keine Schleife).

export const PANE_HEAD = '.ctox-pane-header';
export const ROW_ATTR = 'data-v4-pane-row';
export const HEIGHT_VAR = '--v4-pane-head-h';
export const ICON_ATTR = 'data-v4-under-icon';
export const KIT_ROWS = '.ctox-filterbar';
export const CTRL_ATTR = 'data-v4-under-controls';
// Icon + Wort (Owner-Regel workjet#801): ob Kurzwörter in eine Zeile passen, entscheidet shared/pane-icon-labels.js
// (data-icons-only). Nur der Rahmen kennt die reservierten Zonen: ragt ein Wortknopf in die Zone unter dem App-Icon
// oder dem Knopfcluster, setzt er am Kopf data-v4-icons-only (frame.css: nur Icons). Je Messung neu entschieden.
export const ICONS_ONLY_ATTR = 'data-v4-icons-only';
const LABELLED = '.ctox-pane-icon.has-label';
const TOP_TOLERANCE = 2;
const SIB_ROW_MAX = 64;
const INTERACTIVE = 'button, a, input, select, textarea, [role="button"], [tabindex]';

export function createPaneHeadSync({ raf = (fn) => requestAnimationFrame(fn), caf = (id) => cancelAnimationFrame(id) } = {}) {
  let root = null;
  let ro = null;
  let mo = null;
  let frame = 0;
  const observed = new Set();

  function clear(el) {
    el.removeAttribute(ROW_ATTR);
    el.style.removeProperty(HEIGHT_VAR);
  }

  // Wortknöpfe nur, wenn sie passen (s. ICONS_ONLY_ATTR). Gemessen wird mit Wörtern (Markierung vorher entfernt); die
  // Einrückungen aus markReserved der letzten Messung gelten dabei weiter. Kein Beobachter reagiert auf das Attribut.
  function fitLabels() {
    const rows = [...root.querySelectorAll(`${PANE_HEAD}, ${KIT_ROWS}`)].filter((el) => el.querySelector(LABELLED));
    if (!rows.length) return;
    for (const el of rows) el.removeAttribute(ICONS_ONLY_ATTR);
    const win = root.closest?.('.shell-window');
    const content = root.closest?.('.shell-window-content') || root;
    const zones = win ? zoneRects(win, content) : [];
    if (!zones.length) return;
    for (const el of rows) {
      const hit = [...el.querySelectorAll(LABELLED)].some((b) => {
        const br = b.getBoundingClientRect();
        return br.width > 0 && zones.some((z) => br.left < z.right && br.right > z.left && br.top < z.bottom && br.bottom > z.top);
      });
      if (hit) el.setAttribute(ICONS_ONLY_ATTR, '');
    }
  }

  function measure() {
    frame = 0;
    if (!root) return;
    fitLabels();
    const heads = [...root.querySelectorAll(PANE_HEAD)];
    for (const el of observed) if (!heads.includes(el) || !root.contains(el)) { ro?.unobserve(el); observed.delete(el); }
    for (const el of heads) if (!observed.has(el)) { ro?.observe(el); observed.add(el); }
    // Natürliche Höhen: Angleichung kurz aufheben, alles in EINEM Layout lesen, danach gezielt wieder setzen.
    const prev = new Map(heads.map((el) => [el, el.style.getPropertyValue(HEIGHT_VAR)]));
    for (const el of heads) el.removeAttribute(ROW_ATTR);
    const boxes = heads.map((el) => {
      const r = el.getBoundingClientRect();
      return { el, top: r.top, h: r.height, shown: r.width > 0 && r.height > 0 && el.getClientRects().length > 0 };
    });
    const groups = [];
    for (const b of boxes.filter((x) => x.shown).sort((a, c) => a.top - c.top)) {
      const g = groups.find((x) => Math.abs(x.top - b.top) <= TOP_TOLERANCE);
      if (g) g.items.push(b); else groups.push({ top: b.top, items: [b] });
    }
    const want = new Map();
    for (const g of groups) {
      if (g.items.length < 2) continue;
      const max = Math.ceil(Math.max(...g.items.map((x) => x.h)));
      for (const x of g.items) want.set(x.el, `${max}px`);
    }
    for (const el of heads) {
      const v = want.get(el);
      if (!v) { clear(el); continue; }
      if (prev.get(el) !== v) el.style.setProperty(HEIGHT_VAR, v);
      el.setAttribute(ROW_ATTR, '');
    }
    markReserved(boxes);
  }

  // Reservierte Zonen (FR6a, Owner 15.10.): unter dem schwebenden App-Icon (oben links) und dem schwebenden
  // Knopfcluster (oben rechts) liegen keine Bedienelemente der App. Eingerückt wird nur, was das Zonen-Rechteck
  // tatsächlich schneidet: die direkten Kinder eines Pane-Kopfs (Zeilen bzw. einzelne Bedienelemente). Eine zweite
  // Kopfzeile unterhalb der Zonenhöhe (z. B. das Suchfeld) bleibt in voller Breite. Markierung data-v4-under-icon /
  // data-v4-under-controls = 'pad' (Zeile/Container: Innenabstand) bzw. 'margin' (einzelnes Bedienelement: Außenabstand);
  // frame.css rückt um --shell-icon-inset bzw. --shell-controls-inset ein. Gemessen wird ohne Einrückung (Markierungen
  // kurz entfernt, ein Layout) – keine Rückkopplung.
  function markReserved(boxes) {
    const win = root.closest?.('.shell-window');
    if (!win) return;
    const cs = getComputedStyle(root);
    const iconH = parseFloat(cs.getPropertyValue('--shell-icon-zone-height'));
    const ctrlH = parseFloat(cs.getPropertyValue('--shell-controls-zone-height'));
    const iconW = parseFloat(cs.getPropertyValue('--shell-icon-inset'));
    const ctrlW = parseFloat(cs.getPropertyValue('--shell-controls-inset'));
    if (!(iconH > 0) && !(ctrlH > 0)) return;
    const content = root.closest('.shell-window-content') || root;
    const c = content.getBoundingClientRect();
    const hit = (r, x0, x1, h) => r.width > 0 && r.height > 0 && r.top < c.top + h && r.bottom > c.top && r.left < x1 && r.right > x0;
    const kids = [];
    // Kit-Zeilen außerhalb des Pane-Kopfs (Such-/Filterzeile .ctox-filterbar als Geschwister des Kopfs) zählen selbst.
    for (const k of root.querySelectorAll(KIT_ROWS)) {
      const prevI = k.getAttribute(ICON_ATTR); const prevC = k.getAttribute(CTRL_ATTR);
      kids.push({ k, prevI, prevC, show: k.getClientRects().length > 0 });
      k.removeAttribute(ICON_ATTR); k.removeAttribute(CTRL_ATTR);
    }
    for (const b of boxes) {
      for (const k of b.el.children) {
        if (k.matches(KIT_ROWS)) continue;
        const prevI = k.getAttribute(ICON_ATTR); const prevC = k.getAttribute(CTRL_ATTR);
        kids.push({ k, prevI, prevC, show: b.shown });
        k.removeAttribute(ICON_ATTR); k.removeAttribute(CTRL_ATTR);
      }
      // Zeilen direkt unter dem Kopf (Werkzeug-/Reiterleiste als Geschwister, Strang KIT1): steht eine Pane am linken
      // Rand (schmale Breite, Liste→Detail), reichen sie in die Zone. Nur flache Zeilen (≤ SIB_ROW_MAX px), keine Panes,
      // keine Inhaltsflächen.
      for (let k = b.el.nextElementSibling; k; k = k.nextElementSibling) {
        if (k.matches(KIT_ROWS) || k.matches(PANE_HEAD) || k.matches('.ctox-pane')) continue;
        const prevI = k.getAttribute(ICON_ATTR); const prevC = k.getAttribute(CTRL_ATTR);
        k.removeAttribute(ICON_ATTR); k.removeAttribute(CTRL_ATTR);
        const r = k.getBoundingClientRect();
        if (r.height > SIB_ROW_MAX) break;
        if (!k.matches(INTERACTIVE) && !k.querySelector(INTERACTIVE)) continue;
        kids.push({ k, prevI, prevC, show: b.shown });
      }
    }
    // Rechteck einer Zeile samt ihrer direkten Kinder: ein Bedienelement kann über die Zeile hinausragen (z. B. 44-px-
    // Reiter in einer 37-px-Leiste).
    const boxOf = (el) => {
      const a = el.getBoundingClientRect();
      let { left, top, right, bottom } = a;
      for (const c of el.children) {
        const b = c.getBoundingClientRect();
        if (!(b.width > 0 && b.height > 0)) continue;
        left = Math.min(left, b.left); top = Math.min(top, b.top); right = Math.max(right, b.right); bottom = Math.max(bottom, b.bottom);
      }
      return { left, top, right, bottom, width: right - left, height: bottom - top };
    };
    for (const x of kids) {
      const r = boxOf(x.k);
      const kind = x.k.matches('button, a, input, select, textarea, [role="button"], [tabindex]') ? 'margin' : 'pad';
      const i = x.show && iconW > 0 && hit(r, c.left, c.left + iconW, iconH) ? kind : null;
      const ct = x.show && ctrlW > 0 && hit(r, c.right - ctrlW, c.right, ctrlH) ? kind : null;
      x.nextI = i; x.nextC = ct;
    }
    for (const x of kids) {
      if (x.nextI) x.k.setAttribute(ICON_ATTR, x.nextI);
      if (x.nextC) x.k.setAttribute(CTRL_ATTR, x.nextC);
    }
  }

  function schedule() {
    if (!frame && root) frame = raf(measure);
  }

  const inHead = (n) => {
    const el = n?.nodeType === 1 ? n : n?.parentElement;
    return !!el && (el.matches?.(PANE_HEAD) || !!el.closest?.(PANE_HEAD) || !!el.querySelector?.(PANE_HEAD));
  };

  return {
    attach(appRoot) {
      this.detach();
      root = appRoot;
      if (!root) return;
      if (typeof ResizeObserver === 'function') {
        ro = new ResizeObserver(schedule);
        ro.observe(root);
      }
      mo = new MutationObserver((records) => {
        if (frame) return;
        for (const rec of records) {
          if (inHead(rec.target)) { schedule(); return; }
          for (const n of rec.addedNodes) if (inHead(n)) { schedule(); return; }
          for (const n of rec.removedNodes) if (n.nodeType === 1 && (n.matches?.(PANE_HEAD) || n.querySelector?.(PANE_HEAD))) { schedule(); return; }
        }
      });
      mo.observe(root, { childList: true, subtree: true, characterData: true });
      measure();
    },
    detach() {
      ro?.disconnect();
      mo?.disconnect();
      ro = null;
      mo = null;
      if (frame) caf(frame);
      frame = 0;
      for (const el of observed) clear(el);
      observed.clear();
      for (const k of root?.querySelectorAll?.(`[${ICON_ATTR}], [${CTRL_ATTR}], [${ICONS_ONLY_ATTR}]`) || []) { k.removeAttribute(ICON_ATTR); k.removeAttribute(CTRL_ATTR); k.removeAttribute(ICONS_ONLY_ATTR); }
      root = null;
    },
    measure,
  };
}

// ---------- Allgemeine Zonen-Garantie (Strang ZONE2, K24 / Befund T3-1) ----------
// Die Kit-Mechanik oben rückt nur Pane-Köpfe aus. Eine App OHNE Kit (z. B. eine fremde App mit eigenem <form> oben
// rechts) legt ihre Knöpfe sonst unter das schwebende Chrome. Der Rahmen garantiert daher für JEDE App: schneidet ein
// interaktives Element (GUARD_INTERACTIVE) außerhalb eines Kit-Kopfs das Rechteck einer Zone, bekommt die Wurzel
// data-v4-zone-guard="top" (frame.css: Content-Box beginnt unter --shell-zone-height); schneidet danach noch etwas,
// zusätzlich "right" (Einrückung um --shell-controls-inset). Kit-Apps schneiden nichts → kein Attribut → pixelgleich.
//
//   const guard = createZoneGuard({ pick: (scope) => scope.querySelector(':scope > .v4-app-root') });
//   guard.attach(scope)   scope liegt in einem .shell-window; beobachtet Größe (ResizeObserver) und DOM (MutationObserver)
//   guard.detach()        Beobachter trennen (Attribut bleibt bis zum Abbau des Elements)
//   guard.measure()       sofort prüfen (Test)
//
// Messung ohne Rückkopplung: das eigene Attribut wird kurz entfernt, die natürliche Lage in EINEM Layout gelesen und
// im selben Task wieder gesetzt (kein Bild dazwischen → kein Flackern). Gleiche Lage → gleiches Ergebnis; zurück-
// genommen wird nur, wenn in der natürlichen Lage nichts mehr schneidet. Scrollversatz zählt nicht: geprüft wird die
// Ruhelage (Rechteck + scrollTop/scrollLeft der Vorfahren), sonst entschiede die Scrollposition über die Einrückung.
// Unsichtbare Wurzel (Code-Modus: visibility hidden) → keine Messung, Zustand bleibt. Höchstens eine Messung je Bild.
export const GUARD_ATTR = 'data-v4-zone-guard';
export const GUARD_INTERACTIVE = 'button, a[href], input, select, textarea, summary, [role="button"], [tabindex]';
const GUARD_NATIVE = 'button, a[href], input, select, textarea, summary, [role="button"]';
const GUARD_KIT = `${PANE_HEAD}, ${KIT_ROWS}, [${ICON_ATTR}], [${CTRL_ATTR}]`;
const GUARD_ATTRS = ['class', 'style', 'hidden', 'open'];

export function zoneRects(win, content) {
  const cs = getComputedStyle(content);
  const num = (n) => parseFloat(cs.getPropertyValue(n));
  const iconH = num('--shell-icon-zone-height'); const ctrlH = num('--shell-controls-zone-height');
  const iconW = num('--shell-icon-inset'); const ctrlW = num('--shell-controls-inset');
  const c = content.getBoundingClientRect();
  const zones = [];
  if (iconH > 0 && iconW > 0) zones.push({ kind: 'icon', left: c.left, top: c.top, right: c.left + iconW, bottom: c.top + iconH });
  if (ctrlH > 0 && ctrlW > 0) zones.push({ kind: 'controls', left: c.right - ctrlW, top: c.top, right: c.right, bottom: c.top + ctrlH });
  return zones;
}

// Interaktive Elemente unter `root`, die eine Zone in Ruhelage schneiden (ohne Kit-Köpfe, die pane-heads.js behandelt).
export function zoneHits(root, zones) {
  if (!zones.length) return [];
  const band = Math.max(...zones.map((z) => z.bottom));
  // Kit-Zeilen wie in markReserved: flache Geschwister direkt unter einem Pane-Kopf gehören dem Kit (unabhängig davon,
  // ob pane-heads.js sie in diesem Bild schon markiert hat – sonst entschiede die Reihenfolge der Messungen).
  const kitRows = new Set();
  for (const head of root.querySelectorAll(PANE_HEAD)) {
    for (let k = head.nextElementSibling; k; k = k.nextElementSibling) {
      if (k.matches(KIT_ROWS) || k.matches(PANE_HEAD) || k.matches('.ctox-pane')) continue;
      if (k.getBoundingClientRect().height > SIB_ROW_MAX) break;
      kitRows.add(k);
    }
  }
  const out = [];
  for (const el of root.querySelectorAll(GUARD_INTERACTIVE)) {
    if (!el.matches(GUARD_NATIVE) && !(el.tabIndex >= 0)) continue;
    const r = el.getBoundingClientRect();
    if (!(r.width > 0 && r.height > 0) || r.top >= band) continue;
    const kit = el.closest(GUARD_KIT);
    if (kit && root.contains(kit)) continue;
    if (el.closest('[inert]')) continue;
    let inKitRow = false;
    for (let a = el; a && a !== root; a = a.parentElement) if (kitRows.has(a)) { inKitRow = true; break; }
    if (inKitRow) continue;
    let dx = 0; let dy = 0;
    for (let p = el.parentElement; p && p !== root.parentElement; p = p.parentElement) { dy += p.scrollTop; dx += p.scrollLeft; }
    const top = r.top + dy; const bottom = r.bottom + dy; const left = r.left + dx; const right = r.right + dx;
    if (zones.some((z) => top < z.bottom && bottom > z.top && left < z.right && right > z.left)) out.push(el);
  }
  return out;
}

export function createZoneGuard({ pick = (scope) => scope, raf = (fn) => requestAnimationFrame(fn), caf = (id) => cancelAnimationFrame(id) } = {}) {
  let scope = null;
  let root = null;
  let ro = null;
  let mo = null;
  let frame = 0;

  function measure() {
    frame = 0;
    if (!scope) return;
    const next = pick(scope) || null;
    if (next !== root) {
      if (root) { ro?.unobserve(root); root.removeAttribute(GUARD_ATTR); }
      root = next;
      if (root) ro?.observe(root);
    }
    if (!root || !root.isConnected) return;
    const win = scope.closest('.shell-window');
    if (!win) return;
    if (root.checkVisibility && !root.checkVisibility({ visibilityProperty: true })) return;
    const content = win.querySelector(':scope > .shell-window-content') || scope;
    const prev = root.getAttribute(GUARD_ATTR);
    if (prev !== null) root.removeAttribute(GUARD_ATTR);
    const zones = zoneRects(win, content);
    let want = '';
    if (zoneHits(root, zones).length) {
      root.setAttribute(GUARD_ATTR, 'top');
      want = zoneHits(root, zones).length ? 'top right' : 'top';
    }
    if (want) { if (root.getAttribute(GUARD_ATTR) !== want) root.setAttribute(GUARD_ATTR, want); } else root.removeAttribute(GUARD_ATTR);
  }

  function schedule() {
    if (!frame && scope) frame = raf(measure);
  }

  return {
    attach(el) {
      this.detach();
      scope = el;
      if (!scope) return;
      if (typeof ResizeObserver === 'function') {
        ro = new ResizeObserver(schedule);
        ro.observe(scope);
      }
      // Neue Wurzel (Remount, Versionswechsel): Zustand der alten sofort übernehmen (Mikrotask, vor dem nächsten Bild),
      // die Messung im nächsten Bild bestätigt oder nimmt ihn zurück – sonst zeigte der Remount ein Bild ohne Einrückung.
      mo = new MutationObserver(() => {
        const nr = scope ? pick(scope) : null;
        const st = root?.getAttribute(GUARD_ATTR);
        if (nr && nr !== root && st && !nr.hasAttribute(GUARD_ATTR)) nr.setAttribute(GUARD_ATTR, st);
        schedule();
      });
      mo.observe(scope, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: GUARD_ATTRS });
      measure();
    },
    detach() {
      ro?.disconnect();
      mo?.disconnect();
      ro = null;
      mo = null;
      if (frame) caf(frame);
      frame = 0;
      // Attribut bleibt: detach läuft beim Schließen bzw. Shell-Tausch, während das alte Fenster noch sichtbar ist
      // (Doppelpuffer) – ein Zurücknehmen ließe den Inhalt im letzten Bild unter die Zonen springen.
      root = null;
      scope = null;
    },
    measure,
    get state() { return root?.getAttribute(GUARD_ATTR) || ''; },
  };
}
