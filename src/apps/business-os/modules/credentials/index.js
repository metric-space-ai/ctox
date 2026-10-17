import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';

// Write-only credentials manager. The browser never receives a secret value:
// it sends ctox.secret.{list,put,delete} commands via api.command (App-API 4.0,
// pushed status, never awaited), and the daemon redacts the value from the persisted
// command record (see store.rs accept_rxdb_business_command). Listing returns
// metadata only (name + description + set/unset status) as the `done` result of
// ctox.secret.list. The list is REACTIVE: the module subscribes to put/delete
// commands in business_commands and re-lists when one lands (no refresh button).
// V4-PORT.md OFFEN-1: metadata as a live collection via ctx.cap.secrets.

const SECRETS_MANAGE = 'secrets.manage';
const LIST_COMMAND = 'ctox.secret.list';
const PUT_COMMAND = 'ctox.secret.put';
const DELETE_COMMAND = 'ctox.secret.delete';

const labels = {
  de: {
    title: 'Zugangsdaten',
    listTitle: 'Zugangsdaten',
    newTitle: 'Neue Zugangsdaten',
    newAction: 'Neue Zugangsdaten',
    importAction: 'Importieren',
    exportAction: 'Exportieren',
    searchPlaceholder: 'Suchen...',
    closeDetail: 'Details schließen',
    viewToList: 'Als Liste anzeigen',
    viewToCards: 'Als Karten anzeigen',
    sourceAll: 'Alle Quellen',
    sourceCatalog: 'Katalog',
    sourceExtra: 'Eigene',
    bandAll: 'Alle',
    bandSet: 'Gesetzt',
    bandOpen: 'Offen',
    keyLabel: 'Schlüssel',
    valueLabel: 'Wert',
    valueHint: 'Werte werden verschlüsselt gespeichert und nie wieder angezeigt.',
    status_set: 'Gesetzt',
    status_unset: 'Nicht gesetzt',
    updated: 'aktualisiert {date}',
    ph_set: 'Wert eingeben',
    ph_rotate: 'Neuen Wert (rotieren)',
    add_btn: 'Hinzufügen',
    btn_save: 'Speichern',
    btn_rotate: 'Rotieren',
    btn_delete: 'Löschen',
    field_description: 'Beschreibung',
    field_status: 'Status',
    field_source: 'Quelle',
    field_updated: 'Aktualisiert',
    entries: 'Einträge',
    confirm_delete: '{name} wirklich entfernen?',
    saved: '{name} gespeichert',
    deleted: '{name} entfernt',
    imported: '{count} importiert',
    import_invalid: 'Ungültige JSON-Datei.',
    import_empty: 'Keine gültigen Zugangsdaten (Name + Wert) in der Datei.',
    value_required: 'Bitte einen Wert eingeben.',
    key_invalid: 'Ungültiger Schlüssel: UPPER_SNAKE_CASE (A–Z, 0–9, _).',
    save_failed: 'Speichern fehlgeschlagen.',
    load_failed: 'Laden fehlgeschlagen.',
    no_permission: 'Keine Berechtigung für Zugangsdaten (Rolle Chef oder Admin).',
  },
  en: {
    title: 'Credentials',
    listTitle: 'Credentials',
    newTitle: 'New credential',
    newAction: 'New credential',
    importAction: 'Import',
    exportAction: 'Export',
    searchPlaceholder: 'Search...',
    closeDetail: 'Close details',
    viewToList: 'Show as list',
    viewToCards: 'Show as cards',
    sourceAll: 'All sources',
    sourceCatalog: 'Catalog',
    sourceExtra: 'Custom',
    bandAll: 'All',
    bandSet: 'Set',
    bandOpen: 'Pending',
    keyLabel: 'Key',
    valueLabel: 'Value',
    valueHint: 'Values are stored encrypted and never shown again.',
    status_set: 'Set',
    status_unset: 'Not set',
    updated: 'updated {date}',
    ph_set: 'Enter value',
    ph_rotate: 'New value (rotate)',
    add_btn: 'Add',
    btn_save: 'Save',
    btn_rotate: 'Rotate',
    btn_delete: 'Remove',
    field_description: 'Description',
    field_status: 'Status',
    field_source: 'Source',
    field_updated: 'Updated',
    entries: 'entries',
    confirm_delete: 'Remove {name}?',
    saved: '{name} saved',
    deleted: '{name} removed',
    imported: '{count} imported',
    import_invalid: 'Invalid JSON file.',
    import_empty: 'No valid credentials (name + value) in the file.',
    value_required: 'Please enter a value.',
    key_invalid: 'Invalid key: UPPER_SNAKE_CASE (A–Z, 0–9, _).',
    save_failed: 'Save failed.',
    load_failed: 'Load failed.',
    no_permission: 'No permission for credentials (Chef or Admin role).',
  },
};

