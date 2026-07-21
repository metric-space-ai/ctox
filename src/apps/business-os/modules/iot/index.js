// CTOX Business OS — IoT module (delegation app, RFC 0011).
// IA-Karte: LEFT = asset/signal tree, the SELECTOR column with the canonical
// shell-wired column grammar (search · view-toggle · collapsed filter tray with
// realm scope + reset · counted band [Alle/Signale/Alarme] · recessed well ·
// one-line footer). MAIN = the dashboard of AUTOMATION widgets for the selected
// asset/signal — the unique work surface. NO third column.
//
// A widget is one standing order to CTOX, programmed in three parts: ①
// Trigger-Logik (Rhai watcher, backend) · ② Widget-Code (render_code,
// sandboxed) · ③ Auftrags-Prompt (action_prompt → chat spawn on fire). The
// human writes prompts (Wenn/Dann + signal); CTOX programs the watcher. All
// command flows and collection schemas are unchanged from the previous IA.
import { createContextMenu } from '../../shared/context-menu.js';
import { showBusinessPrompt, showBusinessConfirm, showBusinessAlert } from '../../shared/dialogs.js';

const BUILD = '20260721-iot-ia-karte-v1';
const COLLECTIONS = [
  'iot_realms', 'iot_assets', 'iot_attributes', 'iot_datapoints', 'iot_alarms',
  'iot_dashboards', 'iot_widgets',
];
const ASSET_TYPES = ['Building', 'Room', 'WeatherStation', 'Thermostat', 'Sensor', 'Plug', 'Site'];

const state = {
  ctx: null,
  menu: null,
  collections: empty(),
  realm: 'all',
  band: 'all',            // 'all' | 'signals' | 'alarms'
  view: 'cards',          // LEFT tree view: 'cards' (tree) | 'list' (flat)
  search: '',
  expanded: new Set(),
  selection: { kind: '', assetId: '', attr: '' }, // '' | 'asset' | 'signal'
  creating: null,         // { parentId } | null — asset create
  dashboardId: '',        // implicit persistence dashboard
  mainView: 'cards',      // MAIN dashboard: 'cards' | 'list'
  dragId: null,
  loading: true,
};

function empty() { return Object.fromEntries(COLLECTIONS.map((c) => [c, []])); }
function esc(s) { return String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c])); }
// i18n: German is the inline fallback (always works offline); en.json supplies
// English; de.json may override German. `t(key, de, ...args)` substitutes {0},{1}…
let MESSAGES = {};
function t(key, de, ...args) {
  let s = MESSAGES[key] ?? de ?? key;
  args.forEach((a, i) => { s = s.replace(`{${i}}`, a); });
  return s;
}
function col(name) {
  const db = state.ctx?.db;
  return db?.collection?.(name) || null;
}

// ---------------------------------------------------------------------------
// Pure helpers (exported for tests — no DOM, no RxDB). Asset rows are annotated
// { id, name, realm, parent_id, signalCount, alarmOpen }.
// ---------------------------------------------------------------------------

// Band membership is a filter, not a partition: an asset can be in several.
export function assetMatchesBand(row, band) {
  if (band === 'signals') return Number(row?.signalCount || 0) > 0;
  if (band === 'alarms') return Boolean(row?.alarmOpen);
  return true;
}

export function filterAssetRows(rows, { realm = 'all', band = 'all', search = '' } = {}) {
  const needle = String(search || '').trim().toLowerCase();
  return (Array.isArray(rows) ? rows : []).filter((row) => {
    if (realm !== 'all' && String(row.realm || 'master') !== realm) return false;
    if (!assetMatchesBand(row, band)) return false;
    if (!needle) return true;
    return String(row.name || row.id || '').toLowerCase().includes(needle);
  });
}

// Counted band tallies (zeros included) over an already realm/search-scoped row
// set — the band itself is NOT applied here.
export function countsForAssets(rows) {
  const list = Array.isArray(rows) ? rows : [];
  return {
    all: list.length,
    signals: list.filter((row) => Number(row.signalCount || 0) > 0).length,
    alarms: list.filter((row) => Boolean(row.alarmOpen)).length,
  };
}

// The MAIN dashboard is revealed by selection: no selection → the "select an
// asset" empty state; a selection → the dashboard. (2-pane auto-reveal analog:
// visible = hasSelection; there is no collapsible detail pane to userCollapse.)
export function resolveMainState(hasSelection) {
  return hasSelection ? 'dashboard' : 'select';
}

// Which widgets the MAIN dashboard shows for the current selection. A signal
// selection filters to that exact signal; an asset selection rolls up the asset
// and its descendants. Self-contained signal parse keeps this DOM/collection-free.
export function widgetsForSelection(widgets, selection = {}) {
  const list = Array.isArray(widgets) ? widgets : [];
  const assetOf = (ref) => { const i = String(ref || '').indexOf('::'); return i < 0 ? String(ref || '') : String(ref).slice(0, i); };
  if (selection.signalRef) return list.filter((w) => w.signal_ref === selection.signalRef);
  if (Array.isArray(selection.assetIds)) {
    const set = new Set(selection.assetIds);
    return list.filter((w) => set.has(assetOf(w.signal_ref)));
  }
  return [];
}

