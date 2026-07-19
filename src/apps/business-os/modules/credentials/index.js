import { loadModuleMessages } from '../../shared/i18n.js';
import { canUseBusinessPermission, BusinessOsPermissions } from '../../shared/permissions.js?v=20260623-role-session';

// Write-only credentials manager. The browser never receives a secret value:
// it dispatches ctox.secret.{list,put,delete} control commands over the
// RxDB/WebRTC command bus, and the daemon redacts the value from the persisted
// command record (see store.rs accept_rxdb_business_command). Listing returns
// metadata only (name + description + set/unset status).

const labels = {
  de: {
    title: 'Zugangsdaten',
    subtitle: 'Write-only: Werte werden verschlüsselt im CTOX-Secret-Store abgelegt und nie an den Browser zurückgegeben.',
    refresh: 'Aktualisieren',
    extra_title: 'Weitere Zugangsdaten',
    add_title: 'Eigene Zugangsdaten hinzufügen',
    key_label: 'Schlüssel',
    value_label: 'Wert',
    add_btn: 'Hinzufügen',
    status_set: 'Gesetzt',
    status_unset: 'Nicht gesetzt',
    updated: 'aktualisiert {date}',
    ph_set: 'Wert eingeben',
    ph_rotate: 'Neuen Wert (rotieren)',
    btn_save: 'Speichern',
    btn_rotate: 'Rotieren',
    btn_delete: 'Löschen',
    confirm_delete: '{name} wirklich entfernen?',
    saved: '{name} gespeichert',
    deleted: '{name} entfernt',
    value_required: 'Bitte einen Wert eingeben.',
    key_invalid: 'Ungültiger Schlüssel: UPPER_SNAKE_CASE (A–Z, 0–9, _).',
    save_failed: 'Speichern fehlgeschlagen.',
    load_failed: 'Laden fehlgeschlagen.',
    no_permission: 'Du hast keine Berechtigung, Zugangsdaten zu verwalten (Rolle Chef oder Admin erforderlich).',
    empty_known: 'Keine bekannten Zugangsdaten konfiguriert.',
  },
  en: {
    title: 'Credentials',
    subtitle: 'Write-only: values are stored encrypted in the CTOX secret store and never returned to the browser.',
    refresh: 'Refresh',
    extra_title: 'Other credentials',
    add_title: 'Add a custom credential',
    key_label: 'Key',
    value_label: 'Value',
    add_btn: 'Add',
    status_set: 'Set',
    status_unset: 'Not set',
    updated: 'updated {date}',
    ph_set: 'Enter value',
    ph_rotate: 'New value (rotate)',
    btn_save: 'Save',
    btn_rotate: 'Rotate',
    btn_delete: 'Remove',
    confirm_delete: 'Remove {name}?',
    saved: '{name} saved',
    deleted: '{name} removed',
    value_required: 'Please enter a value.',
    key_invalid: 'Invalid key: UPPER_SNAKE_CASE (A–Z, 0–9, _).',
    save_failed: 'Save failed.',
    load_failed: 'Load failed.',
    no_permission: 'You do not have permission to manage credentials (Chef or Admin role required).',
    empty_known: 'No known credentials configured.',
  },
};

const KEY_RE = /^[A-Z][A-Z0-9_]{0,63}$/;

const state = {
  ctx: null,
  t: (key, fallback) => fallback ?? key,
  canManage: false,
  catalog: [],
  extra: [],
};

const els = {};

