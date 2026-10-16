// Browser – App-API 4.0 (Shell v4). Die App liest browser_sessions & Co. live über api.query und sendet
// Befehle über api.command (nie warten). Der frühere native Direktkanal 'ctox.browser.live.v1' und die
// Frame-Abfrageschleife entfallen; das Live-Bild ist eine Echtzeit-Ausnahme des Kerns (V4-PORT.md OFFEN-1).
import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';

// Module-level translator; set from api.shell.t during mount (eingebettete Texte als Rückfall, OFFEN-B).
let t = (key, fallback) => fallback ?? key;
// Eingebettetes Rückfall-Wörterbuch (FIX8, Muster esign/calendar): gilt, wenn api.shell.t den Schlüssel selbst
// zurückgibt; Inhalt = locales/<lang>.json. Ohne eigenen Eintrag gilt der Rückfalltext am Aufruf.
const labels = {
  de: {
    newWindow: "Neues Fenster",
    sendToCtox: "An CTOX senden",
    close: "Schließen",
    refresh: "Aktualisieren",
    reset: "Reset",
    seed: "Testbild",
    clearImages: "Bilder löschen",
    navBack: "Zurück",
    navForward: "Vor",
    navReload: "Neu laden",
    go: "Los",
    statusReady: "Bereit",
    statusStarting: "Startet",
    statusConnecting: "Verbindet",
    statusError: "Fehler",
    frameStartFailed: "Der Browser konnte nicht gestartet werden",
    viewLive: "Ansicht",
    viewScript: "Skript",
    frameRenderFailed: "Die Seite konnte nicht angezeigt werden.",
    sessionsTitle: "Sitzungen",
    sessionsNew: "Neue Sitzung",
    sessionsImport: "Sitzungen importieren",
    sessionsExport: "Sitzungen exportieren",
    viewShowList: "Als Liste anzeigen",
    viewShowCards: "Als Karten anzeigen",
    shardImported: "Import",
    bandAll: "Alle",
    bandActive: "Aktiv",
    bandClosed: "Beendet",
    filterProfileAll: "Alle Profile",
    filterProfilePersistent: "Persönlich",
    filterProfilePrivate: "Privat",
    ctxBack: "Zurück",
    ctxForward: "Vorwärts",
    ctxReload: "Neu laden",
    ctxCopy: "Kopieren",
    ctxPaste: "Einfügen",
    tabClose: "Tab schließen",
    btnNewTab: "Neuer Tab",
    btnUpload: "Hochladen",
    btnTakeControl: "Steuerung übernehmen",
    btnReleaseControl: "Steuerung freigeben",
    btnObserverAdd: "Beobachter hinzufügen",
    btnObserverRemove: "Beobachter entfernen",
    btnClipCopy: "Zwischenablage kopieren",
    btnClipPaste: "Zwischenablage einfügen",
    btnClipClear: "Zwischenablage leeren",
    bandAdapters: "Scraping",
    adapterOff: "Deaktiviert",
    adapterAuthMissing: "Zugang fehlt",
    adapterUnreachable: "Letzte Prüfung fehlgeschlagen",
    adapterReady: "Bereit",
    adaptersFailed: "Adapter konnten nicht geladen werden: ",
    preciseType: "Einfügen",
    preciseTypeEnter: "Einfügen + Enter",
    preciseClick: "Klick",
    preciseDblClick: "Doppelklick",
    preciseRightClick: "Rechtsklick",
    preciseClose: "Schließen",
    statusDisconnected: "Beendet",
    emptyNoSession: "Keine Browser-Sitzung.",
    appTitle: "Browser",
    commandFailedPrefix: "Browser-Aktion fehlgeschlagen",
    dialogPromptTitle: "Eingabe",
    httpAuthPrompt: "CTOX Secret-Referenz für HTTP-Auth",
    httpAuthTitle: "HTTP-Auth",
    importEmpty: "Keine Sitzungen in der Datei.",
    importInvalid: "Ungültige JSON-Datei.",
    inputRejected: "Eingabe abgelehnt",
    observerPrompt: "Benutzer-ID des Beobachters",
    observerRevokePrompt: "Benutzer-ID des zu entfernenden Beobachters",
    routeUnknownSession: "Unbekannte Browser-Sitzung angefordert.",
    scriptUnavailable: "Das Automatisierungsskript ist in dieser Shell-Version nicht abrufbar.",
    uploadPrompt: "CTOX Datei-ID für den Upload",
    uploadPromptTitle: "Hochladen",
  },
  en: { // v4-allow: en
    newWindow: "New window", // v4-allow: en
    sendToCtox: "Send to CTOX", // v4-allow: en
    close: "Close", // v4-allow: en
    refresh: "Refresh", // v4-allow: en
    reset: "Reset", // v4-allow: en
    seed: "Test image", // v4-allow: en
    clearImages: "Clear images", // v4-allow: en
    navBack: "Back", // v4-allow: en
    navForward: "Forward", // v4-allow: en
    navReload: "Reload", // v4-allow: en
    go: "Go", // v4-allow: en
    statusReady: "Ready", // v4-allow: en
    statusStarting: "Requested", // v4-allow: en
    statusConnecting: "Pending", // v4-allow: en
    statusError: "Error", // v4-allow: en
    frameStartFailed: "The browser could not be started", // v4-allow: en
    viewLive: "View", // v4-allow: en
    viewScript: "Script", // v4-allow: en
    frameRenderFailed: "The page could not be displayed.", // v4-allow: en
    sessionsTitle: "Sessions", // v4-allow: en
    sessionsNew: "New session", // v4-allow: en
    sessionsImport: "Import sessions", // v4-allow: en
    sessionsExport: "Export sessions", // v4-allow: en
    viewShowList: "Show as list", // v4-allow: en
    viewShowCards: "Show as cards", // v4-allow: en
    shardImported: "Imported", // v4-allow: en
    bandAll: "All", // v4-allow: en
    bandActive: "Active", // v4-allow: en
    bandClosed: "Closed", // v4-allow: en
    filterProfileAll: "All profiles", // v4-allow: en
    filterProfilePersistent: "Personal", // v4-allow: en
    filterProfilePrivate: "Private", // v4-allow: en
    ctxBack: "Back", // v4-allow: en
    ctxForward: "Forward", // v4-allow: en
    ctxReload: "Reload", // v4-allow: en
    ctxCopy: "Copy", // v4-allow: en
    ctxPaste: "Paste", // v4-allow: en
    tabClose: "Close tab", // v4-allow: en
    btnNewTab: "New tab", // v4-allow: en
    btnUpload: "Upload", // v4-allow: en
    btnTakeControl: "Take control", // v4-allow: en
    btnReleaseControl: "Release control", // v4-allow: en
    btnObserverAdd: "Add observer", // v4-allow: en
    btnObserverRemove: "Remove observer", // v4-allow: en
    btnClipCopy: "Copy clipboard", // v4-allow: en
    btnClipPaste: "Paste clipboard", // v4-allow: en
    btnClipClear: "Clear clipboard", // v4-allow: en
    bandAdapters: "Scraping", // v4-allow: en
    adapterOff: "Disabled", // v4-allow: en
    adapterAuthMissing: "Access missing", // v4-allow: en
    adapterUnreachable: "Last check failed", // v4-allow: en
    adapterReady: "Ready", // v4-allow: en
    adaptersFailed: "Adapters could not be loaded: ", // v4-allow: en
    preciseType: "Insert", // v4-allow: en
    preciseTypeEnter: "Insert + Enter", // v4-allow: en
    preciseClick: "Click", // v4-allow: en
    preciseDblClick: "Double click", // v4-allow: en
    preciseRightClick: "Right click", // v4-allow: en
    preciseClose: "Close", // v4-allow: en
    statusDisconnected: "Closed", // v4-allow: en
    emptyNoSession: "No browser session.", // v4-allow: en
    appTitle: "Browser", // v4-allow: en
    commandFailedPrefix: "Browser action failed", // v4-allow: en
    dialogPromptTitle: "Input", // v4-allow: en
    httpAuthPrompt: "CTOX secret reference for HTTP auth", // v4-allow: en
    httpAuthTitle: "HTTP auth", // v4-allow: en
    importEmpty: "No sessions in the file.", // v4-allow: en
    importInvalid: "Invalid JSON file.", // v4-allow: en
    inputRejected: "Input rejected", // v4-allow: en
    observerPrompt: "User ID of the observer", // v4-allow: en
    observerRevokePrompt: "User ID of the observer to remove", // v4-allow: en
    routeUnknownSession: "Unknown browser session requested.", // v4-allow: en
    scriptUnavailable: "This shell version does not provide the automation script.", // v4-allow: en
    uploadPrompt: "CTOX file ID for the upload", // v4-allow: en
    uploadPromptTitle: "Upload", // v4-allow: en
  },
};
const DEFAULT_SESSION_ID = 'browser_session_default';
const DEFAULT_TAB_ID = 'browser_tab_default';
const VIEWPORT = { width: 1280, height: 720 };
const POINTER_CLICK_INTERVAL_MS = 500;
const POINTER_CLICK_SLOP_PX = 4;
const POINTER_HOVER_THROTTLE_MS = 50;
const BROWSER_NAV_COMMANDS = {
  navigate: 'browser.navigate',
  back: 'browser.back',
  forward: 'browser.forward',
  reload: 'browser.reload',
};
const BROWSER_LIVE_COLLECTIONS = Object.freeze([
  'business_commands',
  'browser_sessions',
  'browser_tabs',
  'browser_frames',
  'browser_input_events',
  'ctox_queue_tasks',
]);
const SCRAPING_ADAPTER_COLLECTIONS = Object.freeze([
  // Core Outbound campaigns.
  'outbound_research_adapters',
  // Tenant-local customer tenant campaigns. The customer tenant module owns this collection;
  // reading only the core collection made its complete adapter list disappear.
  'outbound_lead_generation_adapters',
]);
const SNAPSHOT_VIEW_KEYS = ['selectedSessionId', 'requestedSessionId', 'controllerLeaseId', 'ansicht', 'preciseEnabled'];

function translator(api) {
  return (key, fallback) => {
    const full = `browser.${key}`;
    let value = '';
    try { value = api?.shell?.t?.(full) || ''; } catch { value = ''; }
    if (value && value !== full) return value;
    let lang = 'de';
    try { lang = String(api?.shell?.locale?.get?.() || 'de').toLowerCase().startsWith('en') ? 'en' : 'de'; } catch { lang = 'de'; }
    return labels[lang]?.[key] ?? fallback ?? key;
  };
}

