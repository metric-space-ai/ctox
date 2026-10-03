const EDITOR_PROTOCOL = 'euro-office-cell-binary-v10';
const EDITOR_PROTOCOL_VERSION = 10;
const EMBEDDED_EDITOR_HTML_BASE64 = Object.freeze({
  document: '__CTOX_EMBEDDED_DOCUMENT_HTML_BASE64__',
  spreadsheet: '__CTOX_EMBEDDED_SPREADSHEET_HTML_BASE64__',
});

// A save acknowledges the revision serialized at its start, never edits that
// happened while the native commit was in flight. Independent of SDK history.
export function createOfficeSaveTracker() {
  let generation = 0;
  let revision = 0;
  let savedRevision = 0;
  let failed = false;
  return {
    edit() { revision += 1; },
    reset() { generation += 1; savedRevision = revision; failed = false; },
    snapshot() { return { generation, revision }; },
    acknowledge(snapshot) {
      if (snapshot.generation !== generation) return false;
      savedRevision = Math.max(savedRevision, snapshot.revision);
      failed = false;
      return savedRevision === revision;
    },
    fail() { failed = true; },
    get dirty() { return failed || revision !== savedRevision; },
  };
}

export async function createCtoxForkRuntime({ root, bridge, permissions, emit, locale = 'de', theme = 'system', appearance = {}, kind = 'spreadsheet', launchArgs = {} }) {
  const appearanceUrl = new URL('../shell-appearance.mjs', import.meta.url);
  const appearanceRevision = new URL(import.meta.url).searchParams.get('v');
  if (appearanceRevision) appearanceUrl.searchParams.set('v', appearanceRevision);
  const { applyShellAppearance } = await import(appearanceUrl.href);
  const isDocument = kind === 'document';
  const productId = isDocument ? 'ctox-documents' : 'ctox-spreadsheets';
  const productName = isDocument ? 'CTOX Documents' : 'CTOX Spreadsheets';
  const runtimeOrigin = new URL(document.baseURI).origin;
  const editorProtocol = isDocument ? 'euro-office-word-binary-v10' : EDITOR_PROTOCOL;
  const editorProtocolVersion = EDITOR_PROTOCOL_VERSION;
  let access = { ...permissions };
  let recordId = null;
  let versionId = null;
  let recordTitle = '';
  let editorBytes = null;
  const saveTracker = createOfficeSaveTracker();
  let documentReady = false;
  let destroyed = false;
  let pendingSave = null;
  let documentMediaResolver = null;
  let forkUi = null;
  let mutationApi = null;
  const onEditorSaveShortcut = (event) => {
    if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey || event.key.toLowerCase() !== 's') return;
    // The compiled SDK's internal shortcut calls its private server-save
    // method, bypassing the public asc_Save adapter. Route the real keyboard
    // command through the same native commit path as the toolbar.
    event.preventDefault();
    event.stopImmediatePropagation();
    if (!destroyed && documentReady && access.write !== false) {
      frame.contentWindow.Asc.editor.asc_Save();
    }
  };
  const onUserActionEnd = () => {
    if (destroyed || !documentReady || access.write === false) return;
    saveTracker.edit();
    emit?.('dirty', { recordId, versionId, dirty: true });
  };
  let requestedTheme = normalizeTheme(theme);
  const colorScheme = matchMedia('(prefers-color-scheme: dark)');
  const frameEditorId = `ctox-office-${crypto.randomUUID()}`;
  const frame = document.createElement('iframe');
  frame.className = `ctox-office-fork-frame ${productId}-frame`;
  frame.title = `${productName} Editor`;
  frame.style.cssText = 'display:block;width:100%;height:100%;border:0;background:transparent';
  const entry = new URL(`../upstream/web-apps/apps/${isDocument ? 'documenteditor' : 'spreadsheeteditor'}/main/index.html`, import.meta.url);
  entry.searchParams.set('lang', locale === 'en' ? 'en' : 'de');
  entry.searchParams.set('frameEditorId', frameEditorId);
  entry.searchParams.set('parentOrigin', runtimeOrigin);
  const assetRevision = new URL(import.meta.url).searchParams.get('v');
  if (assetRevision) entry.searchParams.set('v', assetRevision);
  const embeddedHtml = embeddedEditorDocument(kind, entry);
  // Keep the same-origin document in the frame. Some embedded browser hosts
  // reject blob navigations, leaving about:blank without a load error.
  // The embedded document already supplies its asset base and launch query.
  if (embeddedHtml) frame.srcdoc = embeddedHtml;
  else frame.src = entry.href;
  root.replaceChildren(frame);

  let resolveAppReady;
  let rejectAppReady;
  const appReady = new Promise((resolve, reject) => { resolveAppReady = resolve; rejectAppReady = reject; });
  const appReadyTimeoutMs = normalizeAppReadyTimeout(launchArgs.appReadyTimeoutMs);
  const readyTimeout = setTimeout(() => rejectAppReady(new Error(`${productName} app-ready timed out`)), appReadyTimeoutMs);
  const onMessage = async (event) => {
    if (destroyed || event.origin !== runtimeOrigin || event.source !== frame.contentWindow) return;
    let message = event.data;
    if (typeof message === 'string') {
      try { message = JSON.parse(message); } catch { return; }
    }
    if (!message || message.frameEditorId !== frameEditorId) return;
    switch (message.event) {
      case 'onAppReady':
        clearTimeout(readyTimeout);
        installCtoxSdkAdapter(frame.contentWindow, kind, beginSdkSave, failPendingSave);
        frame.contentWindow.addEventListener('keydown', onEditorSaveShortcut, true);
        forkUi = installCtoxForkUi(frame.contentWindow, { productId, productName, kind, theme: requestedTheme });
        applyShellAppearance(frame.contentDocument, appearance);
        resolveAppReady();
        break;
      case 'onDocumentReady':
        documentReady = true;
        mutationApi = frame.contentWindow.Asc?.editor;
        // CTOX's shell schedules native commits; the inherited server timer
        // must stay disabled even when an older browser saved SDK preferences.
        mutationApi?.asc_setAutoSaveGap?.(0);
        mutationApi?.asc_unregisterCallback?.('asc_onUserActionEnd', onUserActionEnd);
        mutationApi?.asc_registerCallback?.('asc_onUserActionEnd', onUserActionEnd);
        applyCtoxForkTheme(frame.contentWindow, requestedTheme, productId, true);
        forkUi?.setTitle(`${recordTitle || productName} · ${productName}`);
        emit?.('opened', inspection());
        break;
      case 'onDocumentStateChange':
        // SDK "clean" notifications describe its local serialization, not a
        // durable native commit. Only that commit may acknowledge our edits.
        if (message.data === true) onUserActionEnd();
        break;
      case 'onSaveDocument':
        await acceptSavedBinary(message.data);
        break;
      case 'onError':
        if (pendingSave) {
          failPendingSave(pendingSave, Object.assign(
            new Error(message.data?.errorDescription || `${productName} save failed`),
            { code: message.data?.errorCode },
          ));
        } else {
          emit?.('error', { code: message.data?.errorCode, message: message.data?.errorDescription || `${productName} error` });
        }
        break;
    }
  };
  const onColorSchemeChange = () => {
    if (requestedTheme === 'system' && frame.contentWindow) {
      applyCtoxForkTheme(frame.contentWindow, requestedTheme, productId, documentReady);
    }
  };
  colorScheme.addEventListener?.('change', onColorSchemeChange);
  window.addEventListener('message', onMessage);
  frame.addEventListener('error', () => rejectAppReady(new Error(`${productName} iframe failed to load`)), { once: true });
  await appReady;

  const send = (command, data, transfer = []) => {
    const payload = command === 'openDocumentFromBinary' ? { command, data } : JSON.stringify({ command, data });
    frame.contentWindow.postMessage(payload, runtimeOrigin, transfer);
  };

  function createPendingSave(reason) {
    let resolve;
    let reject;
    const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
    // Toolbar saves have no RPC caller, but are still observable through events.
    promise.catch(() => {});
    return { promise, resolve, reject, reason, serialized: false };
  }

  function beginSdkSave() {
    if (destroyed || !documentReady || access.write === false || pendingSave?.serialized) return false;
    pendingSave ||= createPendingSave('toolbar');
    const activeSave = pendingSave;
    activeSave.serialized = true;
    activeSave.snapshot = saveTracker.snapshot();
    activeSave.recordId = recordId;
    activeSave.versionId = versionId;
    emit?.('saving', { recordId, versionId });
    return activeSave;
  }

  function failPendingSave(activeSave, error) {
    // Toolbar and keyboard callers have no RPC catch. Release only the save
    // that failed so they can retry, without clearing the draft or a newer save.
    if (!activeSave || pendingSave !== activeSave) return;
    saveTracker.fail();
    pendingSave = null;
    activeSave.reject(error);
    emit?.('error', { code: error?.code || 'save_failed', message: error?.message || String(error) });
  }

  async function acceptSavedBinary(value) {
    if (!pendingSave?.serialized) {
      if (!beginSdkSave()) return;
    }
    const activeSave = pendingSave;
    if (activeSave.committing) return;
    activeSave.committing = true;
    try {
      const bytes = normalizeBytes(value);
      const result = await bridge.commit({
        recordId: activeSave.recordId,
        baseVersionId: activeSave.versionId,
        editorProtocol,
        editorProtocolVersion,
        implementedFeatures: [],
        reason: activeSave.reason,
        bytes,
      }, [bytes.buffer]);
      if (destroyed || pendingSave !== activeSave) return;
      versionId = result.version_id || result.versionId || versionId;
      editorBytes = bytes;
      const clean = saveTracker.acknowledge(activeSave.snapshot);
      if (clean) markUpstreamDocumentSaved();
      const saved = { ...result, dirty: saveTracker.dirty };
      emit?.('saved', { recordId, versionId, dirty: saveTracker.dirty });
      activeSave.resolve(saved);
    } catch (error) {
      failPendingSave(activeSave, error);
    } finally {
      if (pendingSave === activeSave) pendingSave = null;
    }
  }

  function markUpstreamDocumentSaved() {
    const upstream = frame.contentWindow;
    upstream.AscCommon?.History?.Reset_SavedIndex?.(true);
    upstream.Asc?.editor?.SetUnchangedDocument?.();
    if (!isDocument) upstream.Asc?.editor?.SetDocumentModified?.(false);
    emit?.('clean', { recordId, versionId, dirty: false });
  }

  function inspection() {
    return {
      kind,
      runtime: `${productId}-fork`,
      product_id: productId,
      product_name: productName,
      protocol: editorProtocol,
      protocol_version: editorProtocolVersion,
      record_id: recordId,
      version_id: versionId,
      document_ready: documentReady,
      dirty: saveTracker.dirty,
      read_only: access.write === false,
      source: { fork: productId, upstream_ancestry: 'euro-office-v9.3.1', web_apps: true, sdkjs: true },
    };
  }

  return {
    async open(request = {}) {
      if (pendingSave || saveTracker.dirty) {
        throw Object.assign(new Error('Save pending changes before opening another version'), { code: 'unsaved_changes' });
      }
      let loaded = await bridge.loadVersion(request);
      if (!hasEditorBinarySignature(loaded.editorBytes, kind)) {
        await bridge.prepare({ recordId: request.recordId, versionId: loaded.version?.id || request.versionId });
        loaded = await bridge.loadVersion({ recordId: request.recordId, versionId: loaded.version?.id || request.versionId });
      }
      if (!hasEditorBinarySignature(loaded.editorBytes, kind)) {
        const error = new Error(`CTOX Rust prepare did not return a Euro-Office ${isDocument ? 'DOCY' : 'XLSY'} editor payload`);
        error.code = 'unsupported_editor_protocol';
        throw error;
      }
      editorBytes = normalizeBytes(loaded.editorBytes).slice();
      recordId = request.recordId || loaded.record?.id || null;
      versionId = request.versionId || loaded.version?.id || null;
      recordTitle = loaded.record?.filename || loaded.record?.title || '';
      documentReady = false;
      saveTracker.reset();
      const upstream = frame.contentWindow;
      upstream.__ctoxEditorBinary = editorBytes;
      documentMediaResolver?.destroy?.();
      documentMediaResolver = isDocument
        ? await installDocumentMediaResolver(upstream, loaded.canonicalBytes)
        : null;
      send('init', { config: editorConfig(locale, access, requestedTheme) });
      send('openDocument', { doc: documentConfig(recordId, loaded.record, access, kind) });
      return inspection();
    },
    save({ reason = 'manual' } = {}) {
      if (!documentReady) throw new Error(`${productName} is not ready`);
      if (access.write === false) throw permissionError(`${isDocument ? 'Document' : 'Spreadsheet'} is read-only`);
      if (pendingSave) return pendingSave.promise;
      pendingSave = createPendingSave(String(reason || 'manual'));
      const activeSave = pendingSave;
      try { frame.contentWindow.Asc.editor.asc_Save(); }
      catch (error) {
        failPendingSave(activeSave, error);
      }
      return activeSave.promise;
    },
    async export({ format = 'xlsx' } = {}) {
      const expectedFormat = isDocument ? 'docx' : 'xlsx';
      if (format !== expectedFormat) throw Object.assign(new Error(`Unsupported ${kind} export format: ${format}`), { code: 'unsupported_format' });
      if (access.export === false) throw permissionError(`${isDocument ? 'Document' : 'Spreadsheet'} export is not permitted`);
      // Export the acknowledged snapshot containing the edits present when the
      // user requested the download. A running save may serialize an older
      // revision, so wait for it and save any remaining draft before exporting.
      // A failed save must reject the download, never silently return old data.
      if (pendingSave) await pendingSave.promise;
      if (saveTracker.dirty) await this.save({ reason: 'export' });
      return bridge.export({ recordId, versionId, format });
    },
    focus() { frame.contentWindow.focus(); return { focused: true }; },
    setPermissions(next = {}) { access = { ...access, ...next }; return inspection(); },
    setTheme(nextTheme = 'system') {
      requestedTheme = normalizeTheme(nextTheme);
      applyCtoxForkTheme(frame.contentWindow, requestedTheme, productId, documentReady);
      return { theme: requestedTheme, resolved_theme: resolveTheme(requestedTheme) };
    },
    setAppearance(next = {}) {
      appearance = next;
      requestedTheme = normalizeTheme(next.theme);
      applyCtoxForkTheme(frame.contentWindow, requestedTheme, productId, documentReady);
      applyShellAppearance(frame.contentDocument, appearance);
      return { theme: requestedTheme };
    },
    inspect: inspection,
    async destroy() {
      destroyed = true;
      clearTimeout(readyTimeout);
      window.removeEventListener('message', onMessage);
      frame.contentWindow?.removeEventListener('keydown', onEditorSaveShortcut, true);
      colorScheme.removeEventListener?.('change', onColorSchemeChange);
      mutationApi?.asc_unregisterCallback?.('asc_onUserActionEnd', onUserActionEnd);
      pendingSave?.reject?.(new Error(`${productName} runtime destroyed`));
      pendingSave = null;
      documentMediaResolver?.destroy?.();
      documentMediaResolver = null;
      forkUi?.destroy?.();
      forkUi = null;
      frame.remove();
    },
  };
}

