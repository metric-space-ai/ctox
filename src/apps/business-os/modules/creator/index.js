import { CtoxResizer } from '../../shared/resizer.js';

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
  appId: 'lagerverwaltung',
  appTitle: 'Lagerverwaltung',
  appDesc: 'Echtzeit-Lagerverwaltung und Bestandsüberwachung mit synchronisierten Artikeltabellen.',
  appCategory: 'Management',
  appLayout: 'full-workspace',
  appCollections: ['inventory_records', 'inventory_transactions'],
  appVersion: 'v1',
  generatedFiles: {},
  contextMenu: null,
  contextMenuCleanup: null,
  resizerCleanup: null
};

export async function mount(ctx) {
  state.ctx = ctx;

  // 1. Inject module scoped stylesheet dynamically
  await ensureStyles();

  // 2. Fetch and render raw index.html structure
  const html = await fetch(new URL('./index.html', import.meta.url)).then(res => res.text());
  ctx.host.innerHTML = html;

  // 3. Wire UI events & presets loading
  wireUi(ctx.host);

  // 4. Generate starting files
  generateAllFiles();

  // 5. Initialize CTOX unified context menu
  state.contextMenuCleanup = initCreatorContextMenu(state);

  // 6. Setup column resizer
  state.resizerCleanup = setupResizers(ctx.host);

  return () => {
    state.contextMenuCleanup?.();
    state.resizerCleanup?.();
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
  const containerEl = host.querySelector('[data-creator-root]') || host;
  const resizerEl = host.querySelector('[data-resizer="left"]');
  if (!resizerEl) return () => {};

  const cssVar = '--creator-left-width';
  const storageKey = 'ctox.creator.layout.leftWidth';

  // Read saved width
  const savedWidth = localStorage.getItem(storageKey);
  if (savedWidth) {
    containerEl.style.setProperty(cssVar, `${savedWidth}px`);
  }

  const resizer = new CtoxResizer({
    resizerEl,
    containerEl,
    cssVar,
    side: 'left',
    minWidth: 260,
    maxWidth: 550,
    onResize: (width) => {
      localStorage.setItem(storageKey, width);
    }
  });

  return () => {
    resizer.destroy();
  };
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

  // Accordion Expand/Collapse Trigger
  const accordionTrigger = host.querySelector('#expert-accordion-btn');
  const accordionContent = host.querySelector('#expert-accordion-content');
  const accordionChevron = host.querySelector('.accordion-chevron');
  accordionTrigger.addEventListener('click', () => {
    const isCollapsed = accordionContent.classList.contains('is-collapsed');
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
    inputId.value = preset.id;
    inputTitle.value = preset.title;
    inputDesc.value = preset.desc;
    selectCategory.value = preset.category;
    selectLayout.value = preset.layout;
    state.appCollections = [...preset.collections];

    renderCollectionsList(host);
    syncStateFromInputs();

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

    btnApplyPrompt.disabled = true;
    btnApplyPrompt.innerHTML = `
      <svg class="animate-spin" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" style="animation: pulse-sync 1s infinite alternate; margin-right: 6px;"><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/></svg>
      KI analysiert Prompt...
    `;

    addConsoleLog('==================================================', 'info');
    addConsoleLog('[KI-OPERATOR] Analysiere Anwendungsbeschreibung...', 'info');
    await new Promise(r => setTimeout(r, 600));

    const lowerPrompt = prompt.toLowerCase();
    let guessedTitle = 'Spezialanwendung';
    let guessedId = 'spezialapp';
    let guessedDesc = prompt;
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
      const cleanPromptStr = prompt.replace(/[^a-zA-Z0-9\s]/g, '').trim();
      const words = cleanPromptStr.split(/\s+/).filter(w => w.length > 3);
      if (words.length > 0) {
        guessedTitle = words[0].charAt(0).toUpperCase() + words[0].slice(1);
        if (words[1]) guessedTitle += ' ' + words[1].charAt(0).toUpperCase() + words[1].slice(1);
        guessedId = guessedTitle.toLowerCase().replace(/\s+/g, '-').replace(/[^a-z0-9\-]/g, '');
      }
      guessedDesc = prompt.length > 100 ? prompt.substring(0, 97) + '...' : prompt;
      guessedCollections = [guessedId + '_records', guessedId + '_history'];
    }

    addConsoleLog(`[KI-OPERATOR] Erkenne Domäne & Absicht: ${guessedCategory}`, 'info');
    await new Promise(r => setTimeout(r, 300));
    addConsoleLog(`[KI-OPERATOR] Bestimme Layout-Struktur: ${guessedLayout === 'pane' ? 'Spalten-Tracker' : 'Tabellen-Workspace'}`, 'info');
    addConsoleLog(`[KI-OPERATOR] Generiere RxDB Collections: [${guessedCollections.join(', ')}]`, 'info');

    // Update inputs
    inputId.value = guessedId;
    inputTitle.value = guessedTitle;
    inputDesc.value = guessedDesc;
    selectCategory.value = guessedCategory;
    selectLayout.value = guessedLayout;
    state.appCollections = [...guessedCollections];

    renderCollectionsList(host);
    syncStateFromInputs();

    addConsoleLog(`[SUCCESS] Spezifikation für '${guessedTitle}' erfolgreich generiert!`, 'success');
    addConsoleLog('==================================================', 'success');

    state.ctx.notifications.show({
      title: 'Spezifikation optimiert',
      message: `Die App-Spezifikation für '${guessedTitle}' wurde per KI optimiert.`,
      type: 'success'
    });

    btnApplyPrompt.disabled = false;
    btnApplyPrompt.innerHTML = `
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" style="margin-right: 6px;"><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/></svg>
      Spezifikation optimieren & anwenden
    `;
  });

  const syncStateFromInputs = () => {
    state.appId = inputId.value.trim().toLowerCase().replace(/[^a-z0-9\-]/g, '');
    state.appTitle = inputTitle.value.trim();
    state.appDesc = inputDesc.value.trim();
    state.appCategory = selectCategory.value;
    state.appLayout = selectLayout.value;

    generateAllFiles();
  };

  // Text inputs changed manually inside the expandable accordion
  [inputId, inputTitle, inputDesc, selectCategory, selectLayout].forEach(el => {
    el.addEventListener('input', syncStateFromInputs);
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
        <button type="button" class="os-icon-btn is-danger" data-remove-idx="${idx}" title="Löschen" style="width: 24px; height: 24px; font-size: 11px;">×</button>
      `;
      row.querySelector('[data-remove-idx]').addEventListener('click', (e) => {
        const removeIdx = parseInt(e.currentTarget.getAttribute('data-remove-idx'), 10);
        state.appCollections.splice(removeIdx, 1);
        renderCollectionsList(h);
        syncStateFromInputs();
      });
      listEl.appendChild(row);
    });
  };

  btnAddColl.addEventListener('click', () => {
    const newName = inputNewColl.value.trim().toLowerCase().replace(/[^a-z0-9_]/g, '');
    if (!newName) return;
    if (state.appCollections.includes(newName)) {
      addConsoleLog(`[WARN] Collection '${newName}' existiert bereits.`, 'warning');
      return;
    }
    state.appCollections.push(newName);
    inputNewColl.value = '';
    renderCollectionsList(host);
    syncStateFromInputs();
    addConsoleLog(`[INFO] Collection '${newName}' hinzugefügt.`, 'info');
  });

  renderCollectionsList(host);

  // Install / Deploy Button
  btnDeploy.addEventListener('click', async () => {
    try {
      const currentPrompt = inputPrompt.value.trim();
      if (currentPrompt && !currentPrompt.startsWith('Upgrade für') && (state.appTitle === 'Lagerverwaltung' && state.appId === 'lagerverwaltung')) {
        // Run a quick silent sync
        const lowerPrompt = currentPrompt.toLowerCase();
        if (lowerPrompt.includes('pflanze') || lowerPrompt.includes('blume') || lowerPrompt.includes('garten')) {
          inputId.value = 'pflanzen-tracker';
          inputTitle.value = 'Pflanzen-Tracker';
          state.appCollections = ['plants', 'watering_logs'];
        } else if (lowerPrompt.includes('zeit') || lowerPrompt.includes('stunde')) {
          inputId.value = 'zeiterfassung';
          inputTitle.value = 'Zeiterfassung';
          state.appCollections = ['time_logs', 'projects'];
        }
        syncStateFromInputs();
      }

      await triggerAppDeployment(host);
    } catch (e) {
      console.error('[ERROR] triggerAppDeployment failed:', e);
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

        addConsoleLog(`[INFO] Upgrade-Version erkannt: ${currentVer} -> ${nextVer}. Suffixe aus Collections entfernt.`, 'info');

        renderCollectionsList(host);
        syncStateFromInputs();

        addConsoleLog(`[SUCCESS] Spezifikation für '${manifest.title || upgradeAppId}' erfolgreich geladen. Passen Sie die Prompt-Eingabe an und starten Sie das Deployment!`, 'success');
      } catch (err) {
        addConsoleLog(`[ERROR] Fehler beim Laden des Upgrades: ${err.message}`, 'error');
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
  const appVersion = state.appVersion || 'v1';
  const versionedCollections = collections.map(coll => `${coll}_${appVersion}`);

  const iconSvg = generateSvgLogo(appId, appCategory);
  state.generatedFiles['icon.svg'] = iconSvg;

  // 1. module.json
  state.generatedFiles['module.json'] = JSON.stringify({
    id: appId,
    title: appTitle,
    description: appDesc,
    entry: `installed-modules/${appId}/index.html`,
    collections: versionedCollections,
    layout: {
      shell: appLayout,
      left: `${appTitle} Navigation`,
      center: `${appTitle} Workbench`,
      right: 'AI Operator Queue',
      icon_svg: iconSvg
    },
    category: appCategory,
    version: appVersion,
    developer: 'CTOX Developer App',
    license: 'AGPL-3.0-only',
    tags: [appId, 'installed-module', appCategory.toLowerCase()]
  }, null, 2);

  // 2. schema.js
  let colSchemaProps = '';
  collections.forEach(coll => {
    const versionedColl = `${coll}_${appVersion}`;
    colSchemaProps += `  ${versionedColl}: {\n    schema: {\n      title: '${versionedColl} schema',\n      version: 0,\n      primaryKey: 'id',\n      type: 'object',\n      properties: {\n        id: { type: 'string', maxLength: 100 },\n        title: { type: 'string' },\n        status: { type: 'string' },\n        updated_at_ms: { type: 'number' },\n        data: { type: 'object', additionalProperties: true }\n      },\n      required: ['id', 'title', 'status', 'updated_at_ms']\n    }\n  },\n`;
  });

  state.generatedFiles['schema.js'] = `export const collections = {\n${colSchemaProps.trim().substring(0, colSchemaProps.trim().length - 1)}\n};\n`;

  // 3. index.html
  if (appLayout === 'full-workspace') {
    state.generatedFiles['index.html'] = `<div class="module-root" data-module-root="${appId}">\n  <div class="${appId}-layout">\n    <!-- Links: Listpane -->\n    <div class="${appId}-left">\n      <header class="pane-header">\n        <span class="os-kicker" data-t="backlog">Kategorie</span>\n        <h2 class="os-title" data-t="itemsTitle">${appTitle}</h2>\n      </header>\n      <div class="${appId}-scrollable os-scrollbar" data-list-container>\n        <!-- Listeneinträge werden dynamisch eingefügt -->\n      </div>\n    </div>\n    \n    <!-- Spalten-Resizer Handle -->\n    <div class="os-col-resizer" role="separator" aria-label="Breite anpassen" data-resizer="left"></div>\n\n    <!-- Mitte: Detailworkbench -->\n    <div class="${appId}-center">\n      <header class="pane-header" style="border-bottom: 1px solid var(--line); display: flex; align-items: center; justify-content: space-between;">\n        <div>\n          <span class="os-kicker">Arbeitsfläche</span>\n          <h2 class="os-title" id="selected-item-title">Kein Eintrag gewählt</h2>\n        </div>\n        <button type="button" class="os-btn is-primary" id="btn-create-record">Eintrag erstellen</button>\n      </header>\n      \n      <main class="${appId}-workbench os-scrollbar" style="flex: 1; padding: 20px; overflow-y: auto;">\n        <div id="empty-state" style="display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; color: var(--muted);">\n          <p>Wähle einen Datensatz links aus oder erstelle einen neuen.</p>\n        </div>\n        \n        <div id="detail-card" class="is-hidden" style="background: var(--surface-2); border: 1px solid var(--line); border-radius: var(--panel-radius); padding: 20px;">\n          <div class="form-group">\n            <label class="form-label">Titel des Eintrags</label>\n            <input type="text" id="record-detail-title" class="os-input">\n          </div>\n          <div class="form-group">\n            <label class="form-label">Status</label>\n            <select id="record-detail-status" class="os-select">\n              <option value="Aktiv">Aktiv</option>\n              <option value="Entwurf">Entwurf</option>\n              <option value="Archiviert">Archiviert</option>\n            </select>\n          </div>\n          <button type="button" class="os-btn is-accent" id="btn-save-record" style="margin-top: 12px;">Speichern</button>\n        </div>\n      </main>\n    </div>\n  </div>\n</div>\n`;
  } else {
    state.generatedFiles['index.html'] = `<div class="module-root" data-module-root="${appId}">\n  <!-- In pane layout, outer panels are shell-rendered, center is active active workbench -->\n  <div class="${appId}-center" style="display: flex; flex-direction: column; width: 100%; height: 100%;">\n    <header class="pane-header" style="border-bottom: 1px solid var(--line); display: flex; align-items: center; justify-content: space-between;">\n      <div>\n        <span class="os-kicker">Arbeitsbereich</span>\n        <h2 class="os-title">${appTitle} Workbench</h2>\n      </div>\n      <button type="button" class="os-btn is-primary" id="btn-create-record">Eintrag erstellen</button>\n    </header>\n    \n    <main class="${appId}-workbench os-scrollbar" style="flex: 1; padding: 20px; overflow-y: auto;">\n      <div id="empty-state" style="display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; color: var(--muted);">\n        <p>Nutze die linke Navigationsspalte zur Auswahl und Bearbeitung.</p>\n      </div>\n      \n      <div id="detail-card" class="is-hidden" style="background: var(--surface-2); border: 1px solid var(--line); border-radius: var(--panel-radius); padding: 20px;">\n        <div class="form-group">\n          <label class="form-label">Titel des Eintrags</label>\n          <input type="text" id="record-detail-title" class="os-input">\n        </div>\n        <div class="form-group">\n          <label class="form-label">Status</label>\n          <select id="record-detail-status" class="os-select">\n            <option value="Aktiv">Aktiv</option>\n            <option value="Entwurf">Entwurf</option>\n            <option value="Archiviert">Archiviert</option>\n          </select>\n        </div>\n        <button type="button" class="os-btn is-accent" id="btn-save-record" style="margin-top: 12px;">Eintrag speichern</button>\n      </div>\n    </main>\n  </div>\n</div>\n`;
  }

  // 4. index.css
  state.generatedFiles['index.css'] = `/* Stylesheet dynamic generator for module: ${appId} */
.module-root {
  display: flex;
  width: 100%;
  height: 100%;
  background: var(--bg-1);
  color: var(--text-main);
  font-family: var(--font-sans);
}

.${appId}-layout {
  display: flex;
  width: 100%;
  height: 100%;
}

.${appId}-left {
  width: 300px;
  flex: 0 0 300px;
  border-right: 1px solid var(--line);
  display: flex;
  flex-direction: column;
  background: var(--bg-2);
}

.${appId}-center {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: var(--bg-1);
}

.${appId}-scrollable {
  flex: 1;
  overflow-y: auto;
  padding: 12px;
}

.record-item-card {
  padding: 12px 14px;
  border-radius: var(--panel-radius);
  background: var(--surface-1);
  border: 1px solid var(--line);
  margin-bottom: 8px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.record-item-card:hover {
  background: var(--surface-2);
  border-color: var(--line-active);
}

.record-item-card.is-active {
  background: var(--surface-active);
  border-color: var(--accent);
}

.form-group {
  margin-bottom: 16px;
}

.form-label {
  display: block;
  font-size: 12px;
  color: var(--muted);
  margin-bottom: 6px;
  font-weight: 500;
}

.is-hidden {
  display: none !important;
}

/* Column resizer styling handled by global [data-resizer] in app.css */

@media (max-width: 768px) {
  .${appId}-layout {
    flex-direction: column;
  }
  .${appId}-left {
    display: none !important;
  }
  [data-resizer] {
    display: none !important;
  }
}
`;

  // 5. index.js
  state.generatedFiles['index.js'] = `import { loadModuleMessages } from '../../shared/i18n.js';

const labels = {
  de: {
    backlog: 'Datenkatalog',
    itemsTitle: 'Einträge',
    selectPrompt: 'Wähle ein Element aus, um Details anzuzeigen.'
  },
  en: {
    backlog: 'Data Catalog',
    itemsTitle: 'Items',
    selectPrompt: 'Select an item to view details.'
  }
};

const APP_METADATA = {
  version: '${appVersion}',
  collections: ${JSON.stringify(collections)}
};

const PRIMARY_COLL = \`${primaryColl}_\${APP_METADATA.version}\`;

const state = {
  ctx: null,
  t: (key, fallback) => fallback ?? key,
  records: [],
  selectedId: null,
  dbSubscription: null
};

export async function mount(ctx) {
  state.ctx = ctx;

  // 1. Inject stylesheet dynamically
  await ensureStyles();

  // 2. Fetch and render localization messages
  const messages = await loadModuleMessages(import.meta.url, ctx.locale, labels);
  state.t = (key, fallback) => messages[key] ?? fallback ?? key;

  // 3. Mount HTML template structure
  const html = await fetch(new URL('./index.html', import.meta.url)).then(res => res.text());
  ctx.host.innerHTML = html;

  // 4. Translate static tags
  applyTranslations(ctx.host, state.t);

  // 5. Wire Resizers if in full-workspace
  const cleanupResizers = setupResizers(ctx.host);

  // 6. Run client-side auto-migration
  try {
    await autoMigrate(ctx);
  } catch (err) {
    console.error('[Migration] Auto-migration failed:', err);
  }

  // 7. Setup dynamic db observation and sync
  await loadInitialData();
  state.dbSubscription = wireReactiveSync();

  // 8. Bind interactive clicks
  wireUi(ctx.host);

  return () => {
    if (typeof state.dbSubscription === 'function') {
      state.dbSubscription();
    } else if (state.dbSubscription && typeof state.dbSubscription.unsubscribe === 'function') {
      state.dbSubscription.unsubscribe();
    }
    cleanupResizers();
    console.log('[${appId}] Unmounted successfully.');
  };
}

async function autoMigrate(ctx) {
  const version = APP_METADATA.version;
  const versionNum = parseInt(version.replace('v', ''), 10) || 1;
  if (versionNum <= 1) return;

  console.log(\`[Migration] [\${APP_METADATA.version}] Auto-Migration wird initialisiert...\\n\`);
  for (const baseColl of APP_METADATA.collections) {
    const currentColl = \`\${baseColl}_\${version}\\n\`.trim();
    if (!ctx.db?.raw || !ctx.db.raw[currentColl]) continue;

    const currentCount = await ctx.db.raw[currentColl].find().exec().then(docs => docs.length).catch(() => 0);
    if (currentCount > 0) {
      console.log(\`[Migration] [\${currentColl}] Hat bereits Daten, keine Migration erforderlich.\\n\`);
      continue;
    }

    for (let i = versionNum - 1; i >= 1; i--) {
      const prevColl = \`\${baseColl}_v\${i}\\n\`.trim();
      console.log(\`[Migration] Überprüfe historische Tabelle \${prevColl}...\\n\`);

      if (!ctx.db.raw[prevColl]) {
        try {
          const currentSchema = ctx.db.raw[currentColl].schema.jsonSchema;
          const prevSchema = {
            ...currentSchema,
            title: \`\${prevColl} schema\`
          };
          await ctx.db.raw.addCollections({
            [prevColl]: { schema: prevSchema }
          });
        } catch (e) {
          console.error(\`[Migration] Fehler bei Registrierung von \${prevColl}:\\n\`, e);
          continue;
        }
      }

      const prevCount = await ctx.db.raw[prevColl].find().exec().then(docs => docs.length).catch(() => 0);
      if (prevCount > 0) {
        console.log(\`[Migration] Daten in \${prevColl} gefunden (\${prevCount} Einträge). Starte Migration...\\n\`);
        const oldItems = await ctx.db.raw[prevColl].find().exec();
        const docs = oldItems.map(item => {
          const json = item.toJSON();
          delete json._meta;
          delete json._deleted;
          return json;
        });

        await ctx.db.raw[currentColl].bulkInsert(docs);
        console.log(\`[Migration] Migration von \${prevColl} nach \${currentColl} erfolgreich beendet!\\n\`);
        break;
      }
    }
  }
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
  root.querySelectorAll('[data-t]').forEach(el => el.textContent = t(el.dataset.t));
}

function setupResizers(host) {
  const leftPane = host.querySelector('.${appId}-left');
  const resizer = host.querySelector('[data-resizer="left"]');
  if (!leftPane || !resizer) return () => {};

  let leftWidth = parseInt(localStorage.getItem('ctox.${appId}.leftWidth') || '300', 10);
  const applyWidth = () => {
    leftPane.style.width = \`\${leftWidth}px\`;
    leftPane.style.flex = \`0 0 \${leftWidth}px\`;
  };
  applyWidth();

  let activeDrag = false;
  let startX = 0;
  let startWidth = 0;

  const onPointerDown = (e) => {
    activeDrag = true;
    startX = e.clientX;
    startWidth = leftWidth;
    resizer.classList.add('is-dragging');
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    e.preventDefault();
  };

  const onPointerMove = (e) => {
    if (!activeDrag) return;
    const deltaX = e.clientX - startX;
    leftWidth = Math.min(550, Math.max(220, startWidth + deltaX));
    applyWidth();
  };

  const onPointerUp = () => {
    if (!activeDrag) return;
    activeDrag = false;
    resizer.classList.remove('is-dragging');
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
    localStorage.setItem('ctox.${appId}.leftWidth', leftWidth);
  };

  resizer.addEventListener('pointerdown', onPointerDown);
  window.addEventListener('pointermove', onPointerMove);
  window.addEventListener('pointerup', onPointerUp);

  return () => {
    resizer.removeEventListener('pointerdown', onPointerDown);
    window.removeEventListener('pointermove', onPointerMove);
    window.removeEventListener('pointerup', onPointerUp);
  };
}

async function loadInitialData() {
  if (!state.ctx.db?.raw?.[PRIMARY_COLL]) return;
  const items = await state.ctx.db.raw[PRIMARY_COLL].find().exec();
  state.records = items.map(item => item.toJSON());
  renderList();
}

function wireReactiveSync() {
  if (!state.ctx.db?.raw?.[PRIMARY_COLL]) return () => {};
  const sub = state.ctx.db.raw[PRIMARY_COLL].find().$.subscribe(items => {
    state.records = items.map(item => item.toJSON());
    renderList();
    if (state.selectedId) {
      const activeItem = state.records.find(r => r.id === state.selectedId);
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
    container.innerHTML = '<div style="color: var(--muted); font-size: 12px; text-align: center; margin-top: 20px;">Keine Einträge vorhanden</div>';
    return;
  }

  state.records.forEach(record => {
    const card = document.createElement('div');
    card.className = \`record-item-card \${state.selectedId === record.id ? 'is-active' : ''}\`;
    card.dataset.id = record.id;

    card.setAttribute('data-context-module', '${appId}');
    card.setAttribute('data-context-record-type', PRIMARY_COLL);
    card.setAttribute('data-context-record-id', record.id);
    card.setAttribute('data-context-label', record.title);

    card.innerHTML = \`
      <div style="font-weight: 600; font-size: 13px;">\${record.title}</div>
      <div style="font-size: 11px; color: var(--muted); margin-top: 4px; display: flex; justify-content: space-between;">
        <span>Status: \${record.status}</span>
        <span>\${new Date(record.updated_at_ms).toLocaleTimeString()}</span>
      </div>
    \`;
    card.addEventListener('click', () => selectRecord(record.id));
    container.appendChild(card);
  });
}

function selectRecord(id) {
  state.selectedId = id;
  const record = state.records.find(r => r.id === id);
  if (record) {
    showDetail(record);
    renderList();
  }
}

function showDetail(record) {
  const emptyState = state.ctx.host.querySelector('#empty-state');
  const detailCard = state.ctx.host.querySelector('#detail-card');
  const titleHeader = state.ctx.host.querySelector('#selected-item-title');

  if (emptyState) emptyState.classList.add('is-hidden');
  if (detailCard) detailCard.classList.remove('is-hidden');
  if (titleHeader) titleHeader.textContent = record.title;

  const inputTitle = state.ctx.host.querySelector('#record-detail-title');
  const selectStatus = state.ctx.host.querySelector('#record-detail-status');

  if (inputTitle) inputTitle.value = record.title;
  if (selectStatus) selectStatus.value = record.status;
}

function wireUi(host) {
  const btnCreate = host.querySelector('#btn-create-record') || state.ctx.left?.querySelector('#btn-create-record');
  const btnSave = host.querySelector('#btn-save-record');

  if (btnCreate) {
    btnCreate.addEventListener('click', async () => {
      if (!state.ctx.db?.raw?.[PRIMARY_COLL]) return;
      const newId = \`rec-\${Date.now()}\`;
      await state.ctx.db.raw[PRIMARY_COLL].insert({
        id: newId,
        title: 'Neuer Eintrag',
        status: 'Entwurf',
        updated_at_ms: Date.now(),
        data: {}
      });
      selectRecord(newId);
      state.ctx.notifications.show({
        title: 'Eintrag erstellt',
        message: 'Ein neuer Datensatz wurde erfolgreich angelegt.',
        type: 'success'
      });
    });
  }

  if (btnSave) {
    btnSave.addEventListener('click', async () => {
      if (!state.selectedId || !state.ctx.db?.raw?.[PRIMARY_COLL]) return;
      const inputTitle = host.querySelector('#record-detail-title');
      const selectStatus = host.querySelector('#record-detail-status');

      const doc = await state.ctx.db.raw[PRIMARY_COLL].findOne(state.selectedId).exec();
      if (doc) {
        await doc.patch({
          title: inputTitle.value || 'Unbenannt',
          status: selectStatus.value,
          updated_at_ms: Date.now()
        });
        state.ctx.notifications.show({
          title: 'Gespeichert',
          message: 'Die Änderungen wurden erfolgreich synchronisiert.',
          type: 'success'
        });
      }
    });
  }
}
`;
}

async function triggerAppDeployment(host) {
  const syncDot = host.querySelector('#deploy-sync-dot');
  const syncText = host.querySelector('#deploy-sync-text');
  const btnDeploy = host.querySelector('#btn-deploy-app');

  const appId = state.appId;
  const appTitle = state.appTitle;
  const appDesc = state.appDesc;
  const collections = state.appCollections;
  const appLayout = state.appLayout;
  const appVersion = state.appVersion || 'v1';
  const versionedCollections = collections.map(coll => `${coll}_${appVersion}`);

  if (!appId || !appTitle || !appDesc) {
    state.ctx.notifications.show({
      title: 'Fehler beim Generieren',
      message: 'Bitte fülle alle Pflichtfelder (Modul ID, Titel, Beschreibung) aus.',
      type: 'error'
    });
    addConsoleLog('[FEHLER] Spezifikation unvollständig! ID, Titel und Beschreibung sind erforderlich.', 'error');
    return;
  }

  // Visual lock UI
  btnDeploy.disabled = true;
  syncDot.className = 'sync-dot is-saving';
  syncText.textContent = 'Speichere Modul...';

  // Visual delay logs to mimic compiler
  addConsoleLog('==================================================', 'info');
  addConsoleLog(`[START] Kompiliere Modul-Spezifikation für '${appTitle}' (${appId})...`, 'info');

  await new Promise(r => setTimeout(r, 400));
  addConsoleLog(`[1/3] Generiere module.json manifest für layout.shell: '${appLayout}'...`, 'info');

  await new Promise(r => setTimeout(r, 300));
  addConsoleLog(`[2/3] Bereite RxDB Schema Definition für [${versionedCollections.join(', ')}] vor...`, 'info');

  await new Promise(r => setTimeout(r, 300));
  addConsoleLog(`[3/3] Kompiliere native ESM Modul-Controller index.js und index.css...`, 'info');

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
        entry: `installed-modules/${appId}/index.html`,
        collections: versionedCollections,
        layout: {
          shell: appLayout,
          left: `${appTitle} Navigation`,
          center: `${appTitle} Workbench`,
          right: 'AI Operator Queue',
          icon_svg: state.generatedFiles['icon.svg']
        }
      },
      client_context: clientContext
    });

    await new Promise(r => setTimeout(r, 600));

    // 2. Loop through generated templates and dispatch ctox.source.save
    const filesToSave = ['module.json', 'schema.js', 'index.html', 'index.css', 'index.js', 'icon.svg'];
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
    syncText.textContent = 'Erfolgreich installiert';

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
    syncText.textContent = 'Fehler beim Speichern';
    btnDeploy.disabled = false;
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