export async function mount(root, api, { restored } = {}) {
  t = translator(api);
  if (!root.dataset.app) root.dataset.app = 'browser';
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  root.innerHTML = html;
  api.shell.setTitle(t('appTitle', 'Browser'));

  const appRoot = root.querySelector('[data-browser-root]');
  if (!appRoot) throw new Error('browser: root element missing after fragment mount');
  // env ersetzt den v3-ctx: nur App-API, das eigene root und app-lokaler Fensterfokus.
  const env = {
    api,
    root,
    actor: api.actor,
    surfaceFocused: globalThis.document?.hasFocus?.() !== false,
    statusCodes: new Set(),
    commandWatches: new Set(),
  };
  // Zustandszone (R11) mit Code-Buchfuehrung, damit clear nur eigene Eintraege raeumt.
  env.status = { report: (item) => reportStatus(env, item), clear: (code) => clearStatus(env, code) };
  applyTranslations(appRoot);
  const refs = {
    root: appRoot,
    sessionCard: appRoot.querySelector('[data-browser-session-card]'),
    sessionList: appRoot.querySelector('[data-browser-session-list]'),
    sessionsPane: appRoot.querySelector('.browser-sessions'),
    viewToggle: appRoot.querySelector('[data-browser-view-toggle]'),
    sessions: appRoot.querySelector('[data-browser-sessions]'),
    sessionsEmpty: appRoot.querySelector('[data-browser-sessions-empty]'),
    sessionsImport: appRoot.querySelector('[data-action="import"]'),
    sessionsExport: appRoot.querySelector('[data-action="export"]'),
    start: appRoot.querySelector('[data-browser-start]'),
    toggleAdvanced: appRoot.querySelector('[data-browser-toggle-advanced]'),
    advanced: appRoot.querySelector('[data-browser-advanced]'),
    privateMode: appRoot.querySelector('[data-browser-private]'),
    viewport: appRoot.querySelector('[data-browser-viewport]'),
    newTab: appRoot.querySelector('[data-browser-new-tab]'),
    contextMenu: appRoot.querySelector('[data-browser-context-menu]'),
    tabstrip: appRoot.querySelector('[data-browser-tabstrip]'),
    adapterCount: appRoot.querySelector('[data-pg-count="adapters"]'),
    upload: appRoot.querySelector('[data-browser-upload]'),
    controllerAcquire: appRoot.querySelector('[data-browser-controller-acquire]'),
    controllerRelease: appRoot.querySelector('[data-browser-controller-release]'),
    observerGrant: appRoot.querySelector('[data-browser-observer-grant]'),
    observerRevoke: appRoot.querySelector('[data-browser-observer-revoke]'),
    clipboardCopy: appRoot.querySelector('[data-browser-clipboard-copy]'),
    clipboardPaste: appRoot.querySelector('[data-browser-clipboard-paste]'),
    clipboardClear: appRoot.querySelector('[data-browser-clipboard-clear]'),
    stop: appRoot.querySelector('[data-browser-stop]'),
    back: appRoot.querySelector('[data-browser-back]'),
    forward: appRoot.querySelector('[data-browser-forward]'),
    reload: appRoot.querySelector('[data-browser-reload]'),
    sendToCtox: appRoot.querySelector('[data-browser-send-to-ctox]'),
    notice: appRoot.querySelector('[data-browser-notice]'),
    form: appRoot.querySelector('[data-browser-address-form]'),
    go: appRoot.querySelector('[data-browser-go]'),
    address: appRoot.querySelector('[data-browser-address]'),
    addressClear: null,
    statusChip: appRoot.querySelector('[data-browser-status-chip]'),
    statusTitle: appRoot.querySelector('[data-browser-status-title]'),
    statusMeta: appRoot.querySelector('[data-browser-status-meta]'),
    downloads: appRoot.querySelector('[data-browser-downloads]'),
    authAssist: appRoot.querySelector('[data-browser-auth-assist]'),
    shell: appRoot.querySelector('[data-browser-frame-shell]'),
    canvas: appRoot.querySelector('[data-browser-canvas]'),
    railToggle: appRoot.querySelector('[data-browser-rail-toggle]'),
    preciseToggle: appRoot.querySelector('[data-browser-precise]'),
    preciseLayer: appRoot.querySelector('[data-browser-precise-layer]'),
    preciseMark: appRoot.querySelector('[data-browser-precise-mark]'),
    precisePanel: appRoot.querySelector('[data-browser-precise-panel]'),
    preciseText: appRoot.querySelector('[data-browser-precise-text]'),
    preciseHint: appRoot.querySelector('[data-browser-precise-hint]'),
    empty: appRoot.querySelector('[data-browser-empty]'),
    viewSwitch: appRoot.querySelectorAll('[data-browser-view]'),
    scriptPanel: appRoot.querySelector('[data-browser-script-panel]'),
    scriptCode: appRoot.querySelector('[data-browser-script-code]'),
    scriptPath: appRoot.querySelector('[data-browser-script-path]'),
    automationOverlay: appRoot.querySelector('[data-browser-automation-overlay]'),
    automationTitle: appRoot.querySelector('[data-browser-automation-title]'),
    automationStatus: appRoot.querySelector('[data-browser-automation-status]'),
    automationSource: appRoot.querySelector('[data-browser-automation-source]'),
    automationCode: appRoot.querySelector('[data-browser-automation-code]'),
    frameId: appRoot.querySelector('[data-browser-frame-id]'),
    frameSeq: appRoot.querySelector('[data-browser-frame-seq]'),
    frameSize: appRoot.querySelector('[data-browser-frame-size]'),
    frameTime: appRoot.querySelector('[data-browser-frame-time]'),
    inputState: appRoot.querySelector('[data-browser-input-state]'),
    commandState: appRoot.querySelector('[data-browser-command-state]'),
    commandHistory: appRoot.querySelector('[data-browser-command-history]'),
    handoffHistory: appRoot.querySelector('[data-browser-handoff-history]'),
  };
  refs.addressClear = installAddressClearControl(refs.form, refs.address, refs.go);

  const snap = restored && typeof restored === 'object' ? restored : {};
  const restoredText = (key) => (typeof snap[key] === 'string' ? snap[key] : '');
  const requestedSessionId = browserSessionIdFromArgs({ session_id: restoredText('requestedSessionId') });
  const state = {
    selectedSessionId: browserSessionIdFromArgs({ session_id: restoredText('selectedSessionId') }) || requestedSessionId,
    requestedSessionId,
    latestFrame: null,
    latestSession: null,
    latestTab: null,
    latestCommand: null,
    browserCommands: [],
    handoffTasks: [],
    notice: '',
    drawing: false,
    lastInputSeq: 0,
    lastPointerMoveAt: 0,
    lastPointerClick: null,
    pointerIsDown: false,
    leaseRenewInFlight: false,
    controllerLeaseId: restoredText('controllerLeaseId'),
    addressDirty: false,
    requestedSessionStarts: new Set(),
    // LEFT grammar column (SHELL-wired data-pg-*). The module only mirrors the
    // shell state to filter/label the sessions list; it wires no chrome itself.
    leftView: { search: '', view: 'cards', band: 'all', filters: {} },
    // Imported sessions are a read-only local overlay for review (browser
    // sessions are a read-only projection; the app never persists them).
    importedSessions: [],
    ansicht: snap.ansicht === 'script' ? 'script' : 'live',
    preciseEnabled: snap.preciseEnabled === true,
  };
  if (snap.leftView && typeof snap.leftView === 'object') {
    state.leftView = {
      search: typeof snap.leftView.search === 'string' ? snap.leftView.search : '',
      view: snap.leftView.view === 'list' ? 'list' : 'cards',
      band: typeof snap.leftView.band === 'string' ? snap.leftView.band : 'all',
      filters: snap.leftView.filters && typeof snap.leftView.filters === 'object' ? { ...snap.leftView.filters } : {},
    };
  }
  env.state = state;

  const cleanups = [];
  env.cleanups = cleanups;
  let mounted = true;
  let refreshInFlight = null;
  let refreshQueued = false;
  let leaseTimer = null;
  const scheduleRefresh = debounce(safeLoadAndRender, 80);
  cleanups.push(() => scheduleRefresh.cancel());
  // Die Tab-Leiste wird ausserhalb dieses Scopes gebaut, braucht nach einer
  // Tab-Aktion aber denselben Auffrischer wie jeder andere Knopf.
  state.refresh = safeLoadAndRender;
  env.notice = (text) => { state.notice = text || ''; renderNotice(env, state.notice); };

  function openRequestedBrowserSession(args) {
    if (!browserSessionIdFromArgs(args)) return;
    try {
      const started = ensureRequestedBrowserSession(env, state, args);
      if (!started) state.notice = '';
    } catch (error) {
      state.notice = browserStartErrorMessage(error);
    }
    scheduleRefresh();
  }

  function selectRequestedBrowserSession(detail = {}) {
    const sessionId = browserSessionIdFromArgs(detail);
    if (!sessionId) {
      // Unbekannter Routenparameter: melden statt still auf eine andere Sitzung zu fallen (R13).
      if (detail && (detail.session_id || detail.sessionId)) {
        env.status.report({ level: 'warn', code: 'browser.route.unknown_session', text: t('routeUnknownSession', 'Unbekannte Browser-Sitzung angefordert.'), ref: String(detail.session_id || detail.sessionId) });
      }
      return;
    }
    env.status.clear('browser.route.unknown_session');
    if (sessionId !== state.selectedSessionId) state.controllerLeaseId = '';
    state.selectedSessionId = sessionId;
    state.requestedSessionId = sessionId;
    scheduleRefresh();
    openRequestedBrowserSession(detail);
  }

  // Start- und Auswahlwunsch anderer Apps (v3: eventBus 'browser:select-session' und das DOM-Ereignis
  // 'ctox-business-os-app-launch') kommt in 4.0 über api.shell.onRoute.
  // Startargumente kommen flach (`{session_id, purpose, target_url, …}`) oder in der Startform der Shell
  // (`{title, mode, args: {…}}`, so ruft outbound die Anmeldung auf) – wie v3 `detail.args`.
  cleanups.push(api.shell.onRoute((event) => selectRequestedBrowserSession(browserRouteArgs(event?.params))));

  // Fensterfokus app-lokal (4.0 hat keine Fokus-API, OFFEN-3): Zeiger/Fokus im eigenen root = fokussiert,
  // Zeiger/Fokus ausserhalb = nicht fokussiert.
  const onInside = () => {
    if (env.surfaceFocused) return;
    env.surfaceFocused = true;
    handleFocusRefresh();
  };
  const onDocument = (event) => {
    if (root.contains(event.target)) return;
    if (!env.surfaceFocused) return;
    env.surfaceFocused = false;
    scheduleRefresh();
  };
  root.addEventListener('pointerdown', onInside, true);
  root.addEventListener('focusin', onInside, true);
  globalThis.document?.addEventListener?.('pointerdown', onDocument, true);
  globalThis.document?.addEventListener?.('focusin', onDocument, true);
  cleanups.push(() => {
    root.removeEventListener('pointerdown', onInside, true);
    root.removeEventListener('focusin', onInside, true);
    globalThis.document?.removeEventListener?.('pointerdown', onDocument, true);
    globalThis.document?.removeEventListener?.('focusin', onDocument, true);
  });
  function handleFocusRefresh() {
    scheduleRefresh();
    renewControllerLeaseIfNeeded();
  }
  globalThis.addEventListener?.('focus', handleFocusRefresh);
  globalThis.addEventListener?.('blur', scheduleRefresh);
  globalThis.document?.addEventListener?.('visibilitychange', handleFocusRefresh);
  cleanups.push(() => {
    globalThis.removeEventListener?.('focus', handleFocusRefresh);
    globalThis.removeEventListener?.('blur', scheduleRefresh);
    globalThis.document?.removeEventListener?.('visibilitychange', handleFocusRefresh);
  });

  // Live-Abos statt Sync-Start und Collection-$-Abos: jede Änderung zeichnet neu (entprellt 80 ms).
  // Je Montage lokal gehalten (PORTING §4 Falle 1).
  for (const collectionName of BROWSER_LIVE_COLLECTIONS) {
    cleanups.push(api.query(collectionName, {}).subscribe(() => scheduleRefresh()));
  }
  api.setVisible?.([...BROWSER_LIVE_COLLECTIONS]);
  // Scraping-Adapter (Fremd-Collections, OFFEN-5): Live-Abo nur, solange das Band sichtbar ist.
  let adapterOffs = null;
  state.watchAdapters = (on) => {
    if (!on) {
      for (const off of adapterOffs || []) off();
      adapterOffs = null;
      return;
    }
    if (adapterOffs) return;
    adapterOffs = [];
    const rowsByCollection = new Map();
    for (const collectionName of SCRAPING_ADAPTER_COLLECTIONS) {
      try {
        adapterOffs.push(api.query(collectionName, { sort: [{ updated_at_ms: 'desc' }] }).subscribe((rows) => {
          rowsByCollection.set(collectionName, rows);
          applyScrapingAdapters(env, state, rowsByCollection);
          if (state.leftView?.band === 'adapters') renderAdapterRail(env, refs, state);
        }));
        env.status.clear(`browser.adapters.${collectionName}`);
      } catch (error) {
        env.status.report({ level: 'warn', code: `browser.adapters.${collectionName}`, text: `${t('adaptersFailed', 'Adapter konnten nicht gelesen werden: ')}${collectionName}: ${String(error?.message || error)}` });
      }
    }
  };
  cleanups.push(() => state.watchAdapters(false));

  // LEFT column grammar is SHELL-wired (search / shard-list toggle / filter
  // tray / reset / active-dot / counted band). The module re-renders the
  // sessions list on the bubbling change event; no manual refresh button —
  // the sessions list is reactive (collection subscriptions above).
  const onLeftGrammarChange = (event) => {
    const detail = event?.detail || refs.sessionsPane?.__ctoxPaneGrammar?.state?.() || {};
    state.leftView = {
      search: String(detail.search || '').trim().toLowerCase(),
      view: detail.view === 'list' ? 'list' : 'cards',
      band: detail.band || 'all',
      filters: detail.filters || {},
    };
    // Der Umschalter ist eine AKTION: die Shell stempelt aria-pressed auf jeden
    // [data-pg-view]-Knopf und emittiert danach dieses Ereignis — hier faellt
    // das Attribut wieder weg, und Symbol/Beschriftung folgen der Ansicht.
    syncViewToggle(refs, state.leftView.view);
    renderLeftRail(env, refs, state);
    renderTabstrip(env, refs, state);
  };
  appRoot.addEventListener('ctox-pane-grammar-change', onLeftGrammarChange);
  cleanups.push(() => appRoot.removeEventListener('ctox-pane-grammar-change', onLeftGrammarChange));
  refs.sessionsImport?.addEventListener('click', () => importBrowserSessions(env, state, refs));
  refs.sessionsExport?.addEventListener('click', () => exportBrowserSessions(state, refs));
  // EIN Umschalter Karten/Liste (Betreiber-Direktive 31.08.2026). Der Knopf
  // dreht data-pg-view auf sich selbst; die Shell-Grammatik liest daraus
  // weiterhin die AKTUELLE Ansicht (viewButtons[0].dataset.pgView) und meldet
  // sie ueber ctox-pane-grammar-change zurueck. mount() verdrahtet die Grammatik
  // danach selbst (autoWirePaneGrammar), dieser Listener laeuft also vor ihrem —
  // sie sieht beim Emittieren schon den gedrehten Wert. Das eigene Rendern hier
  // haelt den Umschalter auch ohne Grammatik funktionsfaehig.
  refs.viewToggle?.addEventListener('click', () => {
    const next = refs.viewToggle.dataset.pgView === 'list' ? 'cards' : 'list';
    syncViewToggle(refs, next);
    state.leftView = { ...state.leftView, view: next };
    renderLeftRail(env, refs, state);
  });
  syncViewToggle(refs, state.leftView.view);
  refs.toggleAdvanced?.addEventListener('click', () => {
    if (!refs.advanced) return;
    const hidden = refs.advanced.classList.toggle('is-advanced-hidden');
    refs.toggleAdvanced.setAttribute('aria-pressed', hidden ? 'false' : 'true');
  });
  const startNewBrowserSession = (url = refs.address?.value || 'https://example.com') => {
    const now = Date.now();
    // Jeder Aufruf vergibt eine NEUE Sitzungskennung und startet einen eigenen
    // Chrome. Ohne Sperre wird aus zwei Klicks zweimal alles.
    // Am 16.08.2026 auf der Kundeninstanz gemessen: sieben Startbefehle in zwei
    // Minuten, sieben Sitzungen, 21 Chrome-Prozesse, 34 Eintraege in der Liste.
    // Mitverursacht durch die Reparatur vom 15.08.: seitdem fuehrt "Los" bei
    // einer toten Sitzung in den Start-Zweig — richtig, aber eben oefter.
    const grund = startSperrgrund(state, now);
    if (grund) {
      console.info('[browser] Start uebersprungen:', grund);
      state.notice = grund;
      safeLoadAndRender();
      return;
    }
    state.startPendingSince = now;
    const sessionId = `${userSessionPrefix(env.actor)}_${now}`;
    const tabId = `browser_tab_${now}`;
    const viewport = selectedViewport(refs.viewport);
    console.info(`[browser] session start clicked session_id=${sessionId} tab_id=${tabId}`);
    state.addressDirty = false;
    state.selectedSessionId = sessionId;
    state.requestedSessionId = sessionId;
    state.latestSession = null;
    state.latestTab = null;
    state.latestFrame = null;
    state.controllerLeaseId = newBrowserControllerLeaseId();
    state.notice = '';
    safeLoadAndRender();
    const startPayload = {
      session_id: sessionId,
      tab_id: tabId,
      url,
      viewport_w: viewport.width,
      viewport_h: viewport.height,
      profile_mode: refs.privateMode?.checked ? 'private' : 'persistent',
      lease_id: state.controllerLeaseId,
      new_session: true,
    };
    // Kein Warten: der Befehl liegt sofort lokal; die Sperre loest sich, sobald der
    // Befehl endet (done/failed) – oder nach START_SPERRE_MS (startSperrgrund).
    runBrowserCommand(dispatchBrowserCommand(env, state, 'browser.session.start', startPayload, {
      onSettled: () => { state.startPendingSince = 0; },
    }));
  };
  refs.start?.addEventListener('click', () => startNewBrowserSession());
  refs.stop?.addEventListener('click', () => runBrowserCommand(dispatchBrowserCommand(env, state, 'browser.session.stop')));
  refs.newTab?.addEventListener('click', () => {
    const now = Date.now();
    runBrowserCommand(dispatchBrowserCommand(env, state, 'browser.tab.open', {
      tab_id: `browser_tab_${now}`,
      url: refs.address?.value || 'https://example.com',
    }));
  });
  refs.upload?.addEventListener('click', async () => {
    const fileId = await askText(env, { title: t('uploadPromptTitle', 'Hochladen'), text: t('uploadPrompt', 'CTOX Datei-ID für den Upload') });
    if (!fileId) return;
    runBrowserCommand(dispatchBrowserCommand(env, state, 'browser.upload.select', { file_id: fileId.trim() }));
  });
  refs.controllerAcquire?.addEventListener('click', () => {
    if (state.leaseReacquireInFlight) return;
    state.leaseReacquireInFlight = true;
    state.lastReacquireAtMs = Date.now();
    const leaseId = newBrowserControllerLeaseId();
    state.controllerLeaseId = leaseId;
    runBrowserCommand(
      requestBrowserControllerLease(env, state, 'controller.acquire', leaseId)
        .catch((error) => {
          if (state.controllerLeaseId === leaseId) state.controllerLeaseId = '';
          throw error;
        })
        .finally(() => {
          state.leaseReacquireInFlight = false;
        }),
    );
  });
  refs.controllerRelease?.addEventListener('click', () => {
    const leaseId = state.controllerLeaseId;
    runBrowserCommand(
      requestBrowserControllerLease(env, state, 'controller.release', leaseId)
        .then((result) => {
          if (state.controllerLeaseId === leaseId) state.controllerLeaseId = '';
          return result;
        }),
    );
  });
  refs.observerGrant?.addEventListener('click', async () => {
    const userId = await askText(env, { title: t('btnObserverAdd', 'Beobachter hinzufügen'), text: t('observerPrompt', 'Benutzer-ID des Beobachters') });
    if (!userId) return;
    runBrowserCommand(dispatchBrowserCommand(env, state, 'browser.observer.grant', { user_id: userId.trim() }));
  });
  refs.observerRevoke?.addEventListener('click', async () => {
    const userId = await askText(env, { title: t('btnObserverRemove', 'Beobachter entfernen'), text: t('observerRevokePrompt', 'Benutzer-ID des zu entfernenden Beobachters') });
    if (!userId) return;
    runBrowserCommand(dispatchBrowserCommand(env, state, 'browser.observer.revoke', { user_id: userId.trim() }));
  });
  refs.clipboardCopy?.addEventListener('click', () => runBrowserCommand(dispatchBrowserCommand(env, state, 'browser.clipboard.copy', { confirmed: true })));
  refs.clipboardPaste?.addEventListener('click', () => runBrowserCommand(dispatchBrowserCommand(env, state, 'browser.clipboard.paste', { confirmed: true })));
  refs.clipboardClear?.addEventListener('click', () => runBrowserCommand(dispatchBrowserCommand(env, state, 'browser.clipboard.clear', { confirmed: true })));
  refs.back?.addEventListener('click', () => runBrowserCommand(submitBrowserNav(env, state, refs, 'back')));
  refs.forward?.addEventListener('click', () => runBrowserCommand(submitBrowserNav(env, state, refs, 'forward')));
  refs.reload?.addEventListener('click', () => runBrowserCommand(submitBrowserNav(env, state, refs, 'reload')));
  refs.address?.addEventListener('input', () => {
    state.addressDirty = true;
  });
  refs.address?.addEventListener('focus', (event) => {
    event.currentTarget?.select?.();
  });
  refs.addressClear?.addEventListener('click', () => {
    if (!refs.address) return;
    refs.address.value = '';
    state.addressDirty = true;
    refs.address.focus();
  });
  refs.sendToCtox?.addEventListener('click', () => runBrowserCommand(sendBrowserContextToCtox(env, state)));
  refs.authAssist?.addEventListener('click', async (event) => {
    const permissionButton = event.target?.closest?.('[data-browser-permission-response]');
    if (permissionButton) {
      runBrowserCommand(dispatchBrowserCommand(env, state, 'browser.permission.respond', {
        accept: permissionButton.dataset.browserPermissionResponse === 'accept',
        confirmed: true,
      }));
      return;
    }
    const httpAuthButton = event.target?.closest?.('[data-browser-http-auth-response]');
    if (httpAuthButton) {
      const accept = httpAuthButton.dataset.browserHttpAuthResponse === 'accept';
      const secretName = accept ? await askText(env, { title: t('httpAuthTitle', 'HTTP-Auth'), text: t('httpAuthPrompt', 'CTOX Secret-Referenz für HTTP-Auth') }) : '';
      if (accept && !secretName) return;
      runBrowserCommand(dispatchBrowserCommand(env, state, 'browser.http_auth.respond', {
        accept,
        confirmed: true,
        secret_name: secretName?.trim?.() || '',
      }));
      return;
    }
    const webAuthnButton = event.target?.closest?.('[data-browser-webauthn-response]');
    if (webAuthnButton) {
      runBrowserCommand(dispatchBrowserCommand(env, state, 'browser.webauthn.respond', {
        accept: webAuthnButton.dataset.browserWebauthnResponse === 'accept',
        confirmed: true,
      }));
      return;
    }
    const dialogButton = event.target?.closest?.('[data-browser-dialog-response]');
    if (dialogButton) {
      const accept = dialogButton.dataset.browserDialogResponse === 'accept';
      const dialog = state.latestSession?.payload?.pending_dialog;
      const value = accept && dialog?.type === 'prompt'
        ? await askText(env, { title: t('dialogPromptTitle', 'Eingabe'), text: dialog.message || 'Eingabe', value: dialog.default_value || '' })
        : undefined;
      runBrowserCommand(dispatchBrowserCommand(env, state, 'browser.dialog.respond', { accept, value }));
      return;
    }
    const fillButton = event.target?.closest?.('[data-browser-credential-fill]');
    if (fillButton) {
      runBrowserCommand(fillWebStackCredential(env, state));
      return;
    }
    const completeButton = event.target?.closest?.('[data-browser-auth-complete]');
    if (completeButton) {
      runBrowserCommand(completeWebStackAuthAssist(env, state));
      return;
    }
    const captureButton = event.target?.closest?.('[data-browser-web-stack-capture]');
    if (captureButton) runBrowserCommand(sendBrowserContextToCtox(env, state, { webStack: true }));
    const extractButton = event.target?.closest?.('[data-browser-web-stack-extract]');
    if (extractButton) runBrowserCommand(extractWebStackFields(env, state));
  });
  refs.downloads?.addEventListener('click', (event) => {
    const action = event.target?.closest?.('[data-browser-download-action]');
    if (!action) return;
    runBrowserCommand(dispatchBrowserCommand(env, state, `browser.download.${action.dataset.browserDownloadAction}`, {
      download_id: action.dataset.browserDownloadId || '',
    }));
  });
  refs.sessionList?.addEventListener('click', (event) => {
    const tabItem = event.target?.closest?.('[data-browser-tab-id]');
    if (tabItem) {
      const tabId = tabItem.dataset.browserTabId || '';
      const commandType = event.target?.closest?.('[data-browser-tab-close]')
        ? 'browser.tab.close'
        : 'browser.tab.activate';
      runBrowserCommand(dispatchBrowserCommand(env, state, commandType, { tab_id: tabId }));
      return;
    }
    const item = event.target?.closest?.('[data-browser-session-id]');
    if (!item) return;
    state.selectedSessionId = item.dataset.browserSessionId || '';
    safeLoadAndRender();
  });
  // LEFT sessions selector. Selecting a session is an in-place is-selected flip
  // across existing rows — it must NOT rebuild the list (a rebuild would clamp
  // the well's scrollTop to 0 and yank the operator to the top). Only the data
  // that drives the MAIN canvas changes, so the reactive load re-renders the
  // work surface while the signature guard leaves the list markup untouched.
  refs.sessions?.addEventListener('click', (event) => {
    const item = event.target?.closest?.('[data-browser-session-id]');
    if (!item) return;
    const sessionId = item.dataset.browserSessionId || '';
    if (sessionId === state.selectedSessionId) return;
    if (sessionId !== state.selectedSessionId) state.controllerLeaseId = '';
    state.selectedSessionId = sessionId;
    state.requestedSessionId = '';
    // Bind the work surface synchronously to the clicked row.  Waiting for a
    // second asynchronous collection read leaves latestSession pointing at
    // the previously selected (often disconnected) session.  During that
    // window the controller button and the direct input pump otherwise send
    // acquire/input requests to the old session even though the new row is
    // already painted as selected.
    const selectedSession = latestSession(state.visibleSessions || [], sessionId);
    state.latestSession = selectedSession;
    state.latestFrame = null;
    state.latestTab = null;
    markActiveSession(refs, sessionId);
    safeLoadAndRender();
  });
  const submitAddress = () => {
    // Am 15.08.2026 auf der Kundeninstanz gemessen, und es war die Ursache:
    // Die Entscheidung "navigieren oder neu starten" haing allein an der PACHT,
    // nie an der LEBENDIGKEIT der Sitzung. Zustand der Sitzung des Nutzers:
    //   status = disconnected, Pacht gueltig, Steuerung = der Nutzer selbst.
    // Damit war canNavigate wahr, der Start-Zweig unerreichbar, und jede
    // eingegebene Adresse ging als browser.navigate an eine tote Sitzung.
    // Beleg: letzter Sitzungsstart aus der Oberflaeche 13.08. 12:51, danach in
    // 48 h nur noch 259 x browser.controller.acquire — eine gueltige Pacht auf
    // einer Leiche, die meine eigene Wiederbeschaffung frisch hielt.
    // Eine tote Sitzung MUSS in den Start-Zweig fuehren, sonst gibt es keinen
    // Weg zurueck.
    const url = refs.address?.value || 'https://example.com';
    if (browserAddressAction(state.latestSession, browserSurfaceCanControl(env, state)) === 'start') {
      startNewBrowserSession(url);
      return;
    }
    state.addressDirty = false;
    runBrowserCommand(submitBrowserNav(env, state, refs, 'navigate', { url }));
  };
  refs.form?.addEventListener('submit', (event) => {
    event.preventDefault();
    // OHNE dieses catch stirbt der Klick lautlos. Am 14.08.2026 gemessen: der
    // Nutzer drueckt "Los", nichts passiert, keine Meldung, kein Befehl in der
    // Datenbank. Der Rueckweg war dabei nachweislich intakt — ein serverseitig
    // eingespeister Startbefehl lieferte in 30 s eine aktive Sitzung mit
    // Bildstrom. Wirft irgendetwas in dieser Kette, sieht es fuer den Nutzer
    // aus wie ein toter Knopf.
    try {
      submitAddress();
    } catch (error) {
      console.error('[browser] Adresse absenden fehlgeschlagen', error);
      state.notice = `Der Vorgang konnte nicht gestartet werden: ${error?.message || error}`;
      safeLoadAndRender();
    }
  });
  // Some embedded/webview browsers do not synthesize a form submit for Enter
  // in the address input (this was reproducible in the production UI).  Keep
  // the keyboard path explicit so address navigation is not mouse-only.
  refs.address?.addEventListener('keydown', (event) => {
    if (event.key !== 'Enter' || event.isComposing) return;
    event.preventDefault();
    try {
      submitAddress();
    } catch (error) {
      console.error('[browser] Adresse per Eingabetaste absenden fehlgeschlagen', error);
      state.notice = `Der Vorgang konnte nicht gestartet werden: ${error?.message || error}`;
      safeLoadAndRender();
    }
  });
  installRailToggle(env, refs, snap.railCollapsed === true);
  installPreciseInput(env, refs, state);
  installInputHandlers(env, refs, state, scheduleRefresh);
  installCanvasPaste(env, refs, state);
  installViewSwitch(env, refs, state);
  // Die Grammatik der linken Spalte verdrahtet die App selbst (R14), nicht eine DOM-Heuristik der Shell.
  autoWirePaneGrammar(root);
  // Locale-Wechsel live (R8).
  cleanups.push(api.shell.locale.subscribe(() => {
    applyTranslations(appRoot);
    syncViewToggle(refs, state.leftView.view);
    scheduleRefresh();
  }));
  safeLoadAndRender();

  function snapshot() {
    const out = { v: 1 };
    for (const key of SNAPSHOT_VIEW_KEYS) out[key] = state[key];
    out.leftView = { ...state.leftView, filters: { ...(state.leftView.filters || {}) } };
    out.railCollapsed = appRoot.classList.contains('is-rail-collapsed');
    // Offene Eingaben IMMER mitnehmen (PORTING §2).
    if (state.addressDirty && refs.address) out.addressDraft = refs.address.value;
    if (refs.preciseText?.value) out.preciseDraft = refs.preciseText.value;
    return out;
  }
  if (typeof snap.addressDraft === 'string' && refs.address) {
    refs.address.value = snap.addressDraft;
    state.addressDirty = true;
  }
  if (typeof snap.preciseDraft === 'string' && refs.preciseText) refs.preciseText.value = snap.preciseDraft;

  return {
    snapshot,
    unmount() {
      mounted = false;
      if (leaseTimer) globalThis.clearTimeout(leaseTimer);
      leaseTimer = null;
      for (const off of env.commandWatches) {
        try { off(); } catch (error) { console.error('[browser] cleanup failed', error); }
      }
      env.commandWatches.clear();
      for (const cleanup of cleanups) {
        try { cleanup(); } catch (error) { console.error('[browser] cleanup failed', error); }
      }
      cleanups.length = 0;
      root.replaceChildren();
    },
  };

  // Steuerpacht erneuern ohne Abfrageschleife: ein einzelner Termin aus dem Ablaufzeitpunkt der Pacht
  // im Live-Dokument (75 s vor Ablauf erneuern, bei Ablauf neu holen), neu gesetzt nach jedem Neuzeichnen.
  function scheduleLeaseCheck() {
    if (leaseTimer) globalThis.clearTimeout(leaseTimer);
    leaseTimer = null;
    if (!mounted) return;
    const session = state.latestSession;
    const expiresAt = Number(session?.controller_lease_expires_at_ms || 0);
    if (!session?.id || !Number.isFinite(expiresAt) || expiresAt <= 0) return;
    const now = Date.now();
    const renewAt = expiresAt - 75_000;
    const dueAt = renewAt > now ? renewAt : expiresAt > now ? expiresAt : now + 10_000;
    leaseTimer = globalThis.setTimeout(() => {
      leaseTimer = null;
      renewControllerLeaseIfNeeded();
    }, Math.max(1_000, dueAt - now));
  }

  function renewControllerLeaseIfNeeded() {
    // Auch hier wiedererkennen, nicht nur beim Neuzeichnen: Nach einem
    // Zurueckholen laeuft die Liste nicht zwangslaeufig neu durch, und ohne
    // die eigene Kennung schlaegt die Erneuerung unten still fehl — die Pacht
    // altert dann bis zum Ablauf aus, das Bild friert ein, und erst der
    // naechste Rueckholzyklus bringt es zurueck. Auf der Kundeninstanz
    // gemessen: 89s -> 72s -> 54s -> 36s -> 25s ohne eine einzige Erneuerung,
    // obwohl Sichtbarkeit, Fokus und Fensterfokus alle gesetzt waren.
    erkenneEigenePachtWieder(env, state);
    const session = state.latestSession;
    const actorIds = browserActorIds(env.actor);
    if (!shouldRenewControllerLease(session, actorIds, Date.now(), {
      documentVisible: globalThis.document?.visibilityState !== 'hidden',
      documentFocused: globalThis.document?.hasFocus?.() !== false,
      surfaceFocused: Boolean(env.surfaceFocused),
      renewInFlight: state.leaseRenewInFlight,
      controllerLeaseId: state.controllerLeaseId,
    })) {
      // Abgelaufen? Dann nicht erneuern, sondern NEU HOLEN. Ohne diesen Zweig
      // bleibt die Sitzung fuer immer unbedienbar, waehrend sie "Bereit" meldet.
      versucheSteuerungZurueckzuholen();
      return;
    }
    state.leaseRenewInFlight = true;
    requestBrowserControllerLease(env, state, 'controller.renew', state.controllerLeaseId)
      .catch((error) => {
        console.warn('[browser] controller lease renewal failed', error);
      })
      .finally(() => {
        state.leaseRenewInFlight = false;
      });
  }

  // writeInputEvent liegt ausserhalb dieses Bereichs und kennt nur env/state.
  // Ueber diesen Haken kann ein blockierter Klick die Steuerung anfordern.
  state.steuerungZurueckholen = (optionen) => versucheSteuerungZurueckzuholen(optionen);

  function versucheSteuerungZurueckzuholen(optionen = {}) {
    const session = state.latestSession;
    if (!shouldReacquireControllerLease(session, browserActorIds(env.actor), Date.now(), {
      reacquireInFlight: state.leaseReacquireInFlight,
      lastReacquireAtMs: state.lastReacquireAtMs,
      uebernahmeDurchInteraktion: Boolean(optionen.uebernahmeDurchInteraktion),
      currentLeaseId: state.controllerLeaseId,
    })) return;
    state.leaseReacquireInFlight = true;
    state.lastReacquireAtMs = Date.now();
    const leaseId = newBrowserControllerLeaseId();
    state.controllerLeaseId = leaseId;
    console.info('[browser] Steuerung abgelaufen — hole sie zurück', { session: session?.id });
    requestBrowserControllerLease(env, state, 'controller.acquire', leaseId)
      .then(() => {
        setzeEingabeHinweis(env, state, '');
        safeLoadAndRender();
      })
      .catch((error) => {
        if (state.controllerLeaseId === leaseId) state.controllerLeaseId = '';
        console.warn('[browser] Steuerung konnte nicht zurückgeholt werden', error);
        setzeEingabeHinweis(env, state, 'Steuerung abgelaufen und konnte nicht zurückgeholt werden.');
      })
      .finally(() => {
        state.leaseReacquireInFlight = false;
      });
  }

  function safeLoadAndRender() {
    if (refreshInFlight) {
      refreshQueued = true;
      return refreshInFlight;
    }
    refreshInFlight = loadAndRender()
      .catch((error) => console.warn('[browser] refresh failed', error))
      .finally(() => {
        refreshInFlight = null;
        if (!refreshQueued || !mounted) return;
        refreshQueued = false;
        safeLoadAndRender();
      });
    return refreshInFlight;
  }

  function runBrowserCommand(promise) {
    return promise
      .then((result) => {
        if (result?.opensNewSession && result.sessionId) {
          state.selectedSessionId = result.sessionId;
          state.requestedSessionId = result.sessionId;
        }
        state.notice = '';
        safeLoadAndRender();
      })
      .catch((error) => {
        state.notice = browserStartErrorMessage(error);
        console.warn('[browser] command failed', error);
        safeLoadAndRender();
      });
  }

  async function loadAndRender() {
    if (!mounted) return;
    // Alle Lesezugriffe sind lokale Live-Abfragen (api.query(...).get()), kein Netz, kein Warten.
    const actorIds = browserActorIds(env.actor);
    const commands = readCollection(env, 'business_commands', { limit: 50 });
    // Owner-Entscheidung 04.09.2026: "der browser mit dem unblocking muss
    // auch für alle funktionieren! nicht nur für einen User!" Eine
    // Anmeldesitzung des Web Stack gehoert der FIRMA, nicht der Person, die
    // sie zufaellig ausgeloest hat: sie existiert nur, damit irgendjemand die
    // Recherche entsperrt. Gemessen an 276 Sitzungen lagen 152 bei einem
    // einzigen Konto - kein anderer Nutzer konnte ein Unblocking abschliessen.
    //
    // Persoenliche Sitzungen bleiben persoenlich. Geteilt wird ausschliesslich
    // `purpose = web_stack_auth`.
    const sessions = readCollection(env, 'browser_sessions', {
      limit: 50,
      selector: {
        $or: [
          { owner_user_id: { $in: actorIds } },
          { 'payload.purpose': 'web_stack_auth' },
        ],
      },
    });
    const requestedSession = readDocument(env, 'browser_sessions', state.requestedSessionId || state.selectedSessionId);
    const initialTabs = readCollection(env, 'browser_tabs', { limit: 40 });
    const inputs = readCollection(env, 'browser_input_events', { limit: 80 });
    const handoffTasks = readCollection(env, 'ctox_queue_tasks', { limit: 50 });
    const visibleSessions = mergeRequestedSession(sessions, requestedSession)
      .filter((session) => actorIds.includes(String(session.owner_user_id || ''))
        || String(session.payload?.purpose || '') === 'web_stack_auth');
    if (state.selectedSessionId
      && state.selectedSessionId !== state.requestedSessionId
      && !visibleSessions.some((session) => session.id === state.selectedSessionId)) {
      state.selectedSessionId = '';
    }
    const selectedSession = state.selectedSessionId ? latestSession(visibleSessions, state.selectedSessionId) : null;
    const requestedTab = readDocument(
      env,
      'browser_tabs',
      selectedSession?.current_tab_id || requestedSession?.current_tab_id || '',
    );
    const tabs = mergeRequestedDocument(initialTabs, requestedTab);
    const requestedSessionPending = Boolean(state.requestedSessionId && !selectedSession);
    const frameSessionId = selectedSession?.id || state.selectedSessionId || '';
    // Live-Bild: nur was der Kern als browser_frames projiziert (OFFEN-1, Echtzeit-Spur nicht gebaut).
    const frames = frameSessionId
      ? readCollection(env, 'browser_frames', {
        limit: 20,
        selector: { session_id: frameSessionId },
      })
      : [];
    if (!mounted) return;
    const newestFrame = latestFrame(frames);
    state.latestSession = requestedSessionPending
      ? null
      : selectedSession || latestSession(visibleSessions, newestFrame?.session_id) || latestSession(visibleSessions);
    if (!requestedSessionPending) {
      state.selectedSessionId = state.latestSession?.id || '';
      erkenneEigenePachtWieder(env, state);
      if (state.latestSession?.id === state.requestedSessionId
        && browserSessionIsLive(state.latestSession)) {
        state.requestedSessionId = '';
      }
    }
    state.latestFrame = latestFrame(frames, state.latestSession?.id);
    state.latestTab = latestTab(tabs, state.latestFrame?.tab_id || state.latestSession?.current_tab_id);
    state.latestCommand = latestBrowserCommand(commands, state.latestSession?.id || state.latestFrame?.session_id);
    applyLatestNavigationResult(state, commands);
    state.browserCommands = latestBrowserCommands(commands, state.latestSession?.id || state.latestFrame?.session_id, 5);
    state.handoffTasks = latestBrowserHandoffTasks(handoffTasks, state.latestSession?.id || state.latestFrame?.session_id, 5);
    state.lastInputSeq = Math.max(
      Number(state.lastInputSeq || 0),
      ...inputs.map((event) => Number(event.seq || 0)).filter(Number.isFinite),
    );
    const renderedTabs = state.latestTab?.id
      ? tabs.map((tab) => tab.id === state.latestTab.id ? { ...tab, ...state.latestTab } : tab)
      : tabs;
    state.visibleSessions = visibleSessions;
    state.tabs = tabs;
    // Left = persistent sessions selector; syncs from the shell grammar state
    // (may not be wired yet on the first paint — that is null-guarded).
    const grammar = refs.sessionsPane?.__ctoxPaneGrammar?.state?.();
    if (grammar) {
      state.leftView = {
        search: String(grammar.search || '').trim().toLowerCase(),
        view: grammar.view === 'list' ? 'list' : 'cards',
        band: grammar.band || 'all',
        filters: grammar.filters || {},
      };
    }
    renderLeftRail(env, refs, state);
    renderTabstrip(env, refs, state);
    // Auto-reveal: the remote work surface is meaningful once a session is
    // selected (visible = hasSelection && !userCollapsed). No session -> the
    // canvas shows its empty stage instead of stale chrome.
    refs.root.classList.toggle('is-session-active', browserWorkbenchVisible(Boolean(state.latestSession?.id)));
    renderSessionList(refs, visibleSessions, renderedTabs, state.latestSession);
    renderSession(refs, state.latestSession, state.latestTab, state.latestFrame, state.latestCommand, state);
    renderAuthAssist(refs, state.latestSession);
    renderAutomationOverlay(refs, state.latestSession);
    renderStatus(refs, state.latestSession, state.latestTab, state.latestFrame, state.latestCommand);
    renderDownloads(refs, state.latestSession);
    renderDiagnostics(refs, state.latestFrame, inputs, state.latestCommand, state.browserCommands, state.handoffTasks);
    renderControls(env, refs, state);
    renderNotice(env, state.notice);
    scheduleLeaseCheck();
    await renderFrame(env, refs, state.latestFrame, state);
  }
}

