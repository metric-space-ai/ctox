// CTOX Business OS — IoT module (delegation app, RFC 0011).
// 2-pane: LEFT = assets & signals (the vocabulary you delegate over) ·
// CENTER = dashboards of AUTOMATION widgets. A widget is one standing order to
// CTOX, programmed in three parts: ① Trigger-Logik (Rhai watcher, backend) ·
// ② Widget-Code (render_code, sandboxed) · ③ Auftrags-Prompt (action_prompt →
// chat spawn on fire). The human writes prompts (Wenn/Dann + signal); CTOX
// programs the watcher. No JSON fields, no fake chat, no monitoring framing.
import { CtoxResizer } from '../../shared/resizer.js';
import { createContextMenu } from '../../shared/context-menu.js';
import { showBusinessPrompt, showBusinessConfirm, showBusinessAlert } from '../../shared/dialogs.js';

const BUILD = '20260711-iot-context-compact-v2';
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
  selectedAssetId: '',
  expanded: new Set(),
  creating: null,        // { parentId } | null — asset create
  dashboardId: '',       // selected dashboard
  viewMode: 'cards',     // 'cards' | 'list'
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

// Map a widget status-dot key onto the base.css badge states.
function statusBadgeClass(dot) {
  if (dot === 'armed' || dot === 'ok') return ' is-success';
  if (dot === 'fired') return ' is-warning';
  if (dot === 'attention' || dot === 'warn') return ' is-danger';
  return '';
}

