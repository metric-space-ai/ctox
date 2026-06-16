import { CtoxResizer } from '../../shared/resizer.js';
import { showBusinessConfirm } from '../../shared/dialogs.js';

const PRESETS = {
  'standard-mgmt': {
    id: 'lagerverwaltung',
    title: 'Lagerverwaltung',
    desc: 'Echtzeit-Lagerverwaltung und Bestandsüberwachung mit synchronisierten Artikeltabellen.',
    category: 'Management',
    layout: 'full-workspace',
    collections: ['inventory_records', 'inventory_transactions'],
    prompt: 'Erstelle eine Echtzeit-Lagerverwaltung für mein Unternehmen. Ich möchte Artikel mit Bildern, Beschreibungen, Barcodes und Beständen auflisten. Es soll Buchungen für Wareneingang und Warenausgang geben, um den Bestand automatisch zu aktualisieren.'
  },
  'notes-style': {
    id: 'notizen',
    title: 'Notizen',
    desc: 'Modernes, lokales Markdown-Notizmodul für schnelle Aufzeichnungen und Entwürfe.',
    category: 'Productivity',
    layout: 'full-workspace',
    collections: ['notes_records'],
    prompt: 'Erstelle ein digitales Notizbuch für mein Team. Man soll Notizen erstellen, editieren und löschen können. Die Notizen sollen Tags haben und einen integrierten Markdown-Editor für schöne Formatierungen bieten.'
  },
  'kanban-style': {
    id: 'taskboard',
    title: 'Taskboard',
    desc: 'Kompaktes Aufgaben-Board mit verschiebbaren Tickets, Bearbeitungsstatus und Zuweisungen.',
    category: 'Productivity',
    layout: 'pane',
    collections: ['board_tasks'],
    prompt: 'Erstelle ein agiles Kanban-Taskboard. Es soll drei Spalten geben: \'Zu tun\', \'In Arbeit\' und \'Erledigt\'. Aufgaben sollen Titel, Prioritäten (hoch, mittel, niedrig) und Zuweisungen zu Teammitgliedern enthalten und sich verschieben lassen.'
  },
  'support-style': {
    id: 'supportdesk',
    title: 'Support Desk',
    desc: 'Zentrales Helpdesk-System zur Bearbeitung von Kundenanfragen, Störungstickets und Feedback.',
    category: 'Management',
    layout: 'pane',
    collections: ['tickets', 'ticket_comments'],
    prompt: 'Erstelle ein professionelles Support-Ticket-System. Kunden sollen Tickets mit einer Beschreibung und Priorität erstellen können. Support-Mitarbeiter sollen Kommentare hinterlassen und den Status des Tickets auf Gelöst ändern.'
  },
  'time-style': {
    id: 'zeiterfassung',
    title: 'Zeiterfassung',
    desc: 'Einfaches Tool zur Erfassung von Arbeitszeiten, Projektbudgets und Stundennachweisen.',
    category: 'Productivity',
    layout: 'full-workspace',
    collections: ['time_logs', 'projects'],
    prompt: 'Erstelle eine Zeiterfassungs-App für Dienstleister. Mitarbeiter sollen Zeiteinträge für Projekte erfassen, Start- und Endzeiten eintragen und eine Auswertung der geleisteten Stunden pro Projekt und Monat anzeigen.'
  },
  'plant-style': {
    id: 'pflanzen-tracker',
    title: 'Pflanzen-Tracker',
    desc: 'Übersicht über Büropflanzen, deren Standorte und automatische Gieß-Erinnerungen.',
    category: 'Utilities',
    layout: 'pane',
    collections: ['plants', 'watering_logs'],
    prompt: 'Erstelle einen Pflanzen-Tracker für unsere Büropflanzen. Jede Pflanze hat einen Namen, einen Standort (z. B. Konferenzraum) und ein Gießintervall. Die App soll anzeigen, wann das nächste Gießen fällig ist und Gieß-Logs speichern.'
  }
};

const state = {
  ctx: null,
  t: (key, fallback) => fallback ?? key,
  appId: 'lagerverwaltung',
  appTitle: 'Lagerverwaltung',
  appDesc: 'Echtzeit-Lagerverwaltung und Bestandsüberwachung mit synchronisierten Artikeltabellen.',
  appCategory: 'Management',
  appLayout: 'full-workspace',
  appCollections: ['inventory_records', 'inventory_transactions'],
  appVersion: 'v1',
  specPrompt: '',
  generatedFiles: {},
  contextMenu: null,
  contextMenuCleanup: null,
  resizerCleanup: null,
  catalogSubscription: null,
  commandSubscription: null,
  installedApps: [],
  creatorPrompts: [],
  isOptimizing: false,
  isDeploying: false
};

export function normalizeModuleId(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9-]/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '');
}

export function normalizeCollectionName(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_]/g, '_')
    .replace(/_+/g, '_')
    .replace(/^_|_$/g, '');
}

export function deriveSpecFromPrompt(prompt) {
  const cleanPrompt = String(prompt || '').trim();
  const lowerPrompt = cleanPrompt.toLowerCase();
  let guessedTitle = 'Spezialanwendung';
  let guessedId = 'spezialapp';
  let guessedDesc = cleanPrompt;
  let guessedCategory = 'Management';
  let guessedLayout = 'full-workspace';
  let guessedCollections = ['records'];

  if (lowerPrompt.includes('pflanze') || lowerPrompt.includes('blume') || lowerPrompt.includes('garten') || lowerPrompt.includes('botanik')) {
    guessedTitle = 'Pflanzen-Tracker';
    guessedId = 'pflanzen-tracker';
    guessedDesc = 'Übersicht über Büropflanzen, deren Standorte und Gieß-Erinnerungen.';
    guessedCategory = 'Utilities';
    guessedLayout = 'pane';
    guessedCollections = ['plants', 'watering_logs'];
  } else if (lowerPrompt.includes('auto') || lowerPrompt.includes('fahrzeug') || lowerPrompt.includes('fleet') || lowerPrompt.includes('fuhrpark') || lowerPrompt.includes('kfz')) {
    guessedTitle = 'Fuhrparkverwaltung';
    guessedId = 'fuhrpark';
    guessedDesc = 'Fahrzeuge, Kilometerstände, TÜV-Termine und Wartungsprotokolle im Überblick.';
    guessedCategory = 'Management';
    guessedLayout = 'full-workspace';
    guessedCollections = ['vehicles', 'maintenance_logs', 'refuels'];
  } else if (lowerPrompt.includes('kunde') || lowerPrompt.includes('crm') || lowerPrompt.includes('sales') || lowerPrompt.includes('kontakt')) {
    guessedTitle = 'Kundenverwaltung (CRM)';
    guessedId = 'crm-kontakte';
    guessedDesc = 'Zentrales CRM zur Verwaltung von Leads, Kontakten und Interaktionsberichten.';
    guessedCategory = 'Finance';
    guessedLayout = 'full-workspace';
    guessedCollections = ['customers', 'interactions'];
  } else if (lowerPrompt.includes('ticket') || lowerPrompt.includes('support') || lowerPrompt.includes('helpdesk') || lowerPrompt.includes('fehler')) {
    guessedTitle = 'Support Desk';
    guessedId = 'supportdesk';
    guessedDesc = 'Helpdesk-System zur Bearbeitung von Kundenanfragen, Störungstickets und Fehlermeldungen.';
    guessedCategory = 'Management';
    guessedLayout = 'pane';
    guessedCollections = ['tickets', 'ticket_comments'];
  } else if (lowerPrompt.includes('zeit') || lowerPrompt.includes('stunde') || lowerPrompt.includes('timer') || lowerPrompt.includes('time')) {
    guessedTitle = 'Zeiterfassung';
    guessedId = 'zeiterfassung';
    guessedDesc = 'Tool zur schnellen Erfassung von Arbeitszeiten und Projektstunden.';
    guessedCategory = 'Productivity';
    guessedLayout = 'full-workspace';
    guessedCollections = ['time_logs', 'projects'];
  } else if (lowerPrompt.includes('möbel') || lowerPrompt.includes('inventar') || lowerPrompt.includes('office') || lowerPrompt.includes('anlage') || lowerPrompt.includes('lager')) {
    guessedTitle = 'Inventarverwaltung';
    guessedId = 'inventar';
    guessedDesc = 'Schnelles Tracken von Büroausstattung, Mobiliar und IT-Hardware.';
    guessedCategory = 'Management';
    guessedLayout = 'full-workspace';
    guessedCollections = ['inventory_items', 'audits'];
  } else {
    const cleanPromptStr = cleanPrompt.replace(/[^a-zA-Z0-9\s]/g, '').trim();
    const words = cleanPromptStr.split(/\s+/).filter(w => w.length > 3);
    if (words.length > 0) {
      guessedTitle = words[0].charAt(0).toUpperCase() + words[0].slice(1);
      if (words[1]) guessedTitle += ` ${words[1].charAt(0).toUpperCase()}${words[1].slice(1)}`;
      guessedId = normalizeModuleId(guessedTitle) || guessedId;
    }
    guessedDesc = cleanPrompt.length > 100 ? `${cleanPrompt.substring(0, 97)}...` : cleanPrompt;
    guessedCollections = [`${guessedId}_records`, `${guessedId}_history`];
  }

  return {
    id: normalizeModuleId(guessedId),
    title: guessedTitle,
    desc: guessedDesc,
    category: guessedCategory,
    layout: guessedLayout,
    collections: guessedCollections.map(normalizeCollectionName).filter(Boolean)
  };
}