async function submitBrowserNav(env, state, refs, action, extra = {}) {
  const commandType = BROWSER_NAV_COMMANDS[action];
  if (!commandType) throw new Error('Unknown browser navigation action: ' + action);
  const payload = action === 'navigate'
    ? { url: extra.url || refs.address?.value || 'https://example.com' }
    : {};
  return dispatchBrowserCommand(env, state, commandType, payload);
}

// SPUR auf dem Befehlsweg. Am 13.08.2026 blieb der Sitzungsstart aus, ohne dass
// irgendwo etwas erschien: kein Chrome-Prozess, keine Protokollzeile im Dienst,
// alle Sitzungen 'disconnected' OHNE Fehlereintrag. Drei Reparaturen (Pacht,
// Startmerkliste, Meldungstrennung) haben das Symptom nicht aufgeloest.
// Statt einen vierten Fix zu raten: messen, wo die Kette reisst.
// Ein Vorgang, der nichts protokolliert, ist von einem Vorgang, der nie
// stattfindet, nicht zu unterscheiden.
function spurBefehl(schritt, commandType, extra = {}) {
  try {
    console.info(`[browser][SPUR] ${schritt} type=${commandType} ${JSON.stringify(extra)}`);
  } catch { console.info(`[browser][SPUR] ${schritt} type=${commandType}`); }
}

async function requestBrowserControllerLease(env, state, operation, leaseId) {
  const commandType = `browser.${operation}`;
  const sessionId = state.latestSession?.id;
  if (!sessionId || !leaseId) {
    throw new Error('Browser-Sitzung und Steuerpacht werden benötigt.');
  }
  // Ein Weg: der Befehl (Handler browser.controller.* pflegt die Pacht in browser_sessions). Bis die
  // Projektion zurueckkommt, spiegelt die Oberflaeche die angeforderte Pacht lokal.
  const result = await dispatchBrowserCommand(env, state, commandType, { lease_id: leaseId });
  const released = operation === 'controller.release';
  const actorId = browserActorIds(env.actor)[0] || '';
  state.latestSession = {
    ...(state.latestSession || {}),
    controller_user_id: released ? '' : actorId,
    controller_lease_id: released ? '' : leaseId,
    controller_lease_expires_at_ms: released ? 0 : Date.now() + 120_000,
  };
  if (!released) setzeEingabeHinweis(env, state, '');
  return result;
}

// Befehl absenden, nie warten (R5). Der Status kommt gepusht ueber h.status; failed und lokal ungueltig
// gehen in die Zustandszone (Code browser.command.<typ>), done raeumt den Eintrag.
async function dispatchBrowserCommand(env, state, commandType, payloadPatch = {}, options = {}) {
  spurBefehl('1-aufgerufen', commandType, {
    sitzung: state.latestSession?.id || '-',
    status: state.latestSession?.status || '-',
    neueSitzung: payloadPatch.new_session === true,
  });
  const requiresController = browserCommandRequiresController(commandType, state.latestSession);
  if (requiresController && !browserSurfaceCanControl(env, state)) {
    spurBefehl('2-ABBRUCH-keine-steuerung', commandType, {
      grund: eingabeSperrgrund(env, state) || '(unbekannt)',
    });
    throw new Error('Dieses Browser-Fenster ist nicht aktiv. Aktivieren Sie das Fenster oder übernehmen Sie die Steuerung.');
  }
  spurBefehl('2-steuerung-ok', commandType, { brauchtSteuerung: requiresController });
  const now = Date.now();
  const opensNewSession = payloadPatch.new_session === true;
  const requestedSessionId = browserSessionIdFromArgs(payloadPatch);
  const sessionId = requestedSessionId || (opensNewSession
    ? `${userSessionPrefix(env.actor)}_${now}`
    : state.latestSession?.id || `${userSessionPrefix(env.actor)}_default`);
  const tabId = String(payloadPatch.tab_id || (opensNewSession
    ? `browser_tab_${now}`
    : state.latestTab?.id || state.latestSession?.current_tab_id || DEFAULT_TAB_ID));
  const payload = {
    session_id: sessionId,
    tab_id: tabId,
    viewport_w: VIEWPORT.width,
    viewport_h: VIEWPORT.height,
    ...payloadPatch,
  };
  if (requiresController) payload.lease_id = state.controllerLeaseId;
  delete payload.new_session;
  if (payload.url) payload.url = normalizeUrl(payload.url);
  console.info(`[browser] submitting command command_type=${commandType} session_id=${sessionId}`);
  const handle = submitCommand(env, commandType, {
    ...payload,
    record_id: sessionId,
    client_context: {
      source: 'business-os.browser.runtime',
      module_id: 'browser',
      actor: actorContext(env.actor),
    },
  }, options);
  spurBefehl('3-befehl-gebaut', commandType, { commandId: handle.id, sessionId, tabId });
  return { commandId: handle.id, sessionId, tabId, opensNewSession };
}

function submitCommand(env, commandType, payload, { onSettled } = {}) {
  const code = `browser.command.${commandType}`;
  // record_id zusätzlich als Befehls-Option (§16.19 Nr. 8, Stub 9fa9948): landet am Befehlsdokument.
  const handle = env.api.command(commandType, payload, payload?.record_id ? { record_id: payload.record_id } : undefined);
  let settled = false;
  const settle = () => {
    if (settled) return;
    settled = true;
    try { onSettled?.(); } catch {}
  };
  const off = handle.status.subscribe((status) => {
    if (status?.status === 'failed') {
      env.status.report({ level: 'error', code, text: `${t('commandFailedPrefix', 'Browser-Aktion fehlgeschlagen')} (${commandType}): ${status.error || '-'}`, ref: handle.id });
      settle();
    } else if (status?.status === 'done') {
      env.status.clear(code);
      settle();
    }
  });
  env.commandWatches.add(off);
  handle.local.catch((error) => {
    env.status.report({ level: 'error', code, text: `${t('commandFailedPrefix', 'Browser-Aktion fehlgeschlagen')} (${commandType}): ${String(error?.message || error?.code || error)}`, ref: handle.id });
    settle();
  });
  return handle;
}

function ensureRequestedBrowserSession(env, state, args = {}) {
  const request = browserAuthRequestFromArgs(args);
  if (!request) return false;
  state.selectedSessionId = request.session_id;
  state.requestedSessionId = request.session_id;
  // Die Merkliste verhindert, dass derselbe Start doppelt losgeschickt wird,
  // solange er noch laeuft. Sie darf aber nicht dauerhaft merken: eine Sitzung,
  // die einmal erfolgreich startete und spaeter wegbricht (Dienstneustart,
  // Prozessende), blieb bis 13.08.2026 fuer immer in der Liste — der Eintrag
  // wurde nur im FEHLERfall entfernt. Danach wurde jeder weitere Startversuch
  // stumm verworfen.
  // Deshalb zuerst den Zustand lesen, dann die Merkliste bewerten: was nicht
  // mehr laeuft, ist auch nicht mehr "in Arbeit".
  const existing = readDocument(env, 'browser_sessions', request.session_id);
  if (!browserSessionNeedsStart(existing)) {
    // Laeuft bereits — Merkliste aufraeumen, damit ein spaeteres Wegbrechen
    // wieder startbar ist.
    state.requestedSessionStarts.delete(request.session_id);
    return false;
  }
  if (state.requestedSessionStarts.has(request.session_id)) {
    console.info('[browser] Start bereits angefordert, warte auf Ergebnis', request.session_id);
    return false;
  }

  state.requestedSessionStarts.add(request.session_id);
  state.controllerLeaseId = newBrowserControllerLeaseId();
  dispatchBrowserCommand(env, state, 'browser.session.start', {
    ...request,
    lease_id: state.controllerLeaseId,
  }, {
    // Ende des Startbefehls (done/failed) gibt die Merkliste frei – ein spaeteres Wegbrechen bleibt startbar.
    onSettled: () => state.requestedSessionStarts.delete(request.session_id),
  }).catch((error) => {
    state.requestedSessionStarts.delete(request.session_id);
    env.notice?.(browserStartErrorMessage(error));
  });
  return true;
}

function browserAuthRequestFromArgs(value) {
  const sessionId = browserSessionIdFromArgs(value);
  const purpose = String(value?.purpose || '').trim();
  const targetUrl = String(value?.target_url || value?.targetUrl || '').trim();
  if (!sessionId || purpose !== 'web_stack_auth' || !targetUrl) return null;
  const tabId = String(value?.tab_id || value?.tabId || `browser_tab_${sessionId}`).trim();
  const sourceId = String(value?.source_id || value?.sourceId || '').trim();
  const allowedDomains = Array.isArray(value?.allowed_domains)
    ? value.allowed_domains.map((entry) => String(entry || '').trim()).filter(Boolean)
    : [];
  const captureScript = String(value?.capture_script || value?.captureScript || '').trim();
  const secretName = String(value?.secret_name || value?.required_secret_name || '').trim();
  const verifySelector = String(value?.verify_selector || value?.verifySelector || '').trim();
  const authAssistCommandId = String(value?.auth_assist_command_id || value?.command_id || '').trim();
  const authAssistTaskId = String(value?.auth_assist_task_id || value?.execution_task_id || '').trim();
  const requestingTaskId = String(value?.requesting_task_id || '').trim();
  const instruction = String(value?.instruction || '').trim();
  return {
    session_id: sessionId,
    tab_id: tabId,
    url: targetUrl,
    target_url: targetUrl,
    source_id: sourceId,
    purpose,
    allowed_domains: allowedDomains,
    capture_script: captureScript,
    verify_selector: verifySelector,
    secret_name: secretName,
    auth_assist_command_id: authAssistCommandId,
    auth_assist_task_id: authAssistTaskId,
    requesting_task_id: requestingTaskId,
    instruction,
    auth_assist_status: 'pending',
    profile_mode: 'persistent',
    secret_value_in_rxdb: false,
  };
}

function browserStartErrorMessage(error) {
  const code = String(error?.code || '').toLowerCase();
  if (code === 'auth_required') return 'Die Browser-Anmeldung benötigt eine neue Business-OS-Autorisierung.';
  const message = String(error?.message || '').trim();
  return message
    ? `Die Browser-Anmeldung konnte nicht geöffnet werden: ${message}`
    : 'Die Browser-Anmeldung konnte nicht geöffnet werden.';
}

function browserSessionError(session) {
  const direct = String(session?.last_error || session?.error || '').trim();
  if (direct) return direct;
  const status = String(session?.runtime_status || session?.status || '').toLowerCase();
  if (['active', 'running', 'capturing'].includes(status)) return '';
  return String(session?.payload?.last_error || session?.payload?.error || '').trim();
}

function browserSessionIsLive(session) {
  const status = String(session?.runtime_status || session?.status || '').toLowerCase();
  return status === 'active';
}

// Warum ein Start uebersprungen wird — im Klartext, nicht als Schweigen.
// Leerer Rueckgabewert heisst: starten ist erlaubt.
//
// Ein Start ist teuer: neue Sitzungskennung, neuer Chrome, neuer Eintrag in der
// Liste. Zwei Klicks duerfen daraus nicht zwei Browser machen.
// Die Sperre laeuft nach START_SPERRE_MS ab, damit ein haengengebliebener Start
// den Nutzer nicht dauerhaft aussperrt — genau der Fehler, den die
// Startmerkliste am 14.08.2026 hatte.
const START_SPERRE_MS = 8000;