const KEY_RE = /^[A-Z][A-Z0-9_]{0,63}$/;

// Module-level copy, defaulted to German so the pure helpers below stay
// testable without a DOM. mount() swaps in the merged (locale + file) messages.
let text = labels.de;
const tr = (key) => text[key] ?? labels.de[key] ?? key;

// ---------------------------------------------------------------------------
// Pure helpers (exported for tests — no DOM, no command bus). None of these
// ever emit a secret value: credentials are write-only.
// ---------------------------------------------------------------------------

// Auto-reveal model (design-guide "Progressive Disclosure", outbound idiom):
// the metadata detail card is shown only when something is selected and the
// user has not collapsed it.
export function shouldRevealRecord(hasSelection, userCollapsed) {
  return Boolean(hasSelection) && !userCollapsed;
}

// Merge the daemon's catalog (known credentials) + extra (custom credentials)
// into one tagged list of metadata entries.
export function mergeEntries(catalog, extra) {
  const tag = (arr, source) => (Array.isArray(arr) ? arr : [])
    .filter((e) => e && typeof e === 'object')
    .map((e) => ({
      name: String(e.name || ''),
      description: String(e.description || ''),
      is_set: Boolean(e.is_set),
      updated_at: e.updated_at || null,
      source,
    }))
    .filter((e) => e.name);
  return [...tag(catalog, 'catalog'), ...tag(extra, 'extra')];
}

// Which counted band an entry belongs to. The left band splits Alle / Gesetzt
// (is_set) / Offen (not set) — a genuinely real, operator-facing distinction.
export function credentialBand(entry) {
  return entry && entry.is_set ? 'set' : 'open';
}

export function countsFor(rows) {
  const list = Array.isArray(rows) ? rows : [];
  const counts = { all: list.length, set: 0, open: 0 };
  for (const e of list) counts[credentialBand(e)] += 1;
  return counts;
}

// Apply the current grammar state (band + source filter + search) to the rows.
export function filterRows(rows, { band = 'all', source = 'all', search = '' } = {}) {
  const needle = String(search || '').trim().toLowerCase();
  return (Array.isArray(rows) ? rows : []).filter((e) => {
    if (band && band !== 'all' && credentialBand(e) !== band) return false;
    if (source && source !== 'all' && e.source !== source) return false;
    if (needle) {
      const hay = [e.name, e.description, e.source].filter(Boolean).join(' ').toLowerCase();
      if (!hay.includes(needle)) return false;
    }
    return true;
  });
}

function statusLabel(entry) {
  return entry && entry.is_set ? tr('status_set') : tr('status_unset');
}

function sourceLabel(entry) {
  return entry && entry.source === 'extra' ? tr('sourceExtra') : tr('sourceCatalog');
}

// A shard is a pure selector: it never renders a value, never expands inline
// and carries no per-row buttons (design-guide "Canonical Column Grammar").
// The two views are genuinely different densities (Betreiber-Direktive
// 31.08.2026), not the same row with a different class:
//   cards — 2-3 lines: bold mono key + status, source · description, and the
//           rotation date when the credential is set. Generous padding.
//   list  — exactly ONE line: key + a single short status meta on the right,
//           tight leading, maximum density.
export function credentialRow(entry, opts = {}) {
  const view = opts.view === 'list' ? 'list' : 'cards';
  const selected = Boolean(opts.selected);
  const name = String(entry?.name || '');
  const badge = '<span class="ctox-badge' + (entry?.is_set ? ' is-success' : '') + '" data-cred-status>' + esc(statusLabel(entry)) + '</span>';
  const attrs = ' class="ctox-list-item cred-row cred-row--' + view + (selected ? ' is-selected' : '') + '"'
    + ' role="button" tabindex="0" aria-selected="' + (selected ? 'true' : 'false') + '"'
    + ' data-context-record-id="' + esc(name) + '"'
    + ' data-context-record-type="credential"'
    + ' data-context-label="' + esc(name) + '"';
  if (view === 'list') {
    return '<div' + attrs + '><span class="cred-row-title">' + esc(name) + '</span>' + badge + '</div>';
  }
  const metaBits = [esc(sourceLabel(entry))];
  if (entry?.description) metaBits.push(esc(entry.description));
  const updated = entry?.is_set && entry?.updated_at ? esc(formatUpdated(entry.updated_at)) : '';
  return '<div' + attrs + '>'
    + '<div class="cred-row-head"><span class="cred-row-title">' + esc(name) + '</span>' + badge + '</div>'
    + '<div class="cred-row-meta">' + metaBits.join(' · ') + '</div>'
    + (updated ? '<div class="cred-row-meta cred-row-meta--sub">' + updated + '</div>' : '')
    + '</div>';
}

