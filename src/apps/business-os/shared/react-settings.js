import { showBusinessConfirm } from './dialogs.js';

export async function openReactSettings({
  mount,
  modules = [],
  session = null,
  governance = null,
  syncConfig = null,
  sync = null,
  commandBus = null,
  db = null,
  initialTab = 'runtime',
  onAccount,
  onClose,
  onModulesChanged,
}) {
  mount.hidden = false;
  mount.replaceChildren();

  const body = document.createElement('div');
  body.className = 'drawer-body settings-drawer';
  mount.append(body);

  const user = session?.user || {};
  const role = resolveRole(session);
  const isAdmin = roleCanManage(role);
  const canOpenAdmin = isAdmin || role === 'founder';
  const settingsState = {
    tab: initialTab || 'runtime',
    commandStatus: '',
    runtimeSettings: null,
    runtimeLoading: false,
    users: null,
    canManageUsers: false,
    modules: Array.isArray(modules) ? modules : [],
    governance,
    templates: null,
    editingModuleId: '',
    channels: {
      accounts: [],
      wizard: null,
      step: null,
      provider: null,
      data: {},
      status: '',
    },
  };

  let channelsAccountsSub = null;
  const ensureChannelCollections = async () => {
    await Promise.allSettled([
      sync?.startCollection?.('communication_accounts'),
      sync?.startCollection?.('channel_pairing_state'),
    ]);
  };
  const ensureUserCollections = async () => {
    await Promise.allSettled([
      sync?.startCollection?.('business_users'),
    ]);
  };
  const ensureRuntimeCollections = async () => {
    await Promise.allSettled([
      sync?.startCollection?.('ctox_runtime_settings'),
    ]);
  };
  const ensureModuleCatalogCollections = async () => {
    await Promise.allSettled([
      sync?.startCollection?.('business_module_catalog'),
    ]);
  };
  const refreshUsers = async () => {
    try {
      await ensureUserCollections();
      const payload = await loadUsers({ db, session });
      settingsState.users = payload.users || [];
      settingsState.canManageUsers = payload.can_manage === true;
    } catch (error) {
      console.error('[settings/users] rxdb users load failed:', error);
      settingsState.users = [];
      settingsState.canManageUsers = false;
    }
    render();
  };
  const refreshChannelAccounts = async () => {
    try {
      settingsState.channels.accounts = await loadChannelAccountsFromRxdb(db);
    } catch (error) {
      console.error('[settings/channels] rxdb accounts load failed:', error);
    }
    if (settingsState.tab === 'channels') render();
  };
  const startChannelAccountsSub = () => {
    if (channelsAccountsSub || !db?.collection?.('communication_accounts')?.$) return;
    channelsAccountsSub = db.collection('communication_accounts').$.subscribe(() => {
      refreshChannelAccounts().catch(() => {});
    });
  };

  const refreshManagedModules = async () => {
    try {
      await ensureModuleCatalogCollections();
      const payload = await loadModules({ db });
      settingsState.modules = payload.modules || settingsState.modules;
      settingsState.governance = payload.governance || settingsState.governance;
    } catch (error) {
      settingsState.commandStatus = `Module konnten nicht geladen werden: ${error.message || error}`;
    }
    try {
      const payload = await loadTemplates({ db });
      settingsState.templates = payload.templates || [];
    } catch {
      settingsState.templates = [];
    }
    render();
  };

  const refreshRuntimeSettings = async () => {
    settingsState.runtimeLoading = true;
    render();
    try {
      await ensureRuntimeCollections();
      settingsState.runtimeSettings = await loadRuntimeSettings({ db });
      settingsState.commandStatus = '';
    } catch (error) {
      settingsState.commandStatus = `Runtime-Status konnte nicht geladen werden: ${error.message || error}`;
    }
    settingsState.runtimeLoading = false;
    render();
  };

  const render = () => {
    body.innerHTML = settingsTemplate({
      modules,
      managedModules: settingsState.modules,
      templates: settingsState.templates,
      session,
      syncConfig,
      user,
      role,
      isAdmin,
      canOpenAdmin,
      tab: settingsState.tab,
      commandStatus: settingsState.commandStatus,
      runtimeSettings: settingsState.runtimeSettings,
      runtimeLoading: settingsState.runtimeLoading,
      users: settingsState.users,
      canManageUsers: settingsState.canManageUsers,
      editingModuleId: settingsState.editingModuleId,
      governance: settingsState.governance,
      channels: settingsState.channels,
    });
    body.querySelector('[data-close-settings]')?.addEventListener('click', () => {
      try { channelsAccountsSub?.unsubscribe?.(); } catch {}
      channelsAccountsSub = null;
      if (settingsState.channels?.qrPoll) {
        clearInterval(settingsState.channels.qrPoll);
        settingsState.channels.qrPoll = null;
      }
      onClose?.();
    });
    body.querySelector('[data-open-account-settings]')?.addEventListener('click', onAccount);
    body.querySelectorAll('[data-settings-tab]').forEach((button) => {
      button.addEventListener('click', () => {
        // Leaving the channels tab? Cancel any in-flight QR polling.
        if (settingsState.tab === 'channels' && button.dataset.settingsTab !== 'channels') {
          if (settingsState.channels?.qrPoll) {
            clearInterval(settingsState.channels.qrPoll);
            settingsState.channels.qrPoll = null;
          }
        }
        settingsState.tab = button.dataset.settingsTab;
        settingsState.commandStatus = '';
        render();
        if (settingsState.tab === 'runtime' && !settingsState.runtimeSettings) {
          refreshRuntimeSettings();
        }
        if (settingsState.tab === 'admin' && settingsState.templates === null) {
          refreshManagedModules();
        }
        if (settingsState.tab === 'channels') {
          ensureChannelCollections().then(refreshChannelAccounts).catch(refreshChannelAccounts);
          startChannelAccountsSub();
        }
      });
    });
    wireChannelHandlers(body, settingsState, render, {
      commandBus,
      db,
      session,
      refreshChannelAccounts,
      ensureChannelCollections,
    });
    body.querySelector('[data-logout-settings]')?.addEventListener('click', () => {
      localStorage.removeItem('ctox.businessOs.sessionToken');
      localStorage.removeItem('ctox.businessOs.authHeader');
      localStorage.setItem('ctox.businessOs.loggedOut', '1');
      location.reload();
    });
    body.querySelectorAll('[data-settings-command]').forEach((button) => {
      button.addEventListener('click', async () => {
        if (!commandBus) return;
        button.disabled = true;
        settingsState.commandStatus = 'CTOX Task wird angelegt...';
        render();
        try {
          const command = settingsCommand(button.dataset.settingsCommand, body, { syncConfig });
          const result = await commandBus.dispatch(command);
          settingsState.commandStatus = `Task ${result.task_id || result.command_id || result.status} angelegt.`;
        } catch (error) {
          settingsState.commandStatus = String(error?.message || error);
        }
        render();
      });
    });
    body.querySelector('[data-runtime-save]')?.addEventListener('click', async () => {
      settingsState.commandStatus = 'Runtime/Auth wird gespeichert...';
      render();
      try {
        settingsState.runtimeSettings = await saveRuntimeSettings(
          runtimePayloadFromForm(body),
          { commandBus, db, session },
        );
        settingsState.commandStatus = 'Runtime/Auth gespeichert.';
      } catch (error) {
        settingsState.commandStatus = String(error?.message || error);
      }
      render();
    });
    body.querySelector('[data-runtime-refresh]')?.addEventListener('click', refreshRuntimeSettings);
    body.querySelector('[data-runtime-authorize-subscription]')?.addEventListener('click', async () => {
      const authWindow = window.open('about:blank', 'ctox-chatgpt-subscription');
      writeSubscriptionAuthWindow(
        authWindow,
        'ChatGPT Login wird vorbereitet',
        'CTOX speichert die Runtime-Einstellung und fordert die Login-URL an.',
      );
      settingsState.commandStatus = 'ChatGPT Login wird geöffnet...';
      render();
      try {
        const runtimePayload = runtimePayloadFromForm(body);
        settingsState.runtimeSettings = runtimeSettingsWithDraft(
          settingsState.runtimeSettings,
          runtimePayload,
        );
        await saveRuntimeSettings(runtimePayload, { commandBus, db, session });
        const payload = await startSubscriptionAuth({ commandBus, db, session });
        if (!payload.auth_url) throw new Error('CTOX hat keine Login-URL geliefert.');
        if (authWindow && !authWindow.closed) {
          authWindow.location.href = payload.auth_url;
        } else {
          window.location.href = payload.auth_url;
        }
        settingsState.commandStatus = 'ChatGPT Login geöffnet. Danach Status neu laden.';
        setTimeout(refreshRuntimeSettings, 3000);
        setTimeout(refreshRuntimeSettings, 9000);
      } catch (error) {
        writeSubscriptionAuthWindow(
          authWindow,
          'ChatGPT Login konnte nicht gestartet werden',
          String(error?.message || error),
          true,
        );
        settingsState.commandStatus = String(error?.message || error);
      }
      render();
    });
    body.querySelectorAll('[data-runtime-provider], [data-runtime-auth-mode]').forEach((control) => {
      control.addEventListener('change', () => {
        settingsState.runtimeSettings = runtimeSettingsWithDraft(
          settingsState.runtimeSettings,
          runtimePayloadFromForm(body),
        );
        settingsState.commandStatus = '';
        render();
      });
    });
    body.querySelector('[data-user-save]')?.addEventListener('click', async () => {
      const id = body.querySelector('[data-user-id]')?.value?.trim();
      const displayName = body.querySelector('[data-user-name]')?.value?.trim();
      const roleValue = body.querySelector('[data-user-role]')?.value || 'user';
      if (!id || !displayName) return;
      settingsState.commandStatus = 'Nutzer wird gespeichert...';
      render();
      try {
        const payload = await saveUser(
          { id, display_name: displayName, role: roleValue, active: true },
          { commandBus, db, session },
        );
        settingsState.users = normalizeUsersForSession(payload.users || settingsState.users, session);
        settingsState.canManageUsers = roleCanManage(resolveRole(session));
        await refreshUsers();
        settingsState.commandStatus = `Nutzer ${id} gespeichert.`;
      } catch (error) {
        settingsState.commandStatus = String(error?.message || error);
      }
      render();
    });
    body.querySelector('[data-module-refresh]')?.addEventListener('click', refreshManagedModules);
    body.querySelectorAll('[data-module-edit]').forEach((button) => {
      button.addEventListener('click', () => {
        settingsState.editingModuleId = button.dataset.moduleEdit || '';
        settingsState.commandStatus = '';
        render();
      });
    });
    body.querySelectorAll('[data-founder-save]').forEach((button) => {
      button.addEventListener('click', async () => {
        const moduleId = button.dataset.founderSave || '';
        const userId = body.querySelector(`[data-founder-user="${cssEscape(moduleId)}"]`)?.value?.trim() || '';
        if (!moduleId || !userId) return;
        settingsState.commandStatus = 'Founder-Zuordnung wird gespeichert...';
        render();
        try {
          settingsState.governance = await assignFounder(moduleId, userId, true, { commandBus, db, session });
          settingsState.commandStatus = `${userId} ist Founder fuer ${moduleId}.`;
        } catch (error) {
          settingsState.commandStatus = String(error?.message || error);
        }
        render();
      });
    });
    body.querySelectorAll('[data-module-release]').forEach((button) => {
      button.addEventListener('click', async () => {
        const moduleId = button.dataset.moduleRelease || '';
        settingsState.commandStatus = 'Modul-Version wird gespeichert...';
        render();
        try {
          settingsState.governance = await releaseModule(moduleId, { commandBus, db, session });
          settingsState.commandStatus = `Version fuer ${moduleId} gespeichert.`;
        } catch (error) {
          settingsState.commandStatus = String(error?.message || error);
        }
        render();
      });
    });
    body.querySelectorAll('[data-module-rollback]').forEach((button) => {
      button.addEventListener('click', async () => {
        const moduleId = button.dataset.moduleRollback || '';
        const versionId = body.querySelector(`[data-rollback-version="${cssEscape(moduleId)}"]`)?.value || '';
        if (!moduleId || !versionId) return;
        settingsState.commandStatus = 'Rollback wird angewendet...';
        render();
        try {
          settingsState.governance = await rollbackModule(moduleId, versionId, { commandBus, db, session });
          settingsState.commandStatus = `Rollback fuer ${moduleId} angewendet.`;
          await refreshManagedModules();
          await onModulesChanged?.();
        } catch (error) {
          settingsState.commandStatus = String(error?.message || error);
        }
        render();
      });
    });
    body.querySelector('[data-module-cancel]')?.addEventListener('click', () => {
      settingsState.editingModuleId = '';
      settingsState.commandStatus = '';
      render();
    });
    body.querySelector('[data-module-save]')?.addEventListener('click', async () => {
      const form = body.querySelector('[data-module-edit-form]');
      if (!form) return;
      const moduleId = form.dataset.moduleEditForm || '';
      const payload = modulePayloadFromForm(form, moduleId);
      settingsState.commandStatus = 'Modul wird gespeichert...';
      render();
      try {
        await saveModule(payload, { commandBus, db, session });
        settingsState.commandStatus = `Modul ${payload.id} gespeichert.`;
        settingsState.editingModuleId = '';
        await refreshManagedModules();
        await onModulesChanged?.();
      } catch (error) {
        settingsState.commandStatus = String(error?.message || error);
        render();
      }
    });
    body.querySelectorAll('[data-module-delete]').forEach((button) => {
      button.addEventListener('click', async () => {
        const moduleId = button.dataset.moduleDelete || '';
        if (!moduleId) return;
        const confirmed = await showBusinessConfirm(`Modul ${moduleId} wirklich löschen?`, {
          title: 'Modul löschen',
          confirmLabel: 'Löschen',
        });
        if (!confirmed) return;
        settingsState.commandStatus = 'Modul wird gelöscht...';
        render();
        try {
          await deleteModule(moduleId, { commandBus, db, session });
          settingsState.commandStatus = `Modul ${moduleId} gelöscht.`;
          await refreshManagedModules();
          await onModulesChanged?.();
        } catch (error) {
          settingsState.commandStatus = String(error?.message || error);
          render();
        }
      });
    });
    body.querySelector('[data-module-create]')?.addEventListener('click', async () => {
      const title = body.querySelector('[data-module-new-title]')?.value?.trim() || '';
      const id = body.querySelector('[data-module-new-id]')?.value?.trim() || slugify(title);
      const description = body.querySelector('[data-module-new-description]')?.value?.trim() || '';
      const templateId = body.querySelector('[data-module-new-template]')?.value || '';
      if (!id || !title) return;
      settingsState.commandStatus = 'Modul wird angelegt...';
      render();
      try {
        if (templateId) {
          await installTemplate({ templateId, moduleId: id, title }, { commandBus, db, session });
        } else {
          await saveModule({
            id,
            title,
            description,
            entry: `installed-modules/${slugify(id)}/index.html`,
            collections: ['business_commands'],
            layout: { shell: 'pane', center: 'module workspace' },
          }, { commandBus, db, session });
        }
        settingsState.commandStatus = `Modul ${id} angelegt.`;
        await refreshManagedModules();
        await onModulesChanged?.();
      } catch (error) {
        settingsState.commandStatus = String(error?.message || error);
        render();
      }
    });
  };

  render();
  refreshRuntimeSettings();
  if (settingsState.tab === 'channels') {
    ensureChannelCollections().then(refreshChannelAccounts).catch(refreshChannelAccounts);
    startChannelAccountsSub();
  }
  refreshUsers();
  if (settingsState.tab === 'admin' && canOpenAdmin) {
    refreshManagedModules();
  }
}

