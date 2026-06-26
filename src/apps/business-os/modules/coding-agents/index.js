import { loadModuleMessages } from '../../shared/i18n.js';
import { CtoxResizer } from '../../shared/resizer.js';

const state = {
  ctx: null,
  activeApp: 'antigravity',
  activeWorkspace: null,
  diagnostics: {
    antigravity: { installed: false, electronPid: 'N/A', lsPid: 'N/A', port: 'N/A', resources: 'N/A', uptime: 'N/A', online: false },
    claude: { installed: false, electronPid: 'N/A', lsPid: 'N/A', port: 'N/A', resources: 'N/A', uptime: 'N/A', online: false },
    codex: { installed: false, electronPid: 'N/A', lsPid: 'N/A', port: 'N/A', resources: 'N/A', uptime: 'N/A', online: false }
  },
  trustedPaths: [],
  sessions: [],
  activeSession: null,
  activeSessionApp: null,
  workspaceLoadState: 'loading',
  workspaceLoadError: '',
  isAutomating: false,
  isRefreshing: false,
  projectionTimers: {}
};

const labels = {
  de: {
    moduleTitle: 'Coding Agents Unified OS',
    connectionConnected: 'Verbunden mit Host',
    connectionDisconnected: 'Verbindung getrennt',
    unauthorized: 'NICHT LIZENZIERT',
    authorized: 'AKTIV',
    installing: 'Installiere...',
    installed: 'Installiert',
    notInstalled: 'Nicht installiert',
    active: 'AKTIV',
    inactive: 'INAKTIV',
    automating: 'AUTOMATISIERE...',
    authSuccessful: 'Erfolgreich Authentifiziert!'
  },
  en: {
    moduleTitle: 'Coding Agents Unified OS',
    connectionConnected: 'Connected to Host',
    connectionDisconnected: 'Disconnected',
    unauthorized: 'UNAUTHORIZED',
    authorized: 'ACTIVE',
    installing: 'Installing...',
    installed: 'Installed',
    notInstalled: 'Not installed',
    active: 'ACTIVE',
    inactive: 'INACTIVE',
    automating: 'AUTOMATING...',
    authSuccessful: 'Successfully Authenticated!'
  }
};

let els = {};
let t = (k, f) => f || k;

export async function mount(ctx) {
  state.ctx = ctx;
  const messages = await loadModuleMessages(import.meta.url, ctx.locale, labels);
  t = (key, fallback) => messages[key] ?? fallback ?? key;

  // Inject stylesheet dynamically
  const styleLink = document.createElement('link');
  styleLink.rel = 'stylesheet';
  styleLink.href = new URL('./index.css', import.meta.url).href;
  styleLink.id = 'coding-agents-module-styles';
  document.head.appendChild(styleLink);

  // Set up center content markup
  ctx.host.innerHTML = await loadModuleMarkup();
  ctx.left.replaceChildren();
  // Persistent third pane: session/history inspection stays visible while the active provider prompt remains in the center workbench.
  ctx.right.replaceChildren();

  bindElements(ctx.host);
  wireEvents();
  const projectionSubscriptions = subscribeProjectionUpdates();

  // Initialize resizable layout columns
  const resizers = [];
  const leftResizer = ctx.host.querySelector('[data-resizer="left"]');
  const containerEl = ctx.host.querySelector('[data-coding-agents-root]');

  if (leftResizer && containerEl) {
    const resizerL = new CtoxResizer({
      resizerEl: leftResizer,
      containerEl: containerEl,
      cssVar: '--coding-agents-left-width',
      side: 'left',
      minWidth: 260,
      maxWidth: 450
    });
    resizers.push(resizerL);
  }

  // Set initial diagnostics state
  updateUI();

  // Run initial diagnostic refresh with a timeout safety net so the
  // "Loading workspaces..." placeholder cannot hang indefinitely when the
  // backend is unreachable.
  startInitialLoadWithTimeout();

  const diagnosticsAutoRefresh = startDiagnosticsAutoRefresh();

  return () => {
    diagnosticsAutoRefresh?.stop?.();
    if (state.initialLoadTimer) clearTimeout(state.initialLoadTimer);
    clearProjectionTimers();
    projectionSubscriptions.forEach((subscription) => {
      try { subscription?.unsubscribe?.(); } catch (err) { console.warn('[coding-agents] projection unsubscribe failed', err); }
    });
    resizers.forEach(r => r.destroy());
    styleLink.remove();
  };
}

const INITIAL_LOAD_TIMEOUT_MS = 10000;

function startDiagnosticsAutoRefresh(_options = {}) {
  // Provider status checks are persisted commands. Keeping this module open must
  // not create recurring business_commands/ctox_queue_tasks writes while idle.
  return null;
}

function startInitialLoadWithTimeout() {
  state.initialLoadDone = false;
  state.workspaceLoadState = 'loading';
  state.workspaceLoadError = '';
  renderWorkspaces();
  if (state.initialLoadTimer) clearTimeout(state.initialLoadTimer);

  state.initialLoadTimer = setTimeout(() => {
    if (state.initialLoadDone) return;
    showWorkspacesTimeoutState();
  }, INITIAL_LOAD_TIMEOUT_MS);

  refreshAllData()
    .catch((err) => console.warn('[coding-agents] initial refresh failed', err))
    .finally(() => {
      state.initialLoadDone = true;
      if (state.initialLoadTimer) {
        clearTimeout(state.initialLoadTimer);
        state.initialLoadTimer = null;
      }
    });
}

function showWorkspacesTimeoutState() {
  if (els.workspacesListBox?.querySelector('.workspace-item')) return;
  setWorkspaceLoadState('error', 'Backend antwortet nicht innerhalb von 10 Sekunden.');
}

async function loadModuleMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function bindElements(root) {
  els.root = root.querySelector('[data-coding-agents-root]');

  // Headers & Titles
  els.activeAppTitle = root.querySelector('#active-app-title');
  els.activeAppDesc = root.querySelector('#active-app-desc');
  els.workbenchStatusDot = root.querySelector('#workbench-status-dot');
  els.workbenchStatusText = root.querySelector('#workbench-status-text');

  // Diagnostics (inside settings modal)
  els.diagInstalled = root.querySelector('#diag-installed');
  els.diagElectronPid = root.querySelector('#diag-electron-pid');
  els.diagLsPid = root.querySelector('#diag-ls-pid');
  els.diagActivePort = root.querySelector('#daemon-port-input');
  els.diagResources = root.querySelector('#diag-resources');
  els.diagUptime = root.querySelector('#diag-uptime');

  // Service elements
  els.serviceStatus = root.querySelector('#service-status');
  els.bypassToggle = root.querySelector('#bypass-permissions-toggle');

  // Workspaces list elements
  els.workspacesListBox = root.querySelector('#workspaces-list-box');
  els.addWorkspaceBtn = root.querySelector('#add-workspace-btn');
  els.addWorkspaceModal = root.querySelector('#add-workspace-modal');
  els.addWorkspaceForm = root.querySelector('#add-workspace-form');
  els.addWorkspaceInput = root.querySelector('#add-workspace-input');
  els.addWorkspaceSubmit = root.querySelector('#add-workspace-submit');
  els.addWorkspaceError = root.querySelector('#add-workspace-error');
  els.closeAddWorkspaceBtn = root.querySelector('#close-add-workspace-btn');

  // System Settings Modal elements
  els.openSettingsBtn = root.querySelector('#open-settings-btn');
  els.settingsModal = root.querySelector('#settings-modal');
  els.closeSettingsBtn = root.querySelector('#close-settings-btn');

  // Subscriptions (inside settings modal)
  els.authStatus = root.querySelector('#auth-status');
  els.subLoginForm = root.querySelector('#sub-login-form');
  els.btnTriggerSignup = root.querySelector('#btn-trigger-signup');
  els.browserLogBox = root.querySelector('#browser-log-box');

  // Direct multi-turn sessions logs/chat feed in center pane
  els.sessionSelect = root.querySelector('#workbench-session-select');
  els.newSessionBtn = root.querySelector('#new-session-btn');
  els.newSessionModal = root.querySelector('#new-session-modal');
  els.newSessionForm = root.querySelector('#new-session-form');
  els.newSessionPrompt = root.querySelector('#new-session-prompt');
  els.newSessionSubmit = root.querySelector('#new-session-submit');
  els.newSessionError = root.querySelector('#new-session-error');
  els.newSessionContext = root.querySelector('#new-session-context');
  els.closeNewSessionBtn = root.querySelector('#close-new-session-btn');
  els.chatFeed = root.querySelector('#workbench-chat-feed');
  els.promptForm = root.querySelector('#workbench-prompt-form');
  els.promptInput = root.querySelector('#workbench-prompt-input');
  els.promptSubmit = root.querySelector('#workbench-prompt-submit');

  // Backwards compatibility legacy hooks
  els.appSwitchBtns = root.querySelectorAll('.app-switch-btn');
  els.legacyRightTabSubscription = root.querySelector('[data-right-tab="subscription"]');
  els.legacyTrustedPathsBox = root.querySelector('#trusted-paths-box');
  els.legacyPathAuthForm = root.querySelector('#path-auth-form');
  els.legacyNewPathInput = root.querySelector('#new-path-input');
  els.legacySessionsListBox = root.querySelector('#sessions-list-box');
}

