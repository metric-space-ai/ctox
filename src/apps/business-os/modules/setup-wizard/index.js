const PROFILE_ID = 'default';
const ONBOARDING_ID = 'default';
const SETUP_MODULE_ID = 'setup-wizard';
const ACCOUNT_PREFS_KEY = 'ctox.businessOs.accountPreferences';
const ONBOARDING_DONE_KEY = 'ctox.businessOs.onboarding.completed';
const SETUP_COMMAND_WAIT_MS = 120000;
const SETUP_ONBOARDING_WAIT_MS = 150000;
const NATIVE_COMMAND_POLL_MS = 750;

const STEPS = [
  {
    id: 'appearance',
    title: 'Sprache & Theme',
    kicker: 'Oberfläche',
    summary: 'Sprache und Darstellung',
  },
  {
    id: 'profile',
    title: 'Business Profil',
    kicker: 'Identität',
    summary: 'Mission, Vision und Leitplanken',
  },
  {
    id: 'runtime',
    title: 'LLM Runtime',
    kicker: 'Provider',
    summary: 'Provider, Modell und Auth',
  },
  {
    id: 'communication',
    title: 'Kommunikation',
    kicker: 'Pfade',
    summary: 'Eingänge, Eskalation und Review',
  },
  {
    id: 'review',
    title: 'Abschluss',
    kicker: 'Review',
    summary: 'Speichern und Wizard entfernen',
  },
];

const PROVIDERS = [
  { id: 'local', title: 'Local CTOX', note: 'Lokale Runtime ohne externen API Key.' },
  { id: 'openai', title: 'OpenAI', note: 'API Key oder ChatGPT Subscription.' },
  { id: 'openrouter', title: 'OpenRouter', note: 'API Key für mehrere Modellanbieter.' },
  { id: 'anthropic', title: 'Anthropic', note: 'Claude Runtime per API Key.' },
  { id: 'minimax', title: 'MiniMax', note: 'MiniMax Runtime per API Key.' },
];

const CHANNELS = [
  { id: 'jami', label: 'Jami' },
  { id: 'email', label: 'E-Mail' },
  { id: 'chat', label: 'Chat' },
  { id: 'browser', label: 'Browser Assist' },
  { id: 'manual', label: 'Manuell' },
];

