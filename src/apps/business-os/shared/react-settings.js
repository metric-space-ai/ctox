export async function openReactSettings({
  mount,
  modules = [],
  session = null,
  governance = null,
  syncConfig = null,
  commandBus = null,
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
  };

  const refreshManagedModules = async () => {
    try {
      const payload = await loadModules();
      settingsState.modules = payload.modules || settingsState.modules;
      settingsState.governance = payload.governance || settingsState.governance;
    } catch (error) {
      settingsState.commandStatus = `Module konnten nicht geladen werden: ${error.message || error}`;
    }
    try {
      const payload = await loadTemplates();
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
      settingsState.runtimeSettings = await loadRuntimeSettings();
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
    });
    body.querySelector('[data-close-settings]')?.addEventListener('click', onClose);
    body.querySelector('[data-open-account-settings]')?.addEventListener('click', onAccount);
    body.querySelectorAll('[data-settings-tab]').forEach((button) => {
      button.addEventListener('click', () => {
        settingsState.tab = button.dataset.settingsTab;
        settingsState.commandStatus = '';
        render();
        if (settingsState.tab === 'runtime' && !settingsState.runtimeSettings) {
          refreshRuntimeSettings();
        }
        if (settingsState.tab === 'admin' && settingsState.templates === null) {
          refreshManagedModules();
        }
      });
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
        settingsState.runtimeSettings = await saveRuntimeSettings(runtimePayloadFromForm(body));
        settingsState.commandStatus = 'Runtime/Auth gespeichert.';
      } catch (error) {
        settingsState.commandStatus = String(error?.message || error);
      }
      render();
    });
    body.querySelector('[data-user-save]')?.addEventListener('click', async () => {
      const id = body.querySelector('[data-user-id]')?.value?.trim();
      const displayName = body.querySelector('[data-user-name]')?.value?.trim();
      const roleValue = body.querySelector('[data-user-role]')?.value || 'user';
      if (!id || !displayName) return;
      settingsState.commandStatus = 'Nutzer wird gespeichert...';
      render();
      try {
        const payload = await fetchJson('/api/business-os/users', {
          method: 'POST',
          headers: authHeaders(),
          body: JSON.stringify({ id, display_name: displayName, role: roleValue, active: true }),
        });
        settingsState.users = payload.users || [];
        settingsState.canManageUsers = payload.can_manage ?? true;
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
          settingsState.governance = await assignFounder(moduleId, userId, true);
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
          settingsState.governance = await releaseModule(moduleId);
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
          settingsState.governance = await rollbackModule(moduleId, versionId);
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
        await saveModule(payload);
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
        if (!moduleId || !window.confirm(`Modul ${moduleId} wirklich löschen?`)) return;
        settingsState.commandStatus = 'Modul wird gelöscht...';
        render();
        try {
          await deleteModule(moduleId);
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
          await installTemplate({ templateId, moduleId: id, title });
        } else {
          await saveModule({
            id,
            title,
            description,
            entry: `installed-modules/${slugify(id)}/index.html`,
            collections: ['business_commands'],
            layout: { shell: 'pane', center: 'module workspace' },
          });
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
  loadUsers().then((payload) => {
    settingsState.users = payload.users || [];
    settingsState.canManageUsers = payload.can_manage === true;
    render();
  }).catch(() => {});
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
      ${tabButton('sync', 'Sync', tab)}
      ${tabButton('users', 'Nutzer', tab)}
      ${canOpenAdmin ? tabButton('admin', 'Module', tab) : ''}
    </nav>

    <div class="settings-scroll">
      ${tab === 'runtime' ? runtimePanel(isAdmin, runtimeSettings, runtimeLoading) : ''}
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
  const authMode = auth.mode || 'api_key';
  const needsAttention = Boolean(diagnostics.needs_attention);
  const canManage = Boolean(isAdmin && runtimeSettings?.can_manage !== false);
  return `
    <section class="settings-section ${needsAttention ? 'is-danger' : 'is-ok'}">
      <header>
        <h3>CTOX Status</h3>
        <span>${escapeHtml(runtimeLoading ? 'Prüfe Runtime...' : needsAttention ? 'Eingriff erforderlich' : 'Konfiguriert')}</span>
      </header>
      <div class="settings-alert ${needsAttention ? 'is-danger' : 'is-ok'}">
        <strong>${escapeHtml(needsAttention ? 'CTOX arbeitet gerade nicht korrekt.' : 'CTOX Runtime/Auth wirkt konfiguriert.')}</strong>
        <span>${escapeHtml(diagnostics.message || 'Runtime-Status wird geladen.')}</span>
      </div>
      <dl class="settings-kv">
        ${kv('Quelle', runtime.source || '-')}
        ${kv('Provider', provider)}
        ${kv('Auth', auth.subscription_selected ? 'ChatGPT Subscription' : auth.api_key_configured ? `${auth.api_key_name} gesetzt` : `${auth.api_key_name || 'API Key'} fehlt`)}
        ${kv('Letzter Fehler', diagnostics.last_error || '-')}
      </dl>
    </section>
    <section class="settings-section">
      <header>
        <h3>Model Runtime</h3>
        <span>${escapeHtml(canManage ? 'Chef/Admin kann Runtime direkt ändern.' : 'Nur Chef und Admin dürfen ändern.')}</span>
      </header>
      <div class="settings-grid">
        <label><span>Provider</span><select data-runtime-provider ${canManage ? '' : 'disabled'}>
          ${option('local', 'Local CTOX', provider)}
          ${option('openai', 'OpenAI', provider)}
          ${option('openrouter', 'OpenRouter', provider)}
          ${option('anthropic', 'Anthropic', provider)}
          ${option('minimax', 'MiniMax', provider)}
        </select></label>
        <label><span>Auth</span><select data-runtime-auth-mode ${canManage ? '' : 'disabled'}>
          ${option('api_key', 'API Key', authMode)}
          ${option('chatgpt_subscription', 'ChatGPT Subscription', authMode)}
        </select></label>
        <label><span>Chat Modell</span><select data-runtime-model ${canManage ? '' : 'disabled'}>
          ${option('gpt-5.5', 'gpt-5.5', runtime.chat_model)}
          ${option('gpt-5.4', 'gpt-5.4', runtime.chat_model)}
          ${option('gpt-5.3-codex', 'gpt-5.3-codex', runtime.chat_model)}
          ${option('claude-opus-4-6', 'claude-opus-4-6', runtime.chat_model)}
          ${option('openrouter/minimax/m2.7', 'openrouter/minimax/m2.7', runtime.chat_model)}
        </select></label>
        <label><span>Context</span><select data-runtime-context ${canManage ? '' : 'disabled'}>
          ${option('128k', '128k', runtime.context)}
          ${option('256k', '256k', runtime.context)}
        </select></label>
        <label><span>Max Run</span><input data-runtime-timeout inputmode="numeric" value="${escapeAttr(runtime.max_run_secs || 1800)}" ${canManage ? '' : 'disabled'} /></label>
        <label><span>${escapeHtml(auth.api_key_name || 'API Key')}</span><input data-runtime-api-key type="password" autocomplete="off" placeholder="${escapeAttr(auth.api_key_configured ? 'gespeichert - leer lassen' : 'API Key eingeben')}" ${canManage ? '' : 'disabled'} /></label>
      </div>
      ${canManage ? `<button class="text-button settings-primary" type="button" data-runtime-save>Runtime/Auth speichern</button>` : ''}
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

function kv(key, value) {
  return `<div><dt>${escapeHtml(key)}</dt><dd>${escapeHtml(String(value || '-'))}</dd></div>`;
}

function runtimePayloadFromForm(root) {
  return {
    provider: root.querySelector('[data-runtime-provider]')?.value || 'local',
    auth_mode: root.querySelector('[data-runtime-auth-mode]')?.value || 'api_key',
    chat_model: root.querySelector('[data-runtime-model]')?.value || 'gpt-5.5',
    context: root.querySelector('[data-runtime-context]')?.value || '256k',
    max_run_secs: Number(root.querySelector('[data-runtime-timeout]')?.value || 1800),
    api_key: root.querySelector('[data-runtime-api-key]')?.value || '',
  };
}

function resolveRole(session) {
  const user = session?.user || {};
  if (user.role) return normalizeRole(user.role);
  if (user.is_admin === true || user.id === 'local-dev') return 'admin';
  return session?.authenticated ? 'user' : 'guest';
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

async function loadUsers() {
  return fetchJson('/api/business-os/users', { headers: authHeaders() });
}

async function loadRuntimeSettings() {
  return fetchJson('/api/business-os/ctox/runtime-settings', { headers: authHeaders() });
}

async function saveRuntimeSettings(payload) {
  return fetchJson('/api/business-os/ctox/runtime-settings', {
    method: 'POST',
    headers: authHeaders(),
    body: JSON.stringify(payload),
  });
}

async function loadModules() {
  try {
    return await fetchJson('/api/business-os/modules', { headers: authHeaders() });
  } catch {
    return fetchJson('modules/registry.json');
  }
}

async function loadTemplates() {
  try {
    return await fetchJson('/api/business-os/templates', { headers: authHeaders() });
  } catch {
    return { ok: true, templates: [] };
  }
}

async function saveModule(payload) {
  return fetchJson('/api/business-os/modules', {
    method: 'POST',
    headers: authHeaders(),
    body: JSON.stringify(payload),
  });
}

async function assignFounder(moduleId, userId, active) {
  return fetchJson('/api/business-os/modules/assign-founder', {
    method: 'POST',
    headers: authHeaders(),
    body: JSON.stringify({ module_id: moduleId, user_id: userId, active }),
  });
}

async function releaseModule(moduleId) {
  return fetchJson('/api/business-os/modules/release', {
    method: 'POST',
    headers: authHeaders(),
    body: JSON.stringify({ module_id: moduleId, notes: 'Business OS module release' }),
  });
}

async function rollbackModule(moduleId, versionId) {
  return fetchJson('/api/business-os/modules/rollback', {
    method: 'POST',
    headers: authHeaders(),
    body: JSON.stringify({ module_id: moduleId, version_id: versionId }),
  });
}

async function deleteModule(moduleId) {
  return fetchJson('/api/business-os/modules/delete', {
    method: 'POST',
    headers: authHeaders(),
    body: JSON.stringify({ module_id: moduleId }),
  });
}

async function installTemplate({ templateId, moduleId, title }) {
  return fetchJson('/api/business-os/modules/install-template', {
    method: 'POST',
    headers: authHeaders(),
    body: JSON.stringify({
      template_id: templateId,
      module_id: moduleId,
      title,
    }),
  });
}

async function fetchJson(url, options = {}) {
  const headers = {
    ...(options.headers || {}),
  };
  if (options.body) headers['Content-Type'] = 'application/json';
  const res = await fetch(url, {
    cache: 'no-store',
    ...options,
    headers,
  });
  if (!res.ok) throw new Error(`${url} returned ${res.status}`);
  return res.json();
}

function authHeaders() {
  const token = localStorage.getItem('ctox.businessOs.sessionToken')?.trim();
  const authHeader = localStorage.getItem('ctox.businessOs.authHeader')?.trim();
  if (token) return { 'X-CTOX-Business-OS-Session': token };
  if (authHeader) return { Authorization: authHeader };
  return {};
}

function cssEscape(value) {
  if (globalThis.CSS?.escape) return CSS.escape(value);
  return String(value).replace(/["\\\]]/g, '\\$&');
}
