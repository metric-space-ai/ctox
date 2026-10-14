import {
  FILE_CHUNK_ERROR_CODES,
  readStoredFileFromDemandChunks,
} from '../../shared/file-integrity.js?v=20260816-browser-sync-guards-v141';

const TEXT_PREVIEW_RANGE_BYTES = 256 * 1024;


export async function mount(ctx) {
  const container = ctx.host;
  ensureModuleStyles();
  const args = ctx.args || {};
  const fileId = args.fileId || '';
  const name = args.name || 'Datei';
  const mimeType = args.mimeType || 'application/octet-stream';
  const sizeBytes = Number(args.sizeBytes || args.size_bytes || 0);
  const previewRange = textPreviewRangeFor(mimeType, sizeBytes);
  let objectUrl = '';

  container.innerHTML = await loadModuleMarkup();
  const title = container.querySelector('[data-file-title]');
  const metadata = container.querySelector('[data-file-metadata]');
  if (title) title.textContent = name;
  if (metadata) metadata.textContent = `${mimeType} · ${formatBytes(sizeBytes)}`;

  const content = container.querySelector('[data-file-content]');
  const download = container.querySelector('[data-file-download]');

  try {
    const blob = await readStoredOrMaterializeFile(ctx, {
      fileId,
      path: args.path || '',
      name,
      mimeType,
      contentState: args.contentState || '',
      contentHash: args.contentHash || args.content_hash || '',
      contentHashScheme: args.contentHashScheme || args.content_hash_scheme || '',
      contentGenerationId: args.contentGenerationId || args.content_generation_id || '',
    }, { range: previewRange });
    objectUrl = URL.createObjectURL(blob);
    renderBlob(content, objectUrl, blob, name, mimeType);
    download.addEventListener('click', () => downloadStoredFile(ctx, {
      fileId,
      name,
      mimeType,
      contentGenerationId: args.contentGenerationId || args.content_generation_id || '',
      contentHash: args.contentHash || args.content_hash || '',
      contentHashScheme: args.contentHashScheme || args.content_hash_scheme || '',
    }, objectUrl, previewRange));
    ctx.setTitle?.(name);
  } catch (error) {
    console.warn('[file-viewer] open failed', error);
    ctx.reportFileIntegrityError?.(error, fileIntegrityDetails(args, fileId, mimeType));
    content.innerHTML = '<p class="is-error">Datei konnte nicht geöffnet werden.</p>';
  }

  return () => {
    if (objectUrl) URL.revokeObjectURL(objectUrl);
    container.replaceChildren();
  };
}

async function readStoredOrMaterializeFile(ctx, file, options = {}) {
  if (file.contentState === 'lazy' || file.contentState === 'missing') {
    const commandId = await materializeStoredFile(ctx, file);
    return waitForStoredFile(ctx, file.fileId, file.mimeType, commandId, options);
  }
  try {
    return await readStoredFile(ctx, file.fileId, file.mimeType, {
      contentGenerationId: file.contentGenerationId,
      contentHash: file.contentHash,
      contentHashScheme: file.contentHashScheme,
      range: options.range || null,
    });
  } catch (error) {
    if (!isMissingContentError(error) || !file.path) throw error;
  }
  const commandId = await materializeStoredFile(ctx, file);
  return waitForStoredFile(ctx, file.fileId, file.mimeType, commandId, options);
}

function isMissingContentError(error) {
  return String(error?.message || error || '').includes('Dateiinhalt fehlt');
}

function isRetryableMaterializationReadError(error) {
  return isMissingContentError(error) || error?.code === FILE_CHUNK_ERROR_CODES.GENERATION_MISMATCH;
}

function fileIntegrityDetails(args, fileId, mimeType) {
  return {
    fileId,
    mimeType,
    contentState: args.contentState || '',
    contentGenerationId: args.contentGenerationId || args.content_generation_id || '',
    contentHashScheme: args.contentHashScheme || args.content_hash_scheme || '',
  };
}