export async function mount(container, ctx = {}) {
  ensureStylesheet();
  container.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());

  const state = {
    step: 0,
    busy: false,
    status: '',
    profile: defaultProfile(),
    runtimeSettings: null,
    runtimeDraft: defaultRuntimeDraft(),
    preferences: defaultPreferences(ctx),
    communicationAccounts: [],
    onboarding: null,
  };

  const root = container.querySelector('[data-setup-root]');
  const refs = {
    steps: root.querySelector('[data-setup-steps]'),
    kicker: root.querySelector('[data-step-kicker]'),
    title: root.querySelector('[data-step-title]'),
    progress: root.querySelector('[data-progress-fill]'),
    content: root.querySelector('[data-step-content]'),
    back: root.querySelector('[data-back]'),
    next: root.querySelector('[data-next]'),
    status: root.querySelector('[data-status]'),
    summaryScore: root.querySelector('[data-summary-score]'),
    summary: root.querySelector('[data-summary-list]'),
    openSettings: root.querySelector('[data-open-settings]'),
    skip: root.querySelector('[data-skip-setup]'),
  };

  const subscriptions = [];

  await ensureCollections(ctx);
  await loadInitialState();
  render();

  refs.steps.addEventListener('click', (event) => {
    const button = event.target.closest('[data-step-index]');
    if (!button || state.busy) return;
    syncVisibleForm();
    state.step = Number(button.dataset.stepIndex || 0);
    state.status = '';
    render();
  });
  refs.back.addEventListener('click', () => {
    if (state.busy || state.step <= 0) return;
    syncVisibleForm();
    state.step -= 1;
    state.status = '';
    render();
  });
  refs.next.addEventListener('click', async () => {
    if (state.busy) return;
    syncVisibleForm();
    if (state.step < STEPS.length - 1) {
      state.step += 1;
      state.status = '';
      render();
      return;
    }
    await finishSetup();
  });
  refs.openSettings.addEventListener('click', () => ctx.openSettings?.({ initialTab: 'profile' }));
  refs.skip.addEventListener('click', () => ctx.onClose?.());
  refs.content.addEventListener('input', () => {
    syncVisibleForm();
    renderSummary();
  });
  refs.content.addEventListener('change', (event) => {
    const rerender = Boolean(event.target.closest('[data-runtime-provider], [data-runtime-auth-mode], [data-setup-language], [data-setup-theme]'));
    syncVisibleForm();
    if (rerender) render();
    else renderSummary();
  });
  refs.content.addEventListener('click', (event) => {
    const channel = event.target.closest('[data-channel-choice]');
    if (!channel) return;
    const id = channel.dataset.channelChoice || '';
    toggleChannel(id);
    render();
  });

  startSubscriptions();

  return () => {
    for (const sub of subscriptions) {
      try { sub?.unsubscribe?.(); } catch {}
    }
  };

  async function loadInitialState() {
    const [profile, runtimeSettings, communicationAccounts, onboarding] = await Promise.all([
      readDoc(ctx.db, 'business_profile', PROFILE_ID).catch(() => null),
      readDoc(ctx.db, 'ctox_runtime_settings', 'runtime-settings').catch(() => null),
      readCollection(ctx.db, 'communication_accounts').catch(() => []),
      readDoc(ctx.db, 'business_onboarding_state', ONBOARDING_ID).catch(() => null),
    ]);
    state.profile = mergeProfile(profile);
    state.runtimeSettings = runtimeSettings;
    state.runtimeDraft = runtimeDraftFromSettings(runtimeSettings);
    state.preferences = defaultPreferences(ctx);
    state.communicationAccounts = communicationAccounts;
    state.onboarding = onboarding;
  }

  function startSubscriptions() {
    subscribeDoc(ctx.db, 'business_profile', PROFILE_ID, (doc) => {
      state.profile = mergeProfile(doc);
      render();
    });
    subscribeDoc(ctx.db, 'ctox_runtime_settings', 'runtime-settings', (doc) => {
      state.runtimeSettings = doc;
      if (!state.busy) {
        state.runtimeDraft = runtimeDraftFromSettings(doc);
        render();
      }
    });
    subscribeDoc(ctx.db, 'business_onboarding_state', ONBOARDING_ID, (doc) => {
      state.onboarding = doc;
      if (onboardingCompleted(doc)) {
        localStorage.setItem(ONBOARDING_DONE_KEY, '1');
      }
    });
  }

  function subscribeDoc(db, collection, id, onValue) {
    const coll = db?.collection?.(collection);
    const stream = coll?.findOne?.(id)?.$;
    if (!stream?.subscribe) return;
    const sub = stream.subscribe((doc) => {
      const data = doc?.toJSON?.();
      if (data && data._deleted !== true && data.is_deleted !== true) onValue(data);
    });
    subscriptions.push(sub);
  }

  function render() {
    const step = STEPS[state.step] || STEPS[0];
    refs.kicker.textContent = step.kicker;
    refs.title.textContent = step.title;
    refs.progress.style.width = `${Math.round((state.step / Math.max(1, STEPS.length - 1)) * 100)}%`;
    refs.steps.innerHTML = STEPS
      .map((item, index) => stepButton(item, index, stepComplete(item.id), index === state.step))
      .join('');
    refs.content.innerHTML = stepContent(step.id);
    refs.back.disabled = state.busy || state.step === 0;
    refs.next.disabled = state.busy;
    refs.next.textContent = state.step === STEPS.length - 1 ? 'Abschließen' : 'Weiter';
    refs.status.textContent = state.status;
    renderSummary();
  }

  function currentStepId() {
    return (STEPS[state.step] || STEPS[0]).id;
  }

  function renderSummary() {
    const rows = summaryRows();
    const completeCount = rows.filter((row) => row.ok).length;
    refs.summaryScore.textContent = `${completeCount}/${rows.length}`;
    refs.summary.innerHTML = rows
      .map((row) => `
        <article class="setup-summary-item ${row.ok ? 'is-ok' : 'is-warn'}">
          <span class="setup-summary-dot" aria-hidden="true"></span>
          <div>
            <strong>${escapeHtml(row.title)}</strong>
            <small>${escapeHtml(row.note)}</small>
          </div>
        </article>
      `)
      .join('');
  }

  function stepContent(stepId) {
    if (stepId === 'appearance') return appearanceStep();
    if (stepId === 'profile') return profileStep();
    if (stepId === 'runtime') return runtimeStep();
    if (stepId === 'communication') return communicationStep();
    return reviewStep();
  }

  function appearanceStep() {
    const prefs = normalizePreferences(state.preferences);
    return `
      <section class="setup-section">
        <header class="setup-section-head">
          <h3>Sprache und Darstellung</h3>
          <p>Diese lokalen Einstellungen werden sofort auf Shell, Header und später geladene Module angewendet.</p>
        </header>
        <div class="setup-preference-block">
          <span>Sprache</span>
          <div class="setup-choice-grid is-compact" role="radiogroup" aria-label="Sprache">
            <label class="setup-choice">
              <input type="radio" name="setup-language" data-setup-language value="de" ${prefs.language === 'de' ? 'checked' : ''}>
              <strong>Deutsch</strong>
              <small>Deutsche Shell und Modultexte, soweit verfügbar.</small>
            </label>
            <label class="setup-choice">
              <input type="radio" name="setup-language" data-setup-language value="en" ${prefs.language === 'en' ? 'checked' : ''}>
              <strong>English</strong>
              <small>English shell and module labels where available.</small>
            </label>
          </div>
        </div>
        <div class="setup-preference-block">
          <span>Theme</span>
          <div class="setup-choice-grid is-compact" role="radiogroup" aria-label="Theme">
            <label class="setup-choice setup-theme-choice">
              <input type="radio" name="setup-theme" data-setup-theme value="dark" ${prefs.theme === 'dark' ? 'checked' : ''}>
              <strong>Dark</strong>
              <small>Gedimmte Oberfläche für längere Arbeitsphasen.</small>
            </label>
            <label class="setup-choice setup-theme-choice is-light-preview">
              <input type="radio" name="setup-theme" data-setup-theme value="light" ${prefs.theme === 'light' ? 'checked' : ''}>
              <strong>Light</strong>
              <small>Helle Oberfläche für Tageslicht und Präsentationen.</small>
            </label>
          </div>
        </div>
      </section>
    `;
  }

  function profileStep() {
    const principles = Array.isArray(state.profile.operating_principles)
      ? state.profile.operating_principles.join('\n')
      : '';
    return `
      <section class="setup-section">
        <header class="setup-section-head">
          <h3>Grundprofil</h3>
          <p>Diese Werte geben CTOX den operativen Kontext für Aufgaben, Antworten und Eskalationen.</p>
        </header>
        <div class="setup-grid">
          <label class="setup-field">
            <span>Organisation</span>
            <input data-profile-company value="${escapeAttr(state.profile.company_name)}" autocomplete="organization">
          </label>
          <label class="setup-field">
            <span>Verantwortlich</span>
            <input data-profile-operator value="${escapeAttr(state.profile.operator_name)}" autocomplete="name">
          </label>
          <label class="setup-field is-wide">
            <span>Mission Statement</span>
            <textarea data-profile-mission>${escapeHtml(state.profile.mission_statement)}</textarea>
          </label>
          <label class="setup-field is-wide">
            <span>Vision Statement</span>
            <textarea data-profile-vision>${escapeHtml(state.profile.vision_statement)}</textarea>
          </label>
          <label class="setup-field is-wide">
            <span>Operating Principles</span>
            <textarea data-profile-principles>${escapeHtml(principles)}</textarea>
          </label>
        </div>
      </section>
    `;
  }

  function runtimeStep() {
    const runtime = state.runtimeDraft;
    const provider = runtime.provider || 'local';
    const authMode = normalizedAuthMode(provider, runtime.auth_mode);
    const showAuth = provider !== 'local';
    const showApiKey = showAuth && authMode === 'api_key';
    return `
      <section class="setup-section">
        <header class="setup-section-head">
          <h3>LLM Provider</h3>
          <p>Die Auswahl wird in den regulaeren CTOX Runtime Settings gespeichert.</p>
        </header>
        <div class="setup-choice-grid">
          ${PROVIDERS.map((item) => `
            <label class="setup-choice">
              <input type="radio" name="runtime-provider" data-runtime-provider value="${escapeAttr(item.id)}" ${provider === item.id ? 'checked' : ''}>
              <strong>${escapeHtml(item.title)}</strong>
              <small>${escapeHtml(item.note)}</small>
            </label>
          `).join('')}
        </div>
        <div class="setup-grid">
          ${showAuth ? `
            <label class="setup-field">
              <span>Autorisierung</span>
              <select data-runtime-auth-mode>
                ${option('api_key', 'API Key', authMode)}
                ${provider === 'openai' ? option('chatgpt_subscription', 'ChatGPT Subscription', authMode) : ''}
              </select>
            </label>
          ` : ''}
          <label class="setup-field">
            <span>Chat Modell</span>
            <input data-runtime-model value="${escapeAttr(runtime.chat_model)}" placeholder="${escapeAttr(defaultModelForProvider(provider))}">
          </label>
          <label class="setup-field">
            <span>Preset</span>
            <select data-runtime-preset>
              ${option('Quality', 'Quality', runtime.preset)}
              ${option('Performance', 'Performance', runtime.preset)}
            </select>
          </label>
          <label class="setup-field">
            <span>Context</span>
            <select data-runtime-context>
              ${option('128k', '128k', runtime.context)}
              ${option('256k', '256k', runtime.context)}
            </select>
          </label>
          <label class="setup-field">
            <span>Max Run Sekunden</span>
            <input data-runtime-timeout inputmode="numeric" value="${escapeAttr(runtime.max_run_secs || 1800)}">
          </label>
          ${showApiKey ? `
            <label class="setup-field is-wide">
              <span>API Key</span>
              <input data-runtime-api-key type="password" autocomplete="off" placeholder="${escapeAttr(state.runtimeSettings?.auth?.api_key_configured ? 'gespeichert - leer lassen' : 'API Key eingeben')}">
            </label>
          ` : ''}
        </div>
        ${provider === 'openai' && authMode === 'chatgpt_subscription' ? '<p class="setup-message">Die ChatGPT Subscription kann nach dem Setup in Settings > Runtime verbunden oder erneuert werden.</p>' : ''}
      </section>
    `;
  }

  function communicationStep() {
    const paths = state.profile.communication_paths || {};
    const selected = new Set(Array.isArray(paths.preferred_channels) ? paths.preferred_channels : []);
    const accounts = state.communicationAccounts
      .filter((item) => item && item._deleted !== true && item.is_deleted !== true)
      .map((item) => item.display_name || item.account_key || item.channel)
      .filter(Boolean);
    return `
      <section class="setup-section">
        <header class="setup-section-head">
          <h3>Kommunikationspfade</h3>
          <p>CTOX nutzt diese Pfade für Eingang, Rückfragen und externe Kommunikation.</p>
        </header>
        <div class="setup-chip-row" aria-label="Kommunikationskanaele">
          ${CHANNELS.map((item) => `
            <button type="button" class="setup-chip" data-channel-choice="${escapeAttr(item.id)}" aria-pressed="${selected.has(item.id) ? 'true' : 'false'}">${escapeHtml(item.label)}</button>
          `).join('')}
        </div>
        <div class="setup-grid">
          <label class="setup-field">
            <span>Eskalation an</span>
            <input data-communication-escalation value="${escapeAttr(paths.escalation_target || state.profile.operator_name || '')}" placeholder="Name, Rolle oder Kanal">
          </label>
          <label class="setup-field">
            <span>Externe Zusagen</span>
            <select data-routing-external>
              ${option('founder_review', 'Founder Review erforderlich', state.profile.routing_policy?.external_contact)}
              ${option('chef_review', 'Chef/Admin Review erforderlich', state.profile.routing_policy?.external_contact)}
              ${option('autonomous_after_policy', 'Autonom nach Policy', state.profile.routing_policy?.external_contact)}
            </select>
          </label>
          <label class="setup-field is-wide">
            <span>Kommunikationsnotizen</span>
            <textarea data-communication-notes>${escapeHtml(paths.notes || '')}</textarea>
          </label>
        </div>
        <p class="setup-message">${escapeHtml(accounts.length ? `Vorhandene Accounts: ${accounts.join(', ')}` : 'Weitere Accounts kannst du später in Settings > Channels verbinden.')}</p>
      </section>
    `;
  }

  function reviewStep() {
    const rows = [
      ['Sprache', languageLabel(state.preferences.language)],
      ['Theme', themeLabel(state.preferences.theme)],
      ['Organisation', state.profile.company_name || '-'],
      ['Mission', state.profile.mission_statement || '-'],
      ['Vision', state.profile.vision_statement || '-'],
      ['Provider', providerLabel(state.runtimeDraft.provider)],
      ['Modell', state.runtimeDraft.chat_model || defaultModelForProvider(state.runtimeDraft.provider)],
      ['Kommunikation', selectedChannelLabels().join(', ') || '-'],
      ['Externe Zusagen', routingLabel(state.profile.routing_policy?.external_contact)],
    ];
    return `
      <section class="setup-section">
        <header class="setup-section-head">
          <h3>Speichern und entfernen</h3>
          <p>Nach dem Abschluss wird diese First-Start App aus den installierten Apps entfernt. Im App Store bleibt sie verfügbar.</p>
        </header>
        <div class="setup-review">
          ${rows.map(([label, value]) => `
            <div class="setup-review-row">
              <span class="setup-review-label">${escapeHtml(label)}</span>
              <p>${escapeHtml(value)}</p>
            </div>
          `).join('')}
        </div>
      </section>
    `;
  }

  function syncVisibleForm() {
    const stepId = currentStepId();
    if (stepId === 'appearance') {
      state.preferences = normalizePreferences({
        ...state.preferences,
        language: refs.content.querySelector('[data-setup-language]:checked')?.value || state.preferences.language,
        theme: refs.content.querySelector('[data-setup-theme]:checked')?.value || state.preferences.theme,
      });
      applySetupPreferences(ctx, state.preferences);
    }
    if (stepId === 'profile') {
      state.profile.company_name = refs.content.querySelector('[data-profile-company]')?.value?.trim() || '';
      state.profile.operator_name = refs.content.querySelector('[data-profile-operator]')?.value?.trim() || '';
      state.profile.mission_statement = refs.content.querySelector('[data-profile-mission]')?.value?.trim() || '';
      state.profile.vision_statement = refs.content.querySelector('[data-profile-vision]')?.value?.trim() || '';
      state.profile.operating_principles = lines(refs.content.querySelector('[data-profile-principles]')?.value || '');
    }
    if (stepId === 'runtime') {
      const provider = refs.content.querySelector('[data-runtime-provider]:checked')?.value || state.runtimeDraft.provider || 'local';
      state.runtimeDraft.provider = provider;
      state.runtimeDraft.auth_mode = normalizedAuthMode(
        provider,
        refs.content.querySelector('[data-runtime-auth-mode]')?.value || state.runtimeDraft.auth_mode,
      );
      state.runtimeDraft.chat_model = refs.content.querySelector('[data-runtime-model]')?.value?.trim() || '';
      state.runtimeDraft.preset = refs.content.querySelector('[data-runtime-preset]')?.value || 'Quality';
      state.runtimeDraft.context = refs.content.querySelector('[data-runtime-context]')?.value || '256k';
      state.runtimeDraft.max_run_secs = Number(refs.content.querySelector('[data-runtime-timeout]')?.value || 1800);
      state.runtimeDraft.api_key = refs.content.querySelector('[data-runtime-api-key]')?.value || '';
    }
    if (stepId === 'communication') {
      state.profile.communication_paths = {
        ...(state.profile.communication_paths || {}),
        escalation_target: refs.content.querySelector('[data-communication-escalation]')?.value?.trim() || '',
        notes: refs.content.querySelector('[data-communication-notes]')?.value?.trim() || '',
      };
      state.profile.routing_policy = {
        ...(state.profile.routing_policy || {}),
        external_contact: refs.content.querySelector('[data-routing-external]')?.value || 'founder_review',
      };
    }
  }

  function toggleChannel(channelId) {
    if (!channelId) return;
    const paths = state.profile.communication_paths || {};
    const selected = new Set(Array.isArray(paths.preferred_channels) ? paths.preferred_channels : []);
    if (selected.has(channelId)) selected.delete(channelId);
    else selected.add(channelId);
    state.profile.communication_paths = {
      ...paths,
      preferred_channels: Array.from(selected),
    };
  }

  async function finishSetup() {
    const missing = missingRequirements();
    if (missing.length) {
      state.status = `Fehlt noch: ${missing.join(', ')}`;
      render();
      return;
    }
    state.busy = true;
    state.status = 'Speichere CTOX Basisdaten...';
    applySetupPreferences(ctx, state.preferences);
    render();
    try {
      await dispatchSetupCommand('ctox.business_profile.save', PROFILE_ID, profilePayload(), { timeoutMs: SETUP_COMMAND_WAIT_MS });
      state.status = 'Speichere Runtime...';
      render();
      await dispatchSetupCommand('ctox.runtime_settings.save', 'runtime-settings', runtimePayload(), { timeoutMs: SETUP_COMMAND_WAIT_MS });
      state.status = 'Schliesse Setup ab...';
      render();
      await dispatchSetupCommand('ctox.onboarding.complete', ONBOARDING_ID, {
        profile_id: PROFILE_ID,
        setup_module_id: SETUP_MODULE_ID,
        preferences: normalizePreferences(state.preferences),
        uninstall_setup_wizard: true,
      }, { timeoutMs: SETUP_ONBOARDING_WAIT_MS });
      localStorage.setItem(ONBOARDING_DONE_KEY, '1');
      state.status = 'Setup abgeschlossen.';
      render();
      await ctx.refreshModules?.();
      window.setTimeout(() => ctx.onClose?.({ animation: 'bug-eat' }), 250);
    } catch (error) {
      state.status = String(error?.message || error);
      state.busy = false;
      render();
    }
  }

  async function dispatchSetupCommand(commandType, recordId, payload, options = {}) {
    await ctx.sync?.startCollection?.('business_commands');
    const commandId = `cmd_${newId()}`;
    const clientContext = {
      source: SETUP_MODULE_ID,
      module_id: SETUP_MODULE_ID,
      actor: actorContext(ctx.session),
    };
    const commandDoc = setupCommandDocument(commandId, commandType, recordId, payload, clientContext);
    const attempts = [
      submitNativeCommand(commandDoc),
    ];
    if (ctx.commandBus?.dispatch && ctx.db?.collection?.('business_commands')) {
      attempts.push(ctx.commandBus.dispatch({
        id: commandId,
        module: 'ctox',
        type: commandType,
        record_id: recordId,
        inbound_channel: SETUP_MODULE_ID,
        payload,
        client_context: clientContext,
      }));
    }
    const results = await Promise.allSettled(attempts);
    if (!results.some((result) => result.status === 'fulfilled')) {
      const error = results.find((result) => result.status === 'rejected')?.reason;
      throw error || new Error('business_commands collection ist nicht verfügbar.');
    }
    return waitForCommand(ctx.db, commandId, options.timeoutMs || 45000);
  }

  function setupCommandDocument(commandId, commandType, recordId, payload, clientContext) {
    const now = Date.now();
    return {
      id: commandId,
      command_id: commandId,
      module: 'ctox',
      command_type: commandType,
      record_id: recordId,
      status: 'pending_sync',
      inbound_channel: SETUP_MODULE_ID,
      payload,
      client_context: clientContext,
      created_at_ms: now,
      updated_at_ms: now,
    };
  }

  async function submitNativeCommand(commandDoc) {
    const response = await fetch('/api/business-os/commands', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ command: commandDoc }),
    });
    if (!response.ok) {
      throw new Error(`Native Command HTTP ${response.status}`);
    }
    const envelope = await response.json();
    return envelope?.command || envelope;
  }

  async function waitForCommand(db, commandId, timeoutMs) {
    const deadline = Date.now() + timeoutMs;
    let lastError = null;
    let nextNativePollAt = 0;
    while (Date.now() < deadline) {
      try {
        const doc = await readLocalDoc(db, 'business_commands', commandId);
        const data = doc?.toJSON?.() || doc;
        if (data?.status && data.status !== 'pending_sync') {
          if (data.status === 'failed') throw new Error(data.error || `Command ${commandId} failed`);
          return data.result || data;
        }
      } catch (error) {
        lastError = error;
      }
      if (Date.now() >= nextNativePollAt) {
        nextNativePollAt = Date.now() + NATIVE_COMMAND_POLL_MS;
        try {
          const data = await readNativeCommandStatus(commandId);
          if (data?.status && data.status !== 'pending_sync' && data.status !== 'accepted') {
            if (data.status === 'failed') throw new Error(data.error || `Command ${commandId} failed`);
            return data.result || data;
          }
        } catch (error) {
          lastError = error;
        }
      }
      await delay(300);
    }
    throw lastError || new Error(`Command ${commandId} wurde nicht synchronisiert.`);
  }

  function profilePayload() {
    return {
      id: PROFILE_ID,
      company_name: state.profile.company_name,
      operator_name: state.profile.operator_name,
      mission_statement: state.profile.mission_statement,
      vision_statement: state.profile.vision_statement,
      operating_principles: state.profile.operating_principles,
      communication_paths: state.profile.communication_paths || {},
      routing_policy: state.profile.routing_policy || {},
    };
  }

  function runtimePayload() {
    const provider = state.runtimeDraft.provider || 'local';
    const authMode = normalizedAuthMode(provider, state.runtimeDraft.auth_mode);
    return {
      provider,
      auth_mode: authMode,
      chat_model: state.runtimeDraft.chat_model || defaultModelForProvider(provider),
      preset: state.runtimeDraft.preset || 'Quality',
      context: state.runtimeDraft.context || '256k',
      max_run_secs: Number(state.runtimeDraft.max_run_secs || 1800),
      api_key: authMode === 'api_key' ? (state.runtimeDraft.api_key || '') : '',
    };
  }

  function summaryRows() {
    return [
      {
        title: 'Sprache & Theme',
        ok: stepComplete('appearance'),
        note: `${languageLabel(state.preferences.language)} · ${themeLabel(state.preferences.theme)}`,
      },
      {
        title: 'Business Profil',
        ok: stepComplete('profile'),
        note: state.profile.company_name || 'Organisation, Mission und Vision fehlen.',
      },
      {
        title: 'LLM Runtime',
        ok: stepComplete('runtime'),
        note: `${providerLabel(state.runtimeDraft.provider)} ${state.runtimeDraft.chat_model || defaultModelForProvider(state.runtimeDraft.provider)}`,
      },
      {
        title: 'Kommunikation',
        ok: stepComplete('communication'),
        note: selectedChannelLabels().join(', ') || 'Noch keine Pfade gewählt.',
      },
      {
        title: 'Settings',
        ok: true,
        note: 'Alle Werte bleiben später in Settings editierbar.',
      },
    ];
  }

  function stepComplete(stepId) {
    if (stepId === 'appearance') {
      const prefs = normalizePreferences(state.preferences);
      return Boolean(prefs.language && prefs.theme);
    }
    if (stepId === 'profile') {
      return Boolean(state.profile.company_name && state.profile.mission_statement && state.profile.vision_statement);
    }
    if (stepId === 'runtime') {
      const provider = state.runtimeDraft.provider || 'local';
      const authMode = normalizedAuthMode(provider, state.runtimeDraft.auth_mode);
      if (provider === 'local') return true;
      if (authMode === 'chatgpt_subscription') return true;
      return Boolean(state.runtimeDraft.api_key || state.runtimeSettings?.auth?.api_key_configured);
    }
    if (stepId === 'communication') {
      const channels = state.profile.communication_paths?.preferred_channels;
      return Array.isArray(channels) && channels.length > 0;
    }
    return missingRequirements().length === 0;
  }

  function missingRequirements() {
    const missing = [];
    if (!stepComplete('appearance')) missing.push('Sprache & Theme');
    if (!stepComplete('profile')) missing.push('Business Profil');
    if (!stepComplete('runtime')) missing.push('Runtime Auth');
    if (!stepComplete('communication')) missing.push('Kommunikationspfad');
    return missing;
  }

  function selectedChannelLabels() {
    const selected = new Set(state.profile.communication_paths?.preferred_channels || []);
    return CHANNELS.filter((item) => selected.has(item.id)).map((item) => item.label);
  }

  function readCollection(db, collection) {
    return db?.collection?.(collection)?.find?.().exec?.().then((docs) => docs.map((doc) => doc.toJSON?.() || doc)) || Promise.resolve([]);
  }
}