export async function mount(ctx) {
  state.ctx = ctx;
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
  let resizer = null;
  const handle = root?.querySelector('[data-resizer="left"]');
  if (handle && root) {
    resizer = new CtoxResizer({ resizerEl: handle, containerEl: root, cssVar: '--iot-left-width', side: 'left', minWidth: 264, maxWidth: 540 });
  }

  root?.addEventListener('click', onClick);
  root?.addEventListener('submit', onSubmit);
  root?.addEventListener('dragstart', onDragStart);
  root?.addEventListener('dragover', onDragOver);
  root?.addEventListener('drop', onDrop);
  root?.addEventListener('dragend', () => { state.dragId = null; clearDragMarks(); });

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
    try { resizer?.destroy?.(); } catch {}
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

async function reload(isActive = () => true) {
  const next = empty();
  for (const n of COLLECTIONS) {
    const c = col(n);
    try { next[n] = c?.find ? (await c.find().exec()).map((d) => (d?.toJSON ? d.toJSON() : d)) : []; } catch { next[n] = []; }
  }
  if (!isActive()) return;
  state.collections = next;
  // Default selections.
  if (!state.dashboardId) { const d = dashboards()[0]; if (d) state.dashboardId = d.id; }
  if (state.dashboardId && !dashboards().some((d) => d.id === state.dashboardId)) state.dashboardId = dashboards()[0]?.id || '';
  render();
}

/* ---------- data helpers ---------- */
function realms() { return state.collections.iot_realms || []; }
function allAssets() { return state.collections.iot_assets || []; }
function assetsInRealm() { return state.realm === 'all' ? allAssets() : allAssets().filter((a) => a.realm === state.realm); }
function childrenOf(id) { return assetsInRealm().filter((a) => (a.parent_id || null) === (id || null)); }
function assetById(id) { return allAssets().find((a) => a.id === id) || null; }
function attrsOf(id) { return (state.collections.iot_attributes || []).filter((a) => a.asset_id === id); }
function numericAttrs(id) { return attrsOf(id).filter((a) => typeof a.value === 'number' || a.value_type === 'Number'); }
function descendants(id) { const out = []; const walk = (p) => childrenOf(p).forEach((c) => { out.push(c); walk(c.id); }); walk(id); return out; }

function currentRealm() { return state.realm === 'all' ? 'master' : state.realm; }
function dashboards() {
  const all = state.collections.iot_dashboards || [];
  return state.realm === 'all' ? all : all.filter((d) => (d.realm || 'master') === state.realm);
}
function dashboardById(id) { return (state.collections.iot_dashboards || []).find((d) => d.id === id) || null; }
function widgetsOf(dashId) {
  return (state.collections.iot_widgets || [])
    .filter((w) => w.dashboard_id === dashId)
    .sort((a, b) => Number(a.sort_index || 0) - Number(b.sort_index || 0));
}

// signal_ref canonical form is "<asset_id>::<attribute_name>".
function signalRef(assetId, attr) { return `${assetId}::${attr}`; }
function parseSignal(ref) { const i = String(ref || '').indexOf('::'); return i < 0 ? [ref, ''] : [ref.slice(0, i), ref.slice(i + 2)]; }
function signalLabel(ref) {
  const [aid, attr] = parseSignal(ref);
  const a = assetById(aid);
  return `${a ? a.name : aid} · ${attr}`;
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
  const left = state.ctx.host.querySelector('[data-iot-left]');
  const center = state.ctx.host.querySelector('[data-iot-center]');
  if (left) left.innerHTML = renderLeft();
  if (center) { center.innerHTML = renderCenter(); mountRenderIframes(center); }
}

function renderLeft() {
  const rs = realms();
  const realmRows = [`<button class="iot-realm-row" data-act="realm" data-realm="all" aria-pressed="${state.realm === 'all'}"><span>${esc(t('realm.all', 'Alle Bereiche'))}</span><span class="iot-realm-count">${allAssets().length}</span></button>`]
    .concat(rs.map((r) => {
      const key = r.realm || r.id;
      const n = allAssets().filter((a) => a.realm === key).length;
      return `<button class="iot-realm-row" data-act="realm" data-realm="${esc(key)}" aria-pressed="${state.realm === key}"><span>${esc(r.name || key)}</span><span class="iot-realm-count">${n}</span></button>`;
    }));

  const tree = childrenOf(null).map((a) => renderNode(a, 0)).join('') ||
    `<div class="iot-empty">${esc(t('tree.empty', 'Noch keine Assets. Lege oben links eins an.'))}</div>`;

  const createForm = state.creating ? renderCreateForm() : '';

  return `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">CTOX IoT</span>
          <h2 class="ctox-pane-title">${esc(t('left.title', 'Assets & Signale'))}</h2>
        </div>
        <div class="ctox-pane-actions">
          <button class="ctox-pane-icon" type="button" data-act="new-asset" data-parent="" aria-label="${esc(t('left.newAssetLabel', 'Asset anlegen'))}" title="${esc(t('left.newAssetLabel', 'Asset anlegen'))}">${icon('add')}</button>
        </div>
      </div>
    </header>
    <div class="iot-scroll">
      <div class="iot-section-label">${esc(t('left.realmSection', 'Bereich'))}</div>
      ${realmRows.join('')}
      ${createForm && !state.creating.parentId ? createForm : ''}
      <div class="iot-section-label">${esc(t('left.structSection', 'Struktur · Rechtsklick auf ein Signal = Auftrag'))}</div>
      <div class="iot-tree">${tree}</div>
    </div>`;
}

function renderNode(asset, depth) {
  const kids = childrenOf(asset.id);
  const open = state.expanded.has(asset.id);
  const signals = numericAttrs(asset.id);
  const warn = (state.collections.iot_alarms || []).some((a) => (a.asset_id === asset.id || (a.asset_ids || []).includes(asset.id)) && a.status !== 'Closed' && a.status !== 'Resolved');
  const dot = warn ? 'warn' : signals.length ? 'ok' : '';
  const twisty = (kids.length || signals.length) ? (open ? '▾' : '▸') : '';
  const sel = state.selectedAssetId === asset.id;
  const childForm = state.creating && state.creating.parentId === asset.id ? renderCreateForm() : '';
  const signalRows = open ? signals.map((s) => {
    const name = s.attribute_name || s.name;
    const val = (typeof s.value === 'number') ? `${s.value}${unitOf(s)}` : '';
    return `<div class="iot-signal" data-id="${esc(asset.id)}" data-act="signal" data-asset="${esc(asset.id)}" data-attr="${esc(name)}" data-context-record-id="${esc(`${asset.id}:${name}`)}" data-context-record-type="iot_signal" data-context-label="${esc(`${asset.name || asset.id} · ${name}`)}" style="padding-left:${8 + (depth + 1) * 16}px" title="${esc(t('signal.hint', 'Signal auswählen; weitere Aktionen über das Aktionsmenü'))}">
      <span class="iot-signal-glyph">∿</span><span class="iot-signal-name">${esc(name)}</span><span class="iot-signal-val">${esc(val)}</span></div>`;
  }).join('') : '';
  return `
    <div class="iot-node" data-act="select" data-id="${esc(asset.id)}" aria-selected="${sel}" style="padding-left:${8 + depth * 16}px">
      <span class="iot-twisty" data-act="toggle" data-id="${esc(asset.id)}">${twisty}</span>
      <span class="iot-status-dot ${dot}"></span>
      <span class="iot-node-name">${esc(asset.name)}</span>
      <span class="ctox-badge">${esc(asset.asset_type)}</span>
      <button class="iot-node-add" title="${esc(t('node.addChild', 'Untergeordnetes Asset'))}" data-act="new-asset" data-parent="${esc(asset.id)}">+</button>
    </div>
    ${childForm}
    ${signalRows}
    ${open ? kids.map((k) => renderNode(k, depth + 1)).join('') : ''}`;
}

function renderCreateForm() {
  const parent = state.creating.parentId ? assetById(state.creating.parentId) : null;
  return `
    <form class="iot-form" data-form="create">
      <h4>${parent ? esc(t('asset.formUnder', 'Asset unter „{0}"', parent.name)) : esc(t('asset.formNew', 'Neues Asset'))}</h4>
      <div class="iot-field"><label class="ctox-field-label">${esc(t('field.name', 'Name'))}</label><input class="ctox-input" name="name" placeholder="${esc(t('asset.namePlaceholder', 'z.B. Serverraum'))}" autofocus required></div>
      <div class="iot-field"><label class="ctox-field-label">${esc(t('field.type', 'Typ'))}</label><select class="ctox-select" name="type">
        ${ASSET_TYPES.map((ty) => `<option value="${ty}">${ty}</option>`).join('')}
      </select></div>
      <div class="iot-form-actions">
        <button type="button" class="ctox-button iot-ghost" data-act="cancel-create">${esc(t('btn.cancel', 'Abbrechen'))}</button>
        <button type="submit" class="ctox-button is-primary">${esc(t('btn.create', 'Anlegen'))}</button>
      </div>
    </form>`;
}

/* ---------- center: dashboards of automation widgets ---------- */
function renderCenter() {
  const ds = dashboards();
  const tabs = ds.map((d) => `<button class="ctox-chip ${d.id === state.dashboardId ? 'is-active' : ''}" type="button" data-act="select-dash" data-id="${esc(d.id)}">${esc(d.name)}</button>`).join('');
  const toolbar = `
    <header class="ctox-pane-header ctox-pane-band">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker">CTOX IoT</span>
          <h2 class="ctox-pane-title">${esc(t('center.title', 'Dashboards'))}</h2>
        </div>
        <div class="ctox-pane-actions">
          <div class="ctox-pane-tabs" role="tablist" aria-label="${esc(t('center.viewLabel', 'Ansicht'))}">
            <button class="ctox-pane-tab ${state.viewMode === 'cards' ? 'active' : ''}" type="button" role="tab" aria-selected="${state.viewMode === 'cards'}" data-act="view" data-view="cards">${esc(t('view.cards', 'Karten'))}</button>
            <button class="ctox-pane-tab ${state.viewMode === 'list' ? 'active' : ''}" type="button" role="tab" aria-selected="${state.viewMode === 'list'}" data-act="view" data-view="list">${esc(t('view.list', 'Liste'))}</button>
          </div>
        </div>
      </div>
      <div class="ctox-pane-tools iot-dash-tabs">
        ${tabs || `<span class="iot-dash-sub">${esc(t('dash.none', 'Noch kein Dashboard'))}</span>`}
        <button class="ctox-pane-icon" type="button" data-act="new-dash" aria-label="${esc(t('dash.new', 'Neues Dashboard'))}" title="${esc(t('dash.new', 'Neues Dashboard'))}">${icon('add')}</button>
      </div>
    </header>`;

  if (!ds.length) {
    return toolbar + `<div class="iot-center-empty">
      <div class="iot-center-empty-art">⌖</div>
      <h3>${esc(t('empty.title', 'Beauftrage CTOX, auf deine Signale aufzupassen'))}</h3>
      <p>${t('empty.body', 'Ein Dashboard bündelt <b>Aufträge</b>: pro Auftrag schreibst du <b>Wenn</b> &amp; <b>Dann</b> — CTOX programmiert den Wächter und handelt.')}</p>
      <button class="ctox-button" data-act="new-dash">${esc(t('empty.newDash', 'Dashboard anlegen'))}</button>
    </div>`;
  }

  const widgets = widgetsOf(state.dashboardId);
  const body = state.viewMode === 'list' ? renderList(widgets) : renderCards(widgets);
  return toolbar + body;
}

function renderCards(widgets) {
  if (!widgets.length) {
    return `<div class="iot-dash-grid"><div class="iot-center-empty inline">
      <h3>${esc(t('cards.emptyTitle', 'Noch keine Aufträge'))}</h3>
      <p>${t('cards.emptyBody', 'Rechtsklick auf ein Signal links → <b>„Auftrag von diesem Signal"</b>, oder:')}</p>
      <button class="ctox-button" data-act="new-auftrag">${esc(t('cards.newAuftrag', 'Auftrag anlegen'))}</button>
    </div></div>`;
  }
  const cards = widgets.map(renderWidgetCard).join('');
  return `<div class="iot-dash-grid">${cards}
    <button class="iot-widget iot-add-card" data-act="new-auftrag"><span class="iot-add-plus">+</span><span>${esc(t('cards.addAuftrag', 'Auftrag hinzufügen'))}</span></button>
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
        <button class="iot-widget-more" data-act="widget-menu" data-id="${esc(w.id)}" title="${esc(t('card.actions', 'Aktionen'))}">⋯</button>
      </div>
      <div class="iot-widget-viz">
        <div class="iot-render-host" data-render-widget="${esc(w.id)}">${w.render_code ? '' : (series.length > 1 ? sparkSvg(series, 'iot-spark') : `<div class="iot-viz-empty">${esc(t('card.noData', 'noch keine Messwerte'))}</div>`)}</div>
        <span class="iot-widget-last">${esc(last)}</span>
      </div>
      <div class="iot-when"><span class="iot-tag">${esc(t('tag.when', 'Wenn'))}</span><span class="iot-when-text">${esc(w.cond_text || t('card.condPlaceholder', 'Bedingung wird mit CTOX festgelegt'))}</span></div>
      <div class="iot-then"><span class="iot-tag then">${esc(t('tag.then', 'Dann'))}</span><span class="iot-then-text">${esc(w.action_prompt || t('card.actionPlaceholder', 'Aktion wird mit CTOX festgelegt'))}</span></div>
      <div class="iot-widget-foot">
        <span class="ctox-badge${statusBadgeClass(st.dot)} iot-widget-status">${esc(st.label)}</span>
        <button class="iot-foot-btn" data-act="open-editor" data-id="${esc(w.id)}" title="${esc(t('card.codeHint', 'Von CTOX programmierter Code'))}">&lt;/&gt; ${esc(t('card.code', 'Code'))}</button>
        <button class="iot-foot-btn" data-act="edit-cond" data-id="${esc(w.id)}">${esc(t('tag.when', 'Wenn'))} ✎</button>
        <button class="iot-foot-btn" data-act="edit-action" data-id="${esc(w.id)}">${esc(t('tag.then', 'Dann'))} ✎</button>
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
      <td style="text-align:right"><button class="iot-widget-more" data-act="widget-menu" data-id="${esc(w.id)}">⋯</button></td>
    </tr>`;
  }).join('');
  return `<div class="iot-dash-grid list"><table class="ctox-table iot-table">
    <thead><tr><th>${esc(t('list.colAuftrag', 'Auftrag · Signal'))}</th><th>${esc(t('tag.when', 'Wenn'))}</th><th>${esc(t('tag.then', 'Dann'))}</th><th>${esc(t('list.colStatus', 'Status'))}</th><th></th></tr></thead>
    <tbody>${rows}</tbody></table>
    <div class="iot-list-foot"><button class="ctox-button" data-act="new-auftrag">${esc(t('cards.newAuftrag', 'Auftrag anlegen'))}</button></div>
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
      slot.innerHTML = '<div class="iot-viz-empty">Render-Code abgelehnt (Sandbox)</div>';
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
  const t = theme || { bg: '#171d20', text: '#cfe6e2', accent: '#6cb8aa', danger: '#e06b60' };
  return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'">
<style>html,body{margin:0;height:100%;overflow:hidden;background:${t.bg};font:13px system-ui,sans-serif;color:${t.text}}.val{font-size:26px;font-weight:680;line-height:1}.unit{font-size:14px;opacity:.7;margin-left:3px}svg{width:100%;height:38px;color:${t.accent}}.err{color:${t.danger || '#e06b60'};font-size:12px}</style></head>
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
  const el = e.target.closest('[data-act]'); if (!el) return;
  const act = el.dataset.act;
  if (act === 'realm') { state.realm = el.dataset.realm; state.selectedAssetId = ''; state.dashboardId = ''; reload(); return; }
  if (act === 'toggle') { e.stopPropagation(); const id = el.dataset.id; state.expanded.has(id) ? state.expanded.delete(id) : state.expanded.add(id); render(); return; }
  if (act === 'select') { state.selectedAssetId = el.dataset.id; render(); return; }
  if (act === 'new-asset') { e.stopPropagation(); const p = el.dataset.parent || null; state.creating = { parentId: p }; if (p) state.expanded.add(p); render(); return; }
  if (act === 'cancel-create') { state.creating = null; render(); return; }
  if (act === 'select-dash') { state.dashboardId = el.dataset.id; render(); return; }
  if (act === 'view') { state.viewMode = el.dataset.view; render(); return; }
  if (act === 'new-dash') { newDashboard(); return; }
  if (act === 'new-auftrag') { newAuftrag(null); return; }
  if (act === 'widget-menu') { e.preventDefault(); openWidgetMenu(el.dataset.id, e); return; }
  if (act === 'edit-cond') { editField(el.dataset.id, 'cond'); return; }
  if (act === 'edit-action') { editField(el.dataset.id, 'action'); return; }
  if (act === 'open-editor') { openWidgetEditor(el.dataset.id); return; }
  if (act === 'signal') { state.selectedAssetId = el.dataset.asset; render(); return; }
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

// Move the dragged widget before the target in the current dashboard and persist
// the new order: reassign sort_index 0..n and upsert every widget whose index
// changed (the desktop "drag → persist position" pattern, by sort order).
function reorderWidget(draggedId, targetId) {
  const ws = widgetsOf(state.dashboardId);
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
  render();
}

function openSignalMenu(assetId, attr, event) {
  state.menu?.show(event, [
    { label: t('menu.auftragFromSignal', 'Auftrag von diesem Signal'), icon: '✦', action: () => newAuftrag(signalRef(assetId, attr)) },
    { label: t('menu.openHistory', 'Verlauf öffnen'), icon: '∿', action: () => { state.selectedAssetId = assetId; render(); } },
    { type: 'separator' },
    { label: t('menu.asWebhook', 'Als Webhook-Quelle einrichten'), icon: '↘', action: () => registerWebhook(signalRef(assetId, attr)) },
  ]);
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
      <div class="iot-ed-note">${t('ed.triggerNote', 'Von CTOX generierte <b>Wächter-Logik</b> (Rhai, läuft im Backend pro Messwert). Status: <b>{0}</b>', esc(statusOf(w).label))}</div>
      <textarea class="ctox-textarea iot-ed-area code" data-ed-field="trigger_code" rows="12" spellcheck="false" placeholder="${esc(t('ed.codePlaceholder', '// noch nicht programmiert — „↻ Neu generieren" beauftragt CTOX'))}">${esc(w.trigger_code || '')}</textarea>
      <div class="iot-ed-actions">
        <button class="ctox-button" data-ed="regen-trigger">${esc(t('ed.regen', '↻ Neu generieren (CTOX)'))}</button>
        <button class="ctox-button is-primary" data-ed="save-trigger">${esc(t('btn.save', 'Speichern'))}</button>
      </div>`;
    return `
      <div class="iot-ed-note">${t('ed.renderNote', 'Von CTOX generierter <b>Widget-Code</b> — <code>render(host, api)</code>, gesandboxt. Die Visualisierung ist dem Auftrag untergeordnet.')}</div>
      <textarea class="ctox-textarea iot-ed-area code" data-ed-field="render_code" rows="12" spellcheck="false" placeholder="${esc(t('ed.codePlaceholder', '// noch nicht programmiert — „↻ Neu generieren" beauftragt CTOX'))}">${esc(w.render_code || '')}</textarea>
      <div class="iot-ed-actions">
        <button class="ctox-button" data-ed="regen-render">${esc(t('ed.regen', '↻ Neu generieren (CTOX)'))}</button>
        <button class="ctox-button is-primary" data-ed="save-render">${esc(t('btn.save', 'Speichern'))}</button>
      </div>`;
  };
  const draw = () => {
    host.innerHTML = `
      <div class="ctox-modal-card iot-modal" role="dialog" aria-modal="true" aria-label="${esc(t('ed.title', 'Widget bearbeiten'))}">
        <header class="ctox-modal-header">
          <div class="iot-modal-titles">
            <div class="iot-modal-kicker">${esc(t('ed.kicker', 'CTOX-Auftrag · von CTOX programmiert'))}</div>
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
    const t = e.target.closest('[data-ed],[data-ed-tab]');
    if (!t) return;
    if (t.dataset.edTab) { tab = t.dataset.edTab; draw(); return; }
    switch (t.dataset.ed) {
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
async function newDashboard() {
  const name = await showBusinessPrompt(t('dash.namePrompt', 'Wie soll das Dashboard heißen?'), { title: t('dash.new', 'Neues Dashboard'), confirmLabel: t('btn.create', 'Anlegen'), defaultValue: t('dash.defaultName', 'Mein Dashboard') });
  if (!name) return;
  await dispatch('ctox.iot.dashboard.upsert', { realm: currentRealm(), name: String(name).trim() });
}

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

// Create an order: pick signal (or use the passed ref), then Wenn + Dann as
// prompts. CTOX compiles the watcher (trigger_code) backend-side; until a model
// is wired the widget persists with status idle ("Wächter wird programmiert").
async function newAuftrag(presetSignal) {
  let ref = presetSignal;
  if (!ref) {
    const opts = assetsInRealm().flatMap((a) => numericAttrs(a.id).map((s) => ({ ref: signalRef(a.id, s.attribute_name || s.name), label: `${a.name} · ${s.attribute_name || s.name}` })));
    if (!opts.length) { await showBusinessPrompt(t('auftrag.noSignal', 'Lege zuerst ein Asset mit einem numerischen Signal an.'), { title: t('auftrag.noSignalTitle', 'Kein Signal'), confirmLabel: t('btn.ok', 'OK') }); return; }
    const picked = await showBusinessPrompt(t('auftrag.pickSignal', 'Welches Signal? Schreibe den Namen:') + '\n' + opts.map((o) => '• ' + o.label).join('\n'), { title: t('auftrag.pickTitle', 'Signal wählen'), confirmLabel: t('btn.next', 'Weiter'), defaultValue: opts[0].label });
    if (!picked) return;
    const hit = opts.find((o) => o.label.toLowerCase() === String(picked).trim().toLowerCase()) || opts.find((o) => o.label.toLowerCase().includes(String(picked).trim().toLowerCase()));
    if (!hit) { await showBusinessPrompt(t('auftrag.signalUnknown', 'Signal nicht erkannt — Auftrag abgebrochen.'), { title: t('auftrag.cancelled', 'Abgebrochen'), confirmLabel: t('btn.ok', 'OK') }); return; }
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

function onSubmit(e) {
  const form = e.target.closest('[data-form]'); if (!form) return;
  e.preventDefault();
  const data = Object.fromEntries(new FormData(form).entries());
  if (form.dataset.form === 'create') {
    dispatch('ctox.iot.asset.upsert', { realm: currentRealm(), name: data.name, asset_type: data.type, parent_id: state.creating?.parentId || null });
    state.creating = null; render();
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