async function materializeStoredFile(ctx, file) {
  if (!ctx.commandBus?.dispatch) {
    throw new Error('business_commands collection is required for file materialization');
  }
  const [commandBridge] = await Promise.all([
    ctx.sync?.startCollection?.('business_commands'),
    ctx.sync?.startCollection?.('desktop_files'),
  ]);
  await waitForReplicationBridge(commandBridge, 'business_commands');
  const commandId = `cmd_${crypto.randomUUID()}`;
  await ctx.commandBus.dispatch({
    id: commandId,
    module: 'desktop',
    command_type: 'ctox.file.materialize',
    record_id: file.fileId,
    inbound_channel: 'desktop',
    payload: {
      file_id: file.fileId,
      path: file.path,
      title: `Load ${file.name}`,
    },
    client_context: {
      source: 'business-os-file-viewer',
      actor: actorContext(ctx.session),
    },
  });
  await waitForReplicationBridge(commandBridge, 'business_commands');
  await waitForCommandReplication(ctx, commandId);
  await waitForMaterializedFileProjection(ctx, file.fileId);
  return commandId;
}

async function waitForReplicationBridge(bridge, collection, timeoutMs = 20000) {
  const state = bridge?.state;
  const wait = typeof state?.awaitInSync === 'function'
    ? state.awaitInSync.bind(state)
    : typeof state?.awaitInitialReplication === 'function'
      ? state.awaitInitialReplication.bind(state)
      : null;
  if (!wait) return;
  let timeoutId;
  try {
    await Promise.race([
      wait(),
      new Promise((_, reject) => {
        timeoutId = setTimeout(() => reject(new Error(`${collection} data did not become ready in time`)), timeoutMs);
      }),
    ]);
  } catch (error) {
    console.warn('[file-viewer] replication wait failed', collection, error);
    throw new Error('Datei konnte nicht geöffnet werden.');
  } finally {
    clearTimeout(timeoutId);
  }
}

async function readCommandProjection(db, commandId) {
  const collection = db?.collection?.('business_commands');
  const doc = await collection?.findOne(commandId).exec();
  return doc?.toJSON?.() || null;
}

async function waitForCommandReplication(ctx, commandId, timeoutMs = 30000) {
  const deadline = Date.now() + timeoutMs;
  let nextRestartAt = Date.now() + 5000;
  let lastCommand = null;
  while (Date.now() < deadline) {
    lastCommand = await readCommandProjection(ctx.db, commandId);
    const status = lastCommand?.status || '';
    if (status && status !== 'pending_sync') return lastCommand;
    if (Date.now() >= nextRestartAt) {
      nextRestartAt = Date.now() + 5000;
      const bridge = shouldRestartSyncCollection(ctx, 'business_commands')
        && typeof ctx.sync?.restartCollection === 'function'
        ? await ctx.sync.restartCollection('business_commands')
        : await ctx.sync?.startCollection?.('business_commands');
      await touchCommandDocument(ctx.db, commandId);
      await waitForReplicationBridge(bridge, 'business_commands');
    }
    await delay(300);
  }
  throw new Error(`Der Materialize-Befehl wurde nicht an CTOX übertragen: ${lastCommand?.status || 'missing'}`);
}

function shouldRestartSyncCollection(ctx, collection) {
  const diagnostics = ctx?.sync?.diagnostics || ctx?.syncDiagnostics || {};
  const entry = diagnostics?.collections?.[collection] || {};
  const status = entry.connectionStatus || entry.status || '';
  return ['failed', 'error', 'stopped', 'reconnecting'].includes(status);
}

async function touchCommandDocument(db, commandId) {
  const doc = await db?.collection?.('business_commands')?.findOne(commandId).exec();
  if (!doc?.incrementalPatch) return;
  await doc.incrementalPatch({ updated_at_ms: Date.now() });
}