function wireEvents() {
  const openDialog = (modal, focusTarget) => {
    if (!modal) return;
    modal.removeAttribute('hidden');
    requestAnimationFrame(() => focusTarget?.focus?.());
  };
  const closeDialog = (modal, restoreFocus) => {
    if (!modal) return;
    modal.setAttribute('hidden', '');
    restoreFocus?.focus?.();
  };
  const syncWorkspaceForm = () => {
    const validation = validateWorkspacePath(els.addWorkspaceInput?.value || '');
    setFormError(els.addWorkspaceInput, els.addWorkspaceError, validation.error);
    if (els.addWorkspaceSubmit) els.addWorkspaceSubmit.disabled = !validation.valid;
  };
  const syncNewSessionForm = () => {
    const validation = validateNewSessionPrompt(els.newSessionPrompt?.value || '');
    setFormError(els.newSessionPrompt, els.newSessionError, validation.error);
    if (els.newSessionSubmit) els.newSessionSubmit.disabled = !validation.valid || !state.activeWorkspace;
  };

  // Modal open/close listeners
  if (els.openSettingsBtn) {
    els.openSettingsBtn.addEventListener('click', () => {
      openDialog(els.settingsModal, els.closeSettingsBtn);
    });
  }
  if (els.closeSettingsBtn) {
    els.closeSettingsBtn.addEventListener('click', () => {
      closeDialog(els.settingsModal, els.openSettingsBtn);
    });
  }
  if (els.addWorkspaceBtn) {
    els.addWorkspaceBtn.addEventListener('click', () => {
      if (els.addWorkspaceModal) {
        syncWorkspaceForm();
        openDialog(els.addWorkspaceModal, els.addWorkspaceInput);
      }
    });
  }
  if (els.closeAddWorkspaceBtn) {
    els.closeAddWorkspaceBtn.addEventListener('click', () => {
      closeDialog(els.addWorkspaceModal, els.addWorkspaceBtn);
    });
  }
  if (els.addWorkspaceInput) {
    els.addWorkspaceInput.addEventListener('input', syncWorkspaceForm);
  }
  // Click-outside-to-close for the Add Workspace modal so it always has an
  // escape hatch even if the close button isn't visible.
  if (els.addWorkspaceModal) {
    els.addWorkspaceModal.addEventListener('click', (event) => {
      if (event.target === els.addWorkspaceModal) {
        closeDialog(els.addWorkspaceModal, els.addWorkspaceBtn);
      }
    });
  }
  if (els.settingsModal) {
    els.settingsModal.addEventListener('click', (event) => {
      if (event.target === els.settingsModal) {
        closeDialog(els.settingsModal, els.openSettingsBtn);
      }
    });
  }
  if (els.newSessionModal) {
    els.newSessionModal.addEventListener('click', (event) => {
      if (event.target === els.newSessionModal) {
        closeDialog(els.newSessionModal, els.newSessionBtn);
      }
    });
  }
  if (els.closeNewSessionBtn) {
    els.closeNewSessionBtn.addEventListener('click', () => {
      closeDialog(els.newSessionModal, els.newSessionBtn);
    });
  }
  if (els.newSessionPrompt) {
    els.newSessionPrompt.addEventListener('input', syncNewSessionForm);
  }
  els.root.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape') return;
    const openDialogs = [els.newSessionModal, els.addWorkspaceModal, els.settingsModal].filter((modal) => modal && !modal.hidden);
    const topDialog = openDialogs[0];
    if (!topDialog) return;
    event.preventDefault();
    const restore = topDialog === els.newSessionModal ? els.newSessionBtn : topDialog === els.addWorkspaceModal ? els.addWorkspaceBtn : els.openSettingsBtn;
    closeDialog(topDialog, restore);
  });

  // Handle Workspace creation form submission
  if (els.addWorkspaceForm) {
    els.addWorkspaceForm.addEventListener('submit', async (e) => {
      e.preventDefault();
      const validation = validateWorkspacePath(els.addWorkspaceInput.value);
      setFormError(els.addWorkspaceInput, els.addWorkspaceError, validation.error);
      if (els.addWorkspaceSubmit) els.addWorkspaceSubmit.disabled = !validation.valid;
      if (!validation.valid) return;
      const folderPath = validation.path;

      appendTerminalPrompt(`config grant "${folderPath}"`);
      appendTerminalOutput(`Granting permissions for workspace: ${folderPath}...`);
      if (els.addWorkspaceSubmit) els.addWorkspaceSubmit.disabled = true;

      let res = null;
      state.isAutomating = true;
      try {
        res = await dispatchAgyCommand(['config', 'grant', folderPath]);
      } finally {
        state.isAutomating = false;
      }
      if (res && res.ok) {
        appendTerminalOutput(`Path successfully authorized.`);
        els.addWorkspaceInput.value = '';
        syncWorkspaceForm();
        closeDialog(els.addWorkspaceModal, els.addWorkspaceBtn);
        await refreshBypassData();
      } else {
        appendTerminalOutput(`Failed to authorize path:\n${res?.stderr || 'Unknown error'}`);
        syncWorkspaceForm();
        showBusinessAlert(`Failed to authorize workspace path: ${res?.stderr || 'Error'}`);
      }
    });
  }

  // Change listener on session dropdown
  if (els.sessionSelect) {
    els.sessionSelect.addEventListener('change', () => {
      state.activeSession = els.sessionSelect.value;
      loadSessionDetails(state.activeSession, state.activeApp);
    });
  }

  // Handle session prompt form submission
  if (els.promptForm) {
    els.promptForm.addEventListener('submit', async (e) => {
      e.preventDefault();
      const promptText = els.promptInput.value.trim();
      if (!promptText || !state.activeSession || !state.activeWorkspace) return;

      els.promptInput.value = '';

      // Optimistic user bubble
      const feedBox = els.chatFeed;
      const userBubble = document.createElement('div');
      userBubble.className = 'feed-chat-bubble user';
      userBubble.innerHTML = `
        <span class="bubble-sender">USER</span>
        <div>${escapeHtml(promptText)}</div>
        <span class="bubble-time">just now</span>
      `;
      feedBox.appendChild(userBubble);
      feedBox.scrollTop = feedBox.scrollHeight;

      // Dispatch prompt CLI command (automatically uses activeApp & workspace context via agy)
      const expectedCount = await sessionEventCount(state.activeSession) + 2;
      let res = null;
      state.isAutomating = true;
      try {
        res = await dispatchAgyCommand(['session', 'prompt', state.activeSession, promptText]);
      } finally {
        state.isAutomating = false;
      }
      if (res && res.ok) {
        await waitForSessionEvents(state.activeSession, expectedCount, 120000)
          .catch((err) => appendTerminalOutput(`Session event projection is still catching up: ${err.message || err}`));
        await loadSessionDetails(state.activeSession, state.activeApp);
      } else {
        const errBubble = document.createElement('div');
        errBubble.className = 'feed-chat-bubble assistant';
        errBubble.innerHTML = `
          <span class="bubble-sender text-red">SYSTEM ERROR</span>
          <div>Failed to dispatch prompt command: ${res?.stderr || 'Unknown'}</div>
        `;
        feedBox.appendChild(errBubble);
      }
    });
  }

  // Create new session via [+] button
  if (els.newSessionBtn) {
    els.newSessionBtn.addEventListener('click', () => {
      if (!state.activeWorkspace) return;
      if (els.newSessionContext) {
        els.newSessionContext.textContent = `Workspace: ${state.activeWorkspace}. Agent: ${state.activeApp.toUpperCase()}.`;
      }
      if (els.newSessionPrompt) els.newSessionPrompt.value = '';
      syncNewSessionForm();
      openDialog(els.newSessionModal, els.newSessionPrompt);
    });
  }

  if (els.newSessionForm) {
    els.newSessionForm.addEventListener('submit', async (event) => {
      event.preventDefault();
      if (!state.activeWorkspace) return;
      const validation = validateNewSessionPrompt(els.newSessionPrompt?.value || '');
      setFormError(els.newSessionPrompt, els.newSessionError, validation.error);
      if (els.newSessionSubmit) els.newSessionSubmit.disabled = !validation.valid;
      if (!validation.valid) return;
      const prompt = validation.prompt;

      appendTerminalPrompt(`session create -p "${state.activeWorkspace}" "${prompt}"`);
      appendTerminalOutput(`Spawning new ${state.activeApp.toUpperCase()} workspace session...`);
      if (els.newSessionSubmit) els.newSessionSubmit.disabled = true;

      let res = null;
      state.isAutomating = true;
      try {
        res = await dispatchAgyCommand(['session', 'create', '-p', state.activeWorkspace, prompt]);
      } finally {
        state.isAutomating = false;
      }
      if (res && res.ok) {
        appendTerminalOutput(`Session created successfully.`);
        if (els.newSessionPrompt) els.newSessionPrompt.value = '';
        closeDialog(els.newSessionModal, els.newSessionBtn);
        const createdSessionId = String(res?.data?.session_id || res?.session_id || '').trim();
        if (createdSessionId) state.activeSession = createdSessionId;
        await waitForSessionRecord(createdSessionId, state.activeApp, state.activeWorkspace, 60000)
          .then((session) => {
            if (session?.id) state.activeSession = session.id;
          })
          .catch((err) => appendTerminalOutput(`Session projection is still catching up: ${err.message || err}`));
        await refreshSessions();
        if (state.activeSession) {
          await waitForSessionEvents(state.activeSession, 2, 120000)
            .catch((err) => appendTerminalOutput(`Session event projection is still catching up: ${err.message || err}`));
          await loadSessionDetails(state.activeSession, state.activeApp);
        }
      } else {
        syncNewSessionForm();
        appendTerminalOutput(`Failed to create session:\n${res?.stderr || ''}`);
        showBusinessAlert(`Failed to create session: ${res?.stderr || 'Error'}`);
      }
    });
  }

  // Legacy switch tab triggers to support E2E Playwright test
  els.appSwitchBtns.forEach(btn => {
    btn.addEventListener('click', () => {
      const targetApp = btn.dataset.app;
      state.activeApp = targetApp;

      // Update Root Class Theme
      els.root.className = `coding-agents-module theme-${targetApp}`;

      // Update Header values
      if (targetApp === 'antigravity') {
        els.activeAppTitle.textContent = 'Antigravity';
        els.activeAppDesc.textContent = 'DeepMind Agentic OS';
      } else if (targetApp === 'claude') {
        els.activeAppTitle.textContent = 'Claude Desktop';
        els.activeAppDesc.textContent = 'Anthropic Desktop CLI Client';
      } else if (targetApp === 'codex') {
        els.activeAppTitle.textContent = 'Codex Agent';
        els.activeAppDesc.textContent = 'OpenAI Custom Terminal Wrapper';
      }

      updateUI();
      refreshAllData();
    });
  });

  if (els.legacyRightTabSubscription) {
    els.legacyRightTabSubscription.addEventListener('click', () => {
      // Legacy trigger opens our settings modal overlay directly!
      openDialog(els.settingsModal, els.closeSettingsBtn);
    });
  }

  // Legacy Path form & Box redirect bindings
  if (els.legacyPathAuthForm) {
    els.legacyPathAuthForm.addEventListener('submit', async (e) => {
      e.preventDefault();
      const pVal = els.legacyNewPathInput.value.trim();
      if (!pVal) return;
      await dispatchAgyCommand(['config', 'grant', pVal]);
      els.legacyNewPathInput.value = '';
      await refreshBypassData();
    });
  }

  // Action Buttons (inside Settings Modal)
  els.root.addEventListener('click', async (e) => {
    const actionBtn = e.target.closest('[data-action]');
    if (!actionBtn) return;
    const action = actionBtn.dataset.action;

    if (action === 'refresh-diagnostics') {
      appendTerminalPrompt(`refresh diagnostics`);
      appendTerminalOutput(`Syncing statuses with host...`);
      await refreshAllData();
      appendTerminalOutput(`✔ Synchronized successfully.`);
    }

    if (action === 'install-provider') {
      appendTerminalPrompt(`install --apply`);
      appendTerminalOutput(`Installing or repairing ${state.activeApp.toUpperCase()} CLI on this machine...`);
      const res = await dispatchAgyCommand(['install', '--apply']);
      if (res && res.ok) {
        appendTerminalOutput(res.stdout || `Provider CLI is discoverable.`);
      } else {
        appendTerminalOutput(`❌ Install action failed:\n${res?.stderr || 'Unknown error'}`);
      }
      await refreshAllData();
    }

    if (action === 'start-app') {
      appendTerminalPrompt(`start app`);
      appendTerminalOutput(`🚀 Dispatching start command for ${state.activeApp.toUpperCase()}...`);
      const res = await dispatchAgyCommand(['start']);
      if (res && res.ok) {
        appendTerminalOutput(`✔ Launch signal confirmed by macOS.`);
      } else {
        appendTerminalOutput(`❌ Launch signal failed:\n${res?.stderr || 'Unknown error'}`);
      }
      await refreshAllData();
    }

    if (action === 'stop-app') {
      appendTerminalPrompt(`stop app`);
      appendTerminalOutput(`🛑 Dispatching quit and termination commands...`);
      const res = await dispatchAgyCommand(['stop']);
      if (res && res.ok) {
        appendTerminalOutput(`✔ Service terminated successfully.`);
      } else {
        appendTerminalOutput(`❌ Termination failed:\n${res?.stderr || 'Unknown error'}`);
      }
      await refreshAllData();
    }

    if (action === 'start-headless') {
      const portVal = els.diagActivePort?.value || '8083';
      appendTerminalPrompt(`start headless (port: ${portVal})`);
      appendTerminalOutput(`🚀 Dispatching headless daemon command...`);
      const res = await dispatchAgyCommand(['headless', '--daemon', '--port', portVal]);
      if (res && res.ok) {
        appendTerminalOutput(`✔ Headless background daemon successfully spawned.`);
      } else {
        appendTerminalOutput(`❌ Headless spawn failed:\n${res?.stderr || 'Unknown error'}`);
      }
      await refreshAllData();
    }
  });

  // Switch Toggle for permissions bypass
  if (els.bypassToggle) {
    els.bypassToggle.addEventListener('change', async () => {
      const isChecked = els.bypassToggle.checked;
      appendTerminalPrompt(`bypass permissions = ${isChecked}`);
      if (!state.activeWorkspace) {
        appendTerminalOutput(`Select a workspace before changing provider permissions.`);
        els.bypassToggle.checked = false;
        return;
      }
      const workspacePath = state.activeWorkspace;

      if (isChecked) {
        appendTerminalOutput(`Granting ${state.activeApp.toUpperCase()} access to ${workspacePath}...`);
        const res = await dispatchAgyCommand(['config', 'grant', workspacePath]);
        if (res && res.ok) {
          appendTerminalOutput(`Workspace permission granted on host: ${workspacePath}`);
        } else {
          appendTerminalOutput(`❌ Grant failed: ${res?.stderr || ''}`);
          els.bypassToggle.checked = false;
        }
      } else {
        appendTerminalOutput(`Revoking ${state.activeApp.toUpperCase()} access to ${workspacePath}...`);
        const res = await dispatchAgyCommand(['config', 'revoke', workspacePath]);
        if (res && res.ok) {
          appendTerminalOutput(`Revoked workspace permission: ${workspacePath}`);
        } else {
          appendTerminalOutput(`❌ Revoke failed: ${res?.stderr || ''}`);
          els.bypassToggle.checked = true;
        }
      }
      await refreshBypassData();
    });
  }

  // Subscription automated login submit
  if (els.subLoginForm) {
    els.subLoginForm.addEventListener('submit', async (e) => {
      e.preventDefault();

      state.isAutomating = true;
      els.btnTriggerSignup.disabled = true;
      els.authStatus.textContent = t('automating', 'AUTOMATING...');
      els.authStatus.className = 'card-status-indicator active';

      els.browserLogBox.innerHTML = '';
      appendBrowserLog(`Starting ${state.activeApp.toUpperCase()} provider authentication on the host...`);

      const res = await dispatchAgyCommand(['auth', 'start']);

      state.isAutomating = false;
      els.btnTriggerSignup.disabled = false;

      if (res && res.ok) {
        appendBrowserLog(res.stdout || 'Provider authentication flow started.');
        const diag = diagnosticsFromOutcome(res);
        state.diagnostics[state.activeApp] = { ...state.diagnostics[state.activeApp], ...diag };
        showBusinessAlert(t('authSuccessful', 'Successfully Authenticated!'));
      } else {
        appendBrowserLog(`Provider authentication could not be started.`, 'text-red');
        if (res?.stdout) appendBrowserLog(res.stdout);
        if (res?.stderr) appendBrowserLog(res.stderr, 'text-red');
        showBusinessAlert(`Authentication failed or timed out. Please verify provider setup.`);
      }
      await refreshAllData();
    });
  }
}