// Monochrome stroke icons for header/close buttons. Delegates to the shell's
// getActionIcon (shared/icons.js via mount ctx); inline paths mirror
// actionIconPaths as a fallback for older shells.
const ACTION_ICON_FALLBACK_PATHS = {
  add: 'M12 5v14M5 12h14',
  close: 'M6 6l12 12M18 6L6 18',
};
function icon(name) {
  const fromShell = state.ctx?.getActionIcon?.(name);
  if (fromShell) return fromShell;
  const path = ACTION_ICON_FALLBACK_PATHS[name] || ACTION_ICON_FALLBACK_PATHS.add;
  return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${path}"></path></svg>`;
}
function iconSvg(paths) {
  return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths}</svg>`;
}
const ICON = {
  spark: '<path d="M13 2L3 14h7l-1 8 10-12h-7z"/>',
  webhook: '<circle cx="12" cy="5" r="2.4"/><path d="M12 7.4v4l-3.4 5.9"/><circle cx="6.2" cy="19" r="2.4"/><path d="M8.6 19h6.8"/><circle cx="17.8" cy="19" r="2.4"/><path d="M15.4 17.7L12 11.4"/>',
};

// Map a widget status-dot key onto the base.css badge states.
function statusBadgeClass(dot) {
  if (dot === 'armed' || dot === 'ok') return ' is-success';
  if (dot === 'fired') return ' is-warning';
  if (dot === 'attention' || dot === 'warn') return ' is-danger';
  return '';
}

export async function mount(ctx) {
  state.ctx = ctx;
  state.lang = ctx.locale === 'en' ? 'en' : 'de';
  state.selection = { kind: '', assetId: '', attr: '' };
  state.loading = true;
  // Cache-bust the shared i18n module like CSS/HTML below (?v=BUILD), then load
  // this module's locale messages (German is the inline fallback in t()).
  try {
    const { loadModuleMessages } = await import(`../../shared/i18n.js?v=${BUILD}`);
    MESSAGES = await loadModuleMessages(import.meta.url, ctx.locale || 'de', {});
  } catch { MESSAGES = {}; }
  ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  state.menu = createContextMenu({ host: document.body, viewportEl: document.documentElement });

  const root = ctx.host.querySelector('[data-iot-root]');
  applyStaticLabels();
  seedGrammarState();
  // The left column resizer is wired declaratively by the shell from the
  // `.ctox-column-resizer[data-resizer-var]` handle; the canonical column
  // grammar (search/tray/reset/active-dot/view-toggle/band) is auto-wired by
  // the shell from the data-pg-* markup — no module chrome JS here.
  root?.addEventListener('click', onClick);
  root?.addEventListener('submit', onSubmit);
  root?.addEventListener('dragstart', onDragStart);
  root?.addEventListener('dragover', onDragOver);
  root?.addEventListener('drop', onDrop);
  root?.addEventListener('dragend', () => { state.dragId = null; clearDragMarks(); });
  // The shell reports search / view / tray / band changes on this bubbling
  // event; those are intentional resets (the tree rebuilds).
  root?.addEventListener('ctox-pane-grammar-change', onGrammarChange);

  // Paint the frame from the empty state immediately. Cold native projections
  // must not hold the window lifecycle open while seven collections hydrate.
  render();
  let disposed = false;
  let reloadInFlight = false;
  let reloadQueued = false;
  const requestReload = () => {
    if (disposed) return;
    if (reloadInFlight) {
      reloadQueued = true;
      return;
    }
    reloadInFlight = true;
    reload(() => !disposed)
      .catch((error) => {
        if (!disposed) console.error('[iot] reload failed:', error);
      })
      .finally(() => {
        reloadInFlight = false;
        if (reloadQueued && !disposed) {
          reloadQueued = false;
          requestReload();
        }
      });
  };
  const subs = COLLECTIONS.map((n) => col(n)?.$?.subscribe?.(requestReload)).filter(Boolean);
  requestReload();

  return () => {
    disposed = true;
    subs.forEach((s) => { try { s.unsubscribe?.(); } catch {} });
    try { state.menu?.destroy?.(); } catch {}
    ctx.host.replaceChildren();
  };
}

function ensureStyles() {
  const href = `${new URL('./index.css', import.meta.url).pathname}?v=${BUILD}`;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet'; link.href = href;
  document.head.append(link);
}
async function loadMarkup() {
  const html = await fetch(new URL(`./index.html?v=${BUILD}`, import.meta.url)).then((r) => r.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((n) => n.remove());
  return doc.body.innerHTML;
}

function rootEl() { return state.ctx?.host?.querySelector('[data-iot-root]'); }

// Translate the static data-copy labels + search placeholder once on mount.
function applyStaticLabels() {
  const el = rootEl();
  if (!el) return;
  el.querySelectorAll('[data-copy]').forEach((node) => {
    const value = t(node.dataset.copy, node.textContent);
    if (value) node.textContent = value;
  });
  const search = el.querySelector('[data-pg-search]');
  if (search) search.placeholder = t('search', 'Suchen...');
}

// Seed the cached grammar state from the DOM before the shell fires its first
// change event (the shell wires the pane asynchronously after mount).
function seedGrammarState() {
  const el = rootEl();
  if (!el) return;
  state.search = (el.querySelector('[data-pg-search]')?.value || '').trim().toLowerCase();
  state.view = el.querySelector('[data-pg-view][aria-pressed="true"]')?.dataset.pgView || 'cards';
  state.band = el.querySelector('[data-pg-band][aria-selected="true"]')?.dataset.pgBand || 'all';
  state.realm = el.querySelector('[data-pg-filter][data-pg-name="realm"]')?.value || 'all';
}

function onGrammarChange(event) {
  const detail = event?.detail || {};
  state.search = String(detail.search ?? state.search ?? '').trim().toLowerCase();
  state.view = detail.view || state.view || 'cards';
  state.band = detail.band || 'all';
  state.realm = (detail.filters && detail.filters.realm) || 'all';
  syncSelectionToVisible();
  render();
}

async function reload(isActive = () => true) {
  const next = empty();
  for (const n of COLLECTIONS) {
    const c = col(n);
    try { next[n] = c?.find ? (await c.find().exec()).map((d) => (d?.toJSON ? d.toJSON() : d)) : []; } catch { next[n] = []; }
  }
  if (!isActive()) return;
  state.collections = next;
  state.loading = false;
  // Keep an implicit persistence dashboard current for widget writes.
  if (!state.dashboardId) { const d = dashboards()[0]; if (d) state.dashboardId = d.id; }
  if (state.dashboardId && !dashboards().some((d) => d.id === state.dashboardId)) state.dashboardId = dashboards()[0]?.id || '';
  syncSelectionToVisible();
  render();
}

/* ---------- data helpers ---------- */
function realms() { return state.collections.iot_realms || []; }
function allAssets() { return state.collections.iot_assets || []; }
function assetById(id) { return allAssets().find((a) => a.id === id) || null; }
function attrsOf(id) { return (state.collections.iot_attributes || []).filter((a) => a.asset_id === id); }
function numericAttrs(id) { return attrsOf(id).filter((a) => typeof a.value === 'number' || a.value_type === 'Number'); }
function childrenAll(id) { return allAssets().filter((a) => (a.parent_id || null) === (id || null)); }
function descendantIdsAll(id) { const out = []; const walk = (p) => childrenAll(p).forEach((c) => { out.push(c.id); walk(c.id); }); walk(id); return out; }
function ancestorIdsAll(id) { const out = []; const seen = new Set([id]); let cur = assetById(id); while (cur && cur.parent_id && !seen.has(cur.parent_id)) { out.push(cur.parent_id); seen.add(cur.parent_id); cur = assetById(cur.parent_id); } return out; }
function hasOpenAlarm(id) { return (state.collections.iot_alarms || []).some((al) => (al.asset_id === id || (al.asset_ids || []).includes(id)) && al.status !== 'Closed' && al.status !== 'Resolved'); }

function currentRealm() { return state.realm === 'all' ? 'master' : state.realm; }
function dashboards() {
  const all = state.collections.iot_dashboards || [];
  return state.realm === 'all' ? all : all.filter((d) => (d.realm || 'master') === state.realm);
}
function widgetsOf(dashId) {
  return (state.collections.iot_widgets || [])
    .filter((w) => w.dashboard_id === dashId)
    .sort((a, b) => Number(a.sort_index || 0) - Number(b.sort_index || 0));
}

// Annotated asset rows for the selector + band tallies.
function assetRows() {
  return allAssets().map((a) => ({
    id: a.id,
    name: a.name || a.id,
    realm: a.realm || 'master',
    parent_id: a.parent_id || null,
    signalCount: numericAttrs(a.id).length,
    alarmOpen: hasOpenAlarm(a.id),
  }));
}
function scopedAssetRows() { return filterAssetRows(assetRows(), { realm: state.realm, band: 'all', search: state.search }); }
function visibleAssetRows() { return filterAssetRows(assetRows(), { realm: state.realm, band: state.band, search: state.search }); }

// Selection helpers ---------------------------------------------------------
function selectionKey(sel = state.selection) {
  if (sel.kind === 'signal') return `${sel.assetId}::${sel.attr}`;
  if (sel.kind === 'asset') return sel.assetId;
  return '';
}
function hasSelection() { return Boolean(state.selection.kind); }
function syncSelectionToVisible() {
  const visibleIds = new Set(visibleAssetRows().map((r) => r.id));
  if (state.selection.kind === 'signal' && assetById(state.selection.assetId)) return;
  if (state.selection.kind === 'asset' && visibleIds.has(state.selection.assetId)) return;
  const first = visibleAssetRows()[0];
  state.selection = first ? { kind: 'asset', assetId: first.id, attr: '' } : { kind: '', assetId: '', attr: '' };
}

// signal_ref canonical form is "<asset_id>::<attribute_name>".
function signalRef(assetId, attr) { return `${assetId}::${attr}`; }
function parseSignal(ref) { const i = String(ref || '').indexOf('::'); return i < 0 ? [ref, ''] : [ref.slice(0, i), ref.slice(i + 2)]; }
function signalLabel(ref) {
  const [aid, attr] = parseSignal(ref);
  const a = assetById(aid);
  return `${a ? a.name : aid} · ${attr}`;
}
function assetPath(id) {
  const names = [...ancestorIdsAll(id)].reverse().map((pid) => assetById(pid)?.name).filter(Boolean);
  return names.join(' / ');
}

function datapointSeries(assetId, attrName) {
  const w = (state.collections.iot_datapoints || [])
    .filter((d) => d.asset_id === assetId && (d.attribute_name || '') === attrName)
    .sort((a, b) => Number(b.to_ms || b.updated_at_ms || 0) - Number(a.to_ms || a.updated_at_ms || 0))[0];
  if (!w) return [];
  const raw = Array.isArray(w.data) ? w.data : Array.isArray(w.data?.points) ? w.data.points : [];
  return raw.map((p) => Array.isArray(p) ? { t: +p[0], v: +p[1] } : { t: +(p.t ?? p.timestamp_ms ?? p.ts), v: +(p.v ?? p.value) })
    .filter((p) => Number.isFinite(p.v));
}
function unitOf(attr) { return attr?.unit || (attr?.attribute_name === 'temperature' || attr?.name === 'temperature' ? '°C' : (attr?.attribute_name === 'humidity' || attr?.name === 'humidity' ? '%' : '')); }
function attrOf(assetId, name) { return attrsOf(assetId).find((x) => (x.attribute_name || x.name) === name) || null; }

function statusInfo(key) {
  return ({
    fired: { dot: 'fired', label: t('st.fired', 'CTOX handelt') },
    armed: { dot: 'armed', label: t('st.armed', 'CTOX wacht') },
    needs_attention: { dot: 'attention', label: t('st.attention', 'braucht Aufmerksamkeit') },
    paused: { dot: 'paused', label: t('st.paused', 'pausiert') },
    idle: { dot: 'idle', label: t('st.idle', 'Wächter wird programmiert …') },
  })[key];
}
function statusOf(w) { return statusInfo(w.trigger_status) || (w.trigger_code ? statusInfo('armed') : statusInfo('idle')); }

/* ---------- render ---------- */
function render() {
  renderTree();
  renderCountsAndFooter();
  renderMain();
}

function renderCountsAndFooter() {
  const el = rootEl();
  const counts = countsForAssets(scopedAssetRows());
  const pane = el?.querySelector('.iot-left');
  const pg = pane?.__ctoxPaneGrammar;
  if (pg && typeof pg.setCounts === 'function') pg.setCounts(counts);
  else for (const [key, value] of Object.entries(counts)) {
    const node = el?.querySelector(`[data-pg-count="${key}"]`);
    if (node) node.textContent = ` (${value})`;
  }
  const realmLabel = state.realm === 'all'
    ? t('allRealms', 'Alle Bereiche')
    : (realms().find((r) => (r.realm || r.id) === state.realm)?.name || state.realm);
  const bandLabel = { all: t('bandAll', 'Alle'), signals: t('bandSignals', 'Signale'), alarms: t('bandAlarms', 'Alarme') }[state.band] || t('bandAll', 'Alle');
  const footerText = `${visibleAssetRows().length} ${t('assetsWord', 'Assets')} · ${realmLabel} · ${bandLabel}`;
  if (pg && typeof pg.setFooter === 'function') pg.setFooter(footerText);
  else { const node = el?.querySelector('[data-pg-footer]'); if (node) node.textContent = footerText; }
  // Keep the realm tray options current (preserve the live value).
  applyRealmOptions(el);
}

function applyRealmOptions(el) {
  const select = el?.querySelector('[data-pg-filter][data-pg-name="realm"]');
  if (!select) return;
  const options = [`<option value="all">${esc(t('allRealms', 'Alle Bereiche'))}</option>`]
    .concat(realms().map((r) => { const key = r.realm || r.id; return `<option value="${esc(key)}">${esc(r.name || key)}</option>`; }));
  const next = options.join('');
  if (select.innerHTML !== next) select.innerHTML = next;
  select.value = state.realm;
}

// The tree/list SELECTOR. Rebuilt only on data / grammar changes; selection is
// an in-place class flip (applyTreeSelection), never a rebuild.
function renderTree() {
  const host = state.ctx?.host?.querySelector('[data-iot-tree]');
  if (!host) return;
  if (!allAssets().length) {
    host.innerHTML = `<div class="ctox-empty"><strong>${esc(t('tree.emptyTitle', 'Noch keine Assets'))}</strong><span>${esc(t('tree.emptyBody', 'Lege oben links eins an.'))}</span></div>`;
    return;
  }
  const visible = visibleAssetRows();
  if (!visible.length) {
    host.innerHTML = `<div class="ctox-empty"><span>${esc(t('tree.noMatch', 'Kein Asset passt zum Filter.'))}</span></div>`;
    return;
  }
  if (state.view === 'list') {
    host.innerHTML = visible
      .slice()
      .sort((a, b) => String(a.name).localeCompare(String(b.name)))
      .map((row) => flatRowHtml(row)).join('');
    // The create form (root-level) still needs a home in list view.
    if (state.creating && !state.creating.parentId) host.insertAdjacentHTML('afterbegin', renderCreateForm());
    return;
  }
  // Cards (tree) view: render the hierarchy, restricted to matching assets plus
  // the ancestors needed to reach them so the tree stays navigable.
  const keep = new Set(visible.map((r) => r.id));
  for (const row of visible) for (const anc of ancestorIdsAll(row.id)) keep.add(anc);
  const createRoot = state.creating && !state.creating.parentId ? renderCreateForm() : '';
  host.innerHTML = createRoot + (childrenAll(null).map((a) => renderNode(a, 0, keep)).join('') || '');
}

function renderNode(asset, depth, keep) {
  if (!keep.has(asset.id)) return '';
  const kids = childrenAll(asset.id).filter((k) => keep.has(k.id));
  const open = state.expanded.has(asset.id);
  const signals = numericAttrs(asset.id);
  const warn = hasOpenAlarm(asset.id);
  const dot = warn ? 'warn' : signals.length ? 'ok' : '';
  const twisty = (kids.length || signals.length) ? (open ? '▾' : '▸') : '';
  const sel = state.selection.kind === 'asset' && state.selection.assetId === asset.id;
  const childForm = state.creating && state.creating.parentId === asset.id ? renderCreateForm() : '';
  const signalRows = open ? signals.map((s) => {
    const name = s.attribute_name || s.name;
    const key = signalRef(asset.id, name);
    const val = (typeof s.value === 'number') ? `${s.value}${unitOf(s)}` : '';
    const ssel = state.selection.kind === 'signal' && selectionKey() === key;
    return `<div class="iot-signal${ssel ? ' is-selected' : ''}" role="button" tabindex="0" aria-selected="${ssel}" data-sel-kind="signal" data-sel-key="${esc(key)}" data-asset-id="${esc(asset.id)}" data-attr="${esc(name)}" data-context-record-id="${esc(key)}" data-context-record-type="iot_signal" data-context-label="${esc(`${asset.name || asset.id} · ${name}`)}" style="padding-left:${8 + (depth + 1) * 16}px">
      <span class="iot-signal-glyph">∿</span><span class="iot-signal-name">${esc(name)}</span><span class="iot-signal-val">${esc(val)}</span></div>`;
  }).join('') : '';
  return `
    <div class="iot-node${sel ? ' is-selected' : ''}" role="button" tabindex="0" data-sel-kind="asset" data-sel-key="${esc(asset.id)}" data-asset-id="${esc(asset.id)}" aria-selected="${sel}" data-context-record-id="${esc(asset.id)}" data-context-record-type="iot_asset" data-context-label="${esc(asset.name || asset.id)}" style="padding-left:${8 + depth * 16}px">
      <span class="iot-twisty" data-act="toggle" data-asset-id="${esc(asset.id)}">${twisty}</span>
      <span class="iot-status-dot ${dot}"></span>
      <span class="iot-node-name">${esc(asset.name)}</span>
      <span class="ctox-badge">${esc(asset.asset_type)}</span>
      <button class="ctox-icon-button ctox-icon-button--sm iot-node-add" type="button" title="${esc(t('node.addChild', 'Untergeordnetes Asset'))}" aria-label="${esc(t('node.addChild', 'Untergeordnetes Asset'))}" data-act="new-asset" data-parent="${esc(asset.id)}">+</button>
    </div>
    ${childForm}
    ${signalRows}
    ${open ? kids.map((k) => renderNode(k, depth + 1, keep)).join('') : ''}`;
}

function flatRowHtml(row) {
  const sel = state.selection.kind === 'asset' && state.selection.assetId === row.id;
  const asset = assetById(row.id);
  const path = assetPath(row.id);
  const dot = row.alarmOpen ? 'warn' : row.signalCount ? 'ok' : '';
  const meta = [path, row.signalCount ? `${row.signalCount} ${t('signalsWord', 'Signale')}` : ''].filter(Boolean).join(' · ');
  return `<div class="ctox-list-item iot-flat-row${sel ? ' is-selected' : ''}" role="button" tabindex="0" aria-selected="${sel}" data-sel-kind="asset" data-sel-key="${esc(row.id)}" data-asset-id="${esc(row.id)}" data-context-record-id="${esc(row.id)}" data-context-record-type="iot_asset" data-context-label="${esc(row.name)}">
    <div class="iot-flat-head"><span class="iot-status-dot ${dot}"></span><span class="iot-node-name">${esc(row.name)}</span><span class="ctox-badge">${esc(asset?.asset_type || '')}</span></div>
    ${meta ? `<div class="iot-flat-meta">${esc(meta)}</div>` : ''}
  </div>`;
}

// In-place selection flip across existing rows — NO tree rebuild (scroll stays).
function applyTreeSelection() {
  const host = state.ctx?.host?.querySelector('[data-iot-tree]');
  const key = selectionKey();
  host?.querySelectorAll('[data-sel-kind]').forEach((rowEl) => {
    const on = (rowEl.getAttribute('data-sel-key') || '') === key;
    rowEl.classList.toggle('is-selected', on);
    rowEl.setAttribute('aria-selected', String(on));
  });
}

function selectAsset(id) {
  if (!id) return;
  state.selection = { kind: 'asset', assetId: id, attr: '' };
  applyTreeSelection();
  renderMain();
}
function selectSignal(assetId, attr) {
  if (!assetId || !attr) return;
  state.selection = { kind: 'signal', assetId, attr };
  applyTreeSelection();
  renderMain();
}

function renderCreateForm() {
  const parent = state.creating.parentId ? assetById(state.creating.parentId) : null;
  return `
    <form class="ctox-card iot-form" data-form="create">
      <header>${parent ? esc(t('asset.formUnder', 'Asset unter „{0}"', parent.name)) : esc(t('asset.formNew', 'Neues Asset'))}</header>
      <div class="ctox-card-body iot-form-body">
        <div><label class="ctox-field-label">${esc(t('field.name', 'Name'))}</label><input class="ctox-input" name="name" placeholder="${esc(t('asset.namePlaceholder', 'z.B. Serverraum'))}" autofocus required></div>
        <div><label class="ctox-field-label">${esc(t('field.type', 'Typ'))}</label><select class="ctox-select" name="type">
          ${ASSET_TYPES.map((ty) => `<option value="${ty}">${ty}</option>`).join('')}
        </select></div>
        <div class="iot-form-actions">
          <button type="button" class="ctox-button ctox-button--ghost" data-act="cancel-create">${esc(t('btn.cancel', 'Abbrechen'))}</button>
          <button type="submit" class="ctox-button is-primary">${esc(t('btn.create', 'Anlegen'))}</button>
        </div>
      </div>
    </form>`;
}

/* ---------- MAIN: dashboard of automation widgets for the selection ---------- */
function displayedWidgets() {
  const sel = state.selection;
  if (sel.kind === 'signal') return widgetsForSelection(state.collections.iot_widgets, { signalRef: signalRef(sel.assetId, sel.attr) })
    .sort((a, b) => Number(a.sort_index || 0) - Number(b.sort_index || 0));
  if (sel.kind === 'asset') return widgetsForSelection(state.collections.iot_widgets, { assetIds: [sel.assetId, ...descendantIdsAll(sel.assetId)] })
    .sort((a, b) => Number(a.sort_index || 0) - Number(b.sort_index || 0));
  return [];
}

function renderMain() {
  const center = state.ctx?.host?.querySelector('[data-iot-center]');
  if (!center) return;
  if (resolveMainState(hasSelection()) === 'select' || !allAssets().length) {
    center.innerHTML = mainEmptyState();
    return;
  }
  const widgets = displayedWidgets();
  center.innerHTML = mainHeader() + (state.mainView === 'list' ? renderList(widgets) : renderCards(widgets));
  mountRenderIframes(center);
}

function mainEmptyState() {
  return `<div class="ctox-empty">
    <strong>${esc(t('main.selectTitle', 'Wähle links ein Asset oder Signal'))}</strong>
    <span>${esc(t('main.selectBody', 'Das Dashboard mit den CTOX-Aufträgen erscheint dann hier.'))}</span>
  </div>`;
}

function mainHeader() {
  const sel = state.selection;
  const title = sel.kind === 'signal' ? signalLabel(signalRef(sel.assetId, sel.attr)) : (assetById(sel.assetId)?.name || sel.assetId);
  const path = sel.kind === 'signal' ? (assetById(sel.assetId)?.name || '') : (assetPath(sel.assetId) || t('main.kicker', 'CTOX IoT'));
  const webhookBtn = sel.kind === 'signal'
    ? `<button class="ctox-pane-icon" type="button" data-act="signal-webhook" aria-label="${esc(t('menu.asWebhook', 'Als Webhook-Quelle einrichten'))}" title="${esc(t('menu.asWebhook', 'Als Webhook-Quelle einrichten'))}">${iconSvg(ICON.webhook)}</button>`
    : '';
  return `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">${esc(path || t('main.kicker', 'CTOX IoT'))}</span>
          <h2 class="ctox-pane-title">${esc(title)}</h2>
        </div>
        <div class="ctox-pane-actions">
          ${webhookBtn}
          <button class="ctox-pane-icon is-primary iot-order-action" type="button" data-act="new-auftrag" aria-label="${esc(t('cards.newAuftrag', 'Auftrag anlegen'))}" title="${esc(t('cards.newAuftrag', 'Auftrag anlegen'))}">${iconSvg(ICON.spark)}</button>
        </div>
      </div>
      <div class="ctox-filterbar iot-main-tools">
        <div class="ctox-view-toggle" role="group" aria-label="${esc(t('center.viewLabel', 'Ansicht'))}">
          <button type="button" class="ctox-pane-icon" data-act="view" data-view="cards" aria-pressed="${state.mainView === 'cards'}" aria-label="${esc(t('view.cards', 'Karten'))}" title="${esc(t('view.cards', 'Karten'))}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="4" width="16" height="7" rx="1.5"/><rect x="4" y="14" width="16" height="7" rx="1.5"/></svg></button>
          <button type="button" class="ctox-pane-icon" data-act="view" data-view="list" aria-pressed="${state.mainView === 'list'}" aria-label="${esc(t('view.list', 'Liste'))}" title="${esc(t('view.list', 'Liste'))}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="4" y1="6" x2="20" y2="6"/><line x1="4" y1="12" x2="20" y2="12"/><line x1="4" y1="18" x2="20" y2="18"/></svg></button>
        </div>
      </div>
    </header>`;
}

function renderCards(widgets) {
  if (!widgets.length) {
    return `<div class="ctox-empty">
      <strong>${esc(t('cards.emptyTitle', 'Noch keine Aufträge'))}</strong>
      <p>${t('cards.emptyBody', 'Auftrag anlegen — schreibe <b>Wenn</b> &amp; <b>Dann</b>, CTOX programmiert den Wächter:')}</p>
      <div><button class="ctox-button" data-act="new-auftrag">${esc(t('cards.newAuftrag', 'Auftrag anlegen'))}</button></div>
    </div>`;
  }
  const cards = widgets.map(renderWidgetCard).join('');
  return `<div class="iot-dash-grid">${cards}
    <button class="iot-widget iot-add-card" type="button" data-act="new-auftrag"><span class="iot-add-plus">+</span><span>${esc(t('cards.addAuftrag', 'Auftrag hinzufügen'))}</span></button>
  </div>`;
}

function renderWidgetCard(w) {
  const st = statusOf(w);
  const [aid, attr] = parseSignal(w.signal_ref);
  const series = datapointSeries(aid, attr);
  const a = attrOf(aid, attr);
  const last = a && typeof a.value === 'number' ? `${a.value}${unitOf(a)}` : (series.length ? `${series[series.length - 1].v}` : '—');
  return `
    <div class="iot-widget" data-id="${esc(w.id)}" data-widget="${esc(w.id)}" data-context-record-id="${esc(w.id)}" data-context-record-type="iot_widget" data-context-label="${esc(w.name || w.title || w.id)}" draggable="true">
      <div class="iot-widget-head">
        <span class="iot-status-dot ${st.dot}" title="${esc(st.label)}"></span>
        <span class="iot-widget-title">${esc(signalLabel(w.signal_ref))}</span>
        <button class="ctox-pane-icon iot-widget-more" type="button" data-act="widget-menu" data-id="${esc(w.id)}" aria-label="${esc(t('card.actions', 'Aktionen'))}" title="${esc(t('card.actions', 'Aktionen'))}">⋯</button>
      </div>
      <div class="iot-widget-viz">
        <div class="iot-render-host" data-render-widget="${esc(w.id)}">${w.render_code ? '' : (series.length > 1 ? sparkSvg(series, 'iot-spark') : `<div class="iot-viz-empty">${esc(t('card.noData', 'noch keine Messwerte'))}</div>`)}</div>
        <span class="iot-widget-last">${esc(last)}</span>
      </div>
      <div class="iot-when"><span class="iot-tag">${esc(t('tag.when', 'Wenn'))}</span><span class="iot-when-text">${esc(w.cond_text || t('card.condPlaceholder', 'Bedingung wird mit CTOX festgelegt'))}</span></div>
      <div class="iot-then"><span class="iot-tag then">${esc(t('tag.then', 'Dann'))}</span><span class="iot-then-text">${esc(w.action_prompt || t('card.actionPlaceholder', 'Aktion wird mit CTOX festgelegt'))}</span></div>
      <div class="iot-widget-foot">
        <button class="ctox-button ctox-button--ghost ctox-button--sm" type="button" data-act="edit-cond" data-id="${esc(w.id)}" aria-label="${esc(t('tag.when', 'Wenn'))}">${esc(t('tag.when', 'Wenn'))} ✎</button>
        <button class="ctox-button ctox-button--ghost ctox-button--sm" type="button" data-act="edit-action" data-id="${esc(w.id)}" aria-label="${esc(t('tag.then', 'Dann'))}">${esc(t('tag.then', 'Dann'))} ✎</button>
      </div>
    </div>`;
}

function renderList(widgets) {
  if (!widgets.length) return renderCards(widgets);
  const rows = widgets.map((w) => {
    const st = statusOf(w);
    return `<tr data-id="${esc(w.id)}" data-widget="${esc(w.id)}" data-context-record-id="${esc(w.id)}" data-context-record-type="iot_widget" data-context-label="${esc(w.name || w.title || w.id)}">
      <td><span class="iot-status-dot ${st.dot}"></span> ${esc(signalLabel(w.signal_ref))}</td>
      <td>${esc(w.cond_text || '—')}</td>
      <td>${esc(w.action_prompt || '—')}</td>
      <td><span class="ctox-badge${statusBadgeClass(st.dot)} iot-widget-status">${esc(st.label)}</span></td>
      <td class="is-num"><button class="ctox-pane-icon iot-widget-more" type="button" data-act="widget-menu" data-id="${esc(w.id)}" aria-label="${esc(t('card.actions', 'Aktionen'))}">⋯</button></td>
    </tr>`;
  }).join('');
  return `<div class="iot-dash-grid list"><table class="ctox-table">
    <thead><tr><th>${esc(t('list.colAuftrag', 'Auftrag · Signal'))}</th><th>${esc(t('tag.when', 'Wenn'))}</th><th>${esc(t('tag.then', 'Dann'))}</th><th>${esc(t('list.colStatus', 'Status'))}</th><th></th></tr></thead>
    <tbody>${rows}</tbody></table>
    <div class="iot-list-foot"><button class="ctox-button" type="button" data-act="new-auftrag">${esc(t('cards.newAuftrag', 'Auftrag anlegen'))}</button></div>
  </div>`;
}

/* ---------- mini chart ---------- */
function sparkSvg(series, cls) {
  const W = 300, H = 40, px = 4, py = 6;
  const ts = series.map((p) => p.t), vs = series.map((p) => p.v);
  const minT = Math.min(...ts), maxT = Math.max(...ts), minV = Math.min(...vs), maxV = Math.max(...vs);
  const sT = (maxT - minT) || 1, sV = (maxV - minV) || 1;
  const pts = series.map((p) => {
    const x = px + (series.length === 1 ? 0 : (p.t - minT) / sT) * (W - px * 2);
    const y = py + (1 - (p.v - minV) / sV) * (H - py * 2);
    return [Math.round(x * 10) / 10, Math.round(y * 10) / 10];
  });
  const line = pts.map((p) => p.join(',')).join(' ');
  return `<svg class="${cls}" viewBox="0 0 ${W} ${H}" preserveAspectRatio="none"><polyline points="${line}" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" stroke-linecap="round"/></svg>`;
}

