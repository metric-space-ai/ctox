// Desktop (FR6d): eigene Fläche hinter den Fenstern wie die Shell-V2 (Desktop-App modules/desktop: .desktop-module >
// .desktop-surface > .desktop-icons > .desktop-icon mit 80-px-Bild und Beschriftung). CSS wörtlich aus
// modules/desktop/index.css und app.css über shell-v2-desktop.css (lab/extract-desktop-css.mjs); [data-app="desktop"]
// heißt dort [data-v4-desktop-v2], denn [data-app] gehört im v4-Rahmen allein dem Inhalts-root einer App.
// Raster wie modules/desktop/index.js gridPosition: Zelle 120 × 140, Rand 24; ab ≤ 767 px legt die Shell-V2-CSS die
// Symbole selbst in ein Raster (88 × 132), die Inline-Position gilt dort nicht.
// Ein Klick startet die App (Shell-V2 iconDrag onClick → activateIcon). Ziehen auf die Reiterleiste heftet an
// (DRAG_APP); ein Reiter-Pin, der auf den Desktop gezogen wird (DRAG_PIN), wird gelöst (onUnpin).
// Lücke: Die Shell-V2 speichert Symbolpositionen, Ordner und Dateien in den Collections der Desktop-App; der v4-Kernel
// liefert davon nichts an den Rahmen. Der Desktop zeigt daher die Apps aus kernel.apps in launcher_order.
import { DRAG_APP, DRAG_PIN } from './launcher.js';

const GRID = { cellW: 120, cellH: 140, offset: 24 };
const COLLATOR = new Intl.Collator(undefined, { sensitivity: 'base', numeric: true });

export function desktopMarkup() {
  return '<div class="v4f-desktop-app" data-v4-desktop-v2><section class="desktop-module"><main class="desktop-surface" data-v4-desktop-surface tabindex="-1"><div class="desktop-icons" data-v4-desktop-icons></div></main></section></div>';
}

export function desktopIconMarkup() {
  return '<div class="desktop-icon-glyph" aria-hidden="true" data-v4-icon></div><div class="desktop-icon-label" data-v4-title></div>';
}

export function createDesktop({ host, before = null, t, iconFor, onOpen, order = [], onUnpin = null }) {
  const el = document.createElement('div');
  el.className = 'v4f-desktop';
  el.dataset.v4Desktop = '';
  el.dataset.v4V2 = '';
  el.dataset.shellStyle = 'ctox';
  el.innerHTML = desktopMarkup();
  host.insertBefore(el, before);
  const surface = el.querySelector('[data-v4-desktop-surface]');
  const iconsEl = el.querySelector('[data-v4-desktop-icons]');
  iconsEl.setAttribute('aria-label', t('desktopIcons', 'Desktop-Symbole'));

  const icons = new Map(); // app_id -> .desktop-icon
  let apps = [];
  let rank = new Map();
  const setRank = (ids) => { rank = new Map((ids || []).map((id, i) => [String(id), i])); };
  setRank(order);
  const byRank = (a, b) => {
    const ra = rank.has(a.id) ? rank.get(a.id) : Infinity;
    const rb = rank.has(b.id) ? rank.get(b.id) : Infinity;
    if (ra !== rb) return ra - rb;
    return COLLATOR.compare(String(a.title || a.id), String(b.title || b.id));
  };

  function iconFor2(app, slot) {
    iconFor(app, slot);
    slot.querySelector('img')?.classList.add('workjet-app-raster-icon');
  }

  function iconEl(app) {
    let icon = icons.get(app.id);
    if (!icon) {
      icon = document.createElement('div');
      icon.className = 'desktop-icon';
      icon.dataset.v4DeskApp = app.id;
      icon.dataset.target = app.id;
      icon.setAttribute('role', 'button');
      icon.tabIndex = 0;
      icon.innerHTML = desktopIconMarkup();
      const run = (event) => onOpen(icon.dataset.v4DeskApp, { anchor: icon.querySelector('[data-v4-icon]'), event });
      icon.addEventListener('click', run);
      icon.addEventListener('keydown', (event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); run(event); } });
      icon.draggable = true;
      icon.addEventListener('dragstart', (event) => {
        event.dataTransfer.effectAllowed = 'copy';
        event.dataTransfer.setData(DRAG_APP, icon.dataset.v4DeskApp);
        event.dataTransfer.setData('text/plain', icon.dataset.v4DeskApp);
        icon.classList.add('dragging');
      });
      icon.addEventListener('dragend', () => icon.classList.remove('dragging'));
      icons.set(app.id, icon);
    }
    const title = String(app.title || app.id);
    const label = icon.querySelector('[data-v4-title]');
    if (label.textContent !== title) label.textContent = title;
    icon.title = title;
    icon.setAttribute('aria-label', title);
    iconFor2(app, icon.querySelector('[data-v4-icon]'));
    return icon;
  }

  // modules/desktop/index.js gridPosition: Spalten = floor((Breite − 2·Rand) / Zellbreite).
  function layout() {
    const width = surface.clientWidth || 1024;
    const columns = Math.max(1, Math.floor(Math.max(GRID.cellW, width - GRID.offset * 2) / GRID.cellW));
    [...iconsEl.children].forEach((icon, i) => {
      const left = `${GRID.offset + (i % columns) * GRID.cellW}px`;
      const top = `${GRID.offset + Math.floor(i / columns) * GRID.cellH}px`;
      if (icon.style.left !== left) icon.style.left = left;
      if (icon.style.top !== top) icon.style.top = top;
    });
  }

  function render() {
    const live = new Set(apps.map((a) => a.id));
    for (const [id, icon] of icons) if (!live.has(id)) { icon.remove(); icons.delete(id); }
    apps.slice().sort(byRank).forEach((app, i) => {
      const icon = iconEl(app);
      if (iconsEl.children[i] !== icon) iconsEl.insertBefore(icon, iconsEl.children[i] || null);
    });
    layout();
  }

  const ro = typeof ResizeObserver === 'function' ? new ResizeObserver(() => layout()) : null;
  ro?.observe(surface);

  el.addEventListener('dragover', (event) => {
    if (!onUnpin || !event.dataTransfer.types.includes(DRAG_PIN)) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
  });
  el.addEventListener('drop', (event) => {
    const pin = onUnpin && event.dataTransfer.getData(DRAG_PIN);
    if (!pin) return;
    event.preventDefault();
    onUnpin(pin);
  });

  return {
    element: el,
    setApps(next) { apps = next; render(); },
    setOrder(ids) { setRank(ids); render(); },
    refresh: render,
    anchorFor(appId) { return icons.get(appId)?.querySelector('[data-v4-icon]') || null; },
    // Shell-V2: das Symbol einer laufenden App trägt is-app-open (+ aria-current am vorderen).
    setRunning(ids, focusedId = null) {
      for (const [id, icon] of icons) {
        icon.classList.toggle('is-app-open', ids.has(id));
        if (id === focusedId) icon.setAttribute('aria-current', 'true'); else icon.removeAttribute('aria-current');
      }
    },
    disconnect() { ro?.disconnect(); },
    destroy() { ro?.disconnect(); el.remove(); },
  };
}