function subscribeProjectionUpdates() {
  const subscriptions = [];
  const subscribe = (collectionName, handler) => {
    const collection = getCollection(collectionName);
    const subscription = collection?.$?.subscribe?.(() => {
      scheduleProjectionRefresh(collectionName, handler);
    });
    if (subscription) subscriptions.push(subscription);
  };

  subscribe('coding_agent_workspace_grants', () => refreshBypassData());
  subscribe('coding_agent_sessions', () => {
    if (state.activeWorkspace) return refreshSessions();
    return undefined;
  });
  subscribe('coding_agent_events', () => {
    if (state.activeSession) return loadSessionDetails(state.activeSession, state.activeApp);
    return undefined;
  });
  return subscriptions;
}

function getCollection(collectionName) {
  return state.ctx?.db?.collection?.(collectionName) || null;
}

function scheduleProjectionRefresh(key, fn) {
  if (state.projectionTimers[key]) clearTimeout(state.projectionTimers[key]);
  state.projectionTimers[key] = setTimeout(async () => {
    delete state.projectionTimers[key];
    try {
      await fn();
    } catch (err) {
      console.warn(`[coding-agents] projection refresh failed for ${key}`, err);
    }
  }, 150);
}

function clearProjectionTimers() {
  Object.values(state.projectionTimers || {}).forEach((timer) => clearTimeout(timer));
  state.projectionTimers = {};
}

