const MODULE_ID = 'minecraft-mods';

const defaults = {
  projects: [
    {
      id: 'project-controller-support',
      title: 'Steam Controller Support',
      loader: 'fabric',
      minecraft_version: '1.21.5',
      project_path: '~/Minecraft/mods/controller-support',
      package_id: 'steam-controller-support',
      status: 'ready',
      notes: 'Fabric controller input compatibility layer for Steam Input.'
    },
    {
      id: 'project-shader-pack',
      title: 'Photon Shader Profile',
      loader: 'vanilla',
      minecraft_version: '1.21.5',
      project_path: '~/.minecraft/shaderpacks',
      package_id: 'photon-default',
      status: 'installed',
      notes: 'Default shader profile managed alongside mod installs.'
    }
  ],
  artifacts: [
    {
      id: 'artifact-controller-support',
      project_id: 'project-controller-support',
      filename: 'steam-controller-support-0.1.0.jar',
      jar_path: '~/Minecraft/mods/controller-support/build/libs/steam-controller-support-0.1.0.jar',
      loader: 'fabric',
      mod_id: 'steam-controller-support',
      version: '0.1.0',
      sha256: '',
      build_status: 'queued'
    }
  ],
  installations: [
    {
      id: 'install-default',
      artifact_id: 'artifact-controller-support',
      target_name: 'A6000 default instance',
      minecraft_dir: '~/.minecraft',
      profile: 'default',
      status: 'planned',
      manifest_path: '~/.minecraft/.ctox/minecraft-mods/manifest.json'
    }
  ],
  mergeSets: [
    {
      id: 'merge-controller-pack',
      title: 'Controller + shader baseline',
      source_dirs: ['~/.minecraft/mods', '~/Downloads/minecraft-mods'],
      target_dir: '~/Minecraft/staging/controller-pack/mods',
      status: 'planned',
      conflicts_json: []
    }
  ]
};

const state = {
  ctx: null,
  projects: [],
  artifacts: [],
  installations: [],
  mergeSets: [],
  commands: [],
  selectedProjectId: '',
  query: '',
  loader: 'all',
  unsubscribe: [],
  els: {}
};

export async function mount(ctx) {
  state.ctx = ctx;
  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  ensureStylesheet();
  bindElements(ctx.host);
  wireEvents();
  await Promise.all([
    ctx.sync?.startCollection?.('business_commands'),
    ctx.sync?.startCollection?.('minecraft_mod_projects'),
    ctx.sync?.startCollection?.('minecraft_mod_artifacts'),
    ctx.sync?.startCollection?.('minecraft_mod_installations'),
    ctx.sync?.startCollection?.('minecraft_mod_merge_sets')
  ]);
  await seedIfEmpty();
  await loadAll();
  wireRealtime();
  render();
  return () => {
    for (const subscription of state.unsubscribe) {
      try { subscription?.unsubscribe?.(); } catch {}
    }
    state.unsubscribe = [];
  };
}

