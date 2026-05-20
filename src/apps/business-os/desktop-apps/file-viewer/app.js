export const manifest = {
  id: 'file-viewer',
  title: 'File Viewer',
  glyph: '◫',
  defaultWidth: 760,
  defaultHeight: 560,
};

export async function mount(container, ctx) {
  ensureStyles();
  const args = ctx.args || {};
  const fileId = args.fileId || '';
  const name = args.name || 'Datei';
  const mimeType = args.mimeType || 'application/octet-stream';
  let objectUrl = '';

  container.innerHTML = `
    <section class="file-viewer">
      <header class="file-viewer-toolbar">
        <div>
          <strong>${escapeHtml(name)}</strong>
          <span>${escapeHtml(mimeType)} · ${formatBytes(args.sizeBytes || 0)}</span>
        </div>
        <button type="button" data-file-download>Download</button>
      </header>
      <main class="file-viewer-content" data-file-content>
        <p>Lade Datei...</p>
      </main>
    </section>
  `;

  const content = container.querySelector('[data-file-content]');
  const download = container.querySelector('[data-file-download]');

  try {
    const blob = await readStoredOrMaterializeFile(ctx, {
      fileId,
      path: args.path || '',
      name,
      mimeType,
      contentState: args.contentState || '',
    });
    objectUrl = URL.createObjectURL(blob);
    renderBlob(content, objectUrl, blob, name, mimeType);
    download.addEventListener('click', () => {
      const anchor = document.createElement('a');
      anchor.href = objectUrl;
      anchor.download = name;
      anchor.click();
    });
    ctx.setTitle?.(name);
  } catch (error) {
    content.innerHTML = `<p class="is-error">Datei konnte nicht geöffnet werden: ${escapeHtml(error?.message || error)}</p>`;
  }

  return () => {
    if (objectUrl) URL.revokeObjectURL(objectUrl);
    container.replaceChildren();
  };
}

async function readStoredOrMaterializeFile(ctx, file) {
  if (file.contentState === 'lazy' || file.contentState === 'missing') {
    await materializeStoredFile(ctx, file);
    return waitForStoredFile(ctx.db, file.fileId, file.mimeType);
  }
  try {
    return await readStoredFile(ctx.db, file.fileId, file.mimeType);
  } catch (error) {
    if (!isMissingContentError(error) || !file.path) throw error;
  }
  await materializeStoredFile(ctx, file);
  return waitForStoredFile(ctx.db, file.fileId, file.mimeType);
}

function isMissingContentError(error) {
  return String(error?.message || error || '').includes('Dateiinhalt fehlt');
}

async function materializeStoredFile(ctx, file) {
  if (!ctx.commandBus?.dispatch) {
    throw new Error('business_commands collection is required for file materialization');
  }
  await Promise.all([
    ctx.sync?.startCollection?.('business_commands'),
    ctx.sync?.startCollection?.('desktop_files'),
    ctx.sync?.startCollection?.('desktop_file_chunks'),
  ]);
  const commandId = `cmd_${crypto.randomUUID()}`;
  await ctx.commandBus.dispatch({
    id: commandId,
    module: 'desktop',
    type: 'ctox.file.materialize',
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
  const projection = await waitForCommandProjection(ctx.db, commandId);
  if (projection.status === 'failed') {
    const message = projection?.result?.error || projection?.error || 'Datei konnte nicht materialisiert werden.';
    throw new Error(message);
  }
}

async function waitForCommandProjection(db, commandId, timeoutMs = 45000) {
  const collection = db?.collection?.('business_commands');
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const doc = await collection?.findOne(commandId).exec();
    const data = doc?.toJSON?.();
    if (data && data.status && data.status !== 'pending_sync') return data;
    await delay(300);
  }
  throw new Error(`Command ${commandId} wurde nicht synchronisiert.`);
}

async function waitForStoredFile(db, fileId, mimeType, timeoutMs = 45000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      return await readStoredFile(db, fileId, mimeType);
    } catch (error) {
      if (!isMissingContentError(error)) throw error;
      await delay(300);
    }
  }
  throw new Error('Dateiinhalt wurde nicht über RxDB repliziert.');
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