function openModal(modal, focusTarget) {
  if (!modal) return;
  modal.removeAttribute('hidden');
  requestAnimationFrame(() => focusTarget?.focus?.());
}

function closeModal(modal, restoreFocus) {
  if (!modal) return;
  modal.setAttribute('hidden', '');
  restoreFocus?.focus?.();
}

function setWorkspaceLoadState(status, error = '') {
  state.workspaceLoadState = status;
  state.workspaceLoadError = error;
  renderWorkspaces();
}

function createWorkspaceStateNode(status, error = '') {
  const wrap = document.createElement('div');
  wrap.className = `workspace-load-state ${status === 'error' ? 'error' : ''}`;

  const title = document.createElement('strong');
  const body = document.createElement('span');

  if (status === 'loading') {
    title.textContent = 'Loading workspaces...';
    body.textContent = 'Command-Bus und Workspace-Grants werden geprüft.';
  } else if (status === 'error') {
    title.textContent = 'Workspaces konnten nicht geladen werden';
    body.textContent = error || 'Backend oder Command-Bus antwortet nicht.';
    const retry = document.createElement('button');
    retry.type = 'button';
    retry.className = 'fibu-btn fibu-btn-secondary';
    retry.textContent = 'Erneut versuchen';
    retry.addEventListener('click', () => startInitialLoadWithTimeout());
    wrap.append(title, body, retry);
    return wrap;
  } else {
    title.textContent = 'No workspaces authorized yet';
    body.textContent = 'Add a workspace with an absolute path before creating sessions.';
  }

  wrap.append(title, body);
  return wrap;
}

function renderNoWorkspaceSelected(message) {
  state.activeWorkspace = null;
  state.activeSession = null;
  if (els.activeAppTitle) els.activeAppTitle.textContent = 'Coding Agent Workbench';
  if (els.activeAppDesc) els.activeAppDesc.textContent = 'Select a Workspace';
  if (els.newSessionBtn) els.newSessionBtn.disabled = true;
  if (els.sessionSelect) {
    els.sessionSelect.innerHTML = `<option value="">No workspace selected</option>`;
    els.sessionSelect.disabled = true;
  }
  if (els.promptInput) els.promptInput.disabled = true;
  if (els.promptSubmit) els.promptSubmit.disabled = true;
  if (els.chatFeed) {
    els.chatFeed.innerHTML = `<div class="workbench-empty-state"><strong>No workspace selected.</strong><span>${escapeHtml(message)}</span></div>`;
  }
}

function workspaceLoadErrorFromResult(result) {
  if (!result) return 'Command-Bus nicht verfügbar oder keine Antwort vom Backend.';
  const stderr = String(result.stderr || '').trim();
  if (stderr) return stderr.slice(0, 260);
  if (result.status && result.status !== 'completed') return `Command status: ${result.status}`;
  return 'Backend hat keine gültige Workspace-Antwort geliefert.';
}

function validateWorkspacePath(input) {
  const path = String(input || '').trim();
  if (!path) return { valid: false, path: '', error: 'Bitte einen absoluten Workspace-Pfad eingeben.' };
  if (!isAbsoluteWorkspacePath(path)) {
    return { valid: false, path, error: 'Workspace-Pfad muss absolut sein, z.B. /Users/name/project.' };
  }
  if (/[\n\r]/.test(path)) {
    return { valid: false, path, error: 'Workspace-Pfad darf nur eine Zeile enthalten.' };
  }
  return { valid: true, path, error: '' };
}

function isAbsoluteWorkspacePath(path) {
  return path.startsWith('/') || path.startsWith('~/') || /^[A-Za-z]:[\\/]/.test(path);
}

function validateNewSessionPrompt(input) {
  const prompt = String(input || '').trim();
  if (!prompt) return { valid: false, prompt: '', error: 'Bitte eine Startanweisung für die neue Session eingeben.' };
  if (prompt.length < 8) return { valid: false, prompt, error: 'Die Startanweisung ist zu kurz.' };
  return { valid: true, prompt, error: '' };
}

function syncWorkspaceFormState() {
  const validation = validateWorkspacePath(els.addWorkspaceInput?.value || '');
  setFormError(els.addWorkspaceInput, els.addWorkspaceError, validation.error);
  if (els.addWorkspaceSubmit) els.addWorkspaceSubmit.disabled = !validation.valid;
}

function syncNewSessionFormState() {
  const validation = validateNewSessionPrompt(els.newSessionPrompt?.value || '');
  setFormError(els.newSessionPrompt, els.newSessionError, validation.error);
  if (els.newSessionSubmit) els.newSessionSubmit.disabled = !validation.valid || !state.activeWorkspace;
}