function settingsTemplate({
  modules,
  managedModules,
  templates,
  session,
  syncConfig,
  user,
  role,
  isAdmin,
  canOpenAdmin,
  tab,
  commandStatus,
  runtimeSettings,
  runtimeLoading,
  users,
  canManageUsers,
  editingModuleId,
  governance,
  channels,
}) {
  return `
    <header class="drawer-header-row settings-head">
      <div>
        <h2>CTOX Settings</h2>
        <p>${escapeHtml(session?.authenticated ? 'Verbunden mit dieser CTOX Instanz.' : 'Keine aktive CTOX Sitzung.')}</p>
      </div>
      <button class="icon-button" type="button" data-close-settings aria-label="Schließen">×</button>
    </header>

    <section class="settings-user-card">
      <div class="settings-avatar" aria-hidden="true">${escapeHtml(initials(user.display_name || user.id || 'CTOX'))}</div>
      <div>
        <strong>${escapeHtml(user.display_name || user.id || 'Nicht eingeloggt')}</strong>
        <span>${escapeHtml(user.id || 'keine Session')}</span>
      </div>
      <mark class="role-badge">${escapeHtml(role)}</mark>
    </section>

    <nav class="settings-tabs" aria-label="Settings Bereiche">
      ${tabButton('runtime', 'Runtime', tab)}
      ${tabButton('channels', 'Channels', tab)}
      ${tabButton('sync', 'Sync', tab)}
      ${tabButton('users', 'Nutzer', tab)}
      ${canOpenAdmin ? tabButton('admin', 'Module', tab) : ''}
    </nav>

    <div class="settings-scroll">
      ${tab === 'runtime' ? runtimePanel(isAdmin, runtimeSettings, runtimeLoading) : ''}
      ${tab === 'channels' ? channelsPanel(channels) : ''}
      ${tab === 'sync' ? syncPanel(syncConfig, isAdmin) : ''}
      ${tab === 'users' ? usersPanel(user, role, isAdmin, users, canManageUsers) : ''}
      ${tab === 'admin' && canOpenAdmin ? adminPanel(managedModules || modules, templates, editingModuleId, { isAdmin, role, user, governance }) : ''}
    </div>

    <footer class="settings-footer">
      <button class="text-button" type="button" data-open-account-settings>Account</button>
      <button class="text-button" type="button" data-logout-settings>Logout</button>
      ${commandStatus ? `<span class="settings-status">${escapeHtml(commandStatus)}</span>` : ''}
    </footer>
  `;
}

function runtimePanel(isAdmin, runtimeSettings, runtimeLoading) {
  const runtime = runtimeSettings?.runtime || {};
  const auth = runtimeSettings?.auth || {};
  const diagnostics = runtimeSettings?.diagnostics || {};
  const provider = runtime.provider || 'local';
  const authMode = normalizedRuntimeAuthMode(provider, auth.mode);
  const isLocalProvider = provider === 'local';
  const usesSubscription = provider === 'openai' && isSubscriptionMode(authMode);
  const usesApiKey = !isLocalProvider && !usesSubscription;
  const serviceNeedsAttention = Boolean(diagnostics.service_needs_attention);
  const authNeedsAttention = Boolean(diagnostics.auth_needs_attention);
  const canManage = Boolean(isAdmin && runtimeSettings?.can_manage !== false);
  return `
    <section class="settings-section">
      <header>
        <h3>Model Runtime</h3>
        <span>${escapeHtml(runtimeLoading ? 'Status wird gelesen.' : runtimeAuthSummary(provider, authMode, auth))}</span>
      </header>
      <div class="runtime-status-strip">
        ${runtimePill('Modelle', `${runtimeProviderLabel(provider)}${runtime.chat_model ? ` · ${runtime.chat_model}` : ''}`, false)}
        ${runtimePill('Autorisierung', runtimeAuthSummary(provider, authMode, auth), authNeedsAttention)}
        ${runtimePill('CTOX Service', diagnostics.service_message || 'Status unbekannt', serviceNeedsAttention)}
      </div>
      <div class="settings-grid">
        <label><span>Provider</span><select data-runtime-provider ${canManage ? '' : 'disabled'}>
          ${option('local', 'Local CTOX', provider)}
          ${option('openai', 'OpenAI', provider)}
          ${option('openrouter', 'OpenRouter', provider)}
          ${option('anthropic', 'Anthropic', provider)}
          ${option('minimax', 'MiniMax', provider)}
        </select></label>
        ${!isLocalProvider ? `
          <label><span>Autorisierung</span><select data-runtime-auth-mode ${canManage ? '' : 'disabled'}>
            ${option('api_key', 'API Key', authMode)}
            ${provider === 'openai' ? option('chatgpt_subscription', 'ChatGPT Subscription', authMode) : ''}
          </select></label>
        ` : ''}
        ${runtimeModelControl(provider, runtime.chat_model, canManage)}
        <label><span>Preset</span><select data-runtime-preset ${canManage ? '' : 'disabled'}>
          ${option('Quality', 'Quality', runtimePresetValue(runtime.preset))}
          ${option('Performance', 'Performance', runtimePresetValue(runtime.preset))}
        </select></label>
        <label><span>Context</span><select data-runtime-context ${canManage ? '' : 'disabled'}>
          ${option('128k', '128k', runtimeContextValue(runtime.context))}
          ${option('256k', '256k', runtimeContextValue(runtime.context))}
        </select></label>
        <label><span>Max Run</span><input data-runtime-timeout inputmode="numeric" value="${escapeAttr(runtime.max_run_secs || 1800)}" ${canManage ? '' : 'disabled'} /></label>
        ${usesApiKey ? `<label><span>${escapeHtml(auth.api_key_name || 'API Key')}</span><input data-runtime-api-key type="password" autocomplete="off" placeholder="${escapeAttr(auth.api_key_configured ? 'gespeichert - leer lassen' : 'API Key eingeben')}" ${canManage ? '' : 'disabled'} /></label>` : ''}
      </div>
      ${usesSubscription ? subscriptionStatus(auth, canManage) : ''}
      ${canManage ? `
        <div class="runtime-actions">
          <button class="text-button settings-primary" type="button" data-runtime-save>Runtime speichern</button>
          <button class="text-button" type="button" data-runtime-refresh>Status neu laden</button>
        </div>
      ` : ''}
    </section>
    <section class="settings-section">
      <header><h3>Queue Policy</h3><span>Operative Arbeit läuft über CTOX Tasks.</span></header>
      <div class="settings-grid is-one">
        <label><span>Founder Review</span><select data-policy-review ${isAdmin ? '' : 'disabled'}><option value="strict-founder-review">Externe Nachrichten immer prüfen</option><option value="internal-autonomy">Interne Tasks autonom</option></select></label>
      </div>
      ${isAdmin ? `<button class="text-button settings-primary" type="button" data-settings-command="policy">Policy prüfen lassen</button>` : ''}
    </section>
  `;
}

function syncPanel(syncConfig, isAdmin) {
  const urls = syncConfig?.signaling_urls || [];
  return `
    <section class="settings-section">
      <header><h3>Business OS Hosting</h3><span>App Server gehört zur CTOX Instanz.</span></header>
      <dl class="settings-kv">
        ${kv('App Hosting', syncConfig?.app_hosting || 'ctox_instance_webserver')}
        ${kv('Sync Mode', syncConfig?.sync_mode || 'p2p-first')}
        ${kv('Transport', syncConfig?.transport || 'webrtc')}
        ${kv('Peer Role', syncConfig?.peer_role || 'ctox_instance')}
        ${kv('Instance', syncConfig?.instance_id || '-')}
      </dl>
    </section>
    <section class="settings-section">
      <header><h3>WebRTC Signaling</h3><span>${escapeHtml(isAdmin ? 'Änderungen werden als CTOX Task angelegt.' : 'Nur lesbar.')}</span></header>
      <div class="settings-grid is-one">
        <label><span>Room</span><input data-sync-room value="${escapeAttr(syncConfig?.sync_room || '')}" ${isAdmin ? '' : 'disabled'} /></label>
        <label><span>Signaling URLs</span><textarea data-sync-signaling ${isAdmin ? '' : 'disabled'}>${escapeHtml(urls.join('\n'))}</textarea></label>
      </div>
      ${isAdmin ? `<button class="text-button settings-primary" type="button" data-settings-command="sync">Sync Konfiguration an CTOX geben</button>` : ''}
    </section>
  `;
}

