import { openUniversalImporter } from 'ctox:shared/universal-importer.js';
import { autoWirePaneGrammar } from 'ctox:shared/pane-grammar.js';
import { buildContextChatCommand } from 'ctox:shared/context-chat.js';
import { getActionIcon } from 'ctox:shared/icons.js';
import { createMailContentEditor } from './editor/mail-content-editor.mjs';
import { mountMailBody } from './lib/mail-body-renderer.mjs';
import {
  appendMailContentRevision,
  applyContentRevisionToPending,
  buildMailContentRevision,
  buildMailGroupRecord,
  deriveMailGroupProgress,
  emailGroupDescriptor,
  mailGroupTemplate,
} from './mail-group-model.mjs';

const MAIL_PAGE_SIZE = 50;
const COMPOSER_FIELDS = ['composeFrom', 'composeTo', 'composeCampaign', 'composeSubject', 'composeBody'];
const MAIL_COLLECTIONS = [
  'business_commands',
  // Weiterleitungsziele: App-Liste der Plattform (business_apps ersetzt den V2-business_module_catalog, E48).
  'business_apps',
  'business_users',
  'communication_accounts',
  'communication_threads',
  'communication_messages',
  'outbound_campaigns',
  'outbound_pipeline_items',
  'outbound_engagements',
  'outbound_messages',
  'outbound_approvals',
  // Plattform-Projektion des Mailservers (Kern §16.28, ctox-platform; lesen nur mit Mailserver-Recht, sonst forbidden
  // → Collection fehlt, Verwaltung bleibt leer). Dokumente kind=runtime|domain|user, nie Passwörter/Secrets.
  'mail_server_state',
];
const TERMINAL_COMMAND_STATUSES = new Set(['completed', 'failed', 'cancelled', 'blocked']);
const DRAFT_SEND_STATUSES = new Set([
  '',
  'not_scheduled',
  'draft',
  'awaiting_approval',
  'approved_not_sent',
  'paused',
]);

const FALLBACK_LABELS = {
  de: {
    workspace: 'Arbeitsbereich',
    mail: 'Mail',
    compose: 'Neue Mail',
    keepDraftTitle: 'Entwurf behalten?',
    keepDraft: 'Als Entwurf speichern',
    discardDraft: 'Verwerfen',
    ok: 'OK',
    mailbox: 'Postfach',
    allMailboxes: 'Alle sichtbaren Postfächer',
    search: 'Mails durchsuchen',
    folders: 'Ordner',
    inbox: 'Posteingang',
    unread: 'Ungelesen',
    unknownPerson: 'Unbekannt',
    sent: 'Gesendet',
    drafts: 'Entwürfe',
    groups: 'E-Mail-Gruppen',
    messages: 'Nachrichten',
    thread: 'Thread',
    campaign: 'E-Mail-Gruppe',
    reply: 'Antworten',
    openOutbound: 'In Outbound öffnen',
    requestApproval: 'Zur Freigabe',
    approve: 'Freigeben',
    send: 'Senden',
    pause: 'Pausieren',
    resume: 'Fortsetzen',
    from: 'Von',
    to: 'An',
    group: 'E-Mail-Gruppe',
    ungrouped: 'Neue Gruppe automatisch',
    subject: 'Betreff',
    message: 'Nachricht',
    saveDraft: 'Als Entwurf speichern',
    savedDraft: 'Entwurf gespeichert.',
    approvalRequested: 'Entwurf wurde zur Freigabe eingereicht.',
    approved: 'Nachricht wurde freigegeben.',
    queued: 'Nachricht wurde in die Mailserver-Queue eingereiht.',
    missingRecipient: 'Bitte eine gültige Empfängeradresse eingeben.',
    missingSender: 'Bitte ein Absenderpostfach auswählen.',
    missingContent: 'Betreff oder Nachricht darf nicht leer sein.',
    commandFailed: 'Die Mail-Aktion ist fehlgeschlagen.',
    newGroup: 'Neue Gruppe',
    groupName: 'Name',
    groupDescription: 'Beschreibung',
    createGroup: 'Gruppe erstellen',
    groupCreated: 'Mailgruppe wurde erstellt.',
    groupNameRequired: 'Bitte einen Gruppennamen eingeben.',
    total: 'Gesamt',
    awaiting: 'Freigabe',
    failed: 'Fehler',
    active: 'Aktiv',
    paused: 'Pausiert',
    accountShared: 'Geteilt',
    accountPersonal: 'Zugewiesen',
    manageMailboxes: 'Postfächer verwalten',
    administration: 'Administration',
    mailserverDomains: 'Mailserver-Domains',
    createMailbox: 'Postfach anlegen',
    emailAddress: 'E-Mail-Adresse',
    ownerUser: 'Nutzer-ID oder Login',
    initialPassword: 'Initialpasswort',
    existingMailboxes: 'Vorhandene Postfächer',
    mailboxSecurity: 'Das Passwort wird nach der Einrichtung aus dem Auftrag entfernt.',
    reload: 'Neu laden',
    assignedTo: 'Zugeordnet zu',
    unassigned: 'Nicht zugeordnet',
    delete: 'Löschen',
    confirmDelete: 'Wirklich löschen',
    cancel: 'Abbrechen',
    mailboxRequired: 'Bitte eine gültige E-Mail-Adresse und ein Passwort eingeben.',
    mailboxCreated: 'Postfach wurde angelegt und dem Nutzer zugeordnet.',
    mailboxDeleted: 'Postfach wurde gelöscht.',
    mailboxAdminDenied: 'Nur Administratoren dürfen Postfächer verwalten.',
  },
  en: {
    workspace: 'Workspace',
    mail: 'Mail',
    compose: 'New email',
    keepDraftTitle: 'Keep draft?',
    keepDraft: 'Save as draft',
    discardDraft: 'Discard',
    ok: 'OK',
    mailbox: 'Mailbox',
    allMailboxes: 'All visible mailboxes',
    search: 'Search mail',
    folders: 'Folders',
    inbox: 'Inbox',
    unread: 'Unread',
    unknownPerson: 'Unknown', // v4-allow: en
    sent: 'Sent',
    drafts: 'Drafts',
    groups: 'Email groups',
    messages: 'messages',
    thread: 'Thread',
    campaign: 'Email group',
    reply: 'Reply',
    openOutbound: 'Open in Outbound',
    requestApproval: 'Request approval',
    approve: 'Approve',
    send: 'Send',
    pause: 'Pause',
    resume: 'Resume',
    from: 'From',
    to: 'To',
    group: 'Email group',
    ungrouped: 'Create a new group automatically',
    subject: 'Subject',
    message: 'Message',
    saveDraft: 'Save draft',
    savedDraft: 'Draft saved.',
    approvalRequested: 'Draft was submitted for approval.',
    approved: 'Message approved.',
    queued: 'Message queued in the mailserver.',
    missingRecipient: 'Enter a valid recipient address.',
    missingSender: 'Select a sender mailbox.',
    missingContent: 'Subject or message must not be empty.',
    commandFailed: 'The mail action failed.',
    newGroup: 'New group',
    groupName: 'Name',
    groupDescription: 'Description',
    createGroup: 'Create group',
    groupCreated: 'Mail group created.',
    groupNameRequired: 'Enter a group name.',
    total: 'Total',
    awaiting: 'Approval',
    failed: 'Failed',
    active: 'Active',
    paused: 'Paused',
    accountShared: 'Shared',
    accountPersonal: 'Assigned',
    manageMailboxes: 'Manage mailboxes',
    administration: 'Administration',
    mailserverDomains: 'Mailserver domains',
    createMailbox: 'Create mailbox',
    emailAddress: 'Email address',
    ownerUser: 'User ID or login',
    initialPassword: 'Initial password',
    existingMailboxes: 'Existing mailboxes',
    mailboxSecurity: 'The password is removed from the request after setup.',
    reload: 'Reload',
    assignedTo: 'Assigned to',
    unassigned: 'Not assigned',
    delete: 'Delete',
    confirmDelete: 'Confirm deletion',
    cancel: 'Cancel',
    mailboxRequired: 'Enter a valid email address and a password.',
    mailboxCreated: 'Mailbox created and assigned to the user.',
    mailboxDeleted: 'Mailbox deleted.',
    mailboxAdminDenied: 'Only administrators may manage mailboxes.',
  },
};