function embeddedEditorDocument(kind, entry) {
  const encoded = EMBEDDED_EDITOR_HTML_BASE64[kind];
  if (!encoded || encoded.startsWith('__CTOX_EMBEDDED_')) return '';
  const query = JSON.stringify(entry.search.slice(1));
  const html = atob(encoded)
    .replaceAll('window.location.search.substring(1)', query)
    .replace('+function registerServiceWorker(){', '+function registerServiceWorker(){return;');
  const base = `<base href="${escapeHtmlAttribute(entry.href)}">`;
  return /<head(?:\s[^>]*)?>/i.test(html)
    ? html.replace(/<head(?:\s[^>]*)?>/i, (head) => `${head}\n    ${base}`)
    : `<!doctype html><html><head>${base}</head><body>${html}</body></html>`;
}

function escapeHtmlAttribute(value) {
  return String(value).replaceAll('&', '&amp;').replaceAll('"', '&quot;').replaceAll('<', '&lt;');
}

async function installDocumentMediaResolver(upstream, canonicalBytes) {
  const bytes = canonicalBytes ? normalizeBytes(canonicalBytes).slice() : null;
  const mediaEntries = bytes ? await extractOfficeZipMedia(bytes) : [];
  const urls = {};
  const objectUrls = [];
  for (const entry of mediaEntries) {
    const local = entry.name.replace(/^word\//, '');
    const basename = local.slice(local.lastIndexOf('/') + 1);
    const blob = new upstream.Blob([entry.bytes], { type: mimeTypeForOfficeMedia(entry.name) });
    const objectUrl = upstream.URL.createObjectURL(blob);
    objectUrls.push(objectUrl);
    urls[local] = objectUrl;
    urls[`media/${local}`] = objectUrl;
    urls[`media/${basename}`] = objectUrl;
    urls[basename] = objectUrl;
  }
  upstream.AscCommon?.g_oDocumentUrls?.addUrls?.(urls);
  const restoreImageSrcRewrite = installMediaImageSrcRewrite(upstream, urls);
  const resolver = {
    urls,
    count: mediaEntries.length,
    destroy() {
      restoreImageSrcRewrite?.();
      for (const objectUrl of objectUrls) upstream.URL.revokeObjectURL(objectUrl);
    },
  };
  upstream.__ctoxOfficeMediaResolver = resolver;
  return resolver;
}

function installMediaImageSrcRewrite(upstream, urls) {
  const imagePrototype = upstream.HTMLImageElement?.prototype;
  const descriptor = imagePrototype && Object.getOwnPropertyDescriptor(imagePrototype, 'src');
  if (!descriptor?.set || imagePrototype.__ctoxOfficeMediaSrcRewrite) return null;
  Object.defineProperty(imagePrototype, 'src', {
    configurable: true,
    enumerable: descriptor.enumerable,
    get: descriptor.get,
    set(value) {
      descriptor.set.call(this, resolveOfficeMediaUrl(value, urls) || value);
    },
  });
  imagePrototype.__ctoxOfficeMediaSrcRewrite = true;
  return () => {
    delete imagePrototype.__ctoxOfficeMediaSrcRewrite;
    Object.defineProperty(imagePrototype, 'src', descriptor);
  };
}

function resolveOfficeMediaUrl(value, urls) {
  if (typeof value !== 'string' || !urls) return null;
  if (urls[value]) return urls[value];
  const normalized = value.replaceAll('\\', '/');
  const mediaIndex = normalized.lastIndexOf('/media/');
  if (mediaIndex >= 0) {
    const mediaPath = `media/${normalized.slice(mediaIndex + '/media/'.length)}`;
    return urls[mediaPath] || urls[`media/${mediaPath}`] || urls[mediaPath.slice('media/'.length)] || null;
  }
  if (normalized.startsWith('media/')) {
    return urls[normalized] || urls[`media/${normalized}`] || urls[normalized.slice('media/'.length)] || null;
  }
  return null;
}

async function extractOfficeZipMedia(bytes) {
  const source = normalizeBytes(bytes);
  const entries = [];
  let offset = 0;
  while (offset + 30 <= source.length) {
    if (readU32(source, offset) !== 0x04034b50) break;
    const flags = readU16(source, offset + 6);
    const method = readU16(source, offset + 8);
    const compressedSize = readU32(source, offset + 18);
    const uncompressedSize = readU32(source, offset + 22);
    const nameLength = readU16(source, offset + 26);
    const extraLength = readU16(source, offset + 28);
    const nameStart = offset + 30;
    const dataStart = nameStart + nameLength + extraLength;
    if (nameStart + nameLength > source.length || dataStart > source.length) break;
    const name = new TextDecoder().decode(source.subarray(nameStart, nameStart + nameLength));
    if ((flags & 0x08) !== 0) break;
    const dataEnd = dataStart + compressedSize;
    if (dataEnd > source.length) break;
    if (name.startsWith('word/media/') && !name.endsWith('/')) {
      const compressed = source.subarray(dataStart, dataEnd);
      entries.push({ name, bytes: await inflateZipEntry(compressed, method, uncompressedSize) });
    }
    offset = dataEnd;
  }
  return entries;
}

async function inflateZipEntry(compressed, method, expectedSize) {
  if (method === 0) return compressed.slice();
  if (method !== 8) throw new Error(`Unsupported DOCX media ZIP method: ${method}`);
  if (typeof DecompressionStream !== 'function') throw new Error('Browser does not support deflate-raw media decompression');
  const stream = new Blob([compressed]).stream().pipeThrough(new DecompressionStream('deflate-raw'));
  const bytes = new Uint8Array(await new Response(stream).arrayBuffer());
  if (expectedSize && bytes.byteLength !== expectedSize) {
    throw new Error(`Invalid DOCX media size: expected ${expectedSize}, got ${bytes.byteLength}`);
  }
  return bytes;
}

function mimeTypeForOfficeMedia(name) {
  const lower = String(name || '').toLowerCase();
  if (lower.endsWith('.png')) return 'image/png';
  if (lower.endsWith('.jpg') || lower.endsWith('.jpeg')) return 'image/jpeg';
  if (lower.endsWith('.gif')) return 'image/gif';
  if (lower.endsWith('.webp')) return 'image/webp';
  if (lower.endsWith('.svg')) return 'image/svg+xml';
  if (lower.endsWith('.bmp')) return 'image/bmp';
  return 'application/octet-stream';
}

function readU16(bytes, offset) {
  return bytes[offset] | (bytes[offset + 1] << 8);
}

function readU32(bytes, offset) {
  return (bytes[offset] | (bytes[offset + 1] << 8) | (bytes[offset + 2] << 16) | (bytes[offset + 3] << 24)) >>> 0;
}

function installCtoxSdkAdapter(upstream, kind, beginSave = () => true, failSave = (_save, error) => { throw error; }) {
  const prototype = kind === 'document'
    ? upstream.Asc?.asc_docs_api?.prototype
    : upstream.Asc?.spreadsheet_api?.prototype;
  if (!prototype) throw new Error(`CTOX ${kind} fork SDK API is unavailable`);
  prototype.asc_LoadDocument = function () {
    if (!(upstream.__ctoxEditorBinary instanceof Uint8Array)) throw new Error('CTOX editor binary is unavailable');
    this.asc_openDocumentFromBytes(upstream.__ctoxEditorBinary);
    if (typeof this.ctox_completeInitialServerPhase !== 'function') {
      throw new Error('CTOX fork SDK hook is unavailable');
    }
    this.ctox_completeInitialServerPhase();
  };
  if (kind === 'document') {
    prototype.asc_Save = function (isAutoSave, isIdle) {
      // The shell owns debounced, durable saves through bridge.commit. The
      // inherited timer targets the excluded coauthoring server and otherwise
      // leaves its save action pending forever.
      if (isAutoSave) return false;
      const activeSave = beginSave();
      if (!activeSave) return false;
      try {
        if (typeof this.asc_nativeGetFile2 !== 'function') {
          throw Object.assign(new Error('CTOX document native serializer is unavailable'), { code: 'native_serializer_unavailable' });
        }
        const encoded = this.asc_nativeGetFile2();
        if (typeof encoded !== 'string') {
          throw Object.assign(new Error('CTOX document native serializer returned invalid editor bytes'), { code: 'invalid_native_save' });
        }
        const binary = upstream.atob(encoded);
        const bytes = new Uint8Array(binary.length);
        for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
        if (!hasEditorBinarySignature(bytes, kind)) {
          throw Object.assign(new Error('CTOX document native serializer returned invalid editor bytes'), { code: 'invalid_native_save' });
        }
        this.sendEvent('asc_onSaveDocument', bytes);
        return true;
      } catch (error) {
        failSave(activeSave, error);
        return false;
      }
    };
  } else {
    prototype.asc_Save = function (isAutoSave, isIdle) {
      if (isAutoSave) return false;
      const activeSave = beginSave();
      if (!activeSave) return false;
      try {
        if (typeof this.asc_nativeGetFile !== 'function') {
          throw Object.assign(new Error('CTOX spreadsheet native serializer is unavailable'), { code: 'native_serializer_unavailable' });
        }
        const encoded = this.asc_nativeGetFile();
        const bytes = decodeSpreadsheetNativeFile(upstream, encoded);
        if (!bytes || !hasEditorBinarySignature(bytes, kind)) {
          throw Object.assign(new Error('CTOX spreadsheet native serializer returned invalid editor bytes'), { code: 'invalid_native_save' });
        }
        this.sendEvent('asc_onSaveDocument', bytes);
        return true;
      } catch (error) {
        failSave(activeSave, error);
        return false;
      }
    };
  }
  const sdkReady = waitForFullSdk(upstream, kind);
  prototype.asc_getEditorPermissions = function () {
    const api = this;
    const result = {
      asc_getLicenseType: () => upstream.Asc.c_oLicenseResult.Success,
      asc_getRights: () => upstream.Asc.c_oRights.Edit,
      asc_getBuildVersion: () => '4.3.0',
      asc_getBuildNumber: () => '0',
      asc_getIsLight: () => false,
      asc_getLiveViewerSupport: () => false,
      asc_getIsAnalyticsEnable: () => false,
      asc_getCanBranding: () => false,
      asc_getCustomization: () => false,
      asc_getLicenseMode: () => 0,
      asc_getIsBeta: () => false,
    };
    sdkReady.then(() => api.sendEvent('asc_onGetEditorPermissions', result));
  };
}

function installCtoxForkUi(editorWindow, { productId, productName, kind, theme }) {
  const editorDocument = editorWindow.document;
  let ownedTitle = productName;
  const setTitle = (nextTitle) => {
    ownedTitle = String(nextTitle || productName);
    if (editorDocument.title !== ownedTitle) editorDocument.title = ownedTitle;
  };
  setTitle(productName);
  editorDocument.documentElement.dataset.ctoxProduct = productId;
  editorDocument.body.dataset.ctoxProduct = productId;
  editorDocument.body.dataset.ctoxEditorKind = kind;
  const styles = [
    ['ctox-fork-business-os', new URL('../forks/shared/business-os.css', import.meta.url).href],
    [`${productId}-business-os`, new URL(`../forks/${productId}/business-os.css`, import.meta.url).href],
  ];
  for (const [id, href] of styles) {
    if (editorDocument.getElementById(id)) continue;
    const link = editorDocument.createElement('link');
    link.id = id;
    link.rel = 'stylesheet';
    const stylesheet = new URL(href);
    const revision = new URL(import.meta.url).searchParams.get('v');
    if (revision) stylesheet.searchParams.set('v', revision);
    link.href = stylesheet.href;
    editorDocument.head.append(link);
  }
  applyCtoxForkTheme(editorWindow, theme, productId);
  const titleObserver = new editorWindow.MutationObserver(() => setTitle(ownedTitle));
  titleObserver.observe(editorDocument.head, { childList: true, characterData: true, subtree: true });
  return {
    setTitle,
    destroy() { titleObserver.disconnect(); },
  };
}

function applyCtoxForkTheme(editorWindow, theme, productId, updateSdk = false) {
  const resolved = resolveTheme(theme);
  // The fork's theme service owns icon sprites and SDK palette updates.
  // Body classes alone leave the light icons on a dark shell surface.
  if (updateSdk) editorWindow.Common?.UI?.Themes?.setTheme?.(`theme-${resolved}`);
  const body = editorWindow.document.body;
  editorWindow.document.documentElement.dataset.ctoxProduct = productId;
  editorWindow.document.documentElement.dataset.ctoxTheme = resolved;
  body.dataset.ctoxProduct = productId;
  body.dataset.ctoxTheme = resolved;
  // theme-white has different ribbon metrics (84px controls rather than 66px).
  // Mixing it with the theme-light service makes the ribbon cover the formula
  // input because the editor layout still reserves the theme-light height.
  body.classList.remove('theme-white');
  body.classList.toggle('theme-light', resolved === 'light');
  body.classList.toggle('theme-type-light', resolved === 'light');
  body.classList.toggle('theme-dark', resolved === 'dark');
  body.classList.toggle('theme-type-dark', resolved === 'dark');
}

function normalizeTheme(theme) {
  return theme === 'dark' || theme === 'light' ? theme : 'system';
}

function resolveTheme(theme) {
  if (theme === 'dark' || theme === 'light') return theme;
  return globalThis.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function decodeSpreadsheetNativeFile(upstream, encoded) {
  if (typeof encoded !== 'string' || !encoded.startsWith('XLSY;v')) return null;
  const first = encoded.indexOf(';');
  const second = encoded.indexOf(';', first + 1);
  const third = encoded.indexOf(';', second + 1);
  if (first !== 4 || second < 0 || third < 0) return null;
  const declared = Number.parseInt(encoded.slice(second + 1, third), 10);
  const binary = upstream.atob(encoded.slice(third + 1));
  if (!Number.isSafeInteger(declared) || declared !== binary.length) {
    throw new Error(`CTOX Spreadsheets save length mismatch: declared ${declared}, decoded ${binary.length}`);
  }
  const header = new TextEncoder().encode('XLSY;v10;0;');
  const body = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) body[index] = binary.charCodeAt(index);
  normalizeSpreadsheetSaveOffsets(body, header.length);
  const bytes = new Uint8Array(header.length + body.length);
  bytes.set(header, 0);
  bytes.set(body, header.length);
  return bytes;
}

// asc_nativeGetFile() returns a v2 transport body whose directory and XlsbPos
// offsets are relative to that body. asc_openDocument() consumes the v10 file
// envelope and expects those offsets to be absolute within header + body.
// ref: sdkjs/cell/model/Serialize.js:13733-13870,11973-11994
function normalizeSpreadsheetSaveOffsets(body, headerBytes) {
  const view = new DataView(body.buffer, body.byteOffset, body.byteLength);
  const readU32 = (offset) => {
    if (offset < 0 || offset + 4 > body.length) throw new Error('Euro-Office XLSY offset is truncated');
    return view.getUint32(offset, true);
  };
  const writeU32 = (offset, value) => view.setUint32(offset, value, true);
  const count = body[0];
  if (1 + count * 5 > body.length) throw new Error('Euro-Office XLSY directory is truncated');
  let worksheetsOffset = null;
  for (let index = 0; index < count; index += 1) {
    const entry = 1 + index * 5;
    const relative = readU32(entry + 1);
    if (relative >= body.length) throw new Error(`Euro-Office XLSY table offset is invalid: ${relative}`);
    if (body[entry] === 4) worksheetsOffset = relative;
    writeU32(entry + 1, relative + headerBytes);
  }
  if (worksheetsOffset == null) return;

  const walkItems = (start, end, visit) => {
    let position = start;
    while (position < end) {
      if (position + 5 > end) throw new Error('Euro-Office XLSY item header is truncated');
      const type = body[position];
      const length = readU32(position + 1);
      const content = position + 5;
      const next = content + length;
      if (next > end) throw new Error('Euro-Office XLSY item exceeds its boundary');
      visit(type, content, next);
      position = next;
    }
  };
  const tableLength = readU32(worksheetsOffset);
  const tableEnd = worksheetsOffset + 4 + tableLength;
  if (tableEnd > body.length) throw new Error('Euro-Office XLSY worksheets table is truncated');
  walkItems(worksheetsOffset + 4, tableEnd, (worksheetType, worksheetStart, worksheetEnd) => {
    if (worksheetType !== 0) return;
    walkItems(worksheetStart, worksheetEnd, (recordType, recordStart, recordEnd) => {
      if (recordType !== 9) return;
      walkItems(recordStart, recordEnd, (sheetDataType, sheetDataStart, sheetDataEnd) => {
        if (sheetDataType !== 35) return;
        if (sheetDataEnd - sheetDataStart < 4) throw new Error('Euro-Office XLSY XlsbPos is truncated');
        writeU32(sheetDataStart, readU32(sheetDataStart) + headerBytes);
      });
    });
  });
}

async function waitForFullSdk(upstream, kind) {
  const deadline = performance.now() + 30000;
  while (performance.now() < deadline) {
    const loaded = upstream.performance.getEntriesByType('resource').some((entry) =>
      entry.name.includes(`/sdkjs/${kind === 'document' ? 'word' : 'cell'}/sdk-all.js`) && entry.responseEnd > 0);
    if (loaded) {
      await new Promise((resolve) => setTimeout(resolve, 0));
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error(`CTOX ${kind} fork SDK load timed out`);
}

function editorConfig(locale, permissions, theme = 'system') {
  return {
    mode: permissions.write === false ? 'view' : 'edit',
    lang: locale === 'en' ? 'en' : 'de',
    region: locale === 'en' ? 'en-US' : 'de-DE',
    targetApp: 'web',
    canSaveDocumentToBinary: true,
    user: { id: 'ctox-local-user', name: 'CTOX' },
    customization: {
      about: false,
      feedback: false,
      help: false,
      plugins: false,
      macros: false,
      autosave: false,
      compactHeader: true,
      toolbarHideFileName: true,
      compactToolbar: false,
      hideRightMenu: true,
      uiTheme: resolveTheme(normalizeTheme(theme)) === 'dark' ? 'theme-dark' : 'theme-light',
      zoom: 100,
    },
  };
}

function documentConfig(recordId, record, permissions, kind = 'spreadsheet') {
  const isDocument = kind === 'document';
  return {
    key: String(recordId || `ctox-${kind}`),
    url: `ctox://${kind}/${encodeURIComponent(recordId || 'local')}`,
    title: record?.filename || record?.title || (isDocument ? 'Document.docx' : 'Spreadsheet.xlsx'),
    fileType: isDocument ? 'docx' : 'xlsx',
    permissions: {
      edit: permissions.write !== false,
      download: permissions.export !== false,
      print: false,
      comment: permissions.comment !== false,
      review: permissions.review !== false,
      chat: false,
    },
  };
}

function hasCellBinarySignature(value) {
  if (!value) return false;
  const bytes = normalizeBytes(value);
  return bytes.length >= 5 && bytes[0] === 0x58 && bytes[1] === 0x4c && bytes[2] === 0x53 && bytes[3] === 0x59 && bytes[4] === 0x3b;
}

function hasEditorBinarySignature(value, kind) {
  if (kind !== 'document') return hasCellBinarySignature(value);
  if (!value) return false;
  const bytes = normalizeBytes(value);
  return bytes.length >= 5 && bytes[0] === 0x44 && bytes[1] === 0x4f && bytes[2] === 0x43 && bytes[3] === 0x59 && bytes[4] === 0x3b;
}

function normalizeBytes(value) {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  throw new TypeError('CTOX fork editor payload must be binary');
}

function permissionError(message) { return Object.assign(new Error(message), { code: 'permission_denied' }); }

function normalizeAppReadyTimeout(value) {
  const timeoutMs = Number(value);
  return Number.isFinite(timeoutMs) && timeoutMs >= 10000
    ? Math.min(timeoutMs, 295000)
    : 115000;
}

export const __ctoxForkTestHooks = {
  hasCellBinarySignature,
  hasEditorBinarySignature,
  editorConfig,
  documentConfig,
  extractOfficeZipMedia,
  installDocumentMediaResolver,
  mimeTypeForOfficeMedia,
  resolveOfficeMediaUrl,
  normalizeAppReadyTimeout,
};