function usersPanel(user, role, isAdmin, users, canManageUsers) {
  const rows = Array.isArray(users) && users.length ? users : [{
    id: user.id || '-',
    display_name: user.display_name || '-',
    role,
    active: true,
  }];
  return `
    <section class="settings-section">
      <header><h3>Aktive Sitzung</h3><span>${escapeHtml(roleDisplayName(role))} Session</span></header>
      <table class="settings-table">
        <tbody>
          <tr><th>User</th><td>${escapeHtml(user.display_name || user.id || '-')}</td></tr>
          <tr><th>ID</th><td>${escapeHtml(user.id || '-')}</td></tr>
          <tr><th>Rolle</th><td>${escapeHtml(roleDisplayName(role))}</td></tr>
        </tbody>
      </table>
    </section>
    <section class="settings-section">
      <header><h3>User Management</h3><span>${escapeHtml(canManageUsers ? 'Persistenter Business-OS User Store.' : 'Nur eigene Sitzung sichtbar.')}</span></header>
      <table class="settings-table">
        <thead><tr><th>User</th><th>Rolle</th><th>Status</th></tr></thead>
        <tbody>
          ${rows.map((row) => `
            <tr>
              <td>${escapeHtml(row.display_name || row.id)}</td>
              <td>${escapeHtml(roleDisplayName(row.role || 'user'))}</td>
              <td>${escapeHtml(row.active === false ? 'inaktiv' : 'aktiv')}</td>
            </tr>
          `).join('')}
        </tbody>
      </table>
      ${canManageUsers ? `
        <div class="settings-user-form">
          <input data-user-id placeholder="user-id" />
          <input data-user-name placeholder="Anzeigename" />
          <select data-user-role>
            <option value="user">User</option>
            <option value="founder">Founder</option>
            <option value="admin">Admin</option>
            <option value="chef">Chef</option>
          </select>
          <button class="text-button settings-primary" type="button" data-user-save>Nutzer speichern</button>
        </div>
      ` : `<p class="settings-note">Nutzerverwaltung ist für Admins sichtbar.</p>`}
    </section>
  `;
}

function adminPanel(modules, templates, editingModuleId, permissions) {
  const rows = (Array.isArray(modules) ? modules : [])
    .filter((mod) => canModifyModuleInSettings(mod, permissions));
  const templateRows = Array.isArray(templates) ? templates : [];
  return `
    <section class="settings-section">
      <header>
        <h3>Module verwalten</h3>
        <span>${escapeHtml(`${rows.length} aktiv`)}</span>
      </header>
      <table class="settings-table module-admin-table">
        <thead><tr><th>Modul</th><th>Typ</th><th>Aktion</th></tr></thead>
        <tbody>
	          ${rows.map((mod) => moduleRow(mod, editingModuleId, permissions)).join('')}
        </tbody>
      </table>
      ${editingModuleId ? moduleEditForm(rows.find((mod) => mod.id === editingModuleId)) : ''}
      <button class="text-button settings-primary" type="button" data-module-refresh>Module neu laden</button>
    </section>
	    ${permissions.isAdmin ? `<section class="settings-section">
	      <header><h3>Modul hinzufügen</h3><span>Blanko oder aus Template.</span></header>
      <div class="settings-grid is-one">
        <label><span>Template</span><select data-module-new-template>
          <option value="">Blankes Modul</option>
          ${templateRows.map((template) => `<option value="${escapeAttr(template.id)}">${escapeHtml(template.title || template.id)}</option>`).join('')}
        </select></label>
        <label><span>Modul ID</span><input data-module-new-id placeholder="sales-dashboard" /></label>
        <label><span>Titel</span><input data-module-new-title placeholder="Sales Dashboard" /></label>
        <label><span>Beschreibung</span><textarea data-module-new-description placeholder="Wofür dieses Modul genutzt wird."></textarea></label>
      </div>
	      <button class="text-button settings-primary" type="button" data-module-create>Modul hinzufügen</button>
	    </section>` : ''}
	    ${permissions.isAdmin ? `<section class="settings-section">
	      <header><h3>Inbound / Outbound</h3><span>Wie in der TUI als CTOX-gesteuerte Policy.</span></header>
      <div class="settings-grid is-one">
        <label><span>Inbound</span><select data-inbound-policy><option value="business-os">Business OS Commands</option><option value="founder">Founder Messages</option><option value="tickets">Tickets / Issues</option></select></label>
        <label><span>Outbound</span><select data-outbound-policy><option value="strict-founder-review">Strict Founder Review</option><option value="internal-autonomy">Internal Autonomy</option></select></label>
      </div>
	      <button class="text-button settings-primary" type="button" data-settings-command="routing">Routing Policy an CTOX geben</button>
	    </section>` : ''}
	  `;
}

function moduleRow(mod, editingModuleId, permissions) {
  const kind = moduleKind(mod);
  const canDelete = moduleCanDelete(mod);
  const releases = releasesForModule(permissions.governance, mod.id);
  const founders = foundersForModule(permissions.governance, mod.id);
  return `
    <tr ${editingModuleId === mod.id ? 'aria-current="true"' : ''}>
      <td>
        <strong>${escapeHtml(mod.title || mod.id)}</strong>
        <small>${escapeHtml(mod.id)}</small>
      </td>
	      <td>
	        ${escapeHtml(kind)}
	        ${founders.length ? `<small>Founder: ${founders.map((item) => escapeHtml(item.user_id)).join(', ')}</small>` : ''}
	      </td>
	      <td>
	        <div class="module-admin-actions">
	          <button class="text-button" type="button" data-module-edit="${escapeAttr(mod.id)}">Editieren</button>
	          <button class="text-button" type="button" data-module-release="${escapeAttr(mod.id)}">Version speichern</button>
	          ${releases.length ? `
	            <select data-rollback-version="${escapeAttr(mod.id)}">
	              ${releases.map((release) => `<option value="${escapeAttr(release.version_id)}">v${escapeHtml(release.version)} ${escapeHtml(release.status || '')}</option>`).join('')}
	            </select>
	            <button class="text-button" type="button" data-module-rollback="${escapeAttr(mod.id)}">Rollback</button>
	          ` : ''}
	          <button class="text-button" type="button" data-module-delete="${escapeAttr(mod.id)}" ${canDelete ? '' : 'disabled'}>Löschen</button>
	        </div>
	        ${permissions.isAdmin ? `
	          <div class="module-admin-actions">
	            <input data-founder-user="${escapeAttr(mod.id)}" placeholder="founder user-id" />
	            <button class="text-button" type="button" data-founder-save="${escapeAttr(mod.id)}">Founder zuweisen</button>
	          </div>
	        ` : ''}
	      </td>
	    </tr>
	  `;
}

function moduleEditForm(mod) {
  if (!mod) return '';
  return `
    <div class="module-admin-editor" data-module-edit-form="${escapeAttr(mod.id)}">
      <label><span>Titel</span><input data-module-title value="${escapeAttr(mod.title || '')}" /></label>
      <label><span>Beschreibung</span><textarea data-module-description>${escapeHtml(mod.description || '')}</textarea></label>
      <label><span>Entry</span><input data-module-entry value="${escapeAttr(mod.entry || '')}" ${moduleIsCore(mod) ? 'disabled' : ''} /></label>
      <label><span>Collections</span><textarea data-module-collections>${escapeHtml((mod.collections || []).join('\n'))}</textarea></label>
      <div class="module-admin-actions">
        <button class="text-button settings-primary" type="button" data-module-save>Speichern</button>
        <button class="text-button" type="button" data-module-cancel>Abbrechen</button>
      </div>
    </div>
  `;
}

function settingsCommand(type, root, { syncConfig }) {
  if (type === 'runtime') {
    return {
      module: 'ctox',
      type: 'ctox.runtime.switch',
      record_id: 'runtime-settings',
      payload: {
        model: root.querySelector('[data-runtime-model]')?.value,
        preset: root.querySelector('[data-runtime-preset]')?.value,
        context: root.querySelector('[data-runtime-context]')?.value,
        timeout_secs: Number(root.querySelector('[data-runtime-timeout]')?.value || 1800),
      },
      client_context: { module: 'ctox', surface: 'business-os-settings' },
    };
  }
  if (type === 'sync') {
    return {
      module: 'ctox',
      type: 'ctox.business_os.sync.configure',
      record_id: syncConfig?.instance_id || 'sync-settings',
      payload: {
        sync_room: root.querySelector('[data-sync-room]')?.value,
        signaling_urls: (root.querySelector('[data-sync-signaling]')?.value || '')
          .split(/\n+/).map((url) => url.trim()).filter(Boolean),
      },
      client_context: { module: 'ctox', surface: 'business-os-settings' },
    };
  }
  if (type === 'policy' || type === 'routing') {
    return {
      module: 'ctox',
      type: 'ctox.communication_policy.verify',
      record_id: 'communication-policy',
      payload: {
        review_policy: root.querySelector('[data-policy-review]')?.value,
        inbound_policy: root.querySelector('[data-inbound-policy]')?.value,
        outbound_policy: root.querySelector('[data-outbound-policy]')?.value,
      },
      client_context: { module: 'ctox', surface: 'business-os-settings' },
    };
  }
  const userAction = {
    'user-create': 'Nutzer anlegen',
    'user-role': 'Rolle ändern',
    'session-revoke': 'Session widerrufen',
  }[type] || type;
  return {
    module: 'ctox',
    type: `ctox.users.${type}`,
    record_id: 'users',
    payload: {
      instruction: `${userAction}: öffne die CTOX User- und Session-Verwaltung, prüfe Rollenrechte und bereite die Änderung für diese Business-OS-Instanz vor.`,
    },
    client_context: { module: 'ctox', surface: 'business-os-settings' },
  };
}

function tabButton(id, label, active) {
  return `<button type="button" data-settings-tab="${escapeAttr(id)}" ${id === active ? 'aria-current="page"' : ''}>${escapeHtml(label)}</button>`;
}

function option(value, label, selected) {
  return `<option value="${escapeAttr(value)}" ${String(selected || '').toLowerCase() === String(value).toLowerCase() ? 'selected' : ''}>${escapeHtml(label)}</option>`;
}

function normalizedRuntimeAuthMode(provider, mode) {
  if (String(provider || '').toLowerCase() === 'local') return 'local';
  const value = String(mode || '').toLowerCase();
  if (String(provider || '').toLowerCase() !== 'openai') return 'api_key';
  return isSubscriptionMode(value) ? 'chatgpt_subscription' : 'api_key';
}

function isSubscriptionMode(mode) {
  return ['chatgpt_subscription', 'subscription', 'codex_subscription', 'chatgpt'].includes(
    String(mode || '').trim().toLowerCase(),
  );
}

function runtimeProviderLabel(provider) {
  return {
    local: 'Local CTOX',
    openai: 'OpenAI',
    openrouter: 'OpenRouter',
    anthropic: 'Anthropic',
    minimax: 'MiniMax',
  }[String(provider || '').toLowerCase()] || provider || '-';
}

function runtimeAuthSummary(provider, authMode, auth) {
  if (String(provider || '').toLowerCase() === 'local') return 'nicht erforderlich';
  if (isSubscriptionMode(authMode)) {
    if (auth.subscription_session_configured) {
      return auth.subscription_account_email || 'ChatGPT Subscription autorisiert';
    }
    return 'ChatGPT Subscription nicht autorisiert';
  }
  return auth.api_key_configured
    ? `${auth.api_key_name || 'API Key'} gespeichert`
    : `${auth.api_key_name || 'API Key'} fehlt`;
}

function runtimeModelControl(provider, model, canManage) {
  const value = String(model || '');
  if (String(provider || '').toLowerCase() === 'local') {
    return `<label><span>Lokales Modell</span><input data-runtime-model value="${escapeAttr(value)}" placeholder="kein Modell aus Runtime gemeldet" ${canManage ? '' : 'disabled'} /></label>`;
  }
  const options = runtimeModelOptions(provider, value);
  return `<label><span>Chat Modell</span><select data-runtime-model ${canManage ? '' : 'disabled'}>
    ${options.map(([optionValue, label]) => option(optionValue, label, value)).join('')}
  </select></label>`;
}