// App-API 4.0 (V4-PORT.md): mount(root, api, { restored }) → { unmount, snapshot }.
export async function mount(host, api, { restored } = {}) {
  if (!host.dataset.app) host.dataset.app = 'mail';
  api.shell.setTitle('Mail');
  api.setVisible(['communication_accounts', 'communication_threads', 'communication_messages', 'outbound_messages']);
  host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((response) => response.text());

  const root = host.querySelector('[data-mail-root]');
  if (!root) throw new Error('mail: root missing after fragment mount');
  // Pane-Grammatik (Suche, Bänder, Filter) verdrahtet die App selbst (PORTING R14) statt Shell-DOM-Heuristik.
  autoWirePaneGrammar(root);

  const currentLocale = () => (String(api.shell.locale.get() || document.documentElement.lang || 'de').toLowerCase().startsWith('en') ? 'en' : 'de');
  const messages = { ...(FALLBACK_LABELS.de || {}), ...(FALLBACK_LABELS[currentLocale()] || {}) };
  const t = (key, fallback) => {
    const fromShell = api.shell.t(`mail.${key}`);
    if (fromShell && fromShell !== `mail.${key}`) return fromShell;
    return messages[key] ?? fallback ?? key;
  };
  // Nur Spiegel für die Darstellung; jede Mutation entscheidet der Core (api.actor).
  const actorUser = () => ({ id: api.actor.id, user_id: api.actor.id, role: api.actor.role, display_name: api.actor.display_name, name: api.actor.display_name });
  const actorPermissions = () => ({ list: api.actor.permissions.get() || [] });
  const report = (code, text, level = 'error', ref) => api.status.report(ref ? { level, code, text: String(text || code), ref } : { level, code, text: String(text || code) });
  const restoredView = restored && typeof restored === 'object' && restored.v === 1 ? restored : null;
  applyStaticLabels(root, t);

  const refs = collectRefs(root);
  renderActionIcons(refs);
  // Lesen nur über api.query (lokal, live, ohne Zeilendeckel). Fremd-Collection ohne Recht → forbidden → null.
  const liveRows = new Map();
  const collections = Object.fromEntries(MAIL_COLLECTIONS.map((name) => [name, { name }]));
  // Schreiben über api.put (lokal sofort, Outbox); abgelehnte Writes meldet die Zone.
  async function putRecord(collection, record) {
    if (!collection) throw new Error('Required mail collection is missing');
    const handle = api.put(collection.name, record);
    await handle.local;
    handle.result.then((result) => {
      if (result?.status === 'rejected') report(`mail.write.${collection.name}`, result.reason || 'rejected');
    });
    return record;
  }

  const view = {
    accounts: [],
    threads: [],
    communicationMessages: [],
    campaigns: [],
    engagements: [],
    outboundMessages: [],
    approvals: [],
    commands: [],
    routeDestinations: [],
    users: [],
    accountKey: '',
    scopeType: 'queue',
    scopeId: 'inbound',
    search: '',
    leftGrammar: { search: '', view: 'cards', band: 'queues', filters: { account: 'all', sort: 'recent' } },
    listGrammar: { search: '', view: 'cards', band: 'all', filters: { status: 'all', sort: 'recent' } },
    selectedKind: '',
    selectedId: '',
    requestedSourceFocus: null,
    selectedAccountKey: '',
    loading: true,
    mailReadComplete: false,
    mailReadError: '',
    disposed: false,
    refreshTimer: null,
    refreshSequence: 0,
    lastSuccessfulMailSequence: 0,
    authorityDecisionSequence: 0,
    lastAppliedAuxiliarySequence: 0,
    busy: false,
    mailserver: {
      open: false,
      loading: false,
      domains: [],
      users: [],
      runtimeConfig: {},
      runtimeStatus: {},
      health: {},
      error: '',
      message: '',
      pendingDelete: '',
      pendingDomainDelete: '',
    },
    pendingSeriesHandoff: null,
    selectedRecords: new Set(),
    page: 0,
    pageSize: MAIL_PAGE_SIZE,
    route: { open: false, busy: false, error: '', status: '', lastCommandId: '' },
    contentEditor: null,
    contentGroupId: '',
  };
  const cleanups = [];
  const subscribedCollections = new Set();

  // Spaltenbreiten und Ansicht: Fensterzustand über snapshot()/restored (statt ctx.storageScope).
  const savedLeft = Number(restoredView?.leftWidth || 300);
  const savedRight = Number(restoredView?.rightWidth || 480);
  root.style.setProperty('--mail-left-width', `${clamp(savedLeft, 250, 420)}px`);
  root.style.setProperty('--mail-right-width', `${clamp(savedRight, 300, 620)}px`);
  applyRestoredView(restoredView);

  wireEvents();
  wireCollectionSubscriptions();
  // Live-Abo statt 15-s-Revalidierung und Fokus-Neuladen (PORTING R12): die lokale Collection ist die
  // Wahrheit; der Core entzieht Konten durch Löschung der Projektion.
  refreshData();
  render();
  restoreDrafts(restoredView);
  const offRoute = api.shell.onRoute((event) => {
    if (event?.to && event.to !== 'mail') return;
    applyDeepLink(event?.params || {});
    render();
  });
  cleanups.push(offRoute);

  let unmounted = false;
  return {
    unmount() {
      if (unmounted) return;
      unmounted = true;
      view.disposed = true;
      void view.contentEditor?.destroy?.();
      if (view.refreshTimer) window.clearTimeout(view.refreshTimer);
      for (const cleanup of cleanups) {
        try { cleanup?.(); } catch (error) { console.warn('[mail] cleanup failed', error); }
      }
      host.replaceChildren();
    },
    snapshot() {
      return {
        v: 1,
        leftWidth: parseInt(root.style.getPropertyValue('--mail-left-width'), 10) || 300,
        rightWidth: parseInt(root.style.getPropertyValue('--mail-right-width'), 10) || 480,
        accountKey: view.accountKey,
        scopeType: view.scopeType,
        scopeId: view.scopeId,
        selectedKind: view.selectedKind,
        selectedId: view.selectedId,
        selectedAccountKey: view.selectedAccountKey,
        page: view.page,
        leftGrammar: structuredClone(view.leftGrammar),
        listGrammar: structuredClone(view.listGrammar),
        composer: refs.composer && !refs.composer.hidden ? composerDraft() : null,
      };
    },
  };

  function applyRestoredView(saved) {
    if (!saved) return;
    for (const key of ['accountKey', 'scopeType', 'scopeId', 'selectedKind', 'selectedId', 'selectedAccountKey']) {
      if (typeof saved[key] === 'string') view[key] = saved[key];
    }
    if (Number.isInteger(saved.page) && saved.page >= 0) view.page = saved.page;
    if (saved.leftGrammar && typeof saved.leftGrammar === 'object') view.leftGrammar = normalizePaneGrammar(saved.leftGrammar, view.leftGrammar);
    if (saved.listGrammar && typeof saved.listGrammar === 'object') view.listGrammar = normalizePaneGrammar(saved.listGrammar, view.listGrammar);
    view.search = view.listGrammar.search || '';
  }

  // Offene Eingaben des Composers (Felder tragen nur data-Attribute, daher explizit über refs).
  function composerDraft() {
    return Object.fromEntries(COMPOSER_FIELDS.map((key) => [key, String(refs[key]?.value ?? '')]));
  }

  function restoreDrafts(saved) {
    if (!saved?.composer || typeof saved.composer !== 'object') return;
    openComposer();
    for (const key of COMPOSER_FIELDS) {
      const value = saved.composer[key];
      if (typeof value !== 'string' || !refs[key]) continue;
      if (refs[key].tagName === 'SELECT' && ![...refs[key].options].some((item) => item.value === value)) continue;
      refs[key].value = value;
    }
  }

  function wireEvents() {
    refs.account?.addEventListener('change', () => {
      view.accountKey = refs.account.value === 'all' ? '' : refs.account.value;
      view.selectedKind = '';
      view.selectedId = '';
      view.selectedAccountKey = '';
      render();
    });
    const openNewMail = () => {
      closeMobileNavigation();
      openComposer();
    };
    refs.compose?.addEventListener('click', openNewMail);
    refs.composePrimary?.addEventListener('click', openNewMail);
    refs.importButtons.forEach((button) => button?.addEventListener('click', openMailImporter));
    refs.exportButtons.forEach((button) => button?.addEventListener('click', exportVisibleMail));
    refs.settings?.addEventListener('click', openMailboxAdmin);
    refs.openNav?.addEventListener('click', openMobileNavigation);
    refs.closeNav?.addEventListener('click', closeMobileNavigation);
    refs.closeDetail?.addEventListener('click', closeDetail);
    refs.closeComposer?.addEventListener('click', requestCloseComposer);
    refs.saveDraft?.addEventListener('click', () => submitComposer('draft'));
    refs.requestApproval?.addEventListener('click', () => submitComposer('approval'));
    refs.sendNow?.addEventListener('click', () => submitComposer('send'));
    refs.newGroup?.addEventListener('click', openGroupEditor);
    refs.closeGroup?.addEventListener('click', closeGroupEditor);
    refs.createGroup?.addEventListener('click', createGroup);
    refs.groupTemplate?.addEventListener('change', applyGroupTemplateDefaults);
    refs.closeContentEditor?.addEventListener('click', closeGroupContentEditor);
    refs.saveContent?.addEventListener('click', saveGroupContent);
    refs.closeMailboxAdmin?.addEventListener('click', closeMailboxAdmin);
    refs.serverForm?.addEventListener('submit', saveMailserverRuntime);
    refs.domainForm?.addEventListener('submit', saveMailserverDomain);
    refs.toggleServer?.addEventListener('click', toggleMailserverRuntime);
    refs.mailboxForm?.addEventListener('submit', createMailbox);
    refs.selectPage?.addEventListener('change', togglePageSelection);
    refs.selectFiltered?.addEventListener('click', selectAllFiltered);
    refs.bulkRoute?.addEventListener('click', openRouteDrawer);
    refs.bulkApproval?.addEventListener('click', () => runBulkOutboundAction('request-approval'));
    refs.bulkSend?.addEventListener('click', () => runBulkOutboundAction('send'));
    refs.bulkExport?.addEventListener('click', exportSelectedMail);
    refs.clearSelection?.addEventListener('click', clearSelection);
    refs.closeRoute?.addEventListener('click', closeRouteDrawer);
    refs.cancelRoute?.addEventListener('click', closeRouteDrawer);
    refs.confirmRoute?.addEventListener('click', confirmRoute);
    refs.prevPage?.addEventListener('click', () => changePage(-1));
    refs.nextPage?.addEventListener('click', () => changePage(1));
    refs.leftPane?.addEventListener('ctox-pane-grammar-change', (event) => {
      if (event.target !== refs.leftPane) return;
      view.leftGrammar = normalizePaneGrammar(event.detail, view.leftGrammar);
      const nextAccountKey = view.leftGrammar.filters.account === 'all' ? '' : view.leftGrammar.filters.account;
      if (nextAccountKey !== view.accountKey) {
        view.selectedKind = '';
        view.selectedId = '';
        view.selectedAccountKey = '';
        view.accountKey = nextAccountKey;
        renderDetail();
      }
      view.page = 0;
      renderNavigation();
      renderList();
    });
    refs.listPane?.addEventListener('ctox-pane-grammar-change', (event) => {
      if (event.target !== refs.listPane) return;
      const bandChanged = event.detail.band !== view.listGrammar.band;
      view.listGrammar = normalizePaneGrammar(event.detail, view.listGrammar);
      view.search = view.listGrammar.search;
      view.page = 0;
      if (bandChanged && view.scopeType === 'queue') {
        view.scopeId = ({ inbound: 'inbound', outbound: 'outbound', all: 'all', attention: 'all' })[view.listGrammar.band] || view.scopeId;
        view.selectedKind = '';
        view.selectedId = '';
        renderNavigationSelection();
        renderDetail();
      }
      renderList();
    });

    // One-button view switch. Bound on the button itself, never delegated: the
    // shell grammar re-renders the icon on the same click, which detaches the
    // <svg> the click originated in, so `event.target.closest()` would find
    // nothing by the time the event reached the module root.
    refs.leftPane?.querySelector('[data-mail-view-toggle]')?.addEventListener('click', () => {
      view.leftGrammar = { ...view.leftGrammar, view: view.leftGrammar.view === 'list' ? 'cards' : 'list' };
      renderNavigation();
    });
    refs.listPane?.querySelector('[data-mail-view-toggle]')?.addEventListener('click', () => {
      view.listGrammar = { ...view.listGrammar, view: view.listGrammar.view === 'list' ? 'cards' : 'list' };
      view.page = 0;
      renderList();
    });

    root.addEventListener('click', async (event) => {
      const recordSelector = event.target.closest('[data-mail-select-record]');
      if (recordSelector) {
        event.stopPropagation();
        toggleRecordSelection(recordSelector.dataset.mailSelectRecord || '');
        return;
      }
      const scope = event.target.closest('[data-mail-scope]');
      if (scope) {
        view.scopeType = scope.dataset.mailScope;
        view.scopeId = scope.dataset.mailScopeId || '';
        view.selectedKind = '';
        view.selectedId = '';
        view.selectedAccountKey = '';
        view.page = 0;
        view.listGrammar = { ...view.listGrammar, band: 'all' };
        refs.listPane.querySelectorAll('[data-pg-band]').forEach((tab) => {
          const active = tab.dataset.pgBand === 'all';
          tab.classList.toggle('is-active', active);
          tab.setAttribute('aria-selected', String(active));
        });
        clearSelection();
        closeRouteDrawer();
        concealInspector();
        if (view.scopeType === 'campaign') revealInspector('detail');
        closeMobileNavigation();
        renderNavigationSelection();
        renderList();
        renderDetail();
        return;
      }
      const record = event.target.closest('[data-mail-record-kind]');
      if (record) {
        view.selectedKind = record.dataset.mailRecordKind || '';
        view.selectedId = record.dataset.mailRecordId || '';
        view.selectedAccountKey = record.dataset.mailRecordAccount || '';
        revealInspector('detail');
        renderListSelection();
        renderDetail();
        return;
      }
      const action = event.target.closest('[data-mail-action]');
      if (action) {
        await handleDetailAction(action.dataset.mailAction, action.dataset.mailId || '');
        return;
      }
      const mailboxAction = event.target.closest('[data-mailbox-action]');
      if (mailboxAction) {
        await handleMailboxAction(mailboxAction.dataset.mailboxAction, mailboxAction.dataset.mailboxUsername || '');
        return;
      }
      const domainAction = event.target.closest('[data-mail-domain-action]');
      if (domainAction) {
        await handleDomainAction(domainAction.dataset.mailDomainAction, domainAction.dataset.mailDomainName || '');
      }
    });
    root.addEventListener('keydown', (event) => {
      // Esc schließt die Verfassen-Schublade wie in Outlook/Apple Mail (FIX9a): leer sofort, sonst Rückfrage.
      if (event.key === 'Escape' && refs.composer && !refs.composer.hidden) {
        event.preventDefault();
        event.stopPropagation();
        requestCloseComposer();
        return;
      }
      // Esc schließt auch die Mail-Einstellungen (FIX10); der Fokus geht zurück auf den Einstellungen-Knopf.
      if (event.key === 'Escape' && refs.mailboxAdmin && !refs.mailboxAdmin.hidden) {
        event.preventDefault();
        event.stopPropagation();
        closeMailboxAdmin();
        refs.settings?.focus();
        return;
      }
      if (!['Enter', ' '].includes(event.key) || event.target.closest('input, textarea, select, button')) return;
      const record = event.target.closest('[data-mail-record-kind]');
      if (!record) return;
      event.preventDefault();
      record.click();
    });

  }

  function wireCollectionSubscriptions() {
    for (const name of MAIL_COLLECTIONS) {
      if (subscribedCollections.has(name)) continue;
      subscribedCollections.add(name);
      // Eine installierte App bekommt Fremd-Collections nur mit Recht; ohne Recht → forbidden. Die App
      // läuft ohne diese Collection weiter (vorher brach der Mount ab, thesen 22.09.2026).
      try {
        const off = api.query(name, {}).subscribe((rows) => {
          liveRows.set(name, rows);
          scheduleRefresh();
        });
        cleanups.push(off);
      } catch (error) {
        if (!isPermissionDenied(error)) throw error;
        collections[name] = null;
      }
    }
  }

  function readAll(collection, required = false) {
    if (!collection) return required ? new Error('Mail collection missing') : [];
    return liveRows.get(collection.name) || [];
  }

  function scheduleRefresh() {
    if (view.disposed) return;
    // Entprellung einer Emissionsfolge mehrerer Collections; Lesen ist lokal und synchron.
    if (view.refreshTimer) return;
    view.refreshTimer = window.setTimeout(() => {
      view.refreshTimer = null;
      if (view.disposed) return;
      refreshData();
      if (!view.disposed) render();
    }, 120);
  }

  function refreshData() {
    try {
      readSnapshot();
      api.status.clear('mail.read');
    } catch (error) {
      console.error('[mail] read failed', error);
      report('mail.read', error?.message || error);
    }
  }

  function closeAccountDerivedSurfaces() {
    view.accountKey = '';
    view.scopeType = 'queue';
    view.scopeId = 'inbound';
    view.selectedKind = '';
    view.selectedId = '';
    view.selectedAccountKey = '';
    view.selectedRecords.clear();
    view.pendingSeriesHandoff = null;
    view.route.open = false;
    refs.routeDrawer.hidden = true;
    closeComposer();
    if (view.contentEditor || !refs.contentSurface.hidden) {
      refs.contentSurface.hidden = true;
      void closeGroupContentEditor().catch((error) => {
        console.warn('[mail] content editor cleanup failed', error);
      });
    }
  }

  function hideUnverifiedMail() {
    view.accounts = [];
    view.threads = [];
    view.communicationMessages = [];
    view.campaigns = [];
    view.engagements = [];
    view.outboundMessages = [];
    view.approvals = [];
    closeAccountDerivedSurfaces();
    view.mailReadComplete = false;
  }

  function applyVerifiedAccounts(accounts) {
    const allowed = new Set(accounts.map((account) => account.id));
    const accessContracted = view.accounts.some((account) => !allowed.has(account.id));
    view.accounts = accounts;
    if (!accessContracted) return false;
    view.threads = view.threads.filter((thread) => allowed.has(thread.account_key));
    view.communicationMessages = view.communicationMessages.filter((message) => allowed.has(message.account_key));
    view.outboundMessages = view.outboundMessages.filter((message) => allowed.has(message.communication_account_key || message.sender_account_id));
    view.campaigns = [];
    view.engagements = [];
    view.approvals = [];
    closeAccountDerivedSurfaces();
    return true;
  }

  function readSnapshot() {
    const sequence = ++view.refreshSequence;
    if (collections.mail_server_state) {
      Object.assign(view.mailserver, mailserverStateFromRows(readAll(collections.mail_server_state)));
      if (view.mailserver.open) renderMailboxAdmin();
    }
    const reads = [
      readAll(collections.business_commands),
      readAll(collections.business_apps),
      readAll(collections.business_users),
      readAll(collections.communication_accounts, true),
      readAll(collections.communication_threads, true),
      readAll(collections.communication_messages, true),
      readAll(collections.outbound_campaigns),
      readAll(collections.outbound_engagements),
      readAll(collections.outbound_messages),
      readAll(collections.outbound_approvals),
    ];
    // Konten nur aus der Collection communication_accounts (Projektion des Cores). Der frühere native
    // Gegenlesepfad (readNativeCollectionDocument) entfällt: der Core synchronisiert nur sichtbare Konten
    // und entzieht sie durch Löschung (V4-PORT.md OFFEN).
    const accountRead = reads[3];
    const authorizedAccounts = Array.isArray(accountRead) ? visibleEmailAccounts(accountRead, actorUser()).sort(sortAccount) : null;
    if (authorizedAccounts && sequence > view.authorityDecisionSequence) {
      view.authorityDecisionSequence = sequence;
      if (!authorizedAccounts.length) {
        hideUnverifiedMail();
        view.mailReadComplete = true;
        view.mailReadError = '';
        view.loading = false;
        return;
      }
      applyVerifiedAccounts(authorizedAccounts);
    }
    const snapshots = reads;
    const failedRead = [3, 4, 5].map((index) => snapshots[index])
      .find((snapshot) => snapshot instanceof Error);
    const [commands, catalogs, users, accounts, threads, communicationMessages, campaigns, engagements, outboundMessages, approvals]
      = snapshots.map((snapshot) => Array.isArray(snapshot) ? snapshot : null);
    // Native account reads use a fresh authority token. A later denial must
    // win over an older local snapshot that finishes out of order.
    if (sequence >= view.authorityDecisionSequence) {
      if (authorizedAccounts) {
        view.authorityDecisionSequence = sequence;
        view.accounts = authorizedAccounts;
        const visibleAccountKeys = new Set(view.accounts.map((account) => account.id));
        if (accounts && threads && communicationMessages) {
          view.threads = threads.filter((thread) => thread.channel === 'email' && !isDeleted(thread) && visibleAccountKeys.has(thread.account_key));
          view.communicationMessages = communicationMessages.filter((message) => message.channel === 'email' && !isDeleted(message) && visibleAccountKeys.has(message.account_key));
          view.lastSuccessfulMailSequence = sequence;
          view.mailReadComplete = true;
          view.mailReadError = '';
        } else {
          view.threads = view.threads.filter((thread) => visibleAccountKeys.has(thread.account_key));
          view.communicationMessages = view.communicationMessages.filter((message) => visibleAccountKeys.has(message.account_key));
          if (failedRead) view.mailReadError = String(failedRead.message || failedRead);
        }
        if (view.accountKey && !visibleAccountKeys.has(view.accountKey)) {
          view.accountKey = '';
          view.selectedKind = '';
          view.selectedId = '';
        }
      } else if (failedRead) {
        view.mailReadError = String(failedRead.message || failedRead);
      }
    }
    if (sequence >= view.lastAppliedAuxiliarySequence) {
      if (commands) view.commands = commands.filter((command) => !isDeleted(command));
      if (catalogs) view.routeDestinations = routeDestinationsFromCatalog(catalogs, actorPermissions());
      if (users) view.users = users.filter((user) => !isDeleted(user) && user.active !== false).sort((a, b) => userDisplayName(a).localeCompare(userDisplayName(b)));
      if (campaigns || outboundMessages) view.campaigns = visibleMailCampaigns(campaigns || view.campaigns, actorUser(), view.accounts, outboundMessages || view.outboundMessages);
      if (engagements) view.engagements = engagements.filter((engagement) => !isDeleted(engagement));
      if (outboundMessages) view.outboundMessages = outboundMessages.filter((message) => (
        !isDeleted(message) && (!message.channel || message.channel === 'email')
        && view.accounts.some((account) => account.id === (message.communication_account_key || message.sender_account_id))
      )).sort(sortUpdatedDesc);
      if (approvals) view.approvals = approvals.filter((approval) => !isDeleted(approval));
      view.lastAppliedAuxiliarySequence = sequence;
    }
    if (!view.accounts.length) {
      view.campaigns = [];
      view.engagements = [];
      view.outboundMessages = [];
      view.approvals = [];
    }
    view.loading = !view.mailReadComplete;
  }

  function render() {
    renderAccounts();
    renderNavigation();
    renderList();
    renderDetail();
    reportRequestedSourceFocus();
    renderMailboxAdmin();
    if (view.pendingSeriesHandoff) {
      const seed = view.pendingSeriesHandoff;
      view.pendingSeriesHandoff = null;
      openComposer(seed);
    }
  }

  function renderAccounts() {
    if (!refs.account) return;
    const current = view.accountKey;
    refs.account.innerHTML = '';
    refs.account.append(option('all', t('allMailboxes', 'Alle sichtbaren Postfächer')));
    for (const account of view.accounts) {
      const profile = accountProfile(account);
      const kind = accountOwnedByCurrentUser(account, actorUser())
        ? t('accountPersonal', 'Persönlich')
        : t('accountShared', 'Gemeinsam');
      refs.account.append(option(account.id, `${account.address || account.id} · ${kind}`));
      void profile;
    }
    refs.account.value = current || 'all';
    refs.account.disabled = view.accounts.length === 0;
    if (refs.mailboxOwner) {
      const selectedOwner = refs.mailboxOwner.value;
      refs.mailboxOwner.innerHTML = `<option value="">— ${escapeHtml(t('selectPerson', 'Person wählen'))} —</option>${view.users.map((user) => `<option value="${escapeAttribute(user.id || user.user_id)}">${escapeHtml(userDisplayName(user))}</option>`).join('')}`;
      if (selectedOwner && view.users.some((user) => (user.id || user.user_id) === selectedOwner)) refs.mailboxOwner.value = selectedOwner;
    }
    populateComposerSelects();
  }

  function renderNavigation() {
    const visibleGroups = view.campaigns.filter((campaign) => !campaign.payload?.hidden_in_mail_groups);
    const queueRows = mailQueueDefinitions({
      threads: filteredAccountThreads(),
      inboxThreads: inboxAccountThreads(),
      sentThreads: sentAccountThreads(),
      outboundMessages: filteredAccountOutboundMessages(),
      communicationMessages: filteredAccountMessages(),
      commands: view.commands,
      t,
    });
    writePaneCounts(refs.leftPane, { queues: queueRows.length, campaigns: visibleGroups.length }, mailCountsKnown(view));
    refs.leftPane.dataset.mailView = view.leftGrammar.view;
    syncViewToggleButton(refs.leftPane, view.leftGrammar.view, t);
    const search = view.leftGrammar.search;
    const sort = view.leftGrammar.filters.sort || 'recent';
    const sources = view.leftGrammar.band === 'campaigns'
      ? visibleGroups.map((campaign) => {
        const stats = campaignStats(campaign.id, view.outboundMessages);
        const descriptor = emailGroupDescriptor(campaign);
        return {
          id: campaign.id,
          title: campaign.name || t('campaign', 'E-Mail-Gruppe'),
          meta: `${descriptor.kind} · ${stats.done} von ${stats.total} erledigt`,
          count: stats.percent,
          countLabel: `${stats.percent} %`,
          updatedAt: timeOf(campaign.updated_at_ms || campaign.updated_at),
        };
      })
      : queueRows;
    let rows = sources.filter((item) => !search || `${item.title} ${item.meta}`.toLowerCase().includes(search));
    const folderOrder = ['inbound', 'outbound', 'all', 'approval', 'failed', 'routed'];
    rows = [...rows].sort((a, b) => view.leftGrammar.band === 'queues' && sort === 'recent'
      ? folderOrder.indexOf(a.id) - folderOrder.indexOf(b.id)
      : sort === 'name'
        ? a.title.localeCompare(b.title)
        : sort === 'count'
          ? b.count - a.count
          : Number(b.updatedAt || 0) - Number(a.updatedAt || 0));
    // Shards carry title plus meta line; the list is one dense line per entry
    // with a single short meta on the right.
    const scopeAsList = view.leftGrammar.view === 'list';
    const showFolderSections = view.leftGrammar.band === 'queues' && sort === 'recent' && !search;
    refs.scopeList.innerHTML = rows.length ? rows.map((item) => {
      const scopeType = view.leftGrammar.band === 'campaigns' ? 'campaign' : 'queue';
      // Ordner wie in Outlook/Apple Mail: eine Zeile mit Name und Zähler, keine Erklärzeile (Textdiät Regel 2).
      const compact = scopeAsList || scopeType === 'queue';
      const shape = compact ? 'mail-scope-card--line' : 'mail-scope-card--shard';
      const meta = compact ? '' : `<span class="mail-scope-meta">${escapeHtml(item.meta)}</span>`;
      const section = showFolderSections && (item.id === 'inbound' || item.id === 'approval')
        ? `<div class="mail-scope-section-title">${escapeHtml(item.id === 'inbound' ? t('folders', 'Ordner') : t('workflows', 'Arbeitsabläufe'))}</div>`
        : '';
      return `${section}<button class="mail-scope-card ${shape}${view.scopeType === scopeType && view.scopeId === item.id ? ' is-active' : ''}" type="button" data-mail-scope="${scopeType}" data-mail-scope-id="${escapeAttribute(item.id)}" data-context-record-id="${escapeAttribute(item.id)}" data-context-record-type="${scopeType}" data-context-record-label="${escapeAttribute(item.title)}" data-context-label="${escapeAttribute(item.title)}">
        <span class="mail-scope-title">${escapeHtml(item.title)}</span>${meta}<span class="mail-scope-count">${escapeHtml(mailCountsKnown(view) ? (item.countLabel ?? item.count) : '…')}</span>
      </button>`;
    }).join('') : '';
    renderNavigationSelection();
    const groupsVisible = view.leftGrammar.band === 'campaigns';
    refs.navigationTitle.textContent = groupsVisible ? t('groups', 'E-Mail-Gruppen') : t('mailbox', 'Postfach');
    refs.newGroup.hidden = !groupsVisible;
    // Kein Zähler-Fuß in der Navigation (Textdiät Regel 5); die Zähler stehen an Reitern und Ordnern.
    refs.sidebarFooter.textContent = '';
    refs.leftPane.__ctoxPaneGrammar?.setFooter?.('');
  }

  function renderNavigationSelection() {
    for (const row of refs.scopeList.querySelectorAll('[data-mail-scope]')) {
      const selected = row.dataset.mailScope === view.scopeType
        && (row.dataset.mailScopeId || '') === view.scopeId;
      row.classList.toggle('is-active', selected);
      row.setAttribute('aria-selected', String(selected));
    }
  }

  function renderList() {
    const allRows = currentRecords();
    const maxPage = Math.max(0, Math.ceil(allRows.length / view.pageSize) - 1);
    view.page = Math.min(view.page, maxPage);
    const pageStart = view.page * view.pageSize;
    const rows = allRows.slice(pageStart, pageStart + view.pageSize);
    const globalRecords = [
      ...filteredAccountThreads().map((thread) => ({ ...thread, __kind: 'thread' })),
      ...filteredAccountOutboundMessages().map((message) => ({ ...message, __kind: 'outbound' })),
    ];
    const counts = {
      all: globalRecords.length,
      inbound: inboxAccountThreads().length,
      outbound: sentAccountThreads().length,
      attention: listBandCounts(globalRecords, view.commands, filteredAccountMessages()).attention,
    };
    const scopeLabel = currentScopeLabel();
    refs.listKicker.textContent = view.scopeType === 'campaign' ? t('campaign', 'Kampagne') : t('mailbox', 'Postfach');
    refs.listTitle.textContent = view.listGrammar.band === 'outbound' && ['all', 'inbound'].includes(view.scopeId)
      ? t('sent', 'Gesendet') : scopeLabel;
    refs.listPane.dataset.mailView = view.listGrammar.view;
    syncViewToggleButton(refs.listPane, view.listGrammar.view, t);
    writePaneCounts(refs.listPane, counts, mailCountsKnown(view));
    const range = !mailCountsKnown(view) ? '…' : allRows.length ? `${pageStart + 1}–${pageStart + rows.length} / ${allRows.length}` : '0';
    const footer = `${range} ${t('messages', 'Nachrichten')}${view.accountKey ? ` · ${accountLabel(view.accountKey)}` : ''}`;
    refs.listFooter.textContent = footer;
    refs.prevPage.disabled = view.page === 0;
    refs.nextPage.disabled = view.page >= maxPage;
    refs.listPane.__ctoxPaneGrammar?.setFooter?.(footer);
    refs.recordList.innerHTML = rows.map(renderRecordRow).join('');
    renderListSelection();
    renderBulkBar(rows, allRows);

    // Leere Liste rendert nichts; Lesefehler gehen in die Zustandszone (PORTING R11, refreshData).
  }

  // Two genuinely different shapes for the same record:
  //   shard - bold subject plus one or two meta lines (sender, status, group,
  //           delivery progress) and generous padding;
  //   line  - exactly one dense row: subject and a single short meta (time).
  // Absender/Empfänger in der Liste: Name oder Adresse, sonst „Unbekannt“ – nie eine Roh-ID
  // (Konto-/Thread-Schlüssel wie „communication--001“). Textdiät Regel 4.
  function mailPersonLabel(...candidates) {
    for (const raw of candidates) {
      const value = String(raw ?? '').trim();
      if (!value) continue;
      const address = value.replace(/^[a-z][a-z0-9_-]*:(?=[^@\s]+@)/i, '');
      if (address.includes('@')) return address;
      if (/\s/.test(value) || !/[-_:\d]/.test(value)) return value;
    }
    return t('unknownPerson', 'Unbekannt');
  }

  function renderRecordRow(record) {
    const asList = view.listGrammar.view === 'list';
    const shape = asList ? 'mail-record-row--line' : 'mail-record-row--shard';
    if (record.__kind === 'thread') {
      const latest = latestMessageForThread(record, filteredAccountMessages());
      const sender = latest?.direction === 'outbound'
        ? mailPersonLabel(latest.recipient_addresses_json?.[0], record.participant_keys_json?.[0])
        : mailPersonLabel(latest?.sender_display, latest?.sender_address, record.participant_keys_json?.[0]);
      const subject = latest?.subject || record.subject || '(Kein Betreff)';
      const preview = latest?.preview || latest?.body_text || '';
      const selectionKey = mailRecordKey(record);
      const route = routeCommandForRecord(record, view.commands);
      const unread = Number(record.unread_count || 0);
      const status = route ? routeTargetLabel(route) : (unread ? `${unread} neu` : latest?.direction === 'outbound' ? t('sent', 'Gesendet') : t('inbound', 'Eingang'));
      const open = `<div class="mail-record-row ${shape}${unread > 0 ? ' is-unread' : ''}" role="option" tabindex="0" data-mail-record-kind="thread" data-mail-record-id="${escapeAttribute(record.id)}" data-mail-record-account="${escapeAttribute(record.account_key)}" data-context-record-id="${escapeAttribute(record.id)}" data-context-record-type="communication_thread" data-context-record-label="${escapeAttribute(subject)}" data-context-label="${escapeAttribute(subject)}">
        <input class="mail-record-select" type="checkbox" data-mail-select-record="${escapeAttribute(selectionKey)}" aria-label="${escapeAttribute(subject)} auswählen" ${view.selectedRecords.has(selectionKey) ? 'checked' : ''} />`;
      const time = `<span class="mail-record-time">${escapeHtml(formatRecordTime(record.last_message_at))}</span>`;
      if (asList) {
        return `${open}
        <span class="mail-record-subject">${escapeHtml(subject)}</span>
        ${time}
      </div>`;
      }
      const previewLine = compact(preview);
      return `${open}
        <span class="mail-record-copy"><span class="mail-record-identity"><span class="mail-record-person">${escapeHtml(sender)}</span><span class="mail-record-status">${escapeHtml(status)}</span></span><span class="mail-record-subject">${escapeHtml(subject)}</span>${previewLine ? `<span class="mail-record-preview">${escapeHtml(previewLine)}</span>` : ''}</span>
        ${time}
      </div>`;
    }
    const campaign = view.campaigns.find((item) => item.id === record.campaign_id);
    const recipient = mailPersonLabel(record.recipient_name, record.recipient_email, record.recipient_address_text);
    const status = messageStatusLabel(record, t);
    const selectionKey = mailRecordKey(record);
    const route = routeCommandForRecord(record, view.commands);
    const displayStatus = route ? routeTargetLabel(route) : status;
    const subject = record.subject || '(Kein Betreff)';
    const open = `<div class="mail-record-row ${shape}" role="option" tabindex="0" data-mail-record-kind="outbound" data-mail-record-id="${escapeAttribute(record.id)}" data-context-record-id="${escapeAttribute(record.id)}" data-context-record-type="outbound_message" data-context-record-label="${escapeAttribute(record.subject || recipient)}" data-context-label="${escapeAttribute(record.subject || recipient)}">
      <input class="mail-record-select" type="checkbox" data-mail-select-record="${escapeAttribute(selectionKey)}" aria-label="${escapeAttribute(record.subject || recipient)} auswählen" ${view.selectedRecords.has(selectionKey) ? 'checked' : ''} />`;
    const time = `<span class="mail-record-time">${escapeHtml(formatRecordTime(record.updated_at_ms || record.created_at_ms))}</span>`;
    if (asList) {
      return `${open}
      <span class="mail-record-subject">${escapeHtml(subject)}</span>
      ${time}
    </div>`;
    }
    const progress = messageProgressModel(record);
    const progressIcons = progress.steps.map((step) => `<span class="mail-progress-step is-${escapeAttribute(step.state)}" title="${escapeAttribute(step.label)}" aria-label="${escapeAttribute(step.label)}">${mailActionIcon(step.icon, 11, 1.9)}</span>`).join('');
    const group = campaign?.name ? `${escapeHtml(campaign.name)} · ` : '';
    return `${open}
      <span class="mail-record-copy"><span class="mail-record-identity"><span class="mail-record-person">${escapeHtml(recipient)}</span><span class="mail-record-status">${group}${escapeHtml(status)}</span></span><span class="mail-record-subject">${escapeHtml(subject)}</span><span class="mail-record-progress" role="img" aria-label="${escapeAttribute(progress.ariaLabel)}"><span class="mail-progress-track"><i style="width:${progress.percent}%"></i></span><span class="mail-progress-steps">${progressIcons}</span><span class="mail-progress-label">${escapeHtml(displayStatus)}</span></span></span>
      ${time}
    </div>`;
  }

  function renderListSelection() {
    refs.recordList.querySelectorAll('[data-mail-record-kind]').forEach((row) => {
      row.classList.toggle('is-selected', row.dataset.mailRecordKind === view.selectedKind && row.dataset.mailRecordId === view.selectedId && (row.dataset.mailRecordAccount || '') === view.selectedAccountKey);
      const key = row.querySelector('[data-mail-select-record]')?.dataset.mailSelectRecord || '';
      const bulkSelected = view.selectedRecords.has(key);
      row.classList.toggle('is-bulk-selected', bulkSelected);
      const checkbox = row.querySelector('[data-mail-select-record]');
      if (checkbox) checkbox.checked = bulkSelected;
    });
  }

  function renderDetail() {
    if (view.selectedKind && view.selectedId && !view.route.open) revealInspector('detail');
    if (view.selectedKind === 'thread') {
      refs.detailTitle.textContent = t('thread', 'Thread');
      renderThreadDetail(view.selectedId, view.selectedAccountKey);
      return;
    }
    if (view.selectedKind === 'outbound') {
      refs.detailTitle.textContent = t('message', 'Nachricht');
      renderOutboundDetail(view.selectedId);
      return;
    }
    if (view.scopeType === 'campaign') {
      refs.detailTitle.textContent = t('campaign', 'Kampagne');
      renderCampaignDetail(view.scopeId);
      return;
    }
    if (!view.route.open) concealInspector();
    refs.detailTitle.textContent = t('message', 'Nachricht');
    renderMissingDetail();
  }

  function renderThreadDetail(threadKey, accountKey = '') {
    const thread = filteredAccountThreads().find((item) => item.id === threadKey && (!accountKey || item.account_key === accountKey));
    if (!thread) return renderMissingDetail();
    const timeline = view.communicationMessages
      .filter((message) => messageBelongsToThread(message, thread))
      .sort((a, b) => timeOf(a.external_created_at) - timeOf(b.external_created_at));
    const participants = (thread.participant_keys_json || []).join(', ') || thread.account_key;
    const replyTo = [...timeline].reverse().find((message) => message.direction === 'inbound')?.sender_address || '';
    refs.detail.innerHTML = `<article class="mail-detail-shell" data-context-record-id="${escapeAttribute(threadKey)}" data-context-record-type="communication_thread" data-context-record-label="${escapeAttribute(thread.subject || '(Kein Betreff)')}" data-context-label="${escapeAttribute(thread.subject || '(Kein Betreff)')}">
      <header class="mail-detail-header">
        <div><span class="ctox-pane-kicker">${escapeHtml(t('thread', 'Thread'))}</span><h3>${escapeHtml(thread.subject || timeline.at(-1)?.subject || '(Kein Betreff)')}</h3><div class="mail-detail-participants">${escapeHtml(participants)}</div></div>
        <div class="mail-detail-actions"><button type="button" class="ctox-button is-primary" data-mail-action="reply" data-mail-id="${escapeAttribute(threadKey)}">${escapeHtml(t('reply', 'Antworten'))}</button><button type="button" class="ctox-button" data-mail-action="route" data-mail-id="${escapeAttribute(threadKey)}">${escapeHtml(t('routeToApp', 'An App routen'))}</button></div>
      </header>
      <div class="mail-thread-timeline">${timeline.map(renderTimelineMessage).join('')}</div>
      <footer class="ctox-pane-footer"><span>${escapeHtml(replyTo || thread.account_key)}</span></footer>
    </article>`;
    const bodyHosts = refs.detail.querySelectorAll('[data-mail-message-body]');
    for (const bodyHost of bodyHosts) {
      // Resolve from the already account-scoped timeline, never a global
      // message-key lookup shared by different mailboxes.
      const message = timeline.find((item) => item.id === bodyHost.dataset.mailMessageBody);
      mountMailBody(bodyHost, message);
    }
  }

  function renderTimelineMessage(message) {
    const outbound = message.direction === 'outbound';
    const party = outbound
      ? (message.recipient_addresses_json?.join(', ') || message.account_key)
      : (message.sender_display || message.sender_address || '—');
    return `<section class="mail-message${outbound ? ' is-outbound' : ''}" tabindex="-1" data-context-record-id="${escapeAttribute(message.id)}" data-context-record-type="communication_message">
      <div class="mail-message-head"><strong>${escapeHtml(party)}</strong><span>${escapeHtml(formatRecordTime(message.external_created_at))}</span></div>
      <div class="mail-message-body" data-mail-message-body="${escapeAttribute(message.id)}"></div>
    </section>`;
  }

  function renderOutboundDetail(messageId) {
    const message = filteredAccountOutboundMessages().find((item) => item.id === messageId);
    if (!message) return renderMissingDetail();
    const campaign = view.campaigns.find((item) => item.id === message.campaign_id);
    const actions = [
      ...messageActions(message, t),
      { id: 'refresh-status', label: 'Status abrufen', primary: false },
      { id: 'route', label: t('routeToApp', 'An App routen'), primary: false },
    ]
      .map((action) => `<button type="button" class="ctox-button${action.primary ? ' is-primary' : ''}" data-mail-action="${escapeAttribute(action.id)}" data-mail-id="${escapeAttribute(message.id)}">${escapeHtml(action.label)}</button>`)
      .join('');
    refs.detail.innerHTML = `<article class="mail-detail-shell" data-context-record-id="${escapeAttribute(message.id)}" data-context-record-type="outbound_message" data-context-record-label="${escapeAttribute(message.subject || '(Kein Betreff)')}" data-context-label="${escapeAttribute(message.subject || '(Kein Betreff)')}">
      <header class="mail-detail-header">
        <div><span class="ctox-pane-kicker">${escapeHtml(campaign?.name || t('campaign', 'Kampagne'))}</span><h3>${escapeHtml(message.subject || '(Kein Betreff)')}</h3><div class="mail-detail-participants">${escapeHtml(message.sender_account_id || '')} → ${escapeHtml(message.recipient_email || message.recipient_address_text || '—')}</div></div>
        <div class="mail-detail-actions">${actions}</div>
      </header>
      <div class="mail-delivery-timeline">${messageEventTimeline(message).map((event) => `<div class="mail-delivery-event is-${escapeAttribute(event.state)}"><span></span><div><strong>${escapeHtml(event.label)}</strong><small>${escapeHtml(event.detail)}</small></div></div>`).join('')}</div>
      <div class="mail-thread-timeline">
        <section class="mail-message is-outbound"><div class="mail-message-head"><strong>${escapeHtml(message.recipient_email || '—')}</strong><span>${escapeHtml(messageStatusLabel(message, t))}</span></div><div class="mail-message-body" data-mail-outbound-body></div></section>
      </div>
      <footer class="ctox-pane-footer"><span>${escapeHtml(messageStatusLabel(message, t))}</span></footer>
    </article>`;
    mountMailBody(refs.detail.querySelector('[data-mail-outbound-body]'), message);
  }

  function renderCampaignDetail(campaignId) {
    const campaign = view.campaigns.find((item) => item.id === campaignId);
    if (!campaign) return renderMissingDetail();
    const stats = campaignStats(campaignId, view.outboundMessages);
    const descriptor = emailGroupDescriptor(campaign);
    const status = String(campaign.status || 'active').toLowerCase();
    refs.detail.innerHTML = `<article class="mail-detail-shell" data-context-record-id="${escapeAttribute(campaign.id)}" data-context-record-type="outbound_campaign" data-context-record-label="${escapeAttribute(campaign.name)}" data-context-label="${escapeAttribute(campaign.name)}">
      <header class="mail-detail-header">
        <div><span class="ctox-pane-kicker">${escapeHtml(groupKindLabel(descriptor.kind))}</span><h3>${escapeHtml(campaign.name)}</h3><div class="mail-detail-participants">${escapeHtml(status === 'paused' ? t('paused', 'Pausiert') : t('active', 'Aktiv'))} · ${escapeHtml(descriptor.intakeMode === 'dynamic' ? 'dynamischer Eingang' : 'fester Umfang')}</div></div>
        <div class="mail-detail-actions">
          <button type="button" class="ctox-button is-primary" data-mail-action="compose-campaign" data-mail-id="${escapeAttribute(campaign.id)}">${escapeHtml(t('compose', 'Neue Mail'))}</button>
          <button type="button" class="ctox-button" data-mail-action="edit-group-content" data-mail-id="${escapeAttribute(campaign.id)}">${escapeHtml(groupContentLabel(descriptor.contentMode))}</button>
          <button type="button" class="ctox-button" data-mail-action="campaign-${status === 'paused' ? 'resume' : 'pause'}" data-mail-id="${escapeAttribute(campaign.id)}">${escapeHtml(status === 'paused' ? t('resume', 'Fortsetzen') : t('pause', 'Pausieren'))}</button>
        </div>
      </header>
      <div class="mail-campaign-overview">
        <div class="mail-group-assignment"><span>Auftrag</span><strong>${escapeHtml(campaign.objective || descriptor.objective)}</strong></div>
        <div class="mail-group-progress" aria-label="${escapeAttribute(`${stats.done} von ${stats.total} erledigt`)}"><div><strong>${stats.done} von ${stats.total} erledigt</strong><span>${stats.percent} %</span></div><progress value="${stats.done}" max="${Math.max(1, stats.total)}">${stats.percent} %</progress></div>
        <div class="mail-metric-strip">
          ${renderMetric(stats.total, t('total', 'Gesamt'))}
          ${renderMetric(stats.queued, 'In Queue')}
          ${renderMetric(stats.delivered, 'Zugestellt')}
          ${renderMetric(stats.opened, 'Öffnung erfasst')}
          ${renderMetric(stats.clicked, 'Link geklickt')}
          ${renderMetric(stats.failed, 'Fehler')}
        </div>
      </div>
      <footer class="ctox-pane-footer"><span>${escapeHtml(descriptor.accountKey || 'Absenderkonto noch nicht festgelegt')}</span><span>${stats.complete ? 'Auftrag abgeschlossen' : `${stats.open + stats.working} noch zu bearbeiten`}</span></footer>
    </article>`;
  }

  function renderMissingDetail() {
    if (view.requestedSourceFocus) {
      // Live-Daten sind lokal vollständig: was hier fehlt, ist in diesem Postfach nicht sichtbar.
      report('mail.focus.unavailable', 'Verknüpfte Mail ist in diesem Postfach nicht sichtbar.', 'warn');
      refs.detail.innerHTML = '';
      return;
    }
    // Ohne Auswahl rendert der Inspektor nichts (PORTING R11, keine Hinweisbox).
    refs.detail.innerHTML = '';
  }

  async function handleDetailAction(action, id) {
    if (view.busy) return;
    if (action === 'edit-group-content') {
      await openGroupContentEditor(id);
      return;
    }
    if (action === 'reply') {
      const thread = filteredAccountThreads().find((item) => item.id === id && (!view.selectedAccountKey || item.account_key === view.selectedAccountKey));
      if (!thread) return;
      const timeline = view.communicationMessages.filter((message) => messageBelongsToThread(message, thread));
      const inbound = [...timeline].sort((a, b) => timeOf(b.external_created_at) - timeOf(a.external_created_at)).find((message) => message.direction === 'inbound');
      openComposer({
        to: inbound?.sender_address || thread?.participant_keys_json?.[0] || '',
        subject: replySubject(inbound?.subject || thread?.subject || ''),
        accountKey: thread?.account_key || '',
      });
      return;
    }
    if (action === 'route') {
      const record = filteredAccountThreads().find((item) => item.id === id && (!view.selectedAccountKey || item.account_key === view.selectedAccountKey))
        || filteredAccountOutboundMessages().find((item) => item.id === id);
      if (record) {
        view.selectedRecords.add(mailRecordKey({ ...record, __kind: record.thread_key ? 'thread' : 'outbound' }));
        renderBulkBar(currentPageRecords(), currentRecords());
        openRouteDrawer();
      }
      return;
    }
    if (action === 'compose-campaign') {
      openComposer({ campaignId: id });
      return;
    }
    if (action === 'open-outbound') {
      api.shell.navigate('outbound', { campaign_id: id });
      return;
    }
    if (action === 'refresh-status') {
      await runBusy(async () => {
        dispatchOutbound('outbound.provider.reconcile', id, { message_id: id }, {
          onDone: () => api.status.report({ level: 'info', code: 'mail.notice', text: 'Zustellstatus wurde mit dem Mailserver abgeglichen.' }),
        });
      });
      return;
    }
    if (action === 'campaign-pause' || action === 'campaign-resume') {
      await runBusy(async () => {
        dispatchOutbound('outbound.campaign.status.set', id, {
          campaign_id: id,
          status: action === 'campaign-pause' ? 'paused' : 'active',
        }, { doneText: action === 'campaign-pause' ? t('paused', 'Pausiert') : t('active', 'Aktiv') });
      });
      return;
    }
    const message = filteredAccountOutboundMessages().find((item) => item.id === id);
    if (!message) return;
    await runBusy(async () => {
      if (action === 'request-approval') {
        dispatchOutbound('outbound.message.request_approval', id, { message_id: id }, { doneText: t('approvalRequested', 'Entwurf wurde zur Freigabe eingereicht.') });
      } else if (action === 'approve') {
        dispatchOutbound('outbound.message.approve', id, { message_id: id }, { doneText: t('approved', 'Nachricht wurde freigegeben.') });
      } else if (action === 'send') {
        dispatchOutbound('outbound.message.send_approved', id, { message_id: id }, { doneText: t('queued', 'Nachricht wurde in die Mailserver-Queue eingereiht.') });
      }
    });
  }

  // Schubladen liegen nie übereinander (FIX10): wer eine öffnet, schließt die anderen. Die Verfassen-Schublade mit
  // Inhalt wird nicht still verworfen, sondern fragt wie beim Schließen („Entwurf behalten?“).
  function openComposer(seed = {}) {
    closeMailboxAdmin();
    closeGroupEditor();
    refs.composer.hidden = false;
    refs.composerTitle.textContent = seed.series ? t('seriesEmail', 'Serien-E-Mail') : t('compose', 'Neue Mail');
    refs.composerKicker.textContent = seed.series ? t('campaign', 'Kampagne') : t('draft', 'Entwurf');
    refs.composeFrom.value = seed.accountKey || view.accountKey || view.accounts[0]?.id || '';
    refs.composeTo.value = Array.isArray(seed.to) ? seed.to.join('\n') : (seed.to || '');
    refs.composeCampaign.value = seed.campaignId || (view.scopeType === 'campaign' ? view.scopeId : '');
    refs.composeSubject.value = seed.subject || '';
    refs.composeBody.value = seed.body || '';
    setComposerStatus('');
    window.setTimeout(() => refs.composeTo.focus(), 0);
  }

  function closeComposer() {
    refs.composer.hidden = true;
    setComposerStatus('');
  }

  // Ungespeicherter Inhalt = Empfänger, Betreff oder Text (Absender und Gruppe sind vorbelegt).
  function composerHasContent() {
    return ['composeTo', 'composeSubject', 'composeBody'].some((key) => String(refs[key]?.value ?? '').trim() !== '');
  }

  // Schließen per × oder Esc (FIX9a, Standard Outlook/Apple Mail): leer → sofort zu; sonst „Entwurf behalten?“ über
  // api.shell.dialog. Behalten speichert den Entwurf, Verwerfen schließt ohne Speichern, Abbrechen (Esc) bleibt offen.
  // (Funktion steht hinter dem return von mount → kein let/const davor; Merker am Element.)
  async function requestCloseComposer() {
    if (!refs.composer || refs.composer.hidden || refs.composer.dataset.mailAsking) return;
    if (!composerHasContent()) { closeComposer(); return; }
    refs.composer.dataset.mailAsking = '1';
    let answer = null;
    try {
      answer = await api.shell.dialog({
        kind: 'select',
        title: t('keepDraftTitle', 'Entwurf behalten?'),
        options: [{ value: 'keep', label: t('keepDraft', 'Als Entwurf speichern') }, { value: 'discard', label: t('discardDraft', 'Verwerfen') }],
        default: 'keep',
        confirm_label: t('ok', 'OK'),
      });
    } catch { answer = null; } finally { delete refs.composer.dataset.mailAsking; }
    if (answer === 'keep') await submitComposer('draft');
    else if (answer === 'discard') closeComposer();
    else refs.composeTo?.focus();
  }

  // Verfassen-Schublade vor dem Öffnen einer anderen schließen; true = sie ist zu (leer, gespeichert oder verworfen).
  async function releaseComposer() {
    if (!refs.composer || refs.composer.hidden) return true;
    await requestCloseComposer();
    return refs.composer.hidden;
  }

  function populateComposerSelects() {
    if (!refs.composeFrom || !refs.composeCampaign) return;
    const currentFrom = refs.composeFrom.value;
    refs.composeFrom.innerHTML = '';
    refs.composeFrom.append(option('', t('missingSender', 'Bitte ein Absenderpostfach auswählen.')));
    for (const account of view.accounts) refs.composeFrom.append(option(account.id, account.address || account.id));
    refs.composeFrom.value = view.accounts.some((account) => account.id === currentFrom) ? currentFrom : (view.accountKey || view.accounts[0]?.id || '');

    const currentCampaign = refs.composeCampaign.value;
    refs.composeCampaign.innerHTML = '';
    refs.composeCampaign.append(option('', t('ungrouped', 'Ohne sichtbare Gruppe')));
    for (const campaign of view.campaigns.filter((item) => !item.payload?.hidden_in_mail_groups)) {
      refs.composeCampaign.append(option(campaign.id, campaign.name));
    }
    refs.composeCampaign.value = view.campaigns.some((campaign) => campaign.id === currentCampaign) ? currentCampaign : '';
  }

  async function submitComposer(mode = 'draft') {
    if (view.busy) return;
    const recipients = parseRecipientAddresses(refs.composeTo.value);
    const input = {
      accountKey: refs.composeFrom.value,
      recipient: recipients[0] || '',
      campaignId: refs.composeCampaign.value,
      subject: refs.composeSubject.value.trim(),
      body: refs.composeBody.value.trim(),
    };
    const validation = validateComposeInput(input);
    if (validation) {
      setComposerStatus(t(validation, validation), true);
      return;
    }
    await runBusy(async () => {
      setComposerStatus('');
      const campaign = await ensureComposeCampaign(input.campaignId, { ...input, recipientCount: recipients.length });
      let lastMessageId = '';
      const contentRevisionId = String(campaign.payload?.mail_group?.content?.active_revision_id || '');
      const successMessage = mode === 'send'
        ? `${recipients.length} E-Mail${recipients.length === 1 ? '' : 's'} an den Mailserver übergeben.`
        : mode === 'approval'
          ? t('approvalRequested', 'Entwurf wurde zur Freigabe eingereicht.')
          : `${recipients.length} ${t('draftsSaved', 'Entwurf/Entwürfe gespeichert.')}`;
      // EIN Befehl `outbound.message.compose` (Strang MAIL2): der outbound-Handler führt Engagement → Entwurf →
      // [Freigabe → Versand] je Empfänger in fester Reihenfolge in einer Transaktion aus. Ohne Warten (PORTING R5);
      // die Erfolgsmeldung kommt bei `done`, ein Fehler genau einmal in die Zone.
      const payload = buildComposePayload({ ...input, campaignId: campaign.id, contentRevisionId, mode }, recipients.map((recipient) => ({
        recipient_email: recipient,
        engagement_id: `eng_mail_${crypto.randomUUID()}`,
        message_id: `msg_mail_${crypto.randomUUID()}`,
      })), Date.now());
      lastMessageId = payload.recipients.at(-1)?.message_id || '';
      dispatchOutbound('outbound.message.compose', campaign.id, payload, { renderAfter: false, doneText: successMessage });
      view.scopeType = 'campaign';
      view.scopeId = campaign.id;
      view.selectedKind = 'outbound';
      view.selectedId = lastMessageId;
      closeComposer();
      render();
    }, (error) => setComposerStatus(error.message || t('commandFailed', 'Die Mail-Aktion ist fehlgeschlagen.'), true));
  }

  async function ensureComposeCampaign(campaignId, seed = {}) {
    const existing = view.campaigns.find((campaign) => campaign.id === campaignId);
    if (existing) return existing;
    const now = Date.now();
    const single = Number(seed.recipientCount || 1) === 1;
    const record = buildMailGroupRecord({
      id: `mail_group_${crypto.randomUUID()}`,
      templateId: single ? 'single' : 'outbound-sales',
      name: String(seed.subject || seed.recipient || '').trim() || (single ? 'Einzel-E-Mail' : 'Serien-E-Mail'),
      ownerId: String(api.actor.id || ''),
      accountKey: String(seed.accountKey || ''),
    }, now);
    assertCanWriteCampaigns();
    await putRecord(collections.outbound_campaigns, record);
    view.campaigns.push(record);
    return record;
  }

  async function openGroupEditor() {
    if (!(await releaseComposer())) return;
    closeMailboxAdmin();
    refs.groupEditor.hidden = false;
    refs.groupTemplate.value = 'outbound-sales';
    refs.groupName.value = '';
    refs.groupDescription.value = mailGroupTemplate('outbound-sales').objective;
    window.setTimeout(() => refs.groupName.focus(), 0);
  }

  function applyGroupTemplateDefaults() {
    const template = mailGroupTemplate(refs.groupTemplate?.value || 'free');
    refs.groupDescription.value = template.objective;
  }

  function closeGroupEditor() {
    refs.groupEditor.hidden = true;
  }

  async function createGroup() {
    const name = refs.groupName.value.trim();
    if (!name) {
      report('mail.group.validation', t('groupNameRequired', 'Bitte einen Gruppennamen eingeben.'), 'warn');
      refs.groupName.focus();
      return;
    }
    api.status.clear('mail.group.validation');
    await runBusy(async () => {
      assertCanWriteCampaigns();
      const now = Date.now();
      const record = buildMailGroupRecord({
        id: `camp_mail_${crypto.randomUUID()}`,
        name,
        objective: refs.groupDescription.value.trim(),
        templateId: refs.groupTemplate?.value || 'free',
        ownerId: String(api.actor.id || ''),
        accountKey: view.accountKey || '',
      }, now);
      await putRecord(collections.outbound_campaigns, record);
      view.campaigns.unshift(record);
      view.scopeType = 'campaign';
      view.scopeId = record.id;
      view.selectedKind = '';
      view.selectedId = '';
      closeGroupEditor();
      showToast(t('groupCreated', 'Mailgruppe wurde erstellt.'));
    });
  }

  async function openGroupContentEditor(groupId) {
    const campaign = view.campaigns.find((item) => item.id === groupId);
    if (!campaign) return;
    const descriptor = emailGroupDescriptor(campaign);
    const content = campaign.payload?.mail_group?.content || {};
    const envelope = content.editor_envelope || {};
    if (view.contentEditor) await view.contentEditor.destroy();
    view.contentEditor = null;
    view.contentGroupId = campaign.id;
    refs.contentTitle.textContent = campaign.name;
    refs.contentSurface.dataset.contextRecordId = campaign.id;
    refs.contentSurface.dataset.contextRecordType = 'outbound_campaign';
    refs.contentSurface.dataset.contextLabel = campaign.name;
    refs.contentSurface.hidden = false;
    refs.saveContent.disabled = false;
    view.contentEditor = await createMailContentEditor({
      api,
      getActionIcon,
      host: refs.contentHost,
      commandHost: refs.contentControls,
      locale: currentLocale(),
      mode: descriptor.contentMode === 'word' ? 'rich-text' : 'html',
      documentArtifact: envelope.documentArtifact || null,
      htmlDocument: envelope.htmlDocument || {},
      mergeTags: {
        vorname: 'Anna',
        nachname: 'Beispiel',
        unternehmen: 'Beispiel GmbH',
        email: 'anna@example.test',
      },
      onEvent({ name }) {
        if (name === 'change') refs.saveContent.disabled = false;
      },
    });
    refs.contentHost.querySelector('button, [tabindex]')?.focus?.({ preventScroll: true });
  }

  async function closeGroupContentEditor() {
    await view.contentEditor?.destroy?.();
    view.contentEditor = null;
    view.contentGroupId = '';
    refs.contentHost.replaceChildren();
    delete refs.contentSurface.dataset.contextRecordId;
    delete refs.contentSurface.dataset.contextRecordType;
    delete refs.contentSurface.dataset.contextLabel;
    refs.contentSurface.hidden = true;
  }

  async function saveGroupContent() {
    if (!view.contentEditor || !view.contentGroupId || view.busy) return;
    const campaign = view.campaigns.find((item) => item.id === view.contentGroupId);
    if (!campaign) return;
    await runBusy(async () => {
      assertCanWriteCampaigns();
      refs.saveContent.disabled = true;
      const envelope = await view.contentEditor.serialize();
      const revisionId = `mail_content_${crypto.randomUUID()}`;
      const sourcePayload = envelope.mode === 'rich-text'
        ? {
          document_id: envelope.documentArtifact?.documentId || '',
          document_version_id: envelope.documentArtifact?.versionId || '',
        }
        : {
          format: envelope.format,
          document: envelope.htmlDocument || {},
          mjml: String(envelope.mjml || ''),
        };
      const compiledHtml = String(envelope.compiledHtml || envelope.html || '');
      const compiledText = String(envelope.compiledText || envelope.text || '');
      const sourceSha256 = String(envelope.sourceSha256 || '') || await sha256Text(stableJson(sourcePayload));
      const compiledSha256 = compiledHtml ? await sha256Text(compiledHtml) : '';
      const revision = buildMailContentRevision({
        id: revisionId,
        groupId: campaign.id,
        editorKind: envelope.mode === 'rich-text' ? 'word' : 'email_visual',
        sourceRef: sourcePayload,
        sourceSha256,
        compiledHtmlRef: envelope.compiledHtmlRef || (compiledHtml ? { storage: 'embedded', media_type: 'text/html', content: compiledHtml } : null),
        compiledTextRef: envelope.compiledTextRef || (compiledText ? { storage: 'embedded', media_type: 'text/plain', content: compiledText } : null),
        compiledAssets: envelope.compiledAssets || [],
        diagnostics: envelope.diagnostics || [],
        compiledSha256,
        compilerId: String(envelope.compilerId || ''),
        state: compiledHtml ? 'frozen' : 'draft',
      });
      const groupPayload = campaign.payload?.mail_group || {};
      const nextContent = appendMailContentRevision(groupPayload.content || {}, revision);
      const next = {
        ...campaign,
        payload: {
          ...(campaign.payload || {}),
          mail_group: {
            ...groupPayload,
            content: {
              ...nextContent,
              mode: envelope.mode === 'rich-text' ? 'word' : 'email_visual',
              editor_envelope: envelope,
            },
          },
        },
        updated_at_ms: Date.now(),
      };
      await putRecord(collections.outbound_campaigns, next);
      view.campaigns = view.campaigns.map((item) => item.id === next.id ? next : item);
      const adoptedMessages = applyContentRevisionToPending(view.outboundMessages, revision);
      const changedMessages = adoptedMessages.filter((message, index) => message !== view.outboundMessages[index]);
      for (const message of changedMessages) {
        await putRecord(collections.outbound_messages, message);
      }
      view.outboundMessages = adoptedMessages;
      view.contentEditor.markClean();
      showToast('Gruppeninhalt gespeichert. Bereits versendete E-Mails bleiben unverändert.');
      renderNavigation();
      renderDetail();
    }, (error) => {
      refs.saveContent.disabled = false;
      // Word-Inhalt einfrieren (mail.content.freeze, Strang MAIL3) gescheitert → eigener Zonencode mit Befehls-ID.
      if (error?.command_id) report('mail.command.mail.content.freeze', error.message, 'error', error.command_id);
      else notify('error', error.message || t('commandFailed', 'Die Mail-Aktion ist fehlgeschlagen.'));
    });
  }

  async function openMailboxAdmin() {
    if (!isGlobalMailAdmin(actorUser())) {
      report('mail.mailserver.denied', t('mailboxAdminDenied', 'Nur Administratoren dürfen Postfächer verwalten.'), 'warn');
      return;
    }
    if (!(await releaseComposer())) return;
    closeGroupEditor();
    closeMobileNavigation();
    view.mailserver.open = true;
    view.mailserver.error = '';
    view.mailserver.message = '';
    refs.mailboxAdmin.hidden = false;
    renderMailboxAdmin();
    // Fokus in die Schublade (wie beim Verfassen), damit Esc sie auch nach einer Rückfrage schließt (FIX10).
    window.setTimeout(() => { if (!refs.mailboxAdmin.hidden) refs.closeMailboxAdmin?.focus(); }, 0);
    await loadMailserverConfig();
  }

  function closeMailboxAdmin() {
    view.mailserver.open = false;
    view.mailserver.pendingDelete = '';
    if (refs.mailboxPassword) refs.mailboxPassword.value = '';
    refs.mailboxAdmin.hidden = true;
  }

  function openMobileNavigation() {
    root.classList.add('is-mobile-nav-open');
  }

  function closeMobileNavigation() {
    root.classList.remove('is-mobile-nav-open');
  }

  function loadMailserverConfig() {
    // Kein Befehls-Leseweg (App-API 4.0): Domains, Postfächer, Laufzeit und Erreichbarkeit kommen live aus der
    // Plattform-Collection `mail_server_state` (readSnapshot → view.mailserver); Mutationen laufen als Befehle.
    renderMailboxAdmin();
  }

  async function saveMailserverRuntime(event) {
    event?.preventDefault?.();
    if (!isGlobalMailAdmin(actorUser()) || view.mailserver.loading) return;
    const payload = {
      enabled: view.mailserver.runtimeConfig.enabled !== false,
      hostname: refs.serverHostname.value.trim(),
      bind_host: refs.serverBindHost.value.trim(),
      smtp_port: Number(refs.serverSmtpPort.value),
      imap_port: Number(refs.serverImapPort.value),
      outbound_throttle_per_min: Number(refs.serverThrottle.value),
      max_connections: Number(refs.serverConnections.value),
    };
    await persistMailserverRuntime(payload, 'Serverkonfiguration gespeichert und angewendet.');
  }

  async function toggleMailserverRuntime() {
    if (!isGlobalMailAdmin(actorUser()) || view.mailserver.loading) return;
    const enabled = view.mailserver.runtimeConfig.enabled === false;
    await persistMailserverRuntime(
      { ...view.mailserver.runtimeConfig, enabled },
      enabled ? 'Mailserver gestartet.' : 'Mailserver gestoppt.',
    );
  }

  function persistMailserverRuntime(payload, successMessage) {
    dispatchCommand('ctox', 'ctox.mailserver.save_runtime', 'mailserver-runtime', payload, {
      renderAfter: false, doneText: successMessage,
    });
    renderMailboxAdmin();
  }

  async function saveMailserverDomain(event) {
    event?.preventDefault?.();
    if (!isGlobalMailAdmin(actorUser()) || view.mailserver.loading) return;
    const domainName = refs.domainName.value.trim().toLowerCase();
    if (!domainName || !domainName.includes('.')) {
      view.mailserver.error = 'Bitte eine gültige Versanddomain eingeben.';
      renderMailboxAdmin();
      return;
    }
    view.mailserver.error = '';
    dispatchCommand('ctox', 'ctox.mailserver.save_domain', domainName, {
      domain_name: domainName,
      dkim_selector: refs.dkimSelector.value.trim() || 'default',
      spf_record: refs.spfRecord.value.trim(),
      dmarc_record: refs.dmarcRecord.value.trim(),
    }, { renderAfter: false, doneText: 'Versanddomain gespeichert. Die DNS-Werte stehen unten zum Kopieren bereit.' });
    refs.domainName.value = '';
    refs.dmarcRecord.value = '';
    renderMailboxAdmin();
  }

  async function handleDomainAction(action, domainName) {
    if (!domainName || view.mailserver.loading) return;
    if (action === 'request-delete') {
      view.mailserver.pendingDomainDelete = domainName;
      renderMailboxAdmin();
      return;
    }
    if (action === 'cancel-delete') {
      view.mailserver.pendingDomainDelete = '';
      renderMailboxAdmin();
      return;
    }
    if (action !== 'confirm-delete') return;
    dispatchCommand('ctox', 'ctox.mailserver.delete_domain', domainName, { domain_name: domainName }, {
      renderAfter: false, doneText: 'Versanddomain gelöscht.',
    });
    view.mailserver.pendingDomainDelete = '';
    renderMailboxAdmin();
  }

  async function createMailbox(event) {
    event?.preventDefault?.();
    if (!isGlobalMailAdmin(actorUser()) || view.mailserver.loading) return;
    const username = refs.mailboxUsername.value.trim().toLowerCase();
    const ownerUserId = refs.mailboxOwner.value.trim();
    const password = refs.mailboxPassword.value;
    refs.mailboxPassword.value = '';
    if (!isEmailAddress(username) || !password || !ownerUserId || !view.users.some((user) => (user.id || user.user_id) === ownerUserId)) {
      view.mailserver.error = t('mailboxRequired', 'Bitte eine gültige E-Mail-Adresse und ein Passwort eingeben.');
      view.mailserver.message = '';
      renderMailboxAdmin();
      return;
    }
    view.mailserver.error = '';
    // Passwort geht wie bisher im Befehls-Payload an den Core (Secret-Intake, V4-HANDLERS.md); der Handler sieht es nie.
    dispatchCommand('ctox', 'ctox.mailserver.save_user', username, {
      username,
      owner_user_id: ownerUserId,
      password,
    }, { renderAfter: false, doneText: t('mailboxCreated', 'Postfach wurde angelegt und dem Nutzer zugeordnet.') });
    refs.mailboxUsername.value = '';
    refs.mailboxOwner.value = '';
    render();
  }

  async function handleMailboxAction(action, username) {
    if (!isGlobalMailAdmin(actorUser()) || !username || view.mailserver.loading) return;
    if (action === 'request-delete') {
      view.mailserver.pendingDelete = username;
      renderMailboxAdmin();
      return;
    }
    if (action === 'cancel-delete') {
      view.mailserver.pendingDelete = '';
      renderMailboxAdmin();
      return;
    }
    if (action !== 'confirm-delete') return;
    dispatchCommand('ctox', 'ctox.mailserver.delete_user', username, { username }, {
      renderAfter: false, doneText: t('mailboxDeleted', 'Postfach wurde gelöscht.'),
    });
    view.mailserver.pendingDelete = '';
    render();
  }

  function renderMailboxAdmin() {
    const admin = isGlobalMailAdmin(actorUser());
    if (!admin) {
      refs.mailboxAdmin.hidden = true;
      return;
    }
    refs.mailboxAdmin.hidden = !view.mailserver.open;
    if (!view.mailserver.open) return;
    refs.mailboxAdmin.setAttribute('aria-busy', String(view.mailserver.loading));
    refs.createMailbox.disabled = view.mailserver.loading;
    refs.reloadMailboxes.disabled = view.mailserver.loading;

    const config = view.mailserver.runtimeConfig || {};
    const status = view.mailserver.runtimeStatus || {};
    const health = view.mailserver.health || {};
    const running = config.enabled !== false && status.state !== 'stopped';
    // Laufzustand nur als Punkt (is-running/is-degraded) und über den Schaltknopf; Beeinträchtigung → Zone.
    refs.serverState.textContent = 'Mailserver';
    if (running && Object.keys(health).length && !(health.smtp_reachable && health.imap_reachable)) {
      api.status.report({ level: 'warn', code: 'mail.mailserver.health', text: 'Mailserver ist beeinträchtigt (SMTP/IMAP nicht erreichbar).' });
    } else api.status.clear('mail.mailserver.health');
    refs.serverEndpoints.textContent = `SMTP ${config.bind_host || '127.0.0.1'}:${config.smtp_port || 2525} · IMAP ${config.bind_host || '127.0.0.1'}:${config.imap_port || 1143}`;
    refs.serverDot.classList.toggle('is-running', running && health.smtp_reachable && health.imap_reachable);
    refs.serverDot.classList.toggle('is-degraded', running && !(health.smtp_reachable && health.imap_reachable));
    refs.toggleServer.textContent = running ? 'Stoppen' : 'Starten';
    refs.toggleServer.disabled = view.mailserver.loading;
    refs.queueCount.textContent = String(health.queue_pending || 0);
    refs.deliveredCount.textContent = String(health.delivery_success || 0);
    refs.failedCount.textContent = String(health.delivery_failed || 0);
    refs.serverHostname.value = config.hostname || 'localhost';
    refs.serverBindHost.value = config.bind_host || '127.0.0.1';
    refs.serverSmtpPort.value = String(config.smtp_port || 2525);
    refs.serverImapPort.value = String(config.imap_port || 1143);
    refs.serverThrottle.value = String(config.outbound_throttle_per_min || 120);
    refs.serverConnections.value = String(config.max_connections || 10);
    refs.saveServer.disabled = view.mailserver.loading;
    refs.saveDomain.disabled = view.mailserver.loading;

    const domains = view.mailserver.domains.filter((domain) => mailserverDomain(domain));
    refs.domainList.innerHTML = domains.map((domain) => renderDomainCard(domain)).join('');

    const users = view.mailserver.users.map(mailserverUsername).filter(Boolean).sort((a, b) => a.localeCompare(b));
    refs.mailboxCount.textContent = String(users.length);
    refs.mailboxList.innerHTML = users.map((username) => renderMailboxRow(username)).join('');

    // Zustand nur in die Zustandszone (PORTING R11).
    if (view.mailserver.error) report('mail.mailserver', view.mailserver.error);
    else api.status.clear('mail.mailserver');
  }

  function renderDomainCard(domain) {
    const name = mailserverDomain(domain);
    const pending = view.mailserver.pendingDomainDelete === name;
    const actions = pending
      ? `<button type="button" class="ctox-button ctox-button--sm mail-mailbox-delete" data-mail-domain-action="confirm-delete" data-mail-domain-name="${escapeAttribute(name)}">Wirklich löschen</button><button type="button" class="ctox-button ctox-button--sm" data-mail-domain-action="cancel-delete" data-mail-domain-name="${escapeAttribute(name)}">Abbrechen</button>`
      : `<button type="button" class="ctox-button ctox-button--sm" data-mail-domain-action="request-delete" data-mail-domain-name="${escapeAttribute(name)}">Entfernen</button>`;
    const selector = String(domain.dkim_selector || 'default');
    const publicKey = String(domain.dkim_public_key || '').replace(/-----[^-]+-----|\s+/g, '');
    const dkim = `v=DKIM1; k=rsa; p=${publicKey}`;
    return `<article class="mail-domain-card"><header><div><strong>${escapeHtml(name)}</strong><span>DNS-Konfiguration</span></div><div>${actions}</div></header><dl><div><dt>${escapeHtml(selector)}._domainkey</dt><dd>${escapeHtml(dkim)}</dd></div><div><dt>@ · SPF</dt><dd>${escapeHtml(domain.spf_record || '')}</dd></div><div><dt>_dmarc</dt><dd>${escapeHtml(domain.dmarc_record || '')}</dd></div></dl></article>`;
  }

  function renderMailboxRow(username) {
    const account = view.accounts.find((candidate) => (
      String(candidate.address || '').toLowerCase() === username.toLowerCase()
      || String(candidate.id || '').toLowerCase() === `email:${username.toLowerCase()}`
    ));
    const owner = String(accountProfile(account).owner_user_id || accountProfile(account).ownerUserId || '').trim();
    const ownerUser = view.users.find((user) => String(user.id || user.user_id) === owner);
    const ownerLabel = owner
      ? `${t('assignedTo', 'Zugeordnet zu')}: ${ownerUser ? userDisplayName(ownerUser) : owner}`
      : t('unassigned', 'Nicht zugeordnet');
    const pending = view.mailserver.pendingDelete === username;
    const actions = pending
      ? `<button type="button" class="ctox-button ctox-button--sm mail-mailbox-delete" data-mailbox-action="confirm-delete" data-mailbox-username="${escapeAttribute(username)}">${escapeHtml(t('confirmDelete', 'Wirklich löschen'))}</button>
         <button type="button" class="ctox-button ctox-button--sm" data-mailbox-action="cancel-delete" data-mailbox-username="${escapeAttribute(username)}">${escapeHtml(t('cancel', 'Abbrechen'))}</button>`
      : `<button type="button" class="ctox-button ctox-button--sm mail-mailbox-delete" data-mailbox-action="request-delete" data-mailbox-username="${escapeAttribute(username)}">${escapeHtml(t('delete', 'Löschen'))}</button>`;
    return `<div class="mail-mailbox-row">
      <div class="mail-mailbox-identity"><div class="mail-mailbox-address">${escapeHtml(username)}</div><div class="mail-mailbox-owner">${escapeHtml(ownerLabel)}</div></div>
      <div class="mail-mailbox-actions">${actions}</div>
    </div>`;
  }

  function dispatchOutbound(commandType, recordId, payload, options = {}) {
    return dispatchCommand('outbound', commandType, recordId, payload, options);
  }

  // App-API 4.0: Befehl ohne Warten (PORTING R5). failed → Zustandszone, done → Zone leeren. Ergebnisse
  // kommen über die Collections zurück (Handler projizieren), nie über das Befehlsdokument.
  function dispatchCommand(module, commandType, recordId, payload, options = {}) {
    const handle = sendCommand(commandType, {
      ...payload,
      record_id: recordId,
      client_context: { source_module: 'mail', target_module: module, inbound_channel: 'business_os.mail' },
    }, options.doneText || '', options.onDone);
    if (options.renderAfter !== false) render();
    return { id: handle.id, command_id: handle.id, status: 'queued' };
  }

  // doneText: Erfolgsmeldung erst bei `done` (vorher kam sie nach dem Warten auf den Terminalstatus).
  function sendCommand(commandType, payload, doneText = '', onDone = null) {
    const code = `mail.command.${commandType}`;
    const handle = api.command(commandType, payload);
    let off = null;
    let finished = false;
    off = handle.status.subscribe((state) => {
      if (finished) return;
      if (state?.status === 'failed') report(code, state.error || commandType, 'error', handle.id);
      else if (state?.status === 'done') {
        api.status.clear(code);
        if (doneText) notify('success', doneText);
        onDone?.();
      }
      if (state?.status === 'done' || state?.status === 'failed') {
        finished = true;
        off?.();
      }
    });
    if (finished) off?.();
    cleanups.push(() => off?.());
    handle.local.catch((error) => report(code, error?.message || error, 'error', handle.id));
    return handle;
  }

  async function runBusy(operation, onError) {
    if (view.busy) return;
    view.busy = true;
    root.classList.add('is-busy');
    try {
      await operation();
    } catch (error) {
      console.error('[mail] action failed', error);
      if (onError) onError(error);
      else notify('error', error.message || t('commandFailed', 'Die Mail-Aktion ist fehlgeschlagen.'));
    } finally {
      view.busy = false;
      root.classList.remove('is-busy');
    }
  }

  // Zustand nur in die Zustandszone (PORTING R11): Fehler/Prüfhinweise → api.status.report.
  function setComposerStatus(message, error = false) {
    if (error && message) report('mail.composer', message, 'warn');
    else api.status.clear('mail.composer');
  }

  function showToast(message) {
    notify('info', message);
  }

  function notify(type, message) {
    report(type === 'error' ? 'mail.action.failed' : 'mail.notice', message, type === 'error' ? 'error' : 'info');
  }

  function filteredAccountThreads() {
    const allowedAccounts = new Set(view.accounts.map((account) => account.id));
    return view.threads.filter((thread) => (
      allowedAccounts.has(thread.account_key)
      && (!view.accountKey || thread.account_key === view.accountKey)
    ));
  }

  function filteredAccountMessages() {
    return view.accountKey
      ? view.communicationMessages.filter((message) => message.account_key === view.accountKey)
      : view.communicationMessages;
  }

  function sentAccountThreads() {
    const messages = filteredAccountMessages();
    return filteredAccountThreads().filter((thread) => messages.some((message) => messageBelongsToThread(message, thread)
      && messageIsSent(message)));
  }

  function inboxAccountThreads() {
    const messages = filteredAccountMessages();
    return filteredAccountThreads().filter((thread) => {
      const threadMessages = messages.filter((message) => messageBelongsToThread(message, thread));
      return threadMessages.some(messageIsInbox)
        || !threadMessages.some(messageIsSent);
    });
  }

  function filteredAccountOutboundMessages() {
    const allowedAccounts = new Set(view.accounts.map((account) => account.id));
    return view.outboundMessages.filter((message) => {
      const accountKey = message.communication_account_key || message.sender_account_id;
      if (!allowedAccounts.has(accountKey)) return false;
      return !view.accountKey || accountKey === view.accountKey;
    });
  }

  function assertCanWriteCampaigns() {
    // Schreibrecht entscheidet der Core (api.put → forbidden); kein UI-Gate (App-API 4.0, R9).
  }

  function scopeRecords() {
    const allThreads = filteredAccountThreads().map((thread) => ({ ...thread, __kind: 'thread' }));
    const inbound = inboxAccountThreads().map((thread) => ({ ...thread, __kind: 'thread' }));
    const outbound = filteredAccountOutboundMessages().map((message) => ({ ...message, __kind: 'outbound' }));
    let records;
    if (view.scopeType === 'campaign') {
      records = outbound.filter((message) => message.campaign_id === view.scopeId);
    } else if (view.scopeId === 'all') {
      records = [...allThreads, ...outbound];
    } else if (view.scopeId === 'outbound') {
      records = sentAccountThreads().map((thread) => ({ ...thread, __kind: 'thread' }));
    } else if (view.scopeId === 'approval') {
      records = outbound.filter((message) => String(message.approval_status || '') === 'awaiting_approval');
    } else if (view.scopeId === 'failed') {
      records = outbound.filter((message) => String(message.send_status || '').toLowerCase().includes('fail'));
    } else if (view.scopeId === 'routed') {
      records = [...allThreads, ...outbound].filter((record) => routeCommandForRecord(record, view.commands));
    } else {
      records = inbound;
    }
    return records;
  }

  function currentRecords() {
    const grammar = view.listGrammar;
    let records = scopeRecords();
    if (grammar.band === 'outbound' && view.scopeType === 'campaign') {
      records = records.filter((record) => record.__kind === 'outbound');
    } else if (grammar.band !== 'all') {
      records = records.filter((record) => recordMatchesBand(record, grammar.band, filteredAccountMessages(), view.commands));
    }
    const status = grammar.filters.status || 'all';
    if (status !== 'all') records = records.filter((record) => recordMatchesStatus(record, status, view.commands));
    if (grammar.search) records = records.filter((record) => JSON.stringify(record).toLowerCase().includes(grammar.search));
    const sort = grammar.filters.sort || 'recent';
    return [...records].sort((a, b) => {
      if (sort === 'subject') return recordSubject(a).localeCompare(recordSubject(b));
      const delta = recordTime(a) - recordTime(b);
      return sort === 'oldest' ? delta : -delta;
    });
  }

  function currentScopeLabel() {
    if (view.scopeType === 'campaign') return view.campaigns.find((campaign) => campaign.id === view.scopeId)?.name || t('campaign', 'Kampagne');
    return ({
      all: t('allTraffic', 'Alle Nachrichten'),
      inbound: t('inbox', 'Posteingang'),
      outbound: t('sent', 'Gesendet'),
      approval: t('awaiting', 'Freigabe'),
      failed: t('failed', 'Fehler'),
      routed: t('routed', 'Geroutete E-Mails'),
    })[view.scopeId] || t('inbox', 'Posteingang');
  }

  function currentPageRecords() {
    const records = currentRecords();
    return records.slice(view.page * view.pageSize, (view.page + 1) * view.pageSize);
  }

  function changePage(delta) {
    const maxPage = Math.max(0, Math.ceil(currentRecords().length / view.pageSize) - 1);
    view.page = clamp(view.page + delta, 0, maxPage);
    renderList();
    refs.recordList?.querySelector('[data-mail-record-kind]')?.focus?.();
  }

  function allSelectableRecords() {
    return [
      ...filteredAccountThreads().map((record) => ({ ...record, __kind: 'thread' })),
      ...filteredAccountOutboundMessages().map((record) => ({ ...record, __kind: 'outbound' })),
    ];
  }

  function selectedMailRecords() {
    return allSelectableRecords().filter((record) => view.selectedRecords.has(mailRecordKey(record)));
  }

  function renderBulkBar(pageRows = currentPageRecords(), allRows = currentRecords()) {
    const selected = selectedMailRecords();
    refs.bulkbar.hidden = selected.length === 0;
    refs.selectedCount.textContent = `${selected.length} ${t('selected', 'ausgewählt')}`;
    const pageKeys = pageRows.map(mailRecordKey);
    const selectedOnPage = pageKeys.filter((key) => view.selectedRecords.has(key)).length;
    refs.selectPage.checked = pageKeys.length > 0 && selectedOnPage === pageKeys.length;
    refs.selectPage.indeterminate = selectedOnPage > 0 && selectedOnPage < pageKeys.length;
    refs.selectFiltered.hidden = allRows.length === 0 || allRows.every((record) => view.selectedRecords.has(mailRecordKey(record)));
    refs.selectFiltered.textContent = `${t('selectAllFiltered', 'Alle gefilterten auswählen')} (${allRows.length})`;
    refs.bulkApproval.disabled = !selected.some((record) => record.__kind === 'outbound' && canRequestApproval(record));
    refs.bulkSend.disabled = !selected.some((record) => record.__kind === 'outbound' && String(record.approval_status || '') === 'approved');
    refs.bulkRoute.disabled = selected.length === 0;
    refs.bulkExport.disabled = selected.length === 0;
    refs.bulkbar.dataset.filteredCount = String(allRows.length);
  }

  function toggleRecordSelection(key) {
    if (!key) return;
    if (view.selectedRecords.has(key)) view.selectedRecords.delete(key);
    else view.selectedRecords.add(key);
    renderListSelection();
    renderBulkBar();
  }

  function togglePageSelection() {
    const pageRows = currentPageRecords();
    const shouldSelect = refs.selectPage.checked;
    pageRows.forEach((record) => {
      const key = mailRecordKey(record);
      if (shouldSelect) view.selectedRecords.add(key);
      else view.selectedRecords.delete(key);
    });
    renderListSelection();
    renderBulkBar(pageRows, currentRecords());
  }

  function selectAllFiltered() {
    currentRecords().forEach((record) => view.selectedRecords.add(mailRecordKey(record)));
    renderListSelection();
    renderBulkBar();
  }

  function clearSelection(renderAfter = true) {
    view.selectedRecords.clear();
    if (renderAfter) {
      renderListSelection();
      renderBulkBar();
    }
  }

  function exportSelectedMail() {
    const records = selectedMailRecords();
    if (!records.length) return;
    downloadJson({ schema: 'ctox.mail.bulk-export.v1', exported_at: new Date().toISOString(), records }, `ctox-mail-auswahl-${records.length}.json`, root);
    notify('success', `${records.length} ${t('recordsExported', 'Datensätze exportiert.')}`);
  }

  async function runBulkOutboundAction(action) {
    const records = selectedMailRecords().filter((record) => record.__kind === 'outbound');
    const eligible = records.filter((record) => action === 'request-approval'
      ? canRequestApproval(record)
      : String(record.approval_status || '') === 'approved');
    if (!eligible.length) return;
    await runBusy(async () => {
      const commandType = action === 'request-approval' ? 'outbound.message.request_approval' : 'outbound.message.send_approved';
      const doneText = `${eligible.length} ${action === 'request-approval' ? t('submittedForApproval', 'Mails zur Freigabe eingereicht.') : t('queuedForSending', 'Mails zum Versand eingereiht.')}`;
      eligible.forEach((record, index) => {
        dispatchOutbound(commandType, record.id, { message_id: record.id }, {
          renderAfter: false,
          doneText: index === eligible.length - 1 ? doneText : '',
        });
      });
      clearSelection(false);
      render();
    });
  }

  function openRouteDrawer() {
    const records = selectedMailRecords();
    if (!records.length) return;
    closeComposer();
    closeGroupEditor();
    closeMailboxAdmin();
    view.route.open = true;
    view.route.error = '';
    view.route.status = '';
    refs.routeDrawer.hidden = false;
    revealInspector('route');
    const destinations = view.routeDestinations.length ? view.routeDestinations : defaultMailRouteDestinations();
    refs.routeDestination.innerHTML = `<option value="">— ${escapeHtml(t('selectDestination', 'Ziel-App wählen'))} —</option>${destinations.map((destination) => `<option value="${escapeAttribute(destination.id)}">${escapeHtml(destination.title)}</option>`).join('')}`;
    refs.routeSelection.textContent = `${records.length} ${t('messages', 'Nachrichten')} · ${records.filter((record) => record.__kind === 'thread').length} Inbound · ${records.filter((record) => record.__kind === 'outbound').length} Outbound`;
    refs.routeNote.value = '';
    window.setTimeout(() => refs.routeDestination.focus(), 0);
  }

  function closeRouteDrawer() {
    if (view.route.busy) return;
    view.route.open = false;
    refs.routeDrawer.hidden = true;
    if (view.selectedKind && view.selectedId) revealInspector('detail');
    else concealInspector();
  }

  async function confirmRoute() {
    if (view.route.busy) return;
    const destinationModule = refs.routeDestination.value;
    const records = selectedMailRecords();
    if (!destinationModule || !records.length) {
      report('mail.route', t('routeDestinationRequired', 'Bitte eine Ziel-App wählen.'), 'warn');
      return;
    }
    view.route.busy = true;
    refs.confirmRoute.disabled = true;
    api.status.clear('mail.route');
    const batchId = `mail_route_${crypto.randomUUID()}`;
    try {
      const commands = buildMailRouteCommands({
        batchId,
        destinationModule,
        mode: refs.routeMode.value,
        note: refs.routeNote.value.trim(),
        records,
        actor: actorUser(),
      });
      const commandIds = [];
      view.route.status = `${records.length} ${t('routedTo', 'E-Mails geroutet an')} ${routeDestinationTitle(destinationModule, view.routeDestinations)} · ${batchId}`;
      commands.forEach((command, index) => {
        const handle = sendCommand(command.command_type, {
          ...(command.payload || {}),
          record_id: command.record_id,
          client_context: command.client_context || { source_module: 'mail' },
        }, index === commands.length - 1 ? view.route.status : '');
        commandIds.push(handle.id);
        view.commands.unshift({ ...command, id: handle.id, command_id: handle.id, status: 'queued' });
      });
      view.route.lastCommandId = commandIds[0] || '';
      clearSelection(false);
      view.scopeType = 'queue';
      view.scopeId = 'routed';
      renderNavigation();
      renderList();
    } catch (error) {
      view.route.error = error?.message || String(error);
      report('mail.route', view.route.error);
    } finally {
      view.route.busy = false;
      refs.confirmRoute.disabled = false;
    }
  }

  async function openMailImporter() {
    closeMobileNavigation();
    // shared/universal-importer.js ist noch v3 (ctx.host/ctx.sync); hier nur der Host (dispatch:false).
    await openUniversalImporter({ host: root }, {
      side: 'left',
      moduleId: 'mail',
      entityType: 'series_email_recipients',
      commandType: 'mail.series_email.import',
      title: t('importSeriesEmail', 'Empfänger für Serien-E-Mail importieren'),
      kicker: t('mailCampaign', 'Mail-Kampagne'),
      defaultSource: 'excel',
      showFileExplorer: false,
      defaultTitle: `${t('seriesEmail', 'Serien-E-Mail')} ${new Date().toLocaleDateString(currentLocale() === 'en' ? 'en-US' : 'de-DE')}`,
      submitLabel: t('transferToMail', 'In Serien-E-Mail übernehmen'),
      submittingLabel: t('transferToMail', 'In Serien-E-Mail übernehmen'),
      doneLabel: t('importDone', 'Empfänger übernommen.'),
      closeOnSubmit: true,
      dispatch: false,
      definition: { target: 'mail_series_email', accepted_columns: ['email', 'e-mail', 'mail', 'recipient', 'empfaenger'] },
      clientContext: { campaign_id: view.scopeType === 'campaign' ? view.scopeId : '' },
      onImport: async ({ payload }) => {
        const recipients = extractImportRecipients(payload);
        if (!recipients.length) throw new Error(t('missingRecipient', 'Keine gültige Empfängeradresse gefunden.'));
        const campaignId = view.scopeType === 'campaign' ? view.scopeId : '';
        openComposer({ series: true, campaignId, to: recipients });
        notify('success', `${recipients.length} ${t('recipientsTransferred', 'Empfänger übernommen.')}`);
        return { status: 'prepared', recipients: recipients.length };
      },
    });
  }

  function exportVisibleMail() {
    const records = currentRecords().map(({ __kind, ...record }) => ({ kind: __kind, ...record }));
    downloadJson({
      schema: 'ctox.mail.export.v1',
      exported_at: new Date().toISOString(),
      scope: { type: view.scopeType, id: view.scopeId, account_key: view.accountKey || null },
      records,
    }, `ctox-mail-${new Date().toISOString().slice(0, 10)}.json`, root);
    notify('success', `${records.length} ${t('recordsExported', 'Datensätze exportiert.')}`);
  }

  function closeDetail() {
    view.selectedKind = '';
    view.selectedId = '';
    concealInspector();
    renderListSelection();
  }

  function revealInspector(mode = 'detail') {
    root.classList.remove('is-inspector-hidden');
    root.classList.add('is-inspector-open', 'is-detail-open');
    root.classList.toggle('is-route-open', mode === 'route');
  }

  function concealInspector() {
    root.classList.remove('is-inspector-open', 'is-route-open', 'is-detail-open');
    root.classList.add('is-inspector-hidden');
  }

  function accountLabel(accountKey) {
    const account = view.accounts.find((item) => item.id === accountKey);
    return account?.address || accountKey;
  }

  // Deep-Link nur über api.shell.onRoute (params), nicht mehr über location.hash.
  function applyDeepLink(args = null) {
    const params = new URLSearchParams(Object.entries(args || {}).filter(([, value]) => value != null && typeof value !== 'object').map(([key, value]) => [key, String(value)]));
    const campaignId = params.get('campaign_id') || '';
    const action = params.get('action') || '';
    const recipients = parseRecipientAddresses(params.get('recipients') || '');
    const messageId = String(args?.message_id || params.get('message_id') || '').trim();
    const threadKey = String(args?.thread_key || params.get('thread_key') || '').trim();
    const returnThreadId = String(args?.return_thread_id || params.get('return_thread_id') || '').trim();
    if (campaignId && view.campaigns.some((campaign) => campaign.id === campaignId)) {
      view.scopeType = 'campaign';
      view.scopeId = campaignId;
    }
    if (action === 'series-email') {
      view.pendingSeriesHandoff = {
        series: true,
        campaignId,
        to: recipients,
        subject: params.get('subject') || '',
        body: params.get('body') || '',
      };
    }
    if (messageId) {
      const communicationMessage = view.communicationMessages.find((item) => item.id === messageId);
      view.selectedKind = communicationMessage?.thread_key ? 'thread' : 'outbound';
      view.selectedId = communicationMessage?.thread_key || messageId;
    } else if (threadKey) {
      view.selectedKind = 'thread';
      view.selectedId = threadKey;
    }
    if (returnThreadId && (messageId || threadKey)) {
      view.requestedSourceFocus = { returnThreadId, recordId: messageId || threadKey,
        kind: messageId ? 'message' : 'thread' };
    }
  }

  function reportRequestedSourceFocus() {
    const request = view.requestedSourceFocus;
    if (!request || view.loading) return;
    const communicationMessage = request.kind === 'message'
      ? view.communicationMessages.find((item) => item.id === request.recordId
        && view.threads.some((thread) => thread.id === item.thread_key))
      : null;
    if (communicationMessage && (view.selectedKind !== 'thread'
      || view.selectedId !== communicationMessage.thread_key)) {
      view.selectedKind = 'thread';
      view.selectedId = communicationMessage.thread_key;
      renderListSelection();
      renderDetail();
    }
    const recordElement = [...refs.detail.querySelectorAll('[data-context-record-id]')]
      .find((item) => item.dataset.contextRecordId === request.recordId
        && (request.kind === 'thread'
          ? item.dataset.contextRecordType === 'communication_thread'
          : ['communication_message', 'outbound_message'].includes(item.dataset.contextRecordType)));
    const focused = Boolean(recordElement);
    if (!focused && request.reportedUnavailable) return;
    if (focused) {
      recordElement.focus?.({ preventScroll: true });
      recordElement.scrollIntoView?.({ block: 'nearest' });
    }
    if (focused) view.requestedSourceFocus = null;
    else request.reportedUnavailable = true;
    root.dispatchEvent(new CustomEvent('ctox-business-os-record-focus', {
      bubbles: true,
      detail: { module: 'mail', status: focused ? 'record_focused' : 'unavailable',
        recordId: request.recordId, returnThreadId: request.returnThreadId },
    }));
  }
}

