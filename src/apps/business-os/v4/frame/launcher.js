// Startmenü (FR6d): Popup aus dem Raster-Knopf oben links wie die Shell-V2 (app.js createStartMenuElement/
// filterStartMenu/buildStartMenuItem). Gleiche Klassen und gleiche Struktur, CSS wörtlich aus app.css über
// shell-v2-desktop.css (lab/extract-desktop-css.mjs). Kopf = Suche (+ „Neue App“ mit apps.source.edit, Shell-V2 hat dafür
// keinen Platz), Rumpf = Apps nach Kategorie, Fuß = „Desktop“ links, „Einstellungen“ rechts. Esc, Klick daneben, ein
// Start, „Desktop“ oder „Einstellungen“ schließen das Menü (Shell-V2 hideStartMenu).
// Daten: nur kernel.apps (kein eingebauter Katalog). Gruppen = layout.category, zusammengefasst nach categoryKey
// (Groß/Klein, Leerraum, Unicode-Form); Überschrift = häufigste Schreibweise. Reihenfolge in der Gruppe: launcher_order
// (shell_preferences), dann alphabetisch. Leere Treffer zeigen nichts (kein Platzhaltertext, anders als Shell-V2).
// Reihenfolge ändern (FR2-6): Eintrag ziehen bzw. Alt+Pfeil links/rechts → onReorder(ids). Anheften: „+“/„−“ am Eintrag
// (Shell-V2 start-menu-item-pin-btn) oder Eintrag auf die Reiterleiste ziehen (DRAG_APP).

export const DRAG_APP = 'application/x-ctox-app';
export const DRAG_PIN = 'application/x-ctox-taskbar-pin';

const SEARCH_SVG = '<svg class="start-menu-search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"></circle><line x1="21" y1="21" x2="16.65" y2="16.65"></line></svg>';
const DESKTOP_SVG = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke-width="2.2" stroke-linejoin="round" /><path d="M8 9h8M8 12h8M8 15h5" stroke-linecap="round" /></svg>';
const SETTINGS_SVG = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"></circle><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83 2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"></path></svg>';

// Markup wie app.js createStartMenuElement; die data-v4-*-Haken sind die Schnittstelle des Rahmens und des Labors.
// Auch der Extraktor (lab/extract-desktop-css.mjs) baut seine Probe daraus.
export function startMenuMarkup() {
  return `
    <header class="start-menu-header">
      <div class="start-menu-search-wrapper">
        ${SEARCH_SVG}
        <input type="text" class="start-menu-search-input" data-v4-search autocomplete="off" spellcheck="false" />
      </div>
      <div class="v4f-start-actions" data-v4-launcher-actions hidden></div>
    </header>
    <div class="start-menu-body" data-v4-launcher-body></div>
    <footer class="start-menu-footer" data-v4-launcher-foot>
      <button class="start-menu-footer-btn show-desktop-btn" type="button" data-v4-launcher-desktop>${DESKTOP_SVG}<span data-v4-label></span></button>
      <button class="start-menu-footer-btn settings-btn" type="button" data-v4-launcher-settings>${SETTINGS_SVG}<span data-v4-label></span></button>
    </footer>`;
}

// Eintrag wie app.js buildStartMenuItem; der linke Teil ist der Startknopf (Tastatur: Tab + Enter).
export function startMenuItemMarkup() {
  return `
    <div class="start-menu-item-left" role="button" tabindex="0" data-v4-open>
      <div class="start-menu-item-icon" data-v4-icon></div>
      <div class="start-menu-item-copy">
        <span class="start-menu-item-label" data-v4-title></span>
      </div>
    </div>
    <button class="start-menu-item-pin-btn" type="button" data-v4-pin></button>`;
}

const COLLATOR = new Intl.Collator(undefined, { sensitivity: 'base', numeric: true });

function norm(value) {
  return String(value || '').toLocaleLowerCase();
}

export function categoryKey(value) {
  return String(value ?? '').normalize('NFKC').trim().replace(/\s+/g, ' ').toLocaleLowerCase();
}

// Überschrift einer Gruppe aus ihren Schreibweisen ({Schreibweise: Anzahl}).
function categoryLabel(variants) {
  const upper = (v) => (v && v[0] !== v[0].toLocaleLowerCase() ? 1 : 0);
  return [...variants.entries()].sort((a, b) => b[1] - a[1] || upper(b[0]) - upper(a[0]) || COLLATOR.compare(a[0], b[0]))[0]?.[0] ?? '';
}