/* ---------- render sandbox (P3): run CTOX-generated render_code, ISOLATED ----------
   The real boundary is a sandboxed <iframe> (sandbox="allow-scripts", NO
   allow-same-origin → origin-null, no access to parent/cookies/storage) with a
   CSP of default-src 'none' (no network). The lint + </script> escaping are
   defense-in-depth. Signal data is embedded synchronously into srcdoc (no
   postMessage handshake); a data change re-creates the frame on the next render. */
const RENDER_FORBIDDEN = new RegExp(`\\b(${[
  'import', 'require', 'fetch', 'XMLHttpRequest', 'WebSocket', 'cookie',
  `local${'Storage'}`, `session${'Storage'}`, `indexed${'DB'}`, 'parent', 'top',
  'opener', 'postMessage', 'eval', 'globalThis', '__proto__',
].join('|')})\\b`);

function mountRenderIframes(center) {
  const cs = getComputedStyle(document.documentElement);
  const theme = {
    bg: (cs.getPropertyValue('--surface') || '#171d20').trim() || '#171d20',
    text: (cs.getPropertyValue('--text') || '#cfe6e2').trim() || '#cfe6e2',
    accent: (cs.getPropertyValue('--accent') || '#6cb8aa').trim() || '#6cb8aa',
    danger: (cs.getPropertyValue('--danger') || '#e06b60').trim() || '#e06b60',
  };
  center.querySelectorAll('[data-render-widget]').forEach((slot) => {
    const w = (state.collections.iot_widgets || []).find((x) => x.id === slot.dataset.renderWidget);
    if (!w || !w.render_code || !w.render_code.trim()) return; // sparkline fallback stays
    if (RENDER_FORBIDDEN.test(w.render_code)) {
      slot.innerHTML = `<div class="iot-viz-empty">${esc(t('card.renderRejected', 'Render-Code abgelehnt (Sandbox)'))}</div>`;
      return;
    }
    const [aid, attr] = parseSignal(w.signal_ref);
    const series = datapointSeries(aid, attr);
    const frame = document.createElement('iframe');
    frame.className = 'iot-render-frame';
    frame.setAttribute('sandbox', 'allow-scripts');
    frame.setAttribute('referrerpolicy', 'no-referrer');
    frame.srcdoc = buildRenderSrcdoc(w.render_code, series, theme);
    slot.replaceChildren(frame);
  });
}

