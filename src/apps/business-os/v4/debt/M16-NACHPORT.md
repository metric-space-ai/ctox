# Zug 16m – V2-Fixes aus main, die im v4-Port nicht übernommen sind (Nachport-Liste)

Stand: Merge `origin/main` 2ce94bd22 in `v4/integration` d5e424dd2 (Sa 17.10.2026, Kern-Auftrag Zwischenzug 16m).
Regel: Für nach v4 portierte Apps blieb der v4-Stand; Blöcke, in denen main mehr hatte, stehen hier mit den main-only-Zeilen
(Stempel normalisiert). Vieles ist V2-Datenpfad (ctx/commandBus/mount(ctx)) und entfällt; UX-Verbesserungen sind nachzuportieren.

## `modules/ctox/index.js`

- Block bei Zeile 1 (main-only, 8 Zeilen):
  ```
  import { showBusinessAlert, showBusinessConfirm } from '../../shared/dialogs.js?v=20260816-browser-sync-guards-v141';
  import { renderListOrState } from '../../shared/list-state.js';
  import { crewCreatureHtml, syncCrewProceduralMotion, crewMemberExpression, crewMemberExpressionTtlMs } from '../../shared/business-chat.js?v=20261017-shell-v2-v
  import { canUseBusinessPermission, BusinessOsPermissions } from '../../shared/permissions.js?v=20260816-browser-sync-guards-v141';
  import { startCrewMotion } from '../../shared/crew-motion.js?v=20260928-crew-truth-v7';
  import { renderCrewReference, crewModeForTaskState } from '../../shared/crew-renderer.js?v=20260928-crew-truth-v7';
  import { workspaceDataState } from './data-state.js?v=20260906-data-state-v1';
  import { subscribeTaskHistoryChanges } from '../../shared/task-history-native-changes.js?v=20261017-shell-v2-v4-train16m';
  ```
- Block bei Zeile 46 (main-only, 1 Zeilen):
  ```
  const CTOX_STYLE_BUILD = '20261017-shell-v2-v4-train16m';
  ```
- Block bei Zeile 171 (main-only, 8 Zeilen):
  ```
      controlApplied: "Übernommen.",
      retryResubmitted: 'Neu gestartet: Die Aufgabe läuft als neuer Lauf.',
      taskActionRejected: 'Abgelehnt: {reason}',
      rejectNotFailed: 'Nur fehlgeschlagene oder blockierte Aufgaben lassen sich wiederholen.',
      rejectLeased: 'Die Aufgabe wird gerade bearbeitet. Warte, bis der Lauf endet.',
      rejectNotFound: 'Die Aufgabe gibt es nicht mehr.',
      rejectTimeout: 'Keine Antwort erhalten. Ob die Aktion übernommen wurde, ist unklar.',
      harnessRunning: "Läuft",
  ```
- Block bei Zeile 203 (main-only, 1 Zeilen):
  ```
      waitsFor: "wartet auf",
  ```
- Block bei Zeile 456 (main-only, 7 Zeilen):
  ```
      controlApplied: "Applied.",
      retryResubmitted: 'Restarted: the task runs again as a new run.',
      taskActionRejected: 'Rejected: {reason}',
      rejectNotFailed: 'Only failed or blocked tasks can be retried.',
      rejectLeased: 'The task is being worked on right now. Wait until the run ends.',
      rejectNotFound: 'This task no longer exists.',
      rejectTimeout: 'No answer received. It is unclear whether the action was applied.',
  ```
- Block bei Zeile 4162 (main-only, 21 Zeilen):
  ```
  async function dispatchCtoxTaskMutation(state, { commandType, payload, commandPath }) {
    if (!state.ctx?.commandBus?.dispatch) {
      throw new Error('Die Verbindung zur Crew ist nicht verfügbar.');
    }
    const commandId = `cmd_${commandType.replace(/[^a-z0-9]+/gi, '_')}_${crypto.randomUUID()}`;
    // Wait for the native outcome: "accepted" resolves as soon as the intent is
    // replicated, so a rejected retry used to report "Übernommen." and then do
    // nothing (Welsch, 15.10.2026).
    return state.ctx.commandBus.dispatch({
      id: commandId,
      module: 'ctox',
      command_type: commandType,
  … (+9 Zeilen)
  ```
