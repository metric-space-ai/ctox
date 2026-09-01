import { loadModuleMessages } from '../../shared/i18n.js';
import {
  FILE_CHUNK_HASH_SCHEME,
  FILE_CONTENT_HASH_SCHEME,
  sha256Hex,
} from '../../shared/file-integrity.js?v=20260816-browser-sync-guards-v141';

const MAX_FILES = 400;
const MAX_FILE_BYTES = 512 * 1024;
const CHUNK_SIZE = 16 * 1024;
const TERMINAL_STATUSES = new Set(['completed', 'failed', 'cancelled', 'canceled', 'blocked']);
const RECOVERABLE_DISPATCH_PATTERNS = [
  /webrtc native peer did not open for business_commands/i,
  /reconnect repair is scheduled/i,
];

const FALLBACK_LABELS = {
  title: 'App Importer',
  subtitle: 'Source in. Durable CTOX app job out.',
};

export function parseGitHubUrl(raw) {
  let url;
  try {
    url = new URL(String(raw || '').trim());
  } catch {
    return null;
  }
  if (url.protocol !== 'https:' || url.hostname !== 'github.com' || url.port
    || url.username || url.password || url.search || url.hash) return null;
  const parts = url.pathname.split('/').filter(Boolean);
  if (parts.length !== 2 && !(parts.length >= 4 && parts[2] === 'tree')) return null;
  const owner = parts[0];
  const repo = String(parts[1] || '').replace(/\.git$/, '');
  if (!/^[A-Za-z0-9_.-]+$/.test(owner) || !/^[A-Za-z0-9_.-]+$/.test(repo)) return null;
  return {
    owner,
    repo,
    ref: parts[2] === 'tree' ? parts.slice(3).join('/') : null,
    repositoryUrl: `https://github.com/${owner}/${repo}`,
  };
}

export function shouldSkipPath(path) {
  const normalized = String(path || '').replaceAll('\\', '/');
  return /(^|\/)(node_modules|\.git|dist|build|out|coverage|\.next|\.cache)(\/|$)/.test(normalized)
    || /(^|\/)\./.test(normalized) && !/^\.?[^/]*rc$/.test(normalized.split('/').pop() || '')
    || normalized.endsWith('.lock')
    || normalized.endsWith('package-lock.json')
    || normalized.endsWith('.map');
}

// QML, Python, shell scripts and files without an extension are source
// evidence for the porting agent, not executables the importer must understand.
export function isTextFile(path) {
  const name = String(path || '').split('/').pop() || '';
  return !/\.(avif|gif|ico|jpe?g|png|webp|woff2?|zip|gz|mp3|mp4|ogg|wav)$/i.test(name);
}

export function isImportableFile(path) {
  return Boolean(String(path || '').trim()) && !shouldSkipPath(path);
}

export function validModuleId(id) {
  return /^[a-z0-9][a-z0-9-]{1,63}$/.test(id);
}

export function isRecoverableDispatchError(error) {
  const message = String(error?.message || error || '');
  return RECOVERABLE_DISPATCH_PATTERNS.some((pattern) => pattern.test(message));
}

export function moduleIdFromSource(value) {
  const normalized = String(value || '')
    .replace(/\.git$/i, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 64)
    .replace(/-+$/g, '');
  return validModuleId(normalized) ? normalized : `imported-app-${Date.now()}`;
}

function titleFromModuleId(moduleId) {
  return String(moduleId || '').split('-')
    .filter(Boolean)
    .map((part) => `${part.charAt(0).toUpperCase()}${part.slice(1)}`)
    .join(' ');
}