// The list body markup (shards or compact rows). App-API 4.0 (R11): an empty
// list renders nothing – no syncing shell, no empty text; states go to the zone.
export function renderRecordList(rows, opts = {}) {
  const list = Array.isArray(rows) ? rows : [];
  if (!list.length) return '';
  return list.map((e) => credentialRow(e, { view: opts.view, selected: e.name && e.name === opts.selectedName })).join('');
}

// Read-only metadata detail card for the selected credential (auto-reveal
// target). NEVER renders a value; offers delete when the credential is set.
export function recordDetailHtml(entry) {
  const name = String(entry?.name || '');
  const rows = [];
  if (entry?.description) rows.push(field(tr('field_description'), esc(entry.description)));
  rows.push(field(tr('field_status'), esc(statusLabel(entry))));
  rows.push(field(tr('field_source'), esc(sourceLabel(entry))));
  if (entry?.is_set && entry?.updated_at) rows.push(field(tr('field_updated'), esc(formatUpdated(entry.updated_at))));
  const deleteIcon = entry?.is_set
    ? '<button type="button" class="ctox-pane-icon" data-short-de="Löschen" data-action="delete" data-name="' + esc(name) + '" aria-label="' + esc(tr('btn_delete')) + '" title="' + esc(tr('btn_delete')) + '"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 7h16M9 7V5h6v2M6 7l1 13h10l1-13"/></svg></button>'
    : '';
  return '<header class="cred-detail-head">'
    + '<div class="cred-detail-titles">'
    + '<span class="ctox-badge' + (entry?.is_set ? ' is-success' : '') + '" data-cred-status>' + esc(statusLabel(entry)) + '</span>'
    + '<strong class="cred-detail-name">' + esc(name) + '</strong>'
    + '</div>'
    + '<div class="cred-detail-actions">'
    + deleteIcon
    + '<button type="button" class="ctox-pane-icon" data-action="collapse-detail" aria-label="' + esc(tr('closeDetail')) + '" title="' + esc(tr('closeDetail')) + '"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg></button>'
    + '</div>'
    + '</header>'
    + '<dl class="ctox-fields ctox-fields--stacked">' + rows.join('') + '</dl>';
}

function field(label, valueHtml) {
  return '<dt>' + esc(label) + '</dt><dd>' + valueHtml + '</dd>';
}

// Export is METADATA ONLY: names, descriptions, set-status and source — never a
// value. The header field states the write-only contract explicitly, and the
// payload carries no `value` key on any path.
export function buildExportPayload(rows, nowMs) {
  return {
    _comment: 'CTOX credentials metadata export — WRITE-ONLY. Secret values are never exported; import supplies values to (re)write.',
    kind: 'ctox-credentials-metadata',
    exported_at_ms: Number(nowMs) || 0,
    credentials: (Array.isArray(rows) ? rows : []).map((e) => ({
      name: String(e?.name || ''),
      description: String(e?.description || ''),
      is_set: Boolean(e?.is_set),
      source: e?.source === 'extra' ? 'extra' : 'catalog',
      updated_at: e?.updated_at || null,
    })),
  };
}