function collectRefs(root) {
  const one = (selector) => root.querySelector(selector);
  return {
    leftPane: one('[data-mail-left-pane]'),
    navigationTitle: one('[data-mail-navigation-title]'),
    listPane: one('[data-mail-list-pane]'),
    account: one('[data-mail-account]'),
    search: one('[data-mail-search]'),
    compose: one('[data-mail-compose]'),
    composePrimary: one('[data-mail-compose-primary]'),
    composerTitle: one('[data-mail-composer-title]'),
    composerKicker: one('[data-mail-composer-kicker]'),
    importButtons: [...root.querySelectorAll('[data-mail-import], [data-mail-list-import]')],
    exportButtons: [...root.querySelectorAll('[data-mail-export], [data-mail-list-export]')],
    settings: one('[data-mail-settings]'),
    scopeList: one('[data-mail-scope-list]'),
    groupList: one('[data-mail-group-list]'),
    newGroup: one('[data-mail-new-group]'),
    sidebarFooter: one('[data-mail-sidebar-footer]'),
    listKicker: one('[data-mail-list-kicker]'),
    listTitle: one('[data-mail-list-title]'),
    recordList: one('[data-mail-record-list]'),
    listFooter: one('[data-mail-list-footer]'),
    prevPage: one('[data-mail-prev-page]'),
    nextPage: one('[data-mail-next-page]'),
    bulkbar: one('[data-mail-bulkbar]'),
    selectPage: one('[data-mail-select-page]'),
    selectFiltered: one('[data-mail-select-filtered]'),
    selectedCount: one('[data-mail-selected-count]'),
    bulkRoute: one('[data-mail-bulk-route]'),
    bulkApproval: one('[data-mail-bulk-approval]'),
    bulkSend: one('[data-mail-bulk-send]'),
    bulkExport: one('[data-mail-bulk-export]'),
    clearSelection: one('[data-mail-clear-selection]'),
    openNav: one('[data-mail-open-nav]'),
    closeNav: one('[data-mail-close-nav]'),
    closeDetail: one('[data-mail-close-detail]'),
    detailTitle: one('[data-mail-detail-title]'),
    detail: one('[data-mail-detail]'),
    contentSurface: one('[data-mail-content-surface]'),
    contentTitle: one('[data-mail-content-title]'),
    contentHost: one('[data-mail-content-host]'),
    contentControls: one('[data-mail-content-controls]'),
    closeContentEditor: one('[data-mail-close-content-editor]'),
    saveContent: one('[data-mail-save-content]'),
    composer: one('[data-mail-composer]'),
    closeComposer: one('[data-mail-close-composer]'),
    composeFrom: one('[data-mail-compose-from]'),
    composeTo: one('[data-mail-compose-to]'),
    composeCampaign: one('[data-mail-compose-campaign]'),
    composeSubject: one('[data-mail-compose-subject]'),
    composeBody: one('[data-mail-compose-body]'),
    saveDraft: one('[data-mail-save-draft]'),
    requestApproval: one('[data-mail-request-approval]'),
    sendNow: one('[data-mail-send-now]'),
    groupEditor: one('[data-mail-group-editor]'),
    closeGroup: one('[data-mail-close-group]'),
    groupName: one('[data-mail-group-name]'),
    groupTemplate: one('[data-mail-group-template]'),
    groupDescription: one('[data-mail-group-description]'),
    createGroup: one('[data-mail-create-group]'),
    mailboxAdmin: one('[data-mail-mailbox-admin]'),
    closeMailboxAdmin: one('[data-mail-close-mailbox-admin]'),
    reloadMailboxes: one('[data-mail-reload-mailboxes]'),
    serverForm: one('[data-mail-server-form]'),
    serverState: one('[data-mail-server-state]'),
    serverEndpoints: one('[data-mail-server-endpoints]'),
    serverDot: one('[data-mail-server-dot]'),
    toggleServer: one('[data-mail-toggle-server]'),
    queueCount: one('[data-mail-queue-count]'),
    deliveredCount: one('[data-mail-delivered-count]'),
    failedCount: one('[data-mail-failed-count]'),
    serverHostname: one('[data-mail-server-hostname]'),
    serverBindHost: one('[data-mail-server-bind-host]'),
    serverSmtpPort: one('[data-mail-server-smtp-port]'),
    serverImapPort: one('[data-mail-server-imap-port]'),
    serverThrottle: one('[data-mail-server-throttle]'),
    serverConnections: one('[data-mail-server-connections]'),
    saveServer: one('[data-mail-save-server]'),
    domainForm: one('[data-mail-domain-form]'),
    domainName: one('[data-mail-domain-name]'),
    dkimSelector: one('[data-mail-dkim-selector]'),
    spfRecord: one('[data-mail-spf-record]'),
    dmarcRecord: one('[data-mail-dmarc-record]'),
    saveDomain: one('[data-mail-save-domain]'),
    mailboxForm: one('[data-mail-mailbox-form]'),
    mailboxUsername: one('[data-mail-mailbox-username]'),
    mailboxOwner: one('[data-mail-mailbox-owner]'),
    mailboxPassword: one('[data-mail-mailbox-password]'),
    createMailbox: one('[data-mail-create-mailbox]'),
    domainList: one('[data-mail-domain-list]'),
    mailboxCount: one('[data-mail-mailbox-count]'),
    mailboxList: one('[data-mail-mailbox-list]'),
    routeDrawer: one('[data-mail-route-drawer]'),
    closeRoute: one('[data-mail-close-route]'),
    cancelRoute: one('[data-mail-cancel-route]'),
    confirmRoute: one('[data-mail-confirm-route]'),
    routeSelection: one('[data-mail-route-selection]'),
    routeDestination: one('[data-mail-route-destination]'),
    routeMode: one('[data-mail-route-mode]'),
    routeNote: one('[data-mail-route-note]'),
  };
}