function runtimeModelOptions(provider, current) {
  const byProvider = {
    openai: [
      ['gpt-5.5', 'gpt-5.5'],
      ['gpt-5.4', 'gpt-5.4'],
      ['gpt-5.4-mini', 'gpt-5.4-mini'],
      ['gpt-5.3-codex', 'gpt-5.3-codex'],
    ],
    openrouter: [
      ['openrouter/minimax/m2.7', 'openrouter/minimax/m2.7'],
    ],
    anthropic: [
      ['claude-opus-4-6', 'claude-opus-4-6'],
    ],
    minimax: [
      ['openrouter/minimax/m2.7', 'openrouter/minimax/m2.7'],
    ],
  };
  const options = byProvider[String(provider || '').toLowerCase()] || [];
  if (!current) return [['', 'Nicht gesetzt'], ...options];
  if (!options.some(([value]) => value.toLowerCase() === current.toLowerCase())) {
    return [[current, current], ...options];
  }
  return options;
}

function runtimeDiagnosticMessage(provider, authMode, auth, diagnostics) {
  const message = String(diagnostics?.message || '').trim();
  if (message.includes('wirkt konfiguriert') && isSubscriptionMode(authMode) && !auth.subscription_session_configured) {
    return 'ChatGPT Subscription ist noch nicht verbunden.';
  }
  if (message.includes('wirkt konfiguriert') && String(provider || '').toLowerCase() === 'local') {
    return 'Lokale CTOX Runtime ausgewählt; keine API-Autorisierung nötig.';
  }
  return message || 'Runtime-Status wird geladen.';
}

function subscriptionStatus(auth, canManage) {
  const configured = Boolean(auth.subscription_session_configured);
  const lines = [];
  if (auth.subscription_account_email) lines.push(kv('Account', auth.subscription_account_email));
  if (auth.subscription_plan) lines.push(kv('Plan', auth.subscription_plan));
  return `
    <div class="runtime-auth-status ${configured ? 'is-ok' : 'is-danger'}">
      <strong>${escapeHtml(configured ? 'ChatGPT Subscription verbunden' : 'ChatGPT Subscription verbinden')}</strong>
      <span>${escapeHtml(configured ? 'OpenAI Modelle können diese Subscription verwenden.' : 'Öffnet den ChatGPT Login und speichert die Subscription für OpenAI Modelle.')}</span>
      ${lines.length ? `<dl class="settings-kv">${lines.join('')}</dl>` : ''}
      ${canManage ? `<button class="text-button" type="button" data-runtime-authorize-subscription>${escapeHtml(configured ? 'Subscription erneuern' : 'Subscription verbinden')}</button>` : ''}
    </div>
  `;
}

function runtimePill(label, value, danger) {
  return `
    <div class="runtime-pill ${danger ? 'is-danger' : ''}">
      <span>${escapeHtml(label)}</span>
      <strong>${escapeHtml(value || '-')}</strong>
    </div>
  `;
}

function kv(key, value) {
  return `<div><dt>${escapeHtml(key)}</dt><dd>${escapeHtml(String(value || '-'))}</dd></div>`;
}

function runtimePayloadFromForm(root) {
  const provider = root.querySelector('[data-runtime-provider]')?.value || 'local';
  const authMode = normalizedRuntimeAuthMode(
    provider,
    root.querySelector('[data-runtime-auth-mode]')?.value || 'api_key',
  );
  return {
    provider,
    auth_mode: authMode,
    chat_model: root.querySelector('[data-runtime-model]')?.value || '',
    preset: runtimePresetValue(root.querySelector('[data-runtime-preset]')?.value),
    context: runtimeContextValue(root.querySelector('[data-runtime-context]')?.value),
    max_run_secs: Number(root.querySelector('[data-runtime-timeout]')?.value || 1800),
    api_key: authMode === 'api_key' ? (root.querySelector('[data-runtime-api-key]')?.value || '') : '',
  };
}

function runtimeSettingsWithDraft(current, draft) {
  const provider = draft.provider || 'local';
  const authMode = normalizedRuntimeAuthMode(provider, draft.auth_mode);
  return {
    ...(current || {}),
    runtime: {
      ...(current?.runtime || {}),
      provider,
      source: provider === 'local' ? 'local' : 'api',
      chat_model: draft.chat_model,
      preset: runtimePresetValue(draft.preset),
      context: runtimeContextValue(draft.context),
      max_run_secs: draft.max_run_secs,
    },
    auth: {
      ...(current?.auth || {}),
      mode: authMode,
      subscription_selected: authMode === 'chatgpt_subscription',
    },
  };
}


function resolveRole(session) {
  const user = session?.user || {};
  if (user.role) return normalizeRole(user.role);
  if (user.is_admin === true || user.id === 'local-dev') return 'admin';
  return session?.authenticated ? 'user' : 'guest';
}

function normalizeUsersForSession(users, session) {
  const rows = Array.isArray(users) ? users : [];
  const normalized = rows
    .map((user) => ({
      id: user.id || user.user_id || '',
      display_name: user.display_name || user.name || user.id || user.user_id || '',
      role: normalizeRole(user.role || 'user'),
      active: user.active !== false,
      created_at_ms: Number(user.created_at_ms || 0),
      updated_at_ms: Number(user.updated_at_ms || 0),
    }))
    .filter((user) => user.id);
  if (roleCanManage(resolveRole(session))) return normalized;
  const currentId = session?.user?.id || '';
  return normalized.filter((user) => user.id === currentId);
}

function normalizeRole(role) {
  const value = String(role || '').trim().toLowerCase().replace(/^business_os_/, '');
  if (value === 'owner') return 'chef';
  if (['chef', 'admin', 'founder', 'user'].includes(value)) return value;
  return 'user';
}

function roleCanManage(role) {
  return ['chef', 'admin'].includes(normalizeRole(role));
}

function roleDisplayName(role) {
  return { chef: 'Chef', admin: 'Admin', founder: 'Founder', user: 'User' }[normalizeRole(role)] || role;
}

function moduleKind(mod) {
  return moduleIsCore(mod) ? 'Core' : 'Installiert';
}

const SYSTEM_MODULE_IDS = new Set(['ctox', 'knowledge']);

function moduleIsCore(mod) {
  return SYSTEM_MODULE_IDS.has(mod?.id);
}

function moduleCanDelete(mod) {
  return Boolean(mod?.id) && !moduleIsCore(mod);
}

function canModifyModuleInSettings(mod, { isAdmin, role, user, governance }) {
  if (isAdmin) return true;
  if (normalizeRole(role) !== 'founder') return false;
  const assignments = governance?.founders?.[mod?.id] || [];
  return assignments.some((item) => item.user_id === user?.id && item.active !== false);
}

function foundersForModule(governance, moduleId) {
  return (governance?.founders?.[moduleId] || []).filter((item) => item.active !== false);
}

function releasesForModule(governance, moduleId) {
  return governance?.releases?.[moduleId] || [];
}

function modulePayloadFromForm(form, moduleId) {
  return {
    id: moduleId,
    title: form.querySelector('[data-module-title]')?.value?.trim() || moduleId,
    description: form.querySelector('[data-module-description]')?.value?.trim() || '',
    entry: form.querySelector('[data-module-entry]')?.value?.trim() || '',
    collections: (form.querySelector('[data-module-collections]')?.value || '')
      .split(/\n+/)
      .map((item) => item.trim())
      .filter(Boolean),
    layout: {},
  };
}