function startSperrgrund(state, jetzt = Date.now()) {
  const seit = Number(state?.startPendingSince || 0);
  if (Number.isFinite(seit) && seit > 0 && jetzt - seit < START_SPERRE_MS) {
    return 'Eine Sitzung wird bereits gestartet …';
  }
  // Laeuft die angeforderte Sitzung schon oder faehrt sie gerade hoch, ist ein
  // zweiter Start reine Verschwendung.
  const angefordert = String(state?.requestedSessionId || '');
  const sitzung = state?.latestSession;
  if (angefordert && sitzung?.id === angefordert && !browserSessionNeedsStart(sitzung)) {
    return 'Diese Sitzung ist bereits aktiv.';
  }
  return '';
}

// Was die Adressleiste tun muss: navigieren oder eine neue Sitzung starten.
// Beide Bedingungen sind noetig. Die Steuerung allein genuegt nicht — eine
// gueltige Pacht auf einer toten Sitzung ist genau die Falle, in der die
// Kundeninstanz vom 13.08. bis 15.08.2026 sass.
function browserAddressAction(session, canControl) {
  return browserSessionIsLive(session) && canControl ? 'navigate' : 'start';
}

function browserSessionNeedsStart(session) {
  if (!session?.id) return true;
  const status = String(session.runtime_status || session.status || '').toLowerCase();
  return !['active', 'starting'].includes(status);
}

function browserStartErrorIsRetryable(error) {
  const code = String(error?.code || '').toLowerCase();
  return ['projection_delayed', 'sync_unavailable', 'native_unavailable'].includes(code);
}

function userSessionPrefix(actor) {
  const raw = String(actor?.user?.id || actor?.id || actor?.userId || 'browser-user');
  const safe = raw.toLowerCase().replace(/[^a-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 64);
  return `browser_session_${safe || 'user'}`;
}

function selectedViewport(select) {
  const [width, height] = String(select?.value || '1280x720').split('x').map(Number);
  return {
    width: Number.isFinite(width) ? Math.max(320, Math.min(3840, width)) : VIEWPORT.width,
    height: Number.isFinite(height) ? Math.max(240, Math.min(2160, height)) : VIEWPORT.height,
  };
}

async function sendBrowserContextToCtox(env, state, options = {}) {
  const session = state.latestSession;
  const tab = state.latestTab;
  const frame = state.latestFrame;
  if (!session?.id) {
    state.notice = 'Kein Browser-Fenster zum Senden geoeffnet.';
    return;
  }
  const payloadMetadata = session.payload || {};
  const now = Date.now();
  const url = tab?.url || session.current_url || '';
  const title = browserDisplayTitle(tab, session, url);
  const sourceId = payloadMetadata.source_id || '';
  const captureScript = payloadMetadata.capture_script || '';
  const verifySelector = payloadMetadata.verify_selector || '';
  const purpose = payloadMetadata.purpose || '';
  const sourceModule = options.webStack ? 'web_stack' : 'browser';
  const browserContext = {
    session_id: session.id,
    tab_id: tab?.id || session.current_tab_id || '',
    url,
    title,
    status: session.runtime_status || session.status || '',
    purpose,
    source_id: sourceId,
    capture_script: captureScript,
    verify_selector: verifySelector,
    frame_id: frame?.id || '',
    frame_seq: frame?.seq || 0,
    frame_captured_at_ms: frame?.captured_at_ms || 0,
    frame_expires_at_ms: frame?.expires_at_ms || 0,
    frame_mime_type: frame?.mime_type || '',
    frame_width: frame?.width || 0,
    frame_height: frame?.height || 0,
    frame_size_bytes: frame?.size_bytes || 0,
    frame_hash: frame?.frame_hash || '',
    frame_data_in_payload: false,
  };
  const payload = {
    title: options.webStack && sourceId ? `Web Stack Browser: ${sourceId}` : `Browser: ${title}`,
    instruction: url
      ? `Use this browser context from ${url}. The screenshot is available as the referenced browser frame record.`
      // v4-allow: text – Anweisung an den Agenten (Befehls-Payload), kein sichtbarer Text
      : 'Use this browser context. The screenshot is available as the referenced browser frame record.',
    source_module: sourceModule,
    // SKL-A4: keine required_skills mehr – `browser-context`, `web-stack`, `ctox` gab es nie als Skill (weder
    // src/skills/** noch App-Paket noch Git-Historie); der Core reichte die Namen nur als Text in den Prompt.
    browser_context: browserContext,
    source_id: sourceId,
    capture_script: captureScript,
    secret_value_in_payload: false,
  };
  submitCommand(env, 'ctox.browser_context.capture', {
    ...payload,
    record_id: session.id,
    client_context: {
      source: options.webStack ? 'business-os.browser.web-stack-capture' : 'business-os.browser.context',
      module_id: 'browser',
      actor: actorContext(env.actor),
      browser_context: browserContext,
    },
  });
}

async function extractWebStackFields(env, state) {
  const session = state.latestSession;
  const tab = state.latestTab;
  const frame = state.latestFrame;
  const payload = session?.payload || {};
  if (!session?.id || !payload.capture_script) {
    state.notice = 'Keine Web-Stack-Uebergabe fuer dieses Browser-Fenster verfuegbar.';
    return;
  }
  const now = Date.now();
  const browserContext = {
    session_id: session.id,
    tab_id: tab?.id || session.current_tab_id || '',
    url: tab?.url || session.current_url || '',
    title: browserDisplayTitle(tab, session, tab?.url || session.current_url || ''),
    status: session.runtime_status || session.status || '',
    purpose: payload.purpose || '',
    source_id: payload.source_id || '',
    capture_script: payload.capture_script || '',
    verify_selector: payload.verify_selector || '',
    frame_id: frame?.id || '',
    frame_seq: frame?.seq || 0,
    frame_captured_at_ms: frame?.captured_at_ms || 0,
    frame_expires_at_ms: frame?.expires_at_ms || 0,
    frame_mime_type: frame?.mime_type || '',
    frame_width: frame?.width || 0,
    frame_height: frame?.height || 0,
    frame_size_bytes: frame?.size_bytes || 0,
    frame_hash: frame?.frame_hash || '',
    frame_data_in_payload: false,
  };
  const artifact = {
    kind: 'browser_context',
    schema_version: 1,
    stream: 'rxdb',
    source_module: 'web_stack',
    source_id: payload.source_id || '',
    capture_script: payload.capture_script || '',
    browser_context: browserContext,
    sensitivity: 'browser_context_reference',
    secret_value_in_payload: false,
    frame_data_in_payload: false,
  };
  submitCommand(env, 'browser.capture.extract', {
    session_id: session.id,
    source_id: payload.source_id || '',
    capture_script: payload.capture_script || '',
    frame_id: frame?.id || '',
    browser_context_artifact: artifact,
    secret_value_in_payload: false,
    frame_data_in_payload: false,
    record_id: session.id,
    client_context: {
      source: 'business-os.browser.web-stack-extract',
      module_id: 'browser',
      actor: actorContext(env.actor),
    },
  });
}

async function completeWebStackAuthAssist(env, state) {
  const session = state.latestSession;
  if (!session?.id) {
    state.notice = 'Kein Web-Stack-Browserfenster geoeffnet.';
    return;
  }
  if (session.payload?.purpose !== 'web_stack_auth') {
    state.notice = 'Dieses Browserfenster gehoert nicht zu einer Web-Stack-Anmeldung.';
    return;
  }
  const now = Date.now();
  const payload = {
    session_id: session.id,
    tab_id: state.latestTab?.id || session.current_tab_id || '',
    source_id: session.payload?.source_id || '',
    secret_name: session.payload?.secret_name || '',
    lease_id: state.controllerLeaseId,
    confirmed: true,
    auth_assist_command_id: session.payload?.auth_assist_command_id || '',
    auth_assist_task_id: session.payload?.auth_assist_task_id || '',
    requesting_task_id: session.payload?.requesting_task_id || '',
    completed_at_ms: now,
    browser_stream: 'rxdb',
    secret_value_in_rxdb: false,
  };
  submitCommand(env, 'web_stack.auth_assist.complete', {
    ...payload,
    record_id: session.id,
    client_context: {
      source: 'business-os.browser.auth-assist',
      module_id: 'browser',
      actor: actorContext(env.actor),
    },
  });
}

async function fillWebStackCredential(env, state) {
  const session = state.latestSession;
  if (!session?.id) {
    state.notice = 'Kein Web-Stack-Browserfenster geoeffnet.';
    return;
  }
  const secretName = session.payload?.secret_name || '';
  if (!secretName) {
    state.notice = 'Fuer diese Quelle ist kein Zugang im CTOX Secret Store hinterlegt.';
    return;
  }
  const now = Date.now();
  const payload = {
    session_id: session.id,
    tab_id: state.latestTab?.id || session.current_tab_id || '',
    lease_id: state.controllerLeaseId,
    source_id: session.payload?.source_id || '',
    secret_scope: 'credentials',
    secret_name: secretName,
    field_role: session.payload?.credential_field_role || 'both',
    confirmed: true,
    browser_stream: 'rxdb',
    secret_value_in_rxdb: false,
  };
  const selector = String(session.payload?.credential_selector || session.payload?.selector || '').trim();
  if (selector) payload.selector = selector;
  submitCommand(env, 'browser.credential.fill', {
    ...payload,
    record_id: session.id,
    client_context: {
      source: 'business-os.browser.credential-fill',
      module_id: 'browser',
      actor: actorContext(env.actor),
    },
  });
}

function actorContext(actor) {
  const user = actor?.user || actor || {};
  return {
    user_id: user.id || actor?.userId || '',
    display_name: user.display_name || user.name || '',
    role: user.role || '',
  };
}

// custom: Tab-Leiste der fernen Sitzung.
//
// Der Runner fuehrt seit jeher mehrere Tabs (tab_open/tab_activate/tab_close),
// sichtbar waren sie in der Oberflaeche aber nur als Zaehler "0 Tabs" -- und
// der Knopf "Neuer Tab" schrieb einen Befehl, den niemand ausfuehrte.
//
// Bei einem einzigen Tab bleibt die Leiste verborgen: eine Leiste mit genau
// einem Eintrag kostet Hoehe und sagt nichts.
function tabsDerSitzung(state) {
  // Zuerst die Live-Liste des Runners: sie beschreibt, was der ferne Browser
  // WIRKLICH offen hat, und braucht keine Replikation.
  if (Array.isArray(state.liveTabs) && state.liveTabs.length) {
    return state.liveTabs
      .map((tab) => ({
        id: String(tab.id || tab.tab_id || ''),
        title: tab.title || '',
        url: tab.url || '',
        // Der Runner setzt `active` aus dem tatsaechlich gewaehlten Page-Objekt
        // -- im Gegensatz zur Projektion, die jeden angefassten Tab als aktiv
        // schreibt.
        liveAktiv: tab.active === true,
      }))
      .filter((tab) => tab.id);
  }
  const sessionId = state.latestSession?.id;
  if (!sessionId) return [];
  return (Array.isArray(state.tabs) ? state.tabs : [])
    .filter((tab) => tab.session_id === sessionId && tab.status !== 'closed')
    .sort((a, b) => Number(a.position || 0) - Number(b.position || 0)
      || String(a.id).localeCompare(String(b.id)));
}

// Ein Tab-Befehl endet wie jede andere Aktion: auffrischen, und ein
// Fehlschlag landet im Hinweisband statt in einer stillen Konsolenzeile --
// "keine Steuerung" ist der haeufigste Fall und muss erklaert werden.
function tabBefehl(env, state, commandType, tabId) {
  return dispatchBrowserCommand(env, state, commandType, { tab_id: tabId })
    .then(() => state.refresh?.())
    .catch((error) => {
      state.notice = `Der Vorgang konnte nicht gestartet werden: ${error?.message || error}`;
      state.refresh?.();
    });
}

function renderTabstrip(env, refs, state) {
  const leiste = refs.tabstrip;
  if (!leiste) return;
  const tabs = tabsDerSitzung(state);
  if (tabs.length < 2) {
    leiste.hidden = true;
    leiste.replaceChildren();
    return;
  }
  // Welcher Tab aktiv ist, sagt die Sitzung -- NICHT das Feld `active` am Tab:
  // upsert_browser_tab schreibt dort hart `true` fuer jeden angefassten Tab und
  // setzt die uebrigen nicht zurueck, sodass jeder je benutzte Tab als aktiv
  // gilt. `current_tab_id` wird dagegen bei jedem Befehl mitgefuehrt.
  let aktiverTab = String(tabs.find((tab) => tab.liveAktiv)?.id
    || state.liveActiveTabId
    || state.latestSession?.current_tab_id
    || '');
  // Nach dem Schliessen zeigt current_tab_id noch auf den geschlossenen Tab --
  // der Runner ist dann laengst auf einen anderen gewechselt. Ohne diesen
  // Rueckfall waere kurzzeitig gar kein Tab hervorgehoben.
  if (!tabs.some((tab) => tab.id === aktiverTab)) aktiverTab = tabs[0].id;
  // Nur neu bauen, wenn sich wirklich etwas geaendert hat -- sonst verliert
  // ein Klick waehrend eines Renderdurchlaufs sein Ziel.
  const signatur = tabs.map((tab) => `${tab.id}:${tab.id === aktiverTab ? 1 : 0}:${tab.title || tab.url || ''}`).join('|');
  if (leiste.dataset.signatur === signatur) return;
  leiste.dataset.signatur = signatur;
  leiste.hidden = false;
  leiste.replaceChildren(...tabs.map((tab) => {
    const istAktiv = tab.id === aktiverTab;
    const eintrag = document.createElement('div');
    eintrag.className = 'browser-tab' + (istAktiv ? ' is-active' : '');
    const knopf = document.createElement('button');
    knopf.type = 'button';
    knopf.className = 'browser-tab-title';
    knopf.setAttribute('role', 'tab');
    knopf.setAttribute('aria-selected', istAktiv ? 'true' : 'false');
    const beschriftung = String(tab.title || tab.url || tab.id).trim();
    knopf.textContent = beschriftung;
    knopf.title = String(tab.url || beschriftung);
    knopf.addEventListener('click', () => {
      if (istAktiv) return;
      tabBefehl(env, state, 'browser.tab.activate', tab.id);
    });
    const schliessen = document.createElement('button');
    schliessen.type = 'button';
    schliessen.className = 'browser-tab-close';
    schliessen.setAttribute('aria-label', tBrowser(env, 'tabClose', 'Tab schließen'));
    schliessen.textContent = '\u00d7';
    schliessen.addEventListener('click', (event) => {
      event.stopPropagation();
      tabBefehl(env, state, 'browser.tab.close', tab.id);
    });
    eintrag.append(knopf, schliessen);
    return eintrag;
  }));
}

// custom: Eigenes Kontextmenue ueber der Buehne.
//
// Ohne preventDefault oeffnet der AEUSSERE Browser sein eigenes Menue ueber
// dem Remote-Bild. Dessen Eintraege ("Zurueck", "Neu laden") wirken dann auf
// die Shell statt auf die ferne Seite -- also genau falsch herum.
//
// Die Eintraege loesen die vorhandenen Knoepfe aus, statt die Befehle noch
// einmal zu bauen: Steuerungspflicht, Sperren und Fehlermeldungen existieren
// damit weiterhin nur an einer Stelle, und ein gesperrter Knopf graut den
// Menueeintrag automatisch mit aus.
function installContextMenu(env, refs) {
  const menu = refs.contextMenu;
  if (!menu || !refs.canvas) return;
  const ziele = {
    back: () => refs.back,
    forward: () => refs.forward,
    reload: () => refs.reload,
    copy: () => refs.clipboardCopy,
    paste: () => refs.clipboardPaste,
  };
  const schliessen = () => {
    menu.hidden = true;
  };
  refs.canvas.addEventListener('contextmenu', (event) => {
    event.preventDefault();
    const shell = menu.offsetParent || refs.canvas.parentElement;
    const box = shell?.getBoundingClientRect();
    if (!box) return;
    for (const eintrag of menu.querySelectorAll('[data-browser-context-action]')) {
      const knopf = ziele[eintrag.dataset.browserContextAction]?.();
      eintrag.disabled = !knopf || knopf.disabled;
    }
    menu.hidden = false;
    // Innerhalb der Buehne halten, damit das Menue am rechten oder unteren
    // Rand nicht aus dem Fenster laeuft.
    const breite = menu.offsetWidth || 160;
    const hoehe = menu.offsetHeight || 200;
    const x = Math.min(Math.max(0, event.clientX - box.left), Math.max(0, box.width - breite));
    const y = Math.min(Math.max(0, event.clientY - box.top), Math.max(0, box.height - hoehe));
    menu.style.left = `${Math.round(x)}px`;
    menu.style.top = `${Math.round(y)}px`;
  });
  menu.addEventListener('click', (event) => {
    const eintrag = event.target.closest?.('[data-browser-context-action]');
    if (!eintrag || eintrag.disabled) return;
    schliessen();
    ziele[eintrag.dataset.browserContextAction]?.()?.click();
  });
  // Jeder Weg aus dem Menue heraus schliesst es -- auch der Klick auf die
  // Buehne, der sonst als Eingabe an die ferne Seite ginge.
  refs.canvas.addEventListener('pointerdown', schliessen);
  // Escape muss das Menue schliessen, egal wo der Fokus gerade steht -- beim
  // Oeffnen per Maus liegt er nicht im Menue, und ein Menue, das offen bleibt,
  // verdeckt die Buehne.
  const aufEscape = (event) => {
    if (event.key !== 'Escape' || menu.hidden) return;
    event.stopPropagation();
    schliessen();
    refs.canvas.focus();
  };
  menu.addEventListener('keydown', aufEscape);
  document.addEventListener('keydown', aufEscape, true);
  env?.cleanups?.push(() => document.removeEventListener('keydown', aufEscape, true));
}

// Sitzungsleiste einklappbar: auf schmalen Fenstern und im Freischaltmodus
// braucht die Buehne jede Spalte. Zustand ueberlebt den Remount ueber snapshot()/restored (R13).
function installRailToggle(env, refs, collapsed = false) {
  const root = refs.railToggle?.closest('[data-browser-root]');
  if (!root || !refs.railToggle) return;
  const anwenden = (zu) => {
    root.classList.toggle('is-rail-collapsed', zu);
    refs.railToggle.setAttribute('aria-pressed', zu ? 'true' : 'false');
  };
  if (collapsed) anwenden(true);
  refs.railToggle.addEventListener('click', () => {
    anwenden(!root.classList.contains('is-rail-collapsed'));
  });
}

// Praezise Eingabe: Klickpunkt waehlen, lokal tippen/pasten, am Punkt
// ausfuehren. Umgeht die Latenz des fernen Eingabekanals fuer Formulare.
function installPreciseInput(env, refs, state) {
  if (!refs.preciseToggle || !refs.preciseLayer || !refs.shell || !refs.canvas) return;
  const root = refs.preciseToggle.closest('[data-browser-root]');
  const setHint = (text) => { if (refs.preciseHint) refs.preciseHint.textContent = text || ''; };
  const layerZu = () => {
    refs.preciseLayer.hidden = true;
    setHint('');
  };
  const modusSetzen = (an) => {
    state.preciseEnabled = an;
    refs.preciseToggle.setAttribute('aria-pressed', an ? 'true' : 'false');
    root?.classList.toggle('is-precise', an);
    if (!an) layerZu();
  };
  refs.preciseToggle.addEventListener('click', () => modusSetzen(!state.preciseEnabled));
  if (state.preciseEnabled) modusSetzen(true);
  // Capture-Phase auf dem Buehnen-Container: faengt den Klick VOR den
  // normalen Canvas-Handlern ab, solange der Modus aktiv ist.
  refs.shell.addEventListener('pointerdown', (event) => {
    if (!state.preciseEnabled) return;
    if (refs.precisePanel?.contains(event.target)) return;
    if (event.target !== refs.canvas) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    const punkt = canvasPoint(refs.canvas, event);
    state.precisePoint = punkt;
    const shellBox = refs.shell.getBoundingClientRect();
    const lokalX = event.clientX - shellBox.left;
    const lokalY = event.clientY - shellBox.top;
    refs.preciseLayer.hidden = false;
    if (refs.preciseMark) {
      refs.preciseMark.style.left = `${Math.round(lokalX)}px`;
      refs.preciseMark.style.top = `${Math.round(lokalY)}px`;
    }
    if (refs.precisePanel) {
      const panelBreite = 340;
      const links = Math.max(8, Math.min(lokalX + 14, Math.max(8, shellBox.width - panelBreite - 8)));
      const oben = Math.max(8, Math.min(lokalY + 14, Math.max(8, shellBox.height - 170)));
      refs.precisePanel.style.left = `${Math.round(links)}px`;
      refs.precisePanel.style.top = `${Math.round(oben)}px`;
    }
    setHint(`Punkt ${punkt.x}×${punkt.y} gewählt`);
    refs.preciseText?.focus();
  }, true);
  const fernerKlick = async (button, anzahl = 1) => {
    const punkt = state.precisePoint;
    if (!punkt) return;
    for (let i = 0; i < anzahl; i += 1) {
      await writeInputEvent(env, state, 'click', preciseClickPatch(env, refs, punkt, button));
    }
  };
  refs.precisePanel?.addEventListener('click', async (event) => {
    const knopf = event.target.closest?.('[data-browser-precise-act]');
    if (!knopf) return;
    const akt = knopf.dataset.browserPreciseAct;
    if (akt === 'close') { layerZu(); return; }
    try {
      if (akt === 'click') { await fernerKlick('left'); setHint('Klick gesendet'); }
      if (akt === 'dblclick') { await fernerKlick('left', 2); setHint('Doppelklick gesendet'); }
      if (akt === 'rightclick') { await fernerKlick('right'); setHint('Rechtsklick gesendet'); }
      if (akt === 'type' || akt === 'type-enter') {
        const text = String(refs.preciseText?.value || '');
        await fernerKlick('left');
        await new Promise((r) => setTimeout(r, 160));
        if (text) await writeInputEvent(env, state, 'keyDown', preciseTextPatch(env, text));
        if (akt === 'type-enter') {
          await new Promise((r) => setTimeout(r, 120));
          await writeInputEvent(env, state, 'keyDown', preciseKeyPatch(env, 'Enter'));
        }
        setHint(text ? 'Text am Punkt eingefügt' : 'Klick gesendet (kein Text eingegeben)');
      }
    } catch (error) {
      // Textdiät Regel 1: Fehler in die Zone statt Hinweiszeile im Inhalt.
      env.status.report({ level: 'error', code: 'browser.precise.failed', text: `Eingabe fehlgeschlagen: ${error?.message || error}` });
    }
  });
  refs.preciseText?.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') { layerZu(); refs.canvas?.focus(); }
    event.stopPropagation();
  });
}

