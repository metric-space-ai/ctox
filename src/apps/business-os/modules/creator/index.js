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
  generatedFiles: {}
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

  return () => {
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
      // Run NLP simulation one last time just in case they adjusted prompt without clicking "apply"
      const currentPrompt = inputPrompt.value.trim();
      if (currentPrompt && (state.appTitle === 'Lagerverwaltung' && state.appId === 'lagerverwaltung')) {
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

        state.appCollections = Array.isArray(manifest.collections) ? [...manifest.collections] : ['records'];

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

  const iconSvg = generateSvgLogo(appId, appCategory);
  state.generatedFiles['icon.svg'] = iconSvg;

  // 1. module.json
  state.generatedFiles['module.json'] = JSON.stringify({
    id: appId,
    title: appTitle,
    description: appDesc,
    entry: `installed-modules/${appId}/index.html`,
    collections: collections,
    layout: {
      shell: appLayout,
      left: `${appTitle} Navigation`,
      center: `${appTitle} Workbench`,
      right: 'AI Operator Queue',
      icon_svg: iconSvg
    },
    category: appCategory,
    version: 'v1',
    developer: 'CTOX Developer App',
    license: 'Apache-2.0',
    tags: [appId, 'installed-module', appCategory.toLowerCase()]
  }, null, 2);

  // 2. schema.js
  let colSchemaProps = '';
  collections.forEach(coll => {
    colSchemaProps += `  ${coll}: {\n    schema: {\n      title: '${coll} schema',\n      version: 0,\n      primaryKey: 'id',\n      type: 'object',\n      properties: {\n        id: { type: 'string', maxLength: 100 },\n        title: { type: 'string' },\n        status: { type: 'string' },\n        updated_at_ms: { type: 'number' },\n        data: { type: 'object', additionalProperties: true }\n      },\n      required: ['id', 'title', 'status', 'updated_at_ms']\n    }\n  },\n`;
  });

  state.generatedFiles['schema.js'] = `export const collections = {\n${colSchemaProps.trim().substring(0, colSchemaProps.trim().length - 1)}\n};\n`;

  // 3. index.html
  if (appLayout === 'full-workspace') {
    state.generatedFiles['index.html'] = `<div class="module-root" data-module-root="${appId}">\n  <div class="${appId}-layout">\n    <!-- Links: Listpane -->\n    <div class="${appId}-left">\n      <header class="pane-header">\n        <span class="os-kicker" data-t="backlog">Kategorie</span>\n        <h2 class="os-title" data-t="itemsTitle">${appTitle}</h2>\n      </header>\n      <div class="${appId}-scrollable os-scrollbar" data-list-container>\n        <!-- Listeneinträge werden dynamisch eingefügt -->\n      </div>\n    </div>\n    \n    <!-- Spalten-Resizer Handle -->\n    <div class="os-col-resizer" role="separator" aria-label="Breite anpassen" data-resizer="left"></div>\n\n    <!-- Mitte: Detailworkbench -->\n    <div class="${appId}-center">\n      <header class="pane-header" style="border-bottom: 1px solid var(--line); display: flex; align-items: center; justify-content: space-between;">\n        <div>\n          <span class="os-kicker">Arbeitsfläche</span>\n          <h2 class="os-title" id="selected-item-title">Kein Eintrag gewählt</h2>\n        </div>\n        <button type="button" class="os-btn is-primary" id="btn-create-record">Eintrag erstellen</button>\n      </header>\n      \n      <main class="${appId}-workbench os-scrollbar" style="flex: 1; padding: 20px; overflow-y: auto;">\n        <div id="empty-state" style="display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; color: var(--muted);">\n          <p>Wähle einen Datensatz links aus oder erstelle einen neuen.</p>\n        </div>\n        \n        <div id="detail-card" class="is-hidden" style="background: var(--surface-2); border: 1px solid var(--line); border-radius: var(--panel-radius); padding: 20px;">\n          <div class="form-group">\n            <label class="form-label">Titel des Eintrags</label>\n            <input type="text" id="record-detail-title" class="os-input">\n          </div>\n          <div class="form-group">\n            <label class="form-label">Status</label>\n            <select id="record-detail-status" class="os-select">\n              <option value="Aktiv">Aktiv</option>\n              <option value="Entwurf">Entwurf</option>\n              <option value="Archiviert">Archiviert</option>\n            </select>\n          </div>\n          <button type="button" class="os-btn is-accent" id="btn-save-record" style="margin-top: 12px;">Speichern</button>\n        </div>\n      </main>\n    </div>\n  </div>\n</div>\n`;
  } else {
    state.generatedFiles['index.html'] = `<div class="module-root" data-module-root="${appId}">\n  <!-- In pane layout, outer panels are shell-rendered, center is active active workbench -->\n  <div class="${appId}-center" style="display: flex; flex-direction: column; width: 100%; height: 100%;">\n    <header class="pane-header" style="border-bottom: 1px solid var(--line); display: flex; align-items: center; justify-content: space-between;">\n      <div>\n        <span class="os-kicker">Arbeitsbereich</span>\n        <h2 class="os-title">${appTitle} Workbench</h2>\n      </div>\n      <button type="button" class="os-btn is-primary" id="btn-create-record">Eintrag erstellen</button>\n    </header>\n    \n    <main class="${appId}-workbench os-scrollbar" style="flex: 1; padding: 20px; overflow-y: auto;">\n      <div id="empty-state" style="display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; color: var(--muted);">\n        <p>Nutze die linke Navigationsspalte zur Auswahl und Bearbeitung.</p>\n      </div>\n      \n      <div id="detail-card" class="is-hidden" style="background: var(--surface-2); border: 1px solid var(--line); border-radius: var(--panel-radius); padding: 20px;">\n        <div class="form-group">\n          <label class="form-label">Titel des Eintrags</label>\n          <input type="text" id="record-detail-title" class="os-input">\n        </div>\n        <div class="form-group">\n          <label class="form-label">Status</label>\n          <select id="record-detail-status" class="os-select">\n            <option value="Aktiv">Aktiv</option>\n            <option value="Entwurf">Entwurf</option>\n            <option value="Archiviert">Archiviert</option>\n          </select>\n        </div>\n        <button type="button" class="os-btn is-accent" id="btn-save-record" style="margin-top: 12px;">Eintrag speichern</button>\n      </div>\n    </main>\n  </div>\n</div>\n`;
  }

  // 4. index.css
  state.generatedFiles['index.css'] = `.module-root[data-module-root="${appId}"] {\n  display: flex;\n  width: 100%;\n  height: 100%;\n  overflow: hidden;\n  background: var(--bg);\n  color: var(--text);\n}\n\n.${appId}-layout {\n  display: flex;\n  width: 100%;\n  height: 100%;\n  overflow: hidden;\n}\n\n.${appId}-left {\n  width: 300px;\n  flex: 0 0 300px;\n  background: color-mix(in srgb, var(--surface) 35%, transparent);\n  backdrop-filter: blur(20px) saturate(180%);\n  -webkit-backdrop-filter: blur(20px) saturate(180%);\n  border-right: 1px solid var(--line);\n  display: flex;\n  flex-direction: column;\n  height: 100%;\n  overflow: hidden;\n}\n\n.${appId}-center {\n  flex: 1;\n  display: flex;\n  flex-direction: column;\n  height: 100%;\n  overflow: hidden;\n}\n\n.${appId}-scrollable {\n  flex: 1;\n  overflow-y: auto;\n  padding: 16px;\n}\n\n.pane-header {\n  padding: 16px;\n  border-bottom: 1px solid var(--line);\n  flex-shrink: 0;\n}\n\n.os-kicker {\n  display: block;\n  color: var(--muted);\n  font-size: 11px;\n  font-weight: 780;\n  line-height: 1.1;\n  text-transform: uppercase;\n  letter-spacing: 0.05em;\n}\n\n.os-title {\n  margin: 3px 0 0;\n  font-size: 15px;\n  font-weight: 820;\n  line-height: 1.12;\n  font-family: var(--font-outfit);\n}\n\n.form-group {\n  margin-bottom: 16px;\n}\n\n.form-label {\n  display: block;\n  font-size: 11px;\n  font-weight: 600;\n  color: var(--muted);\n  text-transform: uppercase;\n  margin-bottom: 6px;\n}\n\n.os-input, .os-select {\n  width: 100%;\n  background: color-mix(in srgb, var(--surface-2) 60%, transparent);\n  border: 1px solid var(--line);\n  color: var(--text);\n  border-radius: var(--control-radius);\n  padding: 8px 12px;\n  font-size: 13px;\n}\n\n.os-btn {\n  display: inline-flex;\n  align-items: center;\n  justify-content: center;\n  font-weight: 600;\n  padding: 8px 16px;\n  border-radius: var(--control-radius);\n  border: 1px solid var(--line);\n  background: var(--surface-2);\n  color: var(--text);\n  cursor: pointer;\n  transition: var(--transition-bounce);\n}\n\n.os-btn:hover {\n  border-color: var(--accent);\n}\n\n.os-btn.is-primary {\n  background: var(--accent-gradient);\n  color: #ffffff;\n}\n\n.os-btn.is-accent {\n  background: var(--accent-soft);\n  border-color: var(--accent);\n  color: var(--text);\n}\n\n.record-item-card {\n  background: var(--surface);\n  border: 1px solid var(--line);\n  border-radius: var(--panel-radius);\n  padding: 12px;\n  margin-bottom: 8px;\n  cursor: pointer;\n  transition: var(--transition-bounce);\n}\n\n.record-item-card:hover {\n  transform: translateY(-1px);\n  border-color: var(--accent-soft);\n  box-shadow: var(--shadow-hover);\n}\n\n.record-item-card.is-active {\n  border-color: var(--accent);\n  background: var(--accent-soft);\n}\n\n.is-hidden {\n  display: none !important;\n}\n`;

  // 5. index.js
  state.generatedFiles['index.js'] = `import { loadModuleMessages } from '../../shared/i18n.js';\n\nconst labels = {\n  de: {\n    backlog: 'Datenkatalog',\n    itemsTitle: 'Einträge',\n    selectPrompt: 'Wähle ein Element aus, um Details anzuzeigen.'\n  },\n  en: {\n    backlog: 'Data Catalog',\n    itemsTitle: 'Items',\n    selectPrompt: 'Select an item to view details.'\n  }\n};\n\nconst state = {\n  ctx: null,\n  t: (key, fallback) => fallback ?? key,\n  records: [],\n  selectedId: null,\n  dbSubscription: null\n};\n\nexport async function mount(ctx) {\n  state.ctx = ctx;\n  \n  // 1. Inject stylesheet dynamically\n  await ensureStyles();\n\n  // 2. Fetch and render localization messages\n  const messages = await loadModuleMessages(import.meta.url, ctx.locale, labels);\n  state.t = (key, fallback) => messages[key] ?? fallback ?? key;\n\n  // 3. Mount HTML template structure\n  const html = await fetch(new URL('./index.html', import.meta.url)).then(res => res.text());\n  ctx.host.innerHTML = html;\n\n  // 4. Translate static tags\n  applyTranslations(ctx.host, state.t);\n\n  // 5. Wire Resizers if in full-workspace\n  const cleanupResizers = setupResizers(ctx.host);\n\n  // 6. Setup dynamic db observation and sync\n  await loadInitialData();\n  state.dbSubscription = wireReactiveSync();\n\n  // 7. Bind interactive clicks\n  wireUi(ctx.host);\n\n  return () => {\n    state.dbSubscription?.unsubscribe?.();\n    state.dbSubscription?.();\n    cleanupResizers();\n    console.log('[${appId}] Unmounted successfully.');\n  };\n}\n\nasync function ensureStyles() {\n  if (document.querySelector('link[data-module-styles="${appId} text"]')) return;\n  const link = document.createElement('link');\n  link.rel = 'stylesheet';\n  link.href = new URL('./index.css', import.meta.url).href;\n  link.dataset.moduleStyles = '${appId}';\n  document.head.append(link);\n}\n\nfunction applyTranslations(root, t) {\n  root.querySelectorAll('[data-t]').forEach(el => el.textContent = t(el.dataset.t));\n}\n\nfunction setupResizers(host) {\n  const leftPane = host.querySelector('.${appId}-left');\n  const resizer = host.querySelector('[data-resizer="left"]');\n  if (!leftPane || !resizer) return () => {};\n\n  let leftWidth = parseInt(localStorage.getItem('ctox.${appId}.leftWidth') || '300', 10);\n  const applyWidth = () => {\n    leftPane.style.width = \`\${leftWidth}px\`;\n    leftPane.style.flex = \`0 0 \${leftWidth}px\`;\n  };\n  applyWidth();\n\n  let activeDrag = false;\n  let startX = 0;\n  let startWidth = 0;\n\n  const onPointerDown = (e) => {\n    activeDrag = true;\n    startX = e.clientX;\n    startWidth = leftWidth;\n    resizer.classList.add('is-dragging');\n    document.body.style.cursor = 'col-resize';\n    document.body.style.userSelect = 'none';\n    e.preventDefault();\n  };\n\n  const onPointerMove = (e) => {\n    if (!activeDrag) return;\n    const deltaX = e.clientX - startX;\n    leftWidth = Math.min(550, Math.max(220, startWidth + deltaX));\n    applyWidth();\n  };\n\n  const onPointerUp = () => {\n    if (!activeDrag) return;\n    activeDrag = false;\n    resizer.classList.remove('is-dragging');\n    document.body.style.cursor = '';\n    document.body.style.userSelect = '';\n    localStorage.setItem('ctox.${appId}.leftWidth', leftWidth);\n  };\n\n  resizer.addEventListener('pointerdown', onPointerDown);\n  window.addEventListener('pointermove', onPointerMove);\n  window.addEventListener('pointerup', onPointerUp);\n\n  return () => {\n    resizer.removeEventListener('pointerdown', onPointerDown);\n    window.removeEventListener('pointermove', onPointerMove);\n    window.removeEventListener('pointerup', onPointerUp);\n  };\n}\n\nasync function loadInitialData() {\n  if (!state.ctx.db?.raw?.${primaryColl}) return;\n  const items = await state.ctx.db.raw.${primaryColl}.find().exec();\n  state.records = items.map(item => item.toJSON());\n  renderList();\n}\n\nfunction wireReactiveSync() {\n  if (!state.ctx.db?.raw?.${primaryColl}) return () => {};\n  const sub = state.ctx.db.raw.${primaryColl}.find().$.subscribe(items => {\n    state.records = items.map(item => item.toJSON());\n    renderList();\n    if (state.selectedId) {\n      const activeItem = state.records.find(r => r.id === state.selectedId);\n      if (activeItem) showDetail(activeItem);\n    }\n  });\n  return sub;\n}\n\nfunction renderList() {\n  const container = state.ctx.host.querySelector('[data-list-container]') || state.ctx.left?.querySelector('[data-list-container]');\n  if (!container) return;\n  container.innerHTML = '';\n  \n  if (state.records.length === 0) {\n    container.innerHTML = '<div style=\"color: var(--muted); font-size: 12px; text-align: center; margin-top: 20px;\">Keine Einträge vorhanden</div>';\n    return;\n  }\n\n  state.records.forEach(record => {\n    const card = document.createElement('div');\n    card.className = \`record-item-card \${state.selectedId === record.id ? 'is-active' : ''}\`;\n    card.dataset.id = record.id;\n    \n    card.setAttribute('data-context-module', '${appId}');\n    card.setAttribute('data-context-record-type', '${primaryColl}');\n    card.setAttribute('data-context-record-id', record.id);\n    card.setAttribute('data-context-label', record.title);\n\n    card.innerHTML = \`\n      <div style=\"font-weight: 600; font-size: 13px;\">\${record.title}</div>\n      <div style=\"font-size: 11px; color: var(--muted); margin-top: 4px; display: flex; justify-content: space-between;\">\n        <span>Status: \${record.status}</span>\n        <span>\${new Date(record.updated_at_ms).toLocaleTimeString()}</span>\n      </div>\n    \`;\n    card.addEventListener('click', () => selectRecord(record.id));\n    container.appendChild(card);\n  });\n}\n\nfunction selectRecord(id) {\n  state.selectedId = id;\n  const record = state.records.find(r => r.id === id);\n  if (record) {\n    showDetail(record);\n    renderList();\n  }\n}\n\nfunction showDetail(record) {\n  const emptyState = state.ctx.host.querySelector('#empty-state');\n  const detailCard = state.ctx.host.querySelector('#detail-card');\n  const titleHeader = state.ctx.host.querySelector('#selected-item-title');\n  \n  if (emptyState) emptyState.classList.add('is-hidden');\n  if (detailCard) detailCard.classList.remove('is-hidden');\n  if (titleHeader) titleHeader.textContent = record.title;\n\n  const inputTitle = state.ctx.host.querySelector('#record-detail-title');\n  const selectStatus = state.ctx.host.querySelector('#record-detail-status');\n  \n  if (inputTitle) inputTitle.value = record.title;\n  if (selectStatus) selectStatus.value = record.status;\n}\n\nfunction wireUi(host) {\n  const btnCreate = host.querySelector('#btn-create-record') || state.ctx.left?.querySelector('#btn-create-record');\n  const btnSave = host.querySelector('#btn-save-record');\n  \n  if (btnCreate) {\n    btnCreate.addEventListener('click', async () => {\n      if (!state.ctx.db?.raw?.${primaryColl}) return;\n      const newId = \`rec-\${Date.now()}\`;\n      await state.ctx.db.raw.${primaryColl}.insert({\n        id: newId,\n        title: 'Neuer Eintrag',\n        status: 'Entwurf',\n        updated_at_ms: Date.now(),\n        data: {}\n      });\n      selectRecord(newId);\n      state.ctx.notifications.show({\n        title: 'Eintrag erstellt',\n        message: 'Ein neuer Datensatz wurde erfolgreich angelegt.',\n        type: 'success'\n      });\n    });\n  }\n\n  if (btnSave) {\n    btnSave.addEventListener('click', async () => {\n      if (!state.selectedId || !state.ctx.db?.raw?.${primaryColl}) return;\n      const inputTitle = host.querySelector('#record-detail-title');\n      const selectStatus = host.querySelector('#record-detail-status');\n      \n      const doc = await state.ctx.db.raw.${primaryColl}.findOne(state.selectedId).exec();\n      if (doc) {\n        await doc.patch({\n          title: inputTitle.value || 'Unbenannt',\n          status: selectStatus.value,\n          updated_at_ms: Date.now()\n        });\n        state.ctx.notifications.show({\n          title: 'Gespeichert',\n          message: 'Die Änderungen wurden erfolgreich synchronisiert.',\n          type: 'success'\n        });\n      }\n    });\n  }\n}\n`;
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
  addConsoleLog(`[2/3] Bereite RxDB Schema Definition für [${collections.join(', ')}] vor...`, 'info');

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
        collections: collections,
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