async function readLocalDoc(db, collection, id) {
  const coll = db?.collection?.(collection);
  if (coll?.storageCollection?.findOne) {
    return coll.storageCollection.findOne(id);
  }
  return coll?.findOne?.(id)?.exec?.() || null;
}

async function readNativeCommandStatus(commandId) {
  const url = `/api/business-os/commands/status?command_id=${encodeURIComponent(commandId)}`;
  const response = await fetch(url, { cache: 'no-store' });
  if (!response.ok) {
    throw new Error(`Command Status HTTP ${response.status}`);
  }
  const envelope = await response.json();
  return envelope?.command || null;
}

async function ensureCollections(ctx) {
  await Promise.allSettled([
    ctx.sync?.startCollection?.('business_profile'),
    ctx.sync?.startCollection?.('business_onboarding_state'),
    ctx.sync?.startCollection?.('ctox_runtime_settings'),
    ctx.sync?.startCollection?.('communication_accounts'),
    ctx.sync?.startCollection?.('business_commands'),
  ]);
}

async function readDoc(db, collection, id) {
  const doc = await db?.collection?.(collection)?.findOne?.(id).exec();
  const data = doc?.toJSON?.();
  if (!data || data._deleted === true || data.is_deleted === true) return null;
  return data;
}

function defaultProfile() {
  return {
    id: PROFILE_ID,
    company_name: '',
    operator_name: '',
    mission_statement: '',
    vision_statement: '',
    operating_principles: [
      'Externe Zusagen brauchen Founder Review.',
      'CTOX arbeitet taskbasiert und dokumentiert Entscheidungen.',
      'Unsichere Entscheidungen werden eskaliert.',
    ],
    communication_paths: {
      preferred_channels: ['jami'],
      escalation_target: '',
      notes: '',
    },
    routing_policy: {
      external_contact: 'founder_review',
    },
  };
}