- Block bei Zeile 6065 (main-only, 21 Zeilen):
  ```
    buttons.forEach((el) => el.setAttribute('disabled', 'disabled'));
    if (status) status.textContent = '';
    try {
        if (!state.ctx?.commandBus?.cancel) throw new Error('Die Verbindung zur Crew ist nicht verfügbar.');
        await state.ctx.commandBus.cancel(task.commandId, { reason: t.cancelReasonDefault, until: 'accepted' });
        await dispatchCtoxTaskMutation(state, { commandType: 'ctox.queue.block', payload: { task_id: taskId, reason: t.blockReasonDefault }, commandPath: 'ctox_qu
        await dispatchCtoxTaskMutation(state, { commandType: 'ctox.queue.release', payload: { task_id: taskId }, commandPath: 'ctox_queue_release' });
        const receipt = await dispatchCtoxTaskMutation(state, { commandType: 'ctox.queue.retry', payload: { task_id: taskId }, commandPath: 'ctox_queue_retry' });
        if (receipt?.result?.resubmitted) {
          if (status) status.textContent = t.retryResubmitted;
          refresh(state).catch(() => {});
          return;
  … (+9 Zeilen)
  ```

## `modules/news/index.js`

- Block bei Zeile 1 (main-only, 3 Zeilen):
  ```
  import { applyPaneIconLabels } from '../../shared/pane-icon-labels.js';
  import { loadModuleMessages } from '../../shared/i18n.js';
  import { renderHtmlIfChanged } from '../../shared/stable-dom.js';
  ```
- Block bei Zeile 248 (main-only, 14 Zeilen):
  ```
  export async function mount(ctx) {
    state.ctx = ctx;
    state.disposed = false;
    state.lang = ctx.locale === 'en' ? 'en' : 'de';
    state.messages = await loadModuleMessages(import.meta.url, state.lang, labels);
    ensureStyles();
    ctx.host.innerHTML = await loadModuleMarkup();
    bindElements(ctx.host);
    applyPaneIconLabels(ctx.host, state.lang);
    wireEvents();
    await loadAll();
    subscribeAll();
  … (+2 Zeilen)
  ```
- Block bei Zeile 676 (main-only, 10 Zeilen):
  ```
    setFooter(els.leftPane, topics.length === 1 ? t('topicsFooterOne') : t('topicsFooter', topics.length));
    if (!state.topics.length) {
      renderHtmlIfChanged(els.topicList, `
        <div class="ctox-empty news-empty">
          <strong>${esc(t('emptyTopicsTitle'))}</strong>
          <span>${esc(t('emptyTopicsBody'))}</span>
          <button class="ctox-button is-primary" type="button" data-action="create-topic">${esc(t('createTopic'))}</button>
        </div>`);
      return;
    }
  ```
- Block bei Zeile 1041 (main-only, 12 Zeilen):
  ```
    const freshness = issueWindow(topic, state.issues, state.posts, now);
    const command = buildGenerateCommand(topic, state.sources, issue, previous, freshness);
    issue.command_id = command.command_id;
    try {
      await issuesHandle.insert(issue);
        await dispatch(command);
      notify('success', t('commandQueued'));
    } catch (error) {
      const doc = await issuesHandle.findOne(issue.id).exec().catch(() => null);
      await doc?.incrementalPatch({ status: 'failed', error: String(error?.message || error), updated_at_ms: Date.now() }).catch(() => {});
      notify('error', t('commandFailed'), error?.message);
    }
  ```
- Block bei Zeile 1421 (main-only, 21 Zeilen):
  ```
    if (!(await persistDrawer(topic))) return;
    closeDrawer();
    state.selectedTopicId = topic.id;
  // Write the drawer state (topic + sources) and re-dispatch the schedule when
  // it changed. Shared by "Thema anlegen", instant save and undo.
  async function persistDrawer(topic, sourceList = state.drawer?.sources || [], removed = state.drawer?.removed || new Set()) {
    const topics = collection(COLLECTIONS.topics);
    const sources = collection(COLLECTIONS.sources);
    if (!topics || !sources) {
      notify('error', t('saveFailed'));
      return false;
    try {
  … (+9 Zeilen)
  ```