function buildRenderSrcdoc(code, series, theme) {
  const safe = String(code).replace(/<\/(script|iframe|html|body)/gi, '<\\/$1');
  const data = JSON.stringify(series.map((p) => ({ t: p.t, v: p.v })));
  const th = theme || { bg: '#171d20', text: '#cfe6e2', accent: '#6cb8aa', danger: '#e06b60' };
  return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'">
<style>html,body{margin:0;height:100%;overflow:hidden;background:${th.bg};font:13px system-ui,sans-serif;color:${th.text}}.val{font-size:26px;font-weight:680;line-height:1}.unit{font-size:14px;opacity:.7;margin-left:3px}svg{width:100%;height:38px;color:${th.accent}}.err{color:${th.danger || '#e06b60'};font-size:12px}</style></head>
<body><div id="h"></div><script>
(function(){
  var series=${data},vals=series.map(function(p){return p.v});
  function spark(){var W=300,H=40,p=4;if(vals.length<2)return'';var mn=Math.min.apply(0,vals),mx=Math.max.apply(0,vals),s=(mx-mn)||1;var d=series.map(function(pt,i){var x=p+(series.length<2?0:i/(series.length-1))*(W-2*p);var y=p+(1-(pt.v-mn)/s)*(H-2*p);return (Math.round(x*10)/10)+','+(Math.round(y*10)/10)}).join(' ');return '<svg viewBox="0 0 '+W+' '+H+'" preserveAspectRatio="none"><polyline points="'+d+'" fill="none" stroke="currentColor" stroke-width="2"/></svg>'}
  var api=Object.freeze({
    signal:Object.freeze({last:function(){return vals.length?vals[vals.length-1]:NaN},window:function(){return vals.slice()},rate:function(){return series.length>1?(series[series.length-1].v-series[0].v)/(((series[series.length-1].t-series[0].t)/1000)||1):0}}),
    draw:Object.freeze({value:function(v,u){return '<div class="val">'+String(v)+'<span class="unit">'+(u?String(u):'')+'</span></div>'},line:function(){return spark()},gauge:function(v){return '<div class="val">'+String(v)+'</div>'},grid:function(){return ''}}),
    fmt:function(n,d){return Number(n).toFixed(d==null?1:d)}
  });
  function widgetRender(host,api){ ${safe} }
  try{widgetRender(document.getElementById('h'),api)}catch(e){document.getElementById('h').innerHTML='<div class="err">Render-Fehler</div>'}
})();
<\/script></body></html>`;
}

/* ---------- events ---------- */
function onClick(e) {
  // Standing header actions from index.html (data-action="new|import|export").
  const headerAction = e.target.closest('[data-action]');
  if (headerAction && rootEl()?.contains(headerAction)) { onHeaderAction(headerAction.dataset.action); return; }

  const el = e.target.closest('[data-act]');
  if (el) {
    const act = el.dataset.act;
    if (act === 'toggle') { e.stopPropagation(); const id = el.dataset.assetId; state.expanded.has(id) ? state.expanded.delete(id) : state.expanded.add(id); renderTree(); return; }
    if (act === 'new-asset') { e.stopPropagation(); const p = el.dataset.parent || null; state.creating = { parentId: p }; if (p) state.expanded.add(p); renderTree(); return; }
    if (act === 'cancel-create') { state.creating = null; renderTree(); return; }
    if (act === 'view') { state.mainView = el.dataset.view; renderMain(); return; }
    if (act === 'new-auftrag') { mainNewAuftrag(); return; }
    if (act === 'signal-webhook') { if (state.selection.kind === 'signal') registerWebhook(signalRef(state.selection.assetId, state.selection.attr)); return; }
    if (act === 'widget-menu') { e.preventDefault(); openWidgetMenu(el.dataset.id, e); return; }
    if (act === 'edit-cond') { editField(el.dataset.id, 'cond'); return; }
    if (act === 'edit-action') { editField(el.dataset.id, 'action'); return; }
  }

  // Selection is an in-place flip — never a tree rebuild.
  const tree = state.ctx?.host?.querySelector('[data-iot-tree]');
  const row = e.target.closest('[data-sel-kind]');
  if (row && tree && tree.contains(row)) {
    if (row.dataset.selKind === 'signal') selectSignal(row.dataset.assetId, row.dataset.attr);
    else selectAsset(row.dataset.assetId);
  }
}

function onHeaderAction(action) {
  if (action === 'new') { state.creating = { parentId: null }; renderTree(); }
  else if (action === 'import') { importAssets(); }
  else if (action === 'export') { exportAssets(); }
}

/* ---------- drag-to-reorder the widget grid (persisted via sort_index) ---------- */
function clearDragMarks() { state.ctx?.host?.querySelectorAll('.iot-widget.drag-over').forEach((el) => el.classList.remove('drag-over')); }
function onDragStart(e) {
  const card = e.target.closest('.iot-widget[data-widget]');
  if (!card) return;
  state.dragId = card.dataset.widget;
  try { e.dataTransfer.effectAllowed = 'move'; e.dataTransfer.setData('text/plain', state.dragId); } catch {}
}
function onDragOver(e) {
  if (!state.dragId) return;
  const card = e.target.closest('.iot-widget[data-widget]');
  if (!card || card.dataset.widget === state.dragId) return;
  e.preventDefault();                 // allow drop
  try { e.dataTransfer.dropEffect = 'move'; } catch {}
  clearDragMarks();
  card.classList.add('drag-over');
}
function onDrop(e) {
  const card = e.target.closest('.iot-widget[data-widget]');
  clearDragMarks();
  if (!state.dragId || !card) return;
  e.preventDefault();
  reorderWidget(state.dragId, card.dataset.widget);
  state.dragId = null;
}

// Move the dragged widget before the target within the currently displayed set
// and persist the new order: reassign sort_index 0..n and upsert every widget
// whose index changed (the desktop "drag → persist position" pattern).
function reorderWidget(draggedId, targetId) {
  const ws = displayedWidgets();
  const from = ws.findIndex((w) => w.id === draggedId);
  const to = ws.findIndex((w) => w.id === targetId);
  if (from < 0 || to < 0 || from === to) return;
  const [moved] = ws.splice(from, 1);
  ws.splice(to, 0, moved);
  ws.forEach((w, i) => {
    if (Number(w.sort_index || 0) !== i) {
      w.sort_index = i; // optimistic local update so re-render keeps the order
      dispatch('ctox.iot.widget.upsert', {
        id: w.id, dashboard_id: w.dashboard_id, realm: w.realm || currentRealm(), signal_ref: w.signal_ref,
        cond_text: w.cond_text, action_prompt: w.action_prompt, trigger_code: w.trigger_code, render_code: w.render_code,
        x: w.x, y: w.y, w: w.w, h: w.h, sort_index: i,
      });
    }
  });
  renderMain();
}

// Mint a token-gated inbound webhook bound to this signal and show the operator
// the one-time URL + token (a real connector — no model needed).
async function registerWebhook(ref) {
  const res = await dispatch('ctox.iot.webhook.register', { realm: currentRealm(), signal_ref: ref });
  const path = res && (res.ingest_path || (res.id ? '/ctox/iot/webhook/' + res.id : ''));
  const token = res && res.token;
  const msg = (path || token)
    ? t('webhook.ready', 'Webhook-Quelle für „{0}" ist eingerichtet.\n\nExterne Sensoren POSTen an:\n  {1}\nmit Header:\n  X-Webhook-Token: {2}\n\nDer Wert wird zum Signal-Datenpunkt — gebundene Wächter feuern automatisch.', signalLabel(ref), path || '(siehe ctox iot webhook)', token || '(im Secret-Store)')
    : t('webhook.created', 'Webhook-Quelle für „{0}" wurde angelegt.', signalLabel(ref));
  await showBusinessAlert(msg, { title: t('webhook.title', 'Webhook-Quelle'), confirmLabel: t('btn.ok', 'OK') });
}

function openWidgetMenu(widgetId, event) {
  const w = (state.collections.iot_widgets || []).find((x) => x.id === widgetId); if (!w) return;
  state.menu?.show(event, [
    { label: t('menu.openEditor', 'Editor öffnen (3 CTOX-Teile)'), icon: '</>', action: () => openWidgetEditor(widgetId) },
    { label: t('menu.editWhen', 'Bedingung bearbeiten (Wenn)'), icon: '✎', action: () => editField(widgetId, 'cond') },
    { label: t('menu.editThen', 'Aktion bearbeiten (Dann)'), icon: '✎', action: () => editField(widgetId, 'action') },
    { type: 'separator' },
    {
      label: w.trigger_status === 'paused' ? t('menu.resume', 'Fortsetzen') : t('menu.pause', 'Pausieren'),
      icon: w.trigger_status === 'paused' ? '▶' : '⏸',
      action: () => dispatch('ctox.iot.widget.pause', { widget_id: widgetId, paused: w.trigger_status !== 'paused' }),
    },
    { label: t('menu.deleteAuftrag', 'Auftrag löschen'), icon: '🗑', action: () => deleteWidget(w) },
  ]);
}

// The editor — each widget's THREE CTOX-programmed parts, editable, each with
// "↻ neu generieren": ① Auftrag (Wenn/Dann) · ② Trigger-Logik (Rhai, generated)
// · ③ Widget-Code (render_code, generated). This is the visible "CTOX programs
// each widget" surface. It edits/displays code; it never executes it (the render
// sandbox is a separate, isolated concern).
function openWidgetEditor(widgetId) {
  const w = (state.collections.iot_widgets || []).find((x) => x.id === widgetId);
  if (!w) return;
  let tab = 'auftrag';
  const host = document.createElement('div');
  host.className = 'ctox-modal iot-modal-overlay';
  const TABS = { auftrag: t('ed.tabAuftrag', 'Auftrag'), trigger: t('ed.tabTrigger', 'Trigger-Logik'), widget: t('ed.tabWidget', 'Widget-Code') };

  const tabBody = () => {
    if (tab === 'auftrag') return `
      <label class="ctox-field-label">${esc(t('ed.whenLabel', 'Wenn — die Bedingung (Freitext)'))}</label>
      <textarea class="ctox-textarea iot-ed-area" data-ed-field="cond_text" rows="2" placeholder="${esc(t('ed.whenPlaceholder', 'z.B. wenn es länger als 5 Min über 30°C ist'))}">${esc(w.cond_text || '')}</textarea>
      <label class="ctox-field-label">${esc(t('ed.thenLabel', 'Dann — der Auftrag an CTOX (wird bei Auslösung als Chat gespawnt)'))}</label>
      <textarea class="ctox-textarea iot-ed-area" data-ed-field="action_prompt" rows="3" placeholder="${esc(t('ed.thenPlaceholder', "z.B. Kühlung hochfahren und melden, eskalieren wenn's nicht hilft"))}">${esc(w.action_prompt || '')}</textarea>
      <div class="iot-ed-actions"><button class="ctox-button ctox-run-control" data-ed="save-auftrag"><span aria-hidden="true">▶</span>${esc(t('ed.saveAuftrag', 'Speichern → CTOX programmiert den Wächter neu'))}</button></div>`;
    if (tab === 'trigger') return `
      <div class="ctox-callout">${t('ed.triggerNote', 'Von CTOX generierte <b>Wächter-Logik</b> (Rhai, läuft im Backend pro Messwert). Status: <b>{0}</b>', esc(statusOf(w).label))}</div>
      <textarea class="ctox-textarea iot-ed-area code" data-ed-field="trigger_code" rows="12" spellcheck="false" placeholder="${esc(t('ed.codePlaceholder', '// noch nicht programmiert — „↻ Neu generieren" beauftragt CTOX'))}">${esc(w.trigger_code || '')}</textarea>
      <div class="iot-ed-actions">
        <button class="ctox-button" data-ed="regen-trigger">${esc(t('ed.regen', '↻ Neu generieren (CTOX)'))}</button>
        <button class="ctox-button is-primary" data-ed="save-trigger">${esc(t('btn.save', 'Speichern'))}</button>
      </div>`;
    return `
      <div class="ctox-callout">${t('ed.renderNote', 'Von CTOX generierter <b>Widget-Code</b> — <code>render(host, api)</code>, gesandboxt. Die Visualisierung ist dem Auftrag untergeordnet.')}</div>
      <textarea class="ctox-textarea iot-ed-area code" data-ed-field="render_code" rows="12" spellcheck="false" placeholder="${esc(t('ed.codePlaceholder', '// noch nicht programmiert — „↻ Neu generieren" beauftragt CTOX'))}">${esc(w.render_code || '')}</textarea>
      <div class="iot-ed-actions">
        <button class="ctox-button" data-ed="regen-render">${esc(t('ed.regen', '↻ Neu generieren (CTOX)'))}</button>
        <button class="ctox-button is-primary" data-ed="save-render">${esc(t('btn.save', 'Speichern'))}</button>
      </div>`;
  };
  const draw = () => {
    host.innerHTML = `
      <div class="ctox-modal-card ctox-modal-card--wide" role="dialog" aria-modal="true" aria-label="${esc(t('ed.title', 'Widget bearbeiten'))}">
        <header class="ctox-modal-header">
          <div class="iot-modal-titles">
            <span class="ctox-pane-kicker">${esc(t('ed.kicker', 'CTOX-Auftrag · von CTOX programmiert'))}</span>
            <h3 class="ctox-modal-title">${esc(signalLabel(w.signal_ref))}</h3>
          </div>
          <button class="ctox-pane-icon" type="button" data-ed="close" aria-label="${esc(t('btn.close', 'Schließen'))}">${icon('close')}</button>
        </header>
        <div class="ctox-pane-tabs iot-ed-tabs" role="tablist">
          ${Object.keys(TABS).map((tk) => `<button class="ctox-pane-tab ${tab === tk ? 'active' : ''}" type="button" role="tab" aria-selected="${tab === tk}" data-ed-tab="${tk}">${esc(TABS[tk])}</button>`).join('')}
        </div>
        <div class="ctox-modal-body iot-ed-body">${tabBody()}</div>
      </div>`;
  };
  const close = () => host.remove();
  const field = (name) => host.querySelector(`[data-ed-field="${name}"]`)?.value ?? '';
  const base = () => ({ id: w.id, dashboard_id: w.dashboard_id, realm: w.realm || currentRealm(), signal_ref: w.signal_ref, cond_text: w.cond_text, action_prompt: w.action_prompt, trigger_code: w.trigger_code, render_code: w.render_code });

  host.addEventListener('click', async (e) => {
    if (e.target === host) return close();
    const target = e.target.closest('[data-ed],[data-ed-tab]');
    if (!target) return;
    if (target.dataset.edTab) { tab = target.dataset.edTab; draw(); return; }
    switch (target.dataset.ed) {
      case 'close': return close();
      case 'save-auftrag':
        await dispatch('ctox.iot.widget.upsert', { ...base(), cond_text: field('cond_text').trim(), action_prompt: field('action_prompt').trim() });
        await dispatch('ctox.iot.widget.compile_trigger', { widget_id: w.id }, true);
        return close();
      case 'save-trigger':
        await dispatch('ctox.iot.widget.upsert', { ...base(), trigger_code: field('trigger_code') });
        return close();
      case 'save-render':
        await dispatch('ctox.iot.widget.upsert', { ...base(), render_code: field('render_code') });
        return close();
      case 'regen-trigger':
        await dispatch('ctox.iot.widget.compile_trigger', { widget_id: w.id }, true);
        return close();
      case 'regen-render':
        await dispatch('ctox.iot.widget.generate_render', { widget_id: w.id }, true);
        return close();
    }
  });
  draw();
  (state.ctx?.host || document.body).appendChild(host);
}

/* ---------- mutations (all real commands; CTOX programs the watcher) ---------- */
function genId(prefix) { return `${prefix}_${Date.now().toString(36)}_${Math.floor(Math.random() * 1e9).toString(36)}`; }

// Return a usable dashboard id. If none exists we mint the id CLIENT-SIDE and
// pass it to dashboard.upsert (the backend honours a provided id), so the widget
// we create next references a real dashboard without waiting for the RxDB reload.
async function ensureDashboard() {
  if (state.dashboardId && dashboards().some((d) => d.id === state.dashboardId)) return state.dashboardId;
  const existing = dashboards()[0];
  if (existing) { state.dashboardId = existing.id; return existing.id; }
  const id = genId('dash');
  await dispatch('ctox.iot.dashboard.upsert', { id, realm: currentRealm(), name: t('dash.defaultName', 'Mein Dashboard') });
  state.dashboardId = id;
  return id;
}

// From the MAIN "Neu Auftrag" action: a signal selection preselects the signal;
// an asset selection scopes the picker to that asset + descendants.
function mainNewAuftrag() {
  if (state.selection.kind === 'signal') return newAuftrag(signalRef(state.selection.assetId, state.selection.attr));
  return newAuftrag(null, state.selection.kind === 'asset' ? state.selection.assetId : null);
}

// Create an order: pick signal (or use the passed ref), then Wenn + Dann as
// prompts. CTOX compiles the watcher (trigger_code) backend-side; until a model
// is wired the widget persists with status idle ("Wächter wird programmiert").
async function newAuftrag(presetSignal, scopeAssetId) {
  let ref = presetSignal;
  if (!ref) {
    const scopeIds = scopeAssetId ? new Set([scopeAssetId, ...descendantIdsAll(scopeAssetId)]) : null;
    const pool = scopeIds ? allAssets().filter((a) => scopeIds.has(a.id)) : (state.realm === 'all' ? allAssets() : allAssets().filter((a) => (a.realm || 'master') === state.realm));
    const opts = pool.flatMap((a) => numericAttrs(a.id).map((s) => ({ ref: signalRef(a.id, s.attribute_name || s.name), label: `${a.name} · ${s.attribute_name || s.name}` })));
    if (!opts.length) { await showBusinessAlert(t('auftrag.noSignal', 'Lege zuerst ein Asset mit einem numerischen Signal an.'), { title: t('auftrag.noSignalTitle', 'Kein Signal'), confirmLabel: t('btn.ok', 'OK') }); return; }
    const picked = await showBusinessPrompt(t('auftrag.pickSignal', 'Welches Signal? Schreibe den Namen:') + '\n' + opts.map((o) => '• ' + o.label).join('\n'), { title: t('auftrag.pickTitle', 'Signal wählen'), confirmLabel: t('btn.next', 'Weiter'), defaultValue: opts[0].label });
    if (!picked) return;
    const hit = opts.find((o) => o.label.toLowerCase() === String(picked).trim().toLowerCase()) || opts.find((o) => o.label.toLowerCase().includes(String(picked).trim().toLowerCase()));
    if (!hit) { await showBusinessAlert(t('auftrag.signalUnknown', 'Signal nicht erkannt — Auftrag abgebrochen.'), { title: t('auftrag.cancelled', 'Abgebrochen'), confirmLabel: t('btn.ok', 'OK') }); return; }
    ref = hit.ref;
  }
  const cond = await showBusinessPrompt(t('auftrag.whenPrompt', 'Wann soll CTOX handeln? (frei formuliert)'), { title: t('dlg.when', 'Wenn …'), message: signalLabel(ref), confirmLabel: t('btn.next', 'Weiter'), defaultValue: '' });
  if (cond === null) return;
  const action = await showBusinessPrompt(t('auftrag.thenPrompt', 'Was soll CTOX dann tun?'), { title: t('dlg.then', 'Dann …'), confirmLabel: t('auftrag.create', 'Auftrag anlegen'), defaultValue: '' });
  if (action === null) return;

  const dashId = await ensureDashboard();
  if (!dashId) return;
  const wid = genId('wid');
  const payload = { id: wid, dashboard_id: dashId, realm: currentRealm(), signal_ref: ref, cond_text: String(cond).trim(), action_prompt: String(action).trim() };
  await dispatch('ctox.iot.widget.upsert', payload);
  // Ask CTOX to program the watcher (durable agent-turn task; waits for a model).
  await dispatch('ctox.iot.widget.compile_trigger', { widget_id: wid }, true);
  // Focus the new order's signal so it shows in the dashboard immediately.
  const [aid, attr] = parseSignal(ref);
  state.selection = { kind: 'asset', assetId: aid, attr: '' };
}

async function editField(widgetId, which) {
  const w = (state.collections.iot_widgets || []).find((x) => x.id === widgetId); if (!w) return;
  const isCond = which === 'cond';
  const val = await showBusinessPrompt(isCond ? t('auftrag.whenPromptShort', 'Wann soll CTOX handeln?') : t('auftrag.thenPrompt', 'Was soll CTOX dann tun?'), {
    title: isCond ? t('dlg.when', 'Wenn …') : t('dlg.then', 'Dann …'), confirmLabel: t('btn.apply', 'Übernehmen'), defaultValue: isCond ? (w.cond_text || '') : (w.action_prompt || ''),
  });
  if (val === null) return;
  const patch = { id: w.id, dashboard_id: w.dashboard_id, realm: w.realm || currentRealm(), signal_ref: w.signal_ref };
  if (isCond) { patch.cond_text = String(val).trim(); patch.action_prompt = w.action_prompt; }
  else { patch.action_prompt = String(val).trim(); patch.cond_text = w.cond_text; }
  await dispatch('ctox.iot.widget.upsert', patch);
  if (isCond) await dispatch('ctox.iot.widget.compile_trigger', { widget_id: w.id }, true);
}

async function deleteWidget(w) {
  const ok = await showBusinessConfirm(t('del.confirm', 'Auftrag „{0}" löschen?', signalLabel(w.signal_ref)), { title: t('del.title', 'Auftrag löschen'), confirmLabel: t('btn.delete', 'Löschen'), kind: 'danger' });
  if (!ok) return;
  await dispatch('ctox.iot.widget.delete', { widget_id: w.id });
}

/* ---------- import / export (honest, small — JSON via Blob / file input) ---------- */
// Export the assets currently visible in the selector as a JSON download.
function exportAssets() {
  const rows = visibleAssetRows().map((r) => {
    const a = assetById(r.id) || {};
    return { name: r.name, asset_type: a.asset_type || '', realm: r.realm, parent_id: r.parent_id || null };
  });
  let url = '';
  try {
    const blob = new Blob([JSON.stringify(rows, null, 2)], { type: 'application/json' });
    url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = 'iot-assets.json';
    anchor.rel = 'noopener';
    rootEl()?.appendChild(anchor);
    anchor.click();
    anchor.remove();
  } catch (error) {
    console.error('[iot] export failed', error);
  } finally {
    if (url) window.setTimeout(() => { try { URL.revokeObjectURL(url); } catch {} }, 4000);
  }
}

// Import creates assets from a JSON array of { name, asset_type } via the
// existing ctox.iot.asset.upsert command — projections stay server-owned.
function importAssets() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = 'application/json,.json';
  input.addEventListener('change', async () => {
    const file = input.files && input.files[0];
    if (!file) return;
    let parsed;
    try { parsed = JSON.parse(await file.text()); } catch {
      await showBusinessAlert(t('importInvalid', 'Ungültige JSON-Datei.'), { title: t('import.title', 'Import'), confirmLabel: t('btn.ok', 'OK') });
      return;
    }
    const items = Array.isArray(parsed) ? parsed : (parsed && typeof parsed === 'object' ? [parsed] : []);
    const candidates = items.filter((item) => item && typeof item === 'object' && String(item.name || '').trim());
    if (!candidates.length) {
      await showBusinessAlert(t('importEmpty', 'Keine Datensätze in der Datei.'), { title: t('import.title', 'Import'), confirmLabel: t('btn.ok', 'OK') });
      return;
    }
    let count = 0;
    for (const item of candidates) {
      try {
        await dispatch('ctox.iot.asset.upsert', {
          realm: currentRealm(),
          name: String(item.name).trim(),
          asset_type: String(item.asset_type || item.type || 'Sensor'),
          parent_id: item.parent_id || null,
        });
        count += 1;
      } catch (error) {
        console.error('[iot] import failed', error);
      }
    }
    await showBusinessAlert(`${t('imported', 'Importiert')}: ${count}`, { title: t('import.title', 'Import'), confirmLabel: t('btn.ok', 'OK') });
  });
  input.click();
}

function onSubmit(e) {
  const form = e.target.closest('[data-form]'); if (!form) return;
  e.preventDefault();
  const data = Object.fromEntries(new FormData(form).entries());
  if (form.dataset.form === 'create') {
    dispatch('ctox.iot.asset.upsert', { realm: currentRealm(), name: data.name, asset_type: data.type, parent_id: state.creating?.parentId || null });
    state.creating = null; renderTree();
  }
}

async function dispatch(command_type, payload, tolerant) {
  const bus = state.ctx?.commandBus;
  if (!bus?.dispatch) return;
  try {
    await state.ctx?.sync?.startCollection?.('business_commands');
    return await bus.dispatch({
      id: `cmd_iot_${BUILD}_${Math.round(performance.now())}_${Math.floor(Math.random() * 1e4)}`,
      module: 'iot', command_type,
      record_id: payload.id || payload.widget_id || payload.dashboard_id || payload.name || payload.signal_ref || 'iot',
      inbound_channel: 'business_os.iot', payload, client_context: { source_module: 'iot' },
    });
  } catch (err) {
    if (!tolerant) console.error('[iot] command failed', command_type, err);
    return undefined;
  }
}
