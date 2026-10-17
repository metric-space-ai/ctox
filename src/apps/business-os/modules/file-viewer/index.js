// Datei-Viewer – App-API 4.0 (Shell v4, Strang UC4). Port-Protokoll: V4-PORT.md.
//
// Eingang: Route-Parameter (api.shell.onRoute / openFile der Shell) oder `restored`.
//   { ref?, file_id?|id?|fileId?, name?, mime?|mimeType?, size?|sizeBytes?, path?, content_state?|contentState? }
// Inhalt nur über api.files.get(ref) (§10.2a Nr. 4), Metadaten aus der Plattform-Collection `files`.
// Keine Chunk-Collections, kein Demand-Loader, keine Warteschleifen; Fehler gehen in die Zustandszone.

const APP_ID = 'file-viewer';
const TEXT_PREVIEW_RANGE_BYTES = 256 * 1024;
const TEXT_MIME_EXTRA = ['application/json', 'application/xml'];
const PICKER_LIMIT = 200;

// Eingebettetes Rückfall-Wörterbuch (Name `labels` wie in den anderen Apps, damit check-i18n-keys es findet).
const labels = {
  de: {
    title: 'Datei-Viewer',
    file: 'Datei',
    download: 'Herunterladen',
    openFailed: 'Datei konnte nicht geöffnet werden',
    noPreview: 'Keine Vorschau verfügbar – bitte herunterladen.',
    materializeFailed: 'Materialisieren fehlgeschlagen',
    noFileOpen: 'Keine Datei geöffnet',
    openFile: 'Datei öffnen…',
    pickerTitle: 'Datei öffnen',
    search: 'Suchen',
    cancel: 'Abbrechen',
  },
  en: {
    title: 'File Viewer',
    file: 'File',
    download: 'Download',
    openFailed: 'File could not be opened',
    noPreview: 'No preview available – please download.',
    materializeFailed: 'Materialization failed',
    noFileOpen: 'No file open',
    openFile: 'Open file…',
    pickerTitle: 'Open file',
    search: 'Search',
    cancel: 'Cancel', // v4-allow: en
  },
};