export function buildAppImportCommand({
  moduleId,
  appTitle,
  importSource,
  actor = null,
  now = Date.now(),
}) {
  if (!validModuleId(moduleId)) throw new Error('Invalid module id');
  if (!importSource || !['github', 'desktop-folder'].includes(importSource.kind)) {
    throw new Error('Invalid import source');
  }
  const title = String(appTitle || titleFromModuleId(moduleId)).trim() || moduleId;
  const files = Array.isArray(importSource.files) ? importSource.files : [];
  const dependencies = files.map((file) => ({
    collection: 'desktop_files',
    record_id: file.file_id,
    generation_id: file.generation_id,
    content_hash: file.sha256,
    required: true,
  }));
  return {
    command_id: `app-import-${moduleId}-${now}`,
    module: 'importer',
    command_type: 'ctox.business_os.app.create',
    record_id: moduleId,
    dependencies,
    sync_collections: files.length ? ['desktop_files', 'desktop_file_chunks'] : [],
    payload: {
      title: `Import ${title}`,
      instruction: `Port the complete supplied application into a functional Shell-V2 Business OS app named ${title}. Preserve its user workflows rather than translating framework syntax.`,
      module_id: moduleId,
      app_id: moduleId,
      app_title: title,
      description: `Shell-V2 port of ${title}`,
      category: 'development',
      desired_version: '0.1.0',
      install_target: 'runtime-installed-module',
      target: 'app',
      mode: 'app',
      required_skills: ['business-os-app-module-development'],
      import_source: importSource,
    },
    client_context: {
      source: 'business-os-app-importer',
      target: 'app',
      mode: 'app',
      module_id: moduleId,
      app_id: moduleId,
      install_target: 'runtime-installed-module',
      actor,
    },
  };
}

async function readDirectoryFiles(dirHandle) {
  const files = [];
  async function walk(handle, prefix) {
    for await (const [name, entry] of handle.entries()) {
      const relativePath = prefix ? `${prefix}/${name}` : name;
      if (shouldSkipPath(relativePath)) continue;
      if (entry.kind === 'directory') {
        await walk(entry, relativePath);
        continue;
      }
      if (!isImportableFile(relativePath)) continue;
      if (files.length >= MAX_FILES) {
        throw Object.assign(new Error('too_many_files'), { count: files.length + 1 });
      }
      if (relativePath.length > 240 || relativePath.split('/').some((part) => !part || part === '..')) {
        throw new Error(`invalid_path:${relativePath}`);
      }
      const file = await entry.getFile();
      if (file.size > MAX_FILE_BYTES) {
        throw Object.assign(new Error('file_too_large'), { path: relativePath, size: file.size });
      }
      files.push({ relativePath, file, bytes: new Uint8Array(await file.arrayBuffer()) });
    }
  }
  await walk(dirHandle, '');
  if (!files.length) throw new Error('no_importable_files');
  return files;
}

function uint8ToBase64(bytes) {
  let binary = '';
  for (let index = 0; index < bytes.length; index += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(index, index + 0x8000));
  }
  return btoa(binary);
}

async function writeChunkDocuments(collection, rows) {
  if (typeof collection.bulkUpsert === 'function') return collection.bulkUpsert(rows);
  if (typeof collection.bulkInsert === 'function') return collection.bulkInsert(rows);
  for (const row of rows) await collection.upsert(row);
  return undefined;
}