- Block bei Zeile 1468 (main-only, 80 Zeilen):
  ```
    if (scheduleChanged) {
      dispatch(buildScheduleCommand(topic)).catch((error) => notify('error', t('commandFailed'), error?.message));
    }
    // The digest follows the topic: only a daily, active topic keeps it.
    if (digestActive(topic) !== Boolean(previous && digestActive(previous)) || (digestActive(topic) && !previous)) {
      dispatch(buildScheduleCommand(topic, { digest: true })).catch((error) => notify('error', t('commandFailed'), error?.message));
    }
    return true;
  }
  // --- Instant save + undo (8 s), same pattern as the weekly digest ----------
  const DRAWER_UNDO_MS = 8000;
  function syncDrawerFoot() {
  … (+68 Zeilen)
  ```
- Block bei Zeile 1577 (main-only, 7 Zeilen):
  ```
    try {
      await dispatch(buildScheduleCommand(topic, { deleted: true }));
      if (digestOf(topic).enabled) await dispatch(buildScheduleCommand(topic, { deleted: true, digest: true }));
    } catch (error) {
      notify('error', t('commandFailed'), error?.message);
      return;
    }
  ```
- Block bei Zeile 1631 (main-only, 3 Zeilen):
  ```
          await topics.upsert(topic);
          dispatch(buildScheduleCommand(topic)).catch(() => {});
          if (digestActive(topic)) dispatch(buildScheduleCommand(topic, { digest: true })).catch(() => {});
  ```

## `modules/news/core/news-model.mjs`

- Block bei Zeile 472 (main-only, 52 Zeilen):
  ```
  const HOUR_MS = 3_600_000;
  const DAY_MS = 24 * HOUR_MS;
  // Same canonical link form as the native writeback (store_news_commands.rs).
  export function normalizeStoryUrl(url) {
    let rest = String(url || '').trim().split('#')[0];
    rest = rest.includes('://') ? rest.split('://').slice(1).join('://') : rest;
    const slash = rest.indexOf('/');
    let host = (slash >= 0 ? rest.slice(0, slash) : rest).toLowerCase();
    if (host.startsWith('www.')) host = host.slice(4);
    const path = (slash >= 0 ? rest.slice(slash + 1) : '').replace(/\/+$/, '');
    return path ? `${host}/${path}` : host;
  }
  … (+40 Zeilen)
  ```
- Block bei Zeile 543 (main-only, 2 Zeilen):
  ```
      instruction: buildIssueInstruction(topic, topicSources, previousIssue, commandId, window),
      required_skills: [NEWS_SKILL],
  ```

## `modules/threads/index.js`

- Block bei Zeile 1 (main-only, 4 Zeilen):
  ```
  import { applyPaneIconLabels } from '../../shared/pane-icon-labels.js';
  import { loadModuleMessages } from '../../shared/i18n.js';
  import { renderHtmlIfChanged } from '../../shared/stable-dom.js';
  import { collectUniquePages } from './paging.js';
  ```
- Block bei Zeile 17 (main-only, 33 Zeilen):
  ```
  // Lesbare Betreffzeile statt roher IDs (workjet#801): UUIDs und Befehls-IDs wie
  // `cmd_ctox_queue_retry_f89eb202-…` sind keine Titel.
  const RAW_ID_TITLE = /^(?:[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|(?:cmd|task|thread|thr|msg|req|apr)_[a-z0-9_]*[0-9a-f]{6,}[0-9a-f-]*)$/i;
  export function looksLikeRawId(value) {
    return RAW_ID_TITLE.test(String(value || '').trim());
  }
  const RAW_ID_KIND_LABELS = {
    de: { queue_retry: 'Erneuter Versuch eines Auftrags', approval: 'Freigabe', ctox: 'CTOX-Vorgang', default: 'Abstimmung' },
    en: { queue_retry: 'Task retry', approval: 'Approval', ctox: 'CTOX item', default: 'Thread' },
  };
  function threadsLang() {
    return state?.ctx?.locale === 'en' ? 'en' : 'de';
  … (+21 Zeilen)
  ```
- Block bei Zeile 192 (main-only, 5 Zeilen):
  ```
    applyPaneIconLabels(ctx.host, ctx.locale === 'en' ? 'en' : 'de');
    wireUi();
    const onAppLaunch = (event) => {
      const args = event?.detail?.args || {};
      const threadId = String(args.thread_id || args.thread || '').trim();
  ```