function setFormError(inputEl, errorEl, message) {
  if (inputEl) inputEl.setAttribute('aria-invalid', message ? 'true' : 'false');
  if (!errorEl) return;
  errorEl.textContent = message || '';
  errorEl.hidden = !message;
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function escapeAttr(value) {
  return escapeHtml(value);
}

function cssEscape(value) {
  if (globalThis.CSS?.escape) return globalThis.CSS.escape(value);
  return String(value).replace(/["\\]/g, '\\$&');
}

function updateUI() {
  const diag = state.diagnostics[state.activeApp];

  // Connection Indicator
  const isOnline = diag.online;
  els.workbenchStatusDot.className = `status-dot ${isOnline ? 'online' : 'offline'}`;
  els.workbenchStatusText.textContent = isOnline ? t('connectionConnected') : t('connectionDisconnected');

  // Diagnostics Status Display (inside Modal)
  if (els.diagInstalled) {
    els.diagInstalled.textContent = diag.installed ? t('installed') : t('notInstalled');
    els.diagInstalled.className = `diag-val ${diag.installed ? 'text-glow-green' : 'text-red'}`;
  }

  if (els.diagElectronPid) els.diagElectronPid.textContent = diag.electronPid;
  if (els.diagResources) els.diagResources.textContent = diag.resources;
  if (els.diagUptime) els.diagUptime.textContent = diag.uptime;

  // Service indicator
  if (els.serviceStatus) {
    els.serviceStatus.textContent = isOnline ? t('active') : t('inactive');
    els.serviceStatus.className = `card-status-indicator ${isOnline ? 'active' : ''}`;
  }

  // Auth Status indicator
  const hasAuth = Boolean(diag.authorized || diag.authReady);
  if (els.authStatus) {
    els.authStatus.textContent = hasAuth ? t('authorized') : t('unauthorized');
    els.authStatus.className = `card-status-indicator ${hasAuth ? 'active' : ''}`;
  }
}

/* Data Synchronization & Refreshes */
async function refreshAllData() {
  if (state.isRefreshing) return;
  state.isRefreshing = true;

  try {
    await refreshDiagnosticsSilently();
    await refreshBypassData();
    await refreshSessions();
  } catch (err) {
    console.warn('[coding-agents] Sync failed: ', err);
  } finally {
    state.isRefreshing = false;
  }
}

async function refreshDiagnosticsSilently() {
  const apps = ['antigravity', 'claude', 'codex'];
  for (const app of apps) {
    const res = await dispatchAgyCommand(['status'], { app });
    if (res && res.ok) {
      state.diagnostics[app] = diagnosticsFromOutcome(res);
    } else {
      state.diagnostics[app] = { installed: false, electronPid: 'N/A', lsPid: 'N/A', port: 'N/A', resources: 'N/A', uptime: 'N/A', online: false };
    }

    // Refresh diag state indicator dots in app list
    const diagDot = els.root.querySelector(`#diag-dot-${app}`);
    const diagBadge = els.root.querySelector(`#diag-badge-${app}`);
    if (diagDot && diagBadge) {
      const online = state.diagnostics[app].online;
      diagDot.className = `status-dot ${online ? 'online' : 'offline'}`;
      diagBadge.textContent = online ? 'ONLINE' : 'OFFLINE';
    }
  }
  updateUI();
}

function parseDiagnosticsStdout(stdout) {
  const result = { electronPid: 'N/A', lsPid: 'N/A', port: 'N/A', resources: 'N/A', uptime: 'N/A' };

  // Regex parsing
  const epMatch = stdout.match(/Electron App:\s+RUNNING\s+\(PID:\s+(\d+)\)/);
  if (epMatch) result.electronPid = epMatch[1];

  const lsMatch = stdout.match(/Language Server:\s+RUNNING\s+\(PID:\s+(\d+)\)/);
  if (lsMatch) result.lsPid = lsMatch[1];

  const resMatch = stdout.match(/Resources:\s+([^\n]+)/);
  if (resMatch) result.resources = resMatch[1].trim();

  const upMatch = stdout.match(/Uptime:\s+([^\n]+)/);
  if (upMatch) result.uptime = upMatch[1].trim();

  const portMatch = stdout.match(/Active Port:\s+([^\n]+)/);
  if (portMatch) {
    result.port = portMatch[1].replace(/\x1B\[[0-9;]*[a-zA-Z]/g, '').trim();
  }

  return result;
}

function diagnosticsFromOutcome(outcome) {
  const data = outcome?.data || {};
  const auth = data.auth || {};
  const legacy = parseDiagnosticsStdout(outcome?.stdout || '');
  const installed = Boolean(data.installed ?? outcome?.ok ?? legacy.electronPid !== 'N/A' ?? false);
  const controllable = Boolean(data.controllable);
  const authReady = Boolean(auth.ready);
  const mode = data.mode || data.binary || legacy.port || 'N/A';
  return {
    installed,
    controllable,
    authorized: authReady,
    authReady,
    authStatus: auth.status || auth.state || (authReady ? 'ready' : 'unknown'),
    electronPid: data.app_installed ? 'installed' : legacy.electronPid,
    lsPid: data.binary ? 'available' : legacy.lsPid,
    port: mode || 'N/A',
    resources: data.label || data.provider || legacy.resources,
    uptime: data.version || legacy.uptime,
    online: controllable || authReady,
    mode,
    binary: data.binary || '',
  };
}

async function refreshBypassData() {
  let grants = await workspaceGrantsFromProjection(state.activeApp);
  let res = null;
  if (grants === null) {
    res = await dispatchAgyCommand(['config', 'get-grants']);
    grants = res && res.ok ? grantsFromOutcome(res) : null;
  }

  if (Array.isArray(grants)) {
    state.trustedPaths = grants;
    state.workspaceLoadState = 'ready';
    state.workspaceLoadError = '';

    const isBypassed = Boolean(state.activeWorkspace && grants.includes(state.activeWorkspace));
    if (els.bypassToggle) els.bypassToggle.checked = isBypassed;
    renderWorkspaces();
  } else {
    state.trustedPaths = [];
    if (els.bypassToggle) els.bypassToggle.checked = false;
    state.workspaceLoadState = 'error';
    state.workspaceLoadError = workspaceLoadErrorFromResult(res);
    renderWorkspaces();
  }
}

function parseGrantsStdout(stdout) {
  const lines = stdout.split('\n');
  const grants = [];
  lines.forEach(line => {
    if (line.includes('•') || line.includes('*')) {
      const clean = line
        .replace(/\x1B\[[0-9;]*[a-zA-Z]/g, '')
        .replace(/^.*?[•*]\s*/, '')
        .trim();
      if (clean) grants.push(clean);
    }
  });
  return grants;
}

function grantsFromOutcome(outcome) {
  const grants = outcome?.data?.grants;
  if (Array.isArray(grants)) return grants.filter(Boolean).map(String);
  return parseGrantsStdout(outcome?.stdout || '');
}

async function workspaceGrantsFromProjection(provider) {
  const docs = await readCollectionDocs('coding_agent_workspace_grants');
  if (docs === null) return null;
  return docs
    .filter((doc) => doc.provider === provider && doc.is_deleted !== true && doc.status !== 'revoked' && doc.active !== false)
    .map((doc) => String(doc.path || '').trim())
    .filter(Boolean);
}

async function readCollectionDocs(collectionName) {
  const collection = state.ctx?.db?.collection?.(collectionName);
  if (!collection) return null;
  if (typeof collection.find === 'function') {
    const query = collection.find();
    const docs = await query?.exec?.();
    if (Array.isArray(docs)) return docs.map(toPlainDoc);
  }
  if (typeof collection.toArray === 'function') {
    return (await collection.toArray()).map(toPlainDoc);
  }
  if (Array.isArray(collection.items)) return collection.items.map(toPlainDoc);
  return null;
}

function toPlainDoc(doc) {
  return typeof doc?.toJSON === 'function' ? doc.toJSON() : doc;
}

function renderWorkspaces() {
  const box = els.workspacesListBox;
  if (!box) return;
  box.innerHTML = '';

  if (state.workspaceLoadState === 'loading') {
    box.appendChild(createWorkspaceStateNode('loading'));
    renderNoWorkspaceSelected('Workspace-Daten werden geladen. Session-Aktionen bleiben deaktiviert.');
    return;
  }

  if (state.workspaceLoadState === 'error') {
    box.appendChild(createWorkspaceStateNode('error', state.workspaceLoadError));
    renderNoWorkspaceSelected('Workspaces konnten nicht geladen werden. Prüfe Backend/Command-Bus und versuche es erneut.');
    return;
  }

  // Extract workspaces paths (starts with / and contains no parenthesis)
  const workspaces = state.trustedPaths.filter(g => g.startsWith('/') && !g.includes('(') && !g.includes(')'));

  // Also mirror to legacy hooks for E2E validation if needed
  if (els.legacyTrustedPathsBox) {
    els.legacyTrustedPathsBox.innerHTML = '';
    workspaces.forEach(path => {
      const pBadge = document.createElement('div');
      pBadge.className = 'path-badge';
      pBadge.innerHTML = `
        <span class="path-text">${escapeHtml(path)}</span>
        <button class="btn-remove-path" data-path="${escapeAttr(path)}" aria-label="Remove workspace ${escapeAttr(path)}">&times;</button>
      `;
      pBadge.querySelector('.btn-remove-path').addEventListener('click', async () => {
        await dispatchAgyCommand(['config', 'revoke', path]);
        await refreshBypassData();
      });
      els.legacyTrustedPathsBox.appendChild(pBadge);
    });
  }

  if (workspaces.length === 0) {
    box.appendChild(createWorkspaceStateNode('empty'));
    renderNoWorkspaceSelected('Noch kein Workspace autorisiert. Öffne „Add workspace“ und gib einen absoluten Pfad an.');
    return;
  }

  // Populate dynamic workspace items
  workspaces.forEach(path => {
    const shortName = path.split('/').pop() || path;
    const el = document.createElement('div');
    el.className = `workspace-item ${state.activeWorkspace === path ? 'active' : ''}`;
    el.dataset.workspace = path;
    el.dataset.contextRecordId = path;
    el.dataset.contextRecordType = 'workspace';
    el.dataset.contextLabel = shortName;

    // Load active mapped engine or default
    const mappedApp = localStorage.getItem('workspace_agent_' + path) || 'antigravity';

    el.innerHTML = `
      <div class="workspace-info">
        <span class="workspace-name">${escapeHtml(shortName)}</span>
        <span class="workspace-path" title="${escapeAttr(path)}">${escapeHtml(path)}</span>
      </div>
      <div class="workspace-actions">
        <select class="workspace-agent-select" aria-label="Coding agent for ${escapeAttr(shortName)}" style="pointer-events: auto;">
          <option value="antigravity" ${mappedApp === 'antigravity' ? 'selected' : ''}>Antigravity</option>
          <option value="claude" ${mappedApp === 'claude' ? 'selected' : ''}>Claude</option>
          <option value="codex" ${mappedApp === 'codex' ? 'selected' : ''}>Codex</option>
        </select>
        <button type="button" class="btn-remove-workspace" title="Remove Workspace" aria-label="Remove workspace ${escapeAttr(path)}">&times;</button>
      </div>
    `;

    // Dropdown change mapping listener
    const select = el.querySelector('.workspace-agent-select');
    select.addEventListener('change', (evt) => {
      evt.stopPropagation();
      const newAgent = select.value;
      localStorage.setItem('workspace_agent_' + path, newAgent);

      // If this is the active workspace, switch theme immediately
      if (state.activeWorkspace === path) {
        state.activeApp = newAgent;
        els.root.className = `coding-agents-module theme-${state.activeApp}`;
        els.activeAppDesc.textContent = `Active Coding Agent: ${state.activeApp.toUpperCase()} (${state.activeWorkspace})`;
        updateUI();
        refreshSessions();
      }
    });

    // Revoke workspace click handler
    el.querySelector('.btn-remove-workspace').addEventListener('click', async (evt) => {
      evt.stopPropagation();
      appendTerminalPrompt(`config revoke "${path}"`);
      appendTerminalOutput(`Revoking permissions for workspace: ${path}...`);
      const res = await dispatchAgyCommand(['config', 'revoke', path]);
      if (res && res.ok) {
        appendTerminalOutput(`Workspace permissions revoked.`);
        if (state.activeWorkspace === path) {
          state.activeWorkspace = null;
        }
        await refreshBypassData();
      } else {
        appendTerminalOutput(`Failed to revoke permissions:\n${res?.stderr || ''}`);
        showBusinessAlert(`Failed to revoke workspace: ${res?.stderr || 'Error'}`);
      }
    });

    // Select workspace click handler
    el.addEventListener('click', () => {
      selectWorkspace(path, mappedApp);
    });

    box.appendChild(el);
  });

  // Default to selecting the first workspace if none selected yet
  if (!state.activeWorkspace && workspaces.length > 0) {
    const defaultPath = workspaces[0];
    const defaultApp = localStorage.getItem('workspace_agent_' + defaultPath) || 'antigravity';
    selectWorkspace(defaultPath, defaultApp);
  } else if (state.activeWorkspace) {
    // Keep active workspace styled
    const activeItem = box.querySelector(`[data-workspace="${cssEscape(state.activeWorkspace)}"]`);
    if (activeItem) {
      activeItem.classList.add('active');
    } else {
      state.activeWorkspace = null;
    }
  }
}

function selectWorkspace(path, app) {
  state.activeWorkspace = path;
  state.activeApp = app;

  // Set theme accent on root
  els.root.className = `coding-agents-module theme-${state.activeApp}`;

  // Update header titles
  els.activeAppTitle.textContent = state.activeWorkspace.split('/').pop() || 'Workspace';
  els.activeAppDesc.textContent = `Active Coding Agent: ${state.activeApp.toUpperCase()} (${state.activeWorkspace})`;

  // Enable controls
  els.newSessionBtn.disabled = false;

  // Render active outline
  els.workspacesListBox.querySelectorAll('.workspace-item').forEach(item => {
    item.classList.toggle('active', item.dataset.workspace === path);
  });

  updateUI();
  refreshSessions();
}

async function refreshSessions() {
  if (!state.activeWorkspace) {
    state.sessions = [];
    renderSessions();
    return;
  }
  const app = state.activeApp;
  const projected = await sessionsFromProjection(app, state.activeWorkspace);
  if (projected !== null) {
    state.sessions = projected;
  } else {
    const res = await dispatchAgyCommand(['session', 'list']);
    state.sessions = res && res.ok ? sessionsFromOutcome(res, app) : [];
  }
  renderSessions();
}

function parseSessionsStdout(stdout, app) {
  const lines = stdout.split('\n');
  const list = [];
  lines.forEach(line => {
    if (line.includes('|') && !line.includes('SHORT ID') && !line.includes('===')) {
      const cols = line.split('|').map(s => s.replace(/\x1B\[[0-9;]*[a-zA-Z]/g, '').trim());
      if (cols.length >= 4 && cols[0]) {
        list.push({
          shortId: cols[0],
          id: cols[1],
          updatedAt: cols[2],
          prompt: cols[3],
          app: app
        });
      }
    }
  });
  return list;
}

function sessionsFromOutcome(outcome, app) {
  const records = outcome?.data?.sessions;
  if (Array.isArray(records)) return records.map((record) => sessionFromRecord(record, app));
  return parseSessionsStdout(outcome?.stdout || '', app);
}

async function sessionsFromProjection(provider, workspaceRoot) {
  const docs = await readCollectionDocs('coding_agent_sessions');
  if (docs === null) return null;
  return docs
    .filter((doc) =>
      doc.provider === provider
      && doc.is_deleted !== true
      && String(doc.workspace_root || '') === String(workspaceRoot || '')
      && doc.status !== 'stopped'
    )
    .sort((a, b) => Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0))
    .map((record) => sessionFromRecord(record, provider));
}

function sessionFromRecord(record, app) {
  const id = String(record.session_id || record.id || '').trim();
  return {
    shortId: shortSessionId(id),
    id,
    updatedAt: formatRecordTime(record.updated_at_ms),
    prompt: record.last_prompt || record.title || '',
    app: record.provider || app,
    workspaceRoot: record.workspace_root || '',
    status: record.status || '',
  };
}

function shortSessionId(sessionId) {
  const compact = String(sessionId || '').replace(/^ca_[a-z]+_/, '');
  return compact.slice(0, 8) || String(sessionId || '').slice(0, 8);
}

function renderSessions() {
  const select = els.sessionSelect;
  if (!select) return;
  select.innerHTML = '';

  if (!state.activeWorkspace) {
    select.innerHTML = `<option value="">No workspace selected</option>`;
    select.disabled = true;
    if (els.newSessionBtn) els.newSessionBtn.disabled = true;
    if (els.promptInput) els.promptInput.disabled = true;
    if (els.promptSubmit) els.promptSubmit.disabled = true;
    renderNoWorkspaceSelected('Bitte zuerst einen Workspace auswählen oder autorisieren.');
    return;
  }

  // Mirror sessions to legacy selector for Playwright context if exists
  if (els.legacySessionsListBox) {
    els.legacySessionsListBox.innerHTML = '';
    state.sessions.forEach(sess => {
      const el = document.createElement('div');
      el.className = 'session-item-card';
      el.dataset.contextRecordId = sess.id;
      el.dataset.contextRecordType = 'session';
      el.dataset.contextLabel = sess.prompt && sess.prompt.length > 60 ? sess.prompt.substring(0, 60) + '…' : (sess.prompt || sess.id);
      el.innerHTML = `<div class="session-item-prompt">${escapeHtml(sess.prompt)}</div>`;
      el.addEventListener('click', () => {
        state.activeSession = sess.id;
        select.value = sess.id;
        loadSessionDetails(sess.id, state.activeApp);
      });
      els.legacySessionsListBox.appendChild(el);
    });
  }

  if (state.sessions.length === 0) {
    select.innerHTML = `<option value="">No Active Sessions</option>`;
    select.disabled = true;
    if (els.newSessionBtn) els.newSessionBtn.disabled = false;
    els.promptInput.disabled = true;
    els.promptSubmit.disabled = true;
    els.chatFeed.innerHTML = `<div class="workbench-empty-state"><strong>No active sessions for this workspace.</strong><span>Use "+ New Session" to create one with an initial instruction.</span></div>`;
    return;
  }

  // Populate select dropdown
  state.sessions.forEach(sess => {
    const opt = document.createElement('option');
    opt.value = sess.id;
    const displayPrompt = sess.prompt.length > 50 ? sess.prompt.substring(0, 50) + '...' : sess.prompt;
    opt.textContent = `[${sess.shortId}] ${displayPrompt}`;
    if (state.activeSession === sess.id) {
      opt.selected = true;
    }
    select.appendChild(opt);
  });

  select.disabled = false;
  if (els.newSessionBtn) els.newSessionBtn.disabled = false;

  // Set default active session
  const exists = state.sessions.some(s => s.id === state.activeSession);
  if (!exists) {
    state.activeSession = state.sessions[0].id;
  }
  select.value = state.activeSession;

  loadSessionDetails(state.activeSession, state.activeApp);
}

async function loadSessionDetails(sessionId, app) {
  const feedBox = els.chatFeed;
  if (!feedBox) return;

  if (!sessionId) {
    feedBox.innerHTML = `<div class="empty-list-placeholder">No session active.</div>`;
    els.promptInput.disabled = true;
    els.promptSubmit.disabled = true;
    return;
  }

  els.promptInput.disabled = false;
  els.promptSubmit.disabled = false;

  feedBox.innerHTML = `<div class="empty-list-placeholder" style="animation: pulse-glow 1s infinite alternate;">Retrieving session records from SQLite database...</div>`;

  let elements = await sessionEventsFromProjection(sessionId);
  let res = null;
  if (elements === null) {
    res = await dispatchAgyCommand(['session', 'get', sessionId]);
    elements = res && res.ok ? sessionEventsFromOutcome(res) : null;
  }
  feedBox.innerHTML = '';

  if (Array.isArray(elements)) {
    if (elements.length === 0) {
      feedBox.innerHTML = `<div class="empty-list-placeholder">No conversation history recorded in this session.</div>`;
      return;
    }

    elements.forEach(item => {
      if (item.type === 'user') {
        const el = document.createElement('div');
        el.className = 'feed-chat-bubble user';
        el.innerHTML = `
          <span class="bubble-sender">USER</span>
          <div>${escapeHtml(item.text)}</div>
          <span class="bubble-time">${escapeHtml(item.time)}</span>
        `;
        feedBox.appendChild(el);
      } else if (item.type === 'assistant') {
        const el = document.createElement('div');
        el.className = 'feed-chat-bubble assistant';
        el.innerHTML = `
          <span class="bubble-sender">${escapeHtml(app.toUpperCase())} ASSISTANT</span>
          <div>${escapeHtml(item.text)}</div>
          <span class="bubble-time">${escapeHtml(item.time)}</span>
        `;
        feedBox.appendChild(el);
      } else if (item.type === 'tool') {
        const el = document.createElement('div');
        el.className = 'feed-tool-log';
        el.innerHTML = `
          <span class="tool-log-indicator ${escapeAttr(item.status)}">${item.status === 'success' ? 'OK' : 'FAIL'}</span>
          <span class="tool-log-text">Tool Run: <span class="tool-name-highlight">${escapeHtml(item.name)}</span></span>
        `;
        feedBox.appendChild(el);
      }
    });

    feedBox.scrollTop = feedBox.scrollHeight;
  } else {
    feedBox.innerHTML = `<div class="empty-list-placeholder text-red">Failed to read SQLite logs from host: ${escapeHtml(res?.stderr || 'Timeout')}</div>`;
  }
}

function parseSessionGetStdout(stdout) {
  const lines = stdout.split('\n');
  const items = [];
  lines.forEach(line => {
    const cleanLine = line.replace(/\x1B\[[0-9;]*[a-zA-Z]/g, '').trim();
    if (!cleanLine) return;

    const userMatch = cleanLine.match(/\[([^\]]+)\]\s+(?:👤\s*)?User:\s*(.*)/i);
    if (userMatch) {
      items.push({ type: 'user', time: userMatch[1], text: userMatch[2] });
      return;
    }

    const astMatch = cleanLine.match(/\[([^\]]+)\]\s+(?:🤖\s*)?Assistant:\s*(.*)/i);
    if (astMatch) {
      items.push({ type: 'assistant', time: astMatch[1], text: astMatch[2] });
      return;
    }

    const toolMatch = cleanLine.match(/(✔|✖|OK|FAIL)\s+Tool Run:\s*([^\x1b]+)/i);
    if (toolMatch) {
      const isSuccess = toolMatch[1] === '✔' || toolMatch[1].toUpperCase() === 'OK';
      const toolName = toolMatch[2].replace(/\x1B\[[0-9;]*[a-zA-Z]/g, '').trim();
      items.push({
        type: 'tool',
        status: isSuccess ? 'success' : 'fail',
        name: toolName
      });
      return;
    }
  });
  return items;
}

function sessionEventsFromOutcome(outcome) {
  const records = outcome?.data?.events;
  if (Array.isArray(records)) return records.map(eventFromRecord);
  return parseSessionGetStdout(outcome?.stdout || '');
}

async function sessionEventsFromProjection(sessionId) {
  const docs = await readCollectionDocs('coding_agent_events');
  if (docs === null) return null;
  return docs
    .filter((doc) => doc.session_id === sessionId && doc.is_deleted !== true)
    .sort((a, b) => Number(a.created_at_ms || 0) - Number(b.created_at_ms || 0))
    .map(eventFromRecord);
}

async function sessionEventCount(sessionId) {
  const docs = await readCollectionDocs('coding_agent_events');
  if (!Array.isArray(docs)) return 0;
  return docs.filter((doc) => doc.session_id === sessionId && doc.is_deleted !== true).length;
}

async function waitForSessionRecord(sessionId, provider, workspaceRoot, timeoutMs) {
  return waitForProjection(async () => {
    const sessions = await sessionsFromProjection(provider, workspaceRoot);
    if (!Array.isArray(sessions)) return { ok: false, sessions: null };
    const match = sessionId
      ? sessions.find((session) => session.id === sessionId)
      : sessions[0];
    return { ok: Boolean(match), value: match, count: sessions.length };
  }, timeoutMs, 'session projection');
}

async function waitForSessionEvents(sessionId, minCount, timeoutMs) {
  return waitForProjection(async () => {
    const docs = await readCollectionDocs('coding_agent_events');
    const events = Array.isArray(docs)
      ? docs.filter((doc) => doc.session_id === sessionId && doc.is_deleted !== true)
      : [];
    const assistantCount = events.filter((doc) => String(doc.role || '').toLowerCase() === 'assistant').length;
    return {
      ok: events.length >= minCount && assistantCount >= 1,
      value: events,
      count: events.length,
      assistantCount,
    };
  }, timeoutMs, 'session event projection');
}

async function waitForProjection(predicate, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await predicate();
    if (last?.ok) return last.value;
    await delay(250);
  }
  throw new Error(`${label} timed out: ${JSON.stringify(last)}`);
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function eventFromRecord(record) {
  const role = String(record.role || 'assistant').toLowerCase();
  if (role === 'tool') {
    return {
      type: 'tool',
      status: record.status === 'completed' || record.status === 'success' ? 'success' : 'fail',
      name: record.text || 'tool',
      time: formatRecordTime(record.created_at_ms),
    };
  }
  return {
    type: role === 'assistant' ? 'assistant' : 'user',
    text: record.text || '',
    status: record.status || '',
    time: formatRecordTime(record.created_at_ms),
  };
}

function formatRecordTime(value) {
  const ms = Number(value || 0);
  if (!Number.isFinite(ms) || ms <= 0) return '';
  return new Date(ms).toLocaleString();
}

/* Dispatch command through RxDB sync */
function buildAgyCommandArgs(args, app = state.activeApp) {
  return ['--app', app].concat(args);
}

function buildCodingAgentCommand(args, app = state.activeApp, context = {}) {
  const provider = app;
  const [command, subcommand] = args;
  const payload = { provider };

  if (command === 'status') return { commandType: 'ctox.coding_agent.status', payload };
  if (command === 'install') {
    return {
      commandType: 'ctox.coding_agent.install',
      payload: {
        ...payload,
        apply: args.includes('--apply') || args.includes('--yes') || args.includes('--confirm')
      }
    };
  }
  if (command === 'start' || command === 'stop' || command === 'headless') {
    return { commandType: `ctox.coding_agent.lifecycle.${command}`, payload: { ...payload, args: args.slice(1) } };
  }
  if (command === 'signup' || command === 'login') return { commandType: 'ctox.coding_agent.auth.start', payload };
  if (command === 'auth') {
    const authAction = subcommand === 'status' ? 'status' : 'start';
    return { commandType: `ctox.coding_agent.auth.${authAction}`, payload };
  }
  if (command === 'config' || command === 'workspace') {
    if (subcommand === 'get-grants' || subcommand === 'list') {
      return { commandType: 'ctox.coding_agent.workspace.list', payload };
    }
    if (subcommand === 'grant' || subcommand === 'revoke') {
      const path = parseWorkspaceCommandPath(args.slice(2));
      return { commandType: `ctox.coding_agent.workspace.${subcommand}`, payload: { ...payload, path } };
    }
  }
  if (command === 'session') {
    if (subcommand === 'list') {
      return { commandType: 'ctox.coding_agent.session.list', payload: { ...payload, workspace_root: context.workspace || state.activeWorkspace || '' } };
    }
    if (subcommand === 'get') {
      return { commandType: 'ctox.coding_agent.session.get', payload: { ...payload, session_id: args[2] || '' } };
    }
    if (subcommand === 'stop') {
      return { commandType: 'ctox.coding_agent.session.stop', payload: { ...payload, session_id: args[2] || '' } };
    }
    if (subcommand === 'prompt') {
      return { commandType: 'ctox.coding_agent.session.prompt', payload: { ...payload, session_id: args[2] || '', prompt: args.slice(3).join(' ').trim() } };
    }
    if (subcommand === 'create') {
      const parsed = parseSessionCreateCommand(args.slice(2), context.workspace || state.activeWorkspace || '');
      return { commandType: 'ctox.coding_agent.session.create', payload: { ...payload, workspace_root: parsed.workspace, prompt: parsed.prompt } };
    }
  }
  return null;
}

function parseWorkspaceCommandPath(args) {
  if (args[0] === '--path' || args[0] === '-p') return args[1] || '';
  return args[0] || '';
}

function parseSessionCreateCommand(args, defaultWorkspace) {
  let workspace = defaultWorkspace;
  const prompt = [];
  for (let index = 0; index < args.length; index += 1) {
    const value = args[index];
    if (value === '-p' || value === '--project' || value === '--workspace') {
      workspace = args[index + 1] || workspace;
      index += 1;
    } else if (value === '--prompt' || value === '--message') {
      prompt.push(args[index + 1] || '');
      index += 1;
    } else {
      prompt.push(value);
    }
  }
  return { workspace, prompt: prompt.join(' ').trim() };
}

async function dispatchAgyCommand(args, options = {}) {
  if (!state.ctx?.commandBus?.dispatch) {
    console.warn('[coding-agents] No commandBus available on mount context!');
    return null;
  }

  const app = options.app || state.activeApp;
  const commandId = `cmd_coding_agent_${app}_${crypto.randomUUID()}`;
  const command = buildCodingAgentCommand(args, app, { workspace: options.workspace || state.activeWorkspace });
  if (!command) {
    return { ok: false, exit_code: -1, stdout: '', stderr: `Unsupported coding-agent command: ${args.join(' ')}` };
  }

  try {
    const waitTimeoutMs = codingAgentCommandWaitTimeoutMs(command.commandType);
    const dispatched = await state.ctx.commandBus.dispatch({
      id: commandId,
      module: 'coding-agents',
      command_type: command.commandType,
      record_id: commandId,
      inbound_channel: 'business_os.coding_agents',
      payload: command.payload,
      wait_timeout_ms: waitTimeoutMs,
      client_context: {
        source: 'business-os-coding-agents',
        module: 'coding-agents',
        module_id: 'coding-agents',
        app_id: 'coding-agents',
        source_module: 'coding-agents',
        action: command.commandType,
        target: 'external-agent',
        external_provider: command.payload?.provider || app,
        workspace_root: command.payload?.workspace_root || command.payload?.path || options.workspace || state.activeWorkspace || '',
        session_id: command.payload?.session_id || state.activeSession || '',
        surface: 'coding-agents.module.command',
      }
    });

    return commandOutcome(dispatched) || dispatched;
  } catch (err) {
    console.warn('[coding-agents] Command dispatch failed: ', err);
    return { ok: false, exit_code: -1, stdout: '', stderr: String(err.message || err) };
  }
}

function codingAgentCommandWaitTimeoutMs(commandType) {
  return commandType === 'ctox.coding_agent.session.create'
    || commandType === 'ctox.coding_agent.session.prompt'
    ? 10 * 60 * 1000
    : undefined;
}

function commandOutcome(result) {
  if (!result) return null;
  if (result.payload?.outcome) return result.payload.outcome;
  if (result.result?.outcome) return result.result.outcome;
  if (result.result?.ok !== undefined || result.result?.exit_code !== undefined) return result.result;
  if (result.outcome?.ok !== undefined || result.outcome?.exit_code !== undefined) return result.outcome;
  return null;
}

/* Terminal Helpers */
function appendTerminalPrompt(cmd) {
  // Backwards compatibility if terminal logs are tailed to mock logs
  console.log(`${state.activeApp}$`, cmd);
}

function appendTerminalOutput(text, cssClass = '') {
  console.log(text);
}

function appendBrowserLog(text, cssClass = '') {
  if (els.browserLogBox) {
    const lineEl = document.createElement('span');
    lineEl.className = `terminal-line ${cssClass}`;
    lineEl.textContent = text;
    els.browserLogBox.appendChild(lineEl);
    els.browserLogBox.scrollTop = els.browserLogBox.scrollHeight;
  }
}

function showBusinessAlert(msg) {
  alert(msg);
}

export const __codingAgentsTestHooks = {
  parseDiagnosticsStdout,
  parseGrantsStdout,
  parseSessionsStdout,
  parseSessionGetStdout,
  validateWorkspacePath,
  validateNewSessionPrompt,
  workspaceLoadErrorFromResult,
  buildAgyCommandArgs,
  buildCodingAgentCommand,
  startDiagnosticsAutoRefresh,
  codingAgentCommandWaitTimeoutMs,
  diagnosticsFromOutcome,
  grantsFromOutcome,
  sessionsFromOutcome,
  sessionEventsFromOutcome,
  shortSessionId,
  escapeHtml
};
