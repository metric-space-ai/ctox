import { showBusinessConfirm } from './dialogs.js?v=20260714-chat-queue-v56';
import { appReleaseProjection } from './app-lifecycle.js?v=20260714-chat-queue-v56';
import {
  BusinessOsPermissions,
  canModifyBusinessModule,
  canUseBusinessPermission,
} from './permissions.js?v=20260714-chat-queue-v56';
import {
  brandingExportJson,
  normalizeBrandingImportPayload,
  WORKSPACE_BRANDING_COLLECTION,
  WORKSPACE_BRANDING_DOCUMENT_ID,
} from './branding.js?v=20260714-chat-queue-v56';
import {
  assignableRolesForActor,
  normalizeRole,
  roleCanManage,
  roleDisplayName,
} from './roles.js?v=20260714-chat-queue-v56';
import { renderModuleWhyDiagnosticsHtml } from './shell-permissions-ui.js?v=20260714-chat-queue-v56';

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
  const canManageBranding = canUseBusinessPermission({
    session,
    governance,
    permission: BusinessOsPermissions.WorkspaceBrandingManage,
    scopeType: 'workspace',
  });
  const settingsState = {
    tab: initialTab || 'runtime',
    commandStatus: '',
    subscriptionAuth: null,
    runtimeSettings: null,
    runtimeLoading: false,
    users: null,
    canManageUsers: false,
    activity: {
      events: [],
      loading: false,
      loaded: false,
      error: '',
    },
    branding: {
      loading: false,
      document: null,
      jsonText: '',
      error: '',
      canManage: canManageBranding,
    },
    mcp: {
      loading: false,
      info: null,
      error: '',
      copied: '',
    },
    modules: Array.isArray(modules) ? modules : [],
    governance,
    templates: null,
    editingModuleId: '',
    moduleWhyDiagnostics: {},
    moduleWhyStatus: {},
    moduleSupportDiagnostics: {},
    moduleSupportStatus: {},
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
  let usersSub = null;
  let activityRetryTimer = null;
  let activityRetryAttempts = 0;
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
  const ensureBrandingCollections = async () => {
    await Promise.allSettled([
      sync?.startCollection?.(WORKSPACE_BRANDING_COLLECTION),
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
  const startUsersSub = () => {
    if (usersSub || !db?.collection?.('business_users')?.$) return;
    usersSub = db.collection('business_users').$.subscribe(() => {
      if (!body.isConnected) {
        try { usersSub?.unsubscribe?.(); } catch {}
        usersSub = null;
        return;
      }
      refreshUsers().catch(() => {});
    });
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

  const refreshBranding = async () => {
    if (!settingsState.branding.canManage) return;
    settingsState.branding = { ...settingsState.branding, loading: true, error: '' };
    render();
    try {
      await ensureBrandingCollections();
      const document = await loadWorkspaceBranding({ db });
      settingsState.branding = {
        ...settingsState.branding,
        loading: false,
        document,
        jsonText: brandingExportJson(document),
        error: '',
      };
      settingsState.commandStatus = '';
    } catch (error) {
      settingsState.branding = {
        ...settingsState.branding,
        loading: false,
        error: String(error?.message || error),
      };
    }
    render();
  };

  const refreshActivity = async () => {
    if (!isAdmin) return;
    if (activityRetryTimer) {
      clearTimeout(activityRetryTimer);
      activityRetryTimer = null;
    }
    settingsState.activity = {
      ...settingsState.activity,
      loading: true,
      error: '',
    };
    render();
    try {
      const payload = await loadBusinessActivity({ commandBus, db, session, sync });
      settingsState.activity = {
        events: Array.isArray(payload.events) ? payload.events : [],
        loading: false,
        loaded: true,
        error: '',
      };
      activityRetryAttempts = 0;
    } catch (error) {
      const transient = isTransientActivityLoadError(error);
      settingsState.activity = {
        ...settingsState.activity,
        loading: false,
        loaded: !transient,
        error: String(error?.message || error),
      };
      if (transient && settingsState.tab === 'activity' && activityRetryAttempts < 5) {
        activityRetryAttempts += 1;
        const delayMs = Math.min(1000 * activityRetryAttempts, 5000);
        activityRetryTimer = setTimeout(() => {
          activityRetryTimer = null;
          if (settingsState.tab === 'activity') refreshActivity();
        }, delayMs);
      }
    }
    render();
  };
  const refreshMcpConnectInfo = async () => {
    if (!isAdmin) return;
    settingsState.mcp = {
      ...settingsState.mcp,
      loading: true,
      error: '',
      copied: '',
    };
    render();
    try {
      settingsState.mcp = {
        ...settingsState.mcp,
        loading: false,
        info: await loadMcpConnectInfo(),
        error: '',
        copied: '',
      };
    } catch (error) {
      settingsState.mcp = {
        ...settingsState.mcp,
        loading: false,
        error: String(error?.message || error),
      };
    }
    render();
  };
  const activateSettingsTab = (nextTab) => {
    if (!nextTab || settingsState.tab === nextTab) return;
    // Leaving the channels tab? Cancel any in-flight QR polling.
    if (settingsState.tab === 'channels' && nextTab !== 'channels') {
      if (settingsState.channels?.qrPoll) {
        clearInterval(settingsState.channels.qrPoll);
        settingsState.channels.qrPoll = null;
      }
    }
    settingsState.tab = nextTab;
    settingsState.commandStatus = '';
    render();
    if (settingsState.tab === 'runtime' && !settingsState.runtimeSettings) {
      refreshRuntimeSettings();
    }
    if (settingsState.tab === 'appearance' && settingsState.branding.canManage && !settingsState.branding.document) {
      refreshBranding();
    }
    if (settingsState.tab === 'admin' && settingsState.templates === null) {
      refreshManagedModules();
    }
    if (settingsState.tab === 'channels') {
      ensureChannelCollections().then(refreshChannelAccounts).catch(refreshChannelAccounts);
      startChannelAccountsSub();
    }
    if (settingsState.tab === 'activity' && isAdmin && !settingsState.activity.loaded) {
      refreshActivity();
    }
    if (settingsState.tab === 'mcp' && isAdmin && !settingsState.mcp.info && !settingsState.mcp.loading) {
      refreshMcpConnectInfo();
    }
  };
  body.addEventListener('click', (event) => {
    const button = event.target.closest?.('[data-settings-tab]');
    if (!button || !body.contains(button)) return;
    event.preventDefault();
    event.stopPropagation();
    activateSettingsTab(button.dataset.settingsTab || '');
  });
  const revokeModuleSupportDownloadUrl = (moduleId) => {
    const status = settingsState.moduleSupportStatus?.[moduleId];
    if (status?.downloadUrl && globalThis.URL?.revokeObjectURL) {
      try { globalThis.URL.revokeObjectURL(status.downloadUrl); } catch {}
    }
  };
  const revokeModuleSupportDownloadUrls = () => {
    Object.keys(settingsState.moduleSupportStatus || {}).forEach(revokeModuleSupportDownloadUrl);
  };

  const render = () => {
    const canOpenAdmin = canOpenModuleAdmin({
      modules: settingsState.modules.length ? settingsState.modules : modules,
      session,
      governance: settingsState.governance,
    });
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
      subscriptionAuth: settingsState.subscriptionAuth,
      runtimeSettings: settingsState.runtimeSettings,
      runtimeLoading: settingsState.runtimeLoading,
      users: settingsState.users,
      canManageUsers: settingsState.canManageUsers,
      activity: settingsState.activity,
      branding: settingsState.branding,
      editingModuleId: settingsState.editingModuleId,
      moduleWhyDiagnostics: settingsState.moduleWhyDiagnostics,
      moduleWhyStatus: settingsState.moduleWhyStatus,
      moduleSupportDiagnostics: settingsState.moduleSupportDiagnostics,
      moduleSupportStatus: settingsState.moduleSupportStatus,
      governance: settingsState.governance,
      channels: settingsState.channels,
      mcp: settingsState.mcp,
    });
    body.querySelector('[data-close-settings]')?.addEventListener('click', () => {
      try { channelsAccountsSub?.unsubscribe?.(); } catch {}
      channelsAccountsSub = null;
      try { usersSub?.unsubscribe?.(); } catch {}
      usersSub = null;
      if (settingsState.channels?.qrPoll) {
        clearInterval(settingsState.channels.qrPoll);
        settingsState.channels.qrPoll = null;
      }
      if (activityRetryTimer) {
        clearTimeout(activityRetryTimer);
        activityRetryTimer = null;
      }
      revokeModuleSupportDownloadUrls();
      onClose?.();
    });
    body.querySelector('[data-open-account-settings]')?.addEventListener('click', onAccount);
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
      const runtimePayload = runtimePayloadFromForm(body);
      settingsState.commandStatus = 'Runtime/Auth wird gespeichert...';
      render();
      try {
        settingsState.runtimeSettings = await saveRuntimeSettings(
          runtimePayload,
          { commandBus, db, session, sync },
        );
        settingsState.commandStatus = 'Runtime/Auth gespeichert.';
      } catch (error) {
        settingsState.commandStatus = String(error?.message || error);
      }
      render();
    });
    body.querySelector('[data-runtime-refresh]')?.addEventListener('click', refreshRuntimeSettings);
    body.querySelector('[data-branding-refresh]')?.addEventListener('click', refreshBranding);
    body.querySelector('[data-branding-save]')?.addEventListener('click', async () => {
      const input = body.querySelector('[data-branding-json]');
      const raw = input?.value || '';
      settingsState.commandStatus = 'Corporate Design wird gespeichert...';
      settingsState.branding = { ...settingsState.branding, error: '' };
      render();
      try {
        const payload = normalizeBrandingImportPayload(raw);
        settingsState.branding.document = await saveWorkspaceBranding(
          payload,
          { commandBus, db, session, sync },
        );
        settingsState.branding.jsonText = brandingExportJson(settingsState.branding.document);
        settingsState.commandStatus = 'Corporate Design gespeichert.';
      } catch (error) {
        settingsState.branding = {
          ...settingsState.branding,
          error: String(error?.message || error),
        };
        settingsState.commandStatus = String(error?.message || error);
      }
      render();
    });
    body.querySelector('[data-branding-reset]')?.addEventListener('click', async () => {
      const confirmed = await showBusinessConfirm('Corporate Design auf CTOX Default zuruecksetzen?', {
        title: 'Design zuruecksetzen',
        confirmLabel: 'Zuruecksetzen',
      });
      if (!confirmed) return;
      settingsState.commandStatus = 'Corporate Design wird zurueckgesetzt...';
      render();
      try {
        settingsState.branding.document = await resetWorkspaceBranding({ commandBus, db, session, sync });
        settingsState.branding.jsonText = brandingExportJson(settingsState.branding.document);
        settingsState.branding.error = '';
        settingsState.commandStatus = 'Corporate Design zurueckgesetzt.';
      } catch (error) {
        settingsState.branding = {
          ...settingsState.branding,
          error: String(error?.message || error),
        };
        settingsState.commandStatus = String(error?.message || error);
      }
      render();
    });
    body.querySelector('[data-activity-refresh]')?.addEventListener('click', refreshActivity);
    body.querySelector('[data-mcp-refresh]')?.addEventListener('click', refreshMcpConnectInfo);
    body.querySelectorAll('[data-mcp-copy]').forEach((button) => {
      button.addEventListener('click', async () => {
        const value = mcpCopyValue(button.dataset.mcpCopy, settingsState.mcp.info);
        if (!value) return;
        try {
          await navigator.clipboard?.writeText?.(value);
          settingsState.mcp = { ...settingsState.mcp, copied: button.dataset.mcpCopy };
        } catch {
          settingsState.mcp = { ...settingsState.mcp, copied: 'failed' };
        }
        render();
      });
    });
    body.querySelector('[data-runtime-authorize-subscription]')?.addEventListener('click', async () => {
      const authWindow = window.open('', 'ctox-chatgpt-subscription');
      settingsState.subscriptionAuth = { status: 'starting', message: 'Geräte-Code wird bei CTOX angefordert.' };
      settingsState.commandStatus = 'ChatGPT Login wird vorbereitet...';
      const runtimePayload = runtimePayloadFromForm(body);
      writeSubscriptionAuthWindow(
        authWindow,
        'ChatGPT Login wird vorbereitet',
        'CTOX fordert den Geräte-Code an. Der Code erscheint gleich in den Settings.',
      );
      render();
      try {
        settingsState.runtimeSettings = runtimeSettingsWithDraft(
          settingsState.runtimeSettings,
          runtimePayload,
        );
        const payload = await startSubscriptionAuth({ commandBus, db, session, sync });
        if (!payload.auth_url && !payload.verification_url) throw new Error('CTOX hat keine Login-URL geliefert.');
        if (payload.status === 'device_code' && payload.user_code) {
          settingsState.subscriptionAuth = {
            status: 'device_code',
            userCode: payload.user_code,
            verificationUrl: payload.verification_url || payload.auth_url,
            source: payload.source || 'ctox',
            message: 'Code im OpenAI-Fenster eingeben.',
          };
          settingsState.commandStatus = `ChatGPT Geräte-Code: ${payload.user_code}. Code im OpenAI-Fenster eingeben.`;
          render();
        }
        const authUrl = payload.auth_url || payload.verification_url;
        if (authWindow && !authWindow.closed) {
          authWindow.location.href = authUrl;
        } else {
          window.location.href = authUrl;
        }
        settingsState.commandStatus = payload.user_code
          ? `ChatGPT Geräte-Code: ${payload.user_code}. Danach Status neu laden.`
          : 'ChatGPT Login geöffnet. Danach Status neu laden.';
        saveRuntimeSettings(runtimePayload, {
          commandBus,
          db,
          session,
          sync,
          waitForProjection: false,
        }).catch((error) => {
          const message = `Runtime konnte nach Start des ChatGPT Logins nicht gespeichert werden: ${String(error?.message || error)}`;
          settingsState.commandStatus = settingsState.subscriptionAuth?.userCode
            ? `${settingsState.commandStatus} ${message}`
            : message;
          render();
        });
        setTimeout(refreshRuntimeSettings, 3000);
        setTimeout(refreshRuntimeSettings, 9000);
        setTimeout(refreshRuntimeSettings, 30000);
        setTimeout(refreshRuntimeSettings, 90000);
      } catch (error) {
        writeSubscriptionAuthWindow(
          authWindow,
          'ChatGPT Login konnte nicht gestartet werden',
          String(error?.message || error),
          true,
        );
        settingsState.subscriptionAuth = {
          status: 'failed',
          error: String(error?.message || error),
        };
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
        settingsState.users = confirmedUsersAfterUpsert(
          settingsState.users,
          payload.users,
          { id, display_name: displayName, role: roleValue, active: true, updated_at_ms: Date.now() },
          session,
        );
        settingsState.canManageUsers = roleCanManage(resolveRole(session));
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
    body.querySelectorAll('[data-module-why]').forEach((button) => {
      button.addEventListener('click', async () => {
        const moduleId = button.dataset.moduleWhy || '';
        if (!moduleId) return;
        settingsState.moduleWhyStatus = {
          ...settingsState.moduleWhyStatus,
          [moduleId]: { loading: true, error: '' },
        };
        settingsState.commandStatus = `Zugriff für ${moduleId} wird geprüft...`;
        render();
        try {
          const diagnostics = await loadModuleWhyDiagnostics(moduleId, {
            commandBus,
            db,
            session,
            sync,
          });
          settingsState.moduleWhyDiagnostics = {
            ...settingsState.moduleWhyDiagnostics,
            [moduleId]: diagnostics,
          };
          settingsState.moduleWhyStatus = {
            ...settingsState.moduleWhyStatus,
            [moduleId]: { loading: false, error: '' },
          };
          settingsState.commandStatus = `Zugriff für ${moduleId} geprüft.`;
        } catch (error) {
          settingsState.moduleWhyStatus = {
            ...settingsState.moduleWhyStatus,
            [moduleId]: { loading: false, error: String(error?.message || error) },
          };
          settingsState.commandStatus = String(error?.message || error);
        }
        render();
      });
    });
    body.querySelectorAll('[data-module-support-diagnostics]').forEach((button) => {
      button.addEventListener('click', async () => {
        const moduleId = button.dataset.moduleSupportDiagnostics || '';
        if (!moduleId) return;
        revokeModuleSupportDownloadUrl(moduleId);
        settingsState.moduleSupportStatus = {
          ...settingsState.moduleSupportStatus,
          [moduleId]: { loading: true, error: '', downloadUrl: '', downloadName: '' },
        };
        settingsState.commandStatus = `Support-Paket für ${moduleId} wird erstellt...`;
        render();
        try {
          const artifact = await exportSupportDiagnosticsArtifact(moduleId, {
            commandBus,
            db,
            session,
            sync,
          });
          const download = createSupportDiagnosticsDownload(artifact, moduleId);
          settingsState.moduleSupportDiagnostics = {
            ...settingsState.moduleSupportDiagnostics,
            [moduleId]: artifact,
          };
          settingsState.moduleSupportStatus = {
            ...settingsState.moduleSupportStatus,
            [moduleId]: {
              loading: false,
              error: '',
              downloadUrl: download.url,
              downloadName: download.filename,
            },
          };
          settingsState.commandStatus = `Support-Paket für ${moduleId} erstellt.`;
        } catch (error) {
          settingsState.moduleSupportStatus = {
            ...settingsState.moduleSupportStatus,
            [moduleId]: {
              loading: false,
              error: String(error?.message || error),
              downloadUrl: '',
              downloadName: '',
            },
          };
          settingsState.commandStatus = String(error?.message || error);
        }
        render();
      });
    });
    body.querySelectorAll('[data-founder-save]').forEach((button) => {
      button.addEventListener('click', async () => {
        const moduleId = button.dataset.founderSave || '';
        const userId = body.querySelector(`[data-founder-user="${cssEscape(moduleId)}"]`)?.value?.trim() || '';
        if (!moduleId || !userId) return;
        settingsState.commandStatus = 'Verantwortlichen-Zuordnung wird gespeichert...';
        render();
        try {
          settingsState.governance = await assignFounder(moduleId, userId, true, { commandBus, db, session });
          settingsState.commandStatus = `${userId} ist verantwortlich für ${moduleId}.`;
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
  if (settingsState.tab === 'appearance' && settingsState.branding.canManage) {
    refreshBranding();
  }
  refreshUsers();
  startUsersSub();
  if (settingsState.tab === 'admin' && canOpenModuleAdmin({
    modules: settingsState.modules.length ? settingsState.modules : modules,
    session,
    governance: settingsState.governance,
  })) {
    refreshManagedModules();
  }
  if (settingsState.tab === 'activity' && isAdmin) {
    refreshActivity();
  }
  if (settingsState.tab === 'mcp' && isAdmin) {
    refreshMcpConnectInfo();
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
  subscriptionAuth,
  runtimeSettings,
  runtimeLoading,
  users,
  canManageUsers,
  activity,
  branding,
  editingModuleId,
  moduleWhyDiagnostics,
  moduleWhyStatus,
  moduleSupportDiagnostics,
  moduleSupportStatus,
  governance,
  channels,
  mcp,
}) {
  return `
    <header class="drawer-header-row settings-head">
      ${settingsPreferenceControls()}
      <button class="icon-button" type="button" data-close-settings aria-label="Schließen">×</button>
    </header>

    <section class="settings-user-card">
      <div class="settings-avatar" aria-hidden="true">${escapeHtml(initials(user.display_name || user.id || 'CTOX'))}</div>
      <div>
        <strong>${escapeHtml(user.display_name || user.id || 'Nicht eingeloggt')}</strong>
        <span>${escapeHtml(user.id || 'keine Session')}</span>
      </div>
      <mark class="role-badge">${escapeHtml(roleDisplayName(role))}</mark>
    </section>

    <nav class="settings-tabs" aria-label="Settings Bereiche">
      ${tabButton('runtime', 'Runtime', tab)}
      ${tabButton('channels', 'Channels', tab)}
      ${tabButton('sync', 'Sync', tab)}
      ${branding?.canManage ? tabButton('appearance', 'Design', tab) : ''}
      ${isAdmin ? tabButton('mcp', 'MCP', tab) : ''}
      ${tabButton('users', 'Nutzer', tab)}
      ${isAdmin ? tabButton('activity', 'Aktivität', tab) : ''}
      ${canOpenAdmin ? tabButton('admin', 'Module', tab) : ''}
    </nav>

    <div class="settings-scroll">
      ${tab === 'runtime' ? runtimePanel(isAdmin, runtimeSettings, runtimeLoading, subscriptionAuth) : ''}
      ${tab === 'channels' ? channelsPanel(channels) : ''}
      ${tab === 'sync' ? syncPanel(syncConfig, isAdmin) : ''}
      ${tab === 'appearance' && branding?.canManage ? appearancePanel(branding) : ''}
      ${tab === 'mcp' && isAdmin ? mcpPanel(mcp) : ''}
      ${tab === 'users' ? usersPanel(user, role, isAdmin, users, canManageUsers) : ''}
      ${tab === 'activity' && isAdmin ? activityPanel(activity) : ''}
      ${tab === 'admin' && canOpenAdmin ? adminPanel(managedModules || modules, templates, editingModuleId, {
    isAdmin,
    role,
    user,
    governance,
    moduleWhyDiagnostics,
    moduleWhyStatus,
    moduleSupportDiagnostics,
    moduleSupportStatus,
  }) : ''}
    </div>

    <footer class="settings-footer">
      <button class="text-button" type="button" data-open-account-settings>Account</button>
      <button class="text-button" type="button" data-logout-settings>Logout</button>
      ${commandStatus ? `<span class="settings-status">${escapeHtml(commandStatus)}</span>` : ''}
    </footer>
  `;
}

function settingsPreferenceControls() {
  const lang = document.documentElement.lang === 'en' ? 'en' : 'de';
  const shellStyle = document.documentElement.dataset.shellStyle === 'macos' ? 'macos' : 'windows';
  const theme = document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
  const copy = lang === 'en'
    ? {
        group: 'Appearance and language',
        shellStyle: 'Window',
        shellStyleAria: 'Style',
        language: 'Language',
        languageAria: 'Language',
        theme: 'Scheme',
        themeAria: 'Theme',
      }
    : {
        group: 'Darstellung und Sprache',
        shellStyle: 'Fenster',
        shellStyleAria: 'Stil',
        language: 'Sprache',
        languageAria: 'Sprache',
        theme: 'Schema',
        themeAria: 'Design Theme',
      };
  return `
    <div class="settings-preferences" aria-label="${escapeAttr(copy.group)}" data-shell-t-aria="appearanceSettings">
      <label class="settings-preference-control">
        <span data-shell-t="shellStyleLabel">${escapeHtml(copy.shellStyle)}</span>
        <select class="header-select" data-shell-style-select aria-label="${escapeAttr(copy.shellStyleAria)}" data-shell-t-aria="shellStyleAria">
          ${option('windows', 'Windows', shellStyle)}
          ${option('macos', 'macOS', shellStyle)}
        </select>
      </label>
      <label class="settings-preference-control">
        <span data-shell-t="languageLabel">${escapeHtml(copy.language)}</span>
        <select class="header-select" data-language-select aria-label="${escapeAttr(copy.languageAria)}" data-shell-t-aria="languageAria">
          ${option('de', 'DE', lang)}
          ${option('en', 'EN', lang)}
        </select>
      </label>
      <label class="settings-preference-control">
        <span data-shell-t="themeLabel">${escapeHtml(copy.theme)}</span>
        <select class="header-select" data-theme-select aria-label="${escapeAttr(copy.themeAria)}" data-shell-t-aria="themeAria">
          ${option('dark', 'Dark', theme)}
          ${option('light', 'Light', theme)}
        </select>
      </label>
    </div>
  `;
}

function runtimePanel(isAdmin, runtimeSettings, runtimeLoading, subscriptionAuth = null) {
  if (runtimeLoading && !runtimeSettings) {
    return `
      <section class="settings-section">
        <header><h3>Model Runtime</h3><span>Status wird gelesen.</span></header>
        <div class="runtime-status-strip">
          ${runtimePill('Modelle', 'Status wird gelesen.', false)}
          ${runtimePill('Autorisierung', 'Status wird gelesen.', false)}
          ${runtimePill('CTOX Service', 'Status wird gelesen.', false)}
          ${runtimePill('Route', 'Status wird gelesen.', false)}
        </div>
      </section>
      <section class="settings-section">
        <header><h3>Queue Policy</h3><span>Operative Arbeit läuft über CTOX Tasks.</span></header>
      </section>
    `;
  }
  const runtime = runtimeSettings?.runtime || {};
  const auth = runtimeSettings?.auth || {};
  const diagnostics = runtimeSettings?.diagnostics || {};
  const provider = String(runtime.provider || '').trim().toLowerCase();
  const providerLoaded = Boolean(provider);
  const authMode = normalizedRuntimeAuthMode(provider, auth.mode);
  const isLocalProvider = provider === 'local';
  const usesSubscription = provider === 'openai' && isSubscriptionMode(authMode);
  const usesApiKey = providerLoaded && !isLocalProvider && !usesSubscription;
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
        ${runtimePill('Modelle', providerLoaded ? `${runtimeProviderLabel(provider)}${runtime.chat_model ? ` · ${runtime.chat_model}` : ''}` : 'nicht geladen', false)}
        ${runtimePill('Autorisierung', runtimeAuthSummary(provider, authMode, auth), authNeedsAttention)}
        ${runtimePill('CTOX Service', diagnostics.service_message || 'Status unbekannt', serviceNeedsAttention)}
        ${runtimePill('Route', runtimeRouteSummary(runtime, provider, auth), false)}
      </div>
      <div class="settings-grid">
        <label><span>Provider</span><select data-runtime-provider ${canManage ? '' : 'disabled'}>
          ${providerLoaded ? '' : option('', 'Nicht geladen', provider)}
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
          ${option('256k', '256k', runtimeContextValue(runtime.context))}
        </select></label>
        <label><span>Max Run</span><input data-runtime-timeout inputmode="numeric" value="${escapeAttr(runtime.max_run_secs || 1800)}" ${canManage ? '' : 'disabled'} /></label>
        ${usesApiKey ? `<label><span>${escapeHtml(auth.api_key_name || 'API Key')}</span><input data-runtime-api-key type="password" autocomplete="off" placeholder="${escapeAttr(auth.api_key_configured ? 'gespeichert - leer lassen' : 'API Key eingeben')}" ${canManage ? '' : 'disabled'} /></label>` : ''}
      </div>
      ${usesSubscription ? subscriptionStatus(auth, canManage, subscriptionAuth) : ''}
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
        <label><span>Verantwortlichen-Prüfung</span><select data-policy-review ${isAdmin ? '' : 'disabled'}><option value="strict-founder-review">Externe Nachrichten immer prüfen</option><option value="internal-autonomy">Interne Tasks autonom</option></select></label>
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

function appearancePanel(branding = {}) {
  const document = branding.document || {};
  const isCustom = document.custom === true;
  const jsonText = branding.jsonText || brandingExportJson(document);
  return `
    <section class="settings-section">
      <header>
        <h3>Corporate Design</h3>
        <span>${escapeHtml(branding.loading ? 'Branding wird gelesen.' : (isCustom ? document.name || 'Workspace Branding' : 'CTOX Default'))}</span>
      </header>
      <dl class="settings-kv">
        ${kv('Aktiv', isCustom ? 'Custom Branding' : 'CTOX Default')}
        ${kv('Name', document.name || 'CTOX Default')}
        ${kv('Light Tokens', String(Object.keys(document.light || {}).length))}
        ${kv('Dark Tokens', String(Object.keys(document.dark || {}).length))}
      </dl>
      <div class="settings-grid is-one">
        <label><span>Branding JSON</span><textarea data-branding-json rows="14" spellcheck="false">${escapeHtml(jsonText)}</textarea></label>
      </div>
      <div class="runtime-actions">
        <button class="text-button settings-primary" type="button" data-branding-save ${branding.loading ? 'disabled' : ''}>Branding importieren</button>
        <button class="text-button" type="button" data-branding-refresh ${branding.loading ? 'disabled' : ''}>Neu laden</button>
        <button class="text-button" type="button" data-branding-reset ${branding.loading ? 'disabled' : ''}>CTOX Default</button>
      </div>
      ${branding.error ? `<p class="settings-note">${escapeHtml(branding.error)}</p>` : ''}
    </section>
  `;
}

function mcpPanel(mcp = {}) {
  const info = mcp.info || null;
  const codexConfig = info ? JSON.stringify(info.codex || {}, null, 2) : '';
  const claudeConfig = info ? JSON.stringify(info.claude || {}, null, 2) : '';
  const managed = info?.managed || null;
  const copied = mcp.copied || '';
  const managedStatus = managed?.status || 'nicht geladen';
  const managedReady = managedStatus === 'ready';
  const effectiveStatus = mcpEffectiveStatus(info, mcp.error);
  return `
    <section class="settings-section">
      <header>
        <h3>Business OS MCP</h3>
        <span>${escapeHtml(mcp.loading ? 'Verbindung wird gelesen.' : mcpStatusLabel(info, mcp.error))}</span>
      </header>
      <dl class="settings-kv">
        ${kv('Status', effectiveStatus)}
        ${kv('Modus', info?.mode || '-')}
        ${kv('Server', info?.server_name || '-')}
        ${kv('Lokaler Endpoint', info?.endpoint || '-')}
        ${kv('Managed Endpoint', managed?.endpoint || '-')}
        ${kv('Managed Status', managedStatus)}
        ${kv('Aktive Managed Tokens', managed?.active_token_count ?? '-')}
        ${kv('Instanz', managed?.instance_alias || info?.managed_instance_id || '-')}
        ${kv('Lokales Secret', info?.secret ? `${info.secret.scope}/${info.secret.name}` : 'business_os/mcp_inbound_auth_token')}
      </dl>
      <div class="runtime-actions">
        <button class="text-button settings-primary" type="button" data-mcp-refresh ${mcp.loading ? 'disabled' : ''}>MCP Status laden</button>
        ${info ? `<button class="text-button" type="button" data-mcp-copy="managedEndpoint">Managed Endpoint kopieren</button>` : ''}
      </div>
      ${mcp.error ? `<p class="settings-note">${escapeHtml(mcp.error)}</p>` : ''}
      ${copied ? `<p class="settings-note">${escapeHtml(copied === 'failed' ? 'Kopieren fehlgeschlagen.' : 'In die Zwischenablage kopiert.')}</p>` : ''}
      ${managed && !managedReady ? `<p class="settings-note">Managed MCP ist ${escapeHtml(managedStatus)}. Agent Tokens werden im ctox.dev Dashboard rotiert.</p>` : ''}
    </section>
    ${info ? `
      <section class="settings-section">
        <header><h3>Lokale Codex / Claude Config</h3><span>Nur fuer Agenten mit Zugriff auf 127.0.0.1 dieser Instanz.</span></header>
        <div class="settings-grid is-one">
          <label><span>Lokaler Bearer Token</span><input type="password" readonly value="${escapeAttr(info.token || '')}" /></label>
          <label><span>Codex JSON</span><textarea readonly rows="8">${escapeHtml(codexConfig)}</textarea></label>
          <label><span>Claude JSON</span><textarea readonly rows="8">${escapeHtml(claudeConfig)}</textarea></label>
        </div>
        <div class="runtime-actions">
          <button class="text-button settings-primary" type="button" data-mcp-copy="codex">Codex Config kopieren</button>
          <button class="text-button" type="button" data-mcp-copy="claude">Claude Config kopieren</button>
          <button class="text-button" type="button" data-mcp-copy="authHeader">Authorization Header kopieren</button>
        </div>
        <p class="settings-note">Dieser lokale Token ist nicht der mcp.ctox.dev Bearer. Managed Agent Tokens werden im ctox.dev Dashboard rotiert.</p>
      </section>
    ` : ''}
  `;
}

function mcpEffectiveStatus(info, error) {
  if (error) return 'Fehler';
  if (!info) return 'Noch nicht geladen';
  if (info.managed?.status === 'ready') return 'Managed bereit';
  if (info.status === 'local_ready_managed_not_connected') return 'Lokal bereit; Managed Web Auth fehlt';
  if (info.status === 'ready') return 'Bereit';
  return String(info.status || 'Status unbekannt');
}

function mcpStatusLabel(info, error) {
  if (error) return 'Nicht verbunden.';
  if (!info) return 'Noch nicht geladen.';
  if (info.managed?.status === 'ready') return 'Managed MCP bereit fuer externe Coding Agents.';
  if (info.status === 'local_ready_managed_not_connected') return 'Lokal bereit; Managed Web Auth fehlt.';
  return info.status === 'ready' ? 'Bereit fuer externe MCP Clients.' : String(info.status || 'Status unbekannt.');
}

function mcpCopyValue(kind, info) {
  if (!info) return '';
  if (kind === 'endpoint') return info.endpoint || '';
  if (kind === 'managedEndpoint') return info.managed?.endpoint || '';
  if (kind === 'token') return info.token || '';
  if (kind === 'authHeader') return info.authorization_header || (info.token ? `Bearer ${info.token}` : '');
  if (kind === 'codex') return JSON.stringify(info.codex || {}, null, 2);
  if (kind === 'claude') return JSON.stringify(info.claude || {}, null, 2);
  return '';
}

function usersPanel(user, role, isAdmin, users, canManageUsers) {
  const rows = Array.isArray(users) && users.length ? users : [{
    id: user.id || '-',
    display_name: user.display_name || '-',
    role,
    active: true,
  }];
  const roleOptions = assignableRolesForActor(role);
  return `
    <section class="settings-section">
      <header><h3>Aktive Sitzung</h3><span>${escapeHtml(roleDisplayName(role))} Session</span></header>
      <table class="settings-table">
        <tbody>
          <tr><th>Teammitglied</th><td>${escapeHtml(user.display_name || user.id || '-')}</td></tr>
          <tr><th>ID</th><td>${escapeHtml(user.id || '-')}</td></tr>
          <tr><th>Rolle</th><td>${escapeHtml(roleDisplayName(role))}</td></tr>
        </tbody>
      </table>
    </section>
    <section class="settings-section">
      <header><h3>Team & Zugaenge</h3><span>${escapeHtml(canManageUsers ? 'Persistenter Business-OS Team Store.' : 'Nur eigene Sitzung sichtbar.')}</span></header>
      <table class="settings-table">
        <thead><tr><th>Teammitglied</th><th>Rolle</th><th>Status</th></tr></thead>
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
          <input data-user-id placeholder="team-id" />
          <input data-user-name placeholder="Anzeigename" />
          <select data-user-role>
            ${roleOptions.map((option) => `<option value="${escapeAttr(option)}">${escapeHtml(roleDisplayName(option))}</option>`).join('')}
          </select>
          <button class="text-button settings-primary" type="button" data-user-save>Nutzer speichern</button>
        </div>
      ` : `<p class="settings-note">Nutzerverwaltung ist für Admins sichtbar.</p>`}
    </section>
  `;
}

function activityPanel(activity = {}) {
  const events = Array.isArray(activity.events) ? activity.events : [];
  const summary = activity.loading
    ? 'Aktivität wird geladen.'
    : `${events.length} Einträge`;
  return `
    <section class="settings-section">
      <header>
        <h3>Aktivität</h3>
        <span>${escapeHtml(summary)}</span>
      </header>
      <div class="runtime-actions">
        <button class="text-button settings-primary" type="button" data-activity-refresh ${activity.loading ? 'disabled' : ''}>Neu laden</button>
      </div>
      ${activity.error ? `<p class="settings-note">${escapeHtml(activity.error)}</p>` : ''}
      ${events.length ? `
        <table class="settings-table">
          <thead><tr><th>Ereignis</th><th>Ausgeführt von</th><th>Zeit</th></tr></thead>
          <tbody>
            ${events.map(activityRow).join('')}
          </tbody>
        </table>
      ` : `<p class="settings-note">${escapeHtml(activity.loaded ? 'Noch keine Aktivität.' : 'Noch nicht geladen.')}</p>`}
    </section>
  `;
}

function activityRow(event) {
  const payload = event?.payload || {};
  const actor = payload.actor || {};
  const observedAt = Number(event?.observed_at_ms || payload.observed_at_ms || 0);
  return `
    <tr>
      <td>
        <strong>${escapeHtml(activityTitle(event))}</strong>
        <small>${escapeHtml(activityDetail(event))}</small>
      </td>
      <td>${escapeHtml(actor.display_name || actor.id || '-')}</td>
      <td>${escapeHtml(formatMsShort(observedAt))}</td>
    </tr>
  `;
}

function activityTitle(event) {
  const type = String(event?.type || event?.payload?.event_type || '');
  if (type === 'business_os.policy.allowed') return 'Aktion erlaubt';
  if (type === 'business_os.policy.denied') return 'Aktion blockiert';
  if (type === 'business_os.user.changed') return 'Teammitglied aktualisiert';
  if (type === 'business_os.app_responsibility.changed') return 'App-Verantwortung aktualisiert';
  if (type === 'business_os.external_approval.decided') return 'Externe Freigabe entschieden';
  if (type === 'business_os.module.release.succeeded') return 'App-Version veröffentlicht';
  if (type === 'business_os.module.release.failed') return 'App-Freigabe fehlgeschlagen';
  if (type === 'business_os.module.rollback.succeeded') return 'App-Rollback angewendet';
  if (type === 'business_os.module.rollback.failed') return 'App-Rollback fehlgeschlagen';
  return 'Aktivität';
}

function activityDetail(event) {
  const payload = event?.payload || {};
  const type = String(event?.type || payload.event_type || '');
  if (type === 'business_os.policy.allowed') {
    return `${activityCommandLabel(payload.command_type)} wurde erlaubt.`;
  }
  if (type === 'business_os.policy.denied') {
    return `${activityCommandLabel(payload.command_type)} wurde blockiert.`;
  }
  if (type === 'business_os.user.changed') {
    const previous = payload.previous || {};
    const current = payload.current || {};
    const name = current.display_name || current.id || payload.user_id || event?.record_id || 'Teammitglied';
    if (previous.role && current.role && previous.role !== current.role) {
      return `${name}: ${roleDisplayName(previous.role)} -> ${roleDisplayName(current.role)}`;
    }
    if (typeof previous.active === 'boolean' && typeof current.active === 'boolean' && previous.active !== current.active) {
      return `${name}: ${current.active ? 'aktiviert' : 'deaktiviert'}`;
    }
    return `${name} wurde aktualisiert.`;
  }
  if (type === 'business_os.app_responsibility.changed') {
    const current = payload.current || {};
    const moduleId = payload.module_id || current.module_id || 'App';
    const userId = payload.user_id || current.user_id || 'Teammitglied';
    const active = current.active !== false;
    return active
      ? `${userId} ist verantwortlich für ${moduleId}.`
      : `${userId} ist nicht mehr verantwortlich für ${moduleId}.`;
  }
  if (type === 'business_os.external_approval.decided') {
    const message = payload.message || {};
    const messageId = payload.message_id || message.id || 'Nachricht';
    return `${messageId}: ${approvalDecisionLabel(payload.decision)}`;
  }
  if (type === 'business_os.module.release.succeeded' || type === 'business_os.module.release.failed') {
    const summary = payload.summary || {};
    const moduleId = summary.module_id || payload.record_id || event?.record_id || 'App';
    const version = activityVersionLabel(summary.target_version || summary.version_id);
    const audience = releaseChannelLabel(summary.release_channel);
    return type.endsWith('.succeeded')
      ? `${moduleId}: Version ${version} wurde für ${audience} veröffentlicht.`
      : `${moduleId}: Version ${version} konnte nicht veröffentlicht werden.`;
  }
  if (type === 'business_os.module.rollback.succeeded' || type === 'business_os.module.rollback.failed') {
    const summary = payload.summary || {};
    const moduleId = summary.module_id || payload.record_id || event?.record_id || 'App';
    const version = activityVersionLabel(summary.target_version
      || summary.rollback_version_id
      || summary.version_id
      || summary.rolled_back_to
      || '');
    return type.endsWith('.succeeded')
      ? `${moduleId}: Rollback auf ${version} wurde angewendet.`
      : `${moduleId}: Rollback auf ${version} konnte nicht angewendet werden.`;
  }
  return 'Business-OS Aktivität.';
}

function approvalDecisionLabel(decision) {
  return {
    approved: 'freigegeben',
    rejected: 'abgelehnt',
    changes_requested: 'Änderungen angefordert',
  }[String(decision || '')] || 'entschieden';
}

function releaseChannelLabel(channel) {
  return {
    team: 'Team',
    restricted: 'ausgewählte Personen',
    private: 'nur mich',
  }[String(channel || '')] || 'Team';
}

function activityVersionLabel(value) {
  const text = String(value || '').trim();
  if (!text) return 'gewählte Version';
  if (/^v?\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(text)) return text.replace(/^v/i, '');
  if (/^(?:version|modver|modrel|cmd|evt)[_-]/i.test(text) || /^[0-9a-f]{8,}(?:-[0-9a-f]{4,})*/i.test(text)) {
    return 'gewählte Version';
  }
  return text;
}

function activityCommandLabel(commandType) {
  return {
    'ctox.runtime_settings.save': 'Runtime ändern',
    'ctox.subscription_auth.start': 'ChatGPT Login starten',
    'ctox.business_os.user.upsert': 'Teammitglied speichern',
    'ctox.business_os.audit.list': 'Aktivität ansehen',
    'ctox.channel.configure': 'Channel konfigurieren',
    'ctox.source.save': 'App-Datei speichern',
    'ctox.source.rollback_snapshot': 'App-Datei zurückrollen',
    'ctox.module.release': 'Version speichern',
    'ctox.module.assign_founder': 'Verantwortliche:n zuweisen',
    'ctox.module.save': 'Modul ändern',
    'ctox.module.delete': 'Modul löschen',
    'ctox.module.install_template': 'Modul hinzufügen',
    'ctox.module.rollback': 'Rollback anwenden',
    'ctox.module.rollback_version': 'Rollback anwenden',
    'ctox.business_os.why': 'Zugriff erklären',
    'ctox.business_os.support.export_diagnostics': 'Support-Diagnose exportieren',
    'ctox.app_store.install': 'App installieren',
    'ctox.app_store.uninstall': 'App entfernen',
  }[String(commandType || '')] || 'Aktion';
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
	    ${permissions.isAdmin ? agentGrantBoundaryPanel(permissions.governance, modules) : ''}
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
        <label><span>Inbound</span><select data-inbound-policy><option value="business-os">Business OS Commands</option><option value="founder">Verantwortlichen-Nachrichten</option><option value="tickets">Tickets / Issues</option></select></label>
        <label><span>Outbound</span><select data-outbound-policy><option value="strict-founder-review">Verantwortlichen-Prüfung</option><option value="internal-autonomy">Interne Autonomie</option></select></label>
      </div>
	      <button class="text-button settings-primary" type="button" data-settings-command="routing">Routing Policy an CTOX geben</button>
	    </section>` : ''}
	  `;
}

function agentGrantBoundaryPanel(governance, modules = []) {
  const grants = explicitGrantRows(governance);
  return `
    <section class="settings-section" data-agent-grant-boundary>
      <header>
        <h3>Agent- und App-Zugriff</h3>
        <span>${escapeHtml(`${grants.length} Sonderfreigaben`)}</span>
      </header>
      <p class="settings-note">Diese Ansicht zeigt aktive Sonderfreigaben aus der CTOX Policy. Änderungen laufen über Owner/Admin-Policy, nicht direkt in Settings.</p>
      ${grants.length ? `
        <table class="settings-table agent-grants-table">
          <thead><tr><th>Wer</th><th>Darf</th><th>Wo</th><th>Status</th></tr></thead>
          <tbody>
            ${grants.map((grant) => `
              <tr>
                <td><strong>${escapeHtml(subjectGrantLabel(grant))}</strong><small>${escapeHtml(grant.subject_id || '')}</small></td>
                <td>${escapeHtml(permissionGrantLabel(grant.permission))}</td>
                <td>${escapeHtml(scopeGrantLabel(grant, modules))}</td>
                <td>${escapeHtml(grant.active === false ? 'Inaktiv' : 'Aktiv')}</td>
              </tr>
            `).join('')}
          </tbody>
        </table>
      ` : '<p class="settings-note">Keine Sonderfreigaben. Teammitglieder sehen Team-Versionen und nutzen nur die Datenrechte aus ihrer Rolle oder App-Zuweisung.</p>'}
    </section>
  `;
}

function explicitGrantRows(governance) {
  const grants = governance?.permission_model?.explicit_grants
    || governance?.governance?.permission_model?.explicit_grants
    || [];
  return (Array.isArray(grants) ? grants : [])
    .filter((grant) => grant && grant.active !== false)
    .map((grant) => ({
      grant_id: String(grant.grant_id || ''),
      subject_type: String(grant.subject_type || ''),
      subject_id: String(grant.subject_id || ''),
      permission: String(grant.permission || ''),
      scope_type: String(grant.scope_type || ''),
      scope_id: String(grant.scope_id || ''),
      active: grant.active,
    }));
}

function subjectGrantLabel(grant) {
  const type = String(grant?.subject_type || '').toLowerCase();
  if (['agent', 'mcp_actor', 'service_actor'].includes(type)) return 'Agent';
  if (type === 'user') return 'Teammitglied';
  if (type === 'role') return 'Rolle';
  if (type === 'team') return 'Team';
  return 'Akteur';
}

function permissionGrantLabel(permission) {
  return {
    'apps.view': 'App sehen',
    'apps.modify': 'App ändern',
    'apps.source.view': 'Source ansehen',
    'apps.release': 'Version freigeben',
    'apps.rollback': 'Rollback anwenden',
    'data.read': 'Daten lesen',
    'data.write': 'Daten ändern',
    'external.approve': 'Externe Wirkung freigeben',
    'mcp.manage': 'MCP verwalten',
    'users.manage': 'Team verwalten',
  }[String(permission || '')] || 'Sonderrecht';
}

function businessCollectionLabel(collectionId) {
  return String(collectionId || '')
    .split(/[_\s-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ') || 'Daten';
}

function scopeGrantLabel(grant, modules = []) {
  const type = String(grant?.scope_type || '').toLowerCase();
  const id = String(grant?.scope_id || '').trim();
  if (type === 'workspace') return 'Workspace';
  if (type === 'module') {
    const mod = (Array.isArray(modules) ? modules : []).find((item) => item?.id === id);
    return `App ${mod?.title || id}`;
  }
  if (type === 'collection') return `Datenbereich ${businessCollectionLabel(id)} (${id})`;
  if (type === 'record') return `Datensatz ${id}`;
  if (type === 'task') return `Task ${id}`;
  if (type === 'approval') return `Freigabe ${id}`;
  if (type === 'mcp') return `MCP ${id || 'Business OS'}`;
  return id || 'Scope';
}

function moduleRow(mod, editingModuleId, permissions) {
  const kind = moduleKind(mod);
  const canDelete = moduleCanDelete(mod);
  const releases = releasesForModule(permissions.governance, mod.id);
  const founders = foundersForModule(permissions.governance, mod.id);
  const releaseFacts = moduleReleaseProjectionHtml(mod);
  const whyStatus = permissions.moduleWhyStatus?.[mod.id] || {};
  const whyDiagnostics = permissions.moduleWhyDiagnostics?.[mod.id] || null;
  const whyHtml = moduleWhyDiagnosticsHtml(whyDiagnostics, whyStatus);
  const supportStatus = permissions.moduleSupportStatus?.[mod.id] || {};
  const supportDiagnostics = permissions.moduleSupportDiagnostics?.[mod.id] || null;
  const supportHtml = moduleSupportDiagnosticsHtml(supportDiagnostics, supportStatus, mod);
  return `
    <tr ${editingModuleId === mod.id ? 'aria-current="true"' : ''}>
      <td>
        <strong>${escapeHtml(mod.title || mod.id)}</strong>
        <small>${escapeHtml(mod.id)}</small>
      </td>
	      <td>
	        ${escapeHtml(kind)}
	        ${founders.length ? `<small>Verantwortlich: ${founders.map((item) => escapeHtml(item.user_id)).join(', ')}</small>` : ''}
	        ${releaseFacts}
	      </td>
	      <td>
	        <div class="module-admin-actions">
	          <button class="text-button" type="button" data-module-edit="${escapeAttr(mod.id)}">Editieren</button>
	          <button class="text-button" type="button" data-module-why="${escapeAttr(mod.id)}" ${whyStatus.loading ? 'disabled' : ''}>Warum?</button>
	          ${permissions.isAdmin ? `<button class="text-button" type="button" data-module-support-diagnostics="${escapeAttr(mod.id)}" ${supportStatus.loading ? 'disabled' : ''}>Support-Paket</button>` : ''}
	          ${moduleReleaseDiagnosticsHtml(mod, releases)}
	          <button class="text-button" type="button" data-module-delete="${escapeAttr(mod.id)}" ${canDelete ? '' : 'disabled'}>Löschen</button>
	        </div>
	        ${whyHtml}
	        ${supportHtml}
	        ${permissions.isAdmin ? `
	          <div class="module-admin-actions">
	            <input data-founder-user="${escapeAttr(mod.id)}" placeholder="team user-id" />
	            <button class="text-button" type="button" data-founder-save="${escapeAttr(mod.id)}">Verantwortliche:n zuweisen</button>
	          </div>
	        ` : ''}
	      </td>
	    </tr>
	  `;
}

function moduleWhyDiagnosticsHtml(diagnostics, status = {}) {
  if (status.loading) {
    return '<p class="settings-note" data-module-why-status>Zugriff wird geprüft...</p>';
  }
  if (status.error) {
    return `<p class="settings-note" data-module-why-status>${escapeHtml(status.error)}</p>`;
  }
  if (!diagnostics || typeof diagnostics !== 'object') return '';
  return renderModuleWhyDiagnosticsHtml({
    view: nativeWhyDiagnosticsView(diagnostics),
    labels: { whyTitle: 'Warum?' },
  });
}

function moduleSupportDiagnosticsHtml(artifact, status = {}, mod = {}) {
  if (status.loading) {
    return '<p class="settings-note" data-module-support-status>Support-Paket wird erstellt...</p>';
  }
  if (status.error) {
    return `<p class="settings-note" data-module-support-status>${escapeHtml(status.error)}</p>`;
  }
  if (!artifact || typeof artifact !== 'object') return '';
  const moduleId = String(mod?.id || artifact?.scope?.module_id || '');
  const schema = String(artifact.artifact_schema || '');
  const schemaVersion = Number(artifact.schema_version || 0);
  const profile = String(artifact.redaction?.profile || '');
  const generatedAt = formatMsShort(artifact.generated_at_ms);
  const activityCount = Number(artifact.activity?.count || 0);
  const why = artifact.diagnostics?.why && typeof artifact.diagnostics.why === 'object'
    ? artifact.diagnostics.why
    : null;
  const whyModule = why?.module || {};
  const lifecycle = why?.lifecycle || {};
  const decisions = why?.decisions || {};
  const dataAreas = Array.isArray(why?.data_access?.areas) ? why.data_access.areas : [];
  const readableAreas = dataAreas.filter((area) => supportDecisionAllowed(area?.read_decision)).length;
  const writableAreas = dataAreas.filter((area) => supportDecisionAllowed(area?.write_decision)).length;
  const visibilityText = supportVisibilityText(lifecycle, whyModule, mod);
  const modifyText = supportDecisionAllowed(decisions.modify) ? 'Ändern erlaubt' : 'Ändern gesperrt';
  const releaseText = supportDecisionAllowed(decisions.release) ? 'Freigabe erlaubt' : 'Freigabe gesperrt';
  const downloadHtml = status.downloadUrl
    ? `<a class="text-button" href="${escapeAttr(status.downloadUrl)}" download="${escapeAttr(status.downloadName || supportDiagnosticsFilename(moduleId))}" data-support-diagnostics-download="${escapeAttr(moduleId)}">JSON herunterladen</a>`
    : '<span class="settings-note">Download wird vorbereitet.</span>';
  return `
    <div class="module-support-diagnostics" data-support-diagnostics="${escapeAttr(moduleId)}" data-support-schema="${escapeAttr(schema)}" data-redaction-profile="${escapeAttr(profile)}">
      <strong class="module-support-title">Support-Paket</strong>
      <dl>
        <div data-support-row="schema">
          <dt>Format</dt>
          <dd><strong>${escapeHtml(supportArtifactSchemaLabel(schema))}</strong><span>${escapeHtml(schemaVersion ? `Schema-Version ${schemaVersion}` : 'Schema-Version geprüft')}</span></dd>
        </div>
        <div data-support-row="redaction">
          <dt>Schutz</dt>
          <dd><strong>${escapeHtml(supportRedactionProfileLabel(profile))}</strong><span>Keine Nachrichteninhalte, KI-Eingaben, Datensatzinhalte oder Zugangswerte enthalten.</span></dd>
        </div>
        <div data-support-row="scope">
          <dt>Umfang</dt>
          <dd><strong>${escapeHtml(mod?.title || whyModule.title || moduleId || 'Business OS')}</strong><span>${escapeHtml(visibilityText)}</span></dd>
        </div>
        <div data-support-row="activity">
          <dt>Aktivität</dt>
          <dd><strong>${escapeHtml(`${activityCount} Ereignis${activityCount === 1 ? '' : 'se'} zusammengefasst`)}</strong><span>Erstellt ${escapeHtml(generatedAt)}</span></dd>
        </div>
        <div data-support-row="why">
          <dt>Zugriff</dt>
          <dd><strong>${escapeHtml(`${modifyText} · ${releaseText}`)}</strong><span>${escapeHtml(`${dataAreas.length} Datenbereich(e): ${readableAreas} lesbar, ${writableAreas} schreibbar`)}</span></dd>
        </div>
      </dl>
      <div class="module-support-actions">${downloadHtml}</div>
    </div>
  `;
}

function nativeWhyDiagnosticsView(diagnostics = {}) {
  const app = diagnostics.app || diagnostics.module || {};
  const actor = diagnostics.actor || {};
  const actionDecisions = diagnostics.action_decisions || diagnostics.decisions || {};
  const lifecycle = diagnostics.lifecycle || {};
  const visibility = diagnostics.visibility || actionDecisions.visibility || {};
  const open = actionDecisions.open || {};
  const modify = actionDecisions.modify || {};
  const source = actionDecisions.source || {};
  const release = actionDecisions.release || {};
  const rollback = actionDecisions.rollback || {};
  const dataAreas = Array.isArray(diagnostics.data_areas)
    ? diagnostics.data_areas
    : (Array.isArray(diagnostics.data_access?.areas) ? diagnostics.data_access.areas : []);
  const dataSummary = nativeWhyDataSummary(dataAreas);
  const rows = [
    {
      key: 'actor',
      label: 'Akteur',
      state: 'info',
      value: nativeWhyActorLabel(actor),
      reason: `Entscheidungen gelten für diese App: ${cleanDiagnosticText(app.title || app.module_id || 'App')}`,
    },
    {
      key: 'visibility',
      label: 'Sichtbarkeit',
      state: nativeWhyDecisionState(visibility.allowed),
      value: [cleanDiagnosticText(lifecycle.visibility_state || visibility.value || 'Unklar'), nativeWhyVersionLabel(app)]
        .filter(Boolean)
        .join(' · '),
      reason: nativeWhyReason(visibility, 'App-Sichtbarkeit folgt Version, Sichtbarkeitsstatus und App-Zuweisung.'),
    },
    nativeWhyDecisionRow('open', 'App öffnen', open, 'Öffnen folgt der App-Sichtbarkeit; Datenzugriff wird danach separat geprüft.'),
    nativeWhyDecisionRow('modify', 'App ändern', modify, 'Ändern bleibt App-Verantwortlichen, Admins oder Ownern vorbehalten.'),
    nativeWhyDecisionRow('source', 'Source öffnen', source, 'Source bleibt ohne Source-Recht oder App-Verantwortung verborgen.'),
    nativeWhyDecisionRow('release', 'Freigabe', release, 'Freigaben brauchen ein Release-Recht für diese App.'),
    nativeWhyDecisionRow('rollback', 'Rollback', rollback, 'Rollback braucht ein Rollback-Recht für diese App.'),
    {
      key: 'data',
      label: 'Datenbereiche',
      state: dataAreas.length && dataAreas.some((area) => (
        nativeWhyAreaDecision(area, 'read').allowed === false
          || nativeWhyAreaDecision(area, 'write').allowed === false
      ))
        ? 'blocked'
        : 'info',
      value: dataSummary,
      reason: 'App-Sichtbarkeit gibt keine Datenrechte frei; Lesen und Schreiben werden pro Datenbereich geprüft.',
    },
  ];
  return {
    rows,
    actor: {
      id: cleanDiagnosticText(actor.id),
      label: cleanDiagnosticText(actor.display_name || actor.id) || 'Unbekannter Nutzer',
      role: cleanDiagnosticText(actor.role || 'user'),
      is_admin: actor.is_admin === true,
    },
    app: {
      module_id: cleanDiagnosticText(app.module_id || app.id),
      module_title: cleanDiagnosticText(app.title || app.module_id || app.id),
      version: nativeWhyVersionLabel(app),
      visibility: cleanDiagnosticText(lifecycle.visibility_state || visibility.value),
      lifecycle_state: cleanDiagnosticText(lifecycle.visibility_state),
      public: visibility.allowed === true,
      runtime_installed: lifecycle.runtime_installed === true || app.runtime_installed === true,
      can_see: visibility.allowed === true,
      can_open: open.allowed === true,
      can_modify: modify.allowed === true,
      can_open_source: source.allowed === true,
      can_release: release.allowed === true,
      can_rollback: rollback.allowed === true,
    },
    release: {
      line: nativeWhyDecisionLabel(release.allowed),
      rollback_line: nativeWhyDecisionLabel(rollback.allowed),
      status: '',
      status_label: '',
      has_release_state: false,
    },
    data: {
      summary: dataSummary,
      status: '',
      status_label: '',
      declared_collections: dataAreas.map((area) => cleanDiagnosticText(area.collection_id)).filter(Boolean),
      granted_collections: dataAreas
        .filter((area) => (
          nativeWhyAreaDecision(area, 'read').allowed === true
            || nativeWhyAreaDecision(area, 'write').allowed === true
        ))
        .map((area) => cleanDiagnosticText(area.collection_id || area.collection))
        .filter(Boolean),
      locked_collections: dataAreas
        .filter((area) => (
          nativeWhyAreaDecision(area, 'read').allowed === false
            && nativeWhyAreaDecision(area, 'write').allowed === false
        ))
        .map((area) => cleanDiagnosticText(area.collection_id || area.collection))
        .filter(Boolean),
      review_note: '',
      grants_implied: false,
      decisions: dataAreas.map(nativeWhyDataDecisionRow).filter(Boolean),
    },
  };
}

function nativeWhyDecisionRow(key, label, decision, fallbackReason) {
  return {
    key,
    label,
    state: nativeWhyDecisionState(decision?.allowed),
    value: nativeWhyDecisionLabel(decision?.allowed),
    reason: nativeWhyReason(decision, fallbackReason),
  };
}

function nativeWhyDataDecisionRow(area) {
  const collection = cleanDiagnosticText(area?.collection_id || area?.collection);
  if (!collection) return null;
  const readDecision = nativeWhyAreaDecision(area, 'read');
  const writeDecision = nativeWhyAreaDecision(area, 'write');
  return {
    collection,
    label: businessCollectionLabel(collection),
    read: {
      allowed: readDecision.allowed === true,
      label: `Lesen ${nativeWhyDecisionLabel(readDecision.allowed)}`,
      reason: nativeWhyReason(readDecision, 'Lesen wird über Datenrechte für diesen Datenbereich entschieden.'),
    },
    write: {
      allowed: writeDecision.allowed === true,
      label: `Schreiben ${nativeWhyDecisionLabel(writeDecision.allowed)}`,
      reason: nativeWhyReason(writeDecision, 'Schreiben wird über Datenrechte für diesen Datenbereich entschieden.'),
    },
  };
}

function nativeWhyDataSummary(dataAreas) {
  if (!Array.isArray(dataAreas) || !dataAreas.length) return 'Keine Datenbereiche deklariert';
  const readable = dataAreas.filter((area) => nativeWhyAreaDecision(area, 'read').allowed === true).length;
  const writable = dataAreas.filter((area) => nativeWhyAreaDecision(area, 'write').allowed === true).length;
  return `${dataAreas.length} Datenbereich(e): ${readable} lesbar, ${writable} schreibbar`;
}

function nativeWhyAreaDecision(area, mode) {
  if (!area || typeof area !== 'object') return {};
  if (mode === 'read') return area.read_decision || area.read || {};
  if (mode === 'write') return area.write_decision || area.write || {};
  return {};
}

function nativeWhyActorLabel(actor = {}) {
  const name = cleanDiagnosticText(actor.display_name || actor.id) || 'Unbekannter Nutzer';
  const role = cleanDiagnosticText(actor.role || 'user');
  return role ? `${name} · ${roleDisplayName(role)}` : name;
}

function nativeWhyVersionLabel(app = {}) {
  const version = cleanDiagnosticText(app.version || app.current_semver);
  return version ? (version.startsWith('v') ? version : `v${version}`) : '';
}

function nativeWhyDecisionState(allowed) {
  if (allowed === true) return 'allowed';
  if (allowed === false) return 'blocked';
  return 'info';
}

function nativeWhyDecisionLabel(allowed) {
  if (allowed === true) return 'Erlaubt';
  if (allowed === false) return 'Nicht erlaubt';
  return 'Unklar';
}

function nativeWhyReason(decision, fallback) {
  const text = cleanDiagnosticText(decision?.display_reason || decision?.reason);
  if (!text) return fallback;
  if (/^Allowed\.$/i.test(text)) return fallback;
  if (/^This role is not allowed to perform this action/i.test(text)) return fallback;
  return text;
}

function cleanDiagnosticText(value) {
  return String(value ?? '').trim();
}

function moduleReleaseDiagnosticsHtml(mod, releases) {
  const releaseRows = Array.isArray(releases) ? releases : [];
  return `
    <div class="module-admin-release-diagnostics" data-module-release-diagnostics="${escapeAttr(mod.id)}">
      <button class="text-button" type="button" disabled aria-disabled="true">Freigabe im App Store</button>
      ${releaseRows.length ? `
        <select disabled aria-label="Rollback-Versionen nur Diagnose">
          ${releaseRows.map((release) => `<option value="${escapeAttr(release.version_id)}">v${escapeHtml(release.version)} ${escapeHtml(release.status || '')}</option>`).join('')}
        </select>
        <button class="text-button" type="button" disabled aria-disabled="true">Rollback nur Diagnose</button>
      ` : ''}
      <small>Settings zeigt Release und Rollback nur als Diagnose; aktive Freigabe laeuft ueber den App Store Publish-Flow.</small>
    </div>
  `;
}

function moduleReleaseProjectionHtml(mod) {
  const projection = appReleaseProjection(mod);
  const dataAccess = projection.dataAccess || {};
  const hasDataAccessFact = dataAccess.hasReview === true
    || (Array.isArray(dataAccess.declared) && dataAccess.declared.length > 0)
    || (Array.isArray(dataAccess.granted) && dataAccess.granted.length > 0)
    || (Array.isArray(dataAccess.locked) && dataAccess.locked.length > 0);
  const facts = [];
  if (projection.hasReleaseState) {
    facts.push(['Freigabe', projection.releaseLine]);
  }
  if (projection.rollbackLine) {
    facts.push(['Rollback', projection.rollbackLine]);
  }
  if (hasDataAccessFact && dataAccess.summary) {
    facts.push(['Datenzugriff', dataAccess.summary]);
  }
  if (dataAccess.reviewNote) {
    facts.push(['Review', dataAccess.reviewNote]);
  }
  if (!facts.length) return '';
  return `
    <div class="module-admin-release-facts" data-module-release-projection="${escapeAttr(mod.id)}">
      ${facts.map(([label, value]) => `
        <small class="module-admin-release-fact"><b>${escapeHtml(label)}:</b> ${escapeHtml(value)}</small>
      `).join('')}
    </div>
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
      command_type: 'ctox.runtime.switch',
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
      command_type: 'ctox.business_os.sync.configure',
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
      command_type: 'ctox.communication_policy.verify',
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
      instruction: `${userAction}: öffne die CTOX Team- und Session-Verwaltung, prüfe Rollenrechte und bereite die Änderung für diese Business-OS-Instanz vor.`,
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
  if (!String(provider || '').trim()) return '';
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
  }[String(provider || '').toLowerCase()] || provider || 'nicht geladen';
}

function runtimeAuthSummary(provider, authMode, auth) {
  if (!String(provider || '').trim()) return 'nicht geladen';
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

function runtimeRouteSummary(runtime, provider, auth = {}) {
  const normalizedProvider = String(provider || '').toLowerCase();
  const source = String(runtime?.source || '').toLowerCase();
  const upstream = String(runtime?.upstream_base_url || '').trim();
  const keyName = String(auth?.api_key_name || '').trim();
  if (!normalizedProvider && !source && !upstream) return 'nicht geladen';
  if (normalizedProvider === 'local' || source === 'local') return 'Lokal';
  if (isCtoxProxyUpstream(upstream) || keyName === 'CTOX_LLM_PROXY_API_KEY') {
    return 'ctox.dev Proxy';
  }
  if (upstream) return hostLabel(upstream);
  return normalizedProvider === 'minimax' ? 'MiniMax API' : 'API';
}

function isCtoxProxyUpstream(value) {
  const normalized = String(value || '').trim().toLowerCase();
  return normalized.includes('llm.ctox.dev') || normalized.includes('/api/fallback-llm');
}

function hostLabel(value) {
  try {
    return new URL(value).host || value;
  } catch {
    return String(value || '').replace(/^https?:\/\//i, '').replace(/\/.*$/, '') || value;
  }
}

function runtimeModelControl(provider, model, canManage) {
  const value = String(model || '');
  if (!String(provider || '').trim()) {
    return `<label><span>Chat Modell</span><input data-runtime-model value="${escapeAttr(value)}" placeholder="Runtime nicht geladen" ${canManage ? '' : 'disabled'} /></label>`;
  }
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
      ['MiniMax-M3', 'MiniMax-M3'],
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

function subscriptionStatus(auth, canManage, subscriptionAuth = null) {
  const configured = Boolean(auth.subscription_session_configured);
  const userCode = String(subscriptionAuth?.userCode || '').trim();
  const failed = subscriptionAuth?.status === 'failed';
  const pending = subscriptionAuth?.status === 'starting';
  const lines = [];
  if (auth.subscription_account_email) lines.push(kv('Account', auth.subscription_account_email));
  if (auth.subscription_plan) lines.push(kv('Plan', auth.subscription_plan));
  return `
    <div class="runtime-auth-status ${configured ? 'is-ok' : 'is-danger'}">
      <strong>${escapeHtml(configured ? 'ChatGPT Subscription verbunden' : 'ChatGPT Subscription verbinden')}</strong>
      <span>${escapeHtml(configured ? 'OpenAI Modelle können diese Subscription verwenden.' : 'Öffnet den ChatGPT Login und speichert die Subscription für OpenAI Modelle.')}</span>
      ${pending ? `
        <div class="subscription-device-code is-pending">
          <span>Geräte-Code</span>
          <strong>wird angefordert</strong>
        </div>
      ` : ''}
      ${userCode ? `
        <div class="subscription-device-code">
          <span>Geräte-Code</span>
          <strong>${escapeHtml(formatDeviceCode(userCode))}</strong>
          <em>${escapeHtml(subscriptionAuth?.message || 'Im OpenAI-Fenster eingeben.')}</em>
        </div>
      ` : ''}
      ${failed ? `<div class="subscription-device-error">${escapeHtml(subscriptionAuth.error || 'ChatGPT Login konnte nicht gestartet werden.')}</div>` : ''}
      ${lines.length ? `<dl class="settings-kv">${lines.join('')}</dl>` : ''}
      ${canManage ? `<button class="text-button" type="button" data-runtime-authorize-subscription>${escapeHtml(configured ? 'Subscription erneuern' : 'Subscription verbinden')}</button>` : ''}
    </div>
  `;
}

function formatDeviceCode(value) {
  const compact = String(value || '').replace(/[^a-z0-9]/gi, '').toUpperCase();
  if (compact.length === 9) return `${compact.slice(0, 4)}-${compact.slice(4)}`;
  return String(value || '').trim().toUpperCase();
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
  const provider = root.querySelector('[data-runtime-provider]')?.value || '';
  if (!provider) {
    throw new Error('Runtime-Provider ist noch nicht geladen.');
  }
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
  const provider = draft.provider || current?.runtime?.provider || '';
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

function confirmedUsersAfterUpsert(currentUsers, payloadUsers, upsertedUser, session) {
  if (Array.isArray(payloadUsers) && payloadUsers.length) {
    return normalizeUsersForSession(payloadUsers, session);
  }
  const normalizedUpsert = normalizeUsersForSession([upsertedUser], session)[0];
  if (!normalizedUpsert) return normalizeUsersForSession(currentUsers, session);
  const existing = normalizeUsersForSession(currentUsers, session)
    .filter((user) => user.id !== normalizedUpsert.id);
  return [...existing, normalizedUpsert].sort((left, right) => {
    const leftName = left.display_name || left.id;
    const rightName = right.display_name || right.id;
    return leftName.localeCompare(rightName);
  });
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
  return canModifyBusinessModule(mod, {
    session: { user: { ...user, role, is_admin: isAdmin } },
    governance,
  });
}

function canOpenModuleAdmin({ modules, session, governance }) {
  if (roleCanManage(resolveRole(session))) return true;
  return (Array.isArray(modules) ? modules : []).some((mod) => canModifyBusinessModule(mod, {
    session,
    governance,
  }));
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

async function loadBusinessActivity({ commandBus, db, session, sync } = {}) {
  const startedAtMs = Date.now();
  const maxWaitMs = 28000;
  let lastError = null;
  await sync?.startCollection?.('business_commands')?.catch?.(() => {});
  const immediateFallback = await waitForLocalBusinessActivityFallback({
    db,
    deadlineMs: startedAtMs + 2000,
    minWaitMs: 2000,
  });
  if (immediateFallback?.events?.length) return immediateFallback;
  for (let attempt = 0; attempt < 5 && Date.now() - startedAtMs < maxWaitMs; attempt += 1) {
    try {
      const command = await dispatchModuleCommand({
        commandBus,
        db,
        session,
        sync,
        commandType: 'ctox.business_os.audit.list',
        moduleId: 'ctox',
        recordId: 'business-activity',
        payload: { limit: 50 },
        source: 'business-os-settings',
        timeoutMs: attempt === 0 ? 8000 : 6000,
        requireResult: true,
      });
      const payload = command.result || command;
      if (command.status === 'failed' || payload?.ok === false) {
        throw new Error(payload?.error || 'Aktivität konnte nicht geladen werden.');
      }
      return payload;
    } catch (error) {
      lastError = error;
      if (!isTransientActivityLoadError(error)) break;
      const elapsedMs = Date.now() - startedAtMs;
      if (elapsedMs >= maxWaitMs) break;
      await sync?.startCollection?.('business_commands')?.catch?.(() => {});
      const fallback = await waitForLocalBusinessActivityFallback({
        db,
        deadlineMs: startedAtMs + maxWaitMs,
        minWaitMs: attempt === 0 ? 2500 : 1000,
      });
      if (fallback?.events?.length) return fallback;
      await delay(Math.min(500 + (attempt * 500), 2000));
      await sync?.startCollection?.('business_commands')?.catch?.(() => {});
    }
  }
  throw lastError || new Error('Aktivität konnte nicht geladen werden.');
}

function isTransientActivityLoadError(error) {
  const text = String(error?.code || error?.message || error || '');
  return /projection_delayed|projection_pending|sync_unavailable|native_unavailable|WebRTC peer .* closed|peer-close|replication-cancel|QUERY_CANCELLED|IDBDatabase.*closing|database connection is closing/i.test(text);
}

async function loadLocalBusinessActivityFallback({ db } = {}) {
  const coll = db?.collection?.('business_commands');
  if (!coll?.find) return null;
  let docs = [];
  try {
    docs = await coll.find().exec();
  } catch {
    return null;
  }
  const events = docs
    .map((doc) => doc?.toJSON?.() || doc)
    .filter(Boolean)
    .sort((a, b) => Number(b.updated_at_ms || b.observed_at_ms || 0) - Number(a.updated_at_ms || a.observed_at_ms || 0))
    .map(localBusinessCommandActivityEvent)
    .filter(Boolean)
    .slice(0, 50);
  return events.length ? { ok: true, events } : null;
}

async function waitForLocalBusinessActivityFallback({ db, deadlineMs, minWaitMs = 0 } = {}) {
  const startedAtMs = Date.now();
  const deadline = Number(deadlineMs || 0) || startedAtMs;
  let fallback = null;
  do {
    fallback = await loadLocalBusinessActivityFallback({ db });
    if (fallback?.events?.length) return fallback;
    if (Date.now() >= deadline || Date.now() - startedAtMs >= minWaitMs) break;
    await delay(250);
  } while (Date.now() < deadline);
  return fallback;
}

function localBusinessCommandActivityEvent(command) {
  const nativeEventType = String(command?.type || command?.payload?.event_type || '');
  if (nativeEventType.startsWith('business_os.module.')) {
    return localBusinessNativeActivityEvent(command, nativeEventType);
  }
  const commandType = String(command?.command_type || command?.type || command?.payload?.type || '');
  const status = String(command?.status || command?.terminal_status || '');
  const eventType = localBusinessCommandActivityType(commandType, status);
  if (!eventType) return null;
  const commandId = String(command?.command_id || command?.id || command?.record_id || newId());
  const payload = command?.payload || {};
  const result = command?.result || {};
  const observedAtMs = Number(command?.updated_at_ms || command?.observed_at_ms || Date.now());
  return {
    id: `local_activity_${eventType}_${commandId}`,
    collection: 'business_commands',
    record_id: commandId,
    type: eventType,
    command_type: eventType,
    observed_at_ms: observedAtMs,
    payload: {
      event_type: eventType,
      command_id: commandId,
      module: String(command?.module || 'ctox'),
      command_type: commandType,
      record_id: command?.record_id || '',
      actor: command?.client_context?.actor || {},
      status,
      summary: localBusinessCommandActivitySummary(commandType, command),
      observed_at_ms: observedAtMs,
      reconstructed_from: 'business_commands',
    },
  };
}

function localBusinessNativeActivityEvent(command, eventType) {
  if (![
    'business_os.module.release.succeeded',
    'business_os.module.release.failed',
    'business_os.module.rollback.succeeded',
    'business_os.module.rollback.failed',
  ].includes(eventType)) {
    return null;
  }
  const commandId = String(command?.command_id || command?.id || command?.record_id || newId());
  const payload = command?.payload || {};
  const observedAtMs = Number(command?.observed_at_ms || command?.updated_at_ms || payload.observed_at_ms || Date.now());
  return {
    id: `local_activity_${eventType}_${commandId}`,
    collection: 'business_commands',
    record_id: String(command?.record_id || payload.record_id || commandId),
    type: eventType,
    command_type: eventType,
    observed_at_ms: observedAtMs,
    payload: {
      ...payload,
      event_type: eventType,
      command_id: commandId,
      record_id: command?.record_id || payload.record_id || '',
      actor: payload.actor || command?.client_context?.actor || {},
      summary: payload.summary || localBusinessCommandActivitySummary(String(command?.command_type || command?.payload?.type || ''), command),
      observed_at_ms: observedAtMs,
      reconstructed_from: payload.reconstructed_from || 'business_commands',
    },
  };
}

function localBusinessCommandActivityType(commandType, status) {
  if (commandType === 'ctox.module.release' && status === 'completed') return 'business_os.module.release.succeeded';
  if (commandType === 'ctox.module.release' && status === 'failed') return 'business_os.module.release.failed';
  if (commandType === 'ctox.module.rollback_version' && status === 'completed') return 'business_os.module.rollback.succeeded';
  if (commandType === 'ctox.module.rollback_version' && status === 'failed') return 'business_os.module.rollback.failed';
  return '';
}

function localBusinessCommandActivitySummary(commandType, command) {
  const payload = command?.payload || {};
  const result = command?.result || {};
  const recordId = String(command?.record_id || '');
  const moduleId = String(result.module_id || payload.module_id || recordId || 'App');
  return {
    ok: result.ok !== false,
    module_id: moduleId,
    version_id: String(result.version_id || payload.version_id || ''),
    target_version: String(result.target_version || payload.target_version || ''),
    release_channel: commandType === 'ctox.module.release'
      ? String(result.release_channel || payload.release_channel || 'team')
      : '',
    source_version_id: String(result.source_version_id || payload.source_version_id || ''),
    rollback_version_id: String(result.rollback_version_id || payload.rollback_version_id || payload.version_id || ''),
    rolled_back_at_ms: Number(result.rolled_back_at_ms || 0) || null,
  };
}

async function loadModuleWhyDiagnostics(moduleId, { commandBus, db, session, sync } = {}) {
  const normalizedModuleId = String(moduleId || '').trim();
  if (!normalizedModuleId) throw new Error('App fehlt für die Zugriffsdiagnose.');
  const command = await dispatchModuleCommand({
    commandBus,
    db,
    session,
    sync,
    commandType: 'ctox.business_os.why',
    moduleId: normalizedModuleId,
    recordId: normalizedModuleId,
    payload: {
      module_id: normalizedModuleId,
      include_actions: true,
      include_data_areas: true,
    },
    source: 'business-os-settings',
    timeoutMs: 15000,
    requireResult: true,
  });
  const payload = command.result || command;
  if (command.status === 'failed' || payload?.ok === false) {
    throw new Error(payload?.error || 'Zugriff konnte nicht erklärt werden.');
  }
  if (payload?.kind !== 'business_os_why_diagnostics') {
    throw new Error('Zugriff konnte noch nicht vollständig erklärt werden.');
  }
  return payload;
}

async function exportSupportDiagnosticsArtifact(moduleId, { commandBus, db, session, sync } = {}) {
  const normalizedModuleId = String(moduleId || '').trim();
  if (!normalizedModuleId) throw new Error('App fehlt für das Support-Paket.');
  const command = await dispatchModuleCommand({
    commandBus,
    db,
    session,
    sync,
    commandType: 'ctox.business_os.support.export_diagnostics',
    moduleId: normalizedModuleId,
    recordId: `support:${normalizedModuleId}`,
    payload: {
      module_id: normalizedModuleId,
      include_why: true,
      limit: 50,
    },
    source: 'business-os-settings',
    timeoutMs: 20000,
    requireResult: true,
  });
  const payload = command.result || command;
  if (command.status === 'failed' || payload?.ok === false) {
    throw new Error(payload?.error || 'Support-Paket konnte nicht erstellt werden.');
  }
  if (
    payload?.kind !== 'business_os_support_diagnostics_artifact'
    || payload?.artifact_schema !== 'ctox.business_os.support_diagnostics.v1'
  ) {
    throw new Error('Support-Paket hat noch kein geprüftes Format.');
  }
  return payload;
}

function createSupportDiagnosticsDownload(artifact, moduleId) {
  if (!globalThis.Blob || !globalThis.URL?.createObjectURL) {
    return { url: '', filename: supportDiagnosticsFilename(moduleId) };
  }
  const blob = new Blob([JSON.stringify(artifact, null, 2)], { type: 'application/json' });
  return {
    url: globalThis.URL.createObjectURL(blob),
    filename: supportDiagnosticsFilename(moduleId, artifact?.generated_at_ms),
  };
}

function supportDiagnosticsFilename(moduleId, generatedAtMs = 0) {
  const stamp = Number(generatedAtMs || 0) > 0
    ? new Date(Number(generatedAtMs)).toISOString().replace(/[:.]/g, '-')
    : 'latest';
  return `ctox-business-os-support-${safeDownloadPart(moduleId || 'workspace')}-${stamp}.json`;
}

function safeDownloadPart(value) {
  const normalized = String(value || '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, '-')
    .replace(/^-+|-+$/g, '');
  return normalized || 'workspace';
}

function supportArtifactSchemaLabel(schema) {
  if (schema === 'ctox.business_os.support_diagnostics.v1') return 'CTOX Support-Diagnose';
  return 'Geprüftes Support-Format';
}

function supportRedactionProfileLabel(profile) {
  if (profile === 'support-safe-v1') return 'Support-sicher';
  return 'Geschützt';
}

function supportDecisionAllowed(decision) {
  return decision?.allowed === true;
}

function supportVisibilityText(lifecycle = {}, app = {}, mod = {}) {
  const version = String(lifecycle.current_semver || app.version || mod.version || '').trim();
  const visibility = String(lifecycle.visibility_state || lifecycle.audience || lifecycle.release_channel || '').toLowerCase();
  const label = {
    private: 'Nur Ersteller oder Verantwortliche',
    team: 'Team sichtbar',
    restricted: 'Ausgewählte Personen',
    public: 'Team sichtbar',
  }[visibility] || 'Sichtbarkeit geprüft';
  return version ? `${label} · v${version.replace(/^v/i, '')}` : label;
}

async function loadRuntimeSettings({ db } = {}) {
  const coll = db?.collection?.('ctox_runtime_settings');
  if (!coll) throw new Error('ctox_runtime_settings collection is required for runtime settings');
  const doc = await coll.findOne('runtime-settings').exec();
  const data = doc?.toJSON?.();
  if (!data) throw new Error('Runtime-Status noch nicht synchronisiert.');
  return data;
}

async function saveRuntimeSettings(payload, {
  commandBus,
  db,
  session,
  sync,
  waitForProjection = true,
} = {}) {
  const previousSettings = waitForProjection
    ? await loadRuntimeSettings({ db }).catch(() => null)
    : null;
  const command = await dispatchModuleCommand({
    commandBus,
    db,
    session,
    sync,
    commandType: 'ctox.runtime_settings.save',
    moduleId: 'ctox',
    recordId: 'runtime-settings',
    payload,
    source: 'business-os-settings',
  });
  if (!waitForProjection) return command.result || command;
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
  throw lastError || new Error('Runtime-Status wurde nicht synchronisiert.');
}

async function loadWorkspaceBranding({ db } = {}) {
  const coll = db?.collection?.(WORKSPACE_BRANDING_COLLECTION);
  if (!coll) throw new Error('business_workspace_branding collection is required for workspace branding');
  const doc = await coll.findOne(WORKSPACE_BRANDING_DOCUMENT_ID).exec();
  const data = doc?.toJSON?.();
  if (!data) {
    return {
      id: WORKSPACE_BRANDING_DOCUMENT_ID,
      name: 'CTOX Default',
      custom: false,
      light: {},
      dark: {},
      module_accents: {},
      updated_at_ms: 0,
    };
  }
  return data;
}

async function saveWorkspaceBranding(payload, { commandBus, db, session, sync } = {}) {
  const previousBranding = await loadWorkspaceBranding({ db }).catch(() => null);
  await dispatchModuleCommand({
    commandBus,
    db,
    session,
    sync,
    commandType: 'ctox.business_os.branding.update',
    moduleId: 'ctox',
    recordId: WORKSPACE_BRANDING_DOCUMENT_ID,
    payload,
    source: 'business-os-settings',
  });
  return waitForWorkspaceBrandingProjection(db, {
    previousUpdatedAtMs: Number(previousBranding?.updated_at_ms || 0),
    expectCustom: true,
  });
}

async function resetWorkspaceBranding({ commandBus, db, session, sync } = {}) {
  const previousBranding = await loadWorkspaceBranding({ db }).catch(() => null);
  await dispatchModuleCommand({
    commandBus,
    db,
    session,
    sync,
    commandType: 'ctox.business_os.branding.update',
    moduleId: 'ctox',
    recordId: WORKSPACE_BRANDING_DOCUMENT_ID,
    payload: { reset: true },
    source: 'business-os-settings',
  });
  return waitForWorkspaceBrandingProjection(db, {
    previousUpdatedAtMs: Number(previousBranding?.updated_at_ms || 0),
    expectCustom: false,
  });
}

async function waitForWorkspaceBrandingProjection(db, options = {}) {
  const timeoutMs = Number(options.timeoutMs || 10000);
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      const branding = await loadWorkspaceBranding({ db });
      const updatedAt = Number(branding?.updated_at_ms || 0);
      if (
        branding?.custom === options.expectCustom
        && (updatedAt > Number(options.previousUpdatedAtMs || 0) || options.expectCustom === false)
      ) {
        return branding;
      }
      lastError = new Error('Corporate Design wurde noch nicht aktualisiert.');
    } catch (error) {
      lastError = error;
    }
    await delay(300);
  }
  throw lastError || new Error('Corporate Design wurde nicht synchronisiert.');
}

async function startSubscriptionAuth({ commandBus, db, session, sync } = {}) {
  const command = await dispatchModuleCommand({
    commandBus,
    db,
    session,
    sync,
    commandType: 'ctox.subscription_auth.start',
    moduleId: 'ctox',
    recordId: 'subscription-auth',
    payload: { provider: 'openai', auth_mode: 'chatgpt_subscription', flow: 'device_code' },
    source: 'business-os-settings',
    timeoutMs: 30000,
  });
  const payload = command.result || command;
  if (payload?.user_code || payload?.auth_url || payload?.verification_url) {
    return { ...payload, source: payload.source || 'business_commands' };
  }
  throw new Error(`Command ${command.command_id || command.id || ''} lieferte keinen Geräte-Code.`);
}

function runtimeSettingsReflectPayload(settings, payload, previousUpdatedAtMs = 0) {
  if (!payload) return true;
  const runtime = settings?.runtime || {};
  const auth = settings?.auth || {};
  const provider = String(payload.provider || '').toLowerCase();
  if (!provider) return false;
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
  if (['256k', '262144', '256000', '128k', '131072', '128000'].includes(normalized)) {
    return '256k';
  }
  return '256k';
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

async function loadMcpConnectInfo() {
  const response = await fetch('/api/business-os/mcp/connect-info', {
    method: 'GET',
    headers: { Accept: 'application/json' },
    cache: 'no-store',
  });
  const text = await response.text();
  let payload = null;
  try {
    payload = text ? JSON.parse(text) : null;
  } catch {
    payload = null;
  }
  if (!response.ok) {
    throw new Error(payload?.message || payload?.error || text || `MCP Status konnte nicht geladen werden (${response.status}).`);
  }
  if (!payload?.ok) {
    throw new Error(payload?.message || payload?.error || 'MCP Status konnte nicht geladen werden.');
  }
  return payload;
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
  sync,
  commandType,
  moduleId,
  recordId,
  payload,
  source,
  timeoutMs,
  requireResult,
}) {
  await sync?.startCollection?.('business_commands');
  if (!commandBus?.dispatch || !db?.collection?.('business_commands')) {
    throw new Error('business_commands collection is required for module governance commands');
  }
  const commandId = `cmd_${newId()}`;
  const command = {
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
  };
  return commandBus.dispatch(command, {
    until: requireResult ? 'terminal' : 'accepted',
    timeoutMs,
  });
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
// Wizard actions dispatch server-authoritative CTOX channel commands. The
// browser never talks directly to provider APIs and never stores provider
// tokens in replicated Business OS collections.
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
  {
    id: 'slack',
    title: 'Slack',
    dot: '#4a154b',
    short: 'Slack Workspace per Bot Token, Channel-IDs und Socket-/Events-kompatiblem Backend.',
  },
  {
    id: 'discord',
    title: 'Discord',
    dot: '#5865f2',
    short: 'Discord Bot fuer erlaubte Server- und Channel-Kontexte.',
  },
  {
    id: 'telegram',
    title: 'Telegram',
    dot: '#229ed9',
    short: 'Telegram Bot fuer DMs, Gruppen, Supergruppen und Channels.',
  },
  {
    id: 'matrix',
    title: 'Matrix',
    dot: '#0dbd8b',
    short: 'Matrix Homeserver mit Access Token und erlaubten Rooms.',
  },
  {
    id: 'mattermost',
    title: 'Mattermost',
    dot: '#0058cc',
    short: 'Self-hosted Mattermost per Server-URL, Bot Token und Channel-IDs.',
  },
  {
    id: 'zulip',
    title: 'Zulip',
    dot: '#6492fe',
    short: 'Zulip Realm mit Bot Email, API Key, Streams und Topics.',
  },
  {
    id: 'google_chat',
    title: 'Google Chat',
    dot: '#34a853',
    short: 'Google Workspace Chat Spaces per OAuth Access Token.',
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

const BOT_CHAT_CHANNELS = {
  slack: {
    title: 'Slack einrichten',
    fields: [
      ['botToken', 'Bot Token', 'password', 'xoxb-...'],
      ['workspaceId', 'Workspace-ID', 'text', 'T012345'],
      ['botUserId', 'Bot-User-ID', 'text', 'U012345'],
      ['channelIds', 'Channel-IDs', 'text', 'C012345,C067890'],
      ['appToken', 'App Token (Socket Mode, optional)', 'password', 'xapp-...'],
      ['signingSecret', 'Signing Secret (optional)', 'password', ''],
      ['apiBaseUrl', 'API Base URL (optional)', 'url', 'https://slack.com/api'],
    ],
  },
  discord: {
    title: 'Discord einrichten',
    fields: [
      ['botToken', 'Bot Token', 'password', ''],
      ['applicationId', 'Application-ID', 'text', ''],
      ['botUserId', 'Bot-User-ID (optional)', 'text', ''],
      ['guildIds', 'Guild-IDs', 'text', '123,456'],
      ['channelIds', 'Channel-IDs', 'text', '123,456'],
      ['apiBaseUrl', 'API Base URL (optional)', 'url', 'https://discord.com/api/v10'],
    ],
  },
  telegram: {
    title: 'Telegram einrichten',
    fields: [
      ['botToken', 'Bot Token', 'password', '123456:ABC...'],
      ['botUsername', 'Bot Username', 'text', 'ctox_bot'],
      ['chatIds', 'Chat-IDs', 'text', '-100123,12345'],
      ['apiBaseUrl', 'API Base URL (optional)', 'url', 'https://api.telegram.org'],
    ],
  },
  matrix: {
    title: 'Matrix einrichten',
    fields: [
      ['homeserverUrl', 'Homeserver URL', 'url', 'https://matrix.example.org'],
      ['accessToken', 'Access Token', 'password', ''],
      ['userId', 'User-ID', 'text', '@ctox:example.org'],
      ['roomIds', 'Room-IDs', 'text', '!room:example.org'],
    ],
  },
  mattermost: {
    title: 'Mattermost einrichten',
    fields: [
      ['serverUrl', 'Server URL', 'url', 'https://mattermost.example.org'],
      ['botToken', 'Bot Token', 'password', ''],
      ['botUserId', 'Bot-User-ID', 'text', ''],
      ['teamId', 'Team-ID', 'text', ''],
      ['channelIds', 'Channel-IDs', 'text', 'abc,def'],
    ],
  },
  zulip: {
    title: 'Zulip einrichten',
    fields: [
      ['realmUrl', 'Realm URL', 'url', 'https://zulip.example.org'],
      ['botEmail', 'Bot Email', 'email', 'ctox-bot@example.org'],
      ['apiKey', 'API Key', 'password', ''],
      ['streams', 'Streams', 'text', 'general,support'],
      ['topic', 'Topic (optional)', 'text', 'CTOX'],
    ],
  },
  google_chat: {
    title: 'Google Chat einrichten',
    fields: [
      ['accessToken', 'Access Token', 'password', 'ya29...'],
      ['user', 'User/App Label', 'text', 'ctox@example.com'],
      ['appId', 'App-ID (optional)', 'text', ''],
      ['spaceNames', 'Space Names', 'text', 'spaces/AAAA...'],
      ['apiBaseUrl', 'API Base URL (optional)', 'url', 'https://chat.googleapis.com'],
    ],
  },
};

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
  const adapterStatus = channelAdapterStatus(account);
  const authState = String(adapterStatus.auth_state || '').toLowerCase();
  const syncState = String(adapterStatus.sync_state || '').toLowerCase();
  const scopeState = String(adapterStatus.scope_state || '').toLowerCase();
  const permissionState = String(adapterStatus.permission_state || '').toLowerCase();
  const probeState = String(adapterStatus.probe_state || '').toLowerCase();
  const rateLimitedUntil = Number(adapterStatus.rate_limited_until_ms || 0);
  if (authState === 'failed' || authState === 'deauthorized') {
    return `<span class="channel-row-status channel-row-status--bad">Auth fehlgeschlagen</span>`;
  }
  if (scopeState === 'missing_scope') {
    return `<span class="channel-row-status channel-row-status--bad">Scope fehlt</span>`;
  }
  if (permissionState === 'missing_permission') {
    return `<span class="channel-row-status channel-row-status--bad">Rechte fehlen</span>`;
  }
  if (probeState === 'failed') {
    return `<span class="channel-row-status channel-row-status--warn">Probe fehlgeschlagen</span>`;
  }
  if (syncState === 'failed') {
    return `<span class="channel-row-status channel-row-status--bad">Sync fehlgeschlagen</span>`;
  }
  if (rateLimitedUntil > Date.now()) {
    return `<span class="channel-row-status channel-row-status--warn">Rate Limit</span>`;
  }
  if (adapterStatus.last_success_at_ms) {
    const ageMs = Date.now() - Number(adapterStatus.last_success_at_ms);
    if (ageMs < 24 * 3600 * 1000) return `<span class="channel-row-status channel-row-status--ok">Aktiv</span>`;
  }
  const latest = Math.max(parseIso(account.last_inbound_ok_at), parseIso(account.last_outbound_ok_at));
  if (!latest) return `<span class="channel-row-status channel-row-status--warn">Noch keine Aktivität</span>`;
  const ageMs = Date.now() - latest;
  if (ageMs < 24 * 3600 * 1000) return `<span class="channel-row-status channel-row-status--ok">Aktiv</span>`;
  if (ageMs < 7 * 24 * 3600 * 1000) return `<span class="channel-row-status channel-row-status--warn">Inaktiv (>24 h)</span>`;
  return `<span class="channel-row-status channel-row-status--bad">Verbindung verloren</span>`;
}

function channelLastActivityLine(account) {
  const adapterStatus = channelAdapterStatus(account);
  const inbound = account.last_inbound_ok_at ? `Letzter Eingang: ${formatIsoShort(account.last_inbound_ok_at)}` : 'Noch kein Eingang';
  const outbound = account.last_outbound_ok_at ? `Letzter Ausgang: ${formatIsoShort(account.last_outbound_ok_at)}` : 'Noch kein Ausgang';
  const statusBits = [];
  if (adapterStatus.provider_error_kind && adapterStatus.provider_error_kind !== 'none') {
    statusBits.push(`Status: ${adapterStatus.provider_error_kind}`);
  }
  const remediation = channelAdapterRemediation(adapterStatus);
  if (remediation) statusBits.push(`Hinweis: ${remediation}`);
  if (adapterStatus.last_operation) statusBits.push(`Operation: ${adapterStatus.last_operation}`);
  if (adapterStatus.last_cursor) statusBits.push(`Cursor: ${adapterStatus.last_cursor}`);
  if (adapterStatus.realtime_transport) statusBits.push(`Realtime: ${adapterStatus.realtime_transport}`);
  if (adapterStatus.realtime_config_state && !['configured', 'fake'].includes(String(adapterStatus.realtime_config_state))) {
    statusBits.push(`Realtime-Konfig: ${adapterStatus.realtime_config_state}`);
  }
  if (adapterStatus.realtime_supervision_state && !['polling_via_service_sync', 'fake'].includes(String(adapterStatus.realtime_supervision_state))) {
    statusBits.push(`Realtime-Supervision: ${adapterStatus.realtime_supervision_state}`);
  }
  if (adapterStatus.realtime_last_cursor) statusBits.push(`Realtime-Cursor: ${adapterStatus.realtime_last_cursor}`);
  if (adapterStatus.telegram_group_privacy_state && adapterStatus.telegram_group_privacy_state !== 'all_group_messages_visible') {
    statusBits.push(`Telegram-Privacy: ${adapterStatus.telegram_group_privacy_state}`);
  }
  if (adapterStatus.slack_socket_mode_state && adapterStatus.slack_socket_mode_state !== 'ready_to_connect') {
    statusBits.push(`Slack-Socket: ${adapterStatus.slack_socket_mode_state}`);
  }
  if (adapterStatus.slack_socket_mode_supervisor_state) {
    statusBits.push(`Slack-Socket-Supervision: ${adapterStatus.slack_socket_mode_supervisor_state}`);
  }
  if (adapterStatus.realtime_backoff_reason) {
    statusBits.push(`Realtime-Backoff: ${adapterStatus.realtime_backoff_reason}`);
  }
  if (adapterStatus.matrix_e2ee_state && adapterStatus.matrix_e2ee_state !== 'plaintext_only') {
    statusBits.push(`Matrix-E2EE: ${adapterStatus.matrix_e2ee_state}`);
  }
  if (adapterStatus.matrix_sdk_state_persistence && adapterStatus.matrix_sdk_state_persistence !== 'not_required_plaintext_v1') {
    statusBits.push(`Matrix-SDK-State: ${adapterStatus.matrix_sdk_state_persistence}`);
  }
  if (adapterStatus.channel_probe_state && !['ok', 'unknown'].includes(String(adapterStatus.channel_probe_state))) {
    statusBits.push(`Channel-Probe: ${adapterStatus.channel_probe_state}`);
  }
  if (adapterStatus.guild_probe_state && !['ok', 'unknown'].includes(String(adapterStatus.guild_probe_state))) {
    statusBits.push(`Guild-Probe: ${adapterStatus.guild_probe_state}`);
  }
  if (adapterStatus.gateway_probe_state && !['ok', 'unknown'].includes(String(adapterStatus.gateway_probe_state))) {
    statusBits.push(`Gateway-Probe: ${adapterStatus.gateway_probe_state}`);
  }
  if (adapterStatus.application_probe_state && !['ok', 'unknown'].includes(String(adapterStatus.application_probe_state))) {
    statusBits.push(`Application-Probe: ${adapterStatus.application_probe_state}`);
  }
  if (adapterStatus.server_version) statusBits.push(`Server: ${adapterStatus.server_version}`);
  if (adapterStatus.server_probe_state && !['ok', 'unknown'].includes(String(adapterStatus.server_probe_state))) {
    statusBits.push(`Server-Probe: ${adapterStatus.server_probe_state}`);
  }
  if (adapterStatus.tls_state === 'plain_http') statusBits.push('TLS: plain_http');
  if (adapterStatus.rate_limited_until_ms && Number(adapterStatus.rate_limited_until_ms) > Date.now()) {
    statusBits.push(`Rate Limit bis ${formatMillisShort(Number(adapterStatus.rate_limited_until_ms))}`);
  }
  if (adapterStatus.last_error) statusBits.push(`Fehler: ${adapterStatus.last_error}`);
  const suffix = statusBits.length ? ` · ${escapeHtml(statusBits.join(' · '))}` : '';
  return `<small class="channel-row-meta">${escapeHtml(inbound)} · ${escapeHtml(outbound)}${suffix}</small>`;
}

function channelAdapterStatus(account) {
  const profile = account?.profile_json || {};
  return profile.adapterStatus || profile.adapter_status || {};
}

function channelAdapterRemediation(adapterStatus) {
  const kind = String(adapterStatus?.provider_error_kind || '').toLowerCase();
  if (!kind || kind === 'none') return '';
  if (kind === 'deauthorized') return 'Account neu verbinden oder Bot-Token im Secret Store rotieren.';
  if (kind === 'missing_scope') return 'OAuth-Scopes und Admin-Freigaben beim Anbieter pruefen.';
  if (kind === 'missing_intent') return 'Discord MESSAGE_CONTENT Intent aktivieren oder auf DMs/Mentions begrenzen.';
  if (kind === 'missing_permission') return 'Bot-Mitgliedschaft, Channel-Allowlist und Anbieterrechte pruefen.';
  if (kind === 'rate_limited') return 'Retry-After abwarten; der naechste Sync versucht es erneut.';
  return adapterStatus?.provider_remediation || '';
}

function formatMillisShort(value) {
  if (!Number.isFinite(value) || value <= 0) return '';
  return formatIsoShort(new Date(value).toISOString());
}

function channelsWizardPanel(state) {
  if (state.wizard === 'whatsapp') return whatsappWizard(state);
  if (state.wizard === 'jami') return jamiWizard(state);
  if (state.wizard === 'email') return emailWizard(state);
  if (state.wizard === 'teams') return teamsWizard(state);
  if (BOT_CHAT_CHANNELS[state.wizard]) return botChatWizard(state, BOT_CHAT_CHANNELS[state.wizard]);
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

function botChatWizard(state, def) {
  const step = state.step || 'intro';
  const channelId = state.wizard;
  const errorBlock = state.error
    ? `<div class="channels-alert channels-alert--err">${escapeHtml(state.error)}</div>`
    : '';
  if (step === 'testing') {
    return wizardShell({
      title: def.title,
      step: 2, totalSteps: 3,
      body: `
        <div class="channels-testing">
          <div class="channels-testing-step is-active">CTOX testet ${escapeHtml(channelId)} ...</div>
          <small class="channels-form-note">Backend ruft den nativen Adapter ueber <code>ctox.channel.test</code>.</small>
        </div>
        ${errorBlock}
      `,
    });
  }
  if (step === 'confirm') {
    const result = state.data?.testResult?.adapter_result || state.data?.testResult || {};
    const accountKey = result.account_key || state.data?.connectedAccountKey || '';
    return wizardShell({
      title: def.title,
      step: 3, totalSteps: 3,
      body: `
        <div class="channels-confirm">
          <div class="channels-confirm-icon channels-confirm-icon--ok">✓</div>
          <h4>${escapeHtml(channelTitle(channelId))} ist verbunden</h4>
          ${accountKey ? `<div class="channels-confirm-detail"><span>Account</span><strong>${escapeHtml(accountKey)}</strong></div>` : ''}
          ${result.status ? `<div class="channels-confirm-detail"><span>Status</span><strong>${escapeHtml(result.status)}</strong></div>` : ''}
        </div>
      `,
      backLabel: '',
      nextLabel: 'Fertig',
      nextAction: 'wizard:done',
    });
  }
  return wizardShell({
    title: def.title,
    step: 1, totalSteps: 3,
    body: `
      <div class="channels-form">
        ${def.fields.map(([key, label, type, placeholder]) => `
          <label class="channels-field">
            <span>${escapeHtml(label)}</span>
            <input type="${escapeHtml(type)}" data-channel-input="${escapeHtml(`${channelId}:${key}`)}" placeholder="${escapeHtml(placeholder || '')}" value="${type === 'password' ? '' : escapeHtml(state.data?.[key] || '')}" />
          </label>
        `).join('')}
        <small class="channels-form-note">Tokens werden serverseitig im CTOX Runtime-Settings-Pfad gespeichert und nicht in Browser-Collections repliziert.</small>
        ${errorBlock}
      </div>
    `,
    nextLabel: 'Verbinden + testen',
    nextAction: `${channelId}:save_test`,
  });
}

function channelTitle(channelId) {
  return CHANNEL_DEFINITIONS.find((def) => def.id === channelId)?.title || channelId;
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
  const [channelId, fieldKey] = String(inputKey || '').split(':');
  if (BOT_CHAT_CHANNELS[channelId]?.fields?.some(([key]) => key === fieldKey)) {
    return fieldKey;
  }
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

  const botMatch = action.match(/^([a-z_]+):save_test$/);
  if (botMatch && BOT_CHAT_CHANNELS[botMatch[1]]) {
    const channelId = botMatch[1];
    channels.error = '';
    channels.step = 'testing';
    render();
    const payload = botChatConfigPayload(channelId, channels);
    const saveResult = await postChannelEndpoint('channel.settings.save', {
      channel: channelId,
      config: payload,
    });
    if (!saveResult) {
      channels.step = 'intro';
      render();
      return;
    }
    const testResult = await postChannelEndpoint('channel.test', {
      channel: channelId,
    });
    if (!testResult) {
      channels.step = 'intro';
      render();
      return;
    }
    channels.data.testResult = testResult;
    channels.step = 'confirm';
    pollAccountAppearance(channelId);
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

function botChatConfigPayload(channelId, channels) {
  const data = channels.data || {};
  const snake = (key) => key.replace(/[A-Z]/g, (ch) => `_${ch.toLowerCase()}`);
  const payload = {};
  for (const [key] of BOT_CHAT_CHANNELS[channelId]?.fields || []) {
    payload[snake(key)] = data[key] || '';
  }
  return payload;
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

function formatMsShort(value) {
  const ms = Number(value || 0);
  if (!Number.isFinite(ms) || ms <= 0) return '—';
  const date = new Date(ms);
  return `${String(date.getDate()).padStart(2, '0')}.${String(date.getMonth() + 1).padStart(2, '0')}.${date.getFullYear()} ${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
}

export const __reactSettingsTestHooks = {
  confirmedUsersAfterUpsert,
  settingsTemplate,
};
