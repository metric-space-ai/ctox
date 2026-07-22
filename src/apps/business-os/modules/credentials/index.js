import { loadModuleMessages } from '../../shared/i18n.js';
import { canUseBusinessPermission, BusinessOsPermissions } from '../../shared/permissions.js?v=20260623-role-session';

// Write-only credentials manager. The browser never receives a secret value:
// it dispatches ctox.secret.{list,put,delete} control commands over the
// RxDB/WebRTC command bus, and the daemon redacts the value from the persisted
// command record (see store.rs accept_rxdb_business_command). Listing returns
// metadata only (name + description + set/unset status). The list is REACTIVE:
// the module subscribes to the shared business_commands collection and re-lists
// when a ctox.secret.put/delete command lands (no manual refresh button).

const MOD_BUILD = '20260721-credentials-ia';
const LIST_COMMAND = 'ctox.secret.list';
const PUT_COMMAND = 'ctox.secret.put';
const DELETE_COMMAND = 'ctox.secret.delete';

const labels = {
  de: {
    kicker: 'CTOX',
    title: 'Zugangsdaten',
    listTitle: 'Zugangsdaten',
    newTitle: 'Neue Zugangsdaten',
    editKicker: 'Zugangsdatum',
    newKicker: 'Neu',
    subtitle: 'Write-only: Werte werden verschlüsselt im CTOX-Secret-Store abgelegt und nie an den Browser zurückgegeben.',
    newAction: 'Neue Zugangsdaten',
    importAction: 'Importieren',
    exportAction: 'Exportieren',
    searchPlaceholder: 'Suchen...',
    closeDetail: 'Details schließen',
    sourceAll: 'Alle Quellen',
    sourceCatalog: 'Katalog',
    sourceExtra: 'Eigene',
    bandAll: 'Alle',
    bandSet: 'Gesetzt',
    bandOpen: 'Offen',
    keyLabel: 'Schlüssel',
    valueLabel: 'Wert',
    valueHint: 'Das Wertfeld bleibt immer leer — gespeicherte Werte werden nie zurückgegeben.',
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
    empty_all: 'Keine Zugangsdaten konfiguriert.',
    empty_filtered: 'Kein Eintrag passt zum Filter.',
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
    no_permission: 'Du hast keine Berechtigung, Zugangsdaten zu verwalten (Rolle Chef oder Admin erforderlich).',
  },
  en: {
    kicker: 'CTOX',
    title: 'Credentials',
    listTitle: 'Credentials',
    newTitle: 'New credential',
    editKicker: 'Credential',
    newKicker: 'New',
    subtitle: 'Write-only: values are stored encrypted in the CTOX secret store and never returned to the browser.',
    newAction: 'New credential',
    importAction: 'Import',
    exportAction: 'Export',
    searchPlaceholder: 'Search...',
    closeDetail: 'Close details',
    sourceAll: 'All sources',
    sourceCatalog: 'Catalog',
    sourceExtra: 'Custom',
    bandAll: 'All',
    bandSet: 'Set',
    bandOpen: 'Pending',
    keyLabel: 'Key',
    valueLabel: 'Value',
    valueHint: 'The value field is always empty — stored values are never returned.',
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
    empty_all: 'No credentials configured.',
    empty_filtered: 'No entry matches the filter.',
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
    no_permission: 'You do not have permission to manage credentials (Chef or Admin role required).',
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

// A shard is a pure selector: mono key + ONE muted meta line + a status badge.
// No inline expansion, no per-row buttons (design-guide "Canonical Column
// Grammar"). It never renders a value.
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
  return '<div' + attrs + '>'
    + '<div class="cred-row-head"><span class="cred-row-title">' + esc(name) + '</span>' + badge + '</div>'
    + '<div class="cred-row-meta">' + metaBits.join(' · ') + '</div>'
    + '</div>';
}

// The list body markup (shards or compact rows), or the empty state.
export function renderRecordList(rows, opts = {}) {
  const list = Array.isArray(rows) ? rows : [];
  if (!list.length) {
    return '<div class="ctox-empty"><strong>' + esc(opts.emptyText || tr('empty_all')) + '</strong></div>';
  }
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
    ? '<button type="button" class="ctox-pane-icon" data-action="delete" data-name="' + esc(name) + '" aria-label="' + esc(tr('btn_delete')) + '" title="' + esc(tr('btn_delete')) + '"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 7h16M9 7V5h6v2M6 7l1 13h10l1-13"/></svg></button>'
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
export function buildCommandDoc(commandType, payload, commandId) {
  return {
    id: commandId,
    module: 'credentials',
    command_type: commandType,
    record_id: payload?.name || 'credentials',
    inbound_channel: 'business_os.credentials',
    payload: payload || {},
    client_context: { source_module: 'credentials' },
  };
}

// ---------------------------------------------------------------------------
// Mount
// ---------------------------------------------------------------------------

export async function mount(ctx) {
  const locale = String(ctx.locale || document.documentElement.lang || 'de').toLowerCase().startsWith('en') ? 'en' : 'de';
  const messages = await loadModuleMessages(import.meta.url, locale, labels);
  text = messages;
  const t = (key) => messages[key] ?? labels.de[key] ?? key;

  await ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();

  const root = ctx.host.querySelector('[data-credentials-root]');
  root?.setAttribute('lang', locale);
  root?.querySelectorAll('[data-i18n]').forEach((node) => { node.textContent = t(node.dataset.i18n); });
  root?.querySelectorAll('[data-i18n-placeholder]').forEach((node) => { node.placeholder = t(node.dataset.i18nPlaceholder); });
  root?.querySelectorAll('[data-i18n-title]').forEach((node) => { node.title = node.ariaLabel = t(node.dataset.i18nTitle); });

  const rail = root?.querySelector('.credentials-rail');
  const listEl = root?.querySelector('[data-cred-list]');
  const detailEl = root?.querySelector('[data-cred-detail]');
  const formEl = root?.querySelector('[data-cred-form]');
  const keyEl = root?.querySelector('[data-cred-key]');
  const valueEl = root?.querySelector('[data-cred-value]');
  const submitEl = root?.querySelector('[data-cred-submit]');
  const gateEl = root?.querySelector('[data-cred-gate]');
  const titleEl = root?.querySelector('[data-cred-title]');
  const modeEl = root?.querySelector('[data-cred-mode]');

  const canManage = canUseBusinessPermission({
    session: ctx.session,
    governance: ctx.governance,
    permission: BusinessOsPermissions.SecretsManage,
    scopeType: 'workspace',
  });

  let rowsCache = [];
  let selectedName = null;
  let userCollapsed = false;
  let refreshing = false;
  let refreshQueued = false;

  const collection = () => { try { return ctx.db?.collection?.('business_commands') || null; } catch { return null; } };

  // ---- shell-wired grammar state (read straight from the pane DOM) ----------
  function readGrammar() {
    return {
      search: (rail?.querySelector('[data-pg-search]')?.value || '').trim().toLowerCase(),
      view: rail?.querySelector('[data-pg-view][aria-pressed="true"]')?.dataset.pgView || 'cards',
      band: rail?.querySelector('[data-pg-band][aria-selected="true"]')?.dataset.pgBand || 'all',
      source: rail?.querySelector('[data-pg-filter][data-pg-name="source"]')?.value || 'all',
    };
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

  const GATE_VARIANTS = { ok: ' is-success', block: ' is-danger', offline: ' is-warning' };
  function setGate(message, kind) {
    if (!gateEl) return;
    gateEl.className = 'ctox-callout' + (GATE_VARIANTS[kind] || '');
    gateEl.textContent = message || '';
    gateEl.hidden = !message;
  }
  function toast(message, isError = false) {
    ctx.notifications?.show?.({ type: isError ? 'error' : 'success', title: t('title'), message: String(message ?? '') });
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
    if (modeEl) modeEl.textContent = rec ? t('editKicker') : t('newKicker');
    if (titleEl) titleEl.textContent = rec ? rec.name : t('newTitle');
    if (submitEl) submitEl.textContent = rec ? (rec.is_set ? t('btn_rotate') : t('btn_save')) : t('add_btn');
    if (valueEl) valueEl.placeholder = rec && rec.is_set ? t('ph_rotate') : t('ph_set');
  }

  function render() {
    if (!canManage) {
      if (listEl) listEl.innerHTML = '<div class="ctox-empty"><strong>' + esc(t('no_permission')) + '</strong></div>';
      writeCounts({ all: 0, set: 0, open: 0 });
      writeFooter('— ' + t('entries'));
      if (detailEl) { detailEl.hidden = true; detailEl.innerHTML = ''; }
      setControlsEnabled(false);
      return;
    }
    const g = readGrammar();
    const filtered = filterRows(rowsCache, g);
    if (listEl) {
      const emptyText = rowsCache.length ? t('empty_filtered') : t('empty_all');
      listEl.innerHTML = renderRecordList(filtered, { view: g.view, selectedName, emptyText });
    }
    writeCounts(countsFor(rowsCache));
    const scope = { all: t('bandAll'), set: t('bandSet'), open: t('bandOpen') }[g.band] || t('bandAll');
    writeFooter(filtered.length + ' ' + t('entries') + ' · ' + scope);
    renderDetail();
  }

  // ---- data (command bus round trip) ----------------------------------------
  async function refresh() {
    if (!canManage) { rowsCache = []; render(); return; }
    // Single-flight: a subscription burst must not fan out into parallel lists.
    if (refreshing) { refreshQueued = true; return; }
    refreshing = true;
    try {
      do {
        refreshQueued = false;
        try {
          const { outcome } = await sendCommand(LIST_COMMAND, {});
          rowsCache = mergeEntries(outcome?.catalog, outcome?.extra);
        } catch (error) {
          toast(error?.message || t('load_failed'), true);
          rowsCache = [];
        }
      } while (refreshQueued);
    } finally {
      refreshing = false;
    }
    render();
  }

  async function sendCommand(commandType, payload) {
    const bus = ctx.commandBus;
    if (!bus?.dispatch) throw new Error('command bus unavailable');
    const commandId = 'cmd_cred_' + Date.now() + '_' + Math.floor(Math.random() * 1e6);
    try { await ctx.sync?.startCollection?.('business_commands'); } catch { /* bridge may already run */ }
    const busResult = await bus.dispatch(buildCommandDoc(commandType, payload, commandId));
    return { commandId, outcome: busResult?.result || null };
  }

  // Best-effort: strip a plaintext value that a failed/timed-out put may have
  // left in the local pending command doc. On success the daemon already
  // redacted the durable record, so this is a no-op.
  async function redactLocalCommand(commandId, name) {
    if (!commandId) return;
    try {
      const col = collection();
      if (!col?.findOne) return;
      const doc = await col.findOne(commandId).exec();
      if (doc?.payload && Object.prototype.hasOwnProperty.call(doc.payload, 'value')) {
        await doc.patch({ payload: { name } });
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
    setGate('');
    applyListSelection();
    renderDetail();
  }

  function startNew() {
    selectedName = null;
    userCollapsed = false;
    if (keyEl) { keyEl.value = ''; keyEl.readOnly = false; }
    if (valueEl) valueEl.value = '';
    setGate('');
    applyListSelection();
    renderDetail();
    keyEl?.focus?.();
  }

  // ---- write path (ctox.secret.put / ctox.secret.delete) --------------------
  async function onSubmit(event) {
    event.preventDefault();
    if (!canManage) return;
    const name = (keyEl?.value || '').trim();
    const value = valueEl?.value || '';
    if (!KEY_RE.test(name)) { setGate(t('key_invalid'), 'block'); return; }
    if (!value) { setGate(t('value_required'), 'block'); return; }
    if (valueEl) valueEl.value = '';
    let commandId = null;
    try {
      const result = await sendCommand(PUT_COMMAND, { name, value });
      commandId = result.commandId;
      selectedName = name;
      toast(t('saved').replace('{name}', name));
    } catch (error) {
      toast(error?.message || t('save_failed'), true);
    } finally {
      await redactLocalCommand(commandId, name);
      await refresh();
    }
  }

  async function handleDelete(name) {
    if (!name || !canManage) return;
    if (!window.confirm(t('confirm_delete').replace('{name}', name))) return;
    try {
      await sendCommand(DELETE_COMMAND, { name });
      toast(t('deleted').replace('{name}', name));
    } catch (error) {
      toast(error?.message || t('save_failed'), true);
    }
    if (selectedName === name) startNew();
    await refresh();
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
      try { parsed = JSON.parse(await file.text()); } catch { setGate(t('import_invalid'), 'block'); return; }
      const entries = parseImportEntries(parsed);
      if (!entries.length) { setGate(t('import_empty'), 'block'); return; }
      let count = 0;
      for (const entry of entries) {
        let commandId = null;
        try {
          const result = await sendCommand(PUT_COMMAND, { name: entry.name, value: entry.value });
          commandId = result.commandId;
          count += 1;
        } catch (error) {
          console.error('[credentials] import put failed:', error);
        } finally {
          await redactLocalCommand(commandId, entry.name);
        }
      }
      setGate(t('imported').replace('{count}', String(count)), 'ok');
      await refresh();
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

  listEl?.addEventListener('click', onListClick);
  listEl?.addEventListener('keydown', onListKey);
  root?.addEventListener('click', onAction);
  formEl?.addEventListener('submit', onSubmit);
  rail?.addEventListener('ctox-pane-grammar-change', onGrammarChange);

  // Reactive: re-list when a ctox.secret.put/delete command lands in the shared
  // business_commands collection (own writes + peer writes). The selector
  // excludes ctox.secret.list so our own listing never re-triggers a refresh.
  let subscription = null;
  if (canManage) {
    const col = collection();
    if (col?.find) {
      try {
        subscription = col
          .find({ selector: { command_type: { $in: [PUT_COMMAND, DELETE_COMMAND] } } })
          .$?.subscribe?.(() => { refresh(); });
      } catch { /* live sync optional; explicit refresh after writes still runs */ }
    }
  }

  // Usable window before the list round-trip completes.
  render();
  if (!canManage) setControlsEnabled(false);
  void refresh();

  function setControlsEnabled(enabled) {
    root?.querySelectorAll('input, button').forEach((node) => { node.disabled = !enabled; });
  }

  return () => {
    try { subscription?.unsubscribe?.(); } catch {}
    listEl?.removeEventListener('click', onListClick);
    listEl?.removeEventListener('keydown', onListKey);
    root?.removeEventListener('click', onAction);
    formEl?.removeEventListener('submit', onSubmit);
    rail?.removeEventListener('ctox-pane-grammar-change', onGrammarChange);
    ctx.host.replaceChildren();
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

async function ensureStyles() {
  const cssVersion = String(import.meta.url).split('?v=')[1] || MOD_BUILD;
  const cssHref = new URL('./index.css', import.meta.url).pathname + (cssVersion ? `?v=${cssVersion}` : '');
  let link = document.querySelector('link[data-credentials-style]');
  if (!link) {
    link = document.createElement('link');
    link.rel = 'stylesheet';
    link.dataset.credentialsStyle = 'true';
    document.head.append(link);
  }
  if (link.getAttribute('href') !== cssHref) link.href = cssHref;
}

async function loadMarkup() {
  const markupVersion = String(import.meta.url).split('?v=')[1] || MOD_BUILD;
  const markupHref = new URL('./index.html', import.meta.url).pathname + (markupVersion ? `?v=${markupVersion}` : '');
  const html = await fetch(markupHref).then((r) => r.text());
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
