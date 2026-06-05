import { loadModuleMessages } from '../../shared/i18n.js';

const BUILD = '20260604-iot-w16';

// Read-only phase: subscribe to and read these projected collections only.
// business_commands is read for the diagnostics line; it is never written.
const IOT_COLLECTIONS = Object.freeze([
  'business_commands',
  'iot_realms',
  'iot_asset_types',
  'iot_assets',
  'iot_attributes',
  'iot_datapoints',
  'iot_alarms',
  'iot_agents',
  'iot_agent_status',
  'iot_rulesets',
]);

const CHART_WINDOWS = Object.freeze(['1h', '24h', '7d']);
const CHART_VIEWPORT = Object.freeze({ width: 320, height: 120, padX: 6, padY: 10 });

const labels = {
  de: {
    title: 'IoT',
    kicker: 'IoT',
    refresh: 'Aktualisieren',
    search: 'Suchen',
    searchAssets: 'Assets suchen...',
    scope: 'Realm-Bereich',
    allRealms: 'Alle Realms',
    realms: 'Realms',
    assetTree: 'Asset-Baum',
    noAssets: 'Keine Assets in diesem Realm',
    expand: 'Aufklappen',
    collapse: 'Zuklappen',
    workbench: 'Attribut-Arbeitsbereich',
    selectAsset: 'Wählen Sie ein Asset, um dessen Attribute anzuzeigen',
    asset: 'Asset',
    assetType: 'Asset-Typ',
    realm: 'Realm',
    attributes: 'Attribute',
    noAttributes: 'Keine Attribute für dieses Asset',
    attribute: 'Attribut',
    valueType: 'Typ',
    value: 'Wert',
    lastUpdate: 'Letzte Aktualisierung',
    on: 'Ein',
    off: 'Aus',
    history: 'Verlauf',
    chart: 'Diagramm',
    noHistory: 'Keine Historie für dieses Zeitfenster projiziert',
    windowHour: '1 Std.',
    windowDay: '24 Std.',
    windowWeek: '7 Tage',
    truncated: 'Gekürzt',
    points: 'Punkte',
    map: 'Karte',
    noLocation: 'Keine Standortdaten',
    context: 'Kontext',
    alarms: 'Alarme',
    noAlarms: 'Keine offenen Alarme',
    severity: 'Schweregrad',
    status: 'Status',
    assignee: 'Zugewiesen an',
    created: 'Erstellt',
    sevLow: 'Niedrig',
    sevMedium: 'Mittel',
    sevHigh: 'Hoch',
    rulesets: 'Regelsätze',
    noRulesets: 'Keine Regelsätze',
    enabled: 'Aktiviert',
    disabled: 'Deaktiviert',
    lastFired: 'Zuletzt ausgelöst',
    never: 'Nie',
    agents: 'Agenten',
    noAgents: 'Keine Agenten',
    agent: 'Agent',
    kind: 'Art',
    connected: 'Verbunden',
    unconfigured: 'Nicht konfiguriert',
    error: 'Fehler',
    lastEvent: 'Letztes Ereignis',
    commands: 'Befehle',
    pending: 'Ausstehend',
    completed: 'Abgeschlossen',
    failed: 'Fehlgeschlagen',
    syncNotReady: 'Abgleich nicht bereit',
    syncNotReadyHint: 'Die IoT-Engine oder der Peer ist offline. Es werden keine Daten angezeigt, bis die Projektion fortgesetzt wird.',
    loading: 'Wird geladen...',
    readOnlyNotice: 'Live-RxDB-Ansicht. Steuerung erfordert Berechtigungen und läuft über den Befehlskanal.',
    permissionDenied: 'Keine Berechtigung für IoT-Änderungen.',
    permissionNotice: 'Sie haben keine Schreibberechtigung. Steuerungen sind deaktiviert.',
    commandUnavailable: 'Befehlskanal nicht verfügbar.',
    commandPending: 'Befehl wird gesendet...',
    commandCompleted: 'Befehl abgeschlossen.',
    commandFailed: 'Befehl fehlgeschlagen: {0}',
    commandTimeout: 'Zeitüberschreitung beim Warten auf das Ergebnis.',
    saveRuleset: 'Regelsatz speichern',
    newRuleset: 'Neuer Regelsatz',
    editRuleset: 'Regelsatz bearbeiten',
    enableRuleset: 'Aktivieren',
    disableRuleset: 'Deaktivieren',
    ackAlarm: 'Bestätigen',
    resolveAlarm: 'Beheben',
    closeAlarm: 'Schließen',
    assignAlarm: 'Zuweisen',
    configureAgent: 'Konfigurieren',
    writeAttribute: 'Schreiben',
    editAsset: 'Asset bearbeiten',
    newChildAsset: 'Untergeordnetes Asset',
    deleteAsset: 'Asset löschen',
    confirmDeleteAsset: 'Dieses Asset wirklich löschen?',
    loadHistory: 'Verlauf laden',
    rulesetData: 'Regeldaten (JSON)',
    agentData: 'Agentdaten (JSON)',
    cancel: 'Abbrechen',
    save: 'Speichern',
    name: 'Name',
    parentAsset: 'Übergeordnetes Asset',
    errNameRequired: 'Name ist erforderlich.',
    errRealmRequired: 'Realm ist erforderlich.',
    errDataJson: 'Daten müssen gültiges JSON-Objekt sein.',
    errValueNaN: 'Wert muss eine Zahl sein.',
  },
  en: {
    title: 'IoT',
    kicker: 'IoT',
    refresh: 'Refresh',
    search: 'Search',
    searchAssets: 'Search assets...',
    scope: 'Realm scope',
    allRealms: 'All realms',
    realms: 'Realms',
    assetTree: 'Asset tree',
    noAssets: 'No assets in this realm',
    expand: 'Expand',
    collapse: 'Collapse',
    workbench: 'Attribute workbench',
    selectAsset: 'Select an asset to inspect its attributes',
    asset: 'Asset',
    assetType: 'Asset type',
    realm: 'Realm',
    attributes: 'Attributes',
    noAttributes: 'No attributes for this asset',
    attribute: 'Attribute',
    valueType: 'Type',
    value: 'Value',
    lastUpdate: 'Last update',
    on: 'On',
    off: 'Off',
    history: 'History',
    chart: 'Chart',
    noHistory: 'No history projected for this window',
    windowHour: '1h',
    windowDay: '24h',
    windowWeek: '7d',
    truncated: 'Truncated',
    points: 'Points',
    map: 'Map',
    noLocation: 'No location data',
    context: 'Context',
    alarms: 'Alarms',
    noAlarms: 'No open alarms',
    severity: 'Severity',
    status: 'Status',
    assignee: 'Assignee',
    created: 'Created',
    sevLow: 'Low',
    sevMedium: 'Medium',
    sevHigh: 'High',
    rulesets: 'Rulesets',
    noRulesets: 'No rulesets',
    enabled: 'Enabled',
    disabled: 'Disabled',
    lastFired: 'Last fired',
    never: 'Never',
    agents: 'Agents',
    noAgents: 'No agents',
    agent: 'Agent',
    kind: 'Kind',
    connected: 'Connected',
    unconfigured: 'Not configured',
    error: 'Error',
    lastEvent: 'Last event',
    commands: 'Commands',
    pending: 'Pending',
    completed: 'Completed',
    failed: 'Failed',
    syncNotReady: 'Sync not ready',
    syncNotReadyHint: 'The IoT engine or peer is offline. No data is shown until projection resumes.',
    loading: 'Loading...',
    readOnlyNotice: 'Live RxDB view. Control actions require permission and run through the command bus.',
    permissionDenied: 'No permission for IoT changes.',
    permissionNotice: 'You do not have write permission. Controls are disabled.',
    commandUnavailable: 'Command channel unavailable.',
    commandPending: 'Sending command...',
    commandCompleted: 'Command completed.',
    commandFailed: 'Command failed: {0}',
    commandTimeout: 'Timed out waiting for the result.',
    saveRuleset: 'Save ruleset',
    newRuleset: 'New ruleset',
    editRuleset: 'Edit ruleset',
    enableRuleset: 'Enable',
    disableRuleset: 'Disable',
    ackAlarm: 'Acknowledge',
    resolveAlarm: 'Resolve',
    closeAlarm: 'Close',
    assignAlarm: 'Assign',
    configureAgent: 'Configure',
    writeAttribute: 'Write',
    editAsset: 'Edit asset',
    newChildAsset: 'New child asset',
    deleteAsset: 'Delete asset',
    confirmDeleteAsset: 'Delete this asset?',
    loadHistory: 'Load history',
    rulesetData: 'Rule data (JSON)',
    agentData: 'Agent data (JSON)',
    cancel: 'Cancel',
    save: 'Save',
    name: 'Name',
    parentAsset: 'Parent asset',
    errNameRequired: 'Name is required.',
    errRealmRequired: 'Realm is required.',
    errDataJson: 'Data must be a valid JSON object.',
    errValueNaN: 'Value must be a number.',
  },
};

const state = {
  ctx: null,
  t: translateFallback('de'),
  lang: 'de',
  selectedRealm: '',
  selectedAssetId: '',
  selectedAttributeName: '',
  search: '',
  chartWindow: '24h',
  expandedAssets: null,
  collections: emptyCollections(),
  diagnostics: { loading: false, error: '', lastLoadedAt: 0 },
  cleanup: [],
  renderTimer: 0,
  commandState: '',
  commandTone: '',
  pendingCommands: new Map(),
  formMode: '',
  formRecordId: '',
  formError: '',
  formDraft: null,
};

export async function mount(ctx) {
  resetState(ctx);
  await ensureStyles();
  const messages = await loadModuleMessages(import.meta.url, ctx.locale || 'de', labels);
  state.t = (key, fallback, ...args) => {
    let value = messages[key] ?? fallback ?? key;
    args.forEach((arg, index) => {
      value = String(value).replace(`{${index}}`, arg);
    });
    return value;
  };
  state.lang = ctx.locale === 'en' ? 'en' : 'de';
  ctx.host.innerHTML = await loadModuleMarkup();
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();

  const root = ctx.host.querySelector('[data-iot-root]');
  wireUi(root);
  state.cleanup.push(setupResizers(root));
  await refreshData({ renderLoading: true });
  state.cleanup.push(wireRealtime());
  render();

  return () => {
    for (const cleanup of state.cleanup.splice(0)) {
      try { cleanup?.(); } catch {}
    }
    if (state.renderTimer) window.clearTimeout(state.renderTimer);
    state.renderTimer = 0;
  };
}