- Block bei Zeile 1009 (main-only, 1 Zeilen):
  ```
          data-context-label="${escapeAttr(threadDisplayTitle(thread, threadsLang()))}"
  ```
- Block bei Zeile 1021 (main-only, 1 Zeilen):
  ```
              <span class="threads-list-title" title="${escapeHtml(threadDisplayTitle(thread, threadsLang()))}">${escapeHtml(threadDisplayTitle(thread, threadsLan
  ```
- Block bei Zeile 1075 (main-only, 1 Zeilen):
  ```
    if (els.title) els.title.textContent = threadDisplayTitle(thread, threadsLang());
  ```

## `modules/threads/index.css`

- Block bei Zeile 958 (main-only, 5 Zeilen):
  ```
  .threads-card-actions.is-decide { top: 40px; }
  /* Keine Zahlen hinter Tabs (UI-Regel, workjet#801). Die Knoten bleiben für den Pane-Grammar-Vertrag. */
  .ctox-pane-tab .view-count {
    display: none;
  }
  ```

## `modules/app-store/index.js`

- Block bei Zeile 1128 (main-only, 1 Zeilen):
  ```
      const mod = await import('../../vendor/store-shelf/store-shelf.mjs?v=20261017-shell-v2-v4-train16m');
  ```

## `customer-modules/outbound-lead-generation/index.js`

- Block bei Zeile 11 (main-only, 15 Zeilen):
  ```
  // Rueckfragen und Hinweise IMMER im eigenen Fenster. Die Shell merkt sich das
  // Dialogziel global; nach einem Wechsel zu einer anderen App (z. B.
  // "Zugaenge") und zurueck landete jeder Dialog der Outbound-App unsichtbar im
  // Fenster darunter und wartete dort auf eine Antwort — die App wirkte
  // blockiert (Klicktest P0e, Owner-Befund 11.09.2026 abends).
  function eigenesDialogZiel() {
    const host = globalThis.__olgDialogZiel;
    return host && host.isConnected ? host : undefined;
  }
  function showBusinessAlert(message, options = {}) { return shellAlert(message, { host: eigenesDialogZiel(), ...options }); }
  function showBusinessConfirm(message, options = {}) { return shellConfirm(message, { host: eigenesDialogZiel(), ...options }); }
  function showBusinessPrompt(message, options = {}) { return shellPrompt(message, { host: eigenesDialogZiel(), ...options }); }
  … (+3 Zeilen)
  ```
- Block bei Zeile 800 (main-only, 23 Zeilen):
  ```
    bootSchritt('reload');
    // Die Leads hat die erste Ladung schon vollstaendig vom Server gelesen;
    // ein zweites Blaettern ueber alle Leads kostete den Start 10-40 s.
    await pflegeSchritt('reload-0', () => reload(Object.keys(state.collections)
      .filter((key) => key !== 'leads' || !state.collectionsEverLoaded?.has('leads'))));
    if (!listLeads().length) planeLeerNachladen();
    // Reparatur- und Abgleichsroutinen schreiben ganze Datensaetze. Auf einem
    // noch nicht live abgeglichenen Stand schrieben sie alte Staende zurueck
    // (needs_review -> failed, CH -> DE; 25.09.2026). Sie laufen deshalb nur,
    // wenn alle Sammlungen live sind; sonst beim naechsten Start.
    if (!nichtLive.length) {
      // Nachladen nur, wenn ein Schritt etwas geaendert hat: jedes reload sind
  … (+11 Zeilen)
  ```
- Block bei Zeile 1368 (main-only, 134 Zeilen):
  ```
  // Start, the first scheduled reload and the collection reloader each asked for
  // the full lead list at the same moment; every call paged all leads with its
  // own revision token, so the native peer served the same 200-row pages three
  // times (thesen 07.10.2026: 2.3/4.4/5.7 s for one page). Callers that arrive
  // while a list load of the same binding is running share it. The whole paged
  // list (≈7 pages over a relayed WebRTC link) also outgrew the 15 s single-query
  // budget, was aborted and started over; give it 45 s.
  const LEAD_LIST_LOAD_TIMEOUT_MS = 45000;
  function sharedLeadListLoad(collection, previousLeads, bindingGeneration) {
    const running = state.leadListLoad;
    if (running && running.bindingGeneration === bindingGeneration) return running.promise;
    // Mit lokalem Speicher braucht die Liste keinen CTOX-Lesekanal (client first).
  … (+122 Zeilen)
  ```
