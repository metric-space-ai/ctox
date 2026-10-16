// App-API 4.0 (Shell v4): mount(root, api, {restored}) -> {unmount, snapshot}. Protokoll: V4-PORT.md.
import {
  FILE_CHUNK_HASH_SCHEME,
  FILE_CONTENT_HASH_SCHEME,
  sha256Hex,
} from 'ctox:shared/file-integrity.js';
// Eingebettete Texte (relativer Modul-Import, kein HTTP-Ladeweg); api.shell.t('importer.<key>') übersteuert.
import messagesDe from './locales/de.json' with { type: 'json' };
import messagesEn from './locales/en.json' with { type: 'json' };

const MAX_FILES = 400;
const MAX_FILE_BYTES = 512 * 1024;
const MAX_STANDALONE_HTML_BYTES = 8 * 1024 * 1024;
const MAX_LOCAL_SNAPSHOT_BYTES = 50 * 1024 * 1024;
const CHUNK_SIZE = 16 * 1024;
const IMPORT_CATEGORIES = new Set([
  'workspace', 'collaboration', 'productivity', 'entertainment', 'development',
  'engineering', 'knowledge', 'research', 'sales', 'recruiting', 'finance',
  'operations', 'governance', 'security', 'analytics', 'system', 'imported',
]);
const TERMINAL_STATUSES = new Set(['completed', 'failed', 'cancelled', 'canceled', 'blocked']);