// Import creates credentials via the existing write path: it accepts either a
// bare [{name|key, value}] array or an object with a `credentials` array, and
// keeps only entries with a valid key AND a value to write.
export function parseImportEntries(raw) {
  const src = raw && typeof raw === 'object' ? raw : {};
  const list = Array.isArray(raw)
    ? raw
    : (Array.isArray(src.credentials) ? src.credentials : (raw && typeof raw === 'object' ? [raw] : []));
  const out = [];
  const seen = new Set();
  for (const item of list) {
    if (!item || typeof item !== 'object') continue;
    const name = String(item.name || item.key || '').trim();
    const value = item.value == null ? '' : String(item.value);
    if (!KEY_RE.test(name) || !value || seen.has(name)) continue;
    seen.add(name);
    out.push({ name, value });
  }
  return out;
}

// Command envelope for the RxDB/WebRTC command bus — shape unchanged.
// App-API 4.0: api.command(type, payload) – the command document ({id, type, payload, status, app_id, actor_id})
// is created by the API; routing fields travel inside the payload (PORTING.md R5).
export function buildCommandDoc(commandType, payload) {
  return {
    type: commandType,
    payload: {
      ...(payload || {}),
      record_id: payload?.name || 'credentials',
      inbound_channel: 'business_os.credentials',
      client_context: { source_module: 'credentials' },
    },
  };
}

// ---------------------------------------------------------------------------
// Mount
// ---------------------------------------------------------------------------

function langFrom(value) {
  return String(value || '').toLowerCase().startsWith('en') ? 'en' : 'de';
}