- Block bei Zeile 1625 (main-only, 50 Zeilen):
  ```
    const generation = state.collectionBindingGeneration;
    const requested = [...new Set(ids.filter(Boolean))];
    const summaries = new Map(listLeads().map(row => [row.id, row]));
    const cached = new Map(state.leads.map(row => [row.id, row]));
    let missing = requested.filter(id => fresh || !cached.has(id) || cached.get(id)._rev !== summaries.get(id)?._rev);
    if (!missing.length) return requested.map(id => cached.get(id));
    const sequence = ++state.fullLeadReadSequence;
    // Anzeige liest zuerst den lokalen Speicher (client first); nur fehlende oder
    // veraltete Leads gehen an CTOX. Aktionen (fresh) lesen weiter autoritativ.
    const stored = fresh ? new Map() : await readStoredLeadRows(state.collections.leads, missing);
    const local = missing.map(id => stored.get(id))
      // Der lokale Speicher fuehrt kein _rev; ein lokaler Lead gilt, solange die
  … (+38 Zeilen)
  ```
- Block bei Zeile 3457 (main-only, 1 Zeilen):
  ```
            <button class="ctox-button is-primary" data-action="save-lead-editor" ${state.leadEditorSaving ? 'disabled' : ''}>${state.leadEditorSaving ? 'Wird ges
  ```
- Block bei Zeile 3889 (main-only, 1 Zeilen):
  ```
    const host = state.ctx?.host;
  ```
- Block bei Zeile 7011 (main-only, 12 Zeilen):
  ```
  // Lesen und Schreiben direkt im lokalen Speicher (wie RxDocument.incrementalPatch);
  // find().exec() ging bis zur Shell-Freigabe an CTOX und hing (15.10.2026).
  async function patchStoredDoc(collection, doc, fields) {
    const now = Number(fields?.updated_at_ms || Date.now());
    await collection.storageCollection.upsert({
      ...doc, ...fields, updated_at_ms: now, _meta: { ...(doc._meta || {}), lwt: now },
    });
  }
    const adapters = state.collections.adapters;
    if (typeof adapters?.storageCollection?.allDocuments !== 'function') return false;
    const docs = await adapters.storageCollection.allDocuments();
      if (ids.has(doc.id)) await patchStoredDoc(adapters, doc, { _deleted: true, is_deleted: true });
  ```
- Block bei Zeile 7041 (main-only, 3 Zeilen):
  ```
    const adapters = state.collections.adapters;
    const docs = (await adapters.storageCollection.allDocuments()).filter((doc) => doc.source_id === sourceId);
    for (const doc of docs) await patchStoredDoc(adapters, doc, { _deleted: true, is_deleted: true });
  ```
- Block bei Zeile 7056 (main-only, 1 Zeilen):
  ```
    const confirmed = await showBusinessConfirm(
  ```
- Block bei Zeile 7074 (main-only, 3 Zeilen):
  ```
      const sources = state.collections.sources;
      const current = (await sources.storageCollection.findDocumentsById([sourceId]))[sourceId];
        await patchStoredDoc(sources, current, {
  ```
- Block bei Zeile 7093 (main-only, 3 Zeilen):
  ```
    const sourceDoc = await state.collections.sources.findOne(sourceId).exec();
    const geloeschterStand = sourceDoc ? (sourceDoc.toJSON?.() || sourceDoc) : null;
    if (sourceDoc) await sourceDoc.remove();
  ```
- Block bei Zeile 7989 (main-only, 4 Zeilen):
  ```
    // Die Maske darf erst nach einer bestaetigten Schreiboperation schliessen.
    // Scheitert der vorausgehende native Read (z. B. RATE_LIMITED), bleibt der
    // Entwurf samt Ansprechpartner im Editor fuer einen erneuten Versuch erhalten.
    zeigeHinweis(`„${name}“ wird gespeichert …`, 0);
  ```
- Block bei Zeile 8033 (main-only, 1 Zeilen):
  ```
      await showBusinessAlert(`Die Änderungen an „${name}“ konnten nicht gespeichert werden: ${String(error?.message || error)}`);
  ```