function preciseClickPatch(env, refs, punkt, button) {
  return {
    x: punkt.x, y: punkt.y, detail: 1, clickCount: 1,
    button, buttons: button === 'right' ? 2 : 1, dx: 0, dy: 0,
    key: '', code: '', modifiers: [], text: '',
    payload: {
      pointer_id: 0, pointer_type: 'precise', click_count: 1,
      viewport_w: refs.canvas?.width || VIEWPORT.width,
      viewport_h: refs.canvas?.height || VIEWPORT.height,
      actor: actorContext(env.actor),
    },
  };
}

function preciseTextPatch(env, text) {
  return {
    x: 0, y: 0, detail: 0, clickCount: 0, button: 'none', buttons: 0,
    dx: 0, dy: 0, key: '', code: '', modifiers: [], text,
    payload: { repeat: false, location: 0, actor: actorContext(env.actor) },
  };
}

function preciseKeyPatch(env, key) {
  return {
    x: 0, y: 0, detail: 0, clickCount: 0, button: 'none', buttons: 0,
    dx: 0, dy: 0, key, code: key, modifiers: [], text: '',
    payload: { repeat: false, location: 0, actor: actorContext(env.actor) },
  };
}

// Einfuegen aus der Zwischenablage direkt auf der Buehne: das paste-Ereignis
// traegt den Text lokal; er geht als EIN keyboard.type an die ferne Seite.
function installCanvasPaste(env, refs, state) {
  refs.canvas?.addEventListener('paste', (event) => {
    const text = event.clipboardData?.getData('text/plain') || '';
    if (!text) return;
    event.preventDefault();
    writeInputEvent(env, state, 'keyDown', preciseTextPatch(env, text));
  });
}

function installInputHandlers(env, refs, state, scheduleRefresh) {
  const afterInput = () => scheduleRefresh();
  refs.canvas?.addEventListener('pointerdown', (event) => {
    event.preventDefault();
    refs.canvas.focus();
    refs.canvas.setPointerCapture?.(event.pointerId);
    state.pointerIsDown = true;
    writePointerInput(env, refs, state, 'mouseDown', event).then(afterInput);
  });
  refs.canvas?.addEventListener('pointerup', (event) => {
    event.preventDefault();
    state.pointerIsDown = false;
    writePointerInput(env, refs, state, 'mouseUp', event).then(afterInput);
  });
  refs.canvas?.addEventListener('pointercancel', (event) => {
    state.pointerIsDown = false;
    writePointerInput(env, refs, state, 'mouseUp', event).then(afterInput);
  });
  refs.canvas?.addEventListener('pointermove', (event) => {
    const now = Date.now();
    const dragging = state.pointerIsDown || Number(event.buttons || 0) > 0;
    if (!dragging && now - Number(state.lastPointerMoveAt || 0) < POINTER_HOVER_THROTTLE_MS) return;
    state.lastPointerMoveAt = now;
    writePointerInput(env, refs, state, 'mouseMove', event).then(afterInput);
  });
  refs.canvas?.addEventListener('wheel', (event) => {
    event.preventDefault();
    writePointerInput(env, refs, state, 'wheel', event).then(afterInput);
  }, { passive: false });
  refs.canvas?.addEventListener('keydown', (event) => {
    // Cmd/Ctrl+V muss durch: preventDefault hier wuergte das paste-Ereignis
    // ab, bevor es entstand — deshalb kam ⌘V nie an. installCanvasPaste
    // schickt den Zwischenablage-Text dann als Ganzes.
    if ((event.metaKey || event.ctrlKey) && String(event.key).toLowerCase() === 'v') return;
    event.preventDefault();
    writeKeyboardInput(env, state, 'keyDown', event).then(afterInput);
  });
  refs.canvas?.addEventListener('keyup', (event) => {
    event.preventDefault();
    writeKeyboardInput(env, state, 'keyUp', event).then(afterInput);
  });
  installContextMenu(env, refs);
}

async function writePointerInput(env, refs, state, type, event, extra = {}) {
  const point = canvasPoint(refs.canvas, event);
  const clickCount = type === 'wheel' ? 0 : pointerClickCount(state, type, event, point);
  const buttons = pointerButtons(event, type);
  const patch = {
    x: point.x,
    y: point.y,
    detail: clickCount,
    clickCount,
    button: pointerButton(event.button),
    buttons,
    dx: type === 'wheel' ? Number(event.deltaX || 0) : 0,
    dy: type === 'wheel' ? Number(event.deltaY || 0) : 0,
    key: '',
    code: '',
    modifiers: eventModifiers(event),
    text: '',
    payload: {
      pointer_id: event.pointerId || 0,
      pointer_type: event.pointerType || 'mouse',
      click_count: clickCount,
      viewport_w: refs.canvas?.width || VIEWPORT.width,
      viewport_h: refs.canvas?.height || VIEWPORT.height,
      actor: actorContext(env.actor),
    },
    ...extra,
  };
  await writeInputEvent(env, state, type, patch);
}

async function writeKeyboardInput(env, state, type, event) {
  const key = event.key || '';
  const code = event.code || '';
  const modifiers = eventModifiers(event);
  await writeInputEvent(env, state, type, {
    x: 0,
    y: 0,
    detail: 0,
    clickCount: 0,
    button: 'none',
    buttons: 0,
    dx: 0,
    dy: 0,
    key,
    code,
    modifiers,
    text: keyboardText(type, event),
    payload: {
      repeat: Boolean(event.repeat),
      location: Number(event.location || 0),
      actor: actorContext(env.actor),
    },
  });
}

async function writeInputEvent(env, state, type, patch) {
  // Beide Abbruchgruende waren bis 13.08.2026 STUMM: kein Protokoll, keine
  // Meldung, kein Hinweis in der Oberflaeche. Auf der Kundeninstanz gemessen:
  // null Eingabe-Ereignisse in zehn Minuten aktiver Bedienung, waehrend beide
  // Sitzungen "active" meldeten und die Oberflaeche "Bereit" zeigte. Der
  // Nutzer sah einen laufenden Browser, den niemand bedienen durfte — und
  // nichts sagte ihm warum. Ein Abbruch ohne Grund ist von einem Absturz
  // nicht zu unterscheiden.
  if (!browserSurfaceCanControl(env, state)) {
    meldeEingabeGesperrt(env, state, type);
    // Ein Klick IN dieses Fenster ist die ausdrueckliche Ansage, hier steuern
    // zu wollen. Haelt ein totes Fenster desselben Nutzers noch die Pacht,
    // wird sie jetzt uebernommen — sonst bliebe die Sitzung unbedienbar, bis
    // die Pacht von allein ablaeuft.
    if (type === 'mouseDown' || type === 'keyDown') {
      try { state.steuerungZurueckholen?.({ uebernahmeDurchInteraktion: true }); } catch {}
    }
    return;
  }
  const session = state.latestSession;
  const frame = state.latestFrame;
  const sessionId = session?.id || frame?.session_id;
  if (!sessionId) {
    // Faellt der Bildstrom aus, gibt es kein latestFrame. Ohne latestSession
    // bricht es dann hier ab — der Bildstromdefekt blockiert die Eingabe mit.
    console.warn('[browser] Eingabe verworfen: keine Sitzung zuordenbar', { type });
    setzeEingabeHinweis(env, state, 'Keine Sitzung zugeordnet. Bitte die Sitzung neu starten.');
    return;
  }
  const now = Date.now();
  const seq = Math.max(now, Number(state.lastInputSeq || 0) + 1);
  state.lastInputSeq = seq;
  const event = {
    id: `${sessionId}:input:${seq}:${type}`,
    owner_user_id: session?.owner_user_id || '',
    controller_user_id: session?.controller_user_id || browserActorIds(env.actor)[0] || '',
    session_id: sessionId,
    tab_id: state.latestTab?.id || frame?.tab_id || '',
    seq,
    client_seq: seq,
    frame_seq: Number(frame?.seq || session?.last_frame_seq || 0),
    lease_id: state.controllerLeaseId || '',
    ack_status: 'pending',
    type,
    status: 'pending',
    created_at_ms: now,
    updated_at_ms: now,
    ...patch,
  };
  console.info('[browser] input sent', browserInputTrace(event));
  // Eingabe als Dokument der eigenen Collection (v3-Replikationsweg). Die Echtzeit-Spur fuer Eingabe und
  // Bild ist im Kern offen (V4-PORT.md OFFEN-1); hier keine eigene Warteschlange und kein Abfragetakt.
  const handle = env.api.put('browser_input_events', event);
  await handle.local.catch((error) => {
    env.status.report({ level: 'error', code: 'browser.input.invalid', text: `${t('inputRejected', 'Eingabe abgelehnt')}: ${String(error?.message || error?.code || error)}` });
  });
}

function browserInputTrace(event) {
  const clickCount = Number(event.clickCount || event.detail || event.payload?.click_count || 0);
  return {
    session_id: event.session_id || '',
    tab_id: event.tab_id || '',
    seq: Number(event.seq || 0),
    type: event.type || '',
    x: Number(event.x || 0),
    y: Number(event.y || 0),
    detail: clickCount,
    clickCount,
    button: event.button || '',
    buttons: Number(event.buttons || 0),
    dx: Number(event.dx || 0),
    dy: Number(event.dy || 0),
    modifiers: Array.isArray(event.modifiers) ? event.modifiers : [],
    key: event.key || '',
    code: event.code || '',
    text: event.text || '',
  };
}

function installAddressClearControl(form, address, go) {
  if (!form || !address) return null;
  const existing = form.querySelector?.('[data-browser-address-clear]');
  if (existing) return existing;
  const button = globalThis.document?.createElement?.('button');
  if (!button) return null;
  button.type = 'button';
  button.className = 'ctox-icon-button';
  button.dataset.browserAddressClear = '';
  button.setAttribute('aria-label', 'Adresse löschen');
  button.title = 'Adresse löschen';
  button.textContent = '×';
  form.insertBefore(button, go || null);
  return button;
}

function canvasPoint(canvas, event) {
  const rect = canvas.getBoundingClientRect();
  const sourceWidth = Math.max(1, Number(canvas.width || VIEWPORT.width));
  const sourceHeight = Math.max(1, Number(canvas.height || VIEWPORT.height));
  const fitScale = Math.min(
    Math.max(1, rect.width) / sourceWidth,
    Math.max(1, rect.height) / sourceHeight,
  );
  const renderedWidth = sourceWidth * fitScale;
  const renderedHeight = sourceHeight * fitScale;
  const contentLeft = rect.left + (rect.width - renderedWidth) / 2;
  const contentTop = rect.top + (rect.height - renderedHeight) / 2;
  return {
    x: Math.max(0, Math.min(sourceWidth, Math.round((event.clientX - contentLeft) / fitScale))),
    y: Math.max(0, Math.min(sourceHeight, Math.round((event.clientY - contentTop) / fitScale))),
  };
}

function eventModifiers(event) {
  return [
    event.altKey ? 'Alt' : '',
    event.ctrlKey ? 'Control' : '',
    event.metaKey ? 'Meta' : '',
    event.shiftKey ? 'Shift' : '',
  ].filter(Boolean);
}

function pointerButton(button) {
  if (button === 1) return 'middle';
  if (button === 2) return 'right';
  if (button === -1 || button == null) return 'none';
  return 'left';
}

function pointerButtons(event, type) {
  const reported = Number(event.buttons || 0);
  if (type === 'mouseDown' && reported === 0) {
    if (event.button === 1) return 4;
    if (event.button === 2) return 2;
    return 1;
  }
  if (type === 'mouseUp') return reported;
  return reported;
}

function pointerClickCount(state, type, event, point) {
  const fromDom = Number(event.detail || 0);
  if (fromDom > 0) {
    if (type === 'mouseDown' || type === 'mouseUp') {
      state.lastPointerClick = {
        count: fromDom,
        x: point.x,
        y: point.y,
        at: Date.now(),
        button: Number(event.button || 0),
      };
    }
    return fromDom;
  }
  if (type === 'mouseMove') return Number(state.lastPointerClick?.count || 0);
  if (type !== 'mouseDown' && type !== 'mouseUp') return 0;

  const now = Date.now();
  const previous = state.lastPointerClick;
  const sameButton = previous && Number(previous.button || 0) === Number(event.button || 0);
  const closeEnough = previous
    && Math.abs(previous.x - point.x) <= POINTER_CLICK_SLOP_PX
    && Math.abs(previous.y - point.y) <= POINTER_CLICK_SLOP_PX
    && now - Number(previous.at || 0) <= POINTER_CLICK_INTERVAL_MS;
  const nextCount = type === 'mouseDown'
    ? (sameButton && closeEnough ? Number(previous.count || 0) + 1 : 1)
    : (sameButton && closeEnough ? Number(previous.count || 1) : 1);
  state.lastPointerClick = {
    count: nextCount,
    x: point.x,
    y: point.y,
    at: now,
    button: Number(event.button || 0),
  };
  return nextCount;
}

function keyboardText(type, event) {
  if (type !== 'keyDown') return '';
  const key = event.key || '';
  if (!key || key.length !== 1) return '';
  // Option (macOS) und AltGr (Windows, meldet ctrl+alt) KOMPONIEREN Zeichen:
  // @ ist auf deutschem Mac-Layout Option+L. Solche Tasten muessen als TEXT
  // zur fernen Seite, nicht als Kombination — "Alt+@" drueckt dort nichts.
  const altGr = event.ctrlKey && event.altKey;
  if ((event.ctrlKey || event.metaKey) && !altGr) return '';
  return key;
}

function browserInputPayload(event) {
  return {
    type: event.type || '',
    x: Number(event.x || 0),
    y: Number(event.y || 0),
    detail: Number(event.detail || event.clickCount || 0),
    clickCount: Number(event.clickCount || event.detail || 0),
    button: event.button || 'left',
    buttons: Number(event.buttons || 0),
    dx: Number(event.dx || 0),
    dy: Number(event.dy || 0),
    key: event.key || '',
    code: event.code || '',
    modifiers: Array.isArray(event.modifiers) ? event.modifiers : [],
    text: event.text || '',
  };
}

// Lokale Live-Abfrage (R3): sofortiges Ergebnis, kein Netz. Sortierung wie bisher (updated_at_ms desc).
function readCollection(env, name, options = {}) {
  if (!env?.api) return [];
  const query = {
    selector: options.selector || {},
    sort: options.sort || [{ updated_at_ms: 'desc' }],
  };
  if (Number.isFinite(options.limit)) query.limit = options.limit;
  try {
    return env.api.query(name, query).get().filter((doc) => doc && doc._deleted !== true);
  } catch (error) {
    console.warn(`[browser] ${name} query failed`, error);
    return [];
  }
}

function readDocument(env, name, id) {
  if (!env?.api || !id) return null;
  try {
    const data = env.api.get(name, id).get();
    return data && data._deleted !== true ? data : null;
  } catch {
    return null;
  }
}

function mergeRequestedSession(sessions, requestedSession) {
  return mergeRequestedDocument(sessions, requestedSession);
}

function mergeRequestedDocument(documents, requestedDocument) {
  if (!requestedDocument?.id) return documents;
  return [
    requestedDocument,
    ...documents.filter((document) => document.id !== requestedDocument.id),
  ];
}

function latestFrame(frames, sessionId = '') {
  return frames
    .filter((frame) => frame.data && (!sessionId || frame.session_id === sessionId) && Number(frame.expires_at_ms || 0) > Date.now())
    .sort((a, b) => Number(b.seq || 0) - Number(a.seq || 0) || Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0))[0] || null;
}

function latestSession(sessions, sessionId) {
  const candidates = sessionId
    ? sessions.filter((session) => session.id === sessionId)
    : sessions;
  return candidates.sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0))[0] || null;
}

function latestTab(tabs, tabId) {
  const candidates = tabId
    ? tabs.filter((tab) => tab.id === tabId)
    : tabs;
  return candidates.sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0))[0] || null;
}

function latestBrowserCommand(commands, sessionId) {
  return latestBrowserCommands(commands, sessionId, 1)[0] || null;
}

function applyLatestNavigationResult(state, commands) {
  const sessionId = state.latestSession?.id || state.latestFrame?.session_id;
  if (!sessionId) return;
  const command = commands
    .filter((candidate) => {
      const type = candidate.command_type || candidate.type || '';
      const candidateSessionId = candidate.payload?.session_id || candidate.record_id || '';
      return candidateSessionId === sessionId
        && ['browser.session.start', 'browser.navigate', 'browser.reload', 'browser.back', 'browser.forward', 'browser.reset'].includes(type)
        && candidate.status === 'completed'
        && candidate.result?.url;
    })
    .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0))[0];
  if (!command) return;
  const url = String(command.result.url || '');
  const title = String(command.result.title || state.latestTab?.title || state.latestSession?.title || 'Browser');
  state.latestSession = { ...state.latestSession, current_url: url, title };
  state.latestTab = { ...(state.latestTab || {}), url, title };
}

function latestBrowserCommands(commands, sessionId, limit = 5) {
  return commands
    .filter((command) => {
      const type = command.command_type || command.type || '';
      if (!type.startsWith('browser.')) return false;
      if (!sessionId) return true;
      const payloadSession = command.payload?.session_id;
      return command.record_id === sessionId || payloadSession === sessionId;
    })
    .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0))
    .slice(0, limit);
}

function latestBrowserHandoffTasks(tasks, sessionId, limit = 5) {
  return tasks
    .filter((task) => {
      if (task.command_type !== 'ctox.browser_context.capture') return false;
      if (task.inbound_channel !== 'browser') return false;
      if (!sessionId) return true;
      return String(task.prompt || '').includes(sessionId) || String(task.command_id || '').includes(sessionId);
    })
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0))
    .slice(0, limit);
}

function renderSessionList(refs, sessions, tabs, activeSession) {
  if (!refs.sessionList) return;
  const sorted = [...sessions].sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
  if (!sorted.length) {
    refs.sessionList.innerHTML = '';
    return;
  }
  const activeTabs = tabs
    .filter((tab) => tab.session_id === activeSession?.id && tab.status !== 'closed')
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0));
  const tabMarkup = activeTabs.map((tab) => `
    <span class="ctox-pane-tab browser-tab" data-browser-tab-id="${escapeHtml(tab.id)}" aria-selected="${tab.id === activeSession?.current_tab_id ? 'true' : 'false'}">
      <span class="browser-tab-title">${escapeHtml(tab.title || tab.url || 'Tab')}</span>
      <button type="button" class="ctox-icon-button ctox-icon-button--sm" data-browser-tab-close aria-label="Tab schließen">×</button>
    </span>
  `).join('');
  refs.sessionList.innerHTML = tabMarkup;
}

// ----- LEFT sessions selector (canonical grammar column) -----

// Owned sessions plus the read-only import overlay (imported entries that are
// not already a real owned session). Imported sessions are marked and never
// persisted — browser sessions are a read-only projection.
// custom: Scraping-Adapter im Browser-Modul sichtbar machen.
//
// Die Adapter der Recherche steuern Browser-Laeufe. Core-Outbound und lokale
// Tenant-Apps besitzen getrennte Sammlungen; der Browser zeigt beide in einer
// Betriebssicht. In 4.0 ein Live-Abo je Sammlung (state.watchAdapters im mount),
// solange das Band „Scraping“ sichtbar ist – keine Lease, kein Nachladen.
function applyScrapingAdapters(env, state, rowsByCollection) {
  const deduplicated = new Map();
  for (const collectionName of SCRAPING_ADAPTER_COLLECTIONS) {
    for (const row of rowsByCollection.get(collectionName) || []) {
      if (!row || row.is_deleted === true) continue;
      deduplicated.set(`${collectionName}:${row.id || row.source_id}`, { ...row, adapter_collection: collectionName });
    }
  }
  state.adapters = [...deduplicated.values()];
}

function adapterZustand(adapter) {
  const auth = String(adapter.auth_status || '').toLowerCase();
  const status = String(adapter.status || adapter.last_test?.status || '').toLowerCase();
  if (adapter.enabled === false) return { klasse: 'is-off', text: t('adapterOff', 'Deaktiviert') };
  if (auth.includes('missing') || auth.includes('required') || status.includes('auth')) {
    return { klasse: 'is-error', text: t('adapterAuthMissing', 'Zugang fehlt') };
  }
  if (status.includes('unreachable') || status.includes('fail') || adapter.last_error) {
    return { klasse: 'is-warn', text: t('adapterUnreachable', 'Letzte Prüfung fehlgeschlagen') };
  }
  return { klasse: 'is-ok', text: t('adapterReady', 'Bereit') };
}