async function stageDesktopFolderSnapshot(ctx, dirHandle, onProgress = () => {}) {
  const desktopFiles = ctx?.db?.collection?.('desktop_files');
  const desktopChunks = ctx?.db?.collection?.('desktop_file_chunks');
  if (!desktopFiles || !desktopChunks) throw new Error('desktop_file_sync_unavailable');
  const sourceFiles = await readDirectoryFiles(dirHandle);
  const now = Date.now();
  const snapshotId = `app-import-${crypto.randomUUID()}`;
  const folderId = `fs_${snapshotId}`;
  const folderPath = `/Business OS/App Imports/${snapshotId}`;
  await desktopFiles.upsert({
    id: folderId,
    parent_id: 'fs_root',
    path: folderPath,
    name: dirHandle.name || snapshotId,
    kind: 'folder',
    mime_type: 'inode/directory',
    extension: '',
    size_bytes: 0,
    source: 'app-importer',
    sort_index: now,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  });

  const manifest = [];
  for (const [index, source] of sourceFiles.entries()) {
    onProgress(index + 1, sourceFiles.length, source.relativePath);
    const contentHash = await sha256Hex(source.bytes);
    const fileId = `appimport_${crypto.randomUUID()}`;
    const generationId = `gen_${now}_${contentHash.slice(0, 12)}`;
    const base64 = uint8ToBase64(source.bytes);
    const total = Math.max(1, Math.ceil(base64.length / CHUNK_SIZE));
    const rows = await Promise.all(Array.from({ length: total }, async (_, chunkIndex) => {
      const data = base64.slice(chunkIndex * CHUNK_SIZE, (chunkIndex + 1) * CHUNK_SIZE);
      return {
        id: `${fileId}_${generationId}_${chunkIndex}`,
        file_id: fileId,
        generation_id: generationId,
        content_hash: contentHash,
        content_hash_scheme: FILE_CONTENT_HASH_SCHEME,
        idx: chunkIndex,
        total,
        encoding: 'base64',
        data,
        chunk_hash: await sha256Hex(data),
        chunk_hash_scheme: FILE_CHUNK_HASH_SCHEME,
        size_bytes: data.length,
        created_at_ms: now,
      };
    }));
    await writeChunkDocuments(desktopChunks, rows);
    await desktopFiles.upsert({
      id: fileId,
      parent_id: folderId,
      path: `${folderPath}/${source.relativePath}`,
      name: source.relativePath.split('/').pop(),
      kind: 'file',
      mime_type: source.file.type || 'application/octet-stream',
      extension: source.relativePath.includes('.') ? source.relativePath.split('.').pop().toLowerCase() : '',
      size_bytes: source.bytes.byteLength,
      source: 'app-importer',
      content_ref: fileId,
      content_state: 'available',
      content_hash: contentHash,
      content_hash_scheme: FILE_CONTENT_HASH_SCHEME,
      content_generation_id: generationId,
      content_synced_at_ms: now,
      sort_index: now + index,
      is_deleted: false,
      created_at_ms: now,
      updated_at_ms: now,
    });
    manifest.push({
      file_id: fileId,
      generation_id: generationId,
      relative_path: source.relativePath,
      sha256: contentHash,
      size_bytes: source.bytes.byteLength,
    });
  }
  return {
    kind: 'desktop-folder',
    snapshot_id: snapshotId,
    folder_name: dirHandle.name || 'Imported app',
    files: manifest,
  };
}

function commandStatus(command) {
  return String(command?.status || command?.task_status || command?.terminal_status || '').toLowerCase();
}

function commandResult(command) {
  if (command?.result && typeof command.result === 'object') return command.result;
  if (command?.result_json && typeof command.result_json === 'object') return command.result_json;
  try { return JSON.parse(command?.result_json || '{}'); } catch { return {}; }
}

function commandErrorText(command) {
  return String(command?.error_message || command?.last_error || command?.error
    || commandResult(command)?.error || 'The porting job did not complete.');
}

async function loadModuleMarkup() {
  const response = await fetch(new URL('./index.html', import.meta.url));
  if (!response.ok) throw new Error(`importer markup unavailable: ${response.status}`);
  return response.text();
}

function ensureStyles() {
  if (document.getElementById('importer-module-styles')) return;
  const styleLink = document.createElement('link');
  styleLink.rel = 'stylesheet';
  styleLink.href = new URL('./index.css', import.meta.url).href;
  styleLink.id = 'importer-module-styles';
  document.head.appendChild(styleLink);
}