function ensureStylesheet() {
  const href = new URL('./index.css', import.meta.url).pathname;
  if (document.head.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

function bindElements(root) {
  state.els.root = root.querySelector('[data-minecraft-mods-root]');
  state.els.search = root.querySelector('[data-search]');
  state.els.projectList = root.querySelector('[data-project-list]');
  state.els.artifactRows = root.querySelector('[data-artifact-rows]');
  state.els.mergeList = root.querySelector('[data-merge-list]');
  state.els.installList = root.querySelector('[data-install-list]');
  state.els.commandList = root.querySelector('[data-command-list]');
  state.els.status = root.querySelector('[data-status]');
  state.els.activeTitle = root.querySelector('[data-active-title]');
  state.els.activeLoader = root.querySelector('[data-active-loader]');
  state.els.detailTitle = root.querySelector('[data-detail-title]');
  state.els.form = root.querySelector('[data-detail-form]');
}

function wireEvents() {
  state.els.search.addEventListener('input', () => {
    state.query = state.els.search.value.trim().toLowerCase();
    renderProjects();
  });
  state.els.root.addEventListener('click', (event) => {
    const loader = event.target.closest('[data-loader]');
    if (loader) {
      state.loader = loader.dataset.loader || 'all';
      render();
      return;
    }
    const project = event.target.closest('[data-project-id]');
    if (project) {
      state.selectedProjectId = project.dataset.projectId;
      render();
      return;
    }
    const action = event.target.closest('[data-action]')?.dataset.action;
    if (action) handleAction(action);
  });
  state.els.form.addEventListener('submit', (event) => {
    event.preventDefault();
    saveSelectedProject();
  });
}

async function seedIfEmpty() {
  const projects = state.ctx.db?.collection?.('minecraft_mod_projects');
  if (!projects) return;
  const count = await projects.count().exec().catch(() => 0);
  if (count > 0) return;
  const now = Date.now();
  await Promise.all([
    ...defaults.projects.map((item) => upsert('minecraft_mod_projects', { ...item, updated_at_ms: now })),
    ...defaults.artifacts.map((item) => upsert('minecraft_mod_artifacts', { ...item, updated_at_ms: now })),
    ...defaults.installations.map((item) => upsert('minecraft_mod_installations', { ...item, updated_at_ms: now })),
    ...defaults.mergeSets.map((item) => upsert('minecraft_mod_merge_sets', { ...item, updated_at_ms: now }))
  ]);
}

async function loadAll() {
  const [projects, artifacts, installations, mergeSets, commands] = await Promise.all([
    findAll('minecraft_mod_projects'),
    findAll('minecraft_mod_artifacts'),
    findAll('minecraft_mod_installations'),
    findAll('minecraft_mod_merge_sets'),
    findAll('business_commands')
  ]);
  state.projects = projects.sort(sortUpdated);
  state.artifacts = artifacts.sort(sortUpdated);
  state.installations = installations.sort(sortUpdated);
  state.mergeSets = mergeSets.sort(sortUpdated);
  state.commands = commands.filter((command) => command.module === MODULE_ID || command.inbound_channel === 'business_os.minecraft_mods').sort(sortUpdated).slice(0, 8);
  if (!state.selectedProjectId || !state.projects.some((project) => project.id === state.selectedProjectId)) {
    state.selectedProjectId = state.projects[0]?.id || '';
  }
}

function wireRealtime() {
  for (const name of ['minecraft_mod_projects', 'minecraft_mod_artifacts', 'minecraft_mod_installations', 'minecraft_mod_merge_sets', 'business_commands']) {
    const sub = state.ctx.db?.collection?.(name)?.find?.()?.$?.subscribe?.(() => {
      loadAll().then(render).catch((error) => setStatus(error.message || String(error), true));
    });
    if (sub) state.unsubscribe.push(sub);
  }
}

async function findAll(name) {
  const docs = await state.ctx.db?.collection?.(name)?.find?.()?.exec?.().catch(() => []);
  return (docs || []).map((doc) => typeof doc.toJSON === 'function' ? doc.toJSON() : doc);
}

function selectedProject() {
  return state.projects.find((project) => project.id === state.selectedProjectId) || null;
}

function filteredProjects() {
  return state.projects.filter((project) => {
    const loaderOk = state.loader === 'all' || project.loader === state.loader;
    const haystack = `${project.title} ${project.package_id} ${project.loader} ${project.project_path}`.toLowerCase();
    return loaderOk && (!state.query || haystack.includes(state.query));
  });
}

function render() {
  updateLoaderButtons();
  renderProjects();
  renderCenter();
  renderDetail();
  renderCommands();
}

function updateLoaderButtons() {
  for (const button of state.els.root.querySelectorAll('[data-loader]')) {
    button.setAttribute('aria-pressed', button.dataset.loader === state.loader ? 'true' : 'false');
  }
}

function renderProjects() {
  const projects = filteredProjects();
  state.els.projectList.innerHTML = projects.length
    ? projects.map((project) => `
      <button type="button" class="mc-project" data-project-id="${escapeHtml(project.id)}" aria-selected="${project.id === state.selectedProjectId ? 'true' : 'false'}">
        <strong>${escapeHtml(project.title)}</strong>
        <span class="mc-meta">${escapeHtml(project.loader)} · ${escapeHtml(project.minecraft_version || 'version open')} · ${escapeHtml(project.status || 'draft')}</span>
      </button>
    `).join('')
    : '<div class="mc-row-card"><strong>No projects</strong><span class="mc-meta">Create a mod project or change the filter.</span></div>';
}

function renderCenter() {
  const project = selectedProject();
  state.els.activeTitle.textContent = project?.title || 'Minecraft Mods';
  state.els.activeLoader.textContent = project?.loader || 'All';
  const artifacts = state.artifacts.filter((artifact) => !project || artifact.project_id === project.id);
  state.els.artifactRows.innerHTML = artifacts.length
    ? artifacts.map((artifact) => `
      <tr>
        <td>${escapeHtml(artifact.filename)}</td>
        <td>${escapeHtml(artifact.mod_id)}</td>
        <td>${escapeHtml(artifact.loader)}</td>
        <td>${escapeHtml(artifact.version || 'unknown')}</td>
        <td>${escapeHtml(artifact.build_status || 'tracked')}</td>
      </tr>
    `).join('')
    : '<tr><td colspan="5">No artifacts tracked for this project.</td></tr>';
  state.els.mergeList.innerHTML = state.mergeSets.map((merge) => `
    <div class="mc-row-card">
      <strong>${escapeHtml(merge.title)}</strong>
      <span class="mc-meta">${escapeHtml((merge.source_dirs || []).join(' + '))} -> ${escapeHtml(merge.target_dir || '')}</span>
      <span class="mc-meta">${escapeHtml(merge.status || 'planned')} · ${Number(merge.conflicts_json?.length || 0)} conflicts</span>
    </div>
  `).join('');
  state.els.installList.innerHTML = state.installations.map((install) => `
    <div class="mc-row-card">
      <strong>${escapeHtml(install.target_name)}</strong>
      <span class="mc-meta">${escapeHtml(install.minecraft_dir)} · ${escapeHtml(install.profile || 'default')}</span>
      <span class="mc-meta">${escapeHtml(install.status || 'planned')}</span>
    </div>
  `).join('');
}

function renderDetail() {
  const project = selectedProject();
  state.els.detailTitle.textContent = project?.title || 'New mod';
  for (const field of state.els.form.querySelectorAll('[data-field]')) {
    field.value = project?.[field.dataset.field] || '';
  }
}

function renderCommands() {
  state.els.commandList.innerHTML = state.commands.length
    ? state.commands.map((command) => `
      <div class="mc-command">
        <strong>${escapeHtml(command.command_type || command.type || command.id)}</strong>
        <span class="mc-meta">${escapeHtml(command.status || 'queued')} · ${escapeHtml(command.record_id || '')}</span>
      </div>
    `).join('')
    : '<div class="mc-command"><strong>No commands yet</strong><span class="mc-meta">Build, install, merge, and inspect actions create CTOX commands.</span></div>';
}

async function handleAction(action) {
  if (action === 'new-project') return createProject();
  if (action === 'delete-project') return deleteSelectedProject();
  if (action === 'new-merge') return createMergeSet();
  if (action === 'new-install-target') return createInstallTarget();
  if (['build', 'install', 'merge', 'inspect'].includes(action)) return queueModCommand(action);
}

async function createProject() {
  const id = `project_${Date.now()}`;
  await upsert('minecraft_mod_projects', {
    id,
    title: 'New Minecraft Mod',
    loader: 'fabric',
    minecraft_version: '1.21.5',
    project_path: '',
    package_id: `ctox-mod-${Date.now()}`,
    status: 'draft',
    notes: '',
    updated_at_ms: Date.now()
  });
  state.selectedProjectId = id;
  await loadAll();
  render();
}

async function saveSelectedProject() {
  const project = selectedProject();
  if (!project) return;
  const next = { ...project, updated_at_ms: Date.now() };
  for (const field of state.els.form.querySelectorAll('[data-field]')) {
    next[field.dataset.field] = field.value.trim();
  }
  await upsert('minecraft_mod_projects', next);
  setStatus('Project saved');
  await loadAll();
  render();
}

async function deleteSelectedProject() {
  const project = selectedProject();
  if (!project) return;
  const doc = await state.ctx.db?.collection?.('minecraft_mod_projects')?.findOne(project.id).exec();
  await doc?.remove?.();
  state.selectedProjectId = '';
  setStatus('Project deleted');
  await loadAll();
  render();
}

async function createMergeSet() {
  await upsert('minecraft_mod_merge_sets', {
    id: `merge_${Date.now()}`,
    title: 'New merge plan',
    source_dirs: ['~/.minecraft/mods'],
    target_dir: '~/Minecraft/staging/mods',
    status: 'draft',
    conflicts_json: [],
    updated_at_ms: Date.now()
  });
  await loadAll();
  render();
}

async function createInstallTarget() {
  await upsert('minecraft_mod_installations', {
    id: `install_${Date.now()}`,
    artifact_id: state.artifacts.find((artifact) => artifact.project_id === state.selectedProjectId)?.id || '',
    target_name: 'New Minecraft instance',
    minecraft_dir: '~/.minecraft',
    profile: 'default',
    status: 'draft',
    manifest_path: '~/.minecraft/.ctox/minecraft-mods/manifest.json',
    updated_at_ms: Date.now()
  });
  await loadAll();
  render();
}

async function queueModCommand(action) {
  const project = selectedProject();
  const commandId = `cmd_${Date.now()}_${Math.random().toString(16).slice(2)}`;
  const commandType = `minecraft.mods.${action}`;
  const payload = {
    skill: 'minecraft-mod-development',
    action,
    project,
    artifacts: state.artifacts.filter((artifact) => artifact.project_id === project?.id),
    install_targets: state.installations,
    merge_sets: state.mergeSets,
    helper: 'src/skills/system/product_engineering/minecraft-mod-development/scripts/minecraft_mod_manager.py'
  };
  if (state.ctx.commandBus?.dispatch) {
    await state.ctx.commandBus.dispatch({
      id: commandId,
      module: MODULE_ID,
      type: commandType,
      record_id: project?.id || MODULE_ID,
      inbound_channel: 'business_os.minecraft_mods',
      payload,
      client_context: { source: 'minecraft-mods-module', module_id: MODULE_ID }
    });
  } else {
    await upsert('business_commands', {
      id: commandId,
      command_id: commandId,
      module: MODULE_ID,
      command_type: commandType,
      record_id: project?.id || MODULE_ID,
      status: 'pending_sync',
      inbound_channel: 'business_os.minecraft_mods',
      payload,
      client_context: { source: 'minecraft-mods-module', module_id: MODULE_ID },
      updated_at_ms: Date.now()
    });
  }
  setStatus(`${commandType} queued`);
  await loadAll();
  render();
}

async function upsert(collectionName, doc) {
  const collection = state.ctx.db?.collection?.(collectionName);
  if (!collection) return;
  if (typeof collection.upsert === 'function') {
    await collection.upsert(doc);
    return;
  }
  const existing = await collection.findOne(doc.id).exec();
  if (existing?.incrementalPatch) await existing.incrementalPatch(doc);
  else if (existing?.atomicPatch) await existing.atomicPatch(doc);
  else if (!existing) await collection.insert(doc);
}

function setStatus(message, isError = false) {
  state.els.status.textContent = message;
  state.els.status.style.color = isError ? '#b91c1c' : '';
}

function sortUpdated(a, b) {
  return Number(b.updated_at_ms || 0) - Number(a.updated_at_ms || 0);
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;'
  })[char]);
}