function defaultPreferences(ctx = {}) {
  return normalizePreferences({
    ...readLocalAccountPrefs(),
    ...(ctx.preferences || {}),
    language: ctx.preferences?.language || document.documentElement.lang,
    theme: ctx.preferences?.theme || document.documentElement.dataset.theme,
  });
}

function normalizePreferences(raw = {}) {
  return {
    language: raw.language === 'en' ? 'en' : 'de',
    theme: raw.theme === 'light' ? 'light' : 'dark',
  };
}

function applySetupPreferences(ctx = {}, prefs = {}) {
  const next = normalizePreferences(prefs);
  if (typeof ctx.applyPreferences === 'function') {
    ctx.applyPreferences(next);
    return next;
  }
  const stored = { ...readLocalAccountPrefs(), ...next };
  try {
    localStorage.setItem(ACCOUNT_PREFS_KEY, JSON.stringify(stored));
  } catch {}
  document.documentElement.lang = next.language;
  document.documentElement.dataset.theme = next.theme;
  window.dispatchEvent(new CustomEvent('ctox-business-os-preferences', { detail: next }));
  window.postMessage({ type: 'ctox-business-os-language', lang: next.language }, '*');
  return next;
}

function readLocalAccountPrefs() {
  try {
    return JSON.parse(localStorage.getItem(ACCOUNT_PREFS_KEY) || '{}') || {};
  } catch {
    return {};
  }
}