export async function mount(ctx) {
  ensureStyles();
  const host = ctx?.host || document.body;
  ctx?.left?.replaceChildren?.();
  ctx?.right?.replaceChildren?.();
  host.innerHTML = await loadModuleMarkup();
  const root = host.querySelector('[data-importer-root]') || host;
  const messages = await loadModuleMessages(import.meta.url, ctx?.locale, FALLBACK_LABELS);
  const t = (key, fallback, vars = {}) => {
    let text = messages?.[key] ?? fallback ?? key;
    for (const [name, value] of Object.entries(vars)) text = text.replaceAll(`{${name}}`, String(value));
    return text;
  };
  const q = (selector) => root.querySelector(selector);
  const refs = {
    title: q('[data-imp-title]'), subtitle: q('[data-imp-subtitle]'), notice: q('[data-imp-notice]'),
    sourceSection: q('[data-imp-source-section]'), progressSection: q('[data-imp-progress-section]'),
    doneSection: q('[data-imp-done-section]'), githubForm: q('[data-imp-github-form]'),
    githubUrl: q('[data-imp-github-url]'), githubBtn: q('[data-imp-github-btn]'),
    pickFolder: q('[data-imp-pick-folder]'), sourceHint: q('[data-imp-source-hint]'),
    commandId: q('[data-imp-command-id]'), moduleId: q('[data-imp-module-id]'),
    progressTitle: q('[data-imp-progress-title]'), progressNote: q('[data-imp-progress-note]'),
    phases: [...root.querySelectorAll('[data-imp-phase]')], doneHeading: q('[data-imp-done-heading]'),
    done: q('[data-imp-done]'), open: q('[data-imp-open]'), reset: q('[data-imp-reset]'),
  };
  refs.title.textContent = t('title', FALLBACK_LABELS.title);
  refs.subtitle.textContent = t('subtitle', FALLBACK_LABELS.subtitle);
  root.querySelectorAll('[data-imp-copy]').forEach((node) => {
    node.textContent = t(node.dataset.impCopy, node.textContent);
  });
  refs.githubBtn.textContent = t('startGithub', 'Start porting');
  const state = { commandId: '', moduleId: '', subscription: null, live: false };
  let disposed = false;

  const setStage = (stage) => {
    root.dataset.impStage = stage;
    refs.sourceSection.hidden = stage !== 'source';
    refs.progressSection.hidden = stage !== 'progress';
    refs.doneSection.hidden = stage !== 'done';
    refs.open.hidden = stage !== 'done' || !state.live;
  };
  const notify = (text, isError = false) => {
    refs.notice.hidden = !text;
    refs.notice.textContent = text || '';
    refs.notice.classList.toggle('is-danger', isError);
  };
  const setPhase = (activeIndex, failed = false) => {
    refs.phases.forEach((phase, index) => {
      phase.classList.toggle('is-complete', index < activeIndex && !failed);
      phase.classList.toggle('is-active', index === activeIndex && !failed);
      phase.classList.toggle('is-failed', index === activeIndex && failed);
    });
  };
  const actorContext = () => {
    const session = typeof ctx?.session === 'function' ? ctx.session() : ctx?.session;
    const user = session?.user || {};
    return user.id ? {
      id: user.id,
      display_name: user.display_name || user.name || user.id,
      role: user.role || (user.is_admin ? 'admin' : 'user'),
      is_admin: Boolean(user.is_admin),
    } : null;
  };

  function renderProjection(raw) {
    if (disposed || !raw) return;
    const command = raw?.toJSON?.() || raw;
    const status = commandStatus(command);
    const progress = command?.execution_progress || command?.progress || {};
    const phaseText = String(progress.current_step || progress.phase || command?.execution_phase || status || 'queued');
    refs.progressNote.textContent = phaseText;
    let index = ['pending', 'queued', 'accepted'].includes(status) ? 1 : 2;
    if (/validat|smoke|review/.test(phaseText.toLowerCase())) index = 3;
    if (status === 'completed') index = 4;
    setPhase(index);
    if (!TERMINAL_STATUSES.has(status)) return;
    state.subscription?.unsubscribe?.();
    const result = commandResult(command);
    const passed = status === 'completed'
      && result.live === true
      && String(result.validation_status || '').toLowerCase() === 'passed'
      && String(result.smoke_status || '').toLowerCase() === 'passed';
    state.live = passed;
    setStage('done');
    if (!passed) {
      setPhase(Math.min(index, 3), true);
      refs.doneHeading.textContent = t('failedHeading', 'Porting did not go live.');
      refs.done.innerHTML = `<p>${escapeHtml(commandErrorText(command))}</p><p><code>${escapeHtml(state.commandId)}</code></p>`;
      return;
    }
    refs.doneHeading.textContent = t('doneHeading', 'The ported app is live.');
    refs.done.innerHTML = [
      `<p>${escapeHtml(state.moduleId)}</p>`,
      `<p>${escapeHtml(result.source_revision || 'source revision recorded')}</p>`,
      `<p>${escapeHtml(`Asset ${result.asset_revision || '—'} · Catalog ${result.catalog_revision || '—'}`)}</p>`,
      `<p><code>${escapeHtml(state.commandId)}</code></p>`,
    ].join('');
    refs.open.hidden = false;
  }

  async function dispatchImport(moduleId, appTitle, importSource) {
    if (!ctx?.commandBus?.dispatch) throw new Error('command_bus_unavailable');
    state.moduleId = moduleId;
    state.live = false;
    const command = buildAppImportCommand({ moduleId, appTitle, importSource, actor: actorContext() });
    state.commandId = command.command_id;
    refs.commandId.textContent = command.command_id;
    refs.moduleId.textContent = moduleId;
    refs.progressTitle.textContent = t('progressHeading', 'CTOX is porting the app.');
    refs.progressNote.textContent = t('queueing', 'Sending the durable app job…');
    setStage('progress');
    setPhase(0);
    state.subscription = ctx.commandBus.subscribe(command.command_id, renderProjection);
    try {
      await state.subscription.ready;
      const accepted = await ctx.commandBus.dispatch(command, { until: 'accepted', timeoutMs: 60_000 });
      renderProjection(accepted);
    } catch (error) {
      if (!isRecoverableDispatchError(error)) throw error;
      setStage('progress');
      setPhase(1);
      refs.progressNote.textContent = t('syncPending', 'Job secured locally; waiting for CTOX sync…');
      notify(t(
        'syncPendingNotice',
        'The durable job is secured locally. CTOX is reconnecting and will continue it automatically.',
      ));
    }
    const current = await ctx.commandBus.getStatus(command.command_id).catch(() => null);
    if (current) renderProjection(current);
  }

  refs.githubForm.addEventListener('submit', async (event) => {
    event.preventDefault();
    const source = parseGitHubUrl(refs.githubUrl.value);
    if (!source) {
      notify(t('invalidGithub', 'Enter a public https://github.com/owner/repo URL.'), true);
      return;
    }
    refs.githubBtn.disabled = true;
    notify('');
    try {
      const moduleId = moduleIdFromSource(source.repo);
      await dispatchImport(moduleId, titleFromModuleId(moduleId), {
        kind: 'github', repository_url: source.repositoryUrl, ref: source.ref || 'HEAD',
      });
    } catch (error) {
      notify(t('dispatchFailed', 'The import job could not be started: {error}', { error: error?.message || error }), true);
      setStage('source');
    } finally {
      refs.githubBtn.disabled = false;
    }
  });

  refs.pickFolder.addEventListener('click', async () => {
    if (typeof globalThis.showDirectoryPicker !== 'function') {
      notify(t('noPicker', 'Folder dialog unsupported.'), true);
      return;
    }
    refs.pickFolder.disabled = true;
    try {
      const handle = await globalThis.showDirectoryPicker({ mode: 'read' });
      notify(t('snapshotting', 'Securing the source snapshot…'));
      const importSource = await stageDesktopFolderSnapshot(ctx, handle, (current, total, path) => {
        refs.sourceHint.textContent = `${current}/${total} · ${path}`;
      });
      const moduleId = moduleIdFromSource(handle.name);
      notify('');
      await dispatchImport(moduleId, titleFromModuleId(moduleId), importSource);
    } catch (error) {
      if (error?.name !== 'AbortError') {
        notify(t('folderFailed', 'The folder import could not be started: {error}', { error: error?.message || error }), true);
      }
    } finally {
      refs.pickFolder.disabled = false;
    }
  });

  refs.open.addEventListener('click', () => {
    if (!state.live || !state.moduleId) return;
    globalThis.location.hash = state.moduleId;
    globalThis.location.reload();
  });
  refs.reset.addEventListener('click', () => {
    state.subscription?.unsubscribe?.();
    state.commandId = '';
    state.moduleId = '';
    state.live = false;
    notify('');
    setStage('source');
  });

  setStage('source');
  return () => {
    disposed = true;
    state.subscription?.unsubscribe?.();
  };
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (character) => (
    { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[character]
  ));
}