- Block bei Zeile 11714 (main-only, 1 Zeilen):
  ```
    const doc = await state.collections.leads.findOne(id).exec();
  ```
- Block bei Zeile 13265 (main-only, 1 Zeilen):
  ```
    freitextPruefungPrompt,
  ```

## `customer-modules/outbound-lead-generation/index.css`

- Block bei Zeile 472 (main-only, 15 Zeilen):
  ```
  .leadgen-table { width: 100%; border-collapse: collapse; table-layout: fixed; }
  .leadgen-table th { position: sticky; top: 0; z-index: 1; height: 34px; padding: 0 10px; border-bottom: 1px solid var(--line); background: var(--surface-2); col
  .leadgen-table th:nth-child(2) { width: 38%; }
  .leadgen-table th:nth-child(3) { width: 20%; }
  .leadgen-table th:nth-child(4) { width: 22%; }
  .leadgen-table td { height: 54px; padding: 7px 10px; border-bottom: 1px solid var(--line); vertical-align: middle; }
  .leadgen-table tbody tr { cursor: pointer; }
  .leadgen-table tbody tr:hover, .leadgen-table tbody tr[aria-selected="true"] { background: var(--accent-soft); }
  .leadgen-table tbody tr[data-checked="true"] { box-shadow: inset 3px 0 0 var(--accent); }
  .leadgen-table td:nth-child(2) > strong, .leadgen-table td:nth-child(2) > span { display: block; }
  .leadgen-table td:nth-child(2) > span { margin-top: 2px; }
  .leadgen-table td:nth-child(2) span { color: var(--muted); font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  … (+3 Zeilen)
  ```
- Block bei Zeile 1048 (main-only, 5 Zeilen):
  ```
  .leadgen-table tbody tr[data-checked="true"] { box-shadow: inset 3px 0 0 var(--accent); }
  .leadgen-table tbody tr[aria-selected="true"][data-checked="true"] { box-shadow: inset 3px 0 0 var(--accent); }
  .leadgen-table td:nth-child(2) > span { margin-top: 3px; }
  .leadgen-table td:nth-child(2) strong { font-weight: 650; letter-spacing: -.01em; }
  .leadgen-table td:nth-child(2) span { font-size: 10px; }
  ```

## `scripts/tests/outbound-import-app.test.mjs`