async function waitForMaterializedFileProjection(ctx, fileId, timeoutMs = 90000) {
  const deadline = Date.now() + timeoutMs;
  let restarted = false;
  let lastFileState = '';
  let lastGenerationId = '';
  let lastSyncNudgeAt = 0;
  while (Date.now() < deadline) {
    const file = await ctx.db?.collection?.('desktop_files')?.findOne(fileId).exec();
    const fileData = file?.toJSON?.();
    lastFileState = fileData?.content_state || '';
    lastGenerationId = fileData?.content_generation_id || '';
    if (lastFileState === 'available' && lastGenerationId) return;
    if (Date.now() - lastSyncNudgeAt > 1000) {
      lastSyncNudgeAt = Date.now();
      await nudgeFileProjectionSync(ctx);
    }
    if (!restarted && Date.now() > deadline - timeoutMs + 5000) {
      restarted = true;
      const bridge = typeof ctx.sync?.restartCollection === 'function'
        ? await ctx.sync.restartCollection('desktop_files')
        : await ctx.sync?.startCollection?.('desktop_files');
      await waitForReplicationBridge(bridge, 'desktop_files');
    }
    await delay(300);
  }
  throw new Error(`Die materialisierte Datei wurde nicht im Browser bereitgestellt: state=${lastFileState || 'missing'}, generation=${lastGenerationId || 'missing'}`);
}

async function nudgeFileProjectionSync(ctx) {
  if (!ctx.sync?.startCollection) return;
  const bridge = await ctx.sync.startCollection('desktop_files').catch(() => null);
  if (bridge) await waitForReplicationBridge(bridge, 'desktop_files').catch(() => {});
}

async function waitForStoredFile(ctx, fileId, mimeType, commandId = '', options = {}, timeoutMs = 90000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const file = await ctx.db?.collection?.('desktop_files')?.findOne(fileId).exec();
      const fileData = file?.toJSON?.();
      const fileAvailable = fileData?.content_state === 'available';
      return await readStoredFile(ctx, fileId, mimeType, {
        contentGenerationId: fileAvailable ? fileData?.content_generation_id || '' : '',
        contentHash: fileAvailable ? fileData?.content_hash || '' : '',
        contentHashScheme: fileAvailable ? fileData?.content_hash_scheme || '' : '',
        range: options.range || null,
      });
    } catch (error) {
      if (!isRetryableMaterializationReadError(error)) throw error;
      if (commandId) {
        const command = await readCommandProjection(ctx.db, commandId);
        if (command?.status === 'failed') {
          console.warn('[file-viewer] materialize failed', command?.result?.error || command?.error || command?.status);
          throw new Error('Datei konnte nicht geöffnet werden.');
        }
      }
      await delay(300);
    }
  }
  throw new Error('Dateiinhalt konnte nicht geladen werden.');
}

async function renderBlob(container, objectUrl, blob, name, mimeType) {
  if (mimeType.startsWith('image/')) {
    container.innerHTML = `<img class="file-viewer-image" src="${objectUrl}" alt="">`;
    return;
  }
  if (mimeType === 'application/pdf') {
    container.innerHTML = `<iframe class="file-viewer-frame" src="${objectUrl}" title="${escapeHtml(name)}"></iframe>`;
    return;
  }
  if (mimeType.startsWith('video/')) {
    container.innerHTML = `<video class="file-viewer-media" src="${objectUrl}" controls></video>`;
    return;
  }
  if (mimeType.startsWith('audio/')) {
    container.innerHTML = `<audio class="file-viewer-audio" src="${objectUrl}" controls></audio>`;
    return;
  }
  if (mimeType.startsWith('text/') || ['application/json', 'application/xml'].includes(mimeType)) {
    const text = await blob.text();
    container.innerHTML = '<pre class="file-viewer-text" data-file-text></pre>';
    container.querySelector('[data-file-text]').textContent = text;
    return;
  }
  container.innerHTML = `
    <div class="file-viewer-unsupported">
      <strong>Keine Vorschau verfügbar</strong>
      <span>Diese Datei kann gespeichert oder in einer passenden App geöffnet werden, sobald ein Viewer registriert ist.</span>
    </div>
  `;
}