function resetState(ctx) {
  state.ctx = ctx;
  state.t = translateFallback(ctx?.locale === 'en' ? 'en' : 'de');
  state.lang = ctx?.locale === 'en' ? 'en' : 'de';
  state.selectedRealm = '';
  state.selectedAssetId = '';
  state.selectedAttributeName = '';
  state.search = '';
  state.chartWindow = '24h';
  state.expandedAssets = new Set();
  state.collections = emptyCollections();
  state.diagnostics = { loading: false, error: '', lastLoadedAt: 0 };
  state.cleanup = [];
  state.renderTimer = 0;
  state.commandState = '';
  state.commandTone = '';
  state.pendingCommands = new Map();
  state.formMode = '';
  state.formRecordId = '';
  state.formError = '';
  state.formDraft = null;
}

function translateFallback(lang) {
  const dictionary = labels[lang] || labels.de;
  return (key, fallback, ...args) => {
    let value = dictionary[key] ?? fallback ?? key;
    args.forEach((arg, index) => {
      value = String(value).replace(`{${index}}`, arg);
    });
    return value;
  };
}

async function ensureStyles() {
  if (document.querySelector('link[data-module-styles="iot"]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL(`./index.css?v=${BUILD}`, import.meta.url).href;
  link.dataset.moduleStyles = 'iot';
  document.head.append(link);
}

async function loadModuleMarkup() {
  const html = await fetch(new URL(`./index.html?v=${BUILD}`, import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function setupResizers() {
  // Column resizing is owned by the shell-global resizer (setupModuleResizers in
  // app.js), wired declaratively from the `.ctox-column-resizer[data-resizer-var]`
  // handles inside `[data-resize-frame]`. The module ships no resizer; this keeps
  // a no-op teardown ref so the cleanup contract stays uniform.
  return () => {};
}

function wireUi(root) {
  if (!root) return;
  root.addEventListener('click', onRootClick);
  root.addEventListener('input', onRootInput);
  root.addEventListener('keydown', onRootKeydown);
}

function onRootClick(event) {
  const actionEl = event.target.closest('[data-iot-action]');
  if (!actionEl) return;
  const action = actionEl.dataset.iotAction;
  handleAction(action, actionEl);
}

function onRootInput(event) {
  const formField = event.target.closest('[data-iot-form-field]');
  if (formField) {
    onFormFieldInput(formField);
    return;
  }
  const input = event.target.closest('[data-iot-search]');
  if (!input) return;
  state.search = String(input.value || '');
  scheduleRender();
}

// Form fields update the live draft without a full re-render (which would blow
// away input focus); only the form-open/close transitions re-render.
function onFormFieldInput(field) {
  if (!state.formDraft) return;
  const key = field.dataset.iotFormField;
  if (!key) return;
  if (field.type === 'checkbox') state.formDraft[key] = field.checked;
  else if (key === 'data') state.formDraft.dataText = String(field.value || '');
  else state.formDraft[key] = String(field.value || '');
}

function onRootKeydown(event) {
  const row = event.target.closest('[data-iot-action]');
  if (!row) return;
  if (!isActivationKey(event.key) || isInteractiveTarget(event.target)) return;
  event.preventDefault();
  handleAction(row.dataset.iotAction, row);
}

function isInteractiveTarget(target) {
  const tag = String(target?.tagName || '').toLowerCase();
  return tag === 'input' || tag === 'textarea' || tag === 'select';
}

// Read-only: handlers only mutate UI selection state, never dispatch commands.
function handleAction(action, el) {
  switch (action) {
    case 'refresh':
      refreshData({ renderLoading: true }).catch(reportRefreshError);
      return;
    case 'select-realm':
      state.selectedRealm = el.dataset.iotRealmId || '';
      state.selectedAssetId = '';
      state.selectedAttributeName = '';
      syncSelection();
      render();
      return;
    case 'toggle-asset': {
      const id = el.dataset.iotAssetId || '';
      if (state.expandedAssets.has(id)) state.expandedAssets.delete(id);
      else state.expandedAssets.add(id);
      render();
      return;
    }
    case 'select-asset':
      state.selectedAssetId = el.dataset.iotAssetId || '';
      state.selectedAttributeName = '';
      syncSelection();
      render();
      return;
    case 'select-attribute':
      state.selectedAttributeName = el.dataset.iotAttributeName || '';
      render();
      return;
    case 'select-chart-window':
      state.chartWindow = el.dataset.iotWindow || '24h';
      render();
      return;
    // ---- Phase 6: write actions (all dispatch ctox.iot.* via commandBus) ----
    case 'attr-write': {
      const assetId = el.dataset.iotAssetId || state.selectedAssetId;
      const name = el.dataset.iotAttrName || '';
      const valueType = el.dataset.iotValueType || 'Text';
      const row = el.closest('[data-iot-attr-write-row]') || el.parentElement;
      const coerced = readAttributeInput(row, valueType);
      if (coerced.error) {
        setCommandState(coerced.error, 'failed');
        return;
      }
      dispatchIotCommand(buildAttributeWriteCommand(assetId, name, coerced.value)).catch(reportRefreshError);
      return;
    }
    case 'asset-edit':
      openAssetForm(el.dataset.iotAssetId || state.selectedAssetId, false);
      return;
    case 'asset-new-child':
      openAssetForm(el.dataset.iotAssetId || state.selectedAssetId, true);
      return;
    case 'asset-delete': {
      const assetId = el.dataset.iotAssetId || state.selectedAssetId;
      if (!canModifyModule()) {
        setCommandState(state.t('permissionDenied', labels.de.permissionDenied), 'failed');
        return;
      }
      const confirmFn = state.ctx?.confirm || (typeof window !== 'undefined' ? window.confirm : null);
      if (typeof confirmFn === 'function' && !confirmFn(state.t('confirmDeleteAsset', labels.de.confirmDeleteAsset))) return;
      dispatchIotCommand(buildAssetDeleteCommand(assetId)).catch(reportRefreshError);
      return;
    }
    case 'alarm-ack':
      dispatchIotCommand(buildAlarmUpdateCommand(el.dataset.iotAlarmId || '', 'ack')).catch(reportRefreshError);
      return;
    case 'alarm-resolve':
      dispatchIotCommand(buildAlarmUpdateCommand(el.dataset.iotAlarmId || '', 'resolve')).catch(reportRefreshError);
      return;
    case 'alarm-close':
      dispatchIotCommand(buildAlarmUpdateCommand(el.dataset.iotAlarmId || '', 'close')).catch(reportRefreshError);
      return;
    case 'alarm-assign': {
      const alarmId = el.dataset.iotAlarmId || '';
      const row = el.closest('[data-iot-alarm-row]') || el.parentElement;
      const assignee = String(row?.querySelector('[data-iot-assignee]')?.value || '').trim();
      dispatchIotCommand(buildAlarmUpdateCommand(alarmId, 'assign', { assignee })).catch(reportRefreshError);
      return;
    }
    case 'ruleset-new':
      openRulesetEditor('');
      return;
    case 'ruleset-edit':
      openRulesetEditor(el.dataset.iotRulesetId || '');
      return;
    case 'ruleset-toggle': {
      const rulesetId = el.dataset.iotRulesetId || '';
      const nextEnabled = el.dataset.iotEnabled !== 'true';
      dispatchIotCommand(buildRulesetToggleCommand(rulesetId, nextEnabled)).catch(reportRefreshError);
      return;
    }
    case 'agent-configure':
      openAgentForm(el.dataset.iotAgentId || '');
      return;
    case 'agent-toggle': {
      const agentId = el.dataset.iotAgentId || '';
      const agent = (state.collections.iot_agents || []).find((row) => row.id === agentId);
      if (!agent) return;
      const nextEnabled = el.dataset.iotEnabled !== 'true';
      dispatchIotCommand(buildAgentConfigureCommand({
        id: agent.id, realm: realmKey(agent), name: agent.name || agent.id, kind: agent.kind || 'mqtt', enabled: nextEnabled,
      })).catch(reportRefreshError);
      return;
    }
    case 'load-history': {
      const assetId = el.dataset.iotAssetId || state.selectedAssetId;
      const attrName = el.dataset.iotAttrName || state.selectedAttributeName;
      if (!assetId || !attrName) return;
      dispatchIotCommand(buildDatapointsQueryCommand(assetId, attrName, state.chartWindow)).catch(reportRefreshError);
      return;
    }
    case 'form-submit':
      submitForm().catch(reportRefreshError);
      return;
    case 'form-cancel':
      closeForm();
      return;
    default:
  }
}

function wireRealtime() {
  const subscriptions = IOT_COLLECTIONS
    .map((name) => resolveCollection(name)?.$?.subscribe?.(() => scheduleRefresh()))
    .filter(Boolean);
  return () => subscriptions.forEach((sub) => {
    try { sub.unsubscribe?.(); } catch {}
  });
}

function scheduleRefresh() {
  if (state.renderTimer) return;
  state.renderTimer = window.setTimeout(() => {
    state.renderTimer = 0;
    refreshData().catch(reportRefreshError);
  }, 80);
}

function scheduleRender() {
  if (state.renderTimer) return;
  state.renderTimer = window.setTimeout(() => {
    state.renderTimer = 0;
    render();
  }, 80);
}

async function refreshData(options = {}) {
  if (options.renderLoading) {
    state.diagnostics.loading = true;
    render();
  }
  try {
    const entries = [];
    for (const name of IOT_COLLECTIONS) {
      entries.push([name, await readCollection(name)]);
    }
    state.collections = { ...emptyCollections(), ...Object.fromEntries(entries) };
    state.diagnostics.error = '';
    state.diagnostics.lastLoadedAt = Date.now();
    syncSelection();
  } catch (error) {
    state.diagnostics.error = error?.message || String(error);
  } finally {
    state.diagnostics.loading = false;
    render();
  }
}

function reportRefreshError(error) {
  state.diagnostics.error = error?.message || String(error);
  state.diagnostics.loading = false;
  render();
}

async function readCollection(name) {
  const collection = resolveCollection(name);
  const docs = collection?.find ? await collection.find().exec() : [];
  return docs
    .map((doc) => doc?.toJSON?.() || doc)
    .filter((doc) => doc && doc._deleted !== true)
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
}

function resolveCollection(name) {
  return state.ctx?.db?.raw?.[name]
    || state.ctx?.db?.collections?.[name]
    || state.ctx?.db?.collection?.(name);
}

function syncSelection() {
  const realms = state.collections.iot_realms || [];
  if (state.selectedRealm && state.selectedRealm !== '*'
    && !realms.some((realm) => realmKey(realm) === state.selectedRealm)) {
    state.selectedRealm = '';
  }
  const visible = filterAssets(
    filterAssetsByRealm(state.collections.iot_assets || [], state.selectedRealm),
    { search: state.search },
  );
  if (state.selectedAssetId && !visible.some((asset) => asset.id === state.selectedAssetId)) {
    state.selectedAssetId = '';
  }
  if (!state.selectedAssetId) {
    state.selectedAssetId = visible[0]?.id || '';
  }
  const attributes = relatedAttributes(state.selectedAssetId, state.collections);
  if (state.selectedAttributeName
    && !attributes.some((attr) => attributeName(attr) === state.selectedAttributeName)) {
    state.selectedAttributeName = '';
  }
  if (!state.selectedAttributeName) {
    state.selectedAttributeName = attributeName(attributes[0]) || '';
  }
}

function render() {
  const root = state.ctx?.host?.querySelector('[data-iot-root]');
  if (!root) return;
  renderLeft();
  renderCenter();
  renderRight();
}

// ----------------------------------------------------------------------------
// LEFT pane
// ----------------------------------------------------------------------------
function renderLeft() {
  const target = state.ctx.host.querySelector('[data-iot-left]');
  if (!target) return;
  if (!syncReady(state)) {
    target.innerHTML = renderPaneStatus();
    return;
  }
  const scrollTop = target.querySelector('.iot-scroll')?.scrollTop || 0;
  const realms = state.collections.iot_realms || [];
  const assets = state.collections.iot_assets || [];
  const filtered = filterAssets(filterAssetsByRealm(assets, state.selectedRealm), { search: state.search });
  const tree = buildAssetTree(filtered);
  target.innerHTML = `
    <header class="iot-pane-header">
      <div class="iot-title-group">
        <span class="iot-kicker">${escapeHtml(state.t('kicker', labels.de.kicker))}</span>
        <h2 class="iot-title">${escapeHtml(state.t('title', labels.de.title))}</h2>
      </div>
      <button class="iot-icon-button" type="button" data-iot-action="refresh" title="${escapeAttribute(state.t('refresh', labels.de.refresh))}" aria-label="${escapeAttribute(state.t('refresh', labels.de.refresh))}">${refreshIcon()}</button>
    </header>
    <div class="iot-scroll">
      <section class="iot-section">
        <h3 class="iot-section-label">${escapeHtml(state.t('scope', labels.de.scope))}</h3>
        ${realmButton('', state.t('allRealms', labels.de.allRealms), assets.length)}
        ${realms.map((realm) => realmButton(
          realmKey(realm),
          realm.name || realmKey(realm),
          filterAssetsByRealm(assets, realmKey(realm)).length,
        )).join('')}
      </section>
      <section class="iot-section">
        <h3 class="iot-section-label">${escapeHtml(state.t('search', labels.de.search))}</h3>
        <input class="iot-input" type="search" data-iot-search placeholder="${escapeAttribute(state.t('searchAssets', labels.de.searchAssets))}" value="${escapeAttribute(state.search)}" />
      </section>
      <section class="iot-section">
        <h3 class="iot-section-label">${escapeHtml(state.t('assetTree', labels.de.assetTree))}</h3>
        ${tree.length ? tree.map((node) => renderAssetNode(node, 0)).join('') : `<div class="iot-muted-row">${escapeHtml(state.t('noAssets', labels.de.noAssets))}</div>`}
      </section>
    </div>
  `;
  const nextScroll = target.querySelector('.iot-scroll');
  if (nextScroll) nextScroll.scrollTop = scrollTop;
}

function realmButton(key, label, count) {
  const active = (state.selectedRealm || '') === (key || '');
  return `
    <button class="iot-realm-row" type="button" data-iot-action="select-realm" data-iot-realm-id="${escapeAttribute(key)}" aria-pressed="${active ? 'true' : 'false'}">
      <span class="iot-tree-label">${escapeHtml(label)}</span>
      <span class="iot-count">${Number(count || 0)}</span>
    </button>
  `;
}

function renderAssetNode(node, depth) {
  const hasChildren = node.children && node.children.length > 0;
  const expanded = state.expandedAssets.has(node.asset.id);
  const selected = state.selectedAssetId === node.asset.id;
  const toggle = hasChildren
    ? `<button class="iot-tree-toggle" type="button" data-iot-action="toggle-asset" data-iot-asset-id="${escapeAttribute(node.asset.id)}" aria-label="${escapeAttribute(expanded ? state.t('collapse', labels.de.collapse) : state.t('expand', labels.de.expand))}">${expanded ? '▾' : '▸'}</button>`
    : '<span class="iot-tree-toggle" aria-hidden="true"></span>';
  const childCount = hasChildren ? `<span class="iot-count">${node.children.length}</span>` : '';
  const typeChip = node.asset.asset_type
    ? `<span class="iot-chip-accent">${escapeHtml(node.asset.asset_type)}</span>`
    : '';
  const row = `
    <div class="iot-tree-row" role="button" tabindex="0" data-iot-action="select-asset" data-iot-asset-id="${escapeAttribute(node.asset.id)}" aria-selected="${selected ? 'true' : 'false'}" style="padding-inline-start:${8 + depth * 16}px">
      ${toggle}
      <span class="iot-tree-label">${escapeHtml(node.asset.name || node.asset.id)}</span>
      ${typeChip}
      ${childCount}
    </div>
  `;
  const childrenHtml = hasChildren && expanded
    ? node.children.map((child) => renderAssetNode(child, depth + 1)).join('')
    : '';
  return row + childrenHtml;
}

// ----------------------------------------------------------------------------
// CENTER pane
// ----------------------------------------------------------------------------
function renderCenter() {
  const target = state.ctx.host.querySelector('[data-iot-center]');
  if (!target) return;
  if (!syncReady(state)) {
    target.innerHTML = renderPaneStatus();
    return;
  }
  const scrollTop = target.querySelector('.iot-scroll')?.scrollTop || 0;
  const context = selectedAssetContext(state.selectedAssetId, state.collections);
  if (!context.asset) {
    target.innerHTML = `
      <header class="iot-pane-header">
        <div class="iot-title-group">
          <span class="iot-kicker">${escapeHtml(state.t('workbench', labels.de.workbench))}</span>
          <h2 class="iot-title">${escapeHtml(state.t('asset', labels.de.asset))}</h2>
        </div>
      </header>
      <div class="iot-not-ready"><div class="iot-not-ready-title">${escapeHtml(state.t('selectAsset', labels.de.selectAsset))}</div></div>
    `;
    return;
  }
  const asset = context.asset;
  const attributes = context.attributes;
  target.innerHTML = `
    <header class="iot-pane-header">
      <div class="iot-title-group">
        <span class="iot-kicker">${escapeHtml(state.t('workbench', labels.de.workbench))}</span>
        <h2 class="iot-title">${escapeHtml(asset.name || asset.id)}</h2>
      </div>
      <div class="iot-header-actions">
        <span class="iot-badge iot-badge-accent">${escapeHtml(asset.asset_type || '')}</span>
        <button class="iot-button" type="button" data-iot-action="asset-edit" data-iot-asset-id="${escapeAttribute(asset.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('editAsset', labels.de.editAsset))}</button>
        <button class="iot-button" type="button" data-iot-action="asset-new-child" data-iot-asset-id="${escapeAttribute(asset.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('newChildAsset', labels.de.newChildAsset))}</button>
        <button class="iot-button" type="button" data-iot-action="asset-delete" data-iot-asset-id="${escapeAttribute(asset.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('deleteAsset', labels.de.deleteAsset))}</button>
      </div>
    </header>
    ${renderPermissionNotice()}
    ${state.formMode ? renderFormOverlay() : ''}
    <div class="iot-scroll">
      <div class="iot-card">
        <div class="iot-counts">
          <span><strong>${escapeHtml(asset.asset_type || '—')}</strong> · ${escapeHtml(state.t('assetType', labels.de.assetType))}</span>
          <span><strong>${escapeHtml(asset.realm || '—')}</strong> · ${escapeHtml(state.t('realm', labels.de.realm))}</span>
          <span><strong>${attributes.length}</strong> · ${escapeHtml(state.t('attributes', labels.de.attributes))}</span>
        </div>
      </div>
      <div class="iot-card">
        <div class="iot-section-head"><h3 class="iot-section-label">${escapeHtml(state.t('attributes', labels.de.attributes))}</h3></div>
        ${renderAttributeTable(attributes)}
      </div>
      <div class="iot-card">
        ${renderChartCard(asset)}
      </div>
      ${renderMapCard(asset)}
    </div>
  `;
  const nextScroll = target.querySelector('.iot-scroll');
  if (nextScroll) nextScroll.scrollTop = scrollTop;
  drawChartCanvas();
}

function renderAttributeTable(attributes) {
  if (!attributes.length) {
    return `<div class="iot-muted-row">${escapeHtml(state.t('noAttributes', labels.de.noAttributes))}</div>`;
  }
  const rows = attributes.map((attr) => {
    const name = attributeName(attr);
    const selected = state.selectedAttributeName === name;
    const valueType = attr.value_type || '';
    const valueClass = valueType === 'Number' ? ' iot-value-num' : '';
    return `
      <tr class="iot-attr-row" role="button" tabindex="0" data-iot-action="select-attribute" data-iot-attribute-name="${escapeAttribute(name)}" aria-selected="${selected ? 'true' : 'false'}">
        <td>${escapeHtml(name)}</td>
        <td><span class="iot-chip">${escapeHtml(valueType || '—')}</span></td>
        <td class="${valueClass.trim()}">${escapeHtml(formatAttributeValue(attr, state.t))}</td>
        <td class="iot-list-meta">${escapeHtml(formatRelative(attr.timestamp_ms, state.t))}</td>
        <td>${renderAttributeWriteControl(attr)}</td>
      </tr>
    `;
  }).join('');
  return `
    <table class="iot-table">
      <thead>
        <tr>
          <th>${escapeHtml(state.t('attribute', labels.de.attribute))}</th>
          <th>${escapeHtml(state.t('valueType', labels.de.valueType))}</th>
          <th>${escapeHtml(state.t('value', labels.de.value))}</th>
          <th>${escapeHtml(state.t('lastUpdate', labels.de.lastUpdate))}</th>
          <th>${escapeHtml(state.t('writeAttribute', labels.de.writeAttribute))}</th>
        </tr>
      </thead>
      <tbody>${rows}</tbody>
    </table>
  `;
}

function renderAttributeWriteControl(attr) {
  const assetId = attr.asset_id || state.selectedAssetId;
  const name = attributeName(attr);
  const valueType = attr.value_type || 'Text';
  const current = attributeValue(attr);
  let inputHtml;
  if (valueType === 'Boolean') {
    inputHtml = `<input type="checkbox" data-iot-attr-input${current === true ? ' checked' : ''}${mutableDisabledAttr()} />`;
  } else if (valueType === 'GeoPoint') {
    const lat = current?.lat ?? current?.latitude ?? '';
    const lng = current?.lng ?? current?.lon ?? current?.longitude ?? '';
    inputHtml = `
      <input class="iot-attr-input" type="number" step="any" data-iot-attr-lat placeholder="lat" value="${escapeAttribute(lat)}"${mutableDisabledAttr()} />
      <input class="iot-attr-input" type="number" step="any" data-iot-attr-lng placeholder="lng" value="${escapeAttribute(lng)}"${mutableDisabledAttr()} />`;
  } else if (valueType === 'Number') {
    inputHtml = `<input class="iot-attr-input" type="number" step="any" data-iot-attr-input value="${escapeAttribute(current ?? '')}"${mutableDisabledAttr()} />`;
  } else {
    inputHtml = `<input class="iot-attr-input" type="text" data-iot-attr-input value="${escapeAttribute(current ?? '')}"${mutableDisabledAttr()} />`;
  }
  return `
    <span class="iot-attr-write" data-iot-attr-write-row>
      ${inputHtml}
      <button class="iot-button" type="button" data-iot-action="attr-write" data-iot-asset-id="${escapeAttribute(assetId)}" data-iot-attr-name="${escapeAttribute(name)}" data-iot-value-type="${escapeAttribute(valueType)}"${mutableDisabledAttr()}>${escapeHtml(state.t('writeAttribute', labels.de.writeAttribute))}</button>
    </span>`;
}

function renderChartCard(asset) {
  const window = pickDatapointWindow(asset.id, state.selectedAttributeName, state.chartWindow, state.collections);
  const buttons = CHART_WINDOWS.map((win) => {
    const labelKey = win === '1h' ? 'windowHour' : win === '24h' ? 'windowDay' : 'windowWeek';
    return `<button class="iot-button" type="button" data-iot-action="select-chart-window" data-iot-window="${escapeAttribute(win)}" aria-pressed="${state.chartWindow === win ? 'true' : 'false'}">${escapeHtml(state.t(labelKey, labels.de[labelKey]))}</button>`;
  }).join('');
  const loadBtn = `<button class="iot-button" type="button" data-iot-action="load-history" data-iot-asset-id="${escapeAttribute(asset.id)}" data-iot-attr-name="${escapeAttribute(state.selectedAttributeName)}"${mutableDisabledAttr()}>${escapeHtml(state.t('loadHistory', labels.de.loadHistory))}</button>`;
  const head = `
    <div class="iot-chart-head">
      <h3 class="iot-section-label">${escapeHtml(state.t('history', labels.de.history))}</h3>
      <div class="iot-window-group">${buttons}${loadBtn}</div>
    </div>
  `;
  if (!window) {
    return `${head}<div class="iot-muted-row">${escapeHtml(state.t('noHistory', labels.de.noHistory))}</div>`;
  }
  const points = chartPointsFromDatapoint(window);
  const truncatedBadge = window.truncated
    ? `<span class="iot-badge iot-badge-danger">${escapeHtml(state.t('truncated', labels.de.truncated))}</span>`
    : '';
  const meta = `<div class="iot-list-meta">${Number(window.point_count || points.length)} ${escapeHtml(state.t('points', labels.de.points))} ${truncatedBadge}</div>`;
  if (!points.length) {
    return `${head}<div class="iot-muted-row">${escapeHtml(state.t('noHistory', labels.de.noHistory))}</div>${meta}`;
  }
  // Canvas drawn post-insert in drawChartCanvas(); inline SVG fallback rendered
  // so the chart is meaningful even before the canvas pass and in headless DOM.
  const geometry = buildChartGeometry(points);
  return `${head}
    <svg class="iot-chart" viewBox="0 0 ${CHART_VIEWPORT.width} ${CHART_VIEWPORT.height}" preserveAspectRatio="none" role="img" aria-label="${escapeAttribute(state.t('chart', labels.de.chart))}">
      <polyline points="${escapeAttribute(geometry.polyline)}" fill="none" stroke="currentColor" stroke-width="1.5" />
    </svg>
    <canvas class="iot-chart" data-iot-chart width="${CHART_VIEWPORT.width}" height="${CHART_VIEWPORT.height}" hidden></canvas>
    ${meta}`;
}

// Canvas 2D does not resolve the CSS keyword `currentColor`; an unparseable
// strokeStyle assignment is silently ignored and the stroke falls back to
// black, which is invisible on dark themes. Resolve a real color string from
// the element's computed `color` (which the SVG fallback uses via
// `stroke="currentColor"`), falling back to the shell `--text` token.
function resolveChartStroke(canvas) {
  try {
    const view = canvas?.ownerDocument?.defaultView || (typeof window !== 'undefined' ? window : null);
    if (view && typeof view.getComputedStyle === 'function') {
      const cs = view.getComputedStyle(canvas);
      const color = cs && cs.color;
      if (color && color !== 'currentColor') return color;
      const token = cs && cs.getPropertyValue('--text');
      if (token && token.trim()) return token.trim();
    }
  } catch (_err) {
    // getComputedStyle can throw in detached/headless DOM; fall through.
  }
  return '#eef4f6';
}

function drawChartCanvas() {
  const canvas = state.ctx?.host?.querySelector('[data-iot-chart]');
  if (!canvas || typeof canvas.getContext !== 'function') return;
  const window = pickDatapointWindow(state.selectedAssetId, state.selectedAttributeName, state.chartWindow, state.collections);
  if (!window) return;
  const points = chartPointsFromDatapoint(window);
  if (!points.length) return;
  const geometry = buildChartGeometry(points);
  const ctx2d = canvas.getContext('2d');
  if (!ctx2d) return;
  canvas.hidden = false;
  const svg = canvas.previousElementSibling;
  if (svg && svg.tagName && svg.tagName.toLowerCase() === 'svg') svg.setAttribute('hidden', 'hidden');
  ctx2d.clearRect(0, 0, CHART_VIEWPORT.width, CHART_VIEWPORT.height);
  ctx2d.beginPath();
  geometry.coords.forEach((pt, index) => {
    if (index === 0) ctx2d.moveTo(pt.x, pt.y);
    else ctx2d.lineTo(pt.x, pt.y);
  });
  ctx2d.lineWidth = 1.5;
  ctx2d.strokeStyle = resolveChartStroke(canvas);
  ctx2d.stroke();
}

function renderMapCard(asset) {
  const assets = filterAssetsByRealm(state.collections.iot_assets || [], state.selectedRealm)
    .filter((item) => geoOf(item));
  if (!geoOf(asset)) return '';
  if (!assets.length) {
    return `<div class="iot-card"><h3 class="iot-section-label">${escapeHtml(state.t('map', labels.de.map))}</h3><div class="iot-muted-row">${escapeHtml(state.t('noLocation', labels.de.noLocation))}</div></div>`;
  }
  const geometry = buildMapGeometry(assets, asset.id);
  const circles = geometry.points.map((pt) => `<circle cx="${pt.x}" cy="${pt.y}" r="${pt.selected ? 4 : 2.5}" fill="${pt.selected ? 'currentColor' : 'none'}" stroke="currentColor" stroke-width="1.2" ${pt.selected ? 'class="iot-badge-accent"' : ''} />`).join('');
  return `
    <div class="iot-card">
      <h3 class="iot-section-label">${escapeHtml(state.t('map', labels.de.map))}</h3>
      <svg class="iot-map" viewBox="0 0 ${CHART_VIEWPORT.width} ${CHART_VIEWPORT.height}" role="img" aria-label="${escapeAttribute(state.t('map', labels.de.map))}">${circles}</svg>
    </div>
  `;
}

// ----------------------------------------------------------------------------
// RIGHT pane
// ----------------------------------------------------------------------------
function renderRight() {
  const target = state.ctx.host.querySelector('[data-iot-right]');
  if (!target) return;
  if (!syncReady(state)) {
    target.innerHTML = renderPaneStatus();
    return;
  }
  const scrollTop = target.querySelector('.iot-scroll')?.scrollTop || 0;
  const realm = state.selectedRealm;
  const alarms = (state.collections.iot_alarms || [])
    .filter((alarm) => !realm || realmKey(alarm) === realm);
  const alarmSummary = summarizeAlarms(alarms);
  const rulesets = (state.collections.iot_rulesets || [])
    .filter((rs) => !realm || realmKey(rs) === realm);
  const agents = joinAgentStatus(state.collections.iot_agents || [], state.collections.iot_agent_status || [])
    .filter((row) => !realm || realmKey(row.agent) === realm);
  const commands = summarizeIotCommands((state.collections.business_commands || []).filter((cmd) => cmd.module === 'iot'));
  target.innerHTML = `
    <header class="iot-pane-header">
      <div class="iot-title-group">
        <span class="iot-kicker">${escapeHtml(state.t('context', labels.de.context))}</span>
        <h2 class="iot-title">${escapeHtml(state.t('context', labels.de.context))}</h2>
      </div>
    </header>
    <div class="iot-scroll">
      ${state.commandState ? `<div class="iot-command-state iot-command-${escapeAttribute(state.commandTone)}">${escapeHtml(state.commandState)}</div>` : ''}
      <section class="iot-section">
        <div class="iot-section-head">
          <h3 class="iot-section-label">${escapeHtml(state.t('alarms', labels.de.alarms))}</h3>
          <span class="iot-count">${alarms.length}</span>
        </div>
        ${alarms.length ? `<div class="iot-list">${alarms.map(renderAlarmRow).join('')}</div>` : `<div class="iot-muted-row">${escapeHtml(state.t('noAlarms', labels.de.noAlarms))}</div>`}
        ${alarms.length ? renderAlarmCounts(alarmSummary) : ''}
      </section>
      <section class="iot-section">
        <div class="iot-section-head">
          <h3 class="iot-section-label">${escapeHtml(state.t('rulesets', labels.de.rulesets))}</h3>
          <button class="iot-button" type="button" data-iot-action="ruleset-new"${mutableDisabledAttr()}>${escapeHtml(state.t('newRuleset', labels.de.newRuleset))}</button>
        </div>
        ${renderPermissionNotice()}
        ${rulesets.length ? `<div class="iot-list">${rulesets.map(renderRulesetRow).join('')}</div>` : `<div class="iot-muted-row">${escapeHtml(state.t('noRulesets', labels.de.noRulesets))}</div>`}
      </section>
      <section class="iot-section">
        <h3 class="iot-section-label">${escapeHtml(state.t('agents', labels.de.agents))}</h3>
        ${agents.length ? `<div class="iot-list">${agents.map(renderAgentRow).join('')}</div>` : `<div class="iot-muted-row">${escapeHtml(state.t('noAgents', labels.de.noAgents))}</div>`}
      </section>
      <section class="iot-section">
        <h3 class="iot-section-label">${escapeHtml(state.t('commands', labels.de.commands))}</h3>
        <div class="iot-counts">
          <span><strong>${commands.pending}</strong> · ${escapeHtml(state.t('pending', labels.de.pending))}</span>
          <span><strong>${commands.completed}</strong> · ${escapeHtml(state.t('completed', labels.de.completed))}</span>
          <span><strong>${commands.failed}</strong> · ${escapeHtml(state.t('failed', labels.de.failed))}</span>
        </div>
      </section>
    </div>
  `;
  const nextScroll = target.querySelector('.iot-scroll');
  if (nextScroll) nextScroll.scrollTop = scrollTop;
}

function renderAlarmRow(alarm) {
  const tone = alarmSeverityTone(alarm.severity);
  const sevKey = alarm.severity === 'HIGH' ? 'sevHigh' : alarm.severity === 'MEDIUM' ? 'sevMedium' : 'sevLow';
  const toneClass = tone === 'danger' ? ' iot-badge-danger' : tone === 'accent' ? ' iot-badge-accent' : '';
  return `
    <div class="iot-list-row" data-iot-alarm-row>
      <div class="iot-list-row-head">
        <span class="iot-list-title">${escapeHtml(alarm.title || alarm.id)}</span>
        <span class="iot-badge${toneClass}">${escapeHtml(state.t(sevKey, labels.de[sevKey]))}</span>
      </div>
      <div class="iot-list-meta">${escapeHtml(alarm.status || '')}${alarm.assignee_id ? ` · ${escapeHtml(alarm.assignee_id)}` : ''} · ${escapeHtml(formatRelative(alarm.created_ms, state.t))}</div>
      <div class="iot-row-actions">
        <button class="iot-button" type="button" data-iot-action="alarm-ack" data-iot-alarm-id="${escapeAttribute(alarm.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('ackAlarm', labels.de.ackAlarm))}</button>
        <button class="iot-button" type="button" data-iot-action="alarm-resolve" data-iot-alarm-id="${escapeAttribute(alarm.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('resolveAlarm', labels.de.resolveAlarm))}</button>
        <button class="iot-button" type="button" data-iot-action="alarm-close" data-iot-alarm-id="${escapeAttribute(alarm.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('closeAlarm', labels.de.closeAlarm))}</button>
      </div>
      <div class="iot-row-actions">
        <input class="iot-attr-input" type="text" data-iot-assignee placeholder="${escapeAttribute(state.t('assignee', labels.de.assignee))}"${mutableDisabledAttr()} />
        <button class="iot-button" type="button" data-iot-action="alarm-assign" data-iot-alarm-id="${escapeAttribute(alarm.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('assignAlarm', labels.de.assignAlarm))}</button>
      </div>
    </div>
  `;
}

function renderAlarmCounts(summary) {
  return `<div class="iot-counts" style="margin-top:8px">
    <span><strong>${summary.high}</strong> · ${escapeHtml(state.t('sevHigh', labels.de.sevHigh))}</span>
    <span><strong>${summary.medium}</strong> · ${escapeHtml(state.t('sevMedium', labels.de.sevMedium))}</span>
    <span><strong>${summary.low}</strong> · ${escapeHtml(state.t('sevLow', labels.de.sevLow))}</span>
  </div>`;
}

function renderRulesetRow(ruleset) {
  const tone = rulesetStatusTone(ruleset);
  const labelKey = tone === 'enabled' ? 'enabled' : 'disabled';
  const toneClass = tone === 'enabled' ? ' iot-badge-accent' : '';
  const enabled = tone === 'enabled';
  const toggleLabel = enabled ? state.t('disableRuleset', labels.de.disableRuleset) : state.t('enableRuleset', labels.de.enableRuleset);
  return `
    <div class="iot-list-row">
      <div class="iot-list-row-head">
        <span class="iot-list-title">${escapeHtml(ruleset.name || ruleset.id)}</span>
        <span class="iot-badge${toneClass}">${escapeHtml(state.t(labelKey, labels.de[labelKey]))}</span>
      </div>
      <div class="iot-list-meta">${escapeHtml(state.t('lastFired', labels.de.lastFired))}: ${ruleset.last_fired_ms ? escapeHtml(formatRelative(ruleset.last_fired_ms, state.t)) : escapeHtml(state.t('never', labels.de.never))}</div>
      <div class="iot-row-actions">
        <button class="iot-button" type="button" data-iot-action="ruleset-edit" data-iot-ruleset-id="${escapeAttribute(ruleset.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('editRuleset', labels.de.editRuleset))}</button>
        <button class="iot-button" type="button" data-iot-action="ruleset-toggle" data-iot-ruleset-id="${escapeAttribute(ruleset.id)}" data-iot-enabled="${enabled ? 'true' : 'false'}"${mutableDisabledAttr()}>${escapeHtml(toggleLabel)}</button>
      </div>
    </div>
  `;
}

function renderAgentRow(row) {
  const link = row.link_state || 'unconfigured';
  const tone = agentLinkTone(link);
  const labelKey = link === 'connected' ? 'connected' : link === 'error' ? 'error' : 'unconfigured';
  const toneClass = tone === 'danger' ? ' iot-badge-danger' : tone === 'accent' ? ' iot-badge-accent' : '';
  return `
    <div class="iot-list-row">
      <div class="iot-list-row-head">
        <span class="iot-list-title">${escapeHtml(row.agent.name || row.agent.id)}</span>
        <span class="iot-badge${toneClass}">${escapeHtml(state.t(labelKey, labels.de[labelKey]))}</span>
      </div>
      <div class="iot-list-meta">${escapeHtml(row.agent.kind || '')}${row.last_event_ms ? ` · ${escapeHtml(formatRelative(row.last_event_ms, state.t))}` : ''}</div>
      ${row.error ? `<div class="iot-list-meta iot-badge-danger">${escapeHtml(row.error)}</div>` : ''}
      <div class="iot-row-actions">
        <button class="iot-button" type="button" data-iot-action="agent-configure" data-iot-agent-id="${escapeAttribute(row.agent.id)}"${mutableDisabledAttr()}>${escapeHtml(state.t('configureAgent', labels.de.configureAgent))}</button>
        <button class="iot-button" type="button" data-iot-action="agent-toggle" data-iot-agent-id="${escapeAttribute(row.agent.id)}" data-iot-enabled="${row.agent.enabled !== false ? 'true' : 'false'}"${mutableDisabledAttr()}>${escapeHtml(row.agent.enabled !== false ? state.t('disableRuleset', labels.de.disableRuleset) : state.t('enableRuleset', labels.de.enableRuleset))}</button>
      </div>
    </div>
  `;
}

function renderPaneStatus() {
  if (state.diagnostics.loading && state.diagnostics.lastLoadedAt === 0 && !state.diagnostics.error) {
    return `<div class="iot-loading"><div class="iot-not-ready-title">${escapeHtml(state.t('loading', labels.de.loading))}</div></div>`;
  }
  const detail = state.diagnostics.error
    ? `<div class="iot-not-ready-detail">${escapeHtml(state.diagnostics.error)}</div>`
    : '';
  return `
    <div class="iot-not-ready">
      <div class="iot-not-ready-icon">${signalIcon()}</div>
      <div class="iot-not-ready-title">${escapeHtml(state.t('syncNotReady', labels.de.syncNotReady))}</div>
      <div class="iot-muted-row">${escapeHtml(state.t('syncNotReadyHint', labels.de.syncNotReadyHint))}</div>
      ${detail}
    </div>
  `;
}

// ----------------------------------------------------------------------------
// Phase 6 — ACL awareness (UI affordances only; server is the security boundary)
// ----------------------------------------------------------------------------
function canModifyModule() {
  return canModifyModuleContext(state.ctx);
}

function canModifyModuleContext(ctx = {}) {
  if (ctx?.readonly === true || ctx?.permissions?.readonly === true) return false;
  if (typeof ctx?.canModifyModule === 'function' && ctx.canModifyModule()) return true;
  const user = ctx?.session?.user || {};
  if (user.is_admin === true || user.is_owner === true) return true;
  const role = String(user.role || '').trim().toLowerCase().replace(/^business_os_/, '');
  if (!role) return true; // unknown role -> optimistic enable; server still enforces
  return ['admin', 'chef', 'owner', 'founder'].includes(role);
}

function mutableDisabledAttr() {
  return canModifyModule()
    ? ''
    : ` disabled title="${escapeAttribute(state.t('permissionDenied', labels.de.permissionDenied))}"`;
}

function renderPermissionNotice() {
  if (canModifyModule()) return '';
  return `<div class="iot-readonly-notice">${escapeHtml(state.t('permissionNotice', labels.de.permissionNotice))}</div>`;
}

// ----------------------------------------------------------------------------
// Phase 6 — dispatch core: optimistic UI + await-projection reconcile.
// ----------------------------------------------------------------------------
function setCommandState(message, tone) {
  state.commandState = message || '';
  state.commandTone = tone || '';
  render();
}

// Verbatim port of outbound's projection-await helper. Reading
// db.raw.business_commands is a projected-collection read, not an rxdb import.
async function waitForBusinessCommandProjection(commandId, startedAtMs, timeoutMs = 45000) {
  const collection = state.ctx?.db?.raw?.business_commands;
  if (!collection) return null;
  const earliestUpdatedAt = Math.max(0, Number(startedAtMs || Date.now()) - 1000);
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const doc = await collection.findOne(commandId).exec();
      const match = typeof doc?.toJSON === 'function' ? doc.toJSON() : doc;
      if (
        match
        && Number(match.updated_at_ms || 0) >= earliestUpdatedAt
        && match.status
        && match.status !== 'pending_sync'
      ) {
        return match;
      }
    } catch (_) {
      // Retry below; replicated command results may arrive just after dispatch.
    }
    await new Promise((resolve) => window.setTimeout(resolve, 500));
  }
  return null;
}

async function dispatchIotCommand(command) {
  if (!canModifyModule()) {
    setCommandState(state.t('permissionDenied', labels.de.permissionDenied), 'failed');
    return null;
  }
  if (!state.ctx?.commandBus?.dispatch) {
    setCommandState(state.t('commandUnavailable', labels.de.commandUnavailable), 'failed');
    return null;
  }
  const startedAtMs = Date.now();
  // OPTIMISTIC: show pending immediately, before engine ack.
  setCommandState(state.t('commandPending', labels.de.commandPending), 'pending');
  let result;
  try {
    await state.ctx.sync?.startCollection?.('business_commands');
    result = await state.ctx.commandBus.dispatch(command);
  } catch (error) {
    setCommandState(state.t('commandFailed', labels.de.commandFailed, error?.message || String(error)), 'failed');
    return null;
  }
  const commandId = result?.command_id || command.id;
  state.pendingCommands.set(commandId, { command_type: command.type, record_id: command.record_id || '', startedAtMs });
  render();
  // RECONCILE: await projection, then re-read collections.
  const projection = await waitForBusinessCommandProjection(commandId, startedAtMs);
  state.pendingCommands.delete(commandId);
  if (!projection) {
    setCommandState(state.t('commandTimeout', labels.de.commandTimeout), 'failed');
  } else if (commandStatusTone(projection.status) === 'failed') {
    const reason = projection?.payload?.outcome?.error || projection?.error || projection?.payload?.error || '';
    setCommandState(state.t('commandFailed', labels.de.commandFailed, reason), 'failed');
  } else {
    setCommandState(state.t('commandCompleted', labels.de.commandCompleted), 'completed');
  }
  await refreshData(); // re-read iot_* projections written by project_record()
  render();
  return projection;
}

// ----------------------------------------------------------------------------
// Phase 6 — command builders. Payload shapes mirror commands.rs exactly.
// ----------------------------------------------------------------------------
function buildAttributeWriteCommand(assetId, name, value) {
  return {
    module: 'iot', type: 'ctox.iot.attribute.write', record_id: assetId,
    inbound_channel: 'business_os.iot',
    payload: { asset_id: assetId, name, value },
    client_context: { build: BUILD, surface: 'iot.attribute.write' },
  };
}

function buildAssetUpsertCommand(draft) {
  const payload = { realm: draft.realm, asset_type: draft.asset_type, name: draft.name };
  if (draft.id) payload.id = draft.id;
  if (draft.parent_id) payload.parent_id = draft.parent_id;
  return {
    module: 'iot', type: 'ctox.iot.asset.upsert', record_id: draft.id || '',
    inbound_channel: 'business_os.iot', payload,
    client_context: { build: BUILD, surface: 'iot.asset.upsert' },
  };
}

function buildAssetDeleteCommand(assetId) {
  return {
    module: 'iot', type: 'ctox.iot.asset.delete', record_id: assetId,
    inbound_channel: 'business_os.iot', payload: { asset_id: assetId },
    client_context: { build: BUILD, surface: 'iot.asset.delete' },
  };
}

function buildAlarmUpdateCommand(alarmId, action, extra = {}) {
  const payload = { alarm_id: alarmId, action };
  if (action === 'assign') payload.assignee = extra.assignee || '';
  if (action === 'status') payload.status = extra.status || '';
  return {
    module: 'iot', type: 'ctox.iot.alarm.update', record_id: alarmId,
    inbound_channel: 'business_os.iot', payload,
    client_context: { build: BUILD, surface: `iot.alarm.${action}` },
  };
}

function buildRulesetSaveCommand(draft) {
  const payload = { realm: draft.realm, name: draft.name, enabled: draft.enabled !== false };
  if (draft.id) payload.id = draft.id;
  if (draft.data && typeof draft.data === 'object') payload.data = draft.data;
  return {
    module: 'iot', type: 'ctox.iot.ruleset.save', record_id: draft.id || '',
    inbound_channel: 'business_os.iot', payload,
    client_context: { build: BUILD, surface: 'iot.ruleset.save' },
  };
}

function buildRulesetToggleCommand(rulesetId, enabled) {
  return {
    module: 'iot', type: 'ctox.iot.ruleset.toggle', record_id: rulesetId,
    inbound_channel: 'business_os.iot', payload: { ruleset_id: rulesetId, enabled: !!enabled },
    client_context: { build: BUILD, surface: 'iot.ruleset.toggle' },
  };
}

function buildAgentConfigureCommand(draft) {
  const payload = { realm: draft.realm, name: draft.name, kind: draft.kind, enabled: draft.enabled !== false };
  if (draft.id) payload.id = draft.id;
  if (draft.data && typeof draft.data === 'object') payload.data = draft.data;
  return {
    module: 'iot', type: 'ctox.iot.agent.configure', record_id: draft.id || '',
    inbound_channel: 'business_os.iot', payload,
    client_context: { build: BUILD, surface: 'iot.agent.configure' },
  };
}

function buildDatapointsQueryCommand(assetId, attributeName, chartWindow) {
  const toMs = Date.now();
  const spanMs = chartWindow === '1h' ? 3600e3 : chartWindow === '7d' ? 7 * 24 * 3600e3 : 24 * 3600e3;
  const fromMs = toMs - spanMs;
  const shape = 'lttb';
  const threshold = 200;
  const windowKey = `${assetId}:${attributeName}:${fromMs}:${toMs}:${shape}`;
  return {
    module: 'iot', type: 'ctox.iot.datapoints.query', record_id: windowKey,
    inbound_channel: 'business_os.iot',
    payload: { asset_id: assetId, attribute_name: attributeName, from_ms: fromMs, to_ms: toMs, shape, threshold },
    client_context: { build: BUILD, surface: 'iot.datapoints.query' },
  };
}

// ----------------------------------------------------------------------------
// Phase 6 — value coercion for inline attribute writes.
// Mirrors formatAttributeValue's type switch in reverse.
// ----------------------------------------------------------------------------
function coerceAttributeValue(valueType, raw) {
  switch (valueType) {
    case 'Number': {
      const num = Number(raw);
      if (!Number.isFinite(num)) return { error: state.t('errValueNaN', labels.de.errValueNaN) };
      return { value: num };
    }
    case 'Boolean':
      return { value: raw === true || raw === 'true' || raw === 'on' };
    case 'GeoPoint': {
      const lat = Number(raw?.lat);
      const lng = Number(raw?.lng);
      if (!Number.isFinite(lat) || !Number.isFinite(lng)) return { error: state.t('errValueNaN', labels.de.errValueNaN) };
      return { value: { lat, lng } };
    }
    default:
      return { value: String(raw ?? '') };
  }
}

function readAttributeInput(row, valueType) {
  if (valueType === 'Boolean') {
    const cb = row?.querySelector('[data-iot-attr-input]');
    return coerceAttributeValue('Boolean', cb ? cb.checked : false);
  }
  if (valueType === 'GeoPoint') {
    const lat = row?.querySelector('[data-iot-attr-lat]')?.value;
    const lng = row?.querySelector('[data-iot-attr-lng]')?.value;
    return coerceAttributeValue('GeoPoint', { lat, lng });
  }
  const input = row?.querySelector('[data-iot-attr-input]');
  return coerceAttributeValue(valueType, input ? input.value : '');
}

// ----------------------------------------------------------------------------
// Phase 6 — dense-form editor machinery (plain ESM; no framework vendored).
// ----------------------------------------------------------------------------
function firstRealmKey() {
  const realms = state.collections.iot_realms || [];
  return state.selectedRealm && state.selectedRealm !== '*' ? state.selectedRealm : (realmKey(realms[0]) || '');
}

function openRulesetEditor(rulesetId) {
  if (!canModifyModule()) {
    setCommandState(state.t('permissionDenied', labels.de.permissionDenied), 'failed');
    return;
  }
  const existing = (state.collections.iot_rulesets || []).find((rs) => rs.id === rulesetId);
  state.formMode = existing ? 'ruleset-edit' : 'ruleset-create';
  state.formRecordId = existing ? existing.id : '';
  state.formError = '';
  state.formDraft = {
    id: existing?.id || '',
    name: existing?.name || '',
    realm: existing ? realmKey(existing) : firstRealmKey(),
    enabled: existing ? existing.enabled !== false : true,
    dataText: JSON.stringify(existing?.data ?? {}, null, 2),
  };
  render();
}

function openAgentForm(agentId) {
  if (!canModifyModule()) {
    setCommandState(state.t('permissionDenied', labels.de.permissionDenied), 'failed');
    return;
  }
  const existing = (state.collections.iot_agents || []).find((ag) => ag.id === agentId);
  state.formMode = 'agent-configure';
  state.formRecordId = existing ? existing.id : '';
  state.formError = '';
  state.formDraft = {
    id: existing?.id || '',
    name: existing?.name || '',
    realm: existing ? realmKey(existing) : firstRealmKey(),
    kind: existing?.kind || 'mqtt',
    enabled: existing ? existing.enabled !== false : true,
    dataText: JSON.stringify(existing?.data ?? {}, null, 2),
  };
  render();
}

function openAssetForm(assetId, asChild) {
  if (!canModifyModule()) {
    setCommandState(state.t('permissionDenied', labels.de.permissionDenied), 'failed');
    return;
  }
  const existing = asChild ? null : (state.collections.iot_assets || []).find((a) => a.id === assetId);
  const parent = asChild ? (state.collections.iot_assets || []).find((a) => a.id === assetId) : null;
  state.formMode = 'asset-upsert';
  state.formRecordId = existing ? existing.id : '';
  state.formError = '';
  state.formDraft = {
    id: existing?.id || '',
    name: existing?.name || '',
    realm: existing ? realmKey(existing) : (parent ? realmKey(parent) : firstRealmKey()),
    asset_type: existing?.asset_type || '',
    parent_id: existing?.parent_id || (parent ? parent.id : ''),
  };
  render();
}

function closeForm() {
  state.formMode = '';
  state.formRecordId = '';
  state.formError = '';
  state.formDraft = null;
  render();
}

function validateRulesetDraft(draft) {
  if (!String(draft.name || '').trim()) return { valid: false, error: state.t('errNameRequired', labels.de.errNameRequired) };
  if (!String(draft.realm || '').trim()) return { valid: false, error: state.t('errRealmRequired', labels.de.errRealmRequired) };
  if (draft.dataText && draft.dataText.trim()) {
    try {
      const parsed = JSON.parse(draft.dataText);
      if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error();
      draft.data = parsed;
    } catch {
      return { valid: false, error: state.t('errDataJson', labels.de.errDataJson) };
    }
  } else {
    draft.data = {};
  }
  return { valid: true };
}

function validateAssetDraft(draft) {
  if (!String(draft.name || '').trim()) return { valid: false, error: state.t('errNameRequired', labels.de.errNameRequired) };
  if (!String(draft.realm || '').trim()) return { valid: false, error: state.t('errRealmRequired', labels.de.errRealmRequired) };
  return { valid: true };
}

async function submitForm() {
  if (!canModifyModule()) {
    setCommandState(state.t('permissionDenied', labels.de.permissionDenied), 'failed');
    return;
  }
  const draft = state.formDraft || {};
  let command;
  if (state.formMode === 'asset-upsert') {
    const v = validateAssetDraft(draft);
    if (!v.valid) { state.formError = v.error; render(); return; }
    command = buildAssetUpsertCommand(draft);
  } else {
    // ruleset-create / ruleset-edit / agent-configure all use JSON-data validation.
    const v = validateRulesetDraft(draft);
    if (!v.valid) { state.formError = v.error; render(); return; }
    command = state.formMode === 'agent-configure'
      ? buildAgentConfigureCommand(draft)
      : buildRulesetSaveCommand(draft);
  }
  closeForm();
  await dispatchIotCommand(command);
}

function renderFormOverlay() {
  if (!state.formMode || !state.formDraft) return '';
  const draft = state.formDraft;
  const realms = state.collections.iot_realms || [];
  const realmOptions = realms.map((realm) => {
    const key = realmKey(realm);
    return `<option value="${escapeAttribute(key)}"${draft.realm === key ? ' selected' : ''}>${escapeHtml(realm.name || key)}</option>`;
  }).join('');
  let title = '';
  let fields = '';
  const nameRow = `
    <label class="iot-form-row">
      <span class="iot-form-label">${escapeHtml(state.t('name', labels.de.name))}</span>
      <input class="iot-form-input" type="text" data-iot-form-field="name" value="${escapeAttribute(draft.name || '')}" />
    </label>`;
  const realmRow = `
    <label class="iot-form-row">
      <span class="iot-form-label">${escapeHtml(state.t('realm', labels.de.realm))}</span>
      <select class="iot-form-input" data-iot-form-field="realm">${realmOptions}</select>
    </label>`;
  if (state.formMode === 'asset-upsert') {
    title = state.t('editAsset', labels.de.editAsset);
    const assetTypeRow = `
      <label class="iot-form-row">
        <span class="iot-form-label">${escapeHtml(state.t('assetType', labels.de.assetType))}</span>
        <input class="iot-form-input" type="text" data-iot-form-field="asset_type" value="${escapeAttribute(draft.asset_type || '')}" />
      </label>`;
    const parentRow = `
      <label class="iot-form-row">
        <span class="iot-form-label">${escapeHtml(state.t('parentAsset', labels.de.parentAsset))}</span>
        <input class="iot-form-input" type="text" data-iot-form-field="parent_id" value="${escapeAttribute(draft.parent_id || '')}" />
      </label>`;
    fields = nameRow + realmRow + assetTypeRow + parentRow;
  } else if (state.formMode === 'agent-configure') {
    title = state.t('configureAgent', labels.de.configureAgent);
    const kindRow = `
      <label class="iot-form-row">
        <span class="iot-form-label">${escapeHtml(state.t('kind', labels.de.kind))}</span>
        <select class="iot-form-input" data-iot-form-field="kind">
          ${['mqtt', 'http', 'websocket'].map((k) => `<option value="${k}"${draft.kind === k ? ' selected' : ''}>${k}</option>`).join('')}
        </select>
      </label>`;
    const enabledRow = `
      <label class="iot-form-row iot-form-row-inline">
        <input type="checkbox" data-iot-form-field="enabled"${draft.enabled !== false ? ' checked' : ''} />
        <span class="iot-form-label">${escapeHtml(state.t('enabled', labels.de.enabled))}</span>
      </label>`;
    const dataRow = `
      <label class="iot-form-row">
        <span class="iot-form-label">${escapeHtml(state.t('agentData', labels.de.agentData))}</span>
        <textarea class="iot-form-textarea" data-iot-form-field="data" rows="6">${escapeHtml(draft.dataText || '')}</textarea>
      </label>`;
    fields = nameRow + realmRow + kindRow + enabledRow + dataRow;
  } else {
    title = state.formMode === 'ruleset-edit' ? state.t('editRuleset', labels.de.editRuleset) : state.t('newRuleset', labels.de.newRuleset);
    const enabledRow = `
      <label class="iot-form-row iot-form-row-inline">
        <input type="checkbox" data-iot-form-field="enabled"${draft.enabled !== false ? ' checked' : ''} />
        <span class="iot-form-label">${escapeHtml(state.t('enabled', labels.de.enabled))}</span>
      </label>`;
    const dataRow = `
      <label class="iot-form-row">
        <span class="iot-form-label">${escapeHtml(state.t('rulesetData', labels.de.rulesetData))}</span>
        <textarea class="iot-form-textarea" data-iot-form-field="data" rows="6">${escapeHtml(draft.dataText || '')}</textarea>
      </label>`;
    fields = nameRow + realmRow + enabledRow + dataRow;
  }
  const errorHtml = state.formError
    ? `<div class="iot-form-error">${escapeHtml(state.formError)}</div>`
    : '';
  return `
    <div class="iot-form" role="dialog" aria-modal="true">
      <h3 class="iot-section-label">${escapeHtml(title)}</h3>
      ${fields}
      ${errorHtml}
      <div class="iot-form-actions">
        <button class="iot-button" type="button" data-iot-action="form-cancel">${escapeHtml(state.t('cancel', labels.de.cancel))}</button>
        <button class="iot-button iot-button-accent" type="button" data-iot-action="form-submit">${escapeHtml(state.t('save', labels.de.save))}</button>
      </div>
    </div>`;
}

// ----------------------------------------------------------------------------
// Pure helpers (test hooks) — read-only derivations only.
// ----------------------------------------------------------------------------
function realmKey(record) {
  return String(record?.realm || record?.id || '');
}

function attributeName(attr) {
  if (!attr) return '';
  return String(attr.attribute_name || attr.name || '');
}

function summarizeIotData(collections) {
  return {
    realms: (collections?.iot_realms || []).length,
    assets: (collections?.iot_assets || []).length,
    attributes: (collections?.iot_attributes || []).length,
    alarms: (collections?.iot_alarms || []).length,
    agents: (collections?.iot_agents || []).length,
  };
}

function filterAssetsByRealm(assets, realm) {
  const list = assets || [];
  if (!realm || realm === '*') return list.slice();
  return list.filter((asset) => realmKey(asset) === realm);
}

function filterAssets(assets, options = {}) {
  const search = String(options.search || '').trim().toLowerCase();
  const list = assets || [];
  if (!search) return list.slice();
  return list.filter((asset) => {
    const haystack = `${asset.index_text || ''} ${asset.name || ''} ${asset.asset_type || ''} ${asset.id || ''}`.toLowerCase();
    return haystack.includes(search);
  });
}

function buildAssetTree(assets) {
  const list = assets || [];
  const byId = new Map(list.map((asset) => [asset.id, { asset, children: [] }]));
  const roots = [];
  for (const asset of list) {
    const node = byId.get(asset.id);
    const parentId = asset.parent_id;
    const parent = parentId ? byId.get(parentId) : null;
    if (parent) parent.children.push(node);
    else roots.push(node);
  }
  const sortNodes = (nodes) => {
    nodes.sort((a, b) => String(a.asset.name || a.asset.id).localeCompare(String(b.asset.name || b.asset.id)));
    nodes.forEach((node) => sortNodes(node.children));
  };
  sortNodes(roots);
  return roots;
}

function relatedAttributes(assetId, collections) {
  if (!assetId) return [];
  const rows = (collections?.iot_attributes || []).filter((attr) => attr.asset_id === assetId);
  if (rows.length) {
    return rows.slice().sort((a, b) => attributeName(a).localeCompare(attributeName(b)));
  }
  // Fallback to the asset's attribute_summary map when no iot_attributes rows.
  const asset = (collections?.iot_assets || []).find((item) => item.id === assetId);
  const summary = asset?.attribute_summary;
  if (summary && typeof summary === 'object') {
    return Object.entries(summary).map(([name, value]) => ({
      id: `${assetId}:${name}`,
      asset_id: assetId,
      attribute_name: name,
      value_type: inferValueType(value),
      data: { value },
    })).sort((a, b) => attributeName(a).localeCompare(attributeName(b)));
  }
  return [];
}

function selectedAssetContext(assetId, collections) {
  const asset = (collections?.iot_assets || []).find((item) => item.id === assetId) || null;
  return {
    asset,
    attributes: asset ? relatedAttributes(assetId, collections) : [],
  };
}

function inferValueType(value) {
  if (typeof value === 'number') return 'Number';
  if (typeof value === 'boolean') return 'Boolean';
  if (value && typeof value === 'object') {
    if ('lat' in value && ('lng' in value || 'lon' in value)) return 'GeoPoint';
    return Array.isArray(value) ? 'Array' : 'Object';
  }
  return 'Text';
}

function attributeValue(attr) {
  if (!attr) return undefined;
  if (attr.data && typeof attr.data === 'object' && 'value' in attr.data) return attr.data.value;
  if ('value' in attr) return attr.value;
  return undefined;
}

function formatAttributeValue(attr, t = ((_k, fallback) => fallback)) {
  const type = attr?.value_type || inferValueType(attributeValue(attr));
  const value = attributeValue(attr);
  if (value === undefined || value === null) return '—';
  switch (type) {
    case 'Number':
      return formatNumber(value);
    case 'Boolean':
      return value ? t('on', 'On') : t('off', 'Off');
    case 'GeoPoint': {
      const lat = value.lat ?? value.latitude;
      const lng = value.lng ?? value.lon ?? value.longitude;
      return `${formatNumber(lat)}, ${formatNumber(lng)}`;
    }
    case 'Object':
    case 'Array':
      try { return JSON.stringify(value); } catch { return String(value); }
    default:
      return String(value);
  }
}

function formatNumber(value) {
  const num = Number(value);
  if (!Number.isFinite(num)) return String(value);
  return Number.isInteger(num) ? String(num) : String(Math.round(num * 1000) / 1000);
}

function geoOf(asset) {
  const loc = asset?.location;
  if (!loc || typeof loc !== 'object') return null;
  const lat = loc.lat ?? loc.latitude;
  const lng = loc.lng ?? loc.lon ?? loc.longitude;
  if (!Number.isFinite(Number(lat)) || !Number.isFinite(Number(lng))) return null;
  return { lat: Number(lat), lng: Number(lng) };
}

function buildMapGeometry(assets, selectedId) {
  const geos = (assets || []).map((asset) => ({ asset, geo: geoOf(asset) })).filter((row) => row.geo);
  const lats = geos.map((row) => row.geo.lat);
  const lngs = geos.map((row) => row.geo.lng);
  const minLat = Math.min(...lats);
  const maxLat = Math.max(...lats);
  const minLng = Math.min(...lngs);
  const maxLng = Math.max(...lngs);
  const spanLat = maxLat - minLat || 1;
  const spanLng = maxLng - minLng || 1;
  const { width, height, padX, padY } = CHART_VIEWPORT;
  const points = geos.map((row) => ({
    id: row.asset.id,
    selected: row.asset.id === selectedId,
    x: padX + ((row.geo.lng - minLng) / spanLng) * (width - padX * 2),
    y: padY + (1 - (row.geo.lat - minLat) / spanLat) * (height - padY * 2),
  }));
  return { points };
}

function pickDatapointWindow(assetId, attributeName, chartWindow, collections) {
  if (!assetId || !attributeName) return null;
  const candidates = (collections?.iot_datapoints || []).filter(
    (dp) => dp.asset_id === assetId && (dp.attribute_name || '') === attributeName,
  );
  if (!candidates.length) return null;
  const matchingWindow = candidates.filter((dp) => datapointWindowKey(dp) === chartWindow);
  const pool = matchingWindow.length ? matchingWindow : candidates;
  return pool.slice().sort((a, b) => Number(b.to_ms || b.updated_at_ms || 0) - Number(a.to_ms || a.updated_at_ms || 0))[0] || null;
}

function datapointWindowKey(dp) {
  const span = Number(dp?.to_ms || 0) - Number(dp?.from_ms || 0);
  if (!Number.isFinite(span) || span <= 0) return dp?.window || '';
  const hour = 3600 * 1000;
  if (span <= hour * 1.5) return '1h';
  if (span <= hour * 36) return '24h';
  return '7d';
}

function chartPointsFromDatapoint(window) {
  if (!window) return [];
  const raw = Array.isArray(window.data)
    ? window.data
    : Array.isArray(window.data?.points)
      ? window.data.points
      : [];
  return raw.map((pt) => {
    if (Array.isArray(pt)) return { t: Number(pt[0]), v: Number(pt[1]) };
    const t = pt.t ?? pt.x ?? pt.timestamp ?? pt.ts;
    const v = pt.v ?? pt.y ?? pt.value;
    return { t: Number(t), v: Number(v) };
  }).filter((pt) => Number.isFinite(pt.v));
}

function buildChartGeometry(points) {
  const { width, height, padX, padY } = CHART_VIEWPORT;
  const list = points || [];
  if (!list.length) return { coords: [], polyline: '' };
  const ts = list.map((pt) => (Number.isFinite(pt.t) ? pt.t : 0));
  const vs = list.map((pt) => pt.v);
  const minT = Math.min(...ts);
  const maxT = Math.max(...ts);
  const minV = Math.min(...vs);
  const maxV = Math.max(...vs);
  const spanT = maxT - minT || 1;
  const spanV = maxV - minV || 1;
  const innerW = width - padX * 2;
  const innerH = height - padY * 2;
  const coords = list.map((pt, index) => {
    const tx = list.length === 1 ? 0 : (Number.isFinite(pt.t) ? (pt.t - minT) / spanT : index / (list.length - 1));
    const x = padX + tx * innerW;
    const y = padY + (1 - (pt.v - minV) / spanV) * innerH;
    return { x: round2(x), y: round2(y) };
  });
  return {
    coords,
    polyline: coords.map((pt) => `${pt.x},${pt.y}`).join(' '),
    bounds: { minT, maxT, minV, maxV },
  };
}

function round2(value) {
  return Math.round(value * 100) / 100;
}

function summarizeAlarms(alarms) {
  const summary = { total: 0, high: 0, medium: 0, low: 0 };
  for (const alarm of alarms || []) {
    summary.total += 1;
    const sev = String(alarm.severity || '').toUpperCase();
    if (sev === 'HIGH') summary.high += 1;
    else if (sev === 'MEDIUM') summary.medium += 1;
    else summary.low += 1;
  }
  return summary;
}

function groupAlarmsByStatus(alarms) {
  const groups = {};
  for (const alarm of alarms || []) {
    const key = String(alarm.status || alarm.status_key || 'unknown');
    groups[key] = (groups[key] || 0) + 1;
  }
  return groups;
}

function alarmSeverityTone(severity) {
  const sev = String(severity || '').toUpperCase();
  if (sev === 'HIGH') return 'danger';
  if (sev === 'LOW') return 'accent';
  return 'neutral';
}

function rulesetStatusTone(ruleset) {
  const enabled = ruleset?.enabled === true
    || String(ruleset?.status_key || '').toLowerCase() === 'enabled';
  return enabled ? 'enabled' : 'disabled';
}

function agentLinkTone(linkState) {
  const link = String(linkState || '').toLowerCase();
  if (link === 'connected') return 'accent';
  if (link === 'error') return 'danger';
  return 'neutral';
}

function joinAgentStatus(agents, statuses) {
  const byAgent = new Map();
  for (const status of statuses || []) {
    byAgent.set(status.agent_id, status);
  }
  return (agents || []).map((agent) => {
    const status = byAgent.get(agent.id) || {};
    return {
      agent,
      agent_id: agent.id,
      link_state: status.link_state || 'unconfigured',
      last_event_ms: status.last_event_ms || 0,
      error: status.error || '',
    };
  });
}

function summarizeIotCommands(commands) {
  const summary = { pending: 0, completed: 0, failed: 0 };
  for (const command of commands || []) {
    const tone = commandStatusTone(command.status);
    if (tone === 'completed') summary.completed += 1;
    else if (tone === 'failed') summary.failed += 1;
    else summary.pending += 1;
  }
  return summary;
}

function commandStatusTone(status) {
  const value = String(status || '').toLowerCase();
  if (value === 'completed') return 'completed';
  if (value === 'failed' || value === 'error') return 'failed';
  return 'pending';
}

function syncReady(snapshot) {
  const collections = snapshot?.collections || {};
  const diagnostics = snapshot?.diagnostics || {};
  if (diagnostics.error) return false;
  const iotNames = IOT_COLLECTIONS.filter((name) => name !== 'business_commands');
  const anyResolvable = iotNames.some((name) => Array.isArray(collections[name]));
  if (!anyResolvable) return false;
  const anyData = iotNames.some((name) => (collections[name] || []).length > 0);
  if (!anyData && !(diagnostics.lastLoadedAt > 0)) return false;
  return true;
}

function isActivationKey(key) {
  return key === 'Enter' || key === ' ' || key === 'Spacebar';
}

function formatRelative(ms, t = ((_k, fallback) => fallback)) {
  const value = Number(ms);
  if (!Number.isFinite(value) || value <= 0) return t('never', 'Never');
  const diff = Date.now() - value;
  const sec = Math.round(diff / 1000);
  if (sec < 60) return `${Math.max(sec, 0)}s`;
  const min = Math.round(sec / 60);
  if (min < 60) return `${min}m`;
  const hr = Math.round(min / 60);
  if (hr < 48) return `${hr}h`;
  const day = Math.round(hr / 24);
  return `${day}d`;
}

function emptyCollections() {
  return {
    business_commands: [],
    iot_realms: [],
    iot_asset_types: [],
    iot_assets: [],
    iot_attributes: [],
    iot_datapoints: [],
    iot_alarms: [],
    iot_agents: [],
    iot_agent_status: [],
    iot_rulesets: [],
  };
}

function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

function escapeAttribute(value) {
  return escapeHtml(value);
}

function refreshIcon() {
  return '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M21 12a9 9 0 1 1-2.64-6.36M21 4v6h-6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>';
}

function signalIcon() {
  return '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M12 20h.01M8.5 16.5a5 5 0 0 1 7 0M5 13a10 10 0 0 1 14 0" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>';
}

export const __iotTestHooks = {
  summarizeIotData,
  buildAssetTree,
  filterAssetsByRealm,
  filterAssets,
  relatedAttributes,
  selectedAssetContext,
  groupAlarmsByStatus,
  summarizeAlarms,
  joinAgentStatus,
  pickDatapointWindow,
  chartPointsFromDatapoint,
  buildChartGeometry,
  formatAttributeValue,
  alarmSeverityTone,
  rulesetStatusTone,
  isActivationKey,
  syncReady,
  summarizeIotCommands,
  // Phase 6
  BUILD,
  buildAttributeWriteCommand,
  buildAssetUpsertCommand,
  buildAssetDeleteCommand,
  buildAlarmUpdateCommand,
  buildRulesetSaveCommand,
  buildRulesetToggleCommand,
  buildAgentConfigureCommand,
  buildDatapointsQueryCommand,
  canModifyModuleContext,
  validateRulesetDraft,
  coerceAttributeValue,
  commandStatusTone,
};