// Eine Schiene, zwei Inhalte: Sitzungen oder Scraping-Adapter, je nach Band.
function renderLeftRail(env, refs, state) {
  if (state.leftView?.band === 'adapters') {
    renderAdapterRail(env, refs, state);
    return;
  }
  state.watchAdapters?.(false);
  renderSessions(refs, sessionRenderList(state), state.latestSession, state.leftView, sessionTabCounts(state.tabs), env);
}

function renderAdapterRail(env, refs, state) {
  const rail = refs.sessions;
  if (!rail) return;
  state.watchAdapters?.(true);
  if (refs.adapterCount) refs.adapterCount.textContent = String((state.adapters || []).length || '');
  rail.replaceChildren();
  // Fußzeile gehört zum Band: sonst blieb „6 Sitzungen · Alle“ unter der Adapterliste stehen (Rundgang RUND1).
  const pg = refs.sessionsPane?.__ctoxPaneGrammar;
  const suche = String(state.leftView?.search || '');
  const sichtbar = (state.adapters || []).filter((a) => !suche || String(a.label || a.source_id || a.id || '').toLowerCase().includes(suche)).length;
  // Textdiät Regel 3/5: Anzahl steht im Reiter – kein Fußzähler (der Fuß bleibt beim Bandwechsel leer, nie veraltet).
  void sichtbar;
  const fuss = '';
  if (pg?.setFooter) pg.setFooter(fuss); else writeSessionFooter(refs, fuss);
  // Leere Liste rendert nichts (R11); Lesefehler stehen in der Zustandszone.
  if (!Array.isArray(state.adapters) || !state.adapters.length) return;
  for (const adapter of state.adapters) {
    const label = String(adapter.label || adapter.source_id || adapter.id || '');
    if (suche && !label.toLowerCase().includes(suche)) continue;
    const zustand = adapterZustand(adapter);
    const karte = document.createElement('div');
    karte.className = 'browser-adapter-card';
    const kopf = document.createElement('div');
    kopf.className = 'browser-adapter-head';
    const punkt = document.createElement('span');
    punkt.className = `browser-adapter-dot ${zustand.klasse}`;
    const name = document.createElement('strong');
    name.textContent = label;
    kopf.append(punkt, name);
    const meta = document.createElement('div');
    meta.className = 'browser-adapter-meta';
    const getestet = Number(adapter.last_test?.at_ms || adapter.updated_at_ms || 0);
    const latenz = Number(adapter.latency_ms || adapter.last_test?.latency_ms || 0);
    meta.textContent = [
      String(adapter.source_id || ''),
      zustand.text,
      getestet ? new Date(getestet).toLocaleString() : '',
      latenz ? `${latenz} ms` : '',
    ].filter(Boolean).join(' · ');
    karte.append(kopf, meta);
    if (adapter.last_error) {
      const fehlerzeile = document.createElement('div');
      fehlerzeile.className = 'browser-adapter-error';
      fehlerzeile.textContent = String(adapter.last_error).slice(0, 160);
      karte.appendChild(fehlerzeile);
    }
    rail.appendChild(karte);
  }
}

function sessionRenderList(state) {
  const owned = Array.isArray(state?.visibleSessions) ? state.visibleSessions : [];
  const ownedIds = new Set(owned.map((session) => session.id));
  const imported = (Array.isArray(state?.importedSessions) ? state.importedSessions : [])
    .filter((session) => session?.id && !ownedIds.has(session.id));
  return [...owned, ...imported];
}

function sessionTabCounts(tabs) {
  const counts = {};
  for (const tab of Array.isArray(tabs) ? tabs : []) {
    if (tab.status === 'closed') continue;
    counts[tab.session_id] = Number(counts[tab.session_id] || 0) + 1;
  }
  return counts;
}

// Rebuild the sessions well ONLY when the rendered data changed (signature
// guard). Selection is applied in place via markActiveSession so a re-render
// never clamps the well scroll or moves the operator.
function renderSessions(refs, sessions, activeSession, view = {}, tabCounts = {}, env = null) {
  if (!refs.sessions) return;
  const all = Array.isArray(sessions) ? sessions : [];
  const filtered = filterSessionsForView(all, view);
  const listView = view?.view === 'list' ? 'list' : 'cards';
  refs.sessions.classList.toggle('is-list', listView === 'list');

  const signature = sessionListSignature(filtered, listView, tabCounts);
  if (refs.sessions.dataset.sig !== signature) {
    refs.sessions.dataset.sig = signature;
    refs.sessions.innerHTML = filtered
      .map((session) => sessionShardMarkup(session, tabCounts[session.id] || 0, env, listView))
      .join('');
  }
  if (refs.sessionsEmpty) refs.sessionsEmpty.hidden = filtered.length > 0;
  markActiveSession(refs, activeSession?.id || '');

  const counts = browserSessionViewCounts(all, view);
  const pg = refs.sessionsPane?.__ctoxPaneGrammar;
  if (pg?.setCounts) pg.setCounts(counts);
  else writeSessionCounts(refs, counts);
  // Textdiät Regel 3/5: Anzahl steht im Reiter „Alle(N)“ – kein Fußzähler.
  const footer = '';
  if (pg?.setFooter) pg.setFooter(footer);
  else writeSessionFooter(refs, footer);
}

// In-place selection flip across existing rows — NEVER a rebuild.
function markActiveSession(refs, sessionId) {
  for (const node of refs.sessions?.querySelectorAll('[data-browser-session-id]') || []) {
    const active = node.dataset.browserSessionId === sessionId;
    node.classList.toggle('is-selected', active);
    node.classList.toggle('is-active', active);
    node.setAttribute('aria-selected', active ? 'true' : 'false');
  }
}

// Karten und Liste sind zwei verschiedene Dichten, keine zwei Innenabstaende
// (Betreiber-Direktive 31.08.2026):
//   KARTEN = fetter Titel + Betriebs-Meta + Detailzeile (drei Zeilen),
//   LISTE  = genau eine Zeile, Titel links und EIN Kurz-Meta rechts.
// Beide Formen leben von Feldern, die die App ohnehin geladen hat.
function sessionShardMarkup(session, tabCount, env = null, listView = 'cards') {
  const url = session.current_url || session.payload?.target_url || '';
  const title = browserDisplayTitle(null, session, url);
  const importedClass = session.__imported ? ' browser-session--imported' : '';
  // Das Symbol traegt den Zustand, nicht nur der Text: bei 34 Sitzungen findet
  // man "Eingriff noetig" sonst nicht, ohne jede Zeile zu lesen.
  const z = browserSitzungZustand(session, env);
  const open = `
    <div class="ctox-list-item browser-session${importedClass} ${z.klasse}" role="option" aria-selected="false" tabindex="0"
      data-browser-session-id="${escapeHtml(session.id)}"
      data-browser-zustand="${escapeHtml(z.klasse)}"
      data-context-record-id="${escapeHtml(session.id)}"
      data-context-record-type="browser_session"
      data-context-label="${escapeHtml(title)}">
      <span class="browser-session-zustand" title="${escapeHtml(z.text)}" aria-label="${escapeHtml(z.text)}">${z.symbol}</span>
      <span class="browser-session-title">${escapeHtml(title)}</span>`;
  if (listView === 'list') {
    const brief = browserSessionListMeta(session, tabCount, env);
    return `${open}
      <span class="browser-session-brief" title="${escapeHtml(brief)}">${escapeHtml(brief)}</span>
    </div>`;
  }
  const meta = browserSessionShardMeta(session, tabCount, env);
  const detail = browserSessionShardDetail(session, url);
  return `${open}
      <span class="browser-session-meta" title="${escapeHtml(meta)}">${escapeHtml(meta)}</span>
      <span class="browser-session-detail" title="${escapeHtml(detail)}">${escapeHtml(detail)}</span>
    </div>`;
}

// WER STEUERT DIESE SITZUNG, und muss jemand eingreifen?
//
// Bis 13.08.2026 stand in jeder Zeile nur "Persoenlich · Bereit · 1 Tab". Bei
// 34 Sitzungen war die Liste damit wertlos: man sah nicht, ob eine Sitzung dem
// Nutzer gehoert oder von der Recherche gefahren wird, und vor allem nicht, wo
// jemand eingreifen muss. Eine Automatik, die auf eine Anmeldung oder eine
// Bot-Pruefung wartet, lief still in ihre Zeitueberschreitung — der Klick, der
// gereicht haette, wurde nie angefordert.
//
// Die vier Zustaende kommen ohne neue Felder aus; sie stehen alle schon im
// Sitzungsdatensatz.
const SITZUNG_ZUSTAENDE = Object.freeze({
  anmeldung: { symbol: '🔑', klasse: 'is-anmeldung', text: 'Anmeldung – hier bestätigen' },
  anmeldungRuhend: { symbol: '🔑', klasse: 'is-anmeldung', text: 'Anmeldung nötig – Sitzung starten' },
  eingriff: { symbol: '⚠', klasse: 'is-eingriff', text: 'Eingriff nötig' },
  nutzer: { symbol: '👤', klasse: 'is-nutzer', text: 'Sie steuern' },
  automatik: { symbol: '⚙', klasse: 'is-automatik', text: 'Automatik' },
  ruhend: { symbol: '○', klasse: 'is-ruhend', text: 'Ruhend' },
});

function browserSitzungZustand(session, env, jetzt = Date.now()) {
  if (!session?.id) return SITZUNG_ZUSTAENDE.ruhend;
  // 1. Eingriff zuerst — er ist der einzige Zustand, der jemanden BRAUCHT.
  const fehler = String(browserSessionError(session) || '').toLowerCase();
  const titel = String(session.title || '').toLowerCase();
  const wartetAufMensch = /anmeld|login|sign in|captcha|verify|bot|zustimm|consent|just a moment/
    .test(`${fehler} ${titel}`);
  const lauft = ['active', 'running', 'ready', 'capturing'].includes(
    String(session.runtime_status || session.status || '').toLowerCase());
  // 1a. Eine Anmeldesitzung des Web Stack zaehlt AUCH IM RUHENDEN Zustand als
  //     Eingriff. Am 03.09.2026 gemessen: acht Anmeldeauftraege fuer
  //     dnbhoovers.com standen in der Warteschlange, die zugehoerige Sitzung
  //     trug vollstaendige Metadaten - war aber `disconnected` und stand
  //     deshalb als schlichtes "Ruhend" zwischen dreissig anderen. Niemand
  //     konnte sehen, dass GENAU DIESE Sitzung die Recherche blockiert.
  const anmeldung = String(session.payload?.purpose || '') === 'web_stack_auth'
    && String(session.payload?.auth_assist_status || '') !== 'completed'
    && session.payload?.authenticated !== true;
  if (anmeldung) {
    return lauft
      ? SITZUNG_ZUSTAENDE.anmeldung
      : SITZUNG_ZUSTAENDE.anmeldungRuhend;
  }
  if (!lauft) return SITZUNG_ZUSTAENDE.ruhend;

  // 2. Steuert der angemeldete Nutzer selbst — mit GUELTIGER Pacht?
  //    Das kommt VOR dem Eingriff-Zustand. Am 03.09.2026 wurde eine Sitzung,
  //    die der Nutzer gerade selbst fuhr, als "Eingriff nötig" gefuehrt, nur
  //    weil die geoeffnete Seite "Login" im Titel trug. Wer selbst am Steuer
  //    sitzt, braucht keine Aufforderung einzugreifen.
  const meine = browserActorIds(env?.actor) || [];
  const steuernder = String(session.controller_user_id || '');
  const pachtBis = Number(session.controller_lease_expires_at_ms || 0);
  const selbstAmSteuer = Boolean(steuernder && meine.includes(steuernder) && pachtBis > jetzt);

  if (selbstAmSteuer) return SITZUNG_ZUSTAENDE.nutzer;
  if (wartetAufMensch) return SITZUNG_ZUSTAENDE.eingriff;
  // 3. Sonst faehrt sie jemand anderes — Recherche, Adapter, anderer Nutzer.
  //    Diese Sitzungen NICHT versehentlich uebernehmen: ein Zugriff bricht
  //    einen laufenden Rechercheschritt ab.
  return SITZUNG_ZUSTAENDE.automatik;
}

function browserSessionShardMeta(session, tabCount = 0, env = null) {
  const status = browserStatusLabel(session);
  const profile = (session.profile_mode || session.payload?.profile_mode) === 'private' ? 'Privat' : 'Persönlich';
  const count = Number(tabCount || 0);
  const zustand = browserSitzungZustand(session, env);
  const parts = [zustand.text, profile, status, `${count} ${count === 1 ? 'Tab' : 'Tabs'}`];
  const error = browserSessionError(session);
  if (error) parts.push(error);
  if (session.__imported) parts.push('Import');
  return parts.join(' · ');
}

// Dritte Kartenzeile: wo die Sitzung steht und wann sie sich zuletzt gemeldet
// hat. Reine Anzeige aus bereits geladenen Feldern — kein neuer Datenpfad.
// `updated_at_ms` steht in sessionListSignature, die Zeile veraltet also nicht.
function browserSessionShardDetail(session, url = '') {
  const target = url || session?.current_url || session?.payload?.target_url || '';
  const parts = [];
  if (target) parts.push(browserUrlLabel(target));
  const seen = Number(session?.updated_at_ms || session?.created_at_ms || 0);
  if (seen > 0) {
    parts.push(new Date(seen).toLocaleString(undefined, {
      day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit',
    }));
  }
  if (session?.__imported) parts.push(t('shardImported', 'Import'));
  return parts.join(' · ');
}

// Listenzeile: EIN Kurz-Meta rechts. Der Betriebszustand ist das, wonach in
// einer dichten Liste gesucht wird — Profil, Fehlertext und Adresse stehen in
// der Kartenansicht und im Titel-Tooltip.
function browserSessionListMeta(session, tabCount = 0, env = null) {
  const zustand = browserSitzungZustand(session, env);
  const count = Number(tabCount || 0);
  return count > 0 ? `${zustand.text} · ${count}` : zustand.text;
}

// EIN Umschalter Karten/Liste. `data-pg-view` traegt die AKTUELLE Ansicht (die
// Shell-Grammatik liest sie dort), Symbol und Beschriftung benennen die ZIEL-
// Ansicht. aria-pressed entfaellt: der Knopf ist eine Aktion, kein Zustand —
// die Shell stempelt es auf [data-pg-view], hier faellt es wieder weg.
function syncViewToggle(refs, view) {
  const button = refs?.viewToggle;
  if (!button) return;
  const current = view === 'list' ? 'list' : 'cards';
  button.dataset.pgView = current;
  const label = current === 'list'
    ? t('viewShowCards', 'Als Karten anzeigen')
    : t('viewShowList', 'Als Liste anzeigen');
  button.setAttribute('aria-label', label);
  button.title = label;
  button.removeAttribute('aria-pressed');
}

function browserSessionBand(session) {
  // "Aktiv" means a native runtime has confirmed a live Chromium-backed
  // session. Starting, blocked and disconnected rows do not count as active.
  return browserUiState(session) === 'ready' ? 'active' : 'closed';
}

function browserSessionMatchesBand(session, band) {
  if (!band || band === 'all') return true;
  return browserSessionBand(session) === band;
}

function filterSessionsForView(sessions, view = {}) {
  const search = String(view.search || '').trim().toLowerCase();
  const profile = view.filters?.profile && view.filters.profile !== 'all' ? view.filters.profile : '';
  const band = view.band || 'all';
  return (Array.isArray(sessions) ? sessions : []).filter((session) => {
    if (!browserSessionMatchesBand(session, band)) return false;
    if (profile) {
      const mode = session.profile_mode || session.payload?.profile_mode || 'persistent';
      if (mode !== profile) return false;
    }
    if (search) {
      const hay = `${session.id || ''} ${session.title || ''} ${session.current_url || ''} ${session.payload?.target_url || ''}`.toLowerCase();
      if (!hay.includes(search)) return false;
    }
    return true;
  });
}

// Band counts ignore the band selection (the band IS the selector) but honor
// search + profile filters; zeros are rendered, never hidden.
function browserSessionViewCounts(sessions, view = {}) {
  const base = filterSessionsForView(sessions, { ...view, band: 'all' });
  return {
    all: base.length,
    active: base.filter((session) => browserSessionBand(session) === 'active').length,
    closed: base.filter((session) => browserSessionBand(session) === 'closed').length,
  };
}

// Selection-independent signature: a pure selection change (not part of the
// signature) leaves it identical, so renderSessions skips the rebuild.
function sessionListSignature(filteredSessions, listView, tabCounts = {}) {
  const rows = (Array.isArray(filteredSessions) ? filteredSessions : []).map((session) => [
    session.id || '',
    browserUiState(session),
    String(session.title || session.current_url || ''),
    browserSessionError(session),
    session.profile_mode || session.payload?.profile_mode || '',
    Number(tabCounts[session.id] || 0),
    Number(session.updated_at_ms || 0),
    session.__imported ? 'i' : '',
    // Der Zustand haengt an Steuerndem und Pacht — ohne diese beiden
    // Werte in der Signatur bliebe das Symbol stehen, wenn eine Pacht
    // ablaeuft oder jemand anderes uebernimmt.
    String(session.controller_user_id || ''),
    Number(session.controller_lease_expires_at_ms || 0) > Date.now() ? 'p' : '-',
  ].join(':'));
  return `${listView}::${rows.join('|')}`;
}

// Auto-reveal model (design-guide "Progressive Disclosure"): the remote work
// surface is the browser's "detail" — revealed once a session is selected and
// not user-collapsed. Mirrors the outbound/conversations idiom.
function browserWorkbenchVisible(hasSession, userCollapsed = false) {
  return Boolean(hasSession) && !userCollapsed;
}

function bandLabel(band) {
  if (band === 'active') return 'Aktiv';
  if (band === 'closed') return 'Beendet';
  if (band === 'adapters') return 'Scraping';
  return 'Alle';
}

function writeSessionCounts(refs, counts) {
  for (const [key, value] of Object.entries(counts || {})) {
    const node = refs.sessionsPane?.querySelector(`[data-pg-count="${key}"]`);
    if (node) node.textContent = ` (${value})`;
  }
}

function writeSessionFooter(refs, text) {
  const node = refs.sessionsPane?.querySelector('[data-pg-footer]');
  if (node) node.textContent = text || '';
}

// Export/import are honest and small: export writes owned sessions as JSON via a
// Blob; import overlays sessions for local review only (never persisted).
function buildBrowserSessionsExport(sessions, nowMs = Date.now()) {
  return {
    kind: 'browser_sessions',
    schema_version: 1,
    exported_at_ms: Number(nowMs) || 0,
    sessions: (Array.isArray(sessions) ? sessions : []).map((session) => ({
      id: session.id || '',
      title: browserDisplayTitle(null, session, session.current_url || ''),
      url: session.current_url || session.payload?.target_url || '',
      status: session.runtime_status || session.status || '',
      profile_mode: session.profile_mode || session.payload?.profile_mode || 'persistent',
      owner_user_id: session.owner_user_id || '',
      controller_user_id: session.controller_user_id || '',
      updated_at_ms: Number(session.updated_at_ms || 0),
    })),
  };
}

function parseBrowserSessionsImport(parsed) {
  const rows = Array.isArray(parsed)
    ? parsed
    : Array.isArray(parsed?.sessions) ? parsed.sessions : [];
  return rows
    .map((row) => {
      const id = String(row?.id || '').trim();
      if (!id) return null;
      const status = String(row.status || 'imported');
      return {
        id,
        title: String(row.title || ''),
        current_url: String(row.url || row.current_url || ''),
        status,
        runtime_status: status,
        profile_mode: String(row.profile_mode || 'persistent'),
        updated_at_ms: Number(row.updated_at_ms || 0),
        __imported: true,
      };
    })
    .filter(Boolean);
}

function downloadBrowserJson(payload, filename, root) {
  let url = '';
  try {
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
    url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = filename;
    anchor.rel = 'noopener';
    (root || document.body)?.appendChild?.(anchor);
    anchor.click();
    anchor.remove?.();
  } catch (error) {
    console.error('[browser] session export failed', error);
  } finally {
    if (url) setTimeout(() => { try { URL.revokeObjectURL(url); } catch {} }, 4000);
  }
}

function exportBrowserSessions(state, refs) {
  const rows = sessionRenderList(state).filter((session) => !session.__imported);
  downloadBrowserJson(buildBrowserSessionsExport(rows, Date.now()), 'browser-sessions.json', refs?.root);
}

function importBrowserSessions(env, state, refs) {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = 'application/json,.json';
  input.addEventListener('change', async () => {
    const file = input.files && input.files[0];
    if (!file) return;
    let parsed;
    try {
      parsed = JSON.parse(await file.text());
    } catch {
      env.status.report({ level: 'error', code: 'browser.import.invalid', text: t('importInvalid', 'Ungültige JSON-Datei.') });
      return;
    }
    const imported = parseBrowserSessionsImport(parsed);
    if (!imported.length) {
      env.status.report({ level: 'warn', code: 'browser.import.invalid', text: t('importEmpty', 'Keine Sitzungen in der Datei.') });
      return;
    }
    env.status.clear('browser.import.invalid');
    state.importedSessions = imported;
    renderLeftRail(env, refs, state);
    renderTabstrip(env, refs, state);
  });
  input.click();
}