async function readStoredFile(ctx, fileId, mimeType = 'application/octet-stream', options = {}) {
  const loader = await fileDemandLoaderFor(ctx).catch(() => null);
  if (loader?.fetchFile) {
    const range = normalizeRange(options.range);
    const chunks = await loader.fetchFile(fileId, range ? { range } : undefined);
    return readStoredFileFromDemandChunks(chunks, mimeType, integrityOptionsForRead(options, range));
  }
  throw new Error('Dateiinhalt ist noch nicht über den Sync-Demand-Pfad verfügbar.');
}

function textPreviewRangeFor(mimeType = '', sizeBytes = 0) {
  const size = Number(sizeBytes || 0);
  if (!Number.isFinite(size) || size <= TEXT_PREVIEW_RANGE_BYTES) return null;
  if (!isTextPreviewMimeType(mimeType)) return null;
  return { offset: 0, length: TEXT_PREVIEW_RANGE_BYTES };
}

function isTextPreviewMimeType(mimeType = '') {
  const normalized = String(mimeType || '').toLowerCase();
  return normalized.startsWith('text/')
    || ['application/json', 'application/xml'].includes(normalized);
}

function normalizeRange(range) {
  if (!range || typeof range !== 'object') return null;
  const offset = Math.max(0, Number(range.offset || 0));
  const length = Math.max(0, Number(range.length || range.limit || 0));
  if (!Number.isFinite(offset) || !Number.isFinite(length) || length <= 0) return null;
  return { offset, length };
}

function integrityOptionsForRead(options = {}, range = null) {
  if (range) return {};
  return options;
}

async function downloadStoredFile(ctx, file, currentObjectUrl, previewRange = null) {
  if (!previewRange && currentObjectUrl) {
    triggerDownload(currentObjectUrl, file.name);
    return;
  }
  const blob = await readStoredFile(ctx, file.fileId, file.mimeType, {
    contentGenerationId: file.contentGenerationId,
    contentHash: file.contentHash,
    contentHashScheme: file.contentHashScheme,
  });
  const url = URL.createObjectURL(blob);
  try {
    triggerDownload(url, file.name);
  } finally {
    setTimeout(() => URL.revokeObjectURL(url), 0);
  }
}

function triggerDownload(url, name) {
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = name;
  anchor.click();
}

async function fileDemandLoaderFor(ctx) {
  if (!ctx?.sync?.startCollection) return null;
  const bridge = await ctx.sync.startCollection('desktop_files');
  await waitForReplicationBridge(bridge, 'desktop_files');
  return bridge?.state?.demandFileLoader || null;
}

async function loadModuleMarkup() {
  const markupVersion = String(import.meta.url).split('?v=')[1] || '';
  const markupHref = new URL('./index.html', import.meta.url).pathname + (markupVersion ? `?v=${markupVersion}` : '');
  const html = await fetch(markupHref).then((response) => response.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function ensureModuleStyles() {
  const styleId = 'file-viewer-module-styles';
  if (document.getElementById(styleId)) return;
  const cssVersion = String(import.meta.url).split('?v=')[1] || '';
  const cssHref = new URL('./index.css', import.meta.url).pathname + (cssVersion ? `?v=${cssVersion}` : '');
  const link = document.createElement('link');
  link.id = styleId;
  link.rel = 'stylesheet';
  link.href = cssHref;
  document.head.append(link);
}

function formatBytes(value) {
  const bytes = Number(value || 0);
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function actorContext(session) {
  const user = session?.user || {};
  return {
    id: user.id || 'business-os',
    display_name: user.display_name || user.name || user.email || 'Business OS',
    role: user.role || 'user',
  };
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
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

export const __fileViewerTestHooks = Object.freeze({
  readStoredFile,
  textPreviewRangeFor,
});