export function validateCreatorSpec({ appId, appTitle, appDesc, appCollections }) {
  const errors = [];
  if (!normalizeModuleId(appId)) errors.push('Modul-ID fehlt oder ist ungültig.');
  if (!String(appTitle || '').trim()) errors.push('Titel fehlt.');
  if (!String(appDesc || '').trim()) errors.push('Beschreibung fehlt.');
  const collections = Array.isArray(appCollections) ? appCollections.map(normalizeCollectionName).filter(Boolean) : [];
  if (collections.length === 0) errors.push('Mindestens eine Datentabelle ist erforderlich.');
  return errors;
}

export function computeCreatorActionState({ prompt, specPrompt, appId, appTitle, appDesc, appCollections, isOptimizing = false, isDeploying = false }) {
  const promptText = String(prompt || '').trim();
  const specText = String(specPrompt || '').trim();
  const validationErrors = validateCreatorSpec({ appId, appTitle, appDesc, appCollections });
  const hasPrompt = Boolean(promptText);
  const hasFreshSpec = hasPrompt && specText === promptText;
  const isBusy = Boolean(isOptimizing || isDeploying);
  const optimizeReady = hasPrompt && !isBusy;
  const deployReady = hasFreshSpec && validationErrors.length === 0 && !isBusy;
  let diagnostic = 'Prompt fehlt. Beschreibe zuerst die App.';
  if (isOptimizing) diagnostic = 'Spezifikation wird aktualisiert.';
  else if (isDeploying) diagnostic = 'Installation läuft.';
  else if (hasPrompt && !hasFreshSpec) diagnostic = 'Spezifikation ist nicht aktuell. Bitte zuerst optimieren.';
  else if (hasFreshSpec && validationErrors.length > 0) diagnostic = validationErrors[0];
  else if (deployReady) diagnostic = 'Spezifikation ist aktuell und installierbar.';

  return { hasPrompt, hasFreshSpec, validationErrors, optimizeReady, deployReady, diagnostic };
}

export function normalizeCreatorInstalledApps(catalog) {
  const modules = Array.isArray(catalog?.modules) ? catalog.modules : [];
  return modules
    .filter((mod) => {
      const entry = String(mod?.entry || '');
      const source = String(mod?.source || mod?.store?.distribution || '').toLowerCase();
      return mod?.id
        && mod.id !== 'creator'
        && (
          entry.startsWith('installed-modules/')
          || source === 'installed'
          || source.includes('installed-module')
          || mod?.generated_by === 'creator'
        );
    })
    .map((mod) => ({
      id: normalizeModuleId(mod.id),
      title: String(mod.title || mod.id),
      description: String(mod.description || mod.store?.summary || ''),
      category: String(mod.category || mod.source || 'Custom'),
      version: String(mod.version || 'v1'),
      entry: String(mod.entry || ''),
    }))
    .filter((mod) => mod.id)
    .sort((a, b) => a.title.localeCompare(b.title, 'de'));
}

export function normalizeCreatorPromptSuggestions(commands, limit = 5) {
  const items = Array.isArray(commands) ? commands : [];
  return items
    .filter((command) => {
      const payload = command?.payload || {};
      const type = String(command?.command_type || command?.type || '');
      const module = String(command?.module || payload.module || '');
      const source = String(command?.client_context?.source || payload.source || '');
      return module === 'creator'
        || source === 'business-os-creator'
        || type === 'ctox.business_os.app.modify'
        || type === 'business_os.chat.task';
    })
    .map((command) => {
      const payload = command?.payload || {};
      const context = payload.context || {};
      const prompt = String(payload.prompt || payload.instruction || payload.user_message || command?.title || '').trim();
      return {
        id: String(command?.id || command?.command_id || `${Date.now()}-${prompt}`),
        title: String(payload.title || context.app_title || command?.title || 'CTOX Prompt'),
        prompt,
        status: String(command?.status || 'pending'),
        updated_at_ms: Number(command?.updated_at_ms || command?.created_at_ms || 0),
      };
    })
    .filter((item) => item.prompt)
    .sort((a, b) => b.updated_at_ms - a.updated_at_ms)
    .slice(0, limit);
}

export async function mount(ctx) {
  state.ctx = ctx;

  // 1. Inject module scoped stylesheet dynamically
  await ensureStyles();

  // 1b. Load locale messages (German markup text is the fallback)
  const messages = await loadCreatorMessages(ctx.locale);
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;

  // 2. Fetch and render raw index.html structure
  const html = await fetch(new URL('./index.html', import.meta.url)).then(res => res.text());
  ctx.host.innerHTML = html;
  applyCreatorTranslations(ctx.host, state.t);

  // 3. Wire UI events & presets loading
  wireUi(ctx.host);

  // 4. Generate starting files
  generateAllFiles();

  // 5. Load catalog-backed right rail data
  await startCreatorDataStreams(ctx, ctx.host);

  // 5. Initialize CTOX unified context menu
  state.contextMenuCleanup = initCreatorContextMenu(state);

  // 6. Setup column resizer
  state.resizerCleanup = setupResizers(ctx.host);

  return () => {
    state.contextMenuCleanup?.();
    state.resizerCleanup?.();
    cleanupSubscription(state.catalogSubscription);
    cleanupSubscription(state.commandSubscription);
    state.catalogSubscription = null;
    state.commandSubscription = null;
    state.contextMenu?.remove();
    state.contextMenu = null;
    console.log('[creator] Module unmounted and cleaned up.');
  };
}