export function createLauncher({ host, t, onOpen, onTogglePin, isPinned, iconFor, order = [], onReorder = null, onDesktop = null, onSettings = null, isStartButton = null }) {
  // Hülle [data-v4-v2] (Geltungsbereich von shell-v2-desktop.css), darin das Panel wie in der Shell-V2.
  const hull = document.createElement('div');
  hull.className = 'v4f-start';
  hull.dataset.v4V2 = '';
  hull.dataset.shellStyle = 'ctox';
  const el = document.createElement('div');
  el.className = 'shell-start-menu-panel';
  el.dataset.v4Launcher = '';
  el.innerHTML = startMenuMarkup();
  hull.appendChild(el);
  host.appendChild(hull);
  const search = el.querySelector('[data-v4-search]');
  const body = el.querySelector('[data-v4-launcher-body]');
  const actionsEl = el.querySelector('[data-v4-launcher-actions]');
  search.setAttribute('aria-label', t('search', 'Apps suchen'));
  search.placeholder = t('searchPlaceholder', 'Suche nach Apps...');
  const deskBtn = el.querySelector('[data-v4-launcher-desktop]');
  const setBtn = el.querySelector('[data-v4-launcher-settings]');
  const deskLabel = t('shell.home', 'Desktop');
  deskBtn.querySelector('[data-v4-label]').textContent = deskLabel;
  deskBtn.title = `${deskLabel} (${t('shell.homeKey', 'Strg+Alt+D')})`;
  setBtn.querySelector('[data-v4-label]').textContent = t('settings', 'Einstellungen');
  deskBtn.hidden = !onDesktop;
  setBtn.hidden = !onSettings;
  // Shell-V2: openDesktop(); hideStartMenu() bzw. openSettingsDrawer(); hideStartMenu().
  deskBtn.addEventListener('click', () => { close({ restore: false }); onDesktop?.(); });
  setBtn.addEventListener('click', () => { close({ restore: false }); onSettings?.(); });

  // ---------- Öffnen/Schließen (Shell-V2 showStartMenu/hideStartMenu: Klasse is-active) ----------
  let lastFocus = null;
  const isOpen = () => el.classList.contains('is-active');
  function open() {
    if (isOpen()) return;
    lastFocus = document.activeElement;
    el.classList.add('is-active');
    search.value = '';
    render();
    search.focus();
    document.addEventListener('mousedown', onOutside, true);
    document.addEventListener('keydown', onEsc, true);
  }
  function close({ restore = true } = {}) {
    if (!isOpen()) return;
    el.classList.remove('is-active');
    // Geschlossen ist der Katalog vollständig (Shell-V2 leert die Suche beim nächsten Öffnen; hier schon beim Schließen,
    // damit Anker und Einträge aller Apps bestehen bleiben).
    if (search.value) { search.value = ''; render(); }
    document.removeEventListener('mousedown', onOutside, true);
    document.removeEventListener('keydown', onEsc, true);
    if (restore && el.contains(document.activeElement) && lastFocus?.isConnected) lastFocus.focus?.();
    else if (el.contains(document.activeElement)) document.activeElement.blur();
    lastFocus = null;
  }
  function onOutside(event) {
    if (el.contains(event.target) || isStartButton?.(event.target)) return;
    close({ restore: false });
  }
  function onEsc(event) {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    event.stopPropagation();
    close();
  }

  const tiles = new Map(); // app_id -> .start-menu-item
  const groups = new Map(); // categoryKey -> {section, grid, head}
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

  function start(tile, event) {
    const appId = tile.dataset.v4App;
    const anchor = tile.querySelector('[data-v4-icon]');
    close({ restore: false });
    onOpen(appId, { anchor, event });
  }

  function tileFor(app) {
    let tile = tiles.get(app.id);
    if (!tile) {
      tile = document.createElement('div');
      tile.className = 'start-menu-item';
      tile.dataset.v4App = app.id;
      tile.dataset.target = app.id;
      tile.dataset.targetKind = 'app';
      tile.innerHTML = startMenuItemMarkup();
      const openEl = tile.querySelector('[data-v4-open]');
      tile.addEventListener('click', (event) => {
        if (event.target.closest('[data-v4-pin]')) return;
        start(tile, event);
      });
      tile.querySelector('[data-v4-pin]').addEventListener('click', (event) => {
        event.stopPropagation();
        onTogglePin(tile.dataset.v4App);
      });
      tile.draggable = true;
      tile.addEventListener('dragstart', (event) => {
        dragged = tile.dataset.v4App;
        event.dataTransfer.effectAllowed = 'copyMove';
        event.dataTransfer.setData(DRAG_APP, dragged);
        event.dataTransfer.setData('text/plain', dragged);
        tile.classList.add('is-dragging');
      });
      tile.addEventListener('dragend', () => { dragged = null; tile.classList.remove('is-dragging'); clearMarks(); });
      tile.addEventListener('dragover', (event) => {
        const pos = dropPosition(tile, event);
        if (!pos) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = 'move';
        clearMarks(tile);
        tile.classList.toggle('is-drop-before', pos === 'before');
        tile.classList.toggle('is-drop-after', pos === 'after');
      });
      tile.addEventListener('dragleave', (event) => { if (!tile.contains(event.relatedTarget)) clearMarks(); });
      tile.addEventListener('drop', (event) => {
        const pos = dropPosition(tile, event);
        clearMarks();
        if (!pos) return;
        event.preventDefault();
        event.stopPropagation();
        moveApp(dragged, tile.dataset.v4App, pos);
      });
      openEl.addEventListener('keydown', (event) => {
        if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); start(tile, event); return; }
        if (!event.altKey || (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight' && event.key !== 'ArrowUp' && event.key !== 'ArrowDown')) return;
        event.preventDefault();
        const sibs = [...tile.parentElement.querySelectorAll(':scope > [data-v4-app]')];
        const i = sibs.indexOf(tile);
        const back = event.key === 'ArrowLeft' || event.key === 'ArrowUp';
        const target = sibs[back ? i - 1 : i + 1];
        if (!target) return;
        moveApp(tile.dataset.v4App, target.dataset.v4App, back ? 'before' : 'after');
        tile.querySelector('[data-v4-open]').focus();
      });
      tiles.set(app.id, tile);
    }
    tile.dataset.v4Category = categoryKey(app.category);
    const title = String(app.title || app.id);
    const titleEl = tile.querySelector('[data-v4-title]');
    if (titleEl.textContent !== title) titleEl.textContent = title;
    tile.querySelector('[data-v4-open]').setAttribute('aria-label', title);
    iconFor(app, tile.querySelector('[data-v4-icon]'));
    const pinned = isPinned(app.id);
    const pin = tile.querySelector('[data-v4-pin]');
    pin.classList.toggle('is-pinned', pinned);
    pin.setAttribute('aria-pressed', String(pinned));
    const pinText = pinned ? '−' : '+';
    if (pin.textContent !== pinText) pin.textContent = pinText;
    pin.title = pinned ? t('unpin', 'Von Bar lösen') : t('pin', 'An Bar anheften');
    pin.setAttribute('aria-label', `${pin.title}: ${title}`);
    return tile;
  }

  function groupFor(key, label) {
    let group = groups.get(key);
    if (!group) {
      const section = document.createElement('div');
      section.className = 'start-menu-category';
      section.dataset.v4GroupKey = key;
      const head = document.createElement('div');
      head.className = 'start-menu-category-title';
      head.dataset.v4GroupHead = '';
      head.appendChild(document.createElement('span'));
      section.append(head);
      group = { section, grid: section, head };
      groups.set(key, group);
    }
    if (group.section.dataset.v4Group !== label) group.section.dataset.v4Group = label;
    const span = group.head.firstElementChild;
    if (span.textContent !== label) span.textContent = label;
    return group;
  }

  function render() {
    const q = norm(search.value).trim();
    const live = new Set(apps.map((a) => a.id));
    for (const [id, tile] of tiles) {
      if (!live.has(id)) { tile.remove(); tiles.delete(id); }
    }
    // Überschrift aus ALLEN Apps (nicht nur den Treffern), damit sie sich beim Suchen nicht ändert.
    const variants = new Map(); // key -> Map(Schreibweise -> Anzahl)
    for (const app of apps) {
      const raw = String(app.category ?? '').normalize('NFKC').trim().replace(/\s+/g, ' ');
      const key = categoryKey(raw);
      if (!variants.has(key)) variants.set(key, new Map());
      variants.get(key).set(raw, (variants.get(key).get(raw) || 0) + 1);
    }
    const labelOf = new Map([...variants].map(([key, v]) => [key, categoryLabel(v)]));
    const byCat = new Map();
    for (const app of apps) {
      const key = categoryKey(app.category);
      const hay = `${norm(app.title)} ${norm(app.id)} ${norm(labelOf.get(key))}`;
      if (q && !hay.includes(q)) {
        tiles.get(app.id)?.remove();
        continue;
      }
      if (!byCat.has(key)) byCat.set(key, []);
      byCat.get(key).push(app);
    }
    const cats = [...byCat.keys()].sort((a, b) => (a === '') - (b === '') || COLLATOR.compare(labelOf.get(a), labelOf.get(b)));
    for (const [key, group] of groups) {
      if (!byCat.has(key)) { group.section.remove(); groups.delete(key); }
    }
    const single = cats.length <= 1;
    cats.forEach((cat, index) => {
      const group = groupFor(cat, labelOf.get(cat));
      group.head.hidden = single || !cat;
      const list = byCat.get(cat).sort(byRank);
      list.forEach((app, i) => {
        const tile = tileFor(app);
        const at = group.grid.children[i + 1] || null; // [0] = Überschrift
        if (at !== tile) group.grid.insertBefore(tile, at);
      });
      while (group.grid.children.length > list.length + 1) group.grid.lastElementChild.remove();
      if (body.children[index] !== group.section) body.insertBefore(group.section, body.children[index] || null);
    });
  }

  // ---------- Ziehen ----------
  let dragged = null;
  function clearMarks(except = null) {
    for (const node of body.querySelectorAll('.is-drop-before, .is-drop-after')) if (node !== except) node.classList.remove('is-drop-before', 'is-drop-after');
  }
  // Nur innerhalb derselben Gruppe (Gruppen sortieren nach Kategorie, Rang gilt innerhalb der Gruppe). Die Liste steht
  // untereinander: obere Hälfte = davor, untere = dahinter.
  function dropPosition(tile, event) {
    if (!dragged || dragged === tile.dataset.v4App || !onReorder) return null;
    const from = tiles.get(dragged);
    if (!from || from.dataset.v4Category !== tile.dataset.v4Category) return null;
    const r = tile.getBoundingClientRect();
    return event.clientY < r.top + r.height / 2 ? 'before' : 'after';
  }
  function moveApp(id, targetId, pos) {
    if (!id || !targetId || id === targetId || !onReorder) return;
    // Basis: alle Apps (auch von der Suche ausgeblendete) in aktueller Reihenfolge.
    const ids = apps.slice().sort(byRank).map((a) => a.id).filter((x) => x !== id);
    const at = ids.indexOf(targetId);
    if (at < 0) return;
    ids.splice(pos === 'before' ? at : at + 1, 0, id);
    onReorder(ids);
  }

  search.addEventListener('input', render);
  search.addEventListener('keydown', (event) => {
    if (event.key === 'Enter') {
      const first = body.querySelector('[data-v4-app]');
      if (first) start(first, event);
    }
  });

  // Aktionen im Kopf: [{ id, label, run }]. Knöpfe werden je id wiederverwendet (Fokus bleibt).
  const actionBtns = new Map();
  function setActions(list) {
    const next = Array.isArray(list) ? list.filter((a) => a && a.id && typeof a.run === 'function') : [];
    const keep = new Set(next.map((a) => String(a.id)));
    for (const [id, btn] of actionBtns) if (!keep.has(id)) { btn.remove(); actionBtns.delete(id); }
    next.forEach((action, i) => {
      const id = String(action.id);
      let btn = actionBtns.get(id);
      if (!btn) {
        btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'start-menu-footer-btn v4f-start-action';
        btn.dataset.v4LauncherAction = id;
        btn.innerHTML = '<span class="v4f-launcher-action-glyph" aria-hidden="true"></span><span data-v4-label></span>';
        btn.addEventListener('click', (event) => { const run = btn.__run; close({ restore: false }); run?.({ anchor: btn, event }); });
        actionBtns.set(id, btn);
      }
      btn.__run = action.run;
      const label = String(action.label || id);
      btn.querySelector('[data-v4-label]').textContent = label;
      btn.title = label;
      if (actionsEl.children[i] !== btn) actionsEl.insertBefore(btn, actionsEl.children[i] || null);
    });
    actionsEl.hidden = next.length === 0;
  }

  return {
    element: hull,
    panel: el,
    setApps(next) { apps = next; render(); },
    setActions,
    refresh: render,
    setOrder(ids) { setRank(ids); render(); },
    anchorFor(appId) { return tiles.get(appId)?.querySelector('[data-v4-icon]') || null; },
    get query() { return search.value; },
    set query(value) { search.value = String(value || ''); render(); },
    get isOpen() { return isOpen(); },
    open,
    close,
    toggle() { if (isOpen()) close(); else open(); },
    focusSearch() { open(); search.focus(); },
    destroy() { close({ restore: false }); hull.remove(); },
  };
}