export async function mount(root, api, { restored } = {}) {
  if (!root.dataset.app) root.dataset.app = APP_ID;
  const offs = [];
  let disposed = false;
  let objectUrl = '';
  let current = null; // { ref, fileId, name, mime, size, path, contentState }
  let loadToken = 0;
  let metaOff = null;
  let commandOff = null;

  const lang = () => (String(api.shell.locale.get() || 'de').startsWith('en') ? 'en' : 'de');
  const t = (key) => {
    const full = `${APP_ID}.${key}`;
    const v = api.shell.t(full);
    return v && v !== full ? v : (labels[lang()][key] ?? labels.de[key] ?? key);
  };

  root.innerHTML = await loadModuleMarkup();
  const els = {
    title: root.querySelector('[data-file-title]'),
    metadata: root.querySelector('[data-file-metadata]'),
    content: root.querySelector('[data-file-content]'),
    download: root.querySelector('[data-file-download]'),
    start: root.querySelector('[data-file-start]'),
    empty: root.querySelector('[data-file-empty]'),
    emptyText: root.querySelector('[data-file-empty-text]'),
    openPicker: root.querySelector('[data-file-open-picker]'),
    picker: root.querySelector('[data-file-picker]'),
    pickerTitle: root.querySelector('[data-file-picker-title]'),
    pickerCancel: root.querySelector('[data-file-picker-cancel]'),
    pickerSearch: root.querySelector('[data-file-picker-search]'),
    pickerList: root.querySelector('[data-file-picker-list]'),
  };
  let pickerOff = null;
  let pickerRows = [];

  function applyStaticLabels() {
    els.download?.setAttribute('aria-label', t('download'));
    els.download?.setAttribute('title', t('download'));
    if (els.emptyText) els.emptyText.textContent = t('noFileOpen');
    if (els.openPicker) els.openPicker.textContent = t('openFile');
    if (els.pickerTitle) els.pickerTitle.textContent = t('pickerTitle');
    if (els.pickerCancel) els.pickerCancel.textContent = t('cancel');
    els.pickerSearch?.setAttribute('placeholder', t('search'));
    els.pickerSearch?.setAttribute('aria-label', t('search'));
  }
  applyStaticLabels();

  function report(code, text, level = 'error', ref) {
    api.status.report({ level, code: `${APP_ID}.${code}`, text, ...(ref ? { ref } : {}) });
  }
  function clear(code) { api.status.clear(`${APP_ID}.${code}`); }

  function renderHeader() {
    const name = current?.name || t('file');
    if (els.title) els.title.textContent = name;
    if (els.metadata) els.metadata.textContent = current ? `${current.mime} · ${formatBytes(current.size)}` : '';
    api.shell.setTitle(current?.name || t('title'));
    // Ohne geöffnete Datei hat „Herunterladen“ nichts zu tun: gesperrt statt wirkungslos (Rundgang RUND1).
    if (els.download) els.download.disabled = !current;
    // FIX8: ohne Datei (Launcher/Startmenü) Leerzustand „Keine Datei geöffnet“ + „Datei öffnen…“ statt Fehler.
    if (els.start) els.start.hidden = Boolean(current);
    if (current) closePicker();
  }

  // ---- Dateiauswahl (nur erlaubte Collection `files`, lesend; Abo nur solange die Auswahl offen ist) ----
  function renderPicker() {
    if (!els.pickerList) return;
    const q = String(els.pickerSearch?.value || '').trim().toLowerCase();
    const rows = pickerRows
      .filter((f) => f && (f.ref || f.id) && (!q || `${f.name || ''} ${f.mime || ''}`.toLowerCase().includes(q)));
    els.pickerList.replaceChildren(...rows.map((f) => {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'file-viewer-picker-item';
      b.setAttribute('role', 'option');
      b.dataset.fileId = String(f.id || '');
      const name = document.createElement('span');
      name.textContent = f.name || t('file');
      const meta = document.createElement('small');
      meta.textContent = `${f.mime || ''} · ${formatBytes(f.size)}`;
      b.append(name, meta);
      b.addEventListener('click', () => pick(f));
      return b;
    }));
  }
  function openPicker() {
    if (!els.picker) return;
    els.empty.hidden = true;
    els.picker.hidden = false;
    pickerOff?.();
    pickerOff = api.query('files', { sort: [{ name: 'asc' }], limit: PICKER_LIMIT }).subscribe((rows) => {
      if (disposed) return;
      pickerRows = Array.isArray(rows) ? rows : [];
      renderPicker();
    });
    els.pickerSearch?.focus();
  }
  function closePicker() {
    pickerOff?.(); pickerOff = null;
    pickerRows = [];
    if (els.picker) els.picker.hidden = true;
    if (els.empty) els.empty.hidden = false;
    if (els.pickerSearch) els.pickerSearch.value = '';
    els.pickerList?.replaceChildren();
  }
  function pick(f) {
    closePicker();
    open({ ref: f.ref || '', file_id: f.id || '', name: f.name || '', mime: f.mime || '', size: f.size });
  }
  const onPickerKey = (event) => { if (event.key === 'Escape') { event.preventDefault(); closePicker(); els.openPicker?.focus(); } };
  els.openPicker?.addEventListener('click', openPicker);
  els.pickerCancel?.addEventListener('click', closePicker);
  els.pickerSearch?.addEventListener('input', renderPicker);
  els.picker?.addEventListener('keydown', onPickerKey);

  function resetContent() {
    if (objectUrl) URL.revokeObjectURL(objectUrl);
    objectUrl = '';
    els.content?.replaceChildren();
  }

  function stopWatchers() {
    metaOff?.(); metaOff = null;
    commandOff?.(); commandOff = null;
  }

  async function showBlob(blob) {
    const token = loadToken;
    const preview = previewBlobFor(blob, current.mime, current.size);
    resetContent();
    objectUrl = URL.createObjectURL(preview);
    await renderBlob(els.content, objectUrl, preview, current.name, current.mime, () => report('preview.unsupported', t('noPreview'), 'info'));
    if (token !== loadToken || disposed) return;
    current.blob = blob;
  }

  async function loadRef(ref) {
    const token = ++loadToken;
    try {
      const blob = await api.files.get(ref);
      if (token !== loadToken || disposed) return;
      clear('open');
      await showBlob(blob);
    } catch (error) {
      if (token !== loadToken || disposed) return;
      report('open', `${t('openFailed')}: ${error?.code || error?.message || error}`);
    }
  }

  function materialize() {
    const h = api.command('ctox.file.materialize', {
      record_id: current.fileId,
      file_id: current.fileId,
      path: current.path,
      title: `Load ${current.name}`,
      client_context: { source: 'business-os-file-viewer' },
    });
    h.local.catch((error) => report('command.ctox.file.materialize', `${t('materializeFailed')}: ${error?.message || error}`, 'error', h.id));
    commandOff = h.status.subscribe((s) => {
      if (s?.status === 'failed') report('command.ctox.file.materialize', `${t('materializeFailed')}: ${s.error || 'failed'}`, 'error', h.id);
      if (s?.status === 'done') {
        clear('command.ctox.file.materialize');
        const ref = typeof s.result?.ref === 'string' ? s.result.ref : '';
        if (ref && !current.ref) { current.ref = ref; loadRef(ref); }
      }
    });
  }

  function open(params = {}) {
    const next = normalizeTarget(params);
    if (!next.ref && !next.fileId) return;
    stopWatchers();
    resetContent();
    clear('open'); clear('preview.unsupported');
    current = next;
    renderHeader();
    if (current.ref) { loadRef(current.ref); return; }
    // Nur eine Datei-ID: Metadaten live aus `files` (Plattform); sobald `ref` da ist, laden.
    let requested = false;
    metaOff = api.get('files', current.fileId).subscribe((meta) => {
      if (disposed) return;
      if (meta?.ref) {
        current = { ...current, ref: meta.ref, name: current.name || meta.name, mime: meta.mime || current.mime, size: Number(meta.size ?? current.size) };
        renderHeader();
        metaOff?.(); metaOff = null;
        loadRef(meta.ref);
        return;
      }
      if (!requested && current.path && ['lazy', 'missing', ''].includes(current.contentState)) {
        requested = true;
        materialize();
      }
    });
  }

  function onDownload() {
    if (!current) return;
    const run = async () => {
      const blob = current.blob || (current.ref ? await api.files.get(current.ref) : null);
      if (!blob) return;
      const url = URL.createObjectURL(blob);
      try { triggerDownload(url, current.name); } finally { setTimeout(() => URL.revokeObjectURL(url), 0); }
    };
    run().catch((error) => report('download', `${t('openFailed')}: ${error?.code || error?.message || error}`));
  }
  els.download?.addEventListener('click', onDownload);

  offs.push(api.shell.onRoute((ev) => open(ev?.params || {})));
  offs.push(api.shell.locale.subscribe(() => { applyStaticLabels(); renderHeader(); }));

  const restoredTarget = isPlainObject(restored) && isPlainObject(restored.target) ? restored.target : null;
  if (restoredTarget) open(restoredTarget);
  else renderHeader();

  return {
    unmount() {
      disposed = true;
      loadToken += 1;
      stopWatchers();
      closePicker();
      for (const off of offs.splice(0)) off();
      els.download?.removeEventListener('click', onDownload);
      resetContent();
      root.replaceChildren();
    },
    snapshot() {
      if (!current) return { v: 1 };
      const { ref, fileId, name, mime, size, path, contentState } = current;
      return { v: 1, target: { ref, file_id: fileId, name, mime, size, path, content_state: contentState } };
    },
  };
}