async function ensureStyles() {
  if (document.querySelector('link[data-module-styles="creator"]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./index.css', import.meta.url).href;
  link.dataset.moduleStyles = 'creator';
  document.head.append(link);
}

function setupResizers(host) {
  // Column resizing is now owned by the shell-global resizer (setupModuleResizers
  // in app.js), which wires the `.ctox-column-resizer[data-resizer-var]` handles in
  // index.html declaratively (drag + keyboard + per-module localStorage). We must
  // NOT DIY-wire them here or each handle gets double-wired. Return a no-op teardown;
  // the mount() call site keeps a valid cleanup reference.
  return () => {};

  // eslint-disable-next-line no-unreachable
  const containerEl = host.querySelector('[data-creator-root]') || host;
  const resizers = [];
  const configs = [
    {
      side: 'left',
      selector: '[data-resizer="left"]',
      cssVar: '--creator-left-width',
      storageKey: 'ctox.creator.layout.leftWidth',
      defaultWidth: 320,
      minWidth: 260,
      maxWidth: 550,
    },
    {
      side: 'right',
      selector: '[data-resizer="right"]',
      cssVar: '--creator-right-width',
      storageKey: 'ctox.creator.layout.rightWidth',
      defaultWidth: 300,
      minWidth: 240,
      maxWidth: 520,
    },
  ];

  for (const config of configs) {
    const resizerEl = host.querySelector(config.selector);
    if (!resizerEl) continue;

    const savedWidth = parseInt(localStorage.getItem(config.storageKey) || '', 10);
    const initialWidth = Number.isFinite(savedWidth) ? savedWidth : config.defaultWidth;
    containerEl.style.setProperty(config.cssVar, `${initialWidth}px`);

    resizers.push(new CtoxResizer({
      resizerEl,
      containerEl,
      cssVar: config.cssVar,
      side: config.side,
      minWidth: config.minWidth,
      maxWidth: config.maxWidth,
      onResize: (width) => {
        localStorage.setItem(config.storageKey, String(Math.round(width)));
      }
    }));
  }

  return () => {
    for (const resizer of resizers) resizer.destroy();
  };
}

async function startCreatorDataStreams(ctx, host) {
  await Promise.allSettled([
    ctx.sync?.startCollection?.('business_module_catalog'),
    ctx.sync?.startCollection?.('business_commands'),
  ]);

  const catalogColl = getCollection(ctx, 'business_module_catalog');
  const commandColl = getCollection(ctx, 'business_commands');

  try {
    const catalogDoc = await catalogColl?.findOne?.('module-catalog')?.exec?.();
    state.installedApps = normalizeCreatorInstalledApps(catalogDoc?.toJSON?.() || {});
  } catch (error) {
    addConsoleLog(`[WARN] Modulkatalog konnte nicht geladen werden: ${error.message}`, 'warning');
  }

  try {
    const commandDocs = await commandColl?.find?.()?.exec?.();
    state.creatorPrompts = normalizeCreatorPromptSuggestions(commandDocs?.map((doc) => doc?.toJSON?.() || doc) || []);
  } catch (error) {
    addConsoleLog(`[WARN] CTOX-Prompts konnten nicht geladen werden: ${error.message}`, 'warning');
  }

  state.catalogSubscription = catalogColl?.findOne?.('module-catalog')?.$?.subscribe?.((doc) => {
    state.installedApps = normalizeCreatorInstalledApps(doc?.toJSON?.() || {});
    renderCreatorRightRail(host);
  }) || null;

  state.commandSubscription = commandColl?.find?.()?.$?.subscribe?.((docs) => {
    state.creatorPrompts = normalizeCreatorPromptSuggestions(docs?.map((doc) => doc?.toJSON?.() || doc) || []);
    renderCreatorRightRail(host);
  }) || null;

  renderCreatorRightRail(host);
}

function getCollection(ctx, name) {
  return ctx.db?.collection?.(name) || ctx.db?.[name] || null;
}

function cleanupSubscription(subscription) {
  if (typeof subscription === 'function') {
    subscription();
    return;
  }
  subscription?.unsubscribe?.();
}

function renderCreatorRightRail(host) {
  const installedList = host.querySelector('[data-creator-installed-list]');
  const installedEmpty = host.querySelector('[data-creator-installed-empty]');
  const promptsList = host.querySelector('[data-creator-prompts-list]');
  const promptsEmpty = host.querySelector('[data-creator-prompts-empty]');

  if (installedList && installedEmpty) {
    installedList.innerHTML = state.installedApps.map(renderInstalledAppCard).join('');
    installedEmpty.hidden = state.installedApps.length > 0;
    installedList.hidden = state.installedApps.length === 0;
  }

  if (promptsList && promptsEmpty) {
    promptsList.innerHTML = state.creatorPrompts.map(renderCreatorPromptCard).join('');
    promptsEmpty.hidden = state.creatorPrompts.length > 0;
    promptsList.hidden = state.creatorPrompts.length === 0;
  }
}

function renderInstalledAppCard(app) {
  return `
    <article class="creator-mini-card" data-creator-installed-app="${escapeHtml(app.id)}">
      <div class="creator-mini-card-main">
        <strong>${escapeHtml(app.title)}</strong>
        <span>${escapeHtml(app.category)} · ${escapeHtml(app.version)}</span>
        ${app.description ? `<p>${escapeHtml(app.description)}</p>` : ''}
      </div>
      <div class="creator-mini-actions">
        <button type="button" class="os-icon-btn" data-open-installed-app="${escapeHtml(app.id)}" title="App öffnen" aria-label="${escapeHtml(app.title)} öffnen">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.3" aria-hidden="true"><path d="M7 17 17 7M8 7h9v9"/></svg>
        </button>
        <button type="button" class="os-icon-btn" data-upgrade-installed-app="${escapeHtml(app.id)}" title="Upgrade vorbereiten" aria-label="${escapeHtml(app.title)} Upgrade vorbereiten">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.3" aria-hidden="true"><path d="M12 3v12"/><path d="m7 8 5-5 5 5"/><path d="M5 21h14"/></svg>
        </button>
      </div>
    </article>
  `;
}

function renderCreatorPromptCard(item) {
  const prompt = item.prompt.length > 140 ? `${item.prompt.slice(0, 137)}...` : item.prompt;
  return `
    <article class="creator-mini-card" data-creator-prompt="${escapeHtml(item.id)}">
      <div class="creator-mini-card-main">
        <strong>${escapeHtml(item.title)}</strong>
        <span>${escapeHtml(item.status)}</span>
        <p>${escapeHtml(prompt)}</p>
      </div>
      <div class="creator-mini-actions">
        <button type="button" class="os-icon-btn" data-use-creator-prompt="${escapeHtml(item.id)}" title="Prompt übernehmen" aria-label="Prompt übernehmen">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.3" aria-hidden="true"><path d="M12 5v14"/><path d="m19 12-7 7-7-7"/></svg>
        </button>
      </div>
    </article>
  `;
}

function wireUi(host) {
  const inputId = host.querySelector('#input-app-id');
  const inputTitle = host.querySelector('#input-app-title');
  const inputDesc = host.querySelector('#input-app-desc');
  const selectCategory = host.querySelector('#select-app-category');
  const selectLayout = host.querySelector('#select-app-layout');
  const btnAddColl = host.querySelector('#btn-add-collection');
  const inputNewColl = host.querySelector('#input-new-collection');
  const btnDeploy = host.querySelector('#btn-deploy-app');
  const selectPreset = host.querySelector('#select-preset-prompt');
  const inputPrompt = host.querySelector('#ai-prompt-input');
  const btnApplyPrompt = host.querySelector('#btn-apply-prompt');
  const specDiagnostics = host.querySelector('#creator-spec-diagnostics');
  const syncDot = host.querySelector('#deploy-sync-dot');
  const syncText = host.querySelector('#deploy-sync-text');
  state.specPrompt = '';
  state.isOptimizing = false;
  state.isDeploying = false;

  // Accordion Expand/Collapse Trigger
  const accordionTrigger = host.querySelector('#expert-accordion-btn');
  const accordionContent = host.querySelector('#expert-accordion-content');
  const accordionChevron = host.querySelector('.accordion-chevron');
  accordionTrigger.addEventListener('click', () => {
    const isCollapsed = accordionContent.classList.contains('is-collapsed');
    accordionTrigger.setAttribute('aria-expanded', String(isCollapsed));
    if (isCollapsed) {
      accordionContent.classList.remove('is-collapsed');
      accordionChevron.style.transform = 'rotate(180deg)';
    } else {
      accordionContent.classList.add('is-collapsed');
      accordionChevron.style.transform = 'rotate(0deg)';
    }
  });

  // Preset Selection Change
  selectPreset.addEventListener('change', () => {
    const presetKey = selectPreset.value;
    if (!presetKey || !PRESETS[presetKey]) return;

    const preset = PRESETS[presetKey];
    inputPrompt.value = preset.prompt;

    // Automatically fill advanced values
    inputId.value = normalizeModuleId(preset.id);
    inputTitle.value = preset.title;
    inputDesc.value = preset.desc;
    selectCategory.value = preset.category;
    selectLayout.value = preset.layout;
    state.appCollections = preset.collections.map(normalizeCollectionName).filter(Boolean);

    renderCollectionsList(host);
    syncStateFromInputs();
    state.specPrompt = inputPrompt.value.trim();
    updateCreatorActionState();

    addConsoleLog(`[INFO] Vorlage '${preset.title}' erfolgreich geladen. Spezifikation im Hintergrund angepasst.`, 'info');
  });

  // AI Prompt Spec Optimizer Trigger
  btnApplyPrompt.addEventListener('click', async () => {
    const prompt = inputPrompt.value.trim();
    if (!prompt) {
      state.ctx.notifications.show({
        title: 'Leerer Prompt',
        message: 'Bitte gib eine kurze Beschreibung in das Prompt-Feld ein.',
        type: 'warning'
      });
      return;
    }

    state.isOptimizing = true;
    btnApplyPrompt.innerHTML = `
      <svg class="animate-spin" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" style="animation: pulse-sync 1s infinite alternate; margin-right: 6px;"><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/></svg>
      KI analysiert Prompt...
    `;
    updateCreatorActionState();

    try {
      addConsoleLog('==================================================', 'info');
      addConsoleLog('[KI-OPERATOR] Analysiere Anwendungsbeschreibung...', 'info');
      await new Promise(r => setTimeout(r, 600));

      const spec = deriveSpecFromPrompt(prompt);

      addConsoleLog(`[KI-OPERATOR] Erkenne Domäne & Absicht: ${spec.category}`, 'info');
      await new Promise(r => setTimeout(r, 300));
      addConsoleLog(`[KI-OPERATOR] Bestimme Layout-Struktur: ${spec.layout === 'pane' ? 'Spalten-Tracker' : 'Tabellen-Workspace'}`, 'info');
      addConsoleLog(`[KI-OPERATOR] Generiere Datentabellen: [${spec.collections.join(', ')}]`, 'info');

      inputId.value = spec.id;
      inputTitle.value = spec.title;
      inputDesc.value = spec.desc;
      selectCategory.value = spec.category;
      selectLayout.value = spec.layout;
      state.appCollections = [...spec.collections];

      renderCollectionsList(host);
      syncStateFromInputs({ preserveFreshSpec: true });
      state.specPrompt = prompt;

      addConsoleLog(`[SUCCESS] Spezifikation für '${spec.title}' erfolgreich generiert.`, 'success');
      addConsoleLog('==================================================', 'success');

      state.ctx.notifications.show({
        title: 'Spezifikation optimiert',
        message: `Die App-Spezifikation für '${spec.title}' wurde aktualisiert.`,
        type: 'success'
      });
    } catch (error) {
      state.specPrompt = '';
      addConsoleLog(`[ERROR] Spezifikation konnte nicht aktualisiert werden: ${error.message}`, 'error');
      state.ctx.notifications.show({
        title: 'Spezifikation fehlgeschlagen',
        message: error.message,
        type: 'error'
      });
    } finally {
      state.isOptimizing = false;
      btnApplyPrompt.innerHTML = `
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/></svg>
        Spezifikation optimieren & anwenden
      `;
      updateCreatorActionState();
    }
  });

  const syncStateFromInputs = ({ preserveFreshSpec = false } = {}) => {
    state.appId = normalizeModuleId(inputId.value);
    if (inputId.value !== state.appId) inputId.value = state.appId;
    state.appTitle = inputTitle.value.trim();
    state.appDesc = inputDesc.value.trim();
    state.appCategory = selectCategory.value;
    state.appLayout = selectLayout.value;
    if (!preserveFreshSpec) state.specPrompt = '';

    generateAllFiles();
    updateCreatorActionState();
  };

  const updateCreatorActionState = () => {
    const actionState = computeCreatorActionState({
      prompt: inputPrompt.value,
      specPrompt: state.specPrompt,
      appId: inputId.value,
      appTitle: inputTitle.value,
      appDesc: inputDesc.value,
      appCollections: state.appCollections,
      isOptimizing: state.isOptimizing,
      isDeploying: state.isDeploying
    });
    btnApplyPrompt.disabled = !actionState.optimizeReady;
    btnDeploy.disabled = !actionState.deployReady;
    btnApplyPrompt.setAttribute('aria-disabled', String(btnApplyPrompt.disabled));
    btnDeploy.setAttribute('aria-disabled', String(btnDeploy.disabled));
    btnApplyPrompt.title = actionState.hasPrompt
      ? 'Prompt analysieren und Spezifikation aktualisieren'
      : 'Bitte zuerst einen Prompt eingeben.';
    btnDeploy.title = actionState.deployReady
      ? 'Frische Spezifikation generieren und installieren'
      : actionState.diagnostic;
    btnDeploy.dataset.state = actionState.deployReady ? 'ready' : 'blocked';
    if (specDiagnostics) {
      specDiagnostics.textContent = actionState.diagnostic;
      specDiagnostics.dataset.state = actionState.deployReady ? 'ready' : actionState.hasPrompt ? 'pending' : 'blocked';
    }
    if (!state.isDeploying && syncText && syncDot) {
      syncDot.style.background = '';
      syncText.textContent = actionState.diagnostic;
      syncDot.className = actionState.deployReady ? 'sync-dot is-ready' : 'sync-dot is-blocked';
    }
    return actionState;
  };

  inputPrompt.addEventListener('input', () => {
    state.specPrompt = '';
    updateCreatorActionState();
  });

  // Text inputs changed manually inside the expandable accordion
  [inputId, inputTitle, inputDesc, selectCategory, selectLayout].forEach(el => {
    el.addEventListener('input', () => syncStateFromInputs({ preserveFreshSpec: Boolean(inputPrompt.value.trim()) }));
  });

  // DB Collection Visual builder in advanced accordion
  const renderCollectionsList = (h) => {
    const listEl = h.querySelector('#collections-list');
    listEl.innerHTML = '';
    state.appCollections.forEach((coll, idx) => {
      const row = document.createElement('div');
      row.className = 'collection-row';
      row.innerHTML = `
        <span style="font-family: var(--font-mono); font-size: 11px; color: var(--accent); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">${coll}</span>
        <button type="button" class="os-icon-btn is-danger" data-remove-idx="${idx}" aria-label="Datentabelle ${coll} entfernen" title="Datentabelle entfernen">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" aria-hidden="true"><path d="M18 6 6 18M6 6l12 12"/></svg>
        </button>
      `;
      row.querySelector('[data-remove-idx]').addEventListener('click', async (e) => {
        const removeIdx = parseInt(e.currentTarget.getAttribute('data-remove-idx'), 10);
        const name = state.appCollections[removeIdx];
        const confirmed = await showBusinessConfirm(`Datentabelle "${name}" aus der Spezifikation entfernen?`, {
          title: 'Datentabelle entfernen',
          confirmLabel: 'Entfernen',
          cancelLabel: 'Abbrechen',
          kind: 'danger'
        });
        if (!confirmed) return;
        state.appCollections.splice(removeIdx, 1);
        renderCollectionsList(h);
        syncStateFromInputs({ preserveFreshSpec: Boolean(inputPrompt.value.trim()) });
      });
      listEl.appendChild(row);
    });
  };

  btnAddColl.addEventListener('click', () => {
    const newName = normalizeCollectionName(inputNewColl.value);
    if (!newName) return;
    if (state.appCollections.includes(newName)) {
      addConsoleLog(`[WARN] Datentabelle '${newName}' existiert bereits.`, 'warning');
      return;
    }
    state.appCollections.push(newName);
    inputNewColl.value = '';
    renderCollectionsList(host);
    syncStateFromInputs({ preserveFreshSpec: Boolean(inputPrompt.value.trim()) });
    addConsoleLog(`[INFO] Datentabelle '${newName}' hinzugefügt.`, 'info');
  });

  inputNewColl.addEventListener('keydown', (event) => {
    if (event.key !== 'Enter') return;
    event.preventDefault();
    btnAddColl.click();
  });

  host.querySelector('[data-creator-right-body]')?.addEventListener('click', (event) => {
    const openButton = event.target.closest('[data-open-installed-app]');
    const upgradeButton = event.target.closest('[data-upgrade-installed-app]');
    const promptButton = event.target.closest('[data-use-creator-prompt]');

    if (openButton) {
      window.location.hash = `#${encodeURIComponent(openButton.dataset.openInstalledApp || '')}`;
      return;
    }

    if (upgradeButton) {
      window.location.hash = `#creator?upgrade=${encodeURIComponent(upgradeButton.dataset.upgradeInstalledApp || '')}`;
      return;
    }

    if (promptButton) {
      const prompt = state.creatorPrompts.find((item) => item.id === promptButton.dataset.useCreatorPrompt);
      if (!prompt) return;
      inputPrompt.value = prompt.prompt;
      state.specPrompt = '';
      updateCreatorActionState();
      addConsoleLog(`[INFO] CTOX-Prompt '${prompt.title}' übernommen.`, 'info');
      inputPrompt.focus();
    }
  });

  renderCollectionsList(host);
  updateCreatorActionState();

  // Install / Deploy Button
  btnDeploy.addEventListener('click', async () => {
    try {
      const currentPrompt = inputPrompt.value.trim();
      if (!currentPrompt || state.specPrompt !== currentPrompt) {
        state.ctx.notifications.show({
          title: 'Spezifikation nicht bereit',
          message: 'Bitte gib einen Prompt ein und aktualisiere die Spezifikation, bevor du installierst.',
          type: 'warning'
        });
        addConsoleLog('[BLOCKED] Installation verhindert: Prompt fehlt oder Spezifikation ist nicht frisch.', 'warning');
        updateCreatorActionState();
        return;
      }

      const actionState = updateCreatorActionState();
      if (!actionState.deployReady) {
        addConsoleLog(`[BLOCKED] Installation verhindert: ${actionState.diagnostic}`, 'warning');
        return;
      }

      const confirmed = await showBusinessConfirm(`Modul "${state.appTitle}" (${state.appId}) jetzt installieren? Die generierten Dateien werden in installed-modules/${state.appId}/ geschrieben.`, {
        title: 'Installation bestätigen',
        confirmLabel: 'Installieren',
        cancelLabel: 'Abbrechen'
      });
      if (!confirmed) {
        addConsoleLog('[INFO] Installation abgebrochen. Es wurden keine Dateien geschrieben.', 'info');
        return;
      }

      await triggerAppDeployment(host, updateCreatorActionState);
    } catch (e) {
      console.error('[ERROR] triggerAppDeployment failed:', e);
      state.isDeploying = false;
      updateCreatorActionState();
    }
  });

  // Intercept and parse hash parameters for Upgrade preloading
  (async () => {
    const hash = window.location.hash || '';
    const queryStr = hash.includes('?') ? hash.split('?')[1] : '';
    const params = new URLSearchParams(queryStr);
    const upgradeAppId = params.get('upgrade');

    if (upgradeAppId) {
      try {
        addConsoleLog(`[INFO] Lade bestehende App-Spezifikation für Upgrade von '${upgradeAppId}'...`, 'info');
        const manifestUrl = `installed-modules/${upgradeAppId}/module.json`;
        const manifest = await fetch(manifestUrl).then(res => {
          if (!res.ok) throw new Error(`App '${upgradeAppId}' konnte nicht geladen werden.`);
          return res.json();
        });

        if (inputId) inputId.value = manifest.id || upgradeAppId;
        if (inputTitle) inputTitle.value = manifest.title || '';
        if (inputDesc) inputDesc.value = manifest.description || '';
        if (selectCategory) selectCategory.value = manifest.category || 'Management';
        if (selectLayout) selectLayout.value = manifest.layout?.shell || 'full-workspace';
        if (inputPrompt) {
          inputPrompt.value = `Upgrade für ${manifest.title || upgradeAppId}: ${manifest.description || ''}`;
        }

        // Increment version
        const currentVer = manifest.version || 'v1';
        const verNum = parseInt(currentVer.replace('v', ''), 10) || 1;
        const nextVer = `v${verNum + 1}`;
        state.appVersion = nextVer;

        // Clean collection names of version suffixes
        const baseCollections = (Array.isArray(manifest.collections) ? manifest.collections : ['records'])
          .map(coll => coll.replace(/_v\d+$/, ''));
        state.appCollections = baseCollections;

        addConsoleLog(`[INFO] Upgrade-Version erkannt: ${currentVer} -> ${nextVer}. Suffixe aus Datentabellen entfernt.`, 'info');

        renderCollectionsList(host);
        syncStateFromInputs({ preserveFreshSpec: true });
        state.specPrompt = inputPrompt.value.trim();

        addConsoleLog(`[SUCCESS] Spezifikation für '${manifest.title || upgradeAppId}' erfolgreich geladen. Passen Sie die Prompt-Eingabe an und starten Sie das Deployment!`, 'success');
        updateCreatorActionState();
      } catch (err) {
        addConsoleLog(`[ERROR] Fehler beim Laden des Upgrades: ${err.message}`, 'error');
        updateCreatorActionState();
      }
    }
  })();
}

function addConsoleLog(text, type = '') {
  console.log(text);
  const container = document.querySelector('#console-logs-container');
  if (!container) return;
  const el = document.createElement('div');
  el.className = `console-log-entry ${type}`;
  el.textContent = text;
  container.appendChild(el);
  container.scrollTop = container.scrollHeight;
}

function generateSvgLogo(appId, category) {
  const cat = String(category || '').trim().toLowerCase();

  if (cat === 'productivity') {
    return `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" class="svg-icon svg-${appId}">
  <defs>
    <linearGradient id="grad-${appId}" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#f59e0b" />
      <stop offset="100%" stop-color="#ea580c" />
    </linearGradient>
  </defs>
  <rect x="3" y="4" width="18" height="16" rx="3" ry="3" fill="url(#grad-${appId})" fill-opacity="0.12" stroke="url(#grad-${appId})" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></rect>
  <line x1="3" y1="9" x2="21" y2="9" stroke="url(#grad-${appId})" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></line>
  <path d="M9 2v4M15 2v4" stroke="url(#grad-${appId})" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></path>
  <path d="M8 14l2 2 4-4" stroke="url(#grad-${appId})" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></path>
</svg>`;
  } else if (cat === 'finance') {
    return `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" class="svg-icon svg-${appId}">
  <defs>
    <linearGradient id="grad-${appId}" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#10b981" />
      <stop offset="100%" stop-color="#059669" />
    </linearGradient>
  </defs>
  <rect x="3" y="3" width="18" height="18" rx="3" ry="3" fill="url(#grad-${appId})" fill-opacity="0.12" stroke="url(#grad-${appId})" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></rect>
  <path d="M18 17V9M12 17v-4M6 17v-2" stroke="url(#grad-${appId})" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></path>
  <path d="M6 14l4-3 4 2 4-5" stroke="#ffffff" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"></path>
  <circle cx="18" cy="8" r="1.5" fill="url(#grad-${appId})" stroke="#ffffff" stroke-width="1"></circle>
</svg>`;
  } else if (cat === 'utilities') {
    return `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" class="svg-icon svg-${appId}">
  <defs>
    <linearGradient id="grad-${appId}" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#f43f5e" />
      <stop offset="100%" stop-color="#e11d48" />
    </linearGradient>
  </defs>
  <circle cx="12" cy="12" r="9" fill="url(#grad-${appId})" fill-opacity="0.12" stroke="url(#grad-${appId})" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></circle>
  <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" stroke="url(#grad-${appId})" stroke-width="2" stroke-linecap="round"></path>
  <circle cx="12" cy="12" r="3.5" fill="#ffffff" stroke="url(#grad-${appId})" stroke-width="1.5"></circle>
</svg>`;
  } else if (cat === 'development') {
    return `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" class="svg-icon svg-${appId}">
  <defs>
    <linearGradient id="grad-${appId}" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#8b5cf6" />
      <stop offset="100%" stop-color="#6366f1" />
    </linearGradient>
  </defs>
  <rect x="3" y="3" width="18" height="18" rx="3" ry="3" fill="url(#grad-${appId})" fill-opacity="0.12" stroke="url(#grad-${appId})" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></rect>
  <polyline points="7 8 11 12 7 16" stroke="url(#grad-${appId})" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></polyline>
  <line x1="13" y1="16" x2="17" y2="16" stroke="url(#grad-${appId})" stroke-width="2" stroke-linecap="round"></line>
</svg>`;
  } else {
    return `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" class="svg-icon svg-${appId}">
  <defs>
    <linearGradient id="grad-${appId}" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#3b82f6" />
      <stop offset="100%" stop-color="#06b6d4" />
    </linearGradient>
  </defs>
  <rect x="3" y="3" width="18" height="18" rx="3" ry="3" fill="url(#grad-${appId})" fill-opacity="0.12" stroke="url(#grad-${appId})" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></rect>
  <rect x="6" y="6" width="12" height="3" rx="1" fill="url(#grad-${appId})" fill-opacity="0.2" stroke="url(#grad-${appId})" stroke-width="1.5"></rect>
  <rect x="6" y="11" width="12" height="3" rx="1" fill="url(#grad-${appId})" fill-opacity="0.2" stroke="url(#grad-${appId})" stroke-width="1.5"></rect>
  <path d="M6 16h6" stroke="url(#grad-${appId})" stroke-width="2" stroke-linecap="round"></path>
  <circle cx="16" cy="16" r="1.5" fill="#ffffff" stroke="url(#grad-${appId})" stroke-width="1"></circle>
</svg>`;
  }
}

function generateAllFiles() {
  const appId = state.appId || 'lagerverwaltung';
  const appTitle = state.appTitle || 'Lagerverwaltung';
  const appDesc = state.appDesc || 'Beschreibung';
  const appCategory = state.appCategory || 'Management';
  const appLayout = state.appLayout || 'full-workspace';
  const collections = state.appCollections.length > 0 ? state.appCollections : ['items'];
  const primaryColl = collections[0];
  const appVersion = /^\d+\.\d+\.\d+$/.test(String(state.appVersion || '').trim())
    ? String(state.appVersion).trim()
    : '0.1.0';
  const appCollectionVersion = `v${appVersion.replace(/\./g, '_')}`;
  const versionedCollections = collections.map(coll => `${coll}_${appCollectionVersion}`);
  const moduleCollections = Array.from(new Set(['business_commands', ...versionedCollections]));

  const iconSvg = generateSvgLogo(appId, appCategory);
  state.generatedFiles['icon.svg'] = iconSvg;

  // 1. module.json
  state.generatedFiles['module.json'] = JSON.stringify({
    id: appId,
    title: appTitle,
    description: appDesc,
    entry: `installed-modules/${appId}/index.html`,
    install_scope: 'installed',
    collections: moduleCollections,
    layout: {
      shell: appLayout,
      left: `${appTitle} Navigation`,
      center: `${appTitle} Workbench`
    },
    category: appCategory,
    version: appVersion,
    developer: 'CTOX Developer App',
    license: 'AGPL-3.0-only',
    tags: [appId, 'installed-module', appCategory.toLowerCase()]
  }, null, 2);

  // 2. schemas. Installed modules persist via shell-provided CTOX DB collections.
  const collectionSchemas = Object.fromEntries(versionedCollections.map((collectionName) => [collectionName, {
    title: `${collectionName} schema`,
    version: 0,
    primaryKey: 'id',
    type: 'object',
    properties: {
      id: { type: 'string', maxLength: 120 },
      title: { type: 'string' },
      status: { type: 'string' },
      updated_at_ms: { type: 'number' },
      data: { type: 'object', additionalProperties: true },
    },
    required: ['id', 'title', 'status', 'updated_at_ms'],
  }]));
  state.generatedFiles['collections.schema.json'] = JSON.stringify({
    schema_format: 'ctox-business-os-module-collections-v1',
    collections: collectionSchemas,
  }, null, 2);
  state.generatedFiles['core/schemas.mjs'] = `export const primaryCollection = '${primaryColl}_${appCollectionVersion}';
export const collectionSchemas = ${JSON.stringify(collectionSchemas, null, 2)};
`;
  state.generatedFiles['schema.js'] = `import { collectionSchemas } from './core/schemas.mjs';

export const collections = Object.fromEntries(
  Object.entries(collectionSchemas).map(([name, schema]) => [name, { schema }])
);
`;

  // 3. index.html — built on the shared design system (shared/base.css).
  // The shell loads base.css once; generated apps use its classes directly and
  // get the declarative shell column resizer ([data-resize-frame] +
  // .ctox-column-resizer[data-resizer-var]) without any module JS.
  const detailCardHtml = `      <div id="detail-card" class="ctox-card" hidden>
        <div class="ctox-card-body" style="padding-top: 12px; display: grid; gap: 14px;">
          <div>
            <label class="ctox-field-label" data-t="fieldTitle">Titel des Eintrags</label>
            <input type="text" id="record-detail-title" class="ctox-input">
          </div>
          <div>
            <label class="ctox-field-label" data-t="fieldStatus">Status</label>
            <select id="record-detail-status" class="ctox-select">
              <option value="Aktiv">Aktiv</option>
              <option value="Entwurf">Entwurf</option>
              <option value="Archiviert">Archiviert</option>
            </select>
          </div>
          <div style="display: flex; gap: 8px; flex-wrap: wrap;">
            <button type="button" class="ctox-button is-primary" id="btn-save-record" data-t="saveRecord">Speichern</button>
            <button type="button" class="ctox-button" id="btn-request-review" data-t="requestReview">Review anfordern</button>
          </div>
        </div>
      </div>`;
  if (appLayout === 'full-workspace') {
    state.generatedFiles['index.html'] = `<main class="ctox-workspace ctox-workspace--two-pane ${appId}-module" data-module-root="${appId}" data-resize-frame>
  <aside class="ctox-pane" aria-label="${appTitle} Navigation">
    <header class="ctox-pane-band ctox-pane-header">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker" data-t="backlog">Kategorie</span>
          <h2 class="ctox-pane-title" data-t="itemsTitle">${appTitle}</h2>
        </div>
      </div>
    </header>
    <div class="ctox-list" data-list-container></div>
  </aside>

  <button class="ctox-column-resizer" type="button" data-resizer="left" data-resizer-var="--ctox-left-width" data-resizer-min="220" data-resizer-max="550" aria-label="Spaltenbreite anpassen"></button>

  <section class="ctox-pane" aria-label="${appTitle} Workbench">
    <header class="ctox-pane-band ctox-pane-header">
      <div class="ctox-pane-title-row">
        <div class="ctox-pane-titles">
          <span class="ctox-pane-kicker" data-t="workbench">Arbeitsfläche</span>
          <h2 class="ctox-pane-title" id="selected-item-title" data-t="noSelection">Kein Eintrag gewählt</h2>
        </div>
        <button type="button" class="ctox-button is-primary" id="btn-create-record" data-t="createRecord">Eintrag erstellen</button>
      </div>
    </header>
    <main class="ctox-pane-scroll" style="padding: 16px;">
      <div class="ctox-empty" id="empty-state">
        <span data-t="selectPrompt">Wähle einen Datensatz links aus oder erstelle einen neuen.</span>
      </div>
${detailCardHtml}
    </main>
  </section>
</main>
`;
  } else {
    state.generatedFiles['index.html'] = `<div class="${appId}-module" data-module-root="${appId}">
  <!-- Pane layout: the shell renders the outer left/right panes; the module owns only the center workbench. -->
  <div style="display: flex; flex-direction: column; width: 100%; height: 100%;">
    <header class="ctox-toolbar">
      <div class="ctox-pane-titles" style="flex: 1 1 auto;">
        <span class="ctox-pane-kicker" data-t="workbench">Arbeitsbereich</span>
        <h2 class="ctox-pane-title" id="selected-item-title">${appTitle} Workbench</h2>
      </div>
      <button type="button" class="ctox-button is-primary" id="btn-create-record" data-t="createRecord">Eintrag erstellen</button>
    </header>

    <main class="ctox-pane-scroll" style="flex: 1; padding: 16px;">
      <div class="ctox-empty" id="empty-state">
        <span data-t="selectPromptPane">Nutze die linke Navigationsspalte zur Auswahl und Bearbeitung.</span>
      </div>
${detailCardHtml}
    </main>
  </div>
</div>
`;
  }

  // 4. index.css — module-specific styles only. Frame, panes, lists, buttons,
  // inputs, cards, empty states all come from the shell design system
  // (app.css tokens + shared/base.css classes), so the module stylesheet
  // stays nearly empty and dark/light theming follows the shell automatically.
  state.generatedFiles['index.css'] = `/* ${appId} — module-specific styles.
   The layout frame and controls come from shared/base.css (.ctox-workspace,
   .ctox-pane, .ctox-list, .ctox-button, .ctox-input, .ctox-card, .ctox-empty).
   Resolve every color through the shell tokens (--bg, --surface, --text,
   --muted, --accent, ...). Do not define tokens on :root and do not redefine
   shell tokens — the module conformance guard enforces this. */

.${appId}-record-meta {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  margin-top: 4px;
  font-size: 11px;
  color: var(--muted);
}

@media (max-width: 768px) {
  .${appId}-module.ctox-workspace {
    grid-template-columns: minmax(0, 1fr);
  }
  .${appId}-module .ctox-column-resizer,
  .${appId}-module aside.ctox-pane {
    display: none !important;
  }
}
`;

  // 5. locales — same keys as the inline fallback labels in index.js.
  const localeDe = {
    backlog: 'Datenkatalog',
    itemsTitle: appTitle,
    workbench: 'Arbeitsfläche',
    noSelection: 'Kein Eintrag gewählt',
    createRecord: 'Eintrag erstellen',
    saveRecord: 'Speichern',
    requestReview: 'Review anfordern',
    fieldTitle: 'Titel des Eintrags',
    fieldStatus: 'Status',
    selectPrompt: 'Wähle einen Datensatz links aus oder erstelle einen neuen.',
    selectPromptPane: 'Nutze die linke Navigationsspalte zur Auswahl und Bearbeitung.',
    emptyList: 'Keine Einträge vorhanden',
  };
  const localeEn = {
    backlog: 'Data catalog',
    itemsTitle: appTitle,
    workbench: 'Workbench',
    noSelection: 'No record selected',
    createRecord: 'Create record',
    saveRecord: 'Save',
    requestReview: 'Request review',
    fieldTitle: 'Record title',
    fieldStatus: 'Status',
    selectPrompt: 'Select a record on the left or create a new one.',
    selectPromptPane: 'Use the left navigation pane to select and edit records.',
    emptyList: 'No records yet',
  };
  state.generatedFiles['locales/de.json'] = JSON.stringify(localeDe, null, 2);
  state.generatedFiles['locales/en.json'] = JSON.stringify(localeEn, null, 2);

  // 6. automation helper and self-checks for installed modules.
  state.generatedFiles['core/automation.mjs'] = `export function buildFollowUpCommand(record = {}) {
  const title = record.title || 'Unbenannter Datensatz';
  return {
    id: \`cmd_\${record.id || Date.now()}\`,
    module: '${appId}',
    type: 'business_os.chat.task',
    command_type: 'business_os.chat.task',
    record_id: record.id || null,
    payload: {
      title: \`Review: \${title}\`,
      instruction: \`Review "\${title}" in ${appTitle} and create the next CTOX follow-up if action is required.\`,
      prompt: \`Review "\${title}" in ${appTitle} and create the next CTOX follow-up if action is required.\`,
      source_module: '${appId}',
      source_collection: '${primaryColl}_${appCollectionVersion}',
      record_snapshot: record,
      outbound_channel: 'business_os_chat',
      response_channel: 'business_os_chat',
    },
    client_context: {
      source: '${appId}',
      surface: '${appId}.record-review',
    },
  };
}
`;
  state.generatedFiles[`tests/${appId}.test.mjs`] = `import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { buildFollowUpCommand } from '../core/automation.mjs';

const schemaDoc = JSON.parse(readFileSync(new URL('../collections.schema.json', import.meta.url), 'utf8'));
assert.equal(schemaDoc.schema_format, 'ctox-business-os-module-collections-v1');
assert.ok(schemaDoc.collections['${primaryColl}_${appCollectionVersion}']);

const command = buildFollowUpCommand({ id: 'demo', title: 'Demo' });
assert.equal(command.type, 'business_os.chat.task');
assert.equal(command.command_type, 'business_os.chat.task');
assert.deepEqual(command.payload.record_snapshot, { id: 'demo', title: 'Demo' });
`;

  // 7. index.js
  state.generatedFiles['index.js'] = `import { loadModuleMessages } from '../../shared/i18n.js';
import { buildFollowUpCommand } from './core/automation.mjs';

const labels = {
  de: ${JSON.stringify(localeDe, null, 2).replace(/\n/g, '\n  ')},
  en: ${JSON.stringify(localeEn, null, 2).replace(/\n/g, '\n  ')}
};

const APP_METADATA = {
  version: '${appVersion}',
  collectionVersion: '${appCollectionVersion}',
  primaryCollection: '${primaryColl}_${appCollectionVersion}',
  collections: ${JSON.stringify(collections)}
};

const PRIMARY_COLL = APP_METADATA.primaryCollection;

const state = {
  ctx: null,
  t: (key, fallback) => fallback ?? key,
  records: [],
  selectedId: null,
  dbSubscription: null
};

export async function mount(ctx) {
  state.ctx = ctx;
  await ensureStyles();

  const messages = await loadModuleMessages(import.meta.url, ctx.locale, labels);
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;

  const html = await fetch(new URL('./index.html', import.meta.url)).then(res => res.text());
  ctx.host.innerHTML = html;
  applyTranslations(ctx.host, state.t);

  await Promise.allSettled([
    state.ctx.sync?.startCollection?.(PRIMARY_COLL),
    state.ctx.sync?.startCollection?.('business_commands')
  ]);

  await loadInitialData();
  state.dbSubscription = wireReactiveSync();
  wireUi(ctx.host);

  return () => {
    if (typeof state.dbSubscription === 'function') {
      state.dbSubscription();
    } else if (state.dbSubscription && typeof state.dbSubscription.unsubscribe === 'function') {
      state.dbSubscription.unsubscribe();
    }
  };
}

async function ensureStyles() {
  if (document.querySelector('link[data-module-styles="${appId}"]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./index.css', import.meta.url).href;
  link.dataset.moduleStyles = '${appId}';
  document.head.append(link);
}

function applyTranslations(root, t) {
  root.querySelectorAll('[data-t]').forEach((el) => {
    el.textContent = t(el.dataset.t, el.textContent);
  });
}

function getCollection(name) {
  return state.ctx?.db?.collection?.(name)
    || state.ctx?.db?.collections?.[name]
    || null;
}

async function loadInitialData() {
  const collection = getCollection(PRIMARY_COLL);
  if (!collection) {
    state.records = [];
    renderList();
    return;
  }
  const items = await collection.find().exec();
  state.records = items.map(item => item.toJSON ? item.toJSON() : item);
  renderList();
}

function wireReactiveSync() {
  const collection = getCollection(PRIMARY_COLL);
  if (!collection?.find) return () => {};
  const query = collection.find();
  if (!query?.$?.subscribe) return () => {};

  const sub = query.$.subscribe((items) => {
    state.records = items.map(item => item.toJSON ? item.toJSON() : item);
    renderList();
    if (state.selectedId) {
      const activeItem = state.records.find(record => record.id === state.selectedId);
      if (activeItem) showDetail(activeItem);
    }
  });
  return sub;
}

function renderList() {
  const container = state.ctx.host.querySelector('[data-list-container]') || state.ctx.left?.querySelector('[data-list-container]');
  if (!container) return;
  container.innerHTML = '';

  if (state.records.length === 0) {
    const empty = document.createElement('div');
    empty.className = 'ctox-empty';
    empty.textContent = state.t('emptyList', 'Keine Einträge vorhanden');
    container.appendChild(empty);
    return;
  }

  state.records.forEach((record) => {
    const card = document.createElement('button');
    card.type = 'button';
    card.className = 'ctox-list-item';
    card.setAttribute('aria-selected', state.selectedId === record.id ? 'true' : 'false');
    card.dataset.id = record.id;
    card.setAttribute('data-context-module', '${appId}');
    card.setAttribute('data-context-record-type', PRIMARY_COLL);
    card.setAttribute('data-context-record-id', record.id);
    card.setAttribute('data-context-label', record.title || record.id);

    const title = document.createElement('div');
    title.style.fontWeight = '600';
    title.style.fontSize = '13px';
    title.textContent = record.title || 'Unbenannt';

    const meta = document.createElement('div');
    meta.className = '${appId}-record-meta';
    const status = document.createElement('span');
    status.textContent = 'Status: ' + (record.status || 'Entwurf');
    const updated = document.createElement('span');
    updated.textContent = new Date(record.updated_at_ms || Date.now()).toLocaleTimeString();
    meta.append(status, updated);

    card.append(title, meta);
    card.addEventListener('click', () => selectRecord(record.id));
    container.appendChild(card);
  });
}

function selectRecord(id) {
  state.selectedId = id;
  const record = state.records.find(item => item.id === id);
  if (record) {
    showDetail(record);
    renderList();
  }
}

function showDetail(record) {
  const emptyState = state.ctx.host.querySelector('#empty-state');
  const detailCard = state.ctx.host.querySelector('#detail-card');
  const titleHeader = state.ctx.host.querySelector('#selected-item-title');
  const inputTitle = state.ctx.host.querySelector('#record-detail-title');
  const selectStatus = state.ctx.host.querySelector('#record-detail-status');

  if (emptyState) emptyState.hidden = true;
  if (detailCard) detailCard.hidden = false;
  if (titleHeader) titleHeader.textContent = record.title || 'Unbenannt';
  if (inputTitle) inputTitle.value = record.title || '';
  if (selectStatus) selectStatus.value = record.status || 'Entwurf';
}

function notify(title, message, type = 'success') {
  state.ctx.notifications?.show?.({ title, message, type });
}

function wireUi(host) {
  const btnCreate = host.querySelector('#btn-create-record') || state.ctx.left?.querySelector('#btn-create-record');
  const btnSave = host.querySelector('#btn-save-record');
  const btnReview = host.querySelector('#btn-request-review');

  if (btnCreate) {
    btnCreate.addEventListener('click', async () => {
      const collection = getCollection(PRIMARY_COLL);
      if (!collection) return;
      const newId = 'rec-' + Date.now();
      const record = {
        id: newId,
        title: 'Neuer Eintrag',
        status: 'Entwurf',
        updated_at_ms: Date.now(),
        data: {}
      };
      await collection.insert({
        ...record
      });
      state.records = [record, ...state.records.filter(item => item.id !== newId)];
      selectRecord(newId);
      notify('Eintrag erstellt', 'Ein neuer Datensatz wurde erfolgreich angelegt.');
    });
  }

  if (btnSave) {
    btnSave.addEventListener('click', async () => {
      const collection = getCollection(PRIMARY_COLL);
      if (!state.selectedId || !collection) return;
      const inputTitle = host.querySelector('#record-detail-title');
      const selectStatus = host.querySelector('#record-detail-status');
      const doc = await collection.findOne(state.selectedId).exec();
      if (!doc) return;
      await doc.patch({
        title: inputTitle?.value || 'Unbenannt',
        status: selectStatus?.value || 'Entwurf',
        updated_at_ms: Date.now()
      });
      await loadInitialData();
      const updatedRecord = state.records.find(item => item.id === state.selectedId);
      if (updatedRecord) showDetail(updatedRecord);
      notify('Gespeichert', 'Die Änderungen wurden erfolgreich synchronisiert.');
    });
  }

  if (btnReview) {
    btnReview.addEventListener('click', async () => {
      const record = state.records.find(item => item.id === state.selectedId);
      if (!record || !state.ctx.commandBus?.dispatch) return;
      const command = buildFollowUpCommand(record);
      await state.ctx.commandBus.dispatch({
        ...command,
        type: 'business_os.chat.task',
        command_type: 'business_os.chat.task',
        payload: {
          ...command.payload,
          record_snapshot: record
        }
      });
      notify('Review angefordert', 'Der CTOX Chat hat eine Follow-up Aufgabe erhalten.');
    });
  }
}
`;
}

async function triggerAppDeployment(host, updateCreatorActionState = () => {}) {
  const syncDot = host.querySelector('#deploy-sync-dot');
  const syncText = host.querySelector('#deploy-sync-text');
  const btnDeploy = host.querySelector('#btn-deploy-app');

  const appId = state.appId;
  const appTitle = state.appTitle;
  const appDesc = state.appDesc;
  const collections = state.appCollections;
  const appLayout = state.appLayout;
  const appVersion = /^\d+\.\d+\.\d+$/.test(String(state.appVersion || '').trim())
    ? String(state.appVersion).trim()
    : '0.1.0';
  const appCollectionVersion = `v${appVersion.replace(/\./g, '_')}`;
  const versionedCollections = collections.map(coll => `${coll}_${appCollectionVersion}`);
  const moduleCollections = Array.from(new Set(['business_commands', ...versionedCollections]));

  if (!appId || !appTitle || !appDesc) {
    state.ctx.notifications.show({
      title: 'Fehler beim Generieren',
      message: 'Bitte fülle alle Pflichtfelder (Modul ID, Titel, Beschreibung) aus.',
      type: 'error'
    });
    addConsoleLog('[FEHLER] Spezifikation unvollständig. ID, Titel und Beschreibung sind erforderlich.', 'error');
    return;
  }

  // Visual lock UI
  state.isDeploying = true;
  btnDeploy.disabled = true;
  syncDot.className = 'sync-dot is-saving';
  syncText.textContent = state.t('deploySaving', 'Speichere Modul...');
  updateCreatorActionState();

  // Visual delay logs for the static module generator.
  addConsoleLog('==================================================', 'info');
  addConsoleLog(`[START] Erzeuge statische Business OS Moduldateien für '${appTitle}' (${appId})...`, 'info');

  await new Promise(r => setTimeout(r, 400));
  addConsoleLog(`[1/3] Generiere module.json manifest für layout.shell: '${appLayout}'...`, 'info');

  await new Promise(r => setTimeout(r, 300));
  addConsoleLog(`[2/3] Bereite RxDB Schema Definition für [${versionedCollections.join(', ')}] vor...`, 'info');

  await new Promise(r => setTimeout(r, 300));
  addConsoleLog('[3/3] Schreibe Vanilla HTML/CSS/browser-ESM Dateien ohne Build-Schritt...', 'info');

  try {
    const actorContext = (session) => {
      const user = session?.user || {};
      return {
        id: user.id || 'admin',
        display_name: user.display_name || user.name || 'Admin',
        role: user.role || 'admin',
        is_admin: user.is_admin !== undefined ? Boolean(user.is_admin) : true,
      };
    };

    const clientContext = {
      source: 'business-os-creator',
      actor: actorContext(state.ctx.session),
    };

    // 1. Scaffold the module under installed-modules/<appId>
    addConsoleLog(`[WRITE] Sende ctox.module.save Befehl für ${appId}...`, 'info');
    await state.ctx.commandBus.dispatch({
      command_id: `save-module-${Date.now()}`,
      module: 'creator',
      type: 'ctox.module.save',
      command_type: 'ctox.module.save',
      payload: {
        id: appId,
        title: appTitle,
        description: appDesc,
        version: appVersion,
        entry: `installed-modules/${appId}/index.html`,
        install_scope: 'installed',
        collections: moduleCollections,
        layout: {
          shell: appLayout,
          left: `${appTitle} Navigation`,
          center: `${appTitle} Workbench`
        }
      },
      client_context: clientContext
    });

    await new Promise(r => setTimeout(r, 600));

    // 2. Loop through generated templates and dispatch ctox.source.save
    const filesToSave = [
      'module.json',
      'collections.schema.json',
      'schema.js',
      'core/schemas.mjs',
      'core/automation.mjs',
      'index.html',
      'index.css',
      'index.js',
      'icon.svg',
      'locales/de.json',
      'locales/en.json',
      `tests/${appId}.test.mjs`
    ];
    for (const file of filesToSave) {
      addConsoleLog(`[WRITE] Schreibe Datei: installed-modules/${appId}/${file}...`, 'info');
      await state.ctx.commandBus.dispatch({
        command_id: `save-source-${file}-${Date.now()}`,
        module: 'creator',
        type: 'ctox.source.save',
        command_type: 'ctox.source.save',
        payload: {
          module_id: appId,
          path: file,
          content: state.generatedFiles[file]
        },
        client_context: clientContext
      });
      await new Promise(r => setTimeout(r, 150));
    }

    addConsoleLog('==================================================', 'success');
    addConsoleLog(`[SUCCESS] Modul '${appTitle}' wurde erfolgreich generiert und im System installiert!`, 'success');
    addConsoleLog(`[SUCCESS] Die Dateien befinden sich unter: installed-modules/${appId}/`, 'success');
    addConsoleLog('[INFO] Lade Workspace neu um die Änderungen anzuwenden...', 'info');

    state.ctx.notifications.show({
      title: 'Modul installiert',
      message: `Das Modul '${appTitle}' wurde erfolgreich generiert und geladen!`,
      type: 'success'
    });

    syncDot.className = 'sync-dot';
    syncText.textContent = state.t('deployInstalled', 'Erfolgreich installiert');

    // Reload the app catalog in the background so it shows up in desktop
    setTimeout(() => {
      window.location.reload();
    }, 1500);

  } catch (error) {
    addConsoleLog(`[FEHLER] Fehler bei der Code-Generierung: ${error.message}`, 'error');
    console.error(error);

    state.ctx.notifications.show({
      title: 'Fehler bei der Installation',
      message: `Das Modul konnte nicht vollständig registriert werden: ${error.message}`,
      type: 'error'
    });

    syncDot.className = 'sync-dot';
    syncDot.style.background = 'var(--danger)';
    syncText.textContent = state.t('deployFailed', 'Fehler beim Speichern');
    state.isDeploying = false;
    updateCreatorActionState();
  }
}

function initCreatorContextMenu(state) {
  state.contextMenu?.remove();
  const menu = document.createElement('div');
  menu.className = 'ctox-context-menu creator-context-menu';
  menu.hidden = true;
  document.body.append(menu);
  state.contextMenu = menu;

  const handleContextMenu = (event) => {
    if (state.ctx.module?.id !== 'creator') return;
    const context = creatorCommandContextFromElement(state, event.target);
    event.preventDefault();
    event.stopPropagation();
    renderCreatorContextMenu(state, context, event.clientX, event.clientY);
  };
  const handleOutsideClick = (event) => {
    if (state.contextMenu?.contains(event.target)) return;
    hideCreatorContextMenu(state);
  };
  const handleEscape = (event) => {
    if (event.key === 'Escape') hideCreatorContextMenu(state);
  };

  state.ctx.host.addEventListener('contextmenu', handleContextMenu);
  window.addEventListener('click', handleOutsideClick, { capture: true });
  window.addEventListener('keydown', handleEscape);

  return () => {
    state.ctx.host.removeEventListener('contextmenu', handleContextMenu);
    window.removeEventListener('click', handleOutsideClick, { capture: true });
    window.removeEventListener('keydown', handleEscape);
    hideCreatorContextMenu(state);
  };
}

function hideCreatorContextMenu(state) {
  if (state.contextMenu) state.contextMenu.hidden = true;
}

function canModifyCreatorApp(state) {
  if (typeof state.ctx.canModifyModule === 'function' && state.ctx.canModifyModule()) return true;
  const user = state.ctx.session?.user || {};
  const role = String(user.role || (user.is_admin ? 'admin' : 'user')).trim().toLowerCase().replace(/^business_os_/, '');
  return ['admin', 'chef'].includes(role);
}

function creatorCommandContextFromElement(state, target) {
  const element = target?.nodeType === Node.ELEMENT_NODE ? target : target?.parentElement;

  return {
    module: 'creator',
    column: 'workspace',
    record_type: 'app-spec',
    record_id: state.appId || 'creator',
    label: state.appTitle || 'Creator App Spec',
    app_id: state.appId || '',
    app_title: state.appTitle || '',
    app_desc: state.appDesc || '',
    app_category: state.appCategory || '',
    app_layout: state.appLayout || '',
    app_collections: state.appCollections || [],
    selected_text: String(window.getSelection?.()?.toString?.() || '').trim().slice(0, 1000),
    clicked_text: String(element?.innerText || element?.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 500),
  };
}

function renderCreatorContextMenu(state, context, x, y) {
  ensureCtoxContextMenuStyles();
  const canModifyApp = canModifyCreatorApp(state);
  state.contextMenu.innerHTML = `
    <form class="creator-context-chat" data-creator-context-chat-form>
      <header>
        <div>
          <strong>Chat to CTOX</strong>
          <span>${escapeHtml(context.label || 'Creator')}</span>
        </div>
        <button type="button" data-creator-context-close aria-label="Schließen">×</button>
      </header>
      ${canModifyApp ? `
        <div class="ctox-context-mode" role="radiogroup" aria-label="CTOX Aufgabe">
          <label><input type="radio" name="contextMode" value="data" checked /> Mit Daten arbeiten</label>
          <label><input type="radio" name="contextMode" value="app" /> App modifizieren</label>
        </div>
      ` : ''}
      <textarea data-creator-context-message placeholder="Was soll CTOX mit dieser App-Spezifikation tun?"></textarea>
      <footer>
        <span data-creator-context-status></span>
        <button type="submit">Senden</button>
      </footer>
    </form>
  `;
  state.contextMenu.hidden = false;
  state.contextMenu.style.left = '0px';
  state.contextMenu.style.top = '0px';
  const rect = state.contextMenu.getBoundingClientRect();
  const clampNumber = (val, min, max) => Math.min(max, Math.max(min, val));
  const maxLeft = Math.max(8, window.innerWidth - rect.width - 8);
  const maxTop = Math.max(8, window.innerHeight - rect.height - 8);
  state.contextMenu.style.left = `${clampNumber(x, 8, maxLeft)}px`;
  state.contextMenu.style.top = `${clampNumber(y, 8, maxTop)}px`;

  const form = state.contextMenu.querySelector('[data-creator-context-chat-form]');
  const textarea = state.contextMenu.querySelector('[data-creator-context-message]');
  state.contextMenu.querySelector('[data-creator-context-close]')?.addEventListener('click', () => hideCreatorContextMenu(state));
  form?.addEventListener('submit', async (event) => {
    event.preventDefault();
    const mode = canModifyApp ? (new FormData(form).get('contextMode') || 'data') : 'data';
    await dispatchCreatorContextChat(state, context, textarea?.value || '', mode);
  });
  requestAnimationFrame(() => textarea?.focus());
}

async function dispatchCreatorContextChat(state, context, message, mode = 'data') {
  const trimmed = String(message || '').trim();
  const status = state.contextMenu?.querySelector('[data-creator-context-status]');
  if (!trimmed) {
    if (status) status.textContent = 'Nachricht fehlt.';
    return;
  }

  const safeMode = mode === 'app' && canModifyCreatorApp(state) ? 'app' : 'data';
  if (!document.querySelector('[data-ctox-chat-root]')) {
    if (status) status.textContent = 'Chat ist noch nicht bereit.';
    return;
  }
  if (status) status.textContent = 'Oeffne Chat...';
  const title = `${safeMode === 'app' ? 'Creator App modifizieren' : 'App-Spezifikation anpassen'} · ${context.label || 'Creator'}`;
  const instruction = safeMode === 'app'
    ? `Modifiziere die App-Creator-App anhand dieser Admin-Anweisung. Kontext nur als UI-Bezug verwenden, App-Spezifikationen selbst nicht als primäres Ziel verändern.\n\n${trimmed}`
    : trimmed;

  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', {
    detail: {
      text: trimmed,
      module: 'creator',
      source_title: 'App Creator',
      command_type: safeMode === 'app' ? 'ctox.business_os.app.modify' : 'business_os.chat.task',
      record_id: safeMode === 'app' ? 'creator' : (context.record_id || 'creator'),
      title,
      instruction,
      payload: {
        title,
        instruction,
        prompt: trimmed,
        user_message: trimmed,
        mode: safeMode,
        target: safeMode === 'app' ? 'app' : 'data',
        context,
        thread_key: 'business-os/creator',
      },
      client_context: {
        action: 'context-chat',
        mode: safeMode,
        column: context.column,
        record_type: context.record_type,
        app_id: context.app_id,
        app_title: context.app_title,
      },
    },
  }));
  hideCreatorContextMenu(state);
}