export async function mount(ctx) {
  state.ctx = ctx;

  const styleLink = document.createElement('link');
  styleLink.rel = 'stylesheet';
  styleLink.href = new URL('./index.css', import.meta.url).href;
  styleLink.id = 'credentials-module-styles';
  document.head.appendChild(styleLink);

  const messages = await loadModuleMessages(import.meta.url, ctx.locale, labels);
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;

  state.canManage = canUseBusinessPermission({
    session: ctx.session,
    governance: ctx.governance,
    permission: BusinessOsPermissions.SecretsManage,
    scopeType: 'workspace',
  });

  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  ctx.host.innerHTML = await loadModuleMarkup();

  bindElements(ctx.host);
  applyStaticLabels();
  wireEvents();

  // Listing credentials is a command-bus round trip and can legitimately
  // take seconds while the shared business_commands projection is busy. The
  // shell window must be usable before that hydration completes.
  render();
  void refresh(ctx);

  return () => {
    if (state.ctx === ctx) state.ctx = null;
    styleLink.remove();
  };
}

async function loadModuleMarkup() {
  try {
    const response = await fetch(new URL('./index.html', import.meta.url));
    if (response.ok) return await response.text();
  } catch (_error) {
    /* fall through to minimal markup */
  }
  return '<main class="ctox-workspace ctox-workspace--single credentials-module" data-credentials-root>'
    + '<section class="ctox-pane"><div class="ctox-record-workbench__body">'
    + '<div class="ctox-callout" data-cred-notice></div>'
    + '<div class="ctox-record-list" data-cred-list></div>'
    + '<div class="ctox-record-list" data-cred-extra-list></div>'
    + '</div></section></main>';
}

function bindElements(host) {
  els.host = host;
  els.root = host.querySelector('[data-credentials-root]');
  els.notice = host.querySelector('[data-cred-notice]');
  els.list = host.querySelector('[data-cred-list]');
  els.extraSection = host.querySelector('[data-cred-extra-section]');
  els.extraList = host.querySelector('[data-cred-extra-list]');
  els.addSection = host.querySelector('[data-cred-add-section]');
  els.addForm = host.querySelector('[data-cred-add]');
  els.addKey = host.querySelector('[data-add-key]');
  els.addValue = host.querySelector('[data-add-value]');
  els.toggleAdd = host.querySelector('[data-toggle-add]');
}

function applyStaticLabels() {
  const set = (selector, key) => {
    const node = els.host.querySelector(selector);
    if (node) node.textContent = state.t(key);
  };
  set('[data-cred-title]', 'title');
  set('[data-cred-subtitle]', 'subtitle');
  // The refresh action is a compact icon button — label it for a11y/tooltip
  // instead of replacing its SVG content.
  const refreshButton = els.host.querySelector('[data-cred-refresh-label]');
  if (refreshButton) {
    refreshButton.setAttribute('aria-label', state.t('refresh'));
    refreshButton.setAttribute('title', state.t('refresh'));
  }
  set('[data-cred-extra-title]', 'extra_title');
  set('[data-cred-add-title]', 'add_title');
  set('[data-cred-key-label]', 'key_label');
  set('[data-cred-value-label]', 'value_label');
  set('[data-cred-add-btn]', 'add_btn');
  if (els.toggleAdd) {
    els.toggleAdd.setAttribute('aria-label', state.t('add_title'));
    els.toggleAdd.setAttribute('title', state.t('add_title'));
  }
  if (els.addKey) els.addKey.placeholder = 'CUSTOM_KEY';
}

function wireEvents() {
  // The add-custom-credential form is a rare action: collapsed by default and
  // revealed on demand via the header toggle (threads-style disclosure).
  els.toggleAdd?.addEventListener('click', () => {
    const hidden = els.root?.classList.toggle('is-add-hidden');
    els.toggleAdd.setAttribute('aria-pressed', hidden ? 'false' : 'true');
  });
  els.host.addEventListener('click', async (event) => {
    const button = event.target.closest('[data-action]');
    if (!button) return;
    const action = button.dataset.action;
    if (action === 'refresh') {
      event.preventDefault();
      await refresh();
    } else if (action === 'save') {
      event.preventDefault();
      await handleSave(button.dataset.key);
    } else if (action === 'delete') {
      event.preventDefault();
      await handleDelete(button.dataset.key);
    }
  });
  els.addForm?.addEventListener('submit', async (event) => {
    event.preventDefault();
    await handleAdd();
  });
}