function normalizeTarget(params = {}) {
  const str = (...keys) => { for (const k of keys) if (typeof params[k] === 'string' && params[k]) return params[k]; return ''; };
  const size = Number(params.size ?? params.sizeBytes ?? params.size_bytes ?? 0);
  return {
    ref: str('ref', 'content_ref'),
    fileId: str('file_id', 'fileId', 'id'),
    name: str('name'),
    mime: str('mime', 'mimeType', 'mime_type') || 'application/octet-stream',
    size: Number.isFinite(size) ? size : 0,
    path: str('path', 'local_path'),
    contentState: str('content_state', 'contentState'),
  };
}

function isPlainObject(v) { return Boolean(v) && typeof v === 'object' && !Array.isArray(v); }

function isTextPreviewMimeType(mimeType = '') {
  const normalized = String(mimeType || '').toLowerCase();
  return normalized.startsWith('text/') || TEXT_MIME_EXTRA.includes(normalized);
}

export function textPreviewRangeFor(mimeType = '', sizeBytes = 0) {
  const size = Number(sizeBytes || 0);
  if (!Number.isFinite(size) || size <= TEXT_PREVIEW_RANGE_BYTES) return null;
  if (!isTextPreviewMimeType(mimeType)) return null;
  return { offset: 0, length: TEXT_PREVIEW_RANGE_BYTES };
}

/** Große Textdateien: Vorschau auf die ersten 256 KiB begrenzen (wie vorher über den Bereichs-Abruf). */
function previewBlobFor(blob, mimeType, sizeBytes) {
  const range = textPreviewRangeFor(mimeType, Math.max(Number(sizeBytes || 0), blob.size));
  return range ? blob.slice(range.offset, range.offset + range.length, blob.type) : blob;
}

async function renderBlob(container, objectUrl, blob, name, mimeType, onUnsupported) {
  if (!container) return;
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
  if (isTextPreviewMimeType(mimeType)) {
    const text = await blob.text();
    container.innerHTML = '<pre class="file-viewer-text" data-file-text></pre>';
    container.querySelector('[data-file-text]').textContent = text;
    return;
  }
  container.replaceChildren();
  onUnsupported?.();
}

function triggerDownload(url, name) {
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = name;
  anchor.click();
}

async function loadModuleMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((response) => response.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((node) => node.remove());
  return doc.body.innerHTML;
}

function formatBytes(value) {
  const bytes = Number(value || 0);
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (char) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[char]));
}

export const __fileViewerTestHooks = Object.freeze({ textPreviewRangeFor, previewBlobFor, normalizeTarget });