function languageLabel(value) {
  return value === 'en' ? 'English' : 'Deutsch';
}

function themeLabel(value) {
  return value === 'light' ? 'Light' : 'Dark';
}

function mergeProfile(profile) {
  const fallback = defaultProfile();
  if (!profile) return fallback;
  return {
    ...fallback,
    ...profile,
    operating_principles: Array.isArray(profile.operating_principles)
      ? profile.operating_principles
      : fallback.operating_principles,
    communication_paths: {
      ...fallback.communication_paths,
      ...(profile.communication_paths || {}),
    },
    routing_policy: {
      ...fallback.routing_policy,
      ...(profile.routing_policy || {}),
    },
  };
}

function defaultRuntimeDraft() {
  return {
    provider: 'local',
    auth_mode: 'api_key',
    chat_model: '',
    preset: 'Quality',
    context: '256k',
    max_run_secs: 1800,
    api_key: '',
  };
}

function runtimeDraftFromSettings(settings) {
  const runtime = settings?.runtime || {};
  const auth = settings?.auth || {};
  const provider = runtime.provider || 'local';
  return {
    provider,
    auth_mode: normalizedAuthMode(provider, auth.mode),
    chat_model: runtime.chat_model || '',
    preset: runtime.preset || 'Quality',
    context: runtime.context || '256k',
    max_run_secs: Number(runtime.max_run_secs || 1800),
    api_key: '',
  };
}