function renderActionIcons(refs) {
  const icon = (name) => mailActionIcon(name);
  const assignments = [
    [refs.compose, 'edit'], [refs.newGroup, 'add'], [refs.settings, 'settings'], [refs.openNav, 'columns'],
    [refs.closeNav, 'close'], [refs.closeDetail, 'chevronLeft'],
    [refs.closeComposer, 'close'], [refs.closeGroup, 'close'], [refs.closeMailboxAdmin, 'close'],
    [refs.closeContentEditor, 'chevronLeft'], [refs.saveContent, 'check'],
    [refs.clearSelection, 'close'], [refs.closeRoute, 'close'],
    [refs.prevPage, 'chevronLeft'], [refs.nextPage, 'chevronRight'],
  ];
  refs.importButtons.forEach((button) => assignments.push([button, 'download']));
  refs.exportButtons.forEach((button) => assignments.push([button, 'export']));
  refs.leftPane?.querySelectorAll('[data-pg-tray-toggle]').forEach((button) => assignments.push([button, 'filter']));
  refs.listPane?.querySelectorAll('[data-pg-tray-toggle]').forEach((button) => assignments.push([button, 'filter']));
  refs.leftPane?.querySelectorAll('[data-pg-reset]').forEach((button) => assignments.push([button, 'refresh']));
  refs.listPane?.querySelectorAll('[data-pg-reset]').forEach((button) => assignments.push([button, 'refresh']));
  assignments.forEach(([button, name]) => { if (button) button.innerHTML = icon(name); });
}