const FALLBACK_LABELS = {
  title: 'App Importer',
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

export function standaloneHtmlEntryPath(paths) {
  const normalized = Array.from(paths || []).map((path) => String(path || '').replaceAll('\\', '/'));
  return normalized.length === 1 && /\.html?$/i.test(normalized[0]) ? normalized[0] : null;
}

export function importFileByteLimit(paths) {
  return standaloneHtmlEntryPath(paths) ? MAX_STANDALONE_HTML_BYTES : MAX_FILE_BYTES;
}

export function validModuleId(id) {
  return /^[a-z0-9][a-z0-9-]{1,63}$/.test(id);
}

export function normalizeImportCategory(value) {
  const slug = String(value || '').normalize('NFKD').replace(/[\u0300-\u036f]/g, '')
    .trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
  const normalized = slug === 'unterhaltung' ? 'entertainment' : slug;
  return IMPORT_CATEGORIES.has(normalized) ? normalized : 'imported';
}

export function moduleIdFromSource(value) {
  const normalized = String(value || '')
    .replace(/\.git$/i, '')
    .replace(/\.(?:html?|xhtml)$/i, '')
    .replace(/(?:\s+|[-_.]+)\(?\d+\)?$/i, '')
    .replace(/(?:[-_.\s]+v\d+(?:\.\d+){0,2})$/i, '')
    .replace(/(?:[-_.\s]+standalone)$/i, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 64)
    .replace(/-+$/g, '');
  return validModuleId(normalized) ? normalized : `imported-app-${Date.now()}`;
}

export function titleFromModuleId(moduleId) {
  return String(moduleId || '').split('-')
    .filter(Boolean)
    .map((part) => `${part.charAt(0).toUpperCase()}${part.slice(1)}`)
    .join(' ');
}

function decodeTitleEntities(value) {
  return String(value || '')
    .replace(/&amp;/gi, '&')
    .replace(/&quot;/gi, '"')
    .replace(/&#39;|&apos;/gi, "'")
    .replace(/&lt;/gi, '<')
    .replace(/&gt;/gi, '>')
    .replace(/&nbsp;/gi, ' ')
    .replace(/&#(\d+);/g, (_, code) => String.fromCodePoint(Number(code)))
    .replace(/&#x([\da-f]+);/gi, (_, code) => String.fromCodePoint(Number.parseInt(code, 16)));
}

export function appIdentityFromSource(fileName, sourceText = '') {
  const titleMatch = String(sourceText || '').match(/<title\b[^>]*>([\s\S]*?)<\/title>/i);
  let title = decodeTitleEntities(titleMatch?.[1] || '')
    .replace(/<[^>]+>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  if (title) {
    const segments = title.split(/\s+[·|–—]\s+/);
    while (segments.length > 1 && /^(?:standalone|webgl\b.*|html\b.*|browser\b.*|demo\b.*)$/i.test(segments.at(-1))) {
      segments.pop();
    }
    title = segments.join(' — ');
  }
  const fallbackId = moduleIdFromSource(fileName);
  title = title || titleFromModuleId(fallbackId);
  if (/^[A-Z0-9\s—_-]+$/.test(title)) {
    title = title.toLowerCase().replace(/(^|[\s—_-])([a-z])/g, (_, prefix, letter) => `${prefix}${letter.toUpperCase()}`);
  }
  for (const acronym of ['AI', 'CE', 'CRT', 'HTML', 'VR']) {
    title = title.replace(new RegExp(`\\b${acronym}\\b`, 'gi'), acronym);
  }
  return { moduleId: moduleIdFromSource(title), appTitle: title };
}

export function buildAppImportCommand({
  moduleId,
  appTitle,
  importSource,
  category = 'entertainment',
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
  // v4: die Befehls-ID vergibt api.command; Sync-Lease-Felder (sync_collections, sync_flush_timeout_ms,
  // allow_dependency_delivery_lag) entfallen – die Outbox liefert die Snapshot-Dokumente vor dem Befehl.
  void now;
  return {
    module: 'importer',
    command_type: 'ctox.business_os.app.create',
    record_id: moduleId,
    dependencies,
    payload: {
      title: `Import ${title}`,
      instruction: `Port the complete supplied application into a functional Shell-V2 Business OS app named ${title}. Preserve its visual identity, interactions, content density, animation, audio and game state. Use the generated starter only as contract scaffolding; remove all placeholder UI and records. Compare the mounted result against the source at matching viewports before claiming parity.`,
      module_id: moduleId,
      app_id: moduleId,
      app_title: title,
      description: `Shell-V2 port of ${title}`,
      category: normalizeImportCategory(category),
      desired_version: '1.0.0',
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
  let snapshotBytes = 0;
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
      if (file.size > MAX_STANDALONE_HTML_BYTES) {
        throw Object.assign(new Error('file_too_large'), { path: relativePath, size: file.size });
      }
      snapshotBytes += file.size;
      if (snapshotBytes > MAX_LOCAL_SNAPSHOT_BYTES) throw new Error('snapshot_too_large');
      files.push({ relativePath, file, bytes: new Uint8Array(await file.arrayBuffer()) });
    }
  }
  await walk(dirHandle, '');
  if (!files.length) throw new Error('no_importable_files');
  const standaloneHtml = standaloneHtmlEntryPath(files.map((source) => source.relativePath));
  if (!standaloneHtml) {
    const oversized = files.find((source) => source.file.size > MAX_FILE_BYTES);
    if (oversized) throw Object.assign(new Error('file_too_large'), {
      path: oversized.relativePath, size: oversized.file.size,
    });
  }
  return files;
}

async function readSelectedFiles(fileList) {
  const files = [];
  let snapshotBytes = 0;
  const selected = Array.from(fileList || []);
  const fileLimit = importFileByteLimit(selected.map((file) => file?.name));
  for (const file of selected) {
    const relativePath = String(file?.webkitRelativePath || file?.name || '').replaceAll('\\', '/');
    if (!relativePath || shouldSkipPath(relativePath) || !isImportableFile(relativePath)) continue;
    if (files.length >= MAX_FILES) {
      throw Object.assign(new Error('too_many_files'), { count: files.length + 1 });
    }
    if (relativePath.length > 240 || relativePath.split('/').some((part) => !part || part === '..')) {
      throw new Error(`invalid_path:${relativePath}`);
    }
    if (file.size > fileLimit) {
      throw Object.assign(new Error('file_too_large'), { path: relativePath, size: file.size });
    }
    snapshotBytes += file.size;
    if (snapshotBytes > MAX_LOCAL_SNAPSHOT_BYTES) throw new Error('snapshot_too_large');
    files.push({ relativePath, file, bytes: new Uint8Array(await file.arrayBuffer()) });
  }
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

// Snapshot-Dokumente in die fremden Desktop-Collections (desktop_files, desktop_file_chunks; Eigentümer desktop).
// v4: lokal committen (h.local), Zustellung über die Outbox in Schreibreihenfolge – kein Sync-Lease, kein Retry.
async function putDocuments(api, collection, rows) {
  for (const row of rows) await api.put(collection, row).local;
}

async function stageDesktopSnapshot(api, { folderName, sourceFiles }, onProgress = () => {}) {
  const now = Date.now();
  const snapshotId = `app-import-${crypto.randomUUID()}`;
  const folderId = `fs_${snapshotId}`;
  const folderPath = `/Business OS/App Imports/${snapshotId}`;
  const folderDocument = {
    id: folderId,
    parent_id: 'fs_root',
    path: folderPath,
    name: folderName || snapshotId,
    kind: 'folder',
    mime_type: 'inode/directory',
    extension: '',
    size_bytes: 0,
    source: 'app-importer',
    sort_index: now,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  };
  await putDocuments(api, 'desktop_files', [folderDocument]);

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
    await putDocuments(api, 'desktop_file_chunks', rows);
    const fileDocument = {
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
    };
    await putDocuments(api, 'desktop_files', [fileDocument]);
    manifest.push({
      file_id: fileId,
      generation_id: generationId,
      relative_path: source.relativePath,
      sha256: contentHash,
      size_bytes: source.bytes.byteLength,
    });
  }
  const standaloneHtml = standaloneHtmlEntryPath(sourceFiles.map((source) => source.relativePath));
  return {
    kind: 'desktop-folder',
    snapshot_id: snapshotId,
    folder_name: folderName || 'Imported app',
    ...(standaloneHtml ? {
      profile: 'standalone-html',
      entry_path: standaloneHtml,
    } : {}),
    files: manifest,
  };
}

async function stageDesktopFolderSnapshot(api, dirHandle, onProgress = () => {}) {
  return stageDesktopSnapshot(api, {
    folderName: dirHandle.name,
    sourceFiles: await readDirectoryFiles(dirHandle),
  }, onProgress);
}

export async function stageDesktopFileSnapshot(api, fileList, onProgress = () => {}) {
  const sourceFiles = await readSelectedFiles(fileList);
  const firstName = sourceFiles[0].relativePath.split('/').pop() || 'Imported app';
  return stageDesktopSnapshot(api, {
    folderName: firstName.replace(/\.[^.]+$/, '') || firstName,
    sourceFiles,
  }, onProgress);
}

// v4-Befehlsstatus (queued|accepted|running|done|failed) auf die Projektion abbilden: done = completed.
function commandStatus(command) {
  const status = String(command?.status || command?.task_status || command?.terminal_status || '').toLowerCase();
  return status === 'done' ? 'completed' : status;
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
  if (!response.ok) throw new Error(`importer markup missing: ${response.status}`);
  return response.text();
}

function messagesFor(lang) {
  return { ...messagesDe, ...(lang === 'en' ? messagesEn : {}) };
}

export async function mount(host, api, { restored } = {}) {
  if (!host.dataset.app) host.dataset.app = api.app?.id || 'importer';
  host.innerHTML = await loadModuleMarkup();
  const root = host.querySelector('[data-importer-root]') || host;
  const disposers = [];
  let lang = String(api.shell.locale.get() || '').toLowerCase().startsWith('en') ? 'en' : 'de';
  let messages = messagesFor(lang);
  const t = (key, fallback, vars = {}) => {
    let text = messages?.[key] ?? FALLBACK_LABELS[key] ?? fallback ?? key;
    const shellText = api.shell.t?.(`importer.${key}`);
    if (typeof shellText === 'string' && shellText && shellText !== `importer.${key}`) text = shellText;
    for (const [name, value] of Object.entries(vars)) text = text.replaceAll(`{${name}}`, String(value));
    return text;
  };
  const q = (selector) => root.querySelector(selector);
  const refs = {
    title: q('[data-imp-title]'),
    sourceSection: q('[data-imp-source-section]'), progressSection: q('[data-imp-progress-section]'),
    doneSection: q('[data-imp-done-section]'), githubForm: q('[data-imp-github-form]'),
    githubUrl: q('[data-imp-github-url]'), githubBtn: q('[data-imp-github-btn]'),
    category: q('[data-imp-category]'),
    pickFiles: q('[data-imp-pick-files]'), pickFilesInput: q('[data-imp-pick-files-input]'),
    pickFolder: q('[data-imp-pick-folder]'),
    commandId: q('[data-imp-command-id]'), moduleId: q('[data-imp-module-id]'),
    progressTitle: q('[data-imp-progress-title]'), progressNote: q('[data-imp-progress-note]'),
    phases: [...root.querySelectorAll('[data-imp-phase]')], doneHeading: q('[data-imp-done-heading]'),
    done: q('[data-imp-done]'), open: q('[data-imp-open]'), reset: q('[data-imp-reset]'),
  };
  const applyLabels = () => {
    refs.title.textContent = t('title', FALLBACK_LABELS.title);
    root.querySelectorAll('[data-imp-copy]').forEach((node) => {
      if (!node.dataset.impCopyDefault) node.dataset.impCopyDefault = node.textContent;
      node.textContent = t(node.dataset.impCopy, node.dataset.impCopyDefault);
    });
    // Textdiät Regel 1: Sachhinweise nur als Tooltip am Knopf.
    root.querySelectorAll('[data-imp-tip]').forEach((node) => {
      if (!node.dataset.impTipDefault) node.dataset.impTipDefault = node.title;
      node.title = t(node.dataset.impTip, node.dataset.impTipDefault);
    });
    refs.githubBtn.textContent = t('startGithub', 'Start porting');
  };
  applyLabels();
  api.shell.setTitle(t('title', FALLBACK_LABELS.title));
  const state = { commandId: '', moduleId: '', stopCommand: null, live: false };
  let disposed = false;

  const setStage = (stage) => {
    root.dataset.impStage = stage;
    refs.sourceSection.hidden = stage !== 'source';
    refs.progressSection.hidden = stage !== 'progress';
    refs.doneSection.hidden = stage !== 'done';
    refs.open.hidden = stage !== 'done' || !state.live;
  };
  // Hinweise und Fehler nur in die Zustandszone (kein Hinweiskasten im Inhalt).
  const report = (code, text, level = 'error') => api.status.report({ level, code, text });
  const clear = (...codes) => codes.forEach((code) => api.status.clear(code));
  const setPhase = (activeIndex, failed = false) => {
    refs.phases.forEach((phase, index) => {
      phase.classList.toggle('is-complete', index < activeIndex && !failed);
      phase.classList.toggle('is-active', index === activeIndex && !failed);
      phase.classList.toggle('is-failed', index === activeIndex && failed);
    });
  };
  const actorContext = () => (api.actor?.id ? {
    id: api.actor.id,
    display_name: api.actor.display_name || api.actor.id,
    role: api.actor.role || 'user',
    is_admin: api.actor.role === 'admin',
  } : null);

  function renderProjection(raw) {
    if (disposed || !raw) return;
    const command = raw;
    const status = commandStatus(command);
    const progress = command?.execution_progress || (command?.progress && typeof command.progress === 'object' ? command.progress : {});
    const phaseText = String(progress.current_step || progress.phase || command?.execution_phase || '');
    refs.progressNote.textContent = phaseText;
    let index = ['', 'pending', 'queued', 'accepted'].includes(status) ? 1 : 2;
    if (/validat|smoke|review/.test(phaseText.toLowerCase())) index = 3;
    if (status === 'completed') index = 4;
    setPhase(index);
    if (!TERMINAL_STATUSES.has(status)) return;
    state.stopCommand?.();
    state.stopCommand = null;
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
      refs.done.innerHTML = `<p>${escapeHtml(commandErrorText(command))}</p>`;
      return;
    }
    refs.doneHeading.textContent = t('doneHeading', 'The ported app is live.');
    refs.done.innerHTML = [
      `<p>${escapeHtml(state.moduleId)}</p>`,
      result.source_revision ? `<p>${escapeHtml(result.source_revision)}</p>` : '',
    ].join('');
    refs.open.hidden = false;
  }

  // Projektion des Befehls live über das business_commands-Dokument (status, progress, result, error).
  function followCommand(commandId) {
    state.stopCommand?.();
    const off = api.get('business_commands', commandId).subscribe((doc) => { if (doc) renderProjection(doc); });
    state.stopCommand = off;
    disposers.push(() => off());
  }

  // Befehl absetzen, nie warten (R5); der Stand erscheint live in der Fortschrittsansicht.
  function dispatchImport(moduleId, appTitle, importSource) {
    state.moduleId = moduleId;
    state.live = false;
    const command = buildAppImportCommand({
      moduleId,
      appTitle,
      importSource,
      category: refs.category?.value || 'entertainment',
      actor: actorContext(),
    });
    const handle = api.command(command.command_type, {
      ...command.payload,
      module: command.module,
      record_id: command.record_id,
      dependencies: command.dependencies,
      client_context: command.client_context,
    }, { record_id: command.record_id });
    handle.local.catch((error) => report('importer.command.local', t('dispatchFailed', 'The import job could not be started: {error}', { error: error?.message || error })));
    state.commandId = handle.id;
    if (refs.commandId) refs.commandId.textContent = handle.id;
    refs.moduleId.textContent = moduleId;
    refs.progressTitle.textContent = t('progressHeading', 'CTOX is porting the app.');
    refs.progressNote.textContent = '';
    setStage('progress');
    setPhase(1);
    followCommand(handle.id);
  }

  refs.githubForm.addEventListener('submit', (event) => {
    event.preventDefault();
    const source = parseGitHubUrl(refs.githubUrl.value);
    if (!source) {
      report('importer.github.invalid', t('invalidGithub', 'Enter a public https://github.com/owner/repo URL.'), 'warn');
      return;
    }
    clear('importer.github.invalid', 'importer.dispatch');
    try {
      const moduleId = moduleIdFromSource(source.repo);
      dispatchImport(moduleId, titleFromModuleId(moduleId), {
        kind: 'github', repository_url: source.repositoryUrl, ref: source.ref || 'HEAD',
      });
    } catch (error) {
      report('importer.dispatch', t('dispatchFailed', 'The import job could not be started: {error}', { error: error?.message || error }));
      setStage('source');
    }
  });

  const snapshotProgress = (current, total, path) => {
    api.status.report({ level: 'info', code: 'importer.snapshot.progress', text: `${current}/${total} · ${path}` });
  };

  refs.pickFiles.addEventListener('click', () => refs.pickFilesInput.click());
  refs.pickFilesInput.addEventListener('change', async () => {
    const selectedFiles = refs.pickFilesInput.files;
    if (!selectedFiles?.length) return;
    refs.pickFiles.disabled = true;
    clear('importer.file');
    try {
      const importSource = await stageDesktopFileSnapshot(api, selectedFiles, snapshotProgress);
      const firstSource = await selectedFiles[0].text();
      const identity = appIdentityFromSource(selectedFiles[0].name, firstSource);
      if (disposed) return;
      dispatchImport(identity.moduleId, identity.appTitle, importSource);
    } catch (error) {
      report('importer.file', t('fileFailed', 'The file import could not be started: {error}', { error: error?.message || error }));
    } finally {
      clear('importer.snapshot.progress');
      refs.pickFiles.disabled = false;
      refs.pickFilesInput.value = '';
    }
  });

  refs.pickFolder.addEventListener('click', async () => {
    if (typeof globalThis.showDirectoryPicker !== 'function') {
      report('importer.folder.no_picker', t('noPicker', 'Folder dialog unsupported.'), 'warn');
      return;
    }
    refs.pickFolder.disabled = true;
    clear('importer.folder');
    try {
      const handle = await globalThis.showDirectoryPicker({ mode: 'read' });
      const importSource = await stageDesktopFolderSnapshot(api, handle, snapshotProgress);
      const moduleId = moduleIdFromSource(handle.name);
      if (disposed) return;
      dispatchImport(moduleId, titleFromModuleId(moduleId), importSource);
    } catch (error) {
      if (error?.name !== 'AbortError') {
        report('importer.folder', t('folderFailed', 'The folder import could not be started: {error}', { error: error?.message || error }));
      }
    } finally {
      clear('importer.snapshot.progress');
      refs.pickFolder.disabled = false;
    }
  });

  // Die neue App erscheint über die Aktivierung live im Katalog; öffnen = navigieren, kein Neuladen.
  refs.open.addEventListener('click', () => {
    if (!state.live || !state.moduleId) return;
    api.shell.navigate(state.moduleId);
  });
  refs.reset.addEventListener('click', () => {
    state.stopCommand?.();
    state.stopCommand = null;
    state.commandId = '';
    state.moduleId = '';
    state.live = false;
    setStage('source');
  });

  disposers.push(api.shell.locale.subscribe((locale) => {
    const next = String(locale || '').toLowerCase().startsWith('en') ? 'en' : 'de';
    if (next === lang) return;
    lang = next;
    messages = messagesFor(lang);
    applyLabels();
  }));

  setStage('source');
  // Remount: offene Eingaben und laufenden Auftrag weiterverfolgen.
  if (restored && typeof restored === 'object') {
    if (typeof restored.githubUrl === 'string') refs.githubUrl.value = restored.githubUrl;
    if (typeof restored.category === 'string' && refs.category) refs.category.value = restored.category;
    if (typeof restored.commandId === 'string' && restored.commandId) {
      state.commandId = restored.commandId;
      state.moduleId = String(restored.moduleId || '');
      if (refs.commandId) refs.commandId.textContent = state.commandId;
      refs.moduleId.textContent = state.moduleId;
      refs.progressTitle.textContent = t('progressHeading', 'CTOX is porting the app.');
      setStage('progress');
      setPhase(1);
      followCommand(state.commandId);
    }
  }
  return {
    unmount() {
      disposed = true;
      for (const dispose of disposers.splice(0)) {
        try { dispose(); } catch (error) { console.warn('[importer] unsubscribe failed', error); }
      }
      state.stopCommand = null;
      host.replaceChildren();
    },
    snapshot() {
      return {
        v: 1,
        githubUrl: refs.githubUrl.value,
        category: refs.category?.value || '',
        commandId: state.commandId,
        moduleId: state.moduleId,
      };
    },
  };
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (character) => (
    { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[character]
  ));
}