function slugify(value) {
  return String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

function initials(value) {
  return String(value || 'C')
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase())
    .join('') || 'C';
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

async function loadUsers({ db, session } = {}) {
  const coll = db?.collection?.('business_users');
  if (!coll) {
    return {
      ok: true,
      can_manage: roleCanManage(resolveRole(session)),
      users: [],
    };
  }
  const docs = await coll.find().exec();
  const users = docs
    .map((doc) => doc.toJSON())
    .filter((user) => user && user._deleted !== true && user.is_deleted !== true)
    .map((user) => ({
      id: user.id || user.user_id,
      display_name: user.display_name || user.name || user.id || user.user_id,
      role: user.role || 'user',
      active: user.active !== false,
      created_at_ms: Number(user.created_at_ms || 0),
      updated_at_ms: Number(user.updated_at_ms || 0),
    }));
  return {
    ok: true,
    can_manage: roleCanManage(resolveRole(session)),
    users: normalizeUsersForSession(users, session),
  };
}

async function saveUser(payload, { commandBus, db, session } = {}) {
  const command = await dispatchModuleCommand({
    commandBus,
    db,
    session,
    commandType: 'ctox.business_os.user.upsert',
    moduleId: 'ctox',
    recordId: payload?.id || '',
    payload,
    source: 'business-os-settings',
  });
  return command.result || command;
}

async function loadRuntimeSettings({ db } = {}) {
  const coll = db?.collection?.('ctox_runtime_settings');
  if (!coll) throw new Error('ctox_runtime_settings collection is required for runtime settings');
  const doc = await coll.findOne('runtime-settings').exec();
  const data = doc?.toJSON?.();
  if (!data) throw new Error('Runtime-Status noch nicht synchronisiert.');
  return data;
}

async function saveRuntimeSettings(payload, { commandBus, db, session } = {}) {
  const previousSettings = await loadRuntimeSettings({ db }).catch(() => null);
  await dispatchModuleCommand({
    commandBus,
    db,
    session,
    commandType: 'ctox.runtime_settings.save',
    moduleId: 'ctox',
    recordId: 'runtime-settings',
    payload,
    source: 'business-os-settings',
  });
  return waitForRuntimeSettingsProjection(db, {
    payload,
    previousUpdatedAtMs: Number(previousSettings?.updated_at_ms || 0),
  });
}

async function waitForRuntimeSettingsProjection(db, options = {}) {
  const timeoutMs = Number(options.timeoutMs || 10000);
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  let lastSettings = null;
  while (Date.now() < deadline) {
    try {
      const settings = await loadRuntimeSettings({ db });
      lastSettings = settings;
      if (runtimeSettingsReflectPayload(settings, options.payload, options.previousUpdatedAtMs)) {
        return settings;
      }
      lastError = new Error('Runtime-Status wurde noch nicht aktualisiert.');
    } catch (error) {
      lastError = error;
    }
    await delay(300);
  }
  if (lastSettings) return lastSettings;
  throw lastError || new Error('Runtime-Status wurde nicht synchronisiert.');
}

async function startSubscriptionAuth({ commandBus, db, session } = {}) {
  const command = await dispatchModuleCommand({
    commandBus,
    db,
    session,
    commandType: 'ctox.subscription_auth.start',
    moduleId: 'ctox',
    recordId: 'subscription-auth',
    payload: { provider: 'openai', auth_mode: 'chatgpt_subscription' },
    source: 'business-os-settings',
  });
  return command.result || command;
}

function runtimeSettingsReflectPayload(settings, payload, previousUpdatedAtMs = 0) {
  if (!payload) return true;
  const runtime = settings?.runtime || {};
  const auth = settings?.auth || {};
  const provider = String(payload.provider || 'local').toLowerCase();
  const authMode = normalizedRuntimeAuthMode(provider, payload.auth_mode);
  const updatedAtMs = Number(settings?.updated_at_ms || 0);
  if (previousUpdatedAtMs > 0 && updatedAtMs <= previousUpdatedAtMs) return false;
  if (String(runtime.provider || '').toLowerCase() !== provider) return false;
  if (String(auth.mode || '').toLowerCase() !== authMode) return false;
  if (payload.chat_model && String(runtime.chat_model || '') !== String(payload.chat_model)) return false;
  if (payload.preset && runtimePresetValue(runtime.preset) !== runtimePresetValue(payload.preset)) {
    return false;
  }
  if (payload.context && runtimeContextValue(runtime.context) !== runtimeContextValue(payload.context)) {
    return false;
  }
  if (Number(payload.max_run_secs || 0) > 0
    && Number(runtime.max_run_secs || 0) !== Number(payload.max_run_secs)) {
    return false;
  }
  return true;
}

function runtimePresetValue(value) {
  const normalized = String(value || '').trim().toLowerCase();
  if (normalized === 'performance') return 'Performance';
  return 'Quality';
}

function runtimeContextValue(value) {
  const normalized = String(value || '').trim().toLowerCase();
  if (['128k', '131072', '128000'].includes(normalized)) return '128k';
  if (['256k', '262144', '256000'].includes(normalized)) return '256k';
  return normalized || '256k';
}

function writeSubscriptionAuthWindow(authWindow, title, message, danger = false) {
  if (!authWindow || authWindow.closed) return;
  try {
    authWindow.document.title = title;
    authWindow.document.body.innerHTML = `
      <main style="font-family: system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; margin: 0; min-height: 100vh; display: grid; place-items: center; background: #111819; color: #f4f8f8;">
        <section style="max-width: 520px; padding: 32px; border: 1px solid ${danger ? '#ff4d4d' : '#16d9ad'}; border-radius: 10px; background: #162021;">
          <h1 style="margin: 0 0 12px; font-size: 22px;">${escapeHtml(title)}</h1>
          <p style="margin: 0; color: #a8b6ba; line-height: 1.5;">${escapeHtml(message)}</p>
        </section>
      </main>
    `;
  } catch {
    // Cross-origin navigation can make the placeholder window no longer writable.
  }
}

async function loadModuleCatalog({ db } = {}) {
  const coll = db?.collection?.('business_module_catalog');
  if (!coll) throw new Error('business_module_catalog collection is required for module metadata');
  const doc = await coll.findOne('module-catalog').exec();
  const data = doc?.toJSON?.();
  if (!data) throw new Error('Modulkatalog noch nicht synchronisiert.');
  return data;
}

async function loadModules({ db } = {}) {
  const catalog = await loadModuleCatalog({ db });
  return {
    ok: catalog.ok !== false,
    modules: Array.isArray(catalog.modules) ? catalog.modules : [],
    governance: catalog.governance || null,
  };
}

async function loadTemplates({ db } = {}) {
  const catalog = await loadModuleCatalog({ db });
  return {
    ok: catalog.ok !== false,
    templates: Array.isArray(catalog.templates) ? catalog.templates : [],
  };
}

async function saveModule(payload, { commandBus, db, session } = {}) {
  const command = await dispatchModuleCommand({
    commandBus,
    db,
    session,
    commandType: 'ctox.module.save',
    moduleId: payload?.id || '',
    recordId: payload?.id || '',
    payload,
    source: 'business-os-settings',
  });
  return command.result || command;
}

async function assignFounder(moduleId, userId, active, { commandBus, db, session } = {}) {
  const command = await dispatchModuleCommand({
    commandBus,
    db,
    session,
    commandType: 'ctox.module.assign_founder',
    moduleId,
    recordId: `${moduleId}:founder:${userId}`,
    payload: { module_id: moduleId, user_id: userId, active },
    source: 'business-os-settings',
  });
  return command.result || command;
}

async function releaseModule(moduleId, { commandBus, db, session } = {}) {
  const command = await dispatchModuleCommand({
    commandBus,
    db,
    session,
    commandType: 'ctox.module.release',
    moduleId,
    recordId: moduleId,
    payload: { module_id: moduleId, notes: 'Business OS module release' },
    source: 'business-os-settings',
  });
  return command.result || command;
}

async function rollbackModule(moduleId, versionId, { commandBus, db, session } = {}) {
  const command = await dispatchModuleCommand({
    commandBus,
    db,
    session,
    commandType: 'ctox.module.rollback',
    moduleId,
    recordId: versionId,
    payload: { module_id: moduleId, version_id: versionId },
    source: 'business-os-settings',
  });
  return command.result || command;
}

async function deleteModule(moduleId, { commandBus, db, session } = {}) {
  const command = await dispatchModuleCommand({
    commandBus,
    db,
    session,
    commandType: 'ctox.module.delete',
    moduleId,
    recordId: moduleId,
    payload: { module_id: moduleId },
    source: 'business-os-settings',
  });
  return command.result || command;
}

async function installTemplate({ templateId, moduleId, title }, { commandBus, db, session } = {}) {
  const command = await dispatchModuleCommand({
    commandBus,
    db,
    session,
    commandType: 'ctox.module.install_template',
    moduleId,
    recordId: moduleId || templateId,
    payload: {
      template_id: templateId,
      module_id: moduleId,
      title,
    },
    source: 'business-os-settings',
  });
  return command.result || command;
}

async function dispatchModuleCommand({
  commandBus,
  db,
  session,
  commandType,
  moduleId,
  recordId,
  payload,
  source,
}) {
  if (!commandBus?.dispatch || !db?.collection?.('business_commands')) {
    throw new Error('business_commands collection is required for module governance commands');
  }
  const commandId = `cmd_${newId()}`;
  await commandBus.dispatch({
    id: commandId,
    module: 'ctox',
    type: commandType,
    record_id: recordId || moduleId,
    inbound_channel: moduleId,
    payload,
    client_context: {
      source,
      module_id: moduleId,
      actor: actorContext(session),
    },
  });
  return waitForCommandProjection(db, commandId);
}

async function waitForCommandProjection(db, commandId, timeoutMs = 45000) {
  const collection = db?.collection?.('business_commands');
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const doc = await collection?.findOne(commandId).exec();
    const data = doc?.toJSON?.();
    if (data && data.status && data.status !== 'pending_sync') {
      if (data.status === 'failed') throw new Error(data.error || `Command ${commandId} failed`);
      return data;
    }
    await delay(300);
  }
  throw new Error(`Command ${commandId} wurde nicht synchronisiert.`);
}

function actorContext(session) {
  const user = session?.user || {};
  return {
    id: user.id || '',
    display_name: user.display_name || user.name || user.id || '',
    role: user.role || 'user',
    is_admin: Boolean(user.is_admin),
  };
}

