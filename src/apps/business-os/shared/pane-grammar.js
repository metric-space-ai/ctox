// Canonical column grammar wiring (design-guide "Canonical Column Grammar").
// The repeating per-pane detail — search, shard/list toggle, collapsed filter
// tray with reset + active-dot, counted view band, one-line footer — is wired
// ONCE here instead of being re-implemented in every module. Markup stays
// explicit per app (see the skill's pane-grammar snippet); this helper binds
// the canonical data attributes inside ONE pane element:
//
//   [data-pg-search]                  search input
//   [data-pg-view="cards|list"]       view toggle buttons (aria-pressed)
//   [data-pg-tray-toggle]             tray toggle (gets .has-active-filters dot)
//   [data-pg-tray]                    collapsed tray (hidden attribute)
//   [data-pg-reset]                   reset control inside the tray
//   [data-pg-filter] (select/input)   tray filters; data-pg-default marks the
//                                     neutral value (falls back to 'all'/'')
//   [data-pg-band="<key>"]            counted view band tabs (aria-selected)
//   [data-pg-count="<key>"]           count spans, rendered as ` (n)`
//   [data-pg-footer]                  one-line footer text target
//
// The module owns rendering; this helper owns state + chrome behaviour and
// reports every change through onChange(state).
export function wirePaneGrammar(pane, { onChange } = {}) {
  if (!pane) return null;
  const search = pane.querySelector('[data-pg-search]');
  const viewButtons = [...pane.querySelectorAll('[data-pg-view]')];
  const trayToggle = pane.querySelector('[data-pg-tray-toggle]');
  const tray = pane.querySelector('[data-pg-tray]');
  const reset = pane.querySelector('[data-pg-reset]');
  const filters = [...pane.querySelectorAll('[data-pg-filter]')];
  const bandTabs = [...pane.querySelectorAll('[data-pg-band]')];
  const footer = pane.querySelector('[data-pg-footer]');

  const filterDefault = (el) => el.dataset.pgDefault ?? (el.tagName === 'SELECT' ? 'all' : '');

  const state = () => ({
    search: (search?.value || '').trim().toLowerCase(),
    view: viewButtons.find((b) => b.getAttribute('aria-pressed') === 'true')?.dataset.pgView
      || viewButtons[0]?.dataset.pgView || 'cards',
    band: bandTabs.find((b) => b.getAttribute('aria-selected') === 'true')?.dataset.pgBand
      || bandTabs[0]?.dataset.pgBand || '',
    filters: Object.fromEntries(filters.map((el) => [el.dataset.pgName || el.name || el.dataset.pgFilter || 'filter', el.value])),
  });

  const filtersActive = () => Boolean((search?.value || '').trim())
    || filters.some((el) => el.value !== filterDefault(el));

  const refreshDot = () => {
    trayToggle?.classList.toggle('has-active-filters', filtersActive());
  };

  const emit = () => {
    refreshDot();
    const current = state();
    onChange?.(current);
    // Declarative consumers (shell-wired panes) listen for this instead of
    // owning any wiring code.
    try {
      pane.dispatchEvent?.(new CustomEvent('ctox-pane-grammar-change', { detail: current, bubbles: true }));
    } catch {}
  };

  search?.addEventListener('input', emit);
  filters.forEach((el) => el.addEventListener('change', emit));
  trayToggle?.addEventListener('click', () => {
    if (!tray) return;
    const open = tray.hidden;
    tray.hidden = !open;
    trayToggle.setAttribute('aria-expanded', String(open));
  });
  reset?.addEventListener('click', () => {
    if (search) search.value = '';
    filters.forEach((el) => { el.value = filterDefault(el); });
    emit();
  });
  viewButtons.forEach((button) => button.addEventListener('click', () => {
    viewButtons.forEach((other) => other.setAttribute('aria-pressed', String(other === button)));
    emit();
  }));
  bandTabs.forEach((tab) => tab.addEventListener('click', () => {
    bandTabs.forEach((other) => {
      other.setAttribute('aria-selected', String(other === tab));
      other.classList.toggle('is-active', other === tab);
    });
    emit();
  }));

  refreshDot();
  return {
    state,
    // Counted band + list counters: zeros are rendered, never hidden.
    setCounts(counts) {
      for (const [key, value] of Object.entries(counts || {})) {
        const node = pane.querySelector(`[data-pg-count="${key}"]`);
        if (node) node.textContent = ` (${value})`;
      }
    },
    setFooter(text) {
      if (footer) footer.textContent = text || '';
    },
    refreshDot,
  };
}

// Scroll guard: re-renders must never move the operator. Modules rebuild list
// wells with `innerHTML = ''`, which clamps the container's scrollTop to 0; the
// shell records the last user scroll offset per scroll container and restores
// it after such a rebuild. Intentional resets (search/view/band/filter changes)
// are exempt: the grammar-change event clears the recorded offsets first.
const paneScrollOffsets = new WeakMap();

export function recordPaneScroll(pane, target) {
  if (!target || target === pane || typeof target.scrollTop !== 'number') return;
  let offsets = paneScrollOffsets.get(pane);
  if (!offsets) { offsets = new Map(); paneScrollOffsets.set(pane, offsets); }
  offsets.set(target, target.scrollTop);
}

export function clearPaneScroll(pane) {
  paneScrollOffsets.get(pane)?.clear();
}

export function restorePaneScroll(pane) {
  const offsets = paneScrollOffsets.get(pane);
  if (!offsets) return;
  for (const [el, top] of offsets) {
    if (!el.isConnected || !top) { if (!el.isConnected) offsets.delete(el); continue; }
    if (el.scrollTop === 0 && el.scrollHeight > el.clientHeight) el.scrollTop = top;
  }
}

export function guardPaneScroll(pane) {
  if (!pane || pane.__ctoxScrollGuard) return;
  pane.__ctoxScrollGuard = true;
  // Capture phase: scroll events do not bubble, but they do capture.
  pane.addEventListener('scroll', (event) => recordPaneScroll(pane, event.target), { capture: true, passive: true });
  pane.addEventListener('ctox-pane-grammar-change', () => clearPaneScroll(pane));
  if (typeof MutationObserver === 'function') {
    const observer = new MutationObserver(() => restorePaneScroll(pane));
    observer.observe(pane, { childList: true, subtree: true });
  }
}

// Shell entry point: wire every not-yet-wired grammar pane under `root`.
// Idempotent (marks panes data-pg-wired); the handle is exposed on the pane
// element so a module can call setCounts/setFooter without importing anything.
export function autoWirePaneGrammar(root) {
  const wired = [];
  for (const pane of root?.querySelectorAll?.('.ctox-pane') || []) {
    if (pane.dataset.pgWired === 'true') continue;
    const hasGrammar = pane.querySelector('[data-pg-search], [data-pg-tray-toggle], [data-pg-view], [data-pg-band]');
    if (!hasGrammar) continue;
    pane.dataset.pgWired = 'true';
    const handle = wirePaneGrammar(pane);
    pane.__ctoxPaneGrammar = handle;
    guardPaneScroll(pane);
    wired.push(handle);
  }
  return wired;
}
