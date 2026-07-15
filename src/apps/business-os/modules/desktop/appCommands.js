export function nextQuickAppIdentity(modules, locale = 'de', now = Date.now()) {
  const baseTitle = locale === 'en' ? 'New App' : 'Neue App';
  const usedTitles = new Set((Array.isArray(modules) ? modules : [])
    .map((module) => String(module?.title || '').trim().toLocaleLowerCase(locale))
    .filter(Boolean));
  let title = baseTitle;
  let suffix = 2;
  while (usedTitles.has(title.toLocaleLowerCase(locale))) {
    title = `${baseTitle} ${suffix}`;
    suffix += 1;
  }
  const timestamp = Number.isFinite(Number(now)) ? Number(now) : Date.now();
  return {
    id: `app-${timestamp.toString(36)}`,
    title,
  };
}

export function buildQuickAppCreateCommand({ moduleId, title, actor, now = Date.now() }) {
  const id = clean(moduleId);
  const appTitle = clean(title);
  if (!id) throw new Error('Module id is required.');
  if (!appTitle) throw new Error('App title is required.');
  const timestamp = Number.isFinite(Number(now)) ? Number(now) : Date.now();
  return {
    command_id: `app-create-${id}-${timestamp}`,
    module: 'creator',
    command_type: 'ctox.business_os.app.create',
    record_id: id,
    payload: {
      title: `Create ${appTitle}`,
      instruction: `Create a ready-to-use Business OS starter app named "${appTitle}" in the canonical app shell. Use the record-workbench starter with left navigation, a primary work area, and an inspector. Keep the initial content minimal so the app can be changed from its desktop context menu.`,
      module_id: id,
      app_id: id,
      app_title: appTitle,
      description: 'Business OS starter app with navigation and a primary workspace.',
      category: 'Custom',
      archetype: 'record-workbench',
      layout_hint: 'Canonical Business OS window shell with left navigation and primary work area.',
      presentation: {
        default_mode: 'window',
        supported_modes: ['window', 'maximized', 'focus'],
        initial_size: { width: 960, height: 680 },
        minimum_size: { width: 640, height: 480 },
        multi_instance: false,
        auto_restore: false,
      },
      collections_hint: [],
      desired_version: '0.1.0',
      install_target: 'runtime-installed-module',
      target: 'app',
      mode: 'app',
      required_skills: ['business-os-app-module-development'],
    },
    client_context: {
      source: 'desktop-quick-create',
      target: 'app',
      mode: 'app',
      module_id: id,
      app_id: id,
      archetype: 'record-workbench',
      install_target: 'runtime-installed-module',
      actor: actor || null,
    },
  };
}

export function moduleRenamePayload(module, title) {
  const id = clean(module?.id);
  const nextTitle = clean(title);
  if (!id) throw new Error('Module id is required.');
  if (!nextTitle) throw new Error('App title is required.');
  return {
    id,
    title: nextTitle,
    description: clean(module?.description || module?.store?.summary),
    version: clean(module?.version),
    entry: clean(module?.entry),
    collections: Array.isArray(module?.collections) ? module.collections.map(clean).filter(Boolean) : [],
    layout: module?.layout && typeof module.layout === 'object' ? cloneJson(module.layout) : null,
  };
}

export function isRuntimeInstalledApp(module) {
  const entry = clean(module?.entry);
  const source = clean(module?.source || module?.install_scope || module?.store?.distribution).toLowerCase();
  return Boolean(module?.id) && (
    entry.startsWith('installed-modules/')
    || source === 'installed'
    || source.includes('runtime-installed')
  );
}

function clean(value) {
  return String(value || '').trim();
}

function cloneJson(value) {
  try {
    return structuredClone(value);
  } catch {
    return JSON.parse(JSON.stringify(value));
  }
}