async function readStoredFile(db, fileId, mimeType = 'application/octet-stream') {
  const chunks = db?.collection?.('desktop_file_chunks');
  if (!chunks) throw new Error('Datei-Chunks sind nicht verfügbar.');
  const docs = await chunks.find().exec();
  const allChunks = docs
    .map((doc) => (typeof doc.toJSON === 'function' ? doc.toJSON() : doc))
    .filter((chunk) => chunk.file_id === fileId);
  const latestCreatedAt = Math.max(0, ...allChunks.map((chunk) => Number(chunk.created_at_ms || 0)));
  const generation = allChunks.filter((chunk) => Number(chunk.created_at_ms || 0) === latestCreatedAt);
  const total = Number(generation[0]?.total || generation.length || 0);
  if (!generation.length || total <= 0) throw new Error('Dateiinhalt fehlt.');
  const data = generation
    .filter((chunk) => Number(chunk.idx) < total)
    .sort((a, b) => a.idx - b.idx)
    .map((chunk) => chunk.data)
    .join('');
  const binary = atob(data);
  const bytes = new Uint8Array(binary.length);
  for (let idx = 0; idx < binary.length; idx += 1) bytes[idx] = binary.charCodeAt(idx);
  return new Blob([bytes], { type: mimeType || 'application/octet-stream' });
}

function ensureStyles() {
  if (document.getElementById('file-viewer-styles')) return;
  const style = document.createElement('style');
  style.id = 'file-viewer-styles';
  style.textContent = `
    .file-viewer {
      display: grid;
      grid-template-rows: 48px minmax(0, 1fr);
      height: 100%;
      min-height: 0;
      background: var(--surface);
      color: var(--text);
      font: 12px/1.35 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    .file-viewer-toolbar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      border-bottom: 1px solid var(--hairline, var(--line));
      background: color-mix(in srgb, var(--surface) 90%, var(--surface-2));
      padding: 8px 10px 8px 12px;
    }
    .file-viewer-toolbar > div {
      display: grid;
      gap: 1px;
      min-width: 0;
    }
    .file-viewer-toolbar strong,
    .file-viewer-toolbar span {
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .file-viewer-toolbar span { color: var(--muted); font-size: 11px; }
    .file-viewer-toolbar button {
      min-height: 28px;
      border: 1px solid color-mix(in srgb, var(--accent) 34%, var(--line));
      border-radius: 7px;
      background: color-mix(in srgb, var(--surface) 78%, var(--surface-2));
      color: var(--accent);
      padding: 0 10px;
      font-weight: 730;
    }
    .file-viewer-content {
      display: grid;
      min-height: 0;
      overflow: auto;
      background: color-mix(in srgb, var(--bg) 48%, var(--surface));
    }
    .file-viewer-content > p {
      margin: 16px;
      color: var(--muted);
    }
    .file-viewer-content > p.is-error { color: var(--danger); }
    .file-viewer-image,
    .file-viewer-media {
      align-self: center;
      justify-self: center;
      max-width: calc(100% - 28px);
      max-height: calc(100% - 28px);
      border-radius: 8px;
      border: 1px solid var(--hairline, var(--line));
      background: var(--surface);
    }
    .file-viewer-frame {
      width: 100%;
      height: 100%;
      border: 0;
      background: var(--surface);
    }
    .file-viewer-audio {
      align-self: center;
      justify-self: center;
      width: min(520px, calc(100% - 28px));
    }
    .file-viewer-text {
      margin: 0;
      padding: 14px;
      color: var(--text);
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      font: 12px/1.55 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    }
    .file-viewer-unsupported {
      align-self: center;
      justify-self: center;
      display: grid;
      gap: 6px;
      max-width: 420px;
      color: var(--muted);
      text-align: center;
      padding: 20px;
    }
    .file-viewer-unsupported strong { color: var(--text); }
  `;
  document.head.append(style);
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