function newId() {
  return globalThis.crypto?.randomUUID?.() || `${Date.now()}_${Math.random().toString(36).slice(2)}`;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function cssEscape(value) {
  if (globalThis.CSS?.escape) return CSS.escape(value);
  return String(value).replace(/["\\\]]/g, '\\$&');
}

// ============================================================================
// Channels tab — onboarding hub + per-channel wizards.
// All wizard actions are click-through stubs today; the real wiring lives behind
// CTOX-Core command handlers that don't exist yet. The UI shows the full flow
// so user-facing copy and steps can be iterated before any backend work.
// ============================================================================

const CHANNEL_DEFINITIONS = [
  {
    id: 'whatsapp',
    title: 'WhatsApp',
    dot: '#25d366',
    short: 'Geschäftshandy mit eigener Nummer, verbunden per QR-Code.',
  },
  {
    id: 'jami',
    title: 'Jami',
    dot: '#b794f4',
    short: 'Dezentraler Messenger. CTOX erzeugt seinen eigenen Jami-Account.',
  },
  {
    id: 'email',
    title: 'E-Mail',
    dot: '#4a90e2',
    short: 'Gmail, Microsoft 365, Apple iCloud oder klassischer IMAP/SMTP-Server.',
  },
  {
    id: 'teams',
    title: 'MS Teams',
    dot: '#5059c9',
    short: 'Microsoft Teams via Graph-API. OAuth-Login mit deinem Tenant.',
  },
];

const EMAIL_PROVIDERS = [
  {
    id: 'gmail',
    title: 'Gmail / Google Workspace',
    short: 'OAuth über Google',
    glyph: 'G',
  },
  {
    id: 'microsoft365',
    title: 'Microsoft 365 / Outlook.com',
    short: 'OAuth über Microsoft Graph',
    glyph: 'M',
  },
  {
    id: 'apple',
    title: 'Apple iCloud / Mail',
    short: 'IMAP + SMTP mit App-Passwort',
    glyph: '',
  },
  {
    id: 'imap-standard',
    title: 'One.com / 1&1 / All-Inkl',
    short: 'Standard-IMAP/SMTP eines Hosters',
    glyph: '@',
  },
  {
    id: 'custom',
    title: 'Anderer Anbieter',
    short: 'IMAP/SMTP-Felder manuell ausfüllen',
    glyph: '⚙',
  },
];

function channelsPanel(state) {
  if (state.wizard) return channelsWizardPanel(state);
  return channelsHubPanel(state);
}

function channelsHubPanel(state) {
  const accountsByChannel = new Map();
  for (const account of state.accounts) {
    if (!accountsByChannel.has(account.channel)) accountsByChannel.set(account.channel, []);
    accountsByChannel.get(account.channel).push(account);
  }

  return `
    <section class="settings-section channels-hub">
      <header>
        <h3>Kommunikations-Channels</h3>
        <span>Hier richtest du ein, über welche Kanäle CTOX für dich kommuniziert.</span>
      </header>
      <div class="channels-hub-list">
        ${CHANNEL_DEFINITIONS.map((def) => channelHubRow(def, accountsByChannel.get(def.id) || [])).join('')}
      </div>
    </section>
    ${state.error ? `<div class="settings-alert is-danger"><strong>Letzter Fehler</strong><span>${escapeHtml(state.error)}</span></div>` : ''}
    ${state.status ? `<div class="settings-alert"><span>${escapeHtml(state.status)}</span></div>` : ''}
  `;
}

function channelHubRow(def, accounts) {
  if (!accounts.length) {
    return `
      <article class="channel-row" data-channel-id="${escapeHtml(def.id)}">
        <span class="channel-row-dot" style="background:${def.dot}"></span>
        <div class="channel-row-body">
          <div class="channel-row-head">
            <strong>${escapeHtml(def.title)}</strong>
            <span class="channel-row-status channel-row-status--idle">Nicht verbunden</span>
          </div>
          <p class="channel-row-desc">${escapeHtml(def.short)}</p>
        </div>
        <button class="text-button" type="button" data-channel-setup="${escapeHtml(def.id)}">Einrichten</button>
      </article>
    `;
  }
  return accounts.map((account, index) => `
    <article class="channel-row" data-channel-id="${escapeHtml(def.id)}" data-account-key="${escapeHtml(account.account_key || '')}">
      <span class="channel-row-dot" style="background:${def.dot}"></span>
      <div class="channel-row-body">
        <div class="channel-row-head">
          <strong>${escapeHtml(def.title)}</strong>
          <span class="channel-row-handle">${escapeHtml(account.address || account.account_key || '')}</span>
          ${channelHealthBadge(account)}
        </div>
        <p class="channel-row-desc">${escapeHtml(account.provider || def.short)}</p>
        ${channelLastActivityLine(account)}
      </div>
      <div class="channel-row-actions">
        ${index === 0 ? `<button class="text-button" type="button" data-channel-setup="${escapeHtml(def.id)}">+ Weiterer Account</button>` : ''}
        <button class="text-button" type="button" data-channel-disconnect="${escapeHtml(account.account_key)}">Trennen</button>
      </div>
    </article>
  `).join('');
}

function channelHealthBadge(account) {
  const latest = Math.max(parseIso(account.last_inbound_ok_at), parseIso(account.last_outbound_ok_at));
  if (!latest) return `<span class="channel-row-status channel-row-status--warn">Noch keine Aktivität</span>`;
  const ageMs = Date.now() - latest;
  if (ageMs < 24 * 3600 * 1000) return `<span class="channel-row-status channel-row-status--ok">Aktiv</span>`;
  if (ageMs < 7 * 24 * 3600 * 1000) return `<span class="channel-row-status channel-row-status--warn">Inaktiv (>24 h)</span>`;
  return `<span class="channel-row-status channel-row-status--bad">Verbindung verloren</span>`;
}

function channelLastActivityLine(account) {
  const inbound = account.last_inbound_ok_at ? `Letzter Eingang: ${formatIsoShort(account.last_inbound_ok_at)}` : 'Noch kein Eingang';
  const outbound = account.last_outbound_ok_at ? `Letzter Ausgang: ${formatIsoShort(account.last_outbound_ok_at)}` : 'Noch kein Ausgang';
  return `<small class="channel-row-meta">${escapeHtml(inbound)} · ${escapeHtml(outbound)}</small>`;
}

function channelsWizardPanel(state) {
  if (state.wizard === 'whatsapp') return whatsappWizard(state);
  if (state.wizard === 'jami') return jamiWizard(state);
  if (state.wizard === 'email') return emailWizard(state);
  if (state.wizard === 'teams') return teamsWizard(state);
  return '';
}

function wizardShell({ title, step, totalSteps, body, backLabel = 'Abbrechen', nextLabel = '', nextDisabled = false, nextAction = '' }) {
  return `
    <section class="settings-section channels-wizard">
      <header class="channels-wizard-head">
        <button type="button" class="icon-button" data-channel-back aria-label="Zurück">←</button>
        <div>
          <h3>${escapeHtml(title)}</h3>
          <span>Schritt ${step} von ${totalSteps}</span>
        </div>
      </header>
      <div class="channels-wizard-body">
        ${body}
      </div>
      <footer class="channels-wizard-footer">
        <button type="button" class="text-button" data-channel-cancel>${escapeHtml(backLabel)}</button>
        ${nextLabel ? `<button type="button" class="text-button settings-primary" data-channel-next="${escapeHtml(nextAction)}" ${nextDisabled ? 'disabled' : ''}>${escapeHtml(nextLabel)}</button>` : ''}
      </footer>
    </section>
  `;
}

// ---- WhatsApp wizard ----
function whatsappWizard(state) {
  const step = state.step || 'intro';
  const pairing = state.data?.pairingState || null;
  const errorBlock = state.error
    ? `<div class="channels-alert channels-alert--err">${escapeHtml(state.error)}</div>`
    : '';

  if (step === 'intro') {
    return wizardShell({
      title: 'WhatsApp einrichten',
      step: 1, totalSteps: 3,
      body: `
        <div class="channels-explain">
          <p><strong>Du brauchst ein dediziertes Geschäftshandy.</strong> Nicht dein Privat-Handy.</p>
          <ul class="channels-explain-list">
            <li><b>Privatchats vermischen</b> sich sonst mit Geschäftsnachrichten — und CTOX sieht alles, was reinkommt.</li>
            <li><b>WhatsApp kann Privat-Accounts sperren</b>, wenn Multi-Device-Sessions als Bot erkannt werden. Eine Geschäftsnummer ist sicherer.</li>
            <li><b>DSGVO</b>: Geschäfts- und Privatkommunikation gehören rechtlich getrennt.</li>
          </ul>
          <p>Vor dem nächsten Schritt solltest du Folgendes bereit haben:</p>
          <ul class="channels-checklist">
            <li>Ein zweites Smartphone (Android oder iPhone) oder altes Gerät mit aktiver Geschäfts-SIM</li>
            <li>WhatsApp ist auf dem Gerät installiert und mit der Geschäftsnummer registriert</li>
            <li>Das Handy ist eingeschaltet und online (Strom, WLAN/Mobilfunk) — sonst pausiert WhatsApp die Verbindung nach ca. 14 Tagen</li>
          </ul>
          ${errorBlock}
        </div>
      `,
      nextLabel: 'Geschäftshandy ist bereit → Weiter',
      nextAction: 'whatsapp:qr',
    });
  }
  if (step === 'qr') {
    const status = String(pairing?.status || 'idle').toLowerCase();
    return wizardShell({
      title: 'WhatsApp einrichten',
      step: 2, totalSteps: 3,
      body: `
        <div class="channels-qr-wrap">
          ${renderQrBox(pairing, 'CTOX-Core erzeugt den QR über pair_device_until_success.')}
          <p class="channels-qr-instructions">
            Auf dem Geschäftshandy: <b>WhatsApp öffnen → ⋮ Menü → Verlinkte Geräte → Gerät hinzufügen</b> und diesen QR scannen.
          </p>
          ${renderPairingStatus(status)}
          <button type="button" class="text-button" data-channel-action="whatsapp:refresh-qr">QR erneuern</button>
          ${errorBlock}
        </div>
      `,
    });
  }
  // step === 'confirm'
  const accountKey = state.data?.connectedAccountKey || pairing?.account_key || '';
  return wizardShell({
    title: 'WhatsApp einrichten',
    step: 3, totalSteps: 3,
    body: `
      <div class="channels-confirm">
        <div class="channels-confirm-icon channels-confirm-icon--ok">✓</div>
        <h4>WhatsApp ist verbunden</h4>
        <p>CTOX kann jetzt Nachrichten auf dieser Nummer empfangen und — nach Approval — senden.</p>
        ${accountKey ? `<div class="channels-confirm-detail"><span>Account</span><strong>${escapeHtml(accountKey)}</strong></div>` : ''}
        <small class="channels-confirm-note">Halte das Geschäftshandy online. Wenn es länger als 2 Wochen offline ist, musst du den QR erneut scannen.</small>
      </div>
    `,
    backLabel: '',
    nextLabel: 'Fertig',
    nextAction: 'wizard:done',
  });
}

function renderQrBox(pairing, hint) {
  if (pairing?.qr_svg) {
    return `<div class="channels-qr-image">${pairing.qr_svg}</div>`;
  }
  if (pairing?.qr_payload) {
    const payload = String(pairing.qr_payload);
    const src = payload.startsWith('data:')
      ? payload
      : `https://api.qrserver.com/v1/create-qr-code/?size=200x200&data=${encodeURIComponent(payload)}`;
    // Note: external QR-rendering would violate CTOX privacy; CTOX-Core should
    // emit the SVG directly via qr_svg. The img fallback is a last resort and
    // is disabled here to avoid leaking pairing data to a third party.
    void src;
    return `
      <div class="channels-qr-placeholder">
        <span>QR-Payload empfangen</span>
        <small>CTOX-Core soll qr_svg setzen, damit das QR-Bild lokal gerendert wird (kein externer Renderer).</small>
      </div>
    `;
  }
  return `
    <div class="channels-qr-placeholder">
      <span>Kein QR-Code</span>
      <small>${escapeHtml(hint || '')}</small>
    </div>
  `;
}

function renderPairingStatus(status) {
  if (status === 'paired' || status === 'success') {
    return `<div class="channels-qr-status is-ok">✅ Scan erkannt — verbinde…</div>`;
  }
  if (status === 'failed' || status === 'error') {
    return `<div class="channels-qr-status is-err">⚠ Pairing fehlgeschlagen.</div>`;
  }
  if (status === 'waiting_for_scan' || status === 'idle') {
    return `<div class="channels-qr-status is-waiting">⏳ Warte auf Scan…</div>`;
  }
  return `<div class="channels-qr-status is-waiting">${escapeHtml(status || 'unbekannt')}</div>`;
}

// ---- Jami wizard ----
function jamiWizard(state) {
  const step = state.step || 'intro';
  const pairing = state.data?.pairingState || null;
  const errorBlock = state.error
    ? `<div class="channels-alert channels-alert--err">${escapeHtml(state.error)}</div>`
    : '';

  if (step === 'intro') {
    return wizardShell({
      title: 'Jami einrichten',
      step: 1, totalSteps: 2,
      body: `
        <div class="channels-explain">
          <p>Jami ist ein <strong>dezentraler Messenger</strong>. Du brauchst weder Telefonnummer noch Email-Account. CTOX erzeugt einen eigenen Jami-Account direkt auf deinem CTOX-Server.</p>
          <p>Nach der Erstellung bekommst du einen QR-Code mit der Jami-ID. Damit kannst du CTOX in deiner privaten Jami-App als Kontakt hinzufügen.</p>
          <label class="channels-field">
            <span>Anzeigename für den CTOX-Account</span>
            <input type="text" data-channel-input="jami:displayName" placeholder="CTOX – Acme GmbH" value="${escapeHtml(state.data?.jamiDisplayName || 'CTOX')}" />
          </label>
          ${errorBlock}
        </div>
      `,
      nextLabel: 'Account erstellen',
      nextAction: 'jami:create',
    });
  }
  if (step === 'creating') {
    return wizardShell({
      title: 'Jami einrichten',
      step: 2, totalSteps: 2,
      body: `
        <div class="channels-testing">
          <div class="channels-testing-step is-active">⏳ Jami-Account wird erzeugt…</div>
          <small class="channels-form-note">CTOX-Core ruft den Jami-Daemon. Das dauert wenige Sekunden.</small>
        </div>
        ${errorBlock}
      `,
    });
  }
  // step === 'confirm'
  const jamiId = pairing?.qr_payload || state.data?.connectedAccountKey || '';
  return wizardShell({
    title: 'Jami einrichten',
    step: 2, totalSteps: 2,
    body: `
      <div class="channels-confirm">
        <div class="channels-confirm-icon channels-confirm-icon--ok">✓</div>
        <h4>Jami-Account erstellt</h4>
        ${renderQrBox(pairing, 'CTOX-Core soll qr_svg mit der Jami-ID setzen.')}
        <p>Damit du CTOX in deiner privaten Jami-App siehst, scanne diesen QR mit <b>Jami → Kontakt hinzufügen → QR-Code scannen</b>. Oder kopiere die ID manuell.</p>
        ${jamiId ? `
          <div class="channels-confirm-detail">
            <span>Jami-ID</span>
            <code>${escapeHtml(jamiId)} <button type="button" class="channels-copy" data-channel-copy="${escapeHtml(jamiId)}">⧉</button></code>
          </div>
        ` : ''}
        <details class="channels-advanced">
          <summary>Erweitert</summary>
          <p>Sichere den Account regelmäßig — sonst ist er bei Datenverlust nicht wiederherstellbar.</p>
          <button type="button" class="text-button" data-channel-action="jami:export">Account-Archiv exportieren (.gz)</button>
        </details>
      </div>
    `,
    backLabel: '',
    nextLabel: 'Fertig',
    nextAction: 'wizard:done',
  });
}

// ---- Email wizard ----
function emailWizard(state) {
  const step = state.step || 'provider';
  if (step === 'provider') {
    return wizardShell({
      title: 'E-Mail einrichten',
      step: 1, totalSteps: 3,
      body: `
        <div class="channels-explain">
          <p>Welcher E-Mail-Anbieter?</p>
          <div class="channels-provider-grid">
            ${EMAIL_PROVIDERS.map((p) => `
              <button type="button" class="channels-provider-card" data-channel-action="email:provider:${escapeHtml(p.id)}">
                <span class="channels-provider-glyph">${escapeHtml(p.glyph || '📧')}</span>
                <strong>${escapeHtml(p.title)}</strong>
                <small>${escapeHtml(p.short)}</small>
              </button>
            `).join('')}
          </div>
        </div>
      `,
    });
  }
  if (step === 'form') {
    return wizardShell({
      title: 'E-Mail einrichten',
      step: 2, totalSteps: 3,
      body: emailProviderForm(state),
      nextLabel: 'Verbindung testen',
      nextAction: 'email:test',
    });
  }
  if (step === 'testing') {
    return wizardShell({
      title: 'E-Mail einrichten',
      step: 2, totalSteps: 3,
      body: `
        <div class="channels-testing">
          <div class="channels-testing-step is-active">⏳ CTOX testet IMAP + SMTP …</div>
          <small class="channels-form-note">Backend ruft <code>email_native::test()</code> via <code>RxDB-Command ctox.channel.test</code>.</small>
        </div>
      `,
    });
  }
  const testResult = state.data?.testResult || null;
  return wizardShell({
    title: 'E-Mail einrichten',
    step: 3, totalSteps: 3,
    body: `
      <div class="channels-confirm">
        <div class="channels-confirm-icon channels-confirm-icon--ok">✓</div>
        <h4>E-Mail ist verbunden</h4>
        <div class="channels-confirm-detail">
          <span>Adresse</span><strong>${escapeHtml(state.data?.emailAddress || state.data?.connectedAddress || '—')}</strong>
        </div>
        <div class="channels-confirm-detail">
          <span>Anbieter</span><strong>${escapeHtml(emailProviderLabel(state.data?.emailProvider))}</strong>
        </div>
        ${testResult?.imap_ok !== undefined ? `<div class="channels-confirm-detail"><span>IMAP</span><strong>${testResult.imap_ok ? 'OK' : 'Fehler'}</strong></div>` : ''}
        ${testResult?.smtp_ok !== undefined ? `<div class="channels-confirm-detail"><span>SMTP</span><strong>${testResult.smtp_ok ? 'OK' : 'Fehler'}</strong></div>` : ''}
        ${testResult?.message_count !== undefined ? `<small class="channels-confirm-note">CTOX hat ${testResult.message_count} Eingangsnachrichten im Postfach erkannt.</small>` : ''}
      </div>
    `,
    backLabel: '',
    nextLabel: 'Fertig',
    nextAction: 'wizard:done',
  });
}

function emailProviderLabel(id) {
  return EMAIL_PROVIDERS.find((p) => p.id === id)?.title || 'E-Mail';
}

function emailProviderForm(state) {
  const provider = state.provider || state.data?.emailProvider || 'custom';
  if (provider === 'gmail') {
    return `
      <div class="channels-form">
        <p>CTOX leitet dich gleich zur Google-Anmeldung weiter. Dort meldest du dich mit der E-Mail-Adresse an, die CTOX nutzen soll.</p>
        <p class="channels-form-note">Stelle sicher, dass dein Browser Pop-ups für diese Seite zulässt.</p>
      </div>
    `;
  }
  if (provider === 'microsoft365') {
    return `
      <div class="channels-form">
        <p>CTOX leitet dich gleich zur Microsoft-Anmeldung weiter.</p>
        <label class="channels-toggle">
          <input type="checkbox" data-channel-input="email:customApp" ${state.data?.emailCustomApp ? 'checked' : ''} />
          <span>Eigene Azure-AD-App verwenden</span>
        </label>
        ${state.data?.emailCustomApp ? `
          <label class="channels-field">
            <span>Tenant-ID</span>
            <input type="text" data-channel-input="email:tenantId" placeholder="00000000-0000-0000-0000-000000000000" />
          </label>
          <label class="channels-field">
            <span>Client-ID</span>
            <input type="text" data-channel-input="email:clientId" />
          </label>
          <label class="channels-field">
            <span>Client-Secret</span>
            <input type="password" data-channel-input="email:clientSecret" />
          </label>
        ` : ''}
      </div>
    `;
  }
  if (provider === 'apple') {
    return `
      <div class="channels-form">
        <label class="channels-field">
          <span>iCloud-E-Mail</span>
          <input type="email" data-channel-input="email:address" placeholder="name@icloud.com" value="${escapeHtml(state.data?.emailAddress || '')}" />
        </label>
        <label class="channels-field">
          <span>App-spezifisches Passwort</span>
          <input type="password" data-channel-input="email:password" placeholder="xxxx xxxx xxxx xxxx" />
        </label>
        <p class="channels-form-note">
          <a href="https://support.apple.com/de-de/102654" target="_blank" rel="noopener noreferrer">Wie erstelle ich ein App-Passwort bei Apple? →</a>
        </p>
      </div>
    `;
  }
  if (provider === 'imap-standard') {
    return `
      <div class="channels-form">
        <label class="channels-field">
          <span>E-Mail-Adresse</span>
          <input type="email" data-channel-input="email:address" placeholder="name@firma.de" value="${escapeHtml(state.data?.emailAddress || '')}" />
        </label>
        <label class="channels-field">
          <span>Passwort</span>
          <input type="password" data-channel-input="email:password" />
        </label>
        <details class="channels-advanced">
          <summary>Server-Einstellungen anpassen</summary>
          <div class="channels-form-grid">
            <label class="channels-field"><span>IMAP-Host</span><input type="text" data-channel-input="email:imapHost" placeholder="imap.one.com" /></label>
            <label class="channels-field"><span>IMAP-Port</span><input type="number" data-channel-input="email:imapPort" value="993" /></label>
            <label class="channels-field"><span>SMTP-Host</span><input type="text" data-channel-input="email:smtpHost" placeholder="send.one.com" /></label>
            <label class="channels-field"><span>SMTP-Port</span><input type="number" data-channel-input="email:smtpPort" value="465" /></label>
          </div>
        </details>
      </div>
    `;
  }
  return `
    <div class="channels-form">
      <label class="channels-field">
        <span>E-Mail-Adresse</span>
        <input type="email" data-channel-input="email:address" placeholder="name@firma.de" value="${escapeHtml(state.data?.emailAddress || '')}" />
      </label>
      <label class="channels-field">
        <span>Passwort</span>
        <input type="password" data-channel-input="email:password" />
      </label>
      <div class="channels-form-grid">
        <label class="channels-field"><span>IMAP-Host</span><input type="text" data-channel-input="email:imapHost" /></label>
        <label class="channels-field"><span>IMAP-Port</span><input type="number" data-channel-input="email:imapPort" value="993" /></label>
        <label class="channels-field"><span>SMTP-Host</span><input type="text" data-channel-input="email:smtpHost" /></label>
        <label class="channels-field"><span>SMTP-Port</span><input type="number" data-channel-input="email:smtpPort" value="587" /></label>
      </div>
    </div>
  `;
}

// ---- Teams wizard ----
function teamsWizard(state) {
  const step = state.step || 'intro';
  const errorBlock = state.error
    ? `<div class="channels-alert channels-alert--err">${escapeHtml(state.error)}</div>`
    : '';
  if (step === 'intro') {
    const customApp = state.data?.teamsCustomApp === true;
    return wizardShell({
      title: 'Microsoft Teams einrichten',
      step: 1, totalSteps: 3,
      body: `
        <div class="channels-explain">
          <p>CTOX verbindet sich mit Teams über die <strong>Microsoft Graph API</strong>. Es gibt zwei unterstützte Modi:</p>
          <ul class="channels-explain-list">
            <li><strong>Service-Principal</strong> (empfohlen für Produktion): eine Azure-AD-App mit Tenant-ID, Client-ID und Client-Secret. Dein Admin registriert die App einmalig und du trägst die Werte hier ein.</li>
            <li><strong>Benutzerkonto (ROPC)</strong>: Microsoft-365-Benutzername + Passwort. Funktioniert nur ohne MFA und nutzt Microsofts öffentlichen Office-Client. Eher für Test-Setups.</li>
          </ul>
          <label class="channels-toggle">
            <input type="checkbox" data-channel-input="teams:customApp" ${customApp ? 'checked' : ''} />
            <span>Service-Principal-Modus (Tenant + Client-ID + Secret)</span>
          </label>
          ${customApp ? `
            <label class="channels-field"><span>Tenant-ID</span><input type="text" data-channel-input="teams:tenantId" placeholder="00000000-0000-0000-0000-000000000000" value="${escapeHtml(state.data?.teamsTenantId || '')}" /></label>
            <label class="channels-field"><span>Client-ID</span><input type="text" data-channel-input="teams:clientId" value="${escapeHtml(state.data?.teamsClientId || '')}" /></label>
            <label class="channels-field"><span>Client-Secret</span><input type="password" data-channel-input="teams:clientSecret" /></label>
            <small class="channels-form-note">Mit diesen Werten ruft CTOX <code>acquire_app_token</code> (Client-Credentials-Flow) gegen <code>login.microsoftonline.com</code> auf.</small>
          ` : `
            <label class="channels-field"><span>Tenant-ID (optional)</span><input type="text" data-channel-input="teams:tenantId" placeholder="leer → organizations" value="${escapeHtml(state.data?.teamsTenantId || '')}" /></label>
            <label class="channels-field"><span>Microsoft-Account</span><input type="email" data-channel-input="teams:username" placeholder="name@firma.de" value="${escapeHtml(state.data?.teamsUsername || '')}" /></label>
            <label class="channels-field"><span>Passwort</span><input type="password" data-channel-input="teams:password" /></label>
            <small class="channels-form-note">ROPC-Flow über Microsofts öffentlichen Office-Client. <strong>Bei aktivierter MFA scheitert dieser Modus</strong> — dann musst du Service-Principal nutzen.</small>
          `}
          ${errorBlock}
        </div>
      `,
      nextLabel: 'Verbinden + testen',
      nextAction: 'teams:save_test',
    });
  }
  if (step === 'testing') {
    return wizardShell({
      title: 'Microsoft Teams einrichten',
      step: 2, totalSteps: 2,
      body: `
        <div class="channels-testing">
          <div class="channels-testing-step is-active">⏳ CTOX testet Graph-API …</div>
          <small class="channels-form-note">Backend ruft <code>teams_native::test()</code> via <code>RxDB-Command ctox.channel.test</code>.</small>
        </div>
        ${errorBlock}
      `,
    });
  }
  const testResult = state.data?.testResult || null;
  const tenantLabel = state.data?.connectedAddress || state.data?.connectedAccountKey || state.data?.teamsTenantId || '—';
  return wizardShell({
    title: 'Microsoft Teams einrichten',
    step: 2, totalSteps: 2,
    body: `
      <div class="channels-confirm">
        <div class="channels-confirm-icon channels-confirm-icon--ok">✓</div>
        <h4>Teams ist verbunden</h4>
        <div class="channels-confirm-detail"><span>Tenant</span><strong>${escapeHtml(tenantLabel)}</strong></div>
        ${testResult?.ok !== undefined ? `<div class="channels-confirm-detail"><span>Graph-API</span><strong>${testResult.ok ? 'OK' : 'Fehler'}</strong></div>` : ''}
      </div>
    `,
    backLabel: '',
    nextLabel: 'Fertig',
    nextAction: 'wizard:done',
  });
}

// ---- Event wiring for the channels tab ----
const ACCOUNT_WAIT_TIMEOUT_MS = 30 * 1000;
const QR_POLL_INTERVAL_MS = 1500;

async function loadChannelAccountsFromRxdb(db) {
  const coll = db?.collection?.('communication_accounts');
  if (!coll) return [];
  const docs = await coll.find().exec();
  return docs
    .map((doc) => doc.toJSON())
    .filter((account) => account && account._deleted !== true && account.is_deleted !== true)
    .sort((a, b) => (a.channel || '').localeCompare(b.channel || ''));
}

async function loadPairingStateFromRxdb(db, channelId) {
  const coll = db?.collection?.('channel_pairing_state');
  if (!coll) return null;
  const doc = await coll.findOne(channelId).exec();
  return doc?.toJSON?.() || null;
}

function wireChannelHandlers(
  body,
  settingsState,
  render,
  { commandBus, db, session, refreshChannelAccounts, ensureChannelCollections },
) {
  const channels = settingsState.channels;
  if (!channels.qrPoll) channels.qrPoll = null;

  function resetWizard() {
    channels.wizard = null;
    channels.step = null;
    channels.provider = null;
    channels.data = {};
    channels.error = '';
    stopQrPolling();
    render();
  }

  function stopQrPolling() {
    if (channels.qrPoll) {
      clearInterval(channels.qrPoll);
      channels.qrPoll = null;
    }
  }

  function startQrPolling(channelId) {
    stopQrPolling();
    refreshPairingState(channelId);
    channels.qrPoll = setInterval(() => refreshPairingState(channelId), QR_POLL_INTERVAL_MS);
  }

  async function refreshPairingState(channelId) {
    try {
      await ensureChannelCollections?.();
      const data = await loadPairingStateFromRxdb(db, channelId);
      if (!data) throw new Error('Pairing-State noch nicht synchronisiert.');
      channels.data.pairingState = data || null;
      channels.error = data?.error || '';
      if (data?.account_key && channels.wizard === channelId) {
        channels.step = 'confirm';
        channels.data.connectedAccountKey = data.account_key;
        stopQrPolling();
      }
      render();
    } catch (error) {
      // Only surface the error once the wizard is on the QR screen, so the
      // intro screen doesn't fail loudly before any pairing attempt.
      if (channels.step === 'qr' || channels.step === 'creating') {
        channels.error = `Pairing-Status: ${error?.message || error}`;
        render();
      }
    }
  }

  async function pollAccountAppearance(channelId, expectedAddress = null) {
    const start = Date.now();
    const deadline = start + ACCOUNT_WAIT_TIMEOUT_MS;
    while (Date.now() < deadline) {
      if (channels.wizard !== channelId) return;
      try {
        await ensureChannelCollections?.();
        const accounts = await loadChannelAccountsFromRxdb(db);
        const match = expectedAddress
          ? accounts.find((a) => a.channel === channelId && a.address === expectedAddress)
          : accounts.find((a) => a.channel === channelId && a.created_at && Date.parse(a.created_at) >= start);
        if (match) {
          channels.step = 'confirm';
          channels.data.connectedAccountKey = match.account_key;
          channels.data.connectedAddress = match.address;
          channels.accounts = accounts;
          render();
          return;
        }
      } catch (error) {
        console.error('[settings/channels] account poll failed:', error);
      }
      await new Promise((resolve) => setTimeout(resolve, 1500));
    }
    if (channels.wizard === channelId && channels.step !== 'confirm') {
      channels.error = `${channelId}-Account wurde nach ${ACCOUNT_WAIT_TIMEOUT_MS / 1000}s nicht erkannt. Prüfe CTOX-Core-Logs.`;
      render();
    }
  }

  async function postChannelEndpoint(path, payload) {
    try {
      const command = channelCommandForEndpoint(path, payload || {});
      const result = await dispatchModuleCommand({
        commandBus,
        db,
        session,
        ...command,
        source: 'business-os-settings-channels',
      });
      const body = result.result || result;
      if (body?.ok === false) {
        const message = body?.error || body?.message || body?.status || 'channel command failed';
        channels.error = `${command.commandType}: ${message}`;
        render();
        return null;
      }
      channels.error = '';
      return body;
    } catch (error) {
      channels.error = `${path}: ${error?.message || error}`;
      render();
      return null;
    }
  }

  body.querySelectorAll('[data-channel-setup]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const channelId = btn.dataset.channelSetup;
      channels.wizard = channelId;
      channels.step = channelId === 'email' ? 'provider' : 'intro';
      channels.provider = null;
      channels.data = {};
      channels.error = '';
      channels.status = '';
      stopQrPolling();
      render();
    });
  });

  body.querySelectorAll('[data-channel-disconnect]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const accountKey = btn.dataset.channelDisconnect;
      const confirmed = await showBusinessConfirm(
        'Diesen Channel-Account trennen? CTOX kann darüber keine Nachrichten mehr senden oder empfangen, bis du ihn neu einrichtest. Der Verlauf bleibt gespeichert.',
        { title: 'Channel trennen' },
      );
      if (!confirmed) return;
      const result = await postChannelEndpoint('channel.disconnect', { account_key: accountKey });
      if (result) {
        channels.status = `${accountKey} getrennt.`;
        try {
          await ensureChannelCollections?.();
          await refreshChannelAccounts?.();
        } catch {}
        render();
      }
    });
  });

  body.querySelectorAll('[data-channel-back]').forEach((btn) => {
    btn.addEventListener('click', () => {
      if (channels.wizard === 'email' && channels.step === 'form') {
        channels.step = 'provider';
        channels.provider = null;
        channels.error = '';
        render();
        return;
      }
      resetWizard();
    });
  });

  body.querySelectorAll('[data-channel-cancel]').forEach((btn) => {
    btn.addEventListener('click', () => resetWizard());
  });

  body.querySelectorAll('[data-channel-input]').forEach((input) => {
    input.addEventListener('input', () => {
      const key = input.dataset.channelInput;
      const value = input.type === 'checkbox' ? input.checked : input.value;
      const dataKey = channelDataKey(key);
      if (dataKey) channels.data[dataKey] = value;
      if (input.type === 'checkbox' && (key === 'email:customApp' || key === 'teams:customApp')) {
        render();
      }
    });
  });

  const actionCtx = {
    channels,
    render,
    postChannelEndpoint,
    startQrPolling,
    pollAccountAppearance,
    resetWizard,
  };
  body.querySelectorAll('[data-channel-action]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      await handleChannelAction(btn.dataset.channelAction, actionCtx);
    });
  });

  body.querySelectorAll('[data-channel-next]').forEach((btn) => {
    btn.addEventListener('click', async (event) => {
      await handleChannelAction(event.currentTarget.dataset.channelNext, actionCtx);
    });
  });

  body.querySelectorAll('[data-channel-copy]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const value = btn.dataset.channelCopy;
      if (navigator.clipboard?.writeText) {
        navigator.clipboard.writeText(value).catch(() => {});
      }
      btn.textContent = '✓';
      setTimeout(() => { btn.textContent = '⧉'; }, 1200);
    });
  });
}