function renderSession(refs, session, tab, frame, command, state) {
  if (!session) {
    refs.sessionCard.replaceChildren();
    return;
  }
  const url = tab?.url || session.current_url || '';
  const commandError = commandErrorMessage(command);
  const runtimeError = browserSessionError(session);
  const sessionError = runtimeError
    ? `<div class="browser-error"><strong>Browser-Laufzeit:</strong> ${escapeHtml(runtimeError)}</div>`
    : '';
  const commandLine = command
    ? `<div class="browser-muted">Letzte Browseraktion: ${escapeHtml(browserActionLabel(command.command_type || command.type || 'browser'))}</div>`
    : '';
  const policy = session.payload?.control_policy || {};
  const owner = session.owner_user_id || policy.owner_user_id || '-';
  const controller = session.controller_user_id || policy.controller_user_id || '-';
  const leaseRemaining = Math.max(0, Number(session.controller_lease_expires_at_ms || 0) - Date.now());
  const profileMode = session.profile_mode || session.payload?.profile_mode || 'persistent';
  const status = browserStatusLabel(session);
  if (refs.address && url && !state?.addressDirty && document.activeElement !== refs.address) refs.address.value = url;
  refs.sessionCard.innerHTML = `
    <strong>${escapeHtml(browserDisplayTitle(tab, session, url))}</strong>
    <div class="browser-muted">${escapeHtml(url || 'about:blank')}</div>
    <div class="browser-meta-grid">
      <span>Status</span><span>${escapeHtml(status)}</span>
      <span>Owner</span><span>${escapeHtml(owner)}</span>
      <span>Control</span><span>${escapeHtml(controller)}</span>
      <span>Lease</span><span>${leaseRemaining ? `${Math.ceil(leaseRemaining / 60000)} min` : 'inaktiv'}</span>
      <span>Beobachter</span><span>${Array.isArray(session.allowed_observer_user_ids) ? session.allowed_observer_user_ids.length : 0}</span>
      <span>Profil</span><span>${profileMode === 'private' ? 'Privat – wird gelöscht' : 'Persönlich – persistent'}</span>
      <span>Frame</span><span>${escapeHtml(frame ? `${frame.width}x${frame.height}` : 'Kein Bild')}</span>
    </div>
    ${commandLine}
    ${sessionError}
    ${commandError ? `<div class="browser-error">${escapeHtml(commandError)}</div>` : ''}
  `;
}

function browserActionLabel(commandType) {
  const type = String(commandType || '');
  if (type === 'browser.session.start') return 'Fenster geoeffnet';
  if (type === 'browser.navigate') return 'Navigation';
  if (type === 'browser.reload') return 'Neu laden';
  if (type === 'browser.back') return 'Zurueck';
  if (type === 'browser.forward') return 'Vor';
  if (type === 'browser.reset') return 'Zurueckgesetzt';
  if (type === 'browser.session.stop') return 'Fenster geschlossen';
  return 'Aktualisiert';
}

function renderAuthAssist(refs, session) {
  if (!refs.authAssist) return;
  const payload = session?.payload || {};
  const permission = payload.pending_permission;
  if (permission && typeof permission === 'object') {
    refs.authAssist.hidden = false;
    refs.authAssist.innerHTML = `
      <div>
        <span class="ctox-pane-kicker">Website-Berechtigung</span>
        <strong>${escapeHtml(titleCase(permission.kind || 'permission'))}</strong>
        <small>${escapeHtml(permission.origin || '')}</small>
      </div>
      <div class="ctox-pane-tools">
        <button type="button" class="ctox-button" data-browser-permission-response="dismiss">Blockieren</button>
        <button type="button" class="ctox-button" data-browser-permission-response="accept">Einmal erlauben</button>
      </div>`;
    return;
  }
  const httpAuth = payload.pending_http_auth;
  if (httpAuth && typeof httpAuth === 'object') {
    refs.authAssist.hidden = false;
    refs.authAssist.innerHTML = `
      <div>
        <span class="ctox-pane-kicker">Anmeldung</span>
        <strong>${escapeHtml(httpAuth.realm || httpAuth.origin || 'Geschützter Bereich')}</strong>
      </div>
      <div class="ctox-pane-tools">
        <button type="button" class="ctox-button" data-browser-http-auth-response="dismiss">Abbrechen</button>
        <button type="button" class="ctox-button" data-browser-http-auth-response="accept" title="Zugangsdaten nur aus dem CTOX-Tresor">Gespeicherte Zugangsdaten verwenden</button>
      </div>`;
    return;
  }
  const webAuthn = payload.pending_webauthn;
  if (webAuthn && typeof webAuthn === 'object') {
    refs.authAssist.hidden = false;
    refs.authAssist.innerHTML = `
      <div>
        <span class="ctox-pane-kicker">Passkey ${escapeHtml(webAuthn.type === 'create' ? 'registrieren' : 'verwenden')}</span>
        <strong>${escapeHtml(webAuthn.rp_id || 'Unbekannte Website')}</strong>
      </div>
      <div class="ctox-pane-tools">
        <button type="button" class="ctox-button" data-browser-webauthn-response="dismiss">Ablehnen</button>
        <button type="button" class="ctox-button" data-browser-webauthn-response="accept">Bestätigen</button>
      </div>`;
    return;
  }
  const dialog = payload.pending_dialog;
  if (dialog && typeof dialog === 'object') {
    refs.authAssist.hidden = false;
    refs.authAssist.innerHTML = `
      <div>
        <span class="ctox-pane-kicker">${escapeHtml(titleCase(dialog.type || 'dialog'))}</span>
        <strong>${escapeHtml(dialog.message || 'Die Webseite verlangt eine Entscheidung.')}</strong>
      </div>
      <div class="ctox-pane-tools">
        <button type="button" class="ctox-button" data-browser-dialog-response="dismiss">Abbrechen</button>
        <button type="button" class="ctox-button" data-browser-dialog-response="accept">Bestätigen</button>
      </div>`;
    return;
  }
  const isAuthAssist = payload.purpose === 'web_stack_auth';
  refs.authAssist.hidden = !isAuthAssist;
  if (!isAuthAssist) {
    refs.authAssist.innerHTML = '';
    return;
  }
  const completed = payload.auth_assist_status === 'completed' || payload.authenticated === true;
  const fillStatus = payload.credential_fill_status || '';
  const extractStatus = payload.capture_extract_status || '';
  const canFill = Boolean(payload.secret_name) && !completed;
  const canCapture = Boolean(completed && payload.capture_script);
  const canExtract = Boolean(completed && payload.capture_script);
  const domains = Array.isArray(payload.allowed_domains) ? payload.allowed_domains.join(', ') : '';
  const instruction = String(payload.instruction || '').trim()
    || 'Auf der Seite anmelden (ggf. MFA/Captcha), dann fortsetzen.';
  refs.authAssist.innerHTML = `
    <div class="browser-auth-summary">
      <span class="ctox-pane-kicker">Anmeldung</span>
      <strong>${escapeHtml(payload.source_id || 'Anmeldung erforderlich')}</strong>
      <small class="browser-auth-instruction" title="${escapeHtml(instruction)}">${escapeHtml(domains || payload.target_url || '')}</small>
      ${fillStatus ? `<small>${escapeHtml(authAssistStatusLabel(fillStatus, 'Zugangsdaten werden eingesetzt'))}</small>` : ''}
      ${extractStatus ? `<small>${escapeHtml(authAssistStatusLabel(extractStatus, 'Seitenauswertung laeuft'))}</small>` : ''}
    </div>
    <div class="ctox-pane-tools">
      <button type="button" class="ctox-button" data-browser-credential-fill ${canFill ? '' : 'disabled'}>
        Zugangsdaten einsetzen
      </button>
      <button type="button" class="ctox-button" data-browser-auth-complete ${completed ? 'disabled' : ''}>
        ${completed ? 'Recherche wird fortgesetzt' : 'Erledigt – Recherche fortsetzen'}
      </button>
      <button type="button" class="ctox-button" data-browser-web-stack-capture ${canCapture ? '' : 'disabled'}>
        An CTOX uebergeben
      </button>
      <button type="button" class="ctox-button" data-browser-web-stack-extract ${canExtract ? '' : 'disabled'}>
        Seite auslesen
      </button>
    </div>
  `;
}

function renderAutomationOverlay(refs, session) {
  if (!refs.automationOverlay) return;
  const payload = session?.payload || {};
  const captureScript = String(payload.capture_script || '').trim();
  const isWebStack = payload.purpose === 'web_stack_auth' || Boolean(captureScript);
  refs.automationOverlay.hidden = !isWebStack;
  if (!isWebStack) {
    if (refs.automationCode) refs.automationCode.textContent = '';
    return;
  }
  const source = String(payload.source_id || payload.target_url || session?.current_url || 'Web Stack');
  const extractStatus = String(payload.capture_extract_status || '').toLowerCase();
  const authenticated = payload.auth_assist_status === 'completed' || payload.authenticated === true;
  const status = extractStatus
    ? authAssistStatusLabel(extractStatus, 'Seitenauswertung aktiv')
    : authenticated
      ? 'Bereit zum Auslesen'
      : 'Anmeldung erforderlich';
  if (refs.automationTitle) refs.automationTitle.textContent = 'Playwright-Scraper';
  if (refs.automationStatus) refs.automationStatus.textContent = status;
  if (refs.automationSource) refs.automationSource.textContent = source;
  if (refs.automationCode) {
    refs.automationCode.textContent = captureScript;
  }
}

function authAssistStatusLabel(status, fallback) {
  const normalized = String(status || '').toLowerCase();
  if (['completed', 'done', 'ok', 'success'].includes(normalized)) return 'Abgeschlossen';
  if (['pending', 'pending_sync', 'accepted', 'running'].includes(normalized)) return fallback;
  if (['failed', 'error'].includes(normalized)) return 'Aktion fehlgeschlagen';
  return fallback;
}

function renderStatus(refs, session, tab, frame, command) {
  const state = browserUiState(session);
  if (refs.statusChip) {
    // Textdiät: keine Zustands-Pille im Kopf („Verbunden“); Zustand gehört in die Zone am App-Icon.
    refs.statusChip.textContent = '';
    refs.statusChip.hidden = true;
    refs.statusChip.dataset.state = state;
    refs.statusChip.classList.toggle('is-success', state === 'ready');
    refs.statusChip.classList.toggle('is-warning', ['starting', 'waiting', 'blocked'].includes(state));
    refs.statusChip.classList.toggle('is-danger', state === 'error');
  }
  const url = tab?.url || session?.current_url || '';
  if (refs.statusTitle) {
    refs.statusTitle.textContent = session ? browserDisplayTitle(tab, session, url) : '';
  }
  const bits = [];
  if (url) bits.push(url);
  if (frame?.seq != null) bits.push(`Frame ${frame.seq}`);
  if (session?.frame_rate_target) bits.push(`${session.frame_rate_target} fps`);
  const error = commandErrorMessage(command) || browserSessionError(session);
  if (error) bits.push(`Grund: ${error}`);
  if (refs.statusMeta) refs.statusMeta.textContent = bits.join(' - ') || '-';
}

function renderDownloads(refs, session) {
  if (!refs.downloads) return;
  const downloads = Array.isArray(session?.payload?.downloads) ? session.payload.downloads : [];
  refs.downloads.hidden = downloads.length === 0;
  refs.downloads.innerHTML = downloads.map((download) => `
    <span class="browser-download-item">
      <strong>${escapeHtml(download.filename || 'Download')}</strong>
      · ${escapeHtml(download.status || 'Unbekannt')}
      · ${escapeHtml(formatBytes(download.size_bytes || 0))}
      <button type="button" class="ctox-button ctox-button--sm" data-browser-download-action="release" data-browser-download-id="${escapeHtml(download.id || '')}" ${download.status === 'clean' ? '' : 'disabled'}>Freigeben</button>
      <button type="button" class="ctox-button ctox-button--sm" data-browser-download-action="rescan" data-browser-download-id="${escapeHtml(download.id || '')}" ${['infected', 'discarded', 'released'].includes(download.status) ? 'disabled' : ''}>Neu prüfen</button>
      <button type="button" class="ctox-button ctox-button--sm" data-browser-download-action="discard" data-browser-download-id="${escapeHtml(download.id || '')}" ${['discarded', 'released'].includes(download.status) ? 'disabled' : ''}>Verwerfen</button>
    </span>
  `).join('');
}

function renderControls(env, refs, state) {
  const hasSession = Boolean(state.latestSession?.id);
  const isStopped = ['stopped', 'closed'].includes(String(state.latestSession?.status || state.latestSession?.runtime_status || '').toLowerCase());
  const surfaceFocused = browserSurfaceIsFocused(env);
  const canControl = browserSurfaceCanControl(env, state);
  for (const button of [refs.go, refs.stop, refs.reload, refs.back, refs.forward, refs.sendToCtox, refs.upload, refs.newTab, refs.clipboardCopy, refs.clipboardPaste, refs.clipboardClear]) {
    if (!button) continue;
    button.disabled = !hasSession || isStopped || !canControl;
  }
  // The address bar is also the recovery path for a stale or disconnected
  // session. Keep it operable; submit starts a fresh leased session when the
  // current surface cannot safely navigate the existing one.
  if (refs.go) refs.go.disabled = false;
  if (refs.controllerAcquire) {
    // NICHT am Fokus aufhaengen. Ein Klick AUF diesen Knopf ist die
    // Interaktion, die die Uebernahme rechtfertigt - ein deaktivierter Knopf
    // kann diesen Klick nie empfangen. Meldet die Huelle kein `is-focused`
    // (gemessen am 03.09.2026 auf einem Kundenmandanten: visibilityState=hidden,
    // hasFocus()=false bei offenem Fenster), war die Sitzung dauerhaft
    // unbedienbar UND der einzige Ausweg gesperrt.
    refs.controllerAcquire.disabled = !hasSession || isStopped || canControl;
  }
  if (refs.controllerRelease) {
    refs.controllerRelease.disabled = !hasSession || isStopped || !canControl;
  }
  for (const button of [refs.observerGrant, refs.observerRevoke]) {
    if (!button) continue;
    button.disabled = !hasSession || isStopped || !surfaceFocused;
  }
  refs.canvas?.setAttribute('aria-disabled', canControl ? 'false' : 'true');
}

// Hinweise (Sperrgruende, Fehler) gehen nur in die Zustandszone der Shell (R11), nie in den Inhalt.
function renderNotice(env, notice) {
  if (!env?.status) return;
  if (notice) env.status.report({ level: 'warn', code: 'browser.notice', text: String(notice) });
  else env.status.clear('browser.notice');
}

async function renderFrame(env, refs, frame, state) {
  // Die Buehne zeigt nur das Bild. Ohne Bild bleibt sie leer; Gruende (Laufzeitfehler, abgelaufene
  // Steuerung, geschlossen) stehen in der Zustandszone (browser.frame), nie als Text auf der Buehne (R11).
  // Einzige Ausnahme (FIX8): ganz ohne Sitzung ein Satz + Aktion statt Sackgasse, ohne Ortsangabe.
  renderStageEmpty(refs, state);
  const reason = frame?.data ? null : frameEmptyReason(state);
  if (reason) env.status.report({ level: reason.level, code: 'browser.frame', text: reason.text, ref: state.latestSession?.id || undefined });
  else env.status.clear('browser.frame');
  if (!frame?.data || state.drawing) return;
  state.drawing = true;
  try {
    const img = new Image();
    await new Promise((resolve, reject) => {
      img.onload = resolve;
      img.onerror = reject;
      img.src = `data:${frame.mime_type || 'image/png'};base64,${frame.data}`;
    });
    refs.canvas.width = Number(frame.width || VIEWPORT.width);
    refs.canvas.height = Number(frame.height || VIEWPORT.height);
    const g2d = refs.canvas.getContext('2d');
    g2d.clearRect(0, 0, refs.canvas.width, refs.canvas.height);
    g2d.drawImage(img, 0, 0, refs.canvas.width, refs.canvas.height);
    env.status.clear('browser.frame.render');
  } catch (error) {
    console.error('[browser] frame render failed', error);
    env.status.report({ level: 'error', code: 'browser.frame.render', text: t('frameRenderFailed', 'Die Seite konnte nicht angezeigt werden.') });
  } finally {
    state.drawing = false;
  }
}

// FIX8: Leerzustand der Buehne, nur solange es gar keine Sitzung gibt: „Keine Browser-Sitzung.“ + „Neue Sitzung“.
// Die Aktion loest den vorhandenen Kopfknopf aus (Start-Logik existiert nur einmal).
function renderStageEmpty(refs, state) {
  if (!refs.empty) return;
  const show = !state.latestSession && state.ansicht !== 'script';
  refs.empty.hidden = !show;
  if (!show) return;
  if (!refs.empty.querySelector('[data-browser-empty-start]')) {
    refs.empty.innerHTML = '<span data-browser-empty-text></span>'
      + '<button type="button" class="ctox-button is-primary" data-browser-empty-start></button>';
    refs.empty.querySelector('[data-browser-empty-start]').addEventListener('click', () => refs.start?.click());
  }
  refs.empty.querySelector('[data-browser-empty-text]').textContent = t('emptyNoSession', 'Keine Browser-Sitzung.');
  refs.empty.querySelector('[data-browser-empty-start]').textContent = t('sessionsNew', 'Neue Sitzung');
}

// Nur echte Gruende, die einen Eingriff brauchen; Warten/Starten/„keine Sitzung“ zeigen nichts.
function frameEmptyReason(state) {
  // Fehlgeschlagene Befehle dieser App meldet bereits ihr Befehlsstatus (browser.command.<typ>).
  const session = state.latestSession;
  if (!session) return null;
  const status = String(session.runtime_status || session.status || '').toLowerCase();
  const runtimeError = browserSessionError(session);
  if (['failed', 'error', 'blocked'].includes(status)) {
    return { level: 'error', text: runtimeError || t('frameStartFailed', 'Der Browser konnte nicht gestartet werden') };
  }
  if (['disconnected', 'offline'].includes(status)) {
    return { level: 'warn', text: runtimeError || 'Kein laufender Browser-Prozess. Starten Sie die Sitzung erneut.' };
  }
  // Eine abgelaufene Steuerpacht sieht aus wie ein haengendes Bild. Am 03.09.2026 auf einem
  // Kundenmandanten gemessen: Pacht seit 4,7 Stunden abgelaufen, die Sitzung zeigte „Bereit“.
  const pachtEndeMs = Number(session.controller_lease_expires_at_ms || 0);
  const pachtTot = !Number.isFinite(pachtEndeMs) || pachtEndeMs <= Date.now();
  if (['active', 'running', 'capturing'].includes(status) && pachtTot) {
    return { level: 'warn', text: 'Steuerung abgelaufen – „Steuerung übernehmen“ holt Bild und Eingabe zurück.' };
  }
  return null;
}

// Rueckwaertskompatibel fuer bestehende Tests: Text des Grundes oder ''.
function frameEmptyText(state) {
  return frameEmptyReason(state)?.text || '';
}

function browserUiState(session) {
  if (!session) return 'offline';
  const raw = String(session.runtime_status || session.status || '').toLowerCase();
  if (['active', 'running', 'capturing'].includes(raw)) return 'ready';
  if (['requested', 'starting', 'pending_command', 'pending_sync'].includes(raw)) return 'starting';
  if (raw === 'blocked') return 'blocked';
  if (['stopped', 'closed', 'disconnected', 'offline'].includes(raw)) return 'offline';
  if (['failed', 'error'].includes(raw)) return 'error';
  return raw ? 'waiting' : 'offline';
}

function browserStatusLabel(session) {
  const state = browserUiState(session);
  if (state === 'ready') return t('statusReady', 'Bereit');
  if (state === 'starting') return t('statusStarting', 'Startet');
  if (state === 'waiting') return t('statusConnecting', 'Verbindet');
  if (state === 'blocked') return 'Aktion erforderlich';
  if (state === 'error') return t('statusError', 'Fehler');
  return t('statusDisconnected', 'Beendet');
}

function browserDisplayTitle(tab, session, url = '') {
  const raw = String(tab?.title || session?.title || '').trim();
  if (!raw || /^remote browser$/i.test(raw)) {
    return url ? browserUrlLabel(url) : 'Browser';
  }
  return raw;
}

function browserUrlLabel(url) {
  try {
    const parsed = new URL(url);
    return parsed.hostname || 'Browser';
  } catch {
    return 'Browser';
  }
}

function renderDiagnostics(refs, frame, inputs, command, commands = [], handoffTasks = []) {
  refs.frameId.textContent = frame?.id || '-';
  refs.frameSeq.textContent = frame?.seq == null ? '-' : String(frame.seq);
  refs.frameSize.textContent = frame?.size_bytes ? formatBytes(frame.size_bytes) : '-';
  refs.frameTime.textContent = frame?.captured_at_ms ? formatTime(frame.captured_at_ms) : '-';
  const pending = inputs.filter((event) => event.status === 'pending').length;
  const consumed = inputs.filter((event) => event.status === 'consumed').length;
  refs.inputState.textContent = `${pending} pending / ${consumed} consumed`;
  refs.commandState.textContent = command ? commandSummary(command) : '-';
  if (refs.commandHistory) {
    refs.commandHistory.innerHTML = commands.length
      ? commands.map((item) => `
          <div class="browser-command-row">
            <strong>${escapeHtml(item.command_type || item.type || 'browser')}</strong>
            <span>${escapeHtml(item.status || 'pending')}</span>
            <span>${escapeHtml(formatTime(item.updated_at_ms || item.created_at_ms))}</span>
            <span>${escapeHtml(commandErrorMessage(item) || item.record_id || '')}</span>
          </div>
        `).join('')
      : '';
  }
  if (refs.handoffHistory) {
    refs.handoffHistory.innerHTML = handoffTasks.length
      ? handoffTasks.map((item) => `
          <div class="browser-command-row">
            <strong>${escapeHtml(item.title || 'Browser context')}</strong>
            <span>${escapeHtml(item.status || item.route_status || 'queued')}</span>
            <span>${escapeHtml(formatTime(item.updated_at_ms))}</span>
            <span>${escapeHtml(item.id || item.command_id || '')}</span>
          </div>
        `).join('')
      : '';
  }
}