// The shell normally supplies these glyphs. Keep a local stroke fallback so
// standalone fixtures and a briefly unavailable shell never render blank
// controls (especially close/navigation actions and delivery progress).
const MAIL_ICON_FALLBACK_PATHS = Object.freeze({
  add: 'M12 5v14M5 12h14',
  check: 'M4.5 12.5l5 5L19.5 7',
  chevronLeft: 'M15 6l-6 6 6 6',
  chevronRight: 'M9 6l6 6-6 6',
  clock: 'M12 4.5a7.5 7.5 0 1 1 0 15 7.5 7.5 0 0 1 0-15ZM12 8v4.5l3 2',
  close: 'M6 6l12 12M18 6L6 18',
  columns: 'M4 4h16v16H4zM10 4v16M16 4v16',
  download: 'M12 4v11M12 15l-4-4M12 15l4-4M5 19h14',
  edit: 'M4 20h4L19.5 8.5a2.1 2.1 0 0 0-3-3L5 17v3ZM14.5 6.5l3 3',
  export: 'M12 3v11M12 3 8 7M12 3l4 4M5 12v7h14v-7',
  eye: 'M3 12s3.5-6 9-6 9 6 9 6-3.5 6-9 6-9-6-9-6ZM12 9.5a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5Z',
  filter: 'M4 6h16M7 12h10M10 18h4',
  grid: 'M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z',
  link: 'M10 14a4 4 0 0 0 6 .4l3-3a4 4 0 0 0-5.6-5.6L12 7.2M14 10a4 4 0 0 0-6-.4l-3 3a4 4 0 0 0 5.6 5.6L12 16.8',
  list: 'M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01',
  more: 'M6 12h.01M12 12h.01M18 12h.01',
  refresh: 'M20 12a8 8 0 1 1-2.3-5.6M20 4v4h-4',
  send: 'M4 12 20 4l-4 16-4.5-6.5L4 12ZM11.5 13.5 20 4',
  settings: 'M12 8.5a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7ZM12 3v2.2M12 18.8V21M21 12h-2.2M5.2 12H3M18.4 5.6l-1.6 1.6M7.2 16.8l-1.6 1.6M18.4 18.4l-1.6-1.6M7.2 7.2 5.6 5.6',
  warning: 'M12 4 2.8 19.5h18.4L12 4ZM12 10v4M12 17h.01',
});