function ensureCtoxContextMenuStyles() {
  if (document.getElementById('ctox-unified-context-menu-style')) return;
  const style = document.createElement('style');
  style.id = 'ctox-unified-context-menu-style';
  style.textContent = `
    .ctox-context-menu {
      position: absolute;
      z-index: 2400;
      width: min(560px, calc(100vw - 24px));
      max-width: calc(100% - 16px);
      overflow: hidden;
      border: 1px solid var(--bo-border, var(--border, #d8e1e5));
      border-radius: var(--radius-panel, 12px);
      background: color-mix(in srgb, var(--bo-surface, var(--surface, #fff)) 75%, transparent);
      backdrop-filter: blur(16px);
      -webkit-backdrop-filter: blur(16px);
      box-shadow: 0 18px 50px rgba(0, 0, 0, 0.25);
      padding: 6px;
      font-family: system-ui, -apple-system, sans-serif;
      animation: ctox-menu-fade-in 0.15s ease-out;
    }
    @keyframes ctox-menu-fade-in {
      from { opacity: 0; transform: scale(0.97); }
      to { opacity: 1; transform: scale(1); }
    }
    .ctox-context-menu form {
      display: grid;
      grid-template-columns: minmax(0, 1fr);
      gap: 10px;
      min-width: 0;
      padding: 12px;
    }
    .ctox-context-menu header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      border-bottom: 1px solid var(--bo-border, var(--border, #e5e5ea));
      padding-bottom: 10px;
    }
    .ctox-context-menu header strong {
      font-size: 14px;
      color: var(--bo-text, var(--text, #1c1c1e));
    }
    .ctox-context-menu header span {
      display: block;
      font-size: 11px;
      color: var(--bo-text-muted, var(--text-muted, #8e8e93));
      margin-top: 2px;
    }
    .ctox-context-menu button[type="button"] {
      border: none;
      background: transparent;
      color: var(--bo-text-muted, var(--text-muted, #8e8e93));
      cursor: pointer;
      font-size: 20px;
      line-height: 1;
      padding: 4px 8px;
    }
    .ctox-context-menu .ctox-context-mode {
      display: flex;
      gap: 16px;
      background: var(--bo-surface-2, var(--surface-2, #f2f2f7));
      border-radius: 8px;
      padding: 8px 12px;
    }
    .ctox-context-menu .ctox-context-mode label {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      font-size: 12px;
      font-weight: 500;
      color: var(--bo-text, var(--text, #1c1c1e));
      cursor: pointer;
    }
    .ctox-context-menu textarea {
      width: 100%;
      height: 90px;
      border: 1px solid var(--bo-border, var(--border, #d8e1e5));
      border-radius: 8px;
      background: var(--bo-surface-3, var(--surface-3, #fff));
      color: var(--bo-text, var(--text, #000));
      padding: 8px 12px;
      font-size: 13px;
      font-family: inherit;
      resize: vertical;
    }
    .ctox-context-menu textarea:focus {
      outline: none;
      border-color: var(--bo-accent, var(--accent, #e5a93c));
    }
    .ctox-context-menu footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      border-top: 1px solid var(--bo-border, var(--border, #e5e5ea));
      padding-top: 10px;
    }
    .ctox-context-menu footer span {
      font-size: 12px;
      color: var(--bo-accent, var(--accent, #e5a93c));
    }
    .ctox-context-menu footer button[type="submit"] {
      border: none;
      border-radius: 6px;
      background: var(--bo-accent-gradient, var(--accent-gradient, #e5a93c));
      color: #fff;
      font-size: 13px;
      font-weight: 600;
      padding: 6px 16px;
      cursor: pointer;
    }
  `;
  document.head.append(style);
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}


// --- Creator module i18n -----------------------------------------------------
// Loads locales/<lang>.json for the creator UI itself (the generator templates
// carry their own labels). German markup text is the fallback.
async function loadCreatorMessages(locale) {
  const lang = locale === 'en' ? 'en' : 'de';
  try {
    const response = await fetch(new URL(`./locales/${lang}.json`, import.meta.url));
    if (!response.ok) throw new Error(String(response.status));
    return await response.json();
  } catch {
    return {};
  }
}

function applyCreatorTranslations(root, t) {
  root.querySelectorAll('[data-t]').forEach((el) => {
    el.textContent = t(el.dataset.t, el.textContent.trim());
  });
  root.querySelectorAll('[data-t-placeholder]').forEach((el) => {
    el.setAttribute('placeholder', t(el.dataset.tPlaceholder, el.getAttribute('placeholder') || ''));
  });
  root.querySelectorAll('[data-t-title]').forEach((el) => {
    el.setAttribute('title', t(el.dataset.tTitle, el.getAttribute('title') || ''));
  });
  root.querySelectorAll('[data-t-aria]').forEach((el) => {
    el.setAttribute('aria-label', t(el.dataset.tAria, el.getAttribute('aria-label') || ''));
  });
}