function commandSummary(command) {
  const type = command.command_type || command.type || 'browser';
  const status = command.status || 'pending';
  const error = commandErrorMessage(command);
  if (error) return `${type} failed: ${error}`;
  return `${type} ${status}`;
}

function commandErrorMessage(command) {
  if (!command) return '';
  const status = String(command.status || '').toLowerCase();
  const error = command.error || command.result?.error || command.payload?.error || '';
  if (status === 'failed' || error) return String(error || 'Die Browser-Aktion ist fehlgeschlagen.');
  return '';
}

function normalizeUrl(value) {
  const trimmed = String(value || '').trim();
  if (!trimmed) return 'https://example.com';
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed)) return trimmed;
  return `https://${trimmed}`;
}

function browserRouteArgs(params) {
  const value = params && typeof params === 'object' ? params : {};
  return value.args && typeof value.args === 'object' && !Array.isArray(value.args) ? value.args : value;
}

function browserSessionIdFromArgs(value) {
  const sessionId = String(value?.session_id || value?.sessionId || '').trim();
  return /^browser_session_[a-z0-9_-]+$/i.test(sessionId) ? sessionId : '';
}

// Fensterfokus app-lokal (env.surfaceFocused, gepflegt im mount): 4.0 hat keine Fokus-API (OFFEN-3).
function browserSurfaceIsFocused(env) {
  if (globalThis.document?.visibilityState === 'hidden') return false;
  if (globalThis.document?.hasFocus?.() === false) return false;
  return Boolean(env?.surfaceFocused);
}

// Warum die Bedienung gesperrt ist — im Klartext, nicht als Schweigen.
// Die Reihenfolge folgt der Pruefung in browserSurfaceCanControl.
function eingabeSperrgrund(env, state, now = Date.now()) {
  if (!browserSurfaceIsFocused(env)) return 'Das Browserfenster hat nicht den Fokus.';
  const session = state?.latestSession;
  if (!session?.id) return 'Keine aktive Sitzung.';
  const actorIds = browserActorIds(env?.actor);
  if (!actorIds.length) return 'Kein angemeldeter Nutzer.';
  if (!actorIds.includes(String(session.controller_user_id || ''))) {
    return 'Die Sitzung wird gerade von jemand anderem gesteuert.';
  }
  const leaseId = String(session.controller_lease_id || '').trim();
  if (!leaseId) return 'Keine Steuerungsberechtigung für diese Sitzung.';
  if (session.controller_lease_id !== state.controllerLeaseId) {
    return 'Die Steuerung wurde an ein anderes Fenster übergeben.';
  }
  const expiresAt = Number(session.controller_lease_expires_at_ms || 0);
  if (!Number.isFinite(expiresAt) || expiresAt <= now) {
    return 'Die Steuerung ist abgelaufen.';
  }
  return '';
}

function setzeEingabeHinweis(env, state, text) {
  if (!state) return;
  if (state.eingabeHinweis === text) return;
  state.eingabeHinweis = text;
  // eingabeHinweis wurde gesetzt und NIRGENDS gerendert — es gab nur die zwei
  // Zuweisungen in dieser Funktion. Der Nutzer sah deshalb exakt nichts: kein
  // Klick wirkte, keine Meldung erschien. Am 19.08.2026 auf managed production tenant
  // gemessen: jede Eingabe endete in "Die Steuerung wurde an ein anderes
  // Fenster uebergeben", sichtbar allein in der Browserkonsole. Der Grund
  // gehoert dorthin, wo der Nutzer hinsieht — in die Statuszeile.
  if (text) state.notice = text;
  else if (state.notice === state.letzteSperrmeldung) state.notice = '';
  renderNotice(env, state.notice);
}

// Gedrosselt: bei gehaltener Maus entstehen sonst hunderte gleiche Zeilen.
function meldeEingabeGesperrt(env, state, type) {
  const grund = eingabeSperrgrund(env, state) || 'Bedienung gesperrt.';
  const jetzt = Date.now();
  if (state.letzteSperrmeldung !== grund || jetzt - (state.letzteSperrmeldungMs || 0) > 5000) {
    console.warn('[browser] Eingabe gesperrt', { grund, type });
    state.letzteSperrmeldung = grund;
    state.letzteSperrmeldungMs = jetzt;
  }
  setzeEingabeHinweis(env, state, grund);
}

function browserSurfaceCanControl(env, state, now = Date.now()) {
  if (!browserSurfaceIsFocused(env)) return false;
  const session = state?.latestSession;
  const actorIds = browserActorIds(env?.actor);
  const expiresAt = Number(session?.controller_lease_expires_at_ms || 0);
  // Eine Anmeldesitzung des Web Stack darf JEDER Nutzer des Mandanten bedienen
  // (Owner-Entscheidung 04.09.2026). Die Pachtregeln bleiben unveraendert - wer
  // die Pacht haelt, steuert; die Pacht ist nur nicht mehr an EIN Konto
  // gebunden.
  const gemeinsameAnmeldung = String(session?.payload?.purpose || '') === 'web_stack_auth';
  const kontoPasst = gemeinsameAnmeldung
    || actorIds.includes(String(session?.controller_user_id || ''));
  return Boolean(
    session?.id
      && actorIds.length
      && kontoPasst
      && String(session.controller_lease_id || '').trim()
      && session.controller_lease_id === state.controllerLeaseId
      && Number.isFinite(expiresAt)
      && expiresAt > now
  );
}

function browserCommandRequiresController(commandType, session) {
  if (!session?.id) return false;
  return ![
    'browser.session.start',
    'browser.controller.acquire',
    'browser.observer.grant',
    'browser.observer.revoke',
  ].includes(commandType);
}

function shouldRenewControllerLease(session, actorId, now = Date.now(), options = {}) {
  const {
    documentVisible = true,
    documentFocused = true,
    surfaceFocused = true,
    renewInFlight = false,
    controllerLeaseId = '',
  } = options;
  if (!documentVisible || !documentFocused || !surfaceFocused || renewInFlight) return false;
  const actorIds = Array.isArray(actorId)
    ? actorId.map((value) => String(value || '')).filter(Boolean)
    : [String(actorId || '')].filter(Boolean);
  if (!session?.id || !actorIds.includes(String(session.controller_user_id || ''))) return false;
  if (!String(session.controller_lease_id || '').trim()) return false;
  if (session.controller_lease_id !== controllerLeaseId) return false;
  const expiresAt = Number(session.controller_lease_expires_at_ms || 0);
  // Eine ABGELAUFENE Pacht wird bewusst NICHT erneuert — der Server weist die
  // Erneuerung ab, und alle 30 s ein Fehlschlag waere eine Endlosschleife.
  // Der Wächter darunter haelt das fest. Fuer diesen Fall ist NEU HOLEN
  // zustaendig, nicht Erneuern: siehe shouldReacquireControllerLease.
  if (!Number.isFinite(expiresAt) || expiresAt <= now) return false;
  return expiresAt - now <= 75_000;
}

// Abgelaufene Pacht: neu HOLEN statt zu erneuern.
//
// Bis 13.08.2026 gab es diesen Weg nicht. Die Erneuerung schloss den
// abgelaufenen Zustand zu Recht aus, aber niemand holte die Pacht danach
// zurueck — der Zustand, der die Reparatur am dringendsten braucht, hatte
// keine. Zusammen mit der Bedingung "nur bei sichtbarem, fokussiertem Fenster"
// ergab das eine Falle ohne Ausgang: Fenster wegklicken, Pacht laeuft
// planmaessig ab, zurueckkommen — und sie wird nie wieder angefasst.
// Auf der Kundeninstanz gemessen: beide Sitzungen "active", beide Pachten
// abgelaufen, null Eingabe-Ereignisse in zehn Minuten aktiver Bedienung.
//
// Anders als die Erneuerung braucht das Neuholen KEIN fokussiertes Fenster —
// sonst bliebe genau die Falle bestehen. Es verlangt aber, dass die Sitzung
// uns gehoert, und es laeuft nur, wenn nicht schon ein Versuch unterwegs ist.
// Jeder stille Abbruch hier kostete Stunden Fehlersuche: die Sitzung meldete
// "Bereit", nichts geschah, und keine Zeile sagte warum. Der Grund wird jetzt
// EINMAL je Wechsel protokolliert (nicht bei jedem 30-s-Takt).
function merkeReacquireGrund(session, grund) {
  try {
    const schluessel = `${session?.id || '-'}|${grund}`;
    if (globalThis.__ctoxBrowserReacquireGrund === schluessel) return;
    globalThis.__ctoxBrowserReacquireGrund = schluessel;
    console.info(`[browser] Steuerung wird NICHT zurueckgeholt: ${grund}`, { session: session?.id || '' });
  } catch { /* Diagnose darf nie stoeren */ }
}

function shouldReacquireControllerLease(session, actorId, now = Date.now(), options = {}) {
  const { reacquireInFlight = false, lastReacquireAtMs = 0 } = options;
  if (reacquireInFlight) return false;
  // Nach einem Fehlschlag nicht sofort wieder — sonst entsteht genau die
  // Endlosschleife, die der Wächter der Erneuerung verhindert.
  if (now - Number(lastReacquireAtMs || 0) < 10_000) return false;
  const actorIds = Array.isArray(actorId)
    ? actorId.map((value) => String(value || '')).filter(Boolean)
    : [String(actorId || '')].filter(Boolean);
  if (!session?.id || !actorIds.length) {
    merkeReacquireGrund(session, session?.id ? 'kein angemeldeter Nutzer' : 'keine Sitzung');
    return false;
  }
  // Fremd gesteuerte Sitzungen nicht an uns reissen - AUSSER es ist eine
  // gemeinsame Anmeldesitzung des Web Stack UND ihre Pacht ist abgelaufen.
  // Dann wartet die Recherche auf irgendeinen Menschen, nicht auf einen
  // bestimmten (Owner-Entscheidung 04.09.2026). Eine LAUFENDE fremde Pacht
  // bleibt unangetastet, sonst reissen sich zwei Nutzer die Steuerung weg.
  const controller = String(session.controller_user_id || '');
  const gemeinsameAnmeldung = String(session.payload?.purpose || '') === 'web_stack_auth';
  const fremdePachtLaeuft = Number(session.controller_lease_expires_at_ms || 0) > now;
  if (controller && !actorIds.includes(controller)
    && !(gemeinsameAnmeldung && !fremdePachtLaeuft)) {
    merkeReacquireGrund(session, `Sitzung gehoert ${controller}`);
    return false;
  }
  // Eine tote Sitzung braucht einen START, keine Pacht. Ohne diese Zeile hielt
  // die Wiederbeschaffung genau den Zustand fest, den sie heilen sollte: am
  // 15.08.2026 gemessen 259 erfolgreiche acquire in 48 h auf einer Sitzung mit
  // status=disconnected. Die stets gueltige Pacht liess submitAddress den
  // Navigations-Zweig waehlen und machte den Start-Zweig unerreichbar.
  if (browserSessionNeedsStart(session)) {
    merkeReacquireGrund(session, 'Sitzung braucht einen Start, keine Pacht');
    return false;
  }
  const expiresAt = Number(session.controller_lease_expires_at_ms || 0);
  if (!Number.isFinite(expiresAt) || expiresAt <= now) return true;
  // Gleicher Nutzer, andere Pacht: ein anderes Fenster haelt die Steuerung —
  // meist ein totes, etwa der Tab von vor einem Dienstneustart. Bisher wurde
  // nur bei ABGELAUFENER Pacht zurueckgeholt, also sperrte sich das lebende
  // Fenster selbst aus, solange die tote Pacht noch gueltig war. Die Uebernahme
  // laeuft nur auf ausdrueckliche Interaktion (ein Klick IN dieses Fenster),
  // nie auf einem Zeitgeber — sonst reissen sich zwei offene Tabs die Steuerung
  // im Wechsel gegenseitig weg. Fremde Nutzer sind oben bereits ausgeschlossen.
  if (!options.uebernahmeDurchInteraktion) return false;
  const eigene = String(options.currentLeaseId || '').trim();
  const fremde = String(session.controller_lease_id || '').trim();
  return Boolean(fremde) && fremde !== eigene;
}

// Nach einem Neuladen ist state.controllerLeaseId leer, waehrend die Sitzung
// uns weiterhin als Steuernden mit gueltiger Pacht fuehrt. Die Erneuerung
// verlangt aber Gleichheit beider Kennungen, also erkennt die App ihre eigene
// Pacht nicht wieder und erneuert sie nie. Sie muss stattdessen jedes Mal den
// Ablauf abwarten und zurueckholen — und in genau diesem Fenster lehnt der
// Server jeden Bildabruf ab ("browser live controller lease is missing or
// expired"), weshalb die Flaeche leer bleibt. Auf der Kundeninstanz gemessen:
// eigene=- fremd=d52e78cc rest=-2s, ueber Minuten unveraendert, waehrend die
// Sitzung serverseitig lief.
//
// Nur die EIGENE, noch gueltige Pacht wird wiedererkannt. Eine abgelaufene
// bleibt Sache von shouldReacquireControllerLease, und eine fremde Pacht --
// auch die eines anderen Fensters desselben Nutzers -- bleibt der Uebernahme
// durch ausdrueckliche Interaktion vorbehalten.
function erkenneEigenePachtWieder(env, state) {
  if (state.controllerLeaseId) return;
  const session = state.latestSession;
  const leaseId = String(session?.controller_lease_id || '').trim();
  if (!leaseId) return;
  const actorIds = browserActorIds(env.actor);
  if (!actorIds.includes(String(session.controller_user_id || ''))) return;
  const expiresAt = Number(session.controller_lease_expires_at_ms || 0);
  if (!Number.isFinite(expiresAt) || expiresAt <= Date.now()) return;
  state.controllerLeaseId = leaseId;
}

// Umschalter zwischen Live-Ansicht und dem Automatisierungsskript dieser
// Sitzung. Das Skript wird pro Sitzung erzeugt und liegt neben dem Profil --
// es ist damit die einzige verlaessliche Antwort auf "was steuert diese
// Scraping-Sitzung gerade". Der Quellbaum kann davon abweichen: genau diese
// Abweichung kostete am 20.08.2026 einen Tag, weil der ausgelieferte Runner
// die Bildoperation gar nicht kannte, der Checkout aber schon.
//
// Bewusst nur lesend. Der Runner hat die Datei beim Start geladen; sie hier
// zu aendern wuerde nichts an der laufenden Sitzung aendern und nur den
// Eindruck erwecken, es taete es.
function installViewSwitch(env, refs, state) {
  const knoepfe = [...(refs.viewSwitch || [])];
  if (!knoepfe.length || !refs.scriptPanel) return;
  const zeige = async (ansicht) => {
    state.ansicht = ansicht;
    for (const k of knoepfe) {
      const aktiv = k.dataset.browserView === ansicht;
      k.classList.toggle('is-active', aktiv);
      k.setAttribute('aria-selected', aktiv ? 'true' : 'false');
    }
    const skript = ansicht === 'script';
    refs.scriptPanel.hidden = !skript;
    if (refs.canvas) refs.canvas.style.visibility = skript ? 'hidden' : '';
    if (refs.empty && skript) refs.empty.hidden = true;
    if (!skript) env.status.clear('browser.script.unavailable');
    if (!skript || !refs.scriptCode) return;
    const sessionId = state.latestSession?.id;
    if (!sessionId) {
      refs.scriptCode.textContent = '';
      return;
    }
    // Der Live-Kanal verlangt die Steuerungspacht fuer JEDE Operation, auch
    // fuer diese rein lesende. Ohne Vorabpruefung bekaeme der Betrachter hier
    // "controller lease is missing or expired" zu lesen -- fuer einen Klick auf
    // "Skript" eine Meldung, die niemand mit der Ursache verbindet.
    // Das Automatisierungsskript las v3 ueber den nativen Direktkanal (op 'script'). App-API 4.0 hat dafuer
    // keinen Weg (V4-PORT.md OFFEN-2): ehrlich melden statt einen Ladezustand vorzutaeuschen.
    refs.scriptCode.textContent = '';
    if (refs.scriptPath) refs.scriptPath.textContent = '';
    env.status.report({ level: 'info', code: 'browser.script.unavailable', text: t('scriptUnavailable', 'Das Automatisierungsskript ist in dieser Shell-Version nicht abrufbar.'), ref: sessionId });
  };
  for (const k of knoepfe) {
    k.addEventListener('click', () => { zeige(k.dataset.browserView || 'live'); });
  }
  zeige(state.ansicht === 'script' ? 'script' : 'live');
}

function tBrowser(env, schluessel, standard) {
  return t(schluessel, standard);
}

function newBrowserControllerLeaseId() {
  return globalThis.crypto?.randomUUID?.()
    || `browser-lease-${Date.now()}-${Math.random().toString(36).slice(2, 12)}`;
}

// api.actor (4.0: {id, display_name, role}); die v3-Form {user:{id,email,login}, userId} bleibt lesbar.
function browserActorIds(actor) {
  const user = actor?.user && typeof actor.user === 'object' ? actor.user : {};
  return [...new Set([
    actor?.id,
    user.id,
    user.email,
    user.login,
    actor?.userId,
  ].map((value) => String(value || '').trim()).filter(Boolean))];
}

function debounce(fn, delayMs) {
  let timer = null;
  const run = (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), delayMs);
  };
  run.cancel = () => clearTimeout(timer);
  return run;
}

// Zustandszone der Shell (R11): stabile Codes, damit clear greift.
function reportStatus(env, item) {
  if (!env?.api?.status) return;
  env.statusCodes?.add(item.code);
  const entry = { level: item.level, code: item.code, text: String(item.text || '') };
  if (item.ref) entry.ref = String(item.ref);
  env.api.status.report(entry);
}

function clearStatus(env, code) {
  if (!env?.api?.status) return;
  if (env.statusCodes && !env.statusCodes.has(code)) return;
  env.statusCodes?.delete(code);
  env.api.status.clear(code);
}

// Texteingabe ueber den Fensterdialog der Shell (R7, Form offen: OFFEN-A). Nicht-Text gilt als Abbruch.
async function askText(env, { title, text, value = '' } = {}) {
  const answer = await env.api.shell.dialog({ kind: 'prompt', title, text, value, default: null });
  if (typeof answer === 'string') return answer;
  if (answer && typeof answer === 'object' && typeof answer.value === 'string') return answer.value;
  return '';
}

function formatTime(ms) {
  try {
    return new Date(Number(ms)).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  } catch (_) {
    return '-';
  }
}

function formatBytes(bytes) {
  const value = Number(bytes || 0);
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

function titleCase(value) {
  const text = String(value || '').replace(/[_-]+/g, ' ');
  if (!text) return '';
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

export const __browserTestHooks = {
  SCRAPING_ADAPTER_COLLECTIONS,
  normalizeUrl,
  browserSessionIdFromArgs,
  browserRouteArgs,
  formatBytes,
  titleCase,
  userSessionPrefix,
  selectedViewport,
  browserAuthRequestFromArgs,
  shouldRenewControllerLease,
  shouldReacquireControllerLease,
  browserSitzungZustand,
  eingabeSperrgrund,
  browserCommandRequiresController,
  browserSurfaceIsFocused,
  browserSurfaceCanControl,
  browserActorIds,
  mergeRequestedSession,
  mergeRequestedDocument,
  applyLatestNavigationResult,
  browserSessionError,
  browserSessionIsLive,
  browserAddressAction,
  startSperrgrund,
  browserSessionNeedsStart,
  browserStartErrorIsRetryable,
  newBrowserControllerLeaseId,
  filterSessionsForView,
  browserSessionViewCounts,
  browserSessionBand,
  browserUiState,
  browserStatusLabel,
  frameEmptyText,
  sessionListSignature,
  browserWorkbenchVisible,
  browserSessionShardMeta,
  browserSessionShardDetail,
  browserSessionListMeta,
  sessionShardMarkup,
  syncViewToggle,
  buildBrowserSessionsExport,
  parseBrowserSessionsImport,
  sessionRenderList,
  sessionTabCounts,
  browserInputTrace,
  browserInputPayload,
  eventModifiers,
  pointerButton,
  pointerButtons,
  pointerClickCount,
  keyboardText,
  canvasPoint,
  submitBrowserNav,
  writePointerInput,
  writeKeyboardInput,
  writeInputEvent,
  installInputHandlers,
};

// Translate static markup: data-t (textContent) and data-t-aria (aria-label).
// German markup text is the fallback when a key is missing.
function applyTranslations(root) {
  root.querySelectorAll('[data-t]').forEach((el) => {
    if (el.dataset.tFallback == null) el.dataset.tFallback = el.textContent.trim();
    el.textContent = t(el.dataset.t, el.dataset.tFallback);
  });
  root.querySelectorAll('[data-t-aria]').forEach((el) => {
    if (el.dataset.tAriaFallback == null) el.dataset.tAriaFallback = el.getAttribute('aria-label') || '';
    el.setAttribute('aria-label', t(el.dataset.tAria, el.dataset.tAriaFallback));
  });
}