// App-API 4.0 hat keine Icon-API (app-api.d.ts OFFEN-D): Glyphen bleiben app-lokal.
function mailActionIcon(name, size = 16, strokeWidth = 1.8) {
  const path = MAIL_ICON_FALLBACK_PATHS[name] || MAIL_ICON_FALLBACK_PATHS.more;
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${strokeWidth}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" class="mail-action-icon mail-action-${name}"><path d="${path}"></path></svg>`;
}

// One-button view switch (operator directive, 31.08.2026). The button is an
// action, not a state, so it carries no aria-pressed; icon, aria-label and
// title always describe the view it switches TO. `data-pg-view` stays on it
// because shared/pane-grammar.js reads a pane's view from there - it must
// always hold the CURRENT view, otherwise an unrelated grammar emit (a search
// keystroke, a tray filter, a band tab) would flip the mode as a side effect.
function viewToggleLabel(currentView, t) {
  return currentView === 'list'
    ? t('showAsCards', 'Als Karten anzeigen')
    : t('showAsList', 'Als Liste anzeigen');
}

function syncViewToggleButton(pane, currentView, t) {
  const button = pane?.querySelector?.('[data-mail-view-toggle]');
  if (!button) return;
  const cards = currentView !== 'list';
  const label = viewToggleLabel(currentView, t);
  button.setAttribute('data-pg-view', cards ? 'cards' : 'list');
  // The shell's generic view-button wiring stamps aria-pressed on click; a
  // single toggle is an action, so the state attribute is removed again.
  button.removeAttribute('aria-pressed');
  button.setAttribute('aria-label', label);
  button.setAttribute('title', label);
  button.innerHTML = mailActionIcon(cards ? 'list' : 'grid');
}