async function refresh(expectedCtx = state.ctx) {
  if (!state.canManage) {
    if (state.ctx !== expectedCtx) return;
    showNotice(state.t('no_permission'));
    state.catalog = [];
    state.extra = [];
    render();
    setControlsEnabled(false);
    return;
  }
  hideNotice();
  try {
    const { outcome } = await sendCommand('ctox.secret.list', {});
    if (state.ctx !== expectedCtx) return;
    state.catalog = Array.isArray(outcome?.catalog) ? outcome.catalog : [];
    state.extra = Array.isArray(outcome?.extra) ? outcome.extra : [];
  } catch (error) {
    if (state.ctx !== expectedCtx) return;
    toast(error?.message || state.t('load_failed'), true);
    state.catalog = [];
    state.extra = [];
  }
  render();
}

function render() {
  if (!els.list) return;
  if (!state.canManage) {
    els.list.innerHTML = `<div class="ctox-empty">${esc(state.t('no_permission'))}</div>`;
    if (els.extraSection) els.extraSection.hidden = true;
    return;
  }
  if (!state.catalog.length) {
    els.list.innerHTML = `<div class="ctox-empty">${esc(state.t('empty_known'))}</div>`;
  } else {
    els.list.innerHTML = state.catalog.map((entry) => rowHtml(entry, false)).join('');
  }
  if (els.extraSection && els.extraList) {
    if (state.extra.length) {
      els.extraSection.hidden = false;
      els.extraList.innerHTML = state.extra.map((entry) => rowHtml(entry, true)).join('');
    } else {
      els.extraSection.hidden = true;
      els.extraList.innerHTML = '';
    }
  }
}

function rowHtml(entry, isExtra) {
  const name = String(entry?.name || '');
  const description = String(entry?.description || '');
  const isSet = Boolean(entry?.is_set);
  const statusText = isSet
    ? `${state.t('status_set')}${entry?.updated_at ? ' · ' + formatUpdated(entry.updated_at) : ''}`
    : state.t('status_unset');
  const placeholder = isSet ? state.t('ph_rotate') : state.t('ph_set');
  const saveLabel = isSet ? state.t('btn_rotate') : state.t('btn_save');
  const deleteButton = isSet
    ? `<button type="button" class="ctox-button ctox-button--sm is-danger" data-action="delete" data-key="${esc(name)}">${esc(state.t('btn_delete'))}</button>`
    : '';
  return `<div class="cred-row" data-key="${esc(name)}" data-context-record-id="${esc(name)}" data-context-record-type="credential" data-context-label="${esc(name)}">
    <div class="cred-meta">
      <span class="cred-name">${esc(name)}</span>
      ${description ? `<span class="cred-desc">${esc(description)}</span>` : ''}
      <span class="ctox-badge${isSet ? ' is-success' : ''}" data-cred-status>${esc(statusText)}</span>
    </div>
    <div class="cred-actions">
      <input type="password" class="ctox-input" data-value-for="${esc(name)}" placeholder="${esc(placeholder)}" autocomplete="new-password" />
      <button type="button" class="ctox-button ctox-button--sm" data-action="save" data-key="${esc(name)}">${esc(saveLabel)}</button>
      ${deleteButton}
    </div>
  </div>`;
}

async function handleSave(name) {
  if (!name || !state.canManage) return;
  const input = els.host.querySelector(`[data-value-for="${cssEscape(name)}"]`);
  let value = input ? input.value : '';
  if (!value) {
    toast(state.t('value_required'), true);
    return;
  }
  if (input) input.value = '';
  let commandId = null;
  try {
    const result = await sendCommand('ctox.secret.put', { name, value });
    commandId = result.commandId;
    toast(state.t('saved').replace('{name}', name));
  } catch (error) {
    toast(error?.message || state.t('save_failed'), true);
  } finally {
    value = '';
    // Best-effort: if the put failed/timed out the local pending command may
    // still hold the plaintext value; strip it. On success the daemon already
    // replaced the doc with a redacted projection, so this is a no-op.
    await redactLocalCommand(commandId, name);
    await refresh();
  }
}