function channelCommandForEndpoint(path, payload) {
  switch (path) {
    case 'channel.test':
      return {
        commandType: 'ctox.channel.test',
        moduleId: 'ctox',
        recordId: payload.account_key || payload.channel || 'channel-test',
        payload,
      };
    case 'channel.sync':
      return {
        commandType: 'ctox.channel.sync',
        moduleId: 'ctox',
        recordId: payload.channel || 'channel-sync',
        payload,
      };
    case 'channel.settings.save':
      return {
        commandType: 'ctox.channel.settings.save',
        moduleId: 'ctox',
        recordId: payload.channel || 'channel-settings',
        payload,
      };
    case 'channel.disconnect':
      return {
        commandType: 'ctox.channel.disconnect',
        moduleId: 'ctox',
        recordId: payload.account_key || 'channel-disconnect',
        payload,
      };
    case 'channel.pair.start':
      return {
        commandType: 'ctox.channel.pair.start',
        moduleId: 'ctox',
        recordId: payload.channel || 'channel-pair',
        payload,
      };
    case 'channel.jami.create':
      return {
        commandType: 'ctox.channel.jami.create',
        moduleId: 'ctox',
        recordId: 'jami-create',
        payload,
      };
    case 'channel.jami.export':
      return {
        commandType: 'ctox.channel.jami.export',
        moduleId: 'ctox',
        recordId: 'jami-export',
        payload,
      };
    default:
      throw new Error(`Unsupported channel command endpoint: ${path}`);
  }
}