function normalizePaneGrammar(detail, fallback) {
  return {
    search: String(detail?.search ?? fallback.search ?? '').trim().toLowerCase(),
    view: detail?.view || fallback.view || 'cards',
    band: detail?.band || fallback.band || '',
    filters: { ...(fallback.filters || {}), ...(detail?.filters || {}) },
  };
}

function mailCountsKnown(view) {
  return Boolean(view?.mailReadComplete && !view.loading && !view.mailReadError);
}

function writePaneCounts(pane, counts, known = true) {
  if (known) pane?.__ctoxPaneGrammar?.setCounts?.(counts);
  for (const [key, value] of Object.entries(counts || {})) {
    const node = pane?.querySelector?.(`[data-pg-count="${key}"]`);
    if (node) node.textContent = known ? ` (${Number(value || 0)})` : ' (…)';
  }
}

function userDisplayName(user) {
  return String(user?.display_name || user?.name || user?.email || user?.login || user?.id || user?.user_id || '—');
}

function mailRecordKey(record) {
  const kind = record?.__kind || (record?.thread_key ? 'thread' : 'outbound');
  return kind === 'thread'
    ? `${kind}:${String(record?.account_key || '')}:${String(record?.id || '')}`
    : `${kind}:${String(record?.id || '')}`;
}

// App-API 4.0: Dokument-ID ist `id` (communication_threads: id = thread_key, Kern §16.28; outbound: id).
function mailRecordId(record) {
  return String(record?.id || '');
}

function routeCommandForRecord(record, commands = []) {
  const recordId = mailRecordId(record);
  if (!recordId) return null;
  return [...(commands || [])]
    .filter((command) => {
      const payload = command?.payload || {};
      const sourceIds = arrayStrings(payload.source_record_ids || payload.sourceRecordIds);
      const isRoute = payload.route_kind === 'mail.bulk.route'
        || command?.client_context?.surface === 'mail.bulk.route';
      if (!isRoute || (!sourceIds.includes(recordId) && String(command?.record_id || '') !== recordId)) return false;
      if (!record?.thread_key) return true;
      const snapshot = payload.record_snapshot || payload.context?.record_snapshot;
      const snapshots = Array.isArray(snapshot?.records) ? snapshot.records : [snapshot];
      return snapshots.some((item) => item?.thread_key === recordId && item?.account_key === record.account_key);
    })
    .sort((a, b) => Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0))[0] || null;
}

function routeTargetLabel(command) {
  return String(command?.payload?.target_module || command?.client_context?.target_module || command?.module || 'App');
}

function defaultMailRouteDestinations() {
  return [
    { id: 'support', title: 'Support' },
    { id: 'tickets', title: 'Tickets' },
    { id: 'customers', title: 'Kunden' },
    { id: 'matching', title: 'Matching' },
    { id: 'outbound', title: 'Outbound' },
    { id: 'documents', title: 'Dokumente' },
    { id: 'reports', title: 'Reports' },
  ];
}

function routeDestinationsFromCatalog(catalogs = [], permissions) {
  const seen = new Map(defaultMailRouteDestinations().map((item) => [item.id, item]));
  for (const catalog of catalogs || []) {
    // v4: eine business_apps-Zeile ist selbst der Eintrag; V2-Katalogzeilen tragen modules[].
    for (const module of Array.isArray(catalog?.modules) ? catalog.modules : (catalog?.id ? [catalog] : [])) {
      const id = String(module?.id || module?.module_id || '').trim();
      if (!id || id === 'mail' || module?.hidden === true || module?.installed === false) continue;
      if (typeof permissions?.canViewModule === 'function' && permissions.canViewModule(id) === false) continue;
      seen.set(id, { id, title: String(module?.title || module?.name || id) });
    }
  }
  return [...seen.values()].sort((a, b) => {
    if (a.id === 'support') return -1;
    if (b.id === 'support') return 1;
    return a.title.localeCompare(b.title);
  });
}

function routeDestinationTitle(id, destinations = []) {
  return destinations.find((item) => item.id === id)?.title
    || defaultMailRouteDestinations().find((item) => item.id === id)?.title
    || id;
}

function mailQueueDefinitions({ threads = [], inboxThreads = threads, sentThreads = [], outboundMessages = [], communicationMessages = [], commands = [], t = (_key, fallback) => fallback } = {}) {
  const allRecords = [
    ...threads.map((record) => ({ ...record, __kind: 'thread' })),
    ...outboundMessages.map((record) => ({ ...record, __kind: 'outbound' })),
  ];
  const latest = Math.max(0, ...allRecords.map(recordTime));
  const routed = allRecords.filter((record) => routeCommandForRecord(record, commands));
  return [
    { id: 'all', title: t('allTraffic', 'Alle Nachrichten'), meta: t('inboundOutbound', 'Posteingang und Gesendet'), count: allRecords.length, updatedAt: latest },
    { id: 'inbound', title: t('inbox', 'Posteingang'), meta: `${inboxThreads.filter((item) => Number(item.unread_count || 0) > 0).length} ${t('unread', 'ungelesen')}`, count: inboxThreads.length, updatedAt: Math.max(0, ...inboxThreads.map(recordTime)) },
    { id: 'outbound', title: t('sent', 'Gesendet'), meta: t('sentMail', 'Versendete Nachrichten'), count: sentThreads.length, updatedAt: Math.max(0, ...sentThreads.map(recordTime)) },
    // v4-allow: label (Ordnerbeschreibung „Freigabe“, fachlich, kein App-Zustand)
    { id: 'approval', title: t('awaiting', 'Freigabe'), meta: t('governedSending', 'Wartet auf Freigabe'), count: outboundMessages.filter((item) => String(item.approval_status || '') === 'awaiting_approval').length, updatedAt: latest },
    { id: 'failed', title: t('failed', 'Fehler'), meta: t('retryRequired', 'Prüfung erforderlich'), count: outboundMessages.filter((item) => String(item.send_status || '').toLowerCase().includes('fail')).length, updatedAt: latest },
    { id: 'routed', title: t('routed', 'Geroutete E-Mails'), meta: t('crossAppHandoffs', 'App-Übergaben'), count: routed.length, updatedAt: Math.max(0, ...routed.map((record) => Number(routeCommandForRecord(record, commands)?.updated_at_ms || 0))) },
  ];
}

function routeSnapshot(record) {
  if (record.__kind === 'thread') {
    return {
      kind: 'inbound_thread',
      id: record.id,
      thread_key: record.thread_key,
      account_key: record.account_key || '',
      subject: record.subject || '',
      unread_count: Number(record.unread_count || 0),
      last_message_at: record.last_message_at || '',
    };
  }
  return {
    kind: 'outbound_message',
    id: record.id,
    campaign_id: record.campaign_id || '',
    recipient_email: record.recipient_email || record.recipient_address_text || '',
    subject: record.subject || '',
    approval_status: record.approval_status || '',
    send_status: record.send_status || '',
  };
}

function safeCommandPart(value) {
  return String(value || '').replace(/[^a-zA-Z0-9_-]+/g, '_').slice(0, 72) || 'record';
}

function buildMailRouteCommands({ batchId, destinationModule, mode = 'handoff', note = '', records = [], actor = {} } = {}) {
  const actorContext = {
    id: String(actor.id || actor.user_id || ''),
    display_name: String(actor.display_name || actor.name || actor.email || ''),
    role: String(actor.role || ''),
  };
  const baseContext = {
    actor: actorContext,
    source_module: 'mail',
    target_module: destinationModule,
    surface: 'mail.bulk.route',
    route_batch_id: batchId,
  };
  const commands = [];
  const genericRecords = [];
  for (const record of records) {
    if (destinationModule === 'support' && record.__kind === 'thread') {
      const sourceId = mailRecordId(record);
      const commandId = `cmd_mail_route_${crypto.randomUUID()}`;
      commands.push({
        id: commandId,
        command_id: commandId,
        module: 'support',
        command_type: 'support.conversation.open_from_thread',
        record_id: sourceId,
        inbound_channel: 'business_os.mail',
        status: 'pending_sync',
        payload: {
          conversation_id: `support_mail_${safeCommandPart(sourceId)}`,
          thread_key: sourceId,
          primary_thread_key: sourceId,
          account_key: record.account_key || '',
          channel: 'email',
          subject: record.subject || '',
          unread_count: Number(record.unread_count || 0),
          route_kind: 'mail.bulk.route',
          route_batch_id: batchId,
          route_mode: mode,
          route_note: note,
          source_module: 'mail',
          target_module: 'support',
          source_record_ids: [sourceId],
          record_snapshot: routeSnapshot(record),
        },
        client_context: baseContext,
      });
    } else {
      genericRecords.push(record);
    }
  }
  for (let index = 0; index < genericRecords.length; index += 100) {
    const chunk = genericRecords.slice(index, index + 100);
    const chunkNumber = Math.floor(index / 100) + 1;
    const commandId = `cmd_mail_route_${crypto.randomUUID()}`;
    const sourceRecordIds = chunk.map(mailRecordId);
    const instruction = note || `Übernimm ${chunk.length} E-Mail-Datensätze aus Mail als ${mode === 'reference' ? 'Referenz' : 'Arbeitsauftrag'}.`;
    // „An CTOX übergeben“: Standardbefehl ctox.chat.task der Chat-App ctox über den Baustein (Strang SKL-D, Kern §16.40
    // commands_out). Die Datensätze gehen als Kontext in den Prompt; route_kind/source_record_ids bleiben oben für den
    // Weiterleitungsnachweis der Mail-App (routeCommandForRecord).
    const routeContext = {
      target_module: destinationModule,
      route_kind: 'mail.bulk.route',
      route_batch_id: batchId,
      route_mode: mode,
      source_record_ids: sourceRecordIds,
      record_snapshot: { schema: 'ctox.mail.route-batch.v1', records: chunk.map(routeSnapshot) },
      risk_class: 'internal',
    };
    const routed = buildContextChatCommand({
      source_app: 'mail',
      record_ref: `${batchId}_${chunkNumber}`,
      text: instruction,
      mode: 'data',
      title: `${chunk.length} E-Mails aus Mail`,
      thread_key: `business-os/mail/routes/${batchId}/${chunkNumber}`,
      context: routeContext,
    });
    commands.push({
      id: commandId,
      command_id: commandId,
      module: destinationModule,
      command_type: routed.type,
      record_id: routed.record_id,
      inbound_channel: 'business_os.mail',
      status: 'pending_sync',
      payload: {
        ...routed.payload,
        target_module: destinationModule,
        route_kind: 'mail.bulk.route',
        route_batch_id: batchId,
        route_mode: mode,
        source_record_ids: sourceRecordIds,
      },
      client_context: baseContext,
    });
  }
  return commands;
}

function listBandCounts(records, commands = [], messages = []) {
  return {
    all: records.length,
    inbound: records.filter((record) => recordMatchesBand(record, 'inbound', messages)).length,
    outbound: records.filter((record) => recordMatchesBand(record, 'outbound', messages)).length,
    attention: records.filter((record) => recordNeedsAttention(record) || routeCommandForRecord(record, commands)?.status === 'failed').length,
  };
}

function recordMatchesBand(record, band, messages, commands = []) {
  if (band === 'inbound') return record.__kind === 'thread'
    && (!messages.some((message) => messageBelongsToThread(message, record))
      || messages.some((message) => messageBelongsToThread(message, record) && messageIsInbox(message)));
  if (band === 'outbound') return record.__kind === 'thread' && messages.some((message) => messageBelongsToThread(message, record)
      && messageIsSent(message));
  if (band === 'attention') return recordNeedsAttention(record) || routeCommandForRecord(record, commands)?.status === 'failed';
  return true;
}

function recordMatchesStatus(record, status, commands = []) {
  if (status === 'unread') return record.__kind === 'thread' && Number(record.unread_count || 0) > 0;
  if (status === 'awaiting') return String(record.approval_status || '').toLowerCase() === 'awaiting_approval';
  if (status === 'routed') return Boolean(routeCommandForRecord(record, commands));
  if (status === 'failed') return String(record.send_status || '').toLowerCase().includes('fail')
    || routeCommandForRecord(record, commands)?.status === 'failed';
  return true;
}

function recordNeedsAttention(record) {
  if (record.__kind === 'thread') return Number(record.unread_count || 0) > 0;
  const approval = String(record.approval_status || '').toLowerCase();
  const send = String(record.send_status || '').toLowerCase();
  return approval === 'awaiting_approval' || send.includes('fail');
}

function recordSubject(record) {
  return String(record.subject || record.recipient_email || record.thread_key || '').toLowerCase();
}

function recordTime(record) {
  return timeOf(record.last_message_at || record.updated_at_ms || record.updated_at || record.created_at_ms || record.external_created_at);
}

function parseRecipientAddresses(value) {
  const matches = String(value || '').match(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/gi) || [];
  return [...new Set(matches.map((address) => address.toLowerCase()).filter(isEmailAddress))];
}

function extractImportRecipients(payload = {}) {
  const chunks = [payload.source?.text || ''];
  for (const file of payload.source?.files || []) {
    if (file.text) chunks.push(file.text);
    else if (file.base64) {
      try {
        const binary = atob(file.base64);
        chunks.push(new TextDecoder().decode(Uint8Array.from(binary, (character) => character.charCodeAt(0))));
      } catch {}
    }
  }
  return parseRecipientAddresses(chunks.join('\n'));
}

