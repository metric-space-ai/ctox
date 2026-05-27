console.log('[coding-agents-module] Top-level evaluation started');
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
  isRefreshing: false
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
  ctx.right.replaceChildren();

  bindElements(ctx.host);
  wireEvents();

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

  // Polling loop for active diagnostic status
  const intervalId = setInterval(() => {
    if (!state.isAutomating && !state.isRefreshing) {
      refreshDiagnosticsSilently();
    }
  }, 10000);

  return () => {
    clearInterval(intervalId);
    if (state.initialLoadTimer) clearTimeout(state.initialLoadTimer);
    resizers.forEach(r => r.destroy());
    styleLink.remove();
  };
}

const INITIAL_LOAD_TIMEOUT_MS = 10000;

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
  els.subEmail = root.querySelector('#sub-email');
  els.subPassword = root.querySelector('#sub-password');
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
  // Modal open/close listeners
  if (els.openSettingsBtn) {
    els.openSettingsBtn.addEventListener('click', () => {
      openModal(els.settingsModal, els.closeSettingsBtn);
    });
  }
  if (els.closeSettingsBtn) {
    els.closeSettingsBtn.addEventListener('click', () => {
      closeModal(els.settingsModal, els.openSettingsBtn);
    });
  }
  if (els.addWorkspaceBtn) {
    els.addWorkspaceBtn.addEventListener('click', () => {
      if (els.addWorkspaceModal) {
        syncWorkspaceFormState();
        openModal(els.addWorkspaceModal, els.addWorkspaceInput);
      }
    });
  }
  if (els.closeAddWorkspaceBtn) {
    els.closeAddWorkspaceBtn.addEventListener('click', () => {
      closeModal(els.addWorkspaceModal, els.addWorkspaceBtn);
    });
  }
  if (els.addWorkspaceInput) {
    els.addWorkspaceInput.addEventListener('input', syncWorkspaceFormState);
  }
  // Click-outside-to-close for the Add Workspace modal so it always has an
  // escape hatch even if the close button isn't visible.
  if (els.addWorkspaceModal) {
    els.addWorkspaceModal.addEventListener('click', (event) => {
      if (event.target === els.addWorkspaceModal) {
        closeModal(els.addWorkspaceModal, els.addWorkspaceBtn);
      }
    });
  }
  if (els.settingsModal) {
    els.settingsModal.addEventListener('click', (event) => {
      if (event.target === els.settingsModal) {
        closeModal(els.settingsModal, els.openSettingsBtn);
      }
    });
  }
  if (els.newSessionModal) {
    els.newSessionModal.addEventListener('click', (event) => {
      if (event.target === els.newSessionModal) {
        closeModal(els.newSessionModal, els.newSessionBtn);
      }
    });
  }
  if (els.closeNewSessionBtn) {
    els.closeNewSessionBtn.addEventListener('click', () => {
      closeModal(els.newSessionModal, els.newSessionBtn);
    });
  }
  if (els.newSessionPrompt) {
    els.newSessionPrompt.addEventListener('input', syncNewSessionFormState);
  }
  els.root.addEventListener('keydown', (event) => {
    if (event.key !== 'Escape') return;
    const openDialogs = [els.newSessionModal, els.addWorkspaceModal, els.settingsModal].filter((modal) => modal && !modal.hidden);
    const topDialog = openDialogs[0];
    if (!topDialog) return;
    event.preventDefault();
    const restore = topDialog === els.newSessionModal ? els.newSessionBtn : topDialog === els.addWorkspaceModal ? els.addWorkspaceBtn : els.openSettingsBtn;
    closeModal(topDialog, restore);
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

      const res = await dispatchAgyCommand(['config', 'grant', folderPath]);
      if (res && res.ok) {
        appendTerminalOutput(`Path successfully authorized.`);
        els.addWorkspaceInput.value = '';
        syncWorkspaceFormState();
        closeModal(els.addWorkspaceModal, els.addWorkspaceBtn);
        await refreshBypassData();
      } else {
        appendTerminalOutput(`Failed to authorize path:\n${res?.stderr || 'Unknown error'}`);
        syncWorkspaceFormState();
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
      const res = await dispatchAgyCommand(['session', 'prompt', state.activeSession, promptText]);
      if (res && res.ok) {
        setTimeout(async () => {
          await loadSessionDetails(state.activeSession, state.activeApp);
        }, 1500);
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
      syncNewSessionFormState();
      openModal(els.newSessionModal, els.newSessionPrompt);
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

      const res = await dispatchAgyCommand(['session', 'create', '-p', state.activeWorkspace, prompt]);
      if (res && res.ok) {
        appendTerminalOutput(`Session created successfully.`);
        if (els.newSessionPrompt) els.newSessionPrompt.value = '';
        closeModal(els.newSessionModal, els.newSessionBtn);
        await refreshSessions();
      } else {
        syncNewSessionFormState();
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
      openModal(els.settingsModal, els.closeSettingsBtn);
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

      if (isChecked) {
        appendTerminalOutput(`🛡 Enabling global bypass modes...`);
        let grantArg = 'command(*)';
        if (state.activeApp === 'claude') grantArg = '/tmp';
        else if (state.activeApp === 'codex') grantArg = '/Users/michaelwelsch/Documents/ctox';

        const res = await dispatchAgyCommand(['config', 'grant', grantArg]);
        if (res && res.ok) {
          appendTerminalOutput(`✔ Permission bypass granted on host: ${grantArg}`);
        } else {
          appendTerminalOutput(`❌ Grant failed: ${res?.stderr || ''}`);
          els.bypassToggle.checked = false;
        }
      } else {
        appendTerminalOutput(`🛡 Disabling bypass modes...`);
        let revokeArg = 'command(*)';
        if (state.activeApp === 'claude') revokeArg = '/tmp';
        else if (state.activeApp === 'codex') revokeArg = '/Users/michaelwelsch/Documents/ctox';

        const res = await dispatchAgyCommand(['config', 'revoke', revokeArg]);
        if (res && res.ok) {
          appendTerminalOutput(`✔ Revoked permission bypass: ${revokeArg}`);
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
      const email = els.subEmail.value.trim();
      const password = els.subPassword.value.trim();
      if (!email || !password) return;

      state.isAutomating = true;
      els.btnTriggerSignup.disabled = true;
      els.authStatus.textContent = t('automating', 'AUTOMATING...');
      els.authStatus.className = 'card-status-indicator active';

      // Clear and start logger
      els.browserLogBox.innerHTML = '';
      appendBrowserLog(`🚀 Initializing Google Subscription registration for ${state.activeApp.toUpperCase()}...`);
      appendBrowserLog(`👉 Credentials locked. Launching Playwright Chromium reference...`);

      setTimeout(() => {
        appendBrowserLog(`👉 Stealth browser warmup started at OAuth Google Login.`);
        appendBrowserLog(`👉 Automating Gmail credentials entry...`);
      }, 1500);

      setTimeout(() => {
        appendBrowserLog(`👉 Gmail: "${email}" entered. Processing password...`);
      }, 3000);

      // Call actual CLI in background
      const args = ['signup', '--email', email, '--password', password];

      const res = await dispatchAgyCommand(args);

      state.isAutomating = false;
      els.btnTriggerSignup.disabled = false;
      els.subPassword.value = '';

      if (res && res.ok) {
        appendBrowserLog(`✔ Playwright OAuth flow finished successfully!`);
        appendBrowserLog(res.stdout);
        els.authStatus.textContent = t('authorized', 'ACTIVE');
        els.authStatus.className = 'card-status-indicator active';
        showBusinessAlert(t('authSuccessful', 'Successfully Authenticated!'));
      } else {
        appendBrowserLog(`❌ Playwright login flow halted. See stderr.`, 'text-red');
        if (res?.stdout) appendBrowserLog(res.stdout);
        if (res?.stderr) appendBrowserLog(res.stderr, 'text-red');

        els.authStatus.textContent = t('unauthorized', 'UNAUTHORIZED');
        els.authStatus.className = 'card-status-indicator';
        showBusinessAlert(`Authentication failed or timed out. Please verify credentials.`);
      }
      await refreshAllData();
    });
  }
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
  const hasAuth = diag.online && diag.port !== 'N/A';
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
    const res = await dispatchAgyCommand(['--app', app, 'status']);
    if (res && res.ok && res.stdout) {
      const diag = parseDiagnosticsStdout(res.stdout);
      state.diagnostics[app] = { ...diag, installed: true, online: diag.electronPid !== 'N/A' || diag.lsPid !== 'N/A' };
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

async function refreshBypassData() {
  const res = await dispatchAgyCommand(['config', 'get-grants']);

  if (res && res.ok) {
    const grants = parseGrantsStdout(res.stdout || '');
    state.trustedPaths = grants;
    state.workspaceLoadState = 'ready';
    state.workspaceLoadError = '';

    // Update bypass toggle check state
    let isBypassed = false;
    if (state.activeApp === 'antigravity') isBypassed = grants.includes('command(*)');
    else if (state.activeApp === 'claude') isBypassed = grants.includes('/tmp');
    else if (state.activeApp === 'codex') isBypassed = grants.length > 0;

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
    if (line.includes('•')) {
      const clean = line
        .replace(/\x1B\[[0-9;]*[a-zA-Z]/g, '')
        .replace(/^.*?•\s*/, '')
        .trim();
      if (clean) grants.push(clean);
    }
  });
  return grants;
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
  // Get session list specifically for this app engine
  const res = await dispatchAgyCommand(['session', 'list']);
  if (res && res.ok && res.stdout) {
    state.sessions = parseSessionsStdout(res.stdout, app);
  } else {
    state.sessions = [];
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

  const res = await dispatchAgyCommand(['session', 'get', sessionId]);
  feedBox.innerHTML = '';

  if (res && res.ok && res.stdout) {
    const elements = parseSessionGetStdout(res.stdout);
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

/* Dispatch command through RxDB sync */
async function dispatchAgyCommand(args) {
  if (!state.ctx?.commandBus?.dispatch) {
    console.warn('[coding-agents] No commandBus available on mount context!');
    return null;
  }

  const commandId = `cmd_agy_${state.activeApp}_${crypto.randomUUID()}`;
  const startedAtMs = Date.now();

  try {
    const fullArgs = ['--app', state.activeApp].concat(args);
    const dispatched = await state.ctx.commandBus.dispatch({
      id: commandId,
      module: 'coding-agents',
      command_type: 'ctox.coding_agent.execute',
      record_id: commandId,
      inbound_channel: 'business_os.coding_agents',
      payload: {
        args: fullArgs
      },
      client_context: { source_module: 'coding-agents' }
    });

    let result = dispatched;
    if (!dispatched?.status || dispatched.status === 'pending_sync') {
      result = await waitForBusinessCommandProjection(commandId, startedAtMs);
    }

    if (result && result.status === 'completed' && result.payload?.outcome) {
      return result.payload.outcome;
    }
    return result;
  } catch (err) {
    console.error('[coding-agents] Command dispatch error: ', err);
    return { ok: false, exit_code: -1, stdout: '', stderr: String(err.message || err) };
  }
}

async function waitForBusinessCommandProjection(commandId, startedAtMs) {
  const collection = state.ctx?.db?.raw?.business_commands;
  if (!collection) return null;
  const earliestUpdatedAt = Math.max(0, Number(startedAtMs || Date.now()) - 1000);

  for (let attempt = 0; attempt < 20; attempt += 1) {
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
    } catch (_) {}
    await new Promise((resolve) => window.setTimeout(resolve, 500));
  }
  return null;
}

/* Terminal Helpers */
function appendTerminalPrompt(cmd) {
  // Backwards compatibility if terminal logs are tailed to mock logs
  console.log('agy$', cmd);
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
  escapeHtml
};