function channelDataKey(inputKey) {
  switch (inputKey) {
    case 'jami:displayName': return 'jamiDisplayName';
    case 'email:address': return 'emailAddress';
    case 'email:password': return 'emailPassword';
    case 'email:customApp': return 'emailCustomApp';
    case 'email:tenantId': return 'emailTenantId';
    case 'email:clientId': return 'emailClientId';
    case 'email:clientSecret': return 'emailClientSecret';
    case 'email:imapHost': return 'emailImapHost';
    case 'email:imapPort': return 'emailImapPort';
    case 'email:smtpHost': return 'emailSmtpHost';
    case 'email:smtpPort': return 'emailSmtpPort';
    case 'teams:customApp': return 'teamsCustomApp';
    case 'teams:tenantId': return 'teamsTenantId';
    case 'teams:clientId': return 'teamsClientId';
    case 'teams:clientSecret': return 'teamsClientSecret';
    case 'teams:username': return 'teamsUsername';
    case 'teams:password': return 'teamsPassword';
    default: return null;
  }
}

async function handleChannelAction(action, ctx) {
  const { channels, render, postChannelEndpoint, startQrPolling, pollAccountAppearance, resetWizard } = ctx;

  if (action === 'wizard:done') {
    resetWizard();
    channels.status = 'Setup abgeschlossen.';
    render();
    return;
  }

  // ---- WhatsApp ----
  if (action === 'whatsapp:qr') {
    channels.step = 'qr';
    channels.error = '';
    channels.data.pairingState = null;
    render();
    const result = await postChannelEndpoint('channel.pair.start', { channel: 'whatsapp' });
    if (result) {
      startQrPolling('whatsapp');
      pollAccountAppearance('whatsapp');
    }
    return;
  }
  if (action === 'whatsapp:refresh-qr') {
    channels.data.pairingState = null;
    channels.error = '';
    render();
    const result = await postChannelEndpoint('channel.pair.start', {
      channel: 'whatsapp',
      restart: true,
    });
    if (result) startQrPolling('whatsapp');
    return;
  }

  // ---- Jami ----
  // CTOX-Core has jami_native::sync + resolve_account; account creation is
  // dispatched as a replicated channel command.
  if (action === 'jami:create') {
    channels.step = 'creating';
    channels.error = '';
    render();
    const displayName = String(channels.data.jamiDisplayName || 'CTOX').trim() || 'CTOX';
    const result = await postChannelEndpoint('channel.jami.create', { display_name: displayName });
    if (!result) {
      channels.step = 'intro';
      render();
      return;
    }
    // Account row appears + QR-payload (the Jami-ID) shows up via pair/state.
    startQrPolling('jami');
    pollAccountAppearance('jami');
    return;
  }
  if (action === 'jami:export') {
    const result = await postChannelEndpoint('channel.jami.export', {});
    if (result) {
      channels.status = `Account-Archiv exportiert nach ${result.archive_path || 'runtime/communication/jami/archive/'}.`;
      render();
    }
    return;
  }

  // ---- Email ----
  if (action.startsWith('email:provider:')) {
    const providerId = action.slice('email:provider:'.length);
    channels.provider = providerId;
    channels.data.emailProvider = providerId;
    channels.step = 'form';
    channels.error = '';
    render();
    return;
  }
  if (action === 'email:test') {
    channels.step = 'testing';
    channels.error = '';
    render();
    const payload = emailConfigPayload(channels);
    // Save settings first, then run the adapter test command.
    const saveResult = await postChannelEndpoint('channel.settings.save', {
      channel: 'email',
      config: payload,
    });
    if (!saveResult) {
      channels.step = 'form';
      render();
      return;
    }
    const testResult = await postChannelEndpoint('channel.test', {
      channel: 'email',
      account_key: payload.address ? `email:${payload.address}` : '',
    });
    if (!testResult) {
      channels.step = 'form';
      render();
      return;
    }
    channels.data.testResult = testResult;
    pollAccountAppearance('email', payload.address);
    return;
  }

  // ---- Teams ----
  if (action === 'teams:save_test') {
    channels.error = '';
    channels.step = 'testing';
    render();
    const payload = teamsConfigPayload(channels);
    const saveResult = await postChannelEndpoint('channel.settings.save', {
      channel: 'teams',
      config: payload,
    });
    if (!saveResult) {
      channels.step = 'intro';
      render();
      return;
    }
    const testResult = await postChannelEndpoint('channel.test', {
      channel: 'teams',
    });
    if (!testResult) {
      channels.step = 'intro';
      render();
      return;
    }
    channels.data.testResult = testResult;
    pollAccountAppearance('teams');
    return;
  }
  if (action === 'teams:confirm') {
    // No client-side subscription selection — CTOX-Core syncs all teams/chats
    // it has Graph access to via the existing channel sync pipeline.
    const syncResult = await postChannelEndpoint('channel.sync', {
      channel: 'teams',
    });
    if (syncResult) {
      pollAccountAppearance('teams');
    }
  }
}

function emailConfigPayload(channels) {
  const data = channels.data || {};
  const provider = channels.provider || data.emailProvider || 'custom';
  return {
    provider,
    address: data.emailAddress || '',
    password: data.emailPassword || '',
    imap_host: data.emailImapHost || '',
    imap_port: parseInt(data.emailImapPort, 10) || 0,
    smtp_host: data.emailSmtpHost || '',
    smtp_port: parseInt(data.emailSmtpPort, 10) || 0,
    custom_app: !!data.emailCustomApp,
    tenant_id: data.emailTenantId || '',
    client_id: data.emailClientId || '',
    client_secret: data.emailClientSecret || '',
  };
}

function teamsConfigPayload(channels) {
  const data = channels.data || {};
  return {
    custom_app: !!data.teamsCustomApp,
    tenant_id: data.teamsTenantId || '',
    client_id: data.teamsClientId || '',
    client_secret: data.teamsClientSecret || '',
    username: data.teamsUsername || '',
    password: data.teamsPassword || '',
  };
}

function collectTeamsSubscriptions() {
  // Teams subscriptions list comes from CTOX after OAuth completes. We collect
  // the actual checked checkboxes from the DOM by data-channel-sub identifier.
  const subs = [];
  for (const input of document.querySelectorAll('[data-channels-sub]')) {
    if (input instanceof HTMLInputElement && input.checked) {
      subs.push(input.dataset.channelsSub);
    }
  }
  return subs;
}

function parseIso(value) {
  if (!value) return 0;
  const t = Date.parse(value);
  return Number.isFinite(t) ? t : 0;
}

function formatIsoShort(value) {
  const ms = parseIso(value);
  if (!ms) return '—';
  const date = new Date(ms);
  return `${String(date.getDate()).padStart(2, '0')}.${String(date.getMonth() + 1).padStart(2, '0')}.${date.getFullYear()} ${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
}