- Block bei Zeile 10 (main-only, 8 Zeilen):
  ```
  const fixture=mkdtempSync(join(tmpdir(),'outbound-import-app-'));
  const source=fileURLToPath(new URL('../../customer-modules/outbound-lead-generation/',import.meta.url));
  mkdirSync(join(fixture,'modules','olg'),{recursive:true});
  mkdirSync(join(fixture,'shared'),{recursive:true});
  writeFileSync(join(fixture,'package.json'),'{"type":"module"}');
  for(const name of ['index.js','collection-reloader.mjs','lead-revision-loader.mjs','lead-list-loader.mjs','import-preview-groups.js','current-state-export.mjs',
    copyFileSync(join(source,name),join(fixture,'modules','olg',name));
  writeFileSync(join(fixture,'shared','universal-importer.js'), `
  ```

## In v4 gelöscht, auf main weiterentwickelt (DU-Fälle, gelöscht gelassen)

- `customer-modules/outbound-lead-generation/lead-list-loader.mjs` – main: 8354b96be (v1.0.319 Lead-Details zuerst aus dem lokalen Store), 04cd69ccd (v1.0.321 Leadliste aus dem lokalen Store), 4807ab528 (Kommentar ohne Kundennamen). v4 lädt Leads über api.query (V4-PORT.md); fachlich prüfen, ob „lokal zuerst“ in v4 schon gilt.
- `scripts/tests/outbound-scoped-reload.test.mjs` – main: 9d6e63bdd (Lead-Änderungen bei Lesefehler behalten), a60b8eb7f (Lead-Details im gestapelten Layout sichtbar scrollen), 33488a0c4 (v1.0.323 doppelte Alt-Adapter beim Start entfernen), #725. V2-Test des alten Ladepfads; Verhalten für v4 nachprüfen (v4-api-Test der OLG).

## Übernommen statt nachportiert

- `modules/threads/index.css`: Regel „keine Zahlen hinter Tabs“ (workjet#801) auf `[data-app="threads"]` eingeschränkt übernommen.
- `shared/sync.js`: v4-Seite behalten (Fähigkeits-Token aus `kernel/session.mjs`, V3-Kompatibilität bis Cutover); main nutzte noch `command-bus.js`.


## Zweite Runde (Gate ROT auf 1b7b40b55): automatisch übernommene main-Teile in portierten Apps zurückgenommen

git hatte main-Änderungen außerhalb der Konfliktblöcke in diese v4-Dateien gemischt (V2-Code im v4-Modul, news startete nicht). Sie stehen jetzt komplett auf d5e424dd2; die main-Änderung je Datei ist `git diff 4794af03d 2ce94bd22 -- <datei>` (Nachport):

- `customer-modules/outbound-lead-generation/index.css` (+5 −2)
- `customer-modules/outbound-lead-generation/index.js` (+106 −177)
- `modules/app-store/index.js` (+1 −1)
- `modules/ctox/index.js` (+37 −8)
- `modules/news/core/news-model.mjs` (+88 −9)
- `modules/news/index.css` (+14 −0)
- `modules/news/index.html` (+17 −13)
- `modules/news/index.js` (+306 −21)
- `modules/news/locales/de.json` (+11 −1)
- `modules/news/locales/en.json` (+11 −1)
- `modules/news/tests/news.test.mjs` (+51 −0)
- `modules/threads/index.css` (+5 −0)
- `modules/threads/index.html` (+9 −9)
- `modules/threads/index.js` (+37 −4)

Nicht zurückgenommen (bewusst): `customer-modules/outbound-lead-generation/module.json` steht auf dem main-Stand aus dem Merge – Kern E60(b): Unterschied nur die Version (1.0.317 → 1.0.323), Versionen steigen nur; sonst unangetastet. Neue main-Dateien der OLG (`adapter-dedupe.mjs`, `stacked-detail-reveal.mjs` und ihre `scripts/tests/*`) bleiben; der v4-Stand von `index.js` importiert sie nicht (V2-Funktionen, Nachport).

## Befund unter Last

- Mail-QA `editor.undo-redo`: im parallelen Gate-Lauf fehlte der letzte getippte Buchstabe dauerhaft („…Kampagneninhal“), allein 2/2 grün. Verdacht: EasyEmail-`debounce(200)` verliert unter Last den letzten Tastendruck. Eigener Punkt nach Zug 16.

## Nachport-Stand (Sa 17.10. 14:xx)

- **news/threads b325ef17c „lesbare Thread-Titel statt IDs“**: in v4 bereits abgedeckt – `modules/threads/naming.js`
  (`isIdentifier` filtert UUIDs, `cmd_`/`thread_`-IDs, Hashes; Name aus Aufgabe/Befehl/Projekt). Kein Nachport nötig.
- **news/threads b325ef17c „Icon + Wort in Kopfleisten“ (workjet#801, `shared/pane-icon-labels.js`)**: 1:1-Port geprüft und
  verworfen. Mit Kurzwörtern reichen die Kopfknöpfe in v4 unter den Knopfcluster des Fensters (G8 reservierte Zone, threads
  900 px: 2 Knöpfe), die threads-Suche schrumpft auf 98 px (v4-api „linker Kopf“ rot), der Titel „Threads“ wird bei 420 px
  abgeschnitten (G4); news v4-api 2 Tests im Zeitlimit. Braucht eine Gestaltungsentscheidung für v4-Kopfleisten (z. B. Wort
  nur ab Fensterbreite X oder Überlaufmenü) – gilt dann für alle Apps, nicht nur news/threads.
- **news #799 / 9f6ad058a / #800**: nachportiert (b62ffc4fd Frische-Fenster + Dedupe über Ausgaben, 3fdd00c2b Sofort-Speichern +
  8-s-Rückgängig im Themen-Dialog + Laufvertrag, f66534cb2 Wochen-Digest mit eigenem Zeitplan `news-digest:<topic>`).
  Bewusst weggelassen: Checkpoint-Datei im Laufvertrag (v4-Handler haben kein Dateisystem), HN-Zitate/Trend-Kürzung aus dem
  V2-Writeback (außerhalb der drei Funktionen).