function normalizedAuthMode(provider, mode) {
  const value = String(mode || '').trim().toLowerCase();
  if (provider === 'local') return 'api_key';
  if (provider === 'openai' && ['chatgpt_subscription', 'subscription', 'chatgpt'].includes(value)) {
    return 'chatgpt_subscription';
  }
  return 'api_key';
}

function defaultModelForProvider(provider) {
  return {
    openai: 'gpt-5.1',
    openrouter: 'openrouter/auto',
    anthropic: 'claude-sonnet-4-5',
    minimax: 'MiniMax-M1',
    local: '',
  }[provider || 'local'] || '';
}

function providerLabel(provider) {
  return PROVIDERS.find((item) => item.id === provider)?.title || provider || 'Local CTOX';
}

function routingLabel(value) {
  return {
    founder_review: 'Founder Review erforderlich',
    chef_review: 'Chef/Admin Review erforderlich',
    autonomous_after_policy: 'Autonom nach Policy',
  }[value || 'founder_review'] || value || 'Founder Review erforderlich';
}

function stepButton(item, index, complete, active) {
  return `
    <button type="button" class="setup-step-button ${complete ? 'is-complete' : ''}" data-step-index="${index}" aria-current="${active ? 'step' : 'false'}">
      <span class="setup-step-index">${complete ? '✓' : index + 1}</span>
      <span class="setup-step-copy">
        <strong>${escapeHtml(item.title)}</strong>
        <small>${escapeHtml(item.summary)}</small>
      </span>
    </button>
  `;
}

function option(value, label, selected) {
  return `<option value="${escapeAttr(value)}" ${String(value) === String(selected || '') ? 'selected' : ''}>${escapeHtml(label)}</option>`;
}

function lines(value) {
  return String(value || '')
    .split(/\n+/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function onboardingCompleted(doc) {
  if (!doc || doc._deleted === true || doc.is_deleted === true) return false;
  return doc.status === 'completed' || Number(doc.completed_at_ms || 0) > 0;
}

function actorContext(session) {
  const user = session?.user || {};
  return {
    id: user.id || '',
    display_name: user.display_name || user.id || '',
    role: user.role || (user.is_admin ? 'admin' : 'user'),
  };
}

function newId() {
  return globalThis.crypto?.randomUUID?.() || `${Date.now()}_${Math.random().toString(36).slice(2)}`;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function ensureStylesheet() {
  const href = new URL('./index.css', import.meta.url).pathname;
  if (document.head.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;',
  }[char]));
}

function escapeAttr(value) {
  return escapeHtml(value).replace(/`/g, '&#96;');
}