// App-API 4.0: mount(host, api, {restored}) → {unmount, snapshot}.
export async function mount(host, api, { restored } = {}) {
  let locale = langFrom(api.shell.locale.get());
  text = labels[locale];
  // api.shell.t übersteuert die eingebetteten Texte (Schlüsselraum offen, app-api.d.ts OFFEN-B).
  const t = (key) => {
    const scoped = 'credentials.' + key;
    const value = api.shell.t?.(scoped);
    if (typeof value === 'string' && value && value !== scoped) return value;
    return text[key] ?? labels.de[key] ?? key;
  };
  if (!host.dataset.app) host.dataset.app = api.app?.id || 'credentials';
  host.innerHTML = await loadMarkup();
  const disposers = [];

  const root = host.querySelector('[data-credentials-root]');
  const applyLabels = () => {
    root?.setAttribute('lang', locale);
    root?.querySelectorAll('[data-i18n]').forEach((node) => { node.textContent = t(node.dataset.i18n); });
    root?.querySelectorAll('[data-i18n-placeholder]').forEach((node) => { node.placeholder = t(node.dataset.i18nPlaceholder); });
    root?.querySelectorAll('[data-i18n-title]').forEach((node) => { node.title = node.ariaLabel = t(node.dataset.i18nTitle); });
    // Textdiät Regel 1: Sachinfo nur als Tooltip am Feld (Name bleibt das Label).
    root?.querySelectorAll('[data-i18n-tooltip]').forEach((node) => { node.title = t(node.dataset.i18nTooltip); });
  };
  applyLabels();
  // Pane-Grammatik verdrahtet die App selbst (PORTING.md R14).
  autoWirePaneGrammar(host);
  api.shell.setTitle(t('title'));

  const rail = root?.querySelector('.credentials-rail');
  const listEl = root?.querySelector('[data-cred-list]');
  const detailEl = root?.querySelector('[data-cred-detail]');
  const formEl = root?.querySelector('[data-cred-form]');
  const keyEl = root?.querySelector('[data-cred-key]');
  const valueEl = root?.querySelector('[data-cred-value]');
  const submitEl = root?.querySelector('[data-cred-submit]');
  const titleEl = root?.querySelector('[data-cred-title]');
  const viewToggleEl = root?.querySelector('[data-cred-view-toggle]');

  // Spiegel für die Darstellung (§10.2a Nr. 3); jede Mutation entscheidet der Core.
  const computeCanManage = () => {
    const perms = api.actor?.permissions?.get?.() || [];
    const role = String(api.actor?.role || '').toLowerCase().replace(/^business_os_/, '');
    return perms.includes(SECRETS_MANAGE) || ['admin', 'chef'].includes(role);
  };
  let canManage = computeCanManage();

  let rowsCache = [];
  let selectedName = null;
  let userCollapsed = false;
  let refreshing = false;
  let refreshQueued = false;
  // Module-owned view mode: the shell grammar only wires the two-button
  // [data-pg-view] radio pair, and this app renders ONE toggle button.
  let viewMode = 'cards';

  // ---- shell-wired grammar state (read straight from the pane DOM) ----------
  function readGrammar() {
    return {
      search: (rail?.querySelector('[data-pg-search]')?.value || '').trim().toLowerCase(),
      view: viewMode,
      band: rail?.querySelector('[data-pg-band][aria-selected="true"]')?.dataset.pgBand || 'all',
      source: rail?.querySelector('[data-pg-filter][data-pg-name="source"]')?.value || 'all',
    };
  }

  // The single toggle names the view it switches TO: glyph, aria-label and
  // title all move together, and aria-pressed never appears (it is an action,
  // not a state).
  function updateViewToggle() {
    if (!viewToggleEl) return;
    const listNow = viewMode === 'list';
    const label = listNow ? t('viewToCards') : t('viewToList');
    viewToggleEl.dataset.viewMode = listNow ? 'list' : 'cards';
    viewToggleEl.setAttribute('aria-label', label);
    viewToggleEl.title = label;
    viewToggleEl.removeAttribute('aria-pressed');
    viewToggleEl.querySelectorAll('[data-view-glyph]').forEach((glyph) => {
      glyph.hidden = glyph.dataset.viewGlyph !== (listNow ? 'cards' : 'list');
    });
  }
  function writeCounts(counts) {
    const pg = rail?.__ctoxPaneGrammar;
    if (pg && typeof pg.setCounts === 'function') { pg.setCounts(counts); return; }
    for (const [key, value] of Object.entries(counts)) {
      const node = rail?.querySelector('[data-pg-count="' + key + '"]');
      if (node) node.textContent = ' (' + value + ')';
    }
  }
  function writeFooter(str) {
    const pg = rail?.__ctoxPaneGrammar;
    if (pg && typeof pg.setFooter === 'function') { pg.setFooter(str); return; }
    const node = rail?.querySelector('[data-pg-footer]');
    if (node) node.textContent = str || '';
  }

  // Früher ctx.notifications: Meldungen gehen in die Zustandszone der Shell (stabile code je Fall).
  function toast(message, isError = false, code = isError ? 'credentials.error' : 'credentials.notice') {
    api.status.report({ level: isError ? 'error' : 'info', code, text: String(message ?? '') });
  }
  function hint(code, key) {
    api.status.report({ level: 'warn', code: 'credentials.' + code, text: t(key) });
  }

  // ---- render ---------------------------------------------------------------
  function renderDetail() {
    if (!detailEl) return;
    const rec = selectedName ? rowsCache.find((r) => r.name === selectedName) : null;
    const show = shouldRevealRecord(Boolean(rec), userCollapsed);
    detailEl.hidden = !show;
    detailEl.innerHTML = show ? recordDetailHtml(rec) : '';
    if (show) {
      detailEl.setAttribute('data-context-record-id', rec.name || '');
      detailEl.setAttribute('data-context-record-type', 'credential');
      detailEl.setAttribute('data-context-label', rec.name || '');
    }
    if (titleEl) titleEl.textContent = rec ? rec.name : t('newTitle');
    if (submitEl) submitEl.textContent = rec ? (rec.is_set ? t('btn_rotate') : t('btn_save')) : t('add_btn');
    if (valueEl) valueEl.placeholder = rec && rec.is_set ? t('ph_rotate') : t('ph_set');
  }

  function render() {
    if (!canManage) {
      if (listEl) listEl.innerHTML = '';
      hint('permission.missing', 'no_permission');
      writeCounts({ all: 0, set: 0, open: 0 });
      writeFooter('');
      if (detailEl) { detailEl.hidden = true; detailEl.innerHTML = ''; }
      setControlsEnabled(false);
      return;
    }
    const g = readGrammar();
    const filtered = filterRows(rowsCache, g);
    if (listEl) {
      // The well itself carries the view so the list variant can collapse the
      // inter-row gap to zero (maximum density) without a second class per row.
      listEl.dataset.credView = g.view;
      listEl.innerHTML = renderRecordList(filtered, { view: g.view, selectedName });
    }
    writeCounts(countsFor(rowsCache));
    // Textdiät Regel 3/5: Anzahl steht schon im Reiter – kein zweiter Zähler im Fuß.
    writeFooter('');
    renderDetail();
  }

  // ---- data (ctox.secret.list via api.command, result pushed) ---------------
  function refresh() {
    if (!canManage) { rowsCache = []; render(); return; }
    // Single-flight: a subscription burst must not fan out into parallel lists.
    if (refreshing) { refreshQueued = true; return; }
    refreshing = true;
    refreshQueued = false;
    sendCommand(LIST_COMMAND, {}, {
      done(result) {
        rowsCache = mergeEntries(result?.catalog, result?.extra);
        api.status.clear('credentials.list.failed');
      },
      failed(error) {
        toast((error || t('load_failed')), true, 'credentials.list.failed');
      },
      finally() {
        refreshing = false;
        render();
        if (refreshQueued) refresh();
      },
    });
  }

  // api.command legt das Befehlsdokument an und liefert sofort; der Status kommt gepusht (nie warten).
  function sendCommand(commandType, payload, on = {}) {
    const { type, payload: body } = buildCommandDoc(commandType, payload);
    const handle = api.command(type, body);
    const code = 'credentials.command.' + commandType;
    let off = null;
    let finished = false;
    const stop = () => { finished = true; off?.(); off = null; };
    const end = (fn, arg) => { try { fn?.(arg); } finally { on.finally?.(); } };
    off = handle.status.subscribe((value) => {
      if (finished) return;
      if (value?.status === 'done') {
        stop();
        api.status.clear(code);
        end(on.done, value.result);
      } else if (value?.status === 'failed') {
        stop();
        api.status.report({ level: 'error', code, text: commandType + ': ' + (value.error || 'failed'), ref: handle.id });
        end(on.failed, value.error);
      }
    });
    if (finished) off?.();
    handle.local.catch((error) => {
      if (finished) return;
      stop();
      api.status.report({ level: 'error', code, text: commandType + ': ' + (error?.code || 'local') + ' ' + (error?.message || error), ref: handle.id });
      end(on.failed, error?.message);
    });
    disposers.push(stop);
    return handle;
  }

  // Best-effort: strip a plaintext value that a put may have left in the local
  // command doc. The daemon redacts the durable record; until the Core owns this
  // for every client (V4-PORT.md OFFEN-2) the app keeps its local cleanup.
  function redactLocalCommand(commandId, name) {
    if (!commandId) return;
    try {
      const doc = api.get('business_commands', commandId).get();
      if (doc?.payload && Object.prototype.hasOwnProperty.call(doc.payload, 'value')) {
        // api.patch ist JSON-Merge-Patch (§10.2a): null entfernt das Feld.
        api.patch('business_commands', commandId, { payload: { value: null, name } });
      }
    } catch { /* durable record already redacted server-side */ }
  }

  // ---- selection / create-mode ---------------------------------------------
  function applyListSelection() {
    // Selection is an in-place class flip — a list rebuild would reset the
    // operator's scroll (design-guide: re-renders never move the operator).
    listEl?.querySelectorAll('[data-context-record-id]').forEach((row) => {
      const on = (row.getAttribute('data-context-record-id') || '') === String(selectedName || '');
      row.classList.toggle('is-selected', on);
      row.setAttribute('aria-selected', String(on));
    });
  }

  function selectRecord(name) {
    selectedName = name || null;
    userCollapsed = false;
    const rec = selectedName ? rowsCache.find((r) => r.name === selectedName) : null;
    if (keyEl) {
      keyEl.value = rec ? rec.name : '';
      keyEl.readOnly = Boolean(rec);
    }
    if (valueEl) valueEl.value = '';
    applyListSelection();
    renderDetail();
  }

  function startNew() {
    selectedName = null;
    userCollapsed = false;
    if (keyEl) { keyEl.value = ''; keyEl.readOnly = false; }
    if (valueEl) valueEl.value = '';
    applyListSelection();
    renderDetail();
    keyEl?.focus?.();
  }

  // ---- write path (ctox.secret.put / ctox.secret.delete) --------------------
  function onSubmit(event) {
    event.preventDefault();
    if (!canManage) return;
    const name = (keyEl?.value || '').trim();
    const value = valueEl?.value || '';
    if (!KEY_RE.test(name)) { hint('form.key_invalid', 'key_invalid'); return; }
    if (!value) { hint('form.value_required', 'value_required'); return; }
    api.status.clear('credentials.form.key_invalid');
    api.status.clear('credentials.form.value_required');
    if (valueEl) valueEl.value = '';
    selectedName = name;
    const handle = sendCommand(PUT_COMMAND, { name, value }, {
      done() { toast(t('saved').replace('{name}', name), false, 'credentials.secret.saved'); },
      finally() { redactLocalCommand(handle?.id, name); },
    });
  }

  async function handleDelete(name) {
    if (!name || !canManage) return;
    // Bestätigung im App-Fenster (api.shell.dialog, OFFEN-A); destruktiv → default:false.
    const confirmed = await api.shell.dialog({
      kind: 'confirm',
      title: t('btn_delete'),
      text: t('confirm_delete').replace('{name}', name),
      confirm_label: t('btn_delete'),
      default: false,
    });
    if (confirmed !== true) return;
    sendCommand(DELETE_COMMAND, { name }, {
      done() { toast(t('deleted').replace('{name}', name), false, 'credentials.secret.deleted'); },
    });
    if (selectedName === name) startNew();
  }

  // ---- import / export ------------------------------------------------------
  function exportRecords() {
    // Metadata only — the payload carries no secret value on any path.
    const payload = buildExportPayload(rowsCache, Date.now());
    let url = '';
    try {
      const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
      url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'credentials-metadata.json';
      a.rel = 'noopener';
      root?.appendChild(a);
      a.click();
      a.remove();
    } catch (error) {
      console.error('[credentials] export failed:', error);
    } finally {
      if (url) setTimeout(() => { try { URL.revokeObjectURL(url); } catch {} }, 4000);
    }
  }

  function importRecords() {
    if (!canManage) return;
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'application/json,.json';
    input.addEventListener('change', async () => {
      const file = input.files && input.files[0];
      if (!file) return;
      let parsed;
      try { parsed = JSON.parse(await file.text()); } catch { hint('import.invalid', 'import_invalid'); return; }
      const entries = parseImportEntries(parsed);
      if (!entries.length) { hint('import.empty', 'import_empty'); return; }
      let pending = entries.length;
      let count = 0;
      for (const entry of entries) {
        const handle = sendCommand(PUT_COMMAND, { name: entry.name, value: entry.value }, {
          done() { count += 1; },
          finally() {
            redactLocalCommand(handle?.id, entry.name);
            pending -= 1;
            if (!pending) toast(t('imported').replace('{count}', String(count)), false, 'credentials.import.done');
          },
        });
      }
    });
    input.click();
  }

  // ---- events ---------------------------------------------------------------
  function onListClick(event) {
    const row = event.target?.closest?.('[data-context-record-id]');
    if (!row || !listEl.contains(row)) return;
    selectRecord(row.getAttribute('data-context-record-id'));
  }
  function onListKey(event) {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    const row = event.target?.closest?.('[data-context-record-id]');
    if (!row || !listEl.contains(row)) return;
    event.preventDefault();
    selectRecord(row.getAttribute('data-context-record-id'));
  }
  function onAction(event) {
    const btn = event.target?.closest?.('[data-action]');
    if (!btn || !root?.contains(btn)) return;
    const action = btn.dataset.action;
    if (action === 'new') startNew();
    else if (action === 'import') importRecords();
    else if (action === 'export') exportRecords();
    else if (action === 'collapse-detail') { userCollapsed = true; renderDetail(); }
    else if (action === 'delete') handleDelete(btn.dataset.name || selectedName);
  }
  const onGrammarChange = () => { render(); };
  function onViewToggle() {
    viewMode = viewMode === 'list' ? 'cards' : 'list';
    updateViewToggle();
    render();
  }

  listEl?.addEventListener('click', onListClick);
  listEl?.addEventListener('keydown', onListKey);
  root?.addEventListener('click', onAction);
  formEl?.addEventListener('submit', onSubmit);
  rail?.addEventListener('ctox-pane-grammar-change', onGrammarChange);
  viewToggleEl?.addEventListener('click', onViewToggle);

  // Reactive: re-list when a ctox.secret.put/delete command lands in the shared
  // business_commands collection (own writes + peer writes). The selector
  // excludes ctox.secret.list so our own listing never re-triggers a refresh.
  let lastCommandSignature = null;
  function subscribeCommands() {
    if (!canManage) return () => {};
    return api.query('business_commands', {
      selector: { $or: [{ type: { $in: [PUT_COMMAND, DELETE_COMMAND] } }, { command_type: { $in: [PUT_COMMAND, DELETE_COMMAND] } }] },
    }).subscribe((docs) => {
      // Only a changed put/delete set re-lists (field report 11.09.2026, thesen:
      // re-emits for unrelated writes made every listing trigger the next one).
      const signature = JSON.stringify((Array.isArray(docs) ? docs : []).map((row) => [row?.id || row?.command_id || '', row?.status || '', row?.execution_phase || '']));
      if (signature === lastCommandSignature) return;
      const first = lastCommandSignature === null;
      lastCommandSignature = signature;
      if (!first) refresh();
    });
  }
  let commandsOff = subscribeCommands();
  disposers.push(() => commandsOff());
  disposers.push(api.actor.permissions.subscribe(() => {
    const next = computeCanManage();
    if (next === canManage) return;
    canManage = next;
    if (canManage) api.status.clear('credentials.permission.missing');
    commandsOff();
    commandsOff = subscribeCommands();
    setControlsEnabled(canManage);
    refresh();
  }));
  disposers.push(api.shell.locale.subscribe((value) => {
    const next = langFrom(value);
    if (next === locale) return;
    locale = next;
    text = labels[locale];
    applyLabels();
    updateViewToggle();
    render();
  }));

  // Usable window before the list round-trip completes.
  applyRestored(restored);
  updateViewToggle();
  render();
  if (!canManage) setControlsEnabled(false);
  refresh();

  // Snapshot für den Live-Remount: Auswahl, Ansicht, eingegebener Schlüssel. Den Wert nimmt der Snapshot
  // bewusst NICHT mit (write-only: Geheimnisse verlassen das Eingabefeld nur über ctox.secret.put).
  function snapshot() {
    return {
      v: 1,
      selectedName,
      userCollapsed,
      viewMode,
      search: rail?.querySelector('[data-pg-search]')?.value || '',
      keyDraft: selectedName ? '' : (keyEl?.value || ''),
    };
  }
  function applyRestored(snap) {
    if (!snap || typeof snap !== 'object') return;
    if (typeof snap.selectedName === 'string') selectedName = snap.selectedName;
    if (typeof snap.userCollapsed === 'boolean') userCollapsed = snap.userCollapsed;
    if (snap.viewMode === 'list' || snap.viewMode === 'cards') viewMode = snap.viewMode;
    const search = rail?.querySelector('[data-pg-search]');
    if (search && typeof snap.search === 'string') search.value = snap.search;
    if (keyEl) {
      keyEl.value = selectedName || (typeof snap.keyDraft === 'string' ? snap.keyDraft : '');
      keyEl.readOnly = Boolean(selectedName);
    }
  }

  function setControlsEnabled(enabled) {
    root?.querySelectorAll('input, button').forEach((node) => { node.disabled = !enabled; });
  }

  return {
    unmount() {
      for (const dispose of disposers.splice(0)) {
        try { dispose(); } catch (error) { console.warn('[credentials] unsubscribe failed', error); }
      }
      listEl?.removeEventListener('click', onListClick);
      listEl?.removeEventListener('keydown', onListKey);
      root?.removeEventListener('click', onAction);
      formEl?.removeEventListener('submit', onSubmit);
      rail?.removeEventListener('ctox-pane-grammar-change', onGrammarChange);
      viewToggleEl?.removeEventListener('click', onViewToggle);
      host.replaceChildren();
    },
    snapshot,
  };
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

function formatUpdated(value) {
  try {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return '';
    return tr('updated').replace('{date}', date.toLocaleDateString(text === labels.en ? 'en-US' : 'de-DE', { year: 'numeric', month: 'short', day: 'numeric' }));
  } catch { return ''; }
}

async function loadMarkup() {
  // index.html liefert der Service Worker versioniert (§16.15) – kein Cache-Buster; CSS lädt die Shell.
  const html = await fetch(new URL('./index.html', import.meta.url)).then((r) => r.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((n) => n.remove());
  return doc.body.innerHTML;
}

function esc(value) {
  return String(value == null ? '' : value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

// Pure logic surfaced for unit tests (no DOM needed); see credentials.test.mjs.
export const __credentialsTestHooks = {
  KEY_RE,
  credentialRow,
  recordDetailHtml,
  buildCommandDoc,
  buildExportPayload,
  parseImportEntries,
  mergeEntries,
  credentialBand,
  countsFor,
  filterRows,
};