// `host` ist der App-Host des Moduls: mail hat keinen Modul-State, deshalb
// wird der Host vom Aufrufer durchgereicht statt global gesucht.
function downloadJson(payload, filename, host) {
  let url = '';
  try {
    url = URL.createObjectURL(new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = filename;
    link.rel = 'noopener';
    link.style.display = 'none';
    (host || document.documentElement).append(link);
    link.click();
    link.remove();
  } finally {
    if (url) window.setTimeout(() => URL.revokeObjectURL(url), 4000);
  }
}

function applyStaticLabels(root, t) {
  const labels = {
    '[data-mail-kicker]': t('workspace', 'Arbeitsbereich'),
    '[data-mail-title]': t('mail', 'Mail'),
    '[data-mail-composer-title]': t('compose', 'Neue Mail'),
    '[data-mail-compose-primary]': t('compose', 'Neue Mail'),
    '[data-mail-from-label]': t('from', 'Von'),
    '[data-mail-to-label]': t('to', 'An'),
    '[data-mail-group-label]': t('group', 'Gruppe / Kampagne'),
    '[data-mail-subject-label]': t('subject', 'Betreff'),
    '[data-mail-message-label]': t('message', 'Nachricht'),
    '[data-mail-save-draft]': t('saveDraft', 'Als Entwurf speichern'),
    '[data-mail-request-approval]': t('requestApproval', 'Zur Freigabe'),
    '[data-mail-create-group]': t('createGroup', 'Gruppe erstellen'),
    '[data-mail-mailbox-admin-kicker]': t('administration', 'Administration'),
    '[data-mail-mailbox-admin-title]': t('mailSettings', 'Mail-Einstellungen'),
    '[data-mail-domains-label]': t('mailserverDomains', 'Mailserver-Domains'),
    '[data-mail-new-mailbox-label]': t('createMailbox', 'Postfach anlegen'),
    '[data-mail-address-label]': t('emailAddress', 'E-Mail-Adresse'),
    '[data-mail-owner-label]': t('ownerUser', 'Nutzer'),
    '[data-mail-password-label]': t('initialPassword', 'Initialpasswort'),
    '[data-mail-create-mailbox]': t('createMailbox', 'Postfach anlegen'),
    '[data-mail-existing-mailboxes-label]': t('existingMailboxes', 'Vorhandene Postfächer'),
    '[data-mail-reload-mailboxes]': t('reload', 'Neu laden'),
  };
  for (const [selector, text] of Object.entries(labels)) {
    const element = root.querySelector(selector);
    if (element) element.textContent = text;
  }
  // Sachinformation zum Passwortfeld nur als Tooltip (Textdiät Regel 1).
  const password = root.querySelector('[data-mail-mailbox-security]');
  if (password) password.title = t('mailboxSecurity', 'Das Passwort wird nach der Einrichtung aus dem Auftrag entfernt.');
  const search = root.querySelector('[data-mail-search]');
  if (search) search.placeholder = t('search', 'Mails durchsuchen');
}

function visibleEmailAccounts(accounts, user) {
  const emailAccounts = (accounts || []).filter((account) => account.channel === 'email' && !isDeleted(account));
  if (isGlobalMailAdmin(user)) return emailAccounts.sort(sortAccount);
  return emailAccounts.filter((account) => {
    const profile = accountProfile(account);
    const owner = String(profile.owner_user_id || profile.ownerUserId || '').trim();
    const shared = arrayStrings(profile.shared_user_ids || profile.sharedUserIds || profile.member_user_ids);
    const userId = String(user?.id || user?.user_id || '').trim();
    if (!userId) return false;
    return owner === userId || shared.includes(userId);
  }).sort(sortAccount);
}

function visibleMailCampaigns(campaigns, user, accounts, messages) {
  const active = (campaigns || []).filter((campaign) => !isDeleted(campaign));
  if (isGlobalMailAdmin(user)) return active.sort(sortUpdatedDesc);
  const userIds = new Set([user?.id, user?.user_id, user?.email, user?.login].filter(Boolean).map(String));
  const accountKeys = new Set((accounts || []).map((account) => account.id));
  const campaignIdsForAccounts = new Set((messages || [])
    .filter((message) => accountKeys.has(message.communication_account_key || message.sender_account_id))
    .map((message) => message.campaign_id));
  return active.filter((campaign) => {
    const owner = String(campaign.owner_id || '').trim();
    const shared = arrayStrings(campaign.payload?.shared_user_ids || campaign.payload?.member_user_ids);
    if (owner && userIds.has(owner)) return true;
    if (shared.some((id) => userIds.has(id))) return true;
    return campaignIdsForAccounts.has(campaign.id);
  }).sort(sortUpdatedDesc);
}

function accountOwnedByCurrentUser(account, user) {
  const owner = String(accountProfile(account).owner_user_id || accountProfile(account).ownerUserId || '').trim();
  const userId = String(user?.id || user?.user_id || '').trim();
  return Boolean(owner && userId && owner === userId);
}

function isGlobalMailAdmin(user) {
  return ['admin', 'chef', 'owner'].includes(String(user?.role || '').toLowerCase());
}

// mail_server_state (Plattformmanifest ctox-platform, Kern §16.28): {id, kind:'runtime'|'domain'|'user', name, status,
// data, updated_at_ms}. runtime: status = Laufzustand, data.{config, health}; domain: name = Domain, data = DNS-Werte
// (dkim_selector, dkim_public_key, spf_record, dmarc_record); user: name = Postfachadresse, data.{owner_user_id,
// communication_account_key}. Abbildung auf die Felder, die die Verwaltung schon rendert (vorher get_config-Ergebnis).
function mailserverStateFromRows(rows) {
  const live = (rows || []).filter((row) => row && !isDeleted(row));
  const runtime = live.find((row) => row.kind === 'runtime') || null;
  const data = (row) => (row?.data && typeof row.data === 'object' ? row.data : {});
  const byName = (a, b) => String(a.name || a.id || '').localeCompare(String(b.name || b.id || ''));
  return {
    runtimeConfig: runtime ? { ...(data(runtime).config || {}) } : {},
    runtimeStatus: runtime ? { ...(data(runtime).status || {}), ...(runtime.status ? { state: runtime.status } : {}) } : {},
    health: runtime ? { ...(data(runtime).health || {}) } : {},
    domains: live.filter((row) => row.kind === 'domain').sort(byName)
      .map((row) => ({ ...data(row), domain_name: String(row.name || data(row).domain_name || '') })),
    users: live.filter((row) => row.kind === 'user').sort(byName)
      .map((row) => ({ ...data(row), username: String(row.name || data(row).username || '') })),
  };
}

function mailserverDomain(domain) {
  if (typeof domain === 'string') return domain.trim();
  return String(domain?.domain_name || domain?.domain || domain?.name || '').trim();
}

function mailserverUsername(user) {
  if (typeof user === 'string') return user.trim();
  return String(user?.username || user?.email || user?.address || '').trim();
}

function isEmailAddress(value) {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(String(value || '').trim());
}

function accountProfile(account) {
  const profile = account?.profile_json;
  if (!profile) return {};
  if (typeof profile === 'object') return profile;
  try { return JSON.parse(profile); } catch { return {}; }
}

function filterThreadsForFolder(threads, folder, messages) {
  return (threads || []).filter((thread) => {
    const threadMessages = (messages || []).filter((message) => messageBelongsToThread(message, thread));
    if (folder === 'unread') return Number(thread.unread_count || 0) > 0;
    if (folder === 'sent') return threadMessages.some(messageIsSent);
    return threadMessages.length === 0
      || threadMessages.some(messageIsInbox);
  }).sort((a, b) => timeOf(b.last_message_at) - timeOf(a.last_message_at));
}

function messageIsSent(message) {
  const folder = String(message.folder_hint || '').toLowerCase();
  return ['sent', 'sentitems'].includes(folder)
    || (!['inbox', 'incoming'].includes(folder) && message.direction === 'outbound');
}

function messageIsInbox(message) {
  const folder = String(message.folder_hint || '').toLowerCase();
  if (['sent', 'sentitems'].includes(folder)) return false;
  return ['inbox', 'incoming'].includes(folder) || message.direction === 'inbound';
}

function folderCounts(threads, outboundMessages, communicationMessages) {
  return {
    inbox: filterThreadsForFolder(threads, 'inbox', communicationMessages).length,
    unread: filterThreadsForFolder(threads, 'unread', communicationMessages).length,
    sent: filterThreadsForFolder(threads, 'sent', communicationMessages).length,
    drafts: (outboundMessages || []).filter(isDraftMessage).length,
  };
}

function campaignStats(campaignId, messages) {
  const rows = (messages || []).filter((message) => message.campaign_id === campaignId);
  const progress = deriveMailGroupProgress(campaignId, messages);
  return {
    ...progress,
    total: rows.length,
    drafts: rows.filter(isDraftMessage).length,
    awaiting: rows.filter((message) => message.approval_status === 'awaiting_approval').length,
    sent: rows.filter((message) => ['sent', 'accepted'].includes(message.send_status)).length,
    queued: rows.filter((message) => ['queued_for_provider', 'queued'].includes(String(message.send_status || ''))).length,
    delivered: rows.filter((message) => messageDeliveryStatus(message) === 'delivered').length,
    opened: rows.filter((message) => Number(message.payload?.open_count || 0) > 0).length,
    clicked: rows.filter((message) => Number(message.payload?.click_count || 0) > 0).length,
    failed: rows.filter((message) => String(message.send_status || '').includes('fail')).length,
  };
}

function groupKindLabel(kind) {
  return ({
    'outbound-sales': 'Outbound Sales',
    newsletter: 'Newsletter',
    support: 'Support-Topf',
    routing: 'Routing-Topf',
    single: 'Einzel-E-Mail',
    free: 'Freie E-Mail-Gruppe',
  })[kind] || 'E-Mail-Gruppe';
}

function groupContentLabel(mode) {
  return ({
    word: 'Rich Text · Word',
    email_visual: 'HTML-E-Mail bearbeiten',
    email_html: 'HTML bearbeiten',
  })[mode] || 'Inhalt bearbeiten';
}

// Payload des Sammelbefehls outbound.message.compose (modules/outbound/handlers/outbound.message.compose.js baut daraus
// je Empfänger exakt die früheren Einzel-Payloads: engagement.create, message.prepare, request_approval, approve,
// send_approved).
function buildComposePayload(input, recipients, now) {
  return {
    mode: input.mode || 'draft',
    campaign_id: input.campaignId,
    sender_account_id: input.accountKey,
    subject: input.subject,
    body_text: input.body,
    body_html: plainTextToEmailHtml(input.body),
    content_revision_id: String(input.contentRevisionId || ''),
    created_at_ms: now,
    recipients,
  };
}

function plainTextToEmailHtml(text) {
  return `<div style="font-family:Arial,sans-serif;font-size:16px;line-height:1.55;color:#17202a">${escapeHtml(String(text || '')).replace(/\n/g, '<br>')}</div>`;
}

function validateComposeInput(input) {
  if (!input.accountKey) return 'missingSender';
  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(input.recipient || '')) return 'missingRecipient';
  if (!String(input.subject || '').trim() && !String(input.body || '').trim()) return 'missingContent';
  return '';
}

function buildSeriesEmailTransferHash({ campaignId = '', recipients = [], subject = '', body = '', sourceModule = 'sellify' } = {}) {
  const params = new URLSearchParams({ action: 'series-email', source_module: sourceModule });
  if (campaignId) params.set('campaign_id', campaignId);
  if (recipients.length) params.set('recipients', recipients.join(','));
  if (subject) params.set('subject', subject);
  if (body) params.set('body', body);
  return `mail?${params.toString()}`;
}

function messageActions(message, t) {
  const approval = String(message.approval_status || 'draft');
  const send = String(message.send_status || 'not_scheduled');
  if (approval === 'awaiting_approval') return [{ id: 'approve', label: t('approve', 'Freigeben'), primary: true }];
  if (approval === 'approved' && ['approved_not_sent', 'not_scheduled'].includes(send)) return [{ id: 'send', label: t('send', 'Senden'), primary: true }];
  if (isDraftMessage(message)) return [{ id: 'request-approval', label: t('requestApproval', 'Zur Freigabe'), primary: true }];
  return [];
}

function isDraftMessage(message) {
  return DRAFT_SEND_STATUSES.has(String(message?.send_status || '').toLowerCase())
    && !['sent', 'accepted', 'queued_for_provider'].includes(String(message?.send_status || '').toLowerCase());
}

function canRequestApproval(message) {
  const approval = String(message?.approval_status || 'draft').toLowerCase();
  return !['approved', 'awaiting_approval', 'rejected'].includes(approval) && isDraftMessage(message);
}

function messageStatusLabel(message, t) {
  const send = String(message.send_status || '').toLowerCase();
  const approval = String(message.approval_status || '').toLowerCase();
  if (Number(message.payload?.click_count || 0) > 0) return 'Link geklickt';
  if (Number(message.payload?.open_count || 0) > 0) return 'Öffnung erfasst';
  if (messageDeliveryStatus(message) === 'delivered') return 'Zugestellt';
  if (send === 'queued_for_provider') return t('queued', 'In Mailserver-Queue');
  if (approval === 'awaiting_approval') return t('awaiting', 'Freigabe');
  if (approval === 'approved') return t('approved', 'Freigegeben');
  if (send.includes('fail')) return t('failed', 'Fehler');
  return t('drafts', 'Entwurf');
}

function messageDeliveryStatus(message) {
  const provider = String(message?.payload?.provider_dispatch_status || '').toLowerCase();
  const send = String(message?.send_status || '').toLowerCase();
  if (provider === 'delivered' || ['sent', 'accepted', 'delivered'].includes(send)) return 'delivered';
  if (provider.includes('fail') || send.includes('fail') || send === 'bounced') return 'failed';
  if (['queued_for_provider', 'queued'].includes(send)) return 'queued';
  return 'pending';
}

function messageProgressModel(message) {
  const approval = String(message?.approval_status || 'draft').toLowerCase();
  const send = String(message?.send_status || '').toLowerCase();
  const delivery = messageDeliveryStatus(message);
  const opened = Number(message?.payload?.open_count || 0) > 0;
  const clicked = Number(message?.payload?.click_count || 0) > 0;
  const trackingEnabled = message?.payload?.tracking_enabled === true;
  const queued = ['queued_for_provider', 'queued'].includes(send) || ['delivered', 'failed'].includes(delivery);
  const approved = approval === 'approved' || queued;

  let currentIndex = 0;
  let currentState = 'active';
  if (clicked) {
    currentIndex = 5;
    currentState = 'done';
  } else if (opened) {
    currentIndex = 4;
    currentState = 'done';
  } else if (delivery === 'delivered') {
    currentIndex = 3;
    currentState = 'done';
  } else if (delivery === 'failed') {
    currentIndex = 3;
    currentState = 'failed';
  } else if (queued) {
    currentIndex = 2;
  } else if (approved) {
    currentIndex = 2;
  } else if (approval === 'awaiting_approval') {
    currentIndex = 1;
  }

  // Versandschritte je Nachricht = fachlicher Datensatzstatus (outbound_messages), kein App-Zustand.
  const labels = [
    ['Entwurf erstellt', 'edit'],
    [approved ? 'Freigegeben' : 'Freigabe ausstehend', approved ? 'check' : 'clock'], // v4-allow: label
    [queued ? 'An Mailserver übergeben' : 'Versand ausstehend', 'send'], // v4-allow: label
    [delivery === 'failed' ? 'Zustellung fehlgeschlagen' : delivery === 'delivered' ? 'Zugestellt' : 'Zustellung ausstehend', delivery === 'failed' ? 'warning' : 'check'], // v4-allow: label
    [opened ? 'Öffnung erfasst' : trackingEnabled ? 'Keine Öffnung erfasst' : 'Öffnungstracking nicht aktiv', 'eye'],
    [clicked ? 'Link geklickt' : trackingEnabled ? 'Kein Linkklick erfasst' : 'Klicktracking nicht aktiv', 'link'],
  ];
  const steps = labels.map(([label, icon], index) => ({
    label,
    icon,
    state: index < currentIndex ? 'done' : index === currentIndex ? currentState : 'pending',
  }));
  const percent = Math.round((currentIndex / (steps.length - 1)) * 100);
  const currentLabel = labels[currentIndex]?.[0] || labels[0][0];
  return {
    steps,
    percent,
    currentLabel,
    ariaLabel: `Versandfortschritt: ${currentLabel}. ${percent} Prozent.`,
  };
}

function messageEventTimeline(message) {
  const payload = message?.payload || {};
  const events = [{ label: 'Vorbereitet', detail: formatRecordTime(message.created_at_ms), state: 'done' }];
  const approval = String(message.approval_status || 'draft');
  events.push({
    label: approval === 'approved' ? 'Freigegeben' : approval === 'awaiting_approval' ? 'Freigabe ausstehend' : 'Noch nicht freigegeben', // v4-allow: label
    detail: formatRecordTime(message.updated_at_ms || message.created_at_ms),
    state: approval === 'approved' ? 'done' : 'pending',
  });
  if (payload.provider_queued_at_ms) {
    events.push({ label: 'An Mailserver übergeben', detail: formatRecordTime(payload.provider_queued_at_ms), state: 'done' });
  }
  const delivery = messageDeliveryStatus(message);
  if (delivery === 'delivered') {
    events.push({ label: 'Zugestellt', detail: formatRecordTime(payload.delivered_at_ms || payload.provider_completed_at_ms || message.sent_at_ms), state: 'done' });
  } else if (delivery === 'failed') {
    events.push({ label: 'Zustellung fehlgeschlagen', detail: payload.provider_error_text || payload.last_send_error || 'Der Mailserver hat einen permanenten Fehler gemeldet.', state: 'failed' });
  } else if (delivery === 'queued') {
    events.push({ label: 'Zustellung läuft', detail: 'Wartet auf das terminale SMTP-Ergebnis', state: 'active' }); // v4-allow: label
  }
  if (Number(payload.open_count || 0) > 0) {
    events.push({ label: `Öffnung erfasst · ${payload.open_count}×`, detail: formatRecordTime(payload.last_opened_at_ms), state: 'done' });
  } else if (payload.tracking_enabled) {
    events.push({ label: 'Noch keine Öffnung erfasst', detail: 'Tracking ist aktiv; Datenschutz-Proxys können das Ergebnis beeinflussen.', state: 'pending' });
  }
  if (Number(payload.click_count || 0) > 0) {
    events.push({ label: `Link geklickt · ${payload.click_count}×`, detail: formatRecordTime(payload.last_clicked_at_ms), state: 'done' });
  }
  return events;
}

function messageBelongsToThread(message, thread) {
  return message.thread_key === thread.id && message.account_key === thread.account_key;
}

function latestMessageForThread(thread, messages) {
  return (messages || [])
    .filter((message) => messageBelongsToThread(message, thread))
    .sort((a, b) => timeOf(b.external_created_at) - timeOf(a.external_created_at))[0] || null;
}

function renderMetric(value, label) {
  return `<div class="mail-metric"><strong>${Number(value || 0)}</strong><span>${escapeHtml(label)}</span></div>`;
}

function isPermissionDenied(error) {
  return error?.code === 'forbidden'
    || error?.code === 'CTOX_BUSINESS_OS_PERMISSION_DENIED'
    || error?.name === 'BusinessOsPermissionError';
}

function option(value, label) {
  const element = document.createElement('option');
  element.value = value;
  element.textContent = label;
  return element;
}

function splitAddresses(value) {
  return String(value || '').split(/[;,]/).map((item) => item.trim()).filter(Boolean);
}

function arrayStrings(value) {
  if (Array.isArray(value)) return value.map(String).filter(Boolean);
  if (typeof value === 'string') return splitAddresses(value);
  return [];
}

function isDeleted(value) {
  return value?._deleted === true || value?.is_deleted === true;
}

function sortUpdatedDesc(a, b) {
  return Number(b.updated_at_ms || b.created_at_ms || 0) - Number(a.updated_at_ms || a.created_at_ms || 0);
}

function sortAccount(a, b) {
  return String(a.address || a.id || '').localeCompare(String(b.address || b.id || ''));
}

function timeOf(value) {
  if (typeof value === 'number') return value;
  const parsed = Date.parse(String(value || ''));
  return Number.isFinite(parsed) ? parsed : 0;
}

function formatRecordTime(value) {
  const ms = timeOf(value);
  if (!ms) return '';
  const date = new Date(ms);
  const now = new Date();
  if (date.toDateString() === now.toDateString()) {
    return new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' }).format(date);
  }
  return new Intl.DateTimeFormat(undefined, { day: '2-digit', month: '2-digit', year: date.getFullYear() === now.getFullYear() ? undefined : '2-digit' }).format(date);
}

function replySubject(subject) {
  const clean = String(subject || '').trim();
  if (!clean) return 'Re:';
  return /^re:/i.test(clean) ? clean : `Re: ${clean}`;
}

function compact(value) {
  return String(value || '').replace(/\s+/g, ' ').trim();
}

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, Number(value) || min));
}

function stableJson(value) {
  const normalize = (item) => {
    if (Array.isArray(item)) return item.map(normalize);
    if (!item || typeof item !== 'object') return item;
    return Object.fromEntries(Object.keys(item).sort().map((key) => [key, normalize(item[key])]));
  };
  return JSON.stringify(normalize(value));
}

async function sha256Text(value) {
  const bytes = new TextEncoder().encode(String(value ?? ''));
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  })[char]);
}

function escapeAttribute(value) {
  return escapeHtml(value).replace(/`/g, '&#96;');
}

export const __mailTestHooks = {
  visibleEmailAccounts,
  mailCountsKnown,
  visibleMailCampaigns,
  messageBelongsToThread,
  latestMessageForThread,
  filterThreadsForFolder,
  folderCounts,
  campaignStats,
  messageDeliveryStatus,
  messageProgressModel,
  messageEventTimeline,
  plainTextToEmailHtml,
  buildComposePayload,
  mailserverStateFromRows,
  mailActionIcon,
  validateComposeInput,
  messageActions,
  isDraftMessage,
  replySubject,
  isGlobalMailAdmin,
  mailserverDomain,
  mailserverUsername,
  isEmailAddress,
  parseRecipientAddresses,
  buildSeriesEmailTransferHash,
  listBandCounts,
  mailRecordKey,
  mailQueueDefinitions,
  routeCommandForRecord,
  routeDestinationsFromCatalog,
  buildMailRouteCommands,
  stableJson,
  sha256Text,
};