async function handleDelete(name) {
  if (!name || !state.canManage) return;
  if (!window.confirm(state.t('confirm_delete').replace('{name}', name))) return;
  try {
    await sendCommand('ctox.secret.delete', { name });
    toast(state.t('deleted').replace('{name}', name));
  } catch (error) {
    toast(error?.message || state.t('save_failed'), true);
  }
  await refresh();
}

async function handleAdd() {
  if (!state.canManage) return;
  const name = (els.addKey?.value || '').trim();
  const value = els.addValue?.value || '';
  if (!KEY_RE.test(name)) {
    toast(state.t('key_invalid'), true);
    return;
  }
  if (!value) {
    toast(state.t('value_required'), true);
    return;
  }
  if (els.addValue) els.addValue.value = '';
  let commandId = null;
  try {
    const result = await sendCommand('ctox.secret.put', { name, value });
    commandId = result.commandId;
    if (els.addKey) els.addKey.value = '';
    toast(state.t('saved').replace('{name}', name));
  } catch (error) {
    toast(error?.message || state.t('save_failed'), true);
  } finally {
    await redactLocalCommand(commandId, name);
    await refresh();
  }
}

function buildCommandDoc(commandType, payload, commandId) {
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

async function sendCommand(commandType, payload) {
  const bus = state.ctx?.commandBus;
  if (!bus?.dispatch) throw new Error('command bus unavailable');
  const commandId = `cmd_cred_${Date.now()}_${Math.floor(Math.random() * 1e6)}`;
  try {
    await state.ctx?.sync?.startCollection?.('business_commands');
  } catch (_error) {
    /* sync bridge may already be running */
  }
  const busResult = await bus.dispatch(buildCommandDoc(commandType, payload, commandId));
  return { commandId, outcome: busResult?.result || null };
}

async function redactLocalCommand(commandId, name) {
  if (!commandId) return;
  try {
    const collection = state.ctx?.db?.collection?.('business_commands');
    if (!collection) return;
    const doc = await collection.findOne(commandId).exec();
    if (doc?.payload && Object.prototype.hasOwnProperty.call(doc.payload, 'value')) {
      await doc.patch({ payload: { name } });
    }
  } catch (_error) {
    /* best-effort only; the daemon already redacts the durable record */
  }
}

function setControlsEnabled(enabled) {
  els.host
    .querySelectorAll('input, button[data-action="save"], button[data-action="delete"], button[data-action="add"]')
    .forEach((node) => {
      if (node.dataset.action === 'refresh') return;
      node.disabled = !enabled;
    });
}

function showNotice(text) {
  if (!els.notice) return;
  els.notice.textContent = text;
  els.notice.hidden = false;
}

function hideNotice() {
  if (!els.notice) return;
  els.notice.hidden = true;
  els.notice.textContent = '';
}

function toast(message, isError = false) {
  state.ctx?.notifications?.show?.({
    type: isError ? 'error' : 'success',
    title: state.t('title'),
    message: String(message ?? ''),
  });
}

function formatUpdated(value) {
  try {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return '';
    const locale = state.ctx?.locale === 'en' ? 'en-US' : 'de-DE';
    const formatted = date.toLocaleDateString(locale, { year: 'numeric', month: 'short', day: 'numeric' });
    return state.t('updated').replace('{date}', formatted);
  } catch (_error) {
    return '';
  }
}

function esc(value) {
  return String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function cssEscape(value) {
  if (window.CSS && typeof window.CSS.escape === 'function') return window.CSS.escape(value);
  return String(value).replace(/["\\]/g, '\\$&');
}

// Pure logic surfaced for unit tests (no DOM needed); see credentials.test.mjs.
export const __credentialsTestHooks = { KEY_RE, rowHtml, buildCommandDoc };
