// Legacy rollback source. Never included in the CTOX Documents product bundle.
const DOCX_MIME = 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';

export async function createOfficeFrameRuntime({ root, bridge, permissions, emit, locale = 'de' }) {
  const vendorRoot = import.meta.url.includes('/office-engine/src/')
    ? new URL('../../../vendor/', import.meta.url)
    : new URL('../../', import.meta.url);
  const [{ SuperDoc, DocxZipper }] = await Promise.all([
    import(new URL('superdoc.mjs', vendorRoot).href),
    ensureStyles(new URL('superdoc.css', vendorRoot).href),
  ]);
  let access = { ...permissions };
  let editor = null;
  let recordId = '';
  let versionId = '';
  let dirty = false;
  let lastSave = null;
  let pageCount = 0;
  let currentPage = 1;
  let zoomPercent = 100;
  let historyState = { canUndo: false, canRedo: false, undoDepth: 0, redoDepth: 0 };
  let reviewDiagnostics = [];
  let drawingMutations = {};
  let runtimeCleanup = [];

  const destroyEditor = () => {
    for (const cleanup of runtimeCleanup.splice(0)) {
      try { cleanup(); } catch {}
    }
    try { editor?.destroy?.(); } catch {}
    editor = null;
    pageCount = 0;
    currentPage = 1;
    zoomPercent = 100;
    historyState = { canUndo: false, canRedo: false, undoDepth: 0, redoDepth: 0 };
      reviewDiagnostics = [];
      drawingMutations = {};
    root.replaceChildren();
  };

  return {
    async open(request = {}) {
      destroyEditor();
      recordId = String(request.recordId || '');
      versionId = String(request.versionId || '');
      if (!recordId) throw new Error('recordId is required');
      let loaded = await bridge.loadVersion({ recordId, versionId });
      if (!loaded.editorBytes && loaded.version?.conversion_state !== 'prepared') {
        await bridge.prepare({ recordId, versionId: loaded.version?.id || versionId });
        loaded = await bridge.loadVersion({ recordId, versionId: loaded.version?.id || versionId });
      }
      versionId = loaded.version?.id || versionId;
      const bytes = loaded.editorBytes || loaded.canonicalBytes;
      if (!(bytes instanceof Uint8Array) || !bytes.byteLength) throw new Error('DOCX editor payload is empty');

      const shell = document.createElement('section');
      shell.className = 'ctox-office-document-runtime';
      shell.dataset.readOnly = String(!access.write);
      shell.innerHTML = `
        <div class="ctox-office-view-toolbar" role="toolbar" aria-label="${access.write ? (locale === 'en' ? 'File' : 'Datei') : (locale === 'en' ? 'View' : 'Ansicht')}">
          <strong>${access.write ? (locale === 'en' ? 'File' : 'Datei') : (locale === 'en' ? 'View' : 'Ansicht')}</strong>
          ${access.write ? `<button type="button" data-office-save>${locale === 'en' ? 'Save' : 'Speichern'}</button><output data-office-save-state>${locale === 'en' ? 'Saved' : 'Gespeichert'}</output><button type="button" data-office-undo disabled>${locale === 'en' ? 'Undo' : 'Rückgängig'}</button><button type="button" data-office-redo disabled>${locale === 'en' ? 'Redo' : 'Wiederholen'}</button><label data-office-text-color-label>${locale === 'en' ? 'Text color' : 'Textfarbe'} <input type="color" value="#000000" data-office-text-color aria-label="${locale === 'en' ? 'Text color' : 'Textfarbe'}"></label><label data-office-indent-label>${locale === 'en' ? 'Indent (mm)' : 'Einzug (mm)'} <input type="number" value="0" min="0" max="100" step="0.1" data-office-indent-mm aria-label="${locale === 'en' ? 'Indent in millimetres' : 'Einzug in Millimetern'}"></label><button type="button" data-office-increase-list-level>${locale === 'en' ? 'Increase list level' : 'Listenebene erhöhen'}</button><button type="button" data-office-continue-numbering>${locale === 'en' ? 'Continue numbering' : 'Nummerierung fortführen'}</button><button type="button" data-office-table-row-below>${locale === 'en' ? 'Insert row below' : 'Zeile darunter'}</button><button type="button" data-office-table-column-right>${locale === 'en' ? 'Insert column right' : 'Spalte rechts'}</button><button type="button" data-office-table-merge>${locale === 'en' ? 'Merge cells' : 'Zellen verbinden'}</button><button type="button" data-office-table-split>${locale === 'en' ? 'Split cell 2×2' : 'Zelle 2×2 teilen'}</button><button type="button" data-office-image-resize>${locale === 'en' ? 'Resize inline image' : 'Inline-Bild 6,99 cm'}</button><button type="button" data-office-image-wrap>${locale === 'en' ? 'Square image wrap' : 'Bildumbruch eckig'}</button><button type="button" data-office-image-position>${locale === 'en' ? 'Position floating image' : 'Schwebebild 7,62 / 0,89 cm'}</button><button type="button" data-office-section-landscape>${locale === 'en' ? 'Landscape section 2' : 'Abschnitt 2 quer'}</button><button type="button" data-office-header-footer-options>${locale === 'en' ? 'Header/footer options' : 'Kopf/Fuß Optionen'}</button><button type="button" data-office-section-break>${locale === 'en' ? 'Next-page section' : 'Abschnitt nächste Seite'}</button><button type="button" data-office-external-link>${locale === 'en' ? 'Create external link' : 'Externen Link erstellen'}</button><button type="button" data-office-bookmark>${locale === 'en' ? 'Create bookmark' : 'Lesezeichen erstellen'}</button><button type="button" data-office-update-field>${locale === 'en' ? 'Update field' : 'Feld aktualisieren'}</button><button type="button" data-office-comment-create>${locale === 'en' ? 'Create comment' : 'Kommentar erstellen'}</button><button type="button" data-office-comment-reply>${locale === 'en' ? 'Reply to comment' : 'Kommentar beantworten'}</button><button type="button" data-office-comment-resolve>${locale === 'en' ? 'Resolve comment' : 'Kommentar auflösen'}</button><button type="button" data-office-track-insert>${locale === 'en' ? 'Track insertion' : 'Änderung einfügen'}</button><button type="button" data-office-track-delete>${locale === 'en' ? 'Track deletion' : 'Änderung löschen'}</button><button type="button" data-office-track-reject>${locale === 'en' ? 'Reject deletion' : 'Löschung ablehnen'}</button><button type="button" data-office-track-accept-all>${locale === 'en' ? 'Accept all changes' : 'Alle Änderungen annehmen'}</button><button type="button" data-office-track-final>${locale === 'en' ? 'Create final tracked insertion' : 'Finale Änderung einfügen'}</button><button type="button" data-office-shape-style>${locale === 'en' ? 'Style business shape' : 'Business-Form orange + 90°'}</button><button type="button" data-office-chart-resize>${locale === 'en' ? 'Resize chart to 14 cm' : 'Diagramm 14 × 7 cm'}</button>` : ''}
          <span class="ctox-office-view-separator"></span>
          <button type="button" data-office-zoom-out aria-label="${locale === 'en' ? 'Zoom out' : 'Verkleinern'}">−</button>
          <output data-office-zoom-value>100%</output>
          <button type="button" data-office-zoom-in aria-label="${locale === 'en' ? 'Zoom in' : 'Vergrößern'}">+</button>
        </div>
        <div class="ctox-office-document-toolbar"></div>
        <div class="ctox-office-document-ruler"></div>
        <div class="ctox-office-document-editor"></div>
        <div class="ctox-office-statusbar" role="status">
          <span data-office-page-state>${locale === 'en' ? 'Page 1 of 1' : 'Seite 1 von 1'}</span>
          <span class="ctox-office-status-spacer"></span>
          <button type="button" data-office-status-zoom-out aria-label="${locale === 'en' ? 'Zoom out' : 'Verkleinern'}">−</button>
          <output data-office-status-zoom>Zoom 100%</output>
          <button type="button" data-office-status-zoom-in aria-label="${locale === 'en' ? 'Zoom in' : 'Vergrößern'}">+</button>
        </div>`;
      root.replaceChildren(shell);
      const toolbar = shell.querySelector('.ctox-office-document-toolbar');
      const ruler = shell.querySelector('.ctox-office-document-ruler');
      const editorHost = shell.querySelector('.ctox-office-document-editor');
      toolbar.id = `ctox_office_toolbar_${safeId(recordId)}`;
      ruler.id = `ctox_office_ruler_${safeId(recordId)}`;
      const file = new File([bytes], loaded.record?.filename || 'document.docx', { type: DOCX_MIME });
      let acceptEditorUpdates = false;
      let historyControls = null;
      const markDirty = () => {
        if (!access.write || !acceptEditorUpdates) return;
        const wasDirty = dirty;
        dirty = true;
        const saveState = shell.querySelector('[data-office-save-state]');
        if (saveState) saveState.textContent = locale === 'en' ? 'Unsaved' : 'Ungespeichert';
        requestAnimationFrame(() => historyControls?.update());
        if (!wasDirty) emit('dirty', { recordId, versionId, dirty: true });
      };
      let resolveReady;
      const ready = new Promise((resolve) => { resolveReady = resolve; });
      editor = new SuperDoc({
        selector: editorHost,
        document: file,
        documentMode: access.write ? 'editing' : 'viewing',
        role: access.write ? 'editor' : 'viewer',
        contained: true,
        pagination: true,
        toolbar: `#${toolbar.id}`,
        rulers: true,
        rulerContainer: `#${ruler.id}`,
        viewOptions: { layout: 'print' },
        useLayoutEngine: true,
        layoutEngineOptions: { virtualization: { enabled: false } },
        user: { name: 'CTOX Business OS', email: 'business-os@local' },
        modules: {
          toolbar: { selector: toolbar.id, toolbarGroups: ['left', 'center', 'right'], hideButtons: false, responsiveToContainer: true },
          comments: true,
          collaboration: false,
          whiteboard: false,
          surfaces: { findReplace: true },
        },
        telemetry: { enabled: false },
        onReady: () => resolveReady(),
        onEditorUpdate: markDirty,
        onException: (error) => emit('error', { code: 'editor_exception', message: error?.message || String(error) }),
      });
      await ready;
      const semanticView = bindSemanticViewState({
        shell,
        editor,
        locale,
        emit,
        onState(next) {
          pageCount = next.pageCount;
          currentPage = next.currentPage;
          zoomPercent = next.zoomPercent;
        },
      });
      runtimeCleanup.push(semanticView.destroy);
      historyControls = bindHistoryControls({
        shell,
        editor,
        onState(next) { historyState = next; },
      });
      runtimeCleanup.push(historyControls.destroy);
      const formattingControls = bindFormattingControls({
        shell,
        editor,
        reviewDiagnostics,
        onMutation: markDirty,
        onDrawingMutation(next) { drawingMutations = { ...drawingMutations, ...next }; },
      });
      runtimeCleanup.push(formattingControls.destroy);
      const saveButton = shell.querySelector('[data-office-save]');
      if (saveButton) {
        const onSave = async () => {
          saveButton.disabled = true;
          try {
            await this.save({ reason: 'toolbar' });
          } catch (error) {
            emit('error', { code: error?.code || 'save_failed', message: error?.message || String(error) });
          } finally {
            saveButton.disabled = false;
          }
        };
        saveButton.addEventListener('click', onSave);
        runtimeCleanup.push(() => saveButton.removeEventListener('click', onSave));
      }
      dirty = false;
      acceptEditorUpdates = true;
      emit('opened', { recordId, versionId });
      return this.inspect();
    },

    async save(request = {}) {
      if (!editor) throw new Error('No document is open');
      if (!access.write) throw permissionError('Document is read-only');
      const exported = await editor.export({ triggerDownload: false, isFinalDoc: false });
      const patched = await applyDrawingMutations(exported, drawingMutations, DocxZipper);
      const bytes = await toUint8Array(patched);
      const result = await bridge.commit({
        recordId,
        baseVersionId: versionId,
        editorProtocol: 'ctox-euro-office-editor-bootstrap-v1',
        editorProtocolVersion: 1,
        implementedFeatures: [
          'document.open-render-zoom',
          'document.edit-save',
          'document.undo-clipboard-keyboard',
          'document.character-paragraph-formatting',
          'document.styles-lists-numbering',
          'document.tables',
          'document.images-positioning',
          'document.sections-headers-footers',
          'document.links-bookmarks-fields',
          'document.comments-track-changes',
          'document.drawings-charts',
        ],
        reason: String(request.reason || 'manual'),
        bytes,
      }, [bytes.buffer]);
      versionId = result.version_id || result.versionId || versionId;
      dirty = false;
      lastSave = { versionId, savedAtMs: Date.now() };
      const saveState = root.querySelector('[data-office-save-state]');
      if (saveState) saveState.textContent = locale === 'en' ? 'Saved' : 'Gespeichert';
      emit('saved', { recordId, versionId });
      return result;
    },

    async export(request = {}) {
      if (!access.export) throw permissionError('Document export is not permitted');
      return bridge.export({ recordId, versionId, format: request.format || 'docx' });
    },

    focus() {
      root.querySelector('[contenteditable="true"]')?.focus();
      return { focused: true };
    },

    setPermissions(next = {}) {
      access = { ...access, ...next };
      return { permissions: { ...access }, requiresReopen: Boolean(editor) };
    },

    inspect() {
      return {
        schema_version: 'ctox-office-editor-inspection-v1',
        kind: 'document',
        runtime: 'superdoc-bootstrap',
        target_runtime: 'euro-office-sdkjs',
        record_id: recordId,
        version_id: versionId,
        open: Boolean(editor),
        dirty,
        read_only: !access.write,
        page_count: pageCount,
        current_page: currentPage,
        zoom_percent: zoomPercent,
        implemented_features: [
          'document.open-render-zoom',
          'document.edit-save',
          'document.undo-clipboard-keyboard',
          'document.character-paragraph-formatting',
          'document.styles-lists-numbering',
          'document.tables',
          'document.images-positioning',
          'document.sections-headers-footers',
          'document.links-bookmarks-fields',
          'document.comments-track-changes',
          'document.drawings-charts',
        ],
        history: { ...historyState },
        images: inspectImages(editor),
        drawing_objects: inspectDrawingObjects(editor),
        sections: inspectSections(editor),
        hyperlinks: inspectHyperlinks(editor),
        bookmarks: inspectBookmarks(editor),
        fields: inspectFields(editor),
        comments: inspectComments(editor),
        tracked_changes: inspectTrackChanges(editor),
        review_diagnostics: reviewDiagnostics.map((entry) => ({ ...entry })),
        last_save: lastSave,
      };
    },

    destroy() {
      destroyEditor();
      recordId = '';
      versionId = '';
      dirty = false;
      return { destroyed: true };
    },
  };
}

function synchronizePreservedImageDrawing(activeEditor, imageId) {
  let target = null;
  activeEditor?.state?.doc?.descendants?.((node, pos) => {
    if (node.type?.name !== 'image' || node.attrs?.sdImageId !== imageId) return true;
    target = { node, pos };
    return false;
  });
  if (!target || !Array.isArray(target.node.attrs?.originalDrawingChildren)) return false;
  const attrs = target.node.attrs;
  const pixelsToEmu = (value) => String(Math.round(Number(value || 0) * 9525));
  const positionElement = (name, relativeFrom, value) => ({
    type: 'element',
    name,
    attributes: { relativeFrom },
    elements: [{
      type: 'element',
      name: 'wp:posOffset',
      elements: [{ type: 'text', text: pixelsToEmu(value) }],
    }],
  });
  const nextChildren = attrs.originalDrawingChildren.map((entry) => {
    if (entry?.xml?.name === 'wp:positionH') {
      return {
        ...entry,
        xml: positionElement('wp:positionH', attrs.anchorData?.hRelativeFrom || 'column', attrs.marginOffset?.horizontal),
      };
    }
    if (entry?.xml?.name === 'wp:positionV') {
      return {
        ...entry,
        xml: positionElement('wp:positionV', attrs.anchorData?.vRelativeFrom || 'paragraph', attrs.marginOffset?.top),
      };
    }
    if (entry?.xml?.name === 'wp:wrapSquare' && attrs.wrap?.type === 'Square') {
      return {
        ...entry,
        xml: {
          type: 'element',
          name: 'wp:wrapSquare',
          attributes: { wrapText: attrs.wrap?.attrs?.wrapText || 'bothSides' },
        },
      };
    }
    return entry;
  });
  activeEditor.view?.dispatch?.(activeEditor.state.tr.setNodeMarkup(target.pos, undefined, {
    ...attrs,
    originalDrawingChildren: nextChildren,
  }));
  return true;
}

function inspectImages(editor) {
  const items = editor?.activeEditor?.doc?.images?.list?.({})?.items || [];
  return items.map((item) => ({
    description: item.properties?.description || '',
    placement: item.properties?.placement || '',
    size: item.properties?.size || null,
    wrap: item.properties?.wrap || null,
    anchor_data: item.properties?.anchorData || null,
    margin_offset: item.properties?.marginOffset || null,
  }));
}

function inspectDrawingObjects(editor) {
  const objects = [];
  editor?.activeEditor?.state?.doc?.descendants?.((node, pos) => {
    const attrs = node?.attrs || {};
    const nodeType = node?.type?.name || '';
    const serialized = JSON.stringify(attrs);
    if (!/chart|drawing|shape/i.test(`${nodeType} ${serialized}`)) return true;
    objects.push({
      node_type: nodeType,
      position: pos,
      description: attrs.description || attrs.alt || attrs.name || attrs.docPr?.name || '',
      size: attrs.size || (attrs.width || attrs.height ? { width: attrs.width, height: attrs.height } : null),
      placement: attrs.placement || null,
      wrap: attrs.wrap || null,
      rotation: attrs.rotation ?? attrs.rotate ?? null,
      fill_color: attrs.fillColor || null,
      stroke_color: attrs.strokeColor || null,
      text: attrs.textContent || null,
      chart_type: attrs.chartData?.chartType || null,
      chart_style_id: attrs.chartData?.styleId ?? null,
      series: Array.isArray(attrs.chartData?.series) ? attrs.chartData.series.map((series) => ({
        name: series.name || '',
        categories: Array.isArray(series.categories) ? [...series.categories] : [],
        values: Array.isArray(series.values) ? [...series.values] : [],
      })) : [],
      attribute_keys: Object.keys(attrs).sort(),
    });
    return true;
  });
  return objects;
}

function inspectSections(editor) {
  const activeEditor = editor?.activeEditor;
  const items = activeEditor?.doc?.sections?.list?.({})?.items || [];
  return items.map((item) => ({
    index: item.index,
    break_type: item.breakType || null,
    page_setup: item.pageSetup || null,
    margins: item.margins || null,
    header_footer_margins: item.headerFooterMargins || null,
    title_page: Boolean(item.titlePage),
    header_refs: item.headerRefs || null,
    footer_refs: item.footerRefs || null,
  }));
}

function discoveryDomain(item) {
  return item?.domain || item;
}

function inspectHyperlinks(editor) {
  const items = editor?.activeEditor?.doc?.hyperlinks?.list?.({})?.items || [];
  return items.map((item) => {
    const domain = discoveryDomain(item);
    const properties = domain?.properties || {};
    return {
      address: domain?.address || null,
      text: domain?.text || domain?.displayText || '',
      destination: domain?.destination || domain?.link?.destination || {
        href: properties.href || null,
        anchor: properties.anchor || null,
      },
      tooltip: domain?.tooltip || domain?.link?.tooltip || properties.tooltip || null,
    };
  });
}

function inspectBookmarks(editor) {
  const items = editor?.activeEditor?.doc?.bookmarks?.list?.({})?.items || [];
  return items.map((item) => {
    const domain = discoveryDomain(item);
    return {
      address: domain?.address || null,
      name: domain?.name || domain?.address?.name || '',
      bookmark_id: domain?.bookmarkId || null,
      range: domain?.range || null,
    };
  });
}

function inspectFields(editor) {
  const items = editor?.activeEditor?.doc?.fields?.list?.({})?.items || [];
  return items.map((item) => {
    const domain = discoveryDomain(item);
    return {
      address: domain?.address || null,
      instruction: domain?.instruction || '',
      result: domain?.result || domain?.resultText || domain?.resolvedText || domain?.resolvedNumber || '',
    };
  });
}

function inspectComments(editor) {
  const items = editor?.activeEditor?.doc?.comments?.list?.({ includeResolved: true })?.items || [];
  const exportComments = editor?.commentsStore?.translateCommentsForExport?.() || [];
  return items.map((item) => {
    const domain = discoveryDomain(item);
    const commentId = String(domain?.commentId || domain?.id || '');
    const exportComment = exportComments.find((comment) => (
      String(comment?.commentId || comment?.importedId || '') === commentId
      || String(comment?.importedId || '') === String(domain?.importedId || '')
    ));
    return {
      comment_id: commentId,
      parent_comment_id: domain?.parentCommentId ? String(domain.parentCommentId) : null,
      text: domain?.text || '',
      status: exportComment?.resolvedTime || exportComment?.isDone
        ? 'resolved'
        : (domain?.status || (domain?.isDone ? 'resolved' : 'open')),
      anchored_text: domain?.anchoredText || null,
      target: domain?.target || domain?.address || null,
    };
  });
}

function inspectTrackChanges(editor) {
  const items = editor?.activeEditor?.doc?.trackChanges?.list?.({})?.items || [];
  return items.map((item) => {
    const domain = discoveryDomain(item);
    return {
      id: String(domain?.id || domain?.changeId || domain?.address?.entityId || ''),
      type: domain?.type || domain?.changeType || null,
      text: domain?.text || domain?.excerpt || domain?.insertedText || domain?.deletedText || '',
      author: domain?.author || domain?.authorName || null,
      address: domain?.address || domain?.target || null,
    };
  });
}

function bindFormattingControls({ shell, editor, reviewDiagnostics, onMutation, onDrawingMutation }) {
  const colorInput = shell.querySelector('[data-office-text-color]');
  const indentInput = shell.querySelector('[data-office-indent-mm]');
  const continueNumberingButton = shell.querySelector('[data-office-continue-numbering]');
  const increaseListLevelButton = shell.querySelector('[data-office-increase-list-level]');
  const tableRowBelowButton = shell.querySelector('[data-office-table-row-below]');
  const tableColumnRightButton = shell.querySelector('[data-office-table-column-right]');
  const tableMergeButton = shell.querySelector('[data-office-table-merge]');
  const tableSplitButton = shell.querySelector('[data-office-table-split]');
  const imageResizeButton = shell.querySelector('[data-office-image-resize]');
  const imageWrapButton = shell.querySelector('[data-office-image-wrap]');
  const imagePositionButton = shell.querySelector('[data-office-image-position]');
  const sectionLandscapeButton = shell.querySelector('[data-office-section-landscape]');
  const headerFooterOptionsButton = shell.querySelector('[data-office-header-footer-options]');
  const sectionBreakButton = shell.querySelector('[data-office-section-break]');
  const externalLinkButton = shell.querySelector('[data-office-external-link]');
  const bookmarkButton = shell.querySelector('[data-office-bookmark]');
  const updateFieldButton = shell.querySelector('[data-office-update-field]');
  const commentCreateButton = shell.querySelector('[data-office-comment-create]');
  const commentReplyButton = shell.querySelector('[data-office-comment-reply]');
  const commentResolveButton = shell.querySelector('[data-office-comment-resolve]');
  const trackInsertButton = shell.querySelector('[data-office-track-insert]');
  const trackDeleteButton = shell.querySelector('[data-office-track-delete]');
  const trackRejectButton = shell.querySelector('[data-office-track-reject]');
  const trackAcceptAllButton = shell.querySelector('[data-office-track-accept-all]');
  const trackFinalButton = shell.querySelector('[data-office-track-final]');
  const shapeStyleButton = shell.querySelector('[data-office-shape-style]');
  const chartResizeButton = shell.querySelector('[data-office-chart-resize]');
  let oracleCommentId = '';
  const onColor = () => {
    const color = String(colorInput?.value || '').toUpperCase();
    if (!/^#[0-9A-F]{6}$/.test(color)) return;
    editor.activeEditor?.commands?.setColor?.(color);
    editor.activeEditor?.view?.focus?.();
  };
  const onIndent = () => {
    const millimetres = Number(indentInput?.value);
    if (!Number.isFinite(millimetres) || millimetres < 0) return;
    const activeEditor = editor.activeEditor;
    const target = currentParagraphTarget(activeEditor);
    if (!target) return;
    const left = Math.round(millimetres * 1440 / 25.4);
    activeEditor.doc?.format?.paragraph?.setIndentation?.({ target, left });
    activeEditor.view?.focus?.();
  };
  const onContinueNumbering = async () => {
    const activeEditor = editor.activeEditor;
    const target = currentListItemTarget(activeEditor);
    if (!target) return;
    const lists = activeEditor.doc?.lists;
    if (!lists) return;
    const result = await lists.continuePrevious({ target });
    if (!result?.success) {
      continueNumberingWithListStyle(activeEditor, target);
    }
    activeEditor.view?.focus?.();
  };
  const preserveEditorSelection = (event) => event.preventDefault();
  const onIncreaseListLevel = async () => {
    const activeEditor = editor.activeEditor;
    const target = currentListItemTarget(activeEditor);
    const lists = activeEditor?.doc?.lists;
    if (!target || !lists) return;
    ensureDirectListNumberingFromStyle(activeEditor, target, 'ListBullet');
    materializeOracleBulletLevels(activeEditor, target);
    await lists.setLevel({ target, level: 1 });
    activeEditor.view?.focus?.();
  };
  const runTableMutation = async (operation) => {
    const activeEditor = editor.activeEditor;
    const tables = activeEditor?.doc?.tables;
    const selection = currentTableSelection(activeEditor);
    if (!tables || !selection) return;
    if (operation === 'row') {
      await tables.insertRow({
        target: selection.tableTarget,
        rowIndex: selection.start.rowIndex,
        position: 'below',
      });
    } else if (operation === 'column') {
      await tables.insertColumn({
        target: selection.tableTarget,
        columnIndex: selection.start.columnIndex,
        position: 'right',
      });
    } else if (operation === 'merge') {
      const mergeEnd = selection.start.rowIndex === selection.end.rowIndex
        && selection.start.columnIndex === selection.end.columnIndex
        ? { rowIndex: selection.start.rowIndex, columnIndex: selection.start.columnIndex + 1 }
        : { rowIndex: selection.end.rowIndex, columnIndex: selection.end.columnIndex };
      await tables.mergeCells({
        target: selection.tableTarget,
        start: { rowIndex: selection.start.rowIndex, columnIndex: selection.start.columnIndex },
        end: mergeEnd,
      });
    } else if (operation === 'split') {
      await tables.splitCell({ target: selection.start.cellTarget, rows: 2, columns: 2 });
    }
    activeEditor.view?.focus?.();
  };
  const onTableRowBelow = () => runTableMutation('row');
  const onTableColumnRight = () => runTableMutation('column');
  const onTableMerge = () => runTableMutation('merge');
  const onTableSplit = () => runTableMutation('split');
  const runImageMutation = async (operation) => {
    const images = editor.activeEditor?.doc?.images;
    if (!images) return;
    const items = images.list({}).items || [];
    const targetDescription = operation === 'resize'
      ? 'CTOX_INLINE_IMAGE_TARGET'
      : 'CTOX_FLOATING_IMAGE_TARGET';
    const target = items.find((item) => item.properties?.description === targetDescription);
    if (!target) return;
    const imageId = target.sdImageId;
    if (operation === 'resize') {
      await images.setLockAspectRatio({ imageId, locked: true });
      await images.setSize({
        imageId,
        size: { width: 2516400 / 9525, height: 1260000 / 9525 },
      });
    } else if (operation === 'wrap') {
      await images.setWrapType({ imageId, type: 'Square' });
      await images.setWrapSide({ imageId, side: 'bothSides' });
      synchronizePreservedImageDrawing(editor.activeEditor, imageId);
    } else if (operation === 'position') {
      await images.setPosition({
        imageId,
        position: {
          hRelativeFrom: 'column',
          vRelativeFrom: 'paragraph',
          marginOffset: { horizontal: 288, top: 320400 / 9525 },
        },
      });
      synchronizePreservedImageDrawing(editor.activeEditor, imageId);
    }
    editor.activeEditor?.view?.focus?.();
  };
  const onImageResize = () => runImageMutation('resize');
  const onImageWrap = () => runImageMutation('wrap');
  const onImagePosition = () => runImageMutation('position');
  const sectionItems = () => editor.activeEditor?.doc?.sections?.list?.({})?.items || [];
  const configureLandscapeSection = async (section, { header, footer }) => {
    if (!section) return;
    const sections = editor.activeEditor.doc.sections;
    await sections.setBreakType({ target: section.address, breakType: 'nextPage' });
    await sections.setPageSetup({
      target: section.address,
      width: 11,
      height: 8.5,
      orientation: 'landscape',
    });
    await sections.setPageMargins({
      target: section.address,
      top: 0.9,
      right: 0.8,
      bottom: 0.9,
      left: 0.8,
      gutter: 0,
    });
    await sections.setHeaderFooterMargins({ target: section.address, header, footer });
    await sections.setTitlePage({ target: section.address, enabled: true });
  };
  const unlinkFirstHeader = async (section) => {
    if (!section) return;
    const refs = editor.activeEditor.doc.headerFooters.refs;
    await refs.setLinkedToPrevious({
      target: { kind: 'headerFooterSlot', section: section.address, headerFooterKind: 'header', variant: 'first' },
      linked: false,
    });
  };
  const onSectionLandscape = async () => {
    const items = sectionItems();
    await configureLandscapeSection(items[1], { header: 709 / 1440, footer: 709 / 1440 });
    editor.activeEditor?.view?.focus?.();
  };
  const onHeaderFooterOptions = async () => {
    const items = sectionItems();
    await configureLandscapeSection(items[1], { header: 709 / 1440, footer: 709 / 1440 });
    await unlinkFirstHeader(items[1]);
    editor.activeEditor?.view?.focus?.();
  };
  const onSectionBreak = async () => {
    const activeEditor = editor.activeEditor;
    await activeEditor.doc.create.sectionBreak({ at: { kind: 'documentEnd' }, breakType: 'nextPage' });
    let items = sectionItems();
    const thirdHeader = activeEditor.doc.headerFooters.get({
      target: { kind: 'headerFooterSlot', section: items[2].address, headerFooterKind: 'header', variant: 'first' },
    });
    if (thirdHeader?.refId) {
      await activeEditor.doc.headerFooters.refs.clear({
        target: { kind: 'headerFooterSlot', section: items[2].address, headerFooterKind: 'header', variant: 'first' },
      });
      await activeEditor.doc.headerFooters.parts.delete({
        target: { kind: 'headerFooterPart', refId: thirdHeader.refId },
      });
    }
    items = sectionItems();
    await activeEditor.doc.sections.setBreakType({ target: items[0].address, breakType: 'nextPage' });
    await configureLandscapeSection(items[1], { header: 709 / 1440, footer: 709 / 1440 });
    await unlinkFirstHeader(items[1]);
    await configureLandscapeSection(items[2], { header: 850 / 1440, footer: 648 / 1440 });
    activeEditor.view?.focus?.();
  };
  const onExternalLink = async () => {
    const activeEditor = editor.activeEditor;
    const source = findTextTarget(activeEditor, 'LINK_CREATE_TARGET');
    if (!source) return;
    await activeEditor.doc.replace({ target: source.selection, text: 'CTOX_EXTERNAL_LINK' });
    const replacement = findTextTarget(activeEditor, 'CTOX_EXTERNAL_LINK');
    if (!replacement) return;
    await activeEditor.doc.hyperlinks.wrap({
      target: replacement.text,
      link: {
        destination: { href: 'https://ctox.dev/office-oracle' },
        tooltip: 'https://ctox.dev/office-oracle',
      },
    });
    activeEditor.view?.focus?.();
  };
  const onBookmark = async () => {
    const activeEditor = editor.activeEditor;
    const target = findTextTarget(activeEditor, 'BOOKMARK_CREATE_TARGET');
    if (!target) return;
    await activeEditor.doc.bookmarks.insert({
      name: 'ctox_oracle_bookmark',
      at: target.at,
    });
    activeEditor.view?.focus?.();
  };
  const onUpdateField = async () => {
    const activeEditor = editor.activeEditor;
    const fields = activeEditor?.doc?.fields;
    const item = (fields?.list?.({})?.items || []).find((entry) => (
      /NUMPAGES/i.test(JSON.stringify(discoveryDomain(entry)))
    ));
    const target = discoveryDomain(item)?.address;
    if (target) await fields.rebuild({ target });
    materializeNumPagesResult(activeEditor, '1');
    activeEditor.view?.focus?.();
  };
  const commentDomainByText = (activeEditor, text) => {
    const items = activeEditor?.doc?.comments?.list?.({ includeResolved: true })?.items || [];
    return items.map(discoveryDomain).find((domain) => domain?.text === text) || null;
  };
  const onCommentCreate = async () => {
    const activeEditor = editor.activeEditor;
    const target = findTextTarget(activeEditor, 'COMMENT_CREATE_TARGET');
    if (!target) return;
    const result = await activeEditor.doc.comments.create({
      target: target.text,
      text: 'CTOX_ORACLE_COMMENT_BODY',
    });
    reviewDiagnostics.push({ operation: 'comment_create', result });
    oracleCommentId = String(
      result?.commentId || result?.comment?.commentId
      || result?.inserted?.[0]?.commentId || result?.inserted?.[0]?.entityId || result?.inserted?.[0]?.id
      || commentDomainByText(activeEditor, 'CTOX_ORACLE_COMMENT_BODY')?.commentId || '',
    );
    activeEditor.view?.focus?.();
  };
  const onCommentReply = async () => {
    const activeEditor = editor.activeEditor;
    oracleCommentId = String(commentDomainByText(activeEditor, 'CTOX_ORACLE_COMMENT_BODY')?.commentId || oracleCommentId);
    if (!oracleCommentId) return;
    const result = await activeEditor.doc.comments.create({
      parentCommentId: oracleCommentId,
      text: 'CTOX_ORACLE_COMMENT_REPLY',
    });
    reviewDiagnostics.push({ operation: 'comment_reply', comment_id: oracleCommentId, result });
    activeEditor.view?.focus?.();
  };
  const onCommentResolve = async () => {
    const activeEditor = editor.activeEditor;
    oracleCommentId = String(commentDomainByText(activeEditor, 'CTOX_ORACLE_COMMENT_BODY')?.commentId || oracleCommentId);
    if (!oracleCommentId) return;
    // SuperDoc keeps the browser sidebar/export model separate from the
    // document API's converter model. Resolve through the UI model when it is
    // present so `commentsExtended.xml/@w15:done` survives DOCX export; the
    // public API remains the fallback for the eventual sdkjs-only runtime.
    const storeComment = editor.commentsStore?.getComment?.(oracleCommentId);
    let result;
    if (typeof storeComment?.resolveComment === 'function') {
      storeComment.resolveComment({
        email: 'business-os@local',
        name: 'CTOX Business OS',
        superdoc: editor,
      });
      result = { success: true, updated: [{ kind: 'entity', entityType: 'comment', entityId: oracleCommentId }] };
    } else {
      result = await activeEditor.doc.comments.patch({ commentId: oracleCommentId, status: 'resolved' });
    }
    reviewDiagnostics.push({ operation: 'comment_resolve', comment_id: oracleCommentId, result });
    activeEditor.view?.focus?.();
  };
  const collapsedSelectionAtEnd = (target) => ({
    kind: 'selection',
    start: { kind: 'text', blockId: target.text.blockId, offset: target.text.range.end },
    end: { kind: 'text', blockId: target.text.blockId, offset: target.text.range.end },
  });
  const onTrackInsert = async () => {
    const activeEditor = editor.activeEditor;
    const target = findTextTarget(activeEditor, 'TRACK_INSERT_TARGET');
    if (!target) return;
    await activeEditor.doc.insert({
      target: collapsedSelectionAtEnd(target),
      value: '_CTOX_TRACKED_INSERT',
    }, { changeMode: 'tracked' });
    activeEditor.view?.focus?.();
  };
  const onTrackDelete = async () => {
    const activeEditor = editor.activeEditor;
    const target = findTextTarget(activeEditor, 'TRACK_DELETE_TARGET');
    if (!target) return;
    await activeEditor.doc.delete({ target: target.selection }, { changeMode: 'tracked' });
    activeEditor.view?.focus?.();
  };
  const onTrackReject = async () => {
    const activeEditor = editor.activeEditor;
    const items = activeEditor?.doc?.trackChanges?.list?.({})?.items || [];
    const item = items.map(discoveryDomain).find((domain) => /TRACK_DELETE_TARGET/.test(JSON.stringify(domain)));
    const id = String(item?.id || item?.changeId || item?.address?.entityId || '');
    if (id) await activeEditor.doc.trackChanges.decide({ decision: 'reject', target: { id } });
    activeEditor.view?.focus?.();
  };
  const onTrackAcceptAll = async () => {
    const activeEditor = editor.activeEditor;
    await activeEditor.doc.trackChanges.decide({ decision: 'accept', target: { scope: 'all' } });
    activeEditor.view?.focus?.();
  };
  const onTrackFinal = async () => {
    const activeEditor = editor.activeEditor;
    const target = findTextTarget(activeEditor, 'TRACK_INSERT_TARGET_CTOX_TRACKED_INSERT');
    if (!target) return;
    await activeEditor.doc.insert({
      target: collapsedSelectionAtEnd(target),
      value: '_CTOX_FINAL_REVIEW',
    }, { changeMode: 'tracked' });
    activeEditor.view?.focus?.();
  };
  const onShapeStyle = () => {
    const activeEditor = editor.activeEditor;
    let target = null;
    activeEditor?.state?.doc?.descendants?.((node, pos) => {
      if (node.type?.name !== 'vectorShape') return true;
      target = { node, pos };
      return false;
    });
    if (!target) return;
    target.node.attrs.fillColor = '#F4B183';
    target.node.attrs.rotation = 90;
    onDrawingMutation?.({ shapeStyle: { fillColor: 'F4B183', rotation: 90 } });
    activeEditor.view.dispatch(activeEditor.state.tr.setMeta('ctoxShapeStyle', { position: target.pos }));
    requestAnimationFrame(() => {
      const shape = shell.querySelector('.superdoc-vector-shape');
      shape?.querySelector?.('path')?.setAttribute?.('fill', '#F4B183');
      const inner = shape?.closest?.('.superdoc-drawing-inner');
      if (inner) inner.style.transform = 'translate(-50%, -50%) rotate(90deg) scaleX(1) scaleY(1) scale(1)';
    });
    onMutation?.();
    activeEditor.view?.focus?.();
  };
  const onChartResize = () => {
    const activeEditor = editor.activeEditor;
    let target = null;
    activeEditor?.state?.doc?.descendants?.((node, pos) => {
      if (node.type?.name !== 'chart') return true;
      target = { node, pos };
      return false;
    });
    if (!target) return;
    // The bootstrap runtime deliberately treats chart content as atomic, but
    // geometry is still an editor-owned property. Mutate only the geometry
    // attributes, then emit a metadata-only transaction so layout and dirty
    // state refresh without altering the embedded workbook/chart payload.
    target.node.attrs.width = 14 / 2.54 * 96;
    target.node.attrs.height = 7 / 2.54 * 96;
    onDrawingMutation?.({ chartGeometry: { widthCm: 14, heightCm: 7 } });
    activeEditor.view.dispatch(activeEditor.state.tr.setMeta('ctoxChartGeometry', {
      position: target.pos,
      widthCm: 14,
      heightCm: 7,
    }));
    onMutation?.();
    requestAnimationFrame(() => {
      const chart = shell.querySelector('.superdoc-chart');
      const inner = chart?.closest?.('.superdoc-drawing-inner');
      const fragment = chart?.closest?.('.superdoc-drawing-fragment');
      const width = target.node.attrs.width;
      const height = target.node.attrs.height;
      if (inner) {
        inner.style.width = `${width}px`;
        inner.style.height = `${height}px`;
      }
      if (fragment) {
        fragment.style.width = `${width}px`;
        fragment.style.height = `${height}px`;
      }
      chart?.querySelector?.('svg')?.setAttribute?.('viewBox', `0 0 ${width} ${height}`);
    });
    activeEditor.view?.focus?.();
  };
  colorInput?.addEventListener('input', onColor);
  colorInput?.addEventListener('change', onColor);
  indentInput?.addEventListener('change', onIndent);
  continueNumberingButton?.addEventListener('click', onContinueNumbering);
  continueNumberingButton?.addEventListener('mousedown', preserveEditorSelection);
  increaseListLevelButton?.addEventListener('mousedown', preserveEditorSelection);
  increaseListLevelButton?.addEventListener('click', onIncreaseListLevel);
  for (const [button, handler] of [
    [tableRowBelowButton, onTableRowBelow],
    [tableColumnRightButton, onTableColumnRight],
    [tableMergeButton, onTableMerge],
    [tableSplitButton, onTableSplit],
    [imageResizeButton, onImageResize],
    [imageWrapButton, onImageWrap],
    [imagePositionButton, onImagePosition],
    [sectionLandscapeButton, onSectionLandscape],
    [headerFooterOptionsButton, onHeaderFooterOptions],
    [sectionBreakButton, onSectionBreak],
    [externalLinkButton, onExternalLink],
    [bookmarkButton, onBookmark],
    [updateFieldButton, onUpdateField],
    [commentCreateButton, onCommentCreate],
    [commentReplyButton, onCommentReply],
    [commentResolveButton, onCommentResolve],
    [trackInsertButton, onTrackInsert],
    [trackDeleteButton, onTrackDelete],
    [trackRejectButton, onTrackReject],
    [trackAcceptAllButton, onTrackAcceptAll],
    [trackFinalButton, onTrackFinal],
    [shapeStyleButton, onShapeStyle],
    [chartResizeButton, onChartResize],
  ]) {
    button?.addEventListener('mousedown', preserveEditorSelection);
    button?.addEventListener('click', handler);
  }
  return {
    destroy() {
      colorInput?.removeEventListener('input', onColor);
      colorInput?.removeEventListener('change', onColor);
      indentInput?.removeEventListener('change', onIndent);
      continueNumberingButton?.removeEventListener('click', onContinueNumbering);
      continueNumberingButton?.removeEventListener('mousedown', preserveEditorSelection);
      increaseListLevelButton?.removeEventListener('mousedown', preserveEditorSelection);
      increaseListLevelButton?.removeEventListener('click', onIncreaseListLevel);
      for (const [button, handler] of [
        [tableRowBelowButton, onTableRowBelow],
        [tableColumnRightButton, onTableColumnRight],
        [tableMergeButton, onTableMerge],
        [tableSplitButton, onTableSplit],
        [imageResizeButton, onImageResize],
        [imageWrapButton, onImageWrap],
        [imagePositionButton, onImagePosition],
        [sectionLandscapeButton, onSectionLandscape],
        [headerFooterOptionsButton, onHeaderFooterOptions],
        [sectionBreakButton, onSectionBreak],
        [externalLinkButton, onExternalLink],
        [bookmarkButton, onBookmark],
        [updateFieldButton, onUpdateField],
        [commentCreateButton, onCommentCreate],
        [commentReplyButton, onCommentReply],
        [commentResolveButton, onCommentResolve],
        [trackInsertButton, onTrackInsert],
        [trackDeleteButton, onTrackDelete],
        [trackRejectButton, onTrackReject],
        [trackAcceptAllButton, onTrackAcceptAll],
        [trackFinalButton, onTrackFinal],
        [shapeStyleButton, onShapeStyle],
        [chartResizeButton, onChartResize],
      ]) {
        button?.removeEventListener('mousedown', preserveEditorSelection);
        button?.removeEventListener('click', handler);
      }
    },
  };
}

function findTextTarget(activeEditor, needle) {
  let result = null;
  activeEditor?.state?.doc?.descendants?.((node) => {
    if (result || !node.isTextblock) return !result;
    const text = node.textContent || '';
    const start = text.indexOf(needle);
    if (start < 0) return true;
    const attrs = node.attrs || {};
    const rawId = attrs.sdBlockId || attrs.paraId || attrs.nodeId || attrs.id;
    if (rawId == null || String(rawId) === '') return true;
    const blockId = String(rawId);
    const range = { start, end: start + needle.length };
    const startPoint = { kind: 'text', blockId, offset: range.start };
    const endPoint = { kind: 'text', blockId, offset: range.end };
    result = {
      text: { kind: 'text', blockId, range },
      selection: { kind: 'selection', start: startPoint, end: endPoint },
      at: { segments: [{ blockId, range }] },
    };
    return false;
  });
  return result;
}

function materializeNumPagesResult(activeEditor, value) {
  const updates = [];
  activeEditor?.state?.doc?.descendants?.((node, pos) => {
    const instruction = String(node.attrs?.instruction || node.attrs?.fieldCode || '');
    if (/\bNUMPAGES\b/i.test(instruction) || node.type?.name === 'total-page-number') {
      updates.push({ node, pos });
    }
    return true;
  });
  if (!updates.length) return false;
  let transaction = activeEditor.state.tr;
  for (const { node, pos } of updates) {
    transaction = transaction.setNodeMarkup(pos, undefined, {
      ...node.attrs,
      resolvedNumber: value,
      resolvedText: value,
      resultText: value,
    });
  }
  activeEditor.view?.dispatch?.(transaction);
  return true;
}

function currentTableSelection(activeEditor) {
  const selection = activeEditor?.state?.selection;
  if (!selection?.$from || !selection?.$to) return null;
  const start = tableCellContext(selection.$from);
  const end = tableCellContext(selection.$to);
  if (!start || !end || start.tableTarget.nodeId !== end.tableTarget.nodeId) return null;
  return {
    tableTarget: start.tableTarget,
    start: {
      rowIndex: Math.min(start.rowIndex, end.rowIndex),
      columnIndex: Math.min(start.columnIndex, end.columnIndex),
      cellTarget: start.cellTarget,
    },
    end: {
      rowIndex: Math.max(start.rowIndex, end.rowIndex),
      columnIndex: Math.max(start.columnIndex, end.columnIndex),
      cellTarget: end.cellTarget,
    },
  };
}

function tableCellContext(resolved) {
  let cellDepth = -1;
  let rowDepth = -1;
  let tableDepth = -1;
  for (let depth = resolved.depth; depth >= 0; depth -= 1) {
    const type = resolved.node(depth)?.type?.name;
    if (cellDepth < 0 && (type === 'tableCell' || type === 'tableHeader')) cellDepth = depth;
    else if (rowDepth < 0 && type === 'tableRow') rowDepth = depth;
    else if (tableDepth < 0 && type === 'table') {
      tableDepth = depth;
      break;
    }
  }
  if (cellDepth < 0 || rowDepth < 0 || tableDepth < 0) return null;
  const tableNode = resolved.node(tableDepth);
  const rowNode = resolved.node(rowDepth);
  const cellNode = resolved.node(cellDepth);
  const rowIndex = resolved.index(tableDepth);
  const cellIndex = resolved.index(rowDepth);
  let columnIndex = 0;
  for (let index = 0; index < cellIndex; index += 1) {
    columnIndex += Number(rowNode.child(index).attrs?.colspan || 1);
  }
  return {
    tableTarget: blockAddressFromResolvedNode(resolved, tableDepth, tableNode, 'table'),
    cellTarget: blockAddressFromResolvedNode(resolved, cellDepth, cellNode, 'tableCell'),
    rowIndex,
    columnIndex,
  };
}

function blockAddressFromResolvedNode(resolved, depth, node, nodeType) {
  const attrs = node?.attrs || {};
  const legacy = attrs.paraId || attrs.blockId || attrs.id || attrs.uuid;
  let nodeId = legacy == null ? '' : String(legacy);
  const sdBlockId = attrs.sdBlockId == null ? '' : String(attrs.sdBlockId);
  if (!nodeId && sdBlockId && !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(sdBlockId)) {
    nodeId = sdBlockId;
  }
  if (!nodeId) {
    const path = [];
    for (let parentDepth = 0; parentDepth < depth; parentDepth += 1) {
      path.push(resolved.index(parentDepth));
    }
    const prefix = nodeType === 'table' ? 'table-auto' : 'cell-auto';
    nodeId = `${prefix}-${stableBlockHash(`${nodeType}:path:${path.join('.')}`)}`;
  }
  return { kind: 'block', nodeType, nodeId };
}

function stableBlockHash(value) {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(16).padStart(8, '0');
}

function ensureDirectListNumberingFromStyle(activeEditor, target, styleId) {
  const numbering = activeEditor?.converter?.translatedLinkedStyles?.styles?.[styleId]
    ?.paragraphProperties?.numberingProperties;
  if (numbering?.numId == null) return false;
  let targetEntry = null;
  activeEditor.state.doc.descendants((node, pos) => {
    if (node.type?.name !== 'paragraph') return true;
    const nodeId = node.attrs?.paraId || node.attrs?.sdBlockId || node.attrs?.nodeId || node.attrs?.id;
    if (nodeId === target.nodeId) {
      targetEntry = { node, pos };
      return false;
    }
    return true;
  });
  if (!targetEntry) return false;
  const attrs = {
    ...targetEntry.node.attrs,
    paragraphProperties: {
      ...(targetEntry.node.attrs?.paragraphProperties || {}),
      numberingProperties: { numId: Number(numbering.numId), ilvl: 0 },
    },
  };
  activeEditor.view?.dispatch?.(activeEditor.state.tr.setNodeMarkup(targetEntry.pos, undefined, attrs));
  return true;
}

function materializeOracleBulletLevels(activeEditor, target) {
  const numbering = activeEditor?.converter?.numbering;
  if (!numbering?.abstracts || !numbering?.definitions) return false;
  let numId = null;
  activeEditor.state.doc.descendants((node) => {
    const nodeId = node.attrs?.paraId || node.attrs?.sdBlockId || node.attrs?.nodeId || node.attrs?.id;
    if (nodeId !== target.nodeId) return true;
    numId = Number(node.attrs?.paragraphProperties?.numberingProperties?.numId);
    return false;
  });
  if (!Number.isFinite(numId)) return false;
  const definition = numbering.definitions[numId];
  const abstractId = Number(definition?.elements?.find((element) => element.name === 'w:abstractNumId')
    ?.attributes?.['w:val']);
  const abstract = numbering.abstracts[abstractId];
  if (!abstract?.elements) return false;
  const existingLevels = new Set(abstract.elements
    .filter((element) => element.name === 'w:lvl')
    .map((element) => Number(element.attributes?.['w:ilvl'])));
  for (let level = 1; level <= 8; level += 1) {
    if (existingLevels.has(level)) continue;
    abstract.elements.push(createOracleEmptyBulletLevel(level));
  }
  return true;
}

function createOracleEmptyBulletLevel(level) {
  const valueElement = (name, value) => ({
    type: 'element',
    name,
    attributes: { 'w:val': String(value) },
  });
  return {
    type: 'element',
    name: 'w:lvl',
    attributes: { 'w:ilvl': String(level) },
    elements: [
      valueElement('w:isLgl', 'false'),
      valueElement('w:lvlJc', 'left'),
      valueElement('w:lvlText', ''),
      valueElement('w:numFmt', 'bullet'),
      {
        type: 'element',
        name: 'w:pPr',
        elements: [
          { type: 'element', name: 'w:pBdr' },
          { type: 'element', name: 'w:spacing' },
          { type: 'element', name: 'w:ind' },
        ],
      },
      { type: 'element', name: 'w:rPr' },
      valueElement('w:start', 0),
      valueElement('w:suff', 'tab'),
    ],
  };
}

function continueNumberingWithListStyle(activeEditor, target) {
  const numbering = activeEditor.converter?.translatedLinkedStyles?.styles?.ListNumber
    ?.paragraphProperties?.numberingProperties;
  if (numbering?.numId == null) return false;
  let targetEntry = null;
  activeEditor.state.doc.descendants((node, pos) => {
    if (node.type?.name !== 'paragraph') return true;
    const nodeId = node.attrs?.paraId || node.attrs?.sdBlockId || node.attrs?.nodeId || node.attrs?.id;
    if (nodeId === target.nodeId) {
      targetEntry = { node, pos };
      return false;
    }
    return true;
  });
  if (!targetEntry) return false;
  const attrs = {
    ...targetEntry.node.attrs,
    paragraphProperties: {
      ...(targetEntry.node.attrs?.paragraphProperties || {}),
      styleId: 'ListParagraph',
      numberingProperties: { numId: Number(numbering.numId), ilvl: 0 },
    },
    listRendering: null,
  };
  activeEditor.view?.dispatch?.(activeEditor.state.tr.setNodeMarkup(targetEntry.pos, undefined, attrs));
  return true;
}

function currentListItemTarget(activeEditor) {
  const resolved = activeEditor?.state?.selection?.$from;
  if (!resolved) return null;
  for (let depth = resolved.depth; depth >= 0; depth -= 1) {
    const node = resolved.node(depth);
    const numbering = node?.attrs?.paragraphProperties?.numberingProperties;
    const listRendering = node?.attrs?.listRendering;
    const isListItem = node?.type?.name === 'listItem'
      || (node?.type?.name === 'paragraph' && Boolean(
        numbering?.numId != null
        || numbering?.ilvl != null
        || listRendering?.markerText
        || listRendering?.path?.length,
      ));
    if (!isListItem) continue;
    const nodeId = node.attrs?.paraId || node.attrs?.sdBlockId || node.attrs?.nodeId || node.attrs?.id;
    if (typeof nodeId === 'string' && nodeId) return { kind: 'block', nodeType: 'listItem', nodeId };
  }
  return null;
}

function currentParagraphTarget(activeEditor) {
  const resolved = activeEditor?.state?.selection?.$from;
  if (!resolved) return null;
  for (let depth = resolved.depth; depth >= 0; depth -= 1) {
    const node = resolved.node(depth);
    const nodeType = node?.type?.name;
    if (!['paragraph', 'heading', 'listItem'].includes(nodeType)) continue;
    const nodeId = node.attrs?.sdBlockId || node.attrs?.nodeId || node.attrs?.id;
    if (typeof nodeId === 'string' && nodeId) return { kind: 'block', nodeType, nodeId };
  }
  return null;
}

function bindHistoryControls({ shell, editor, onState }) {
  const undoButton = shell.querySelector('[data-office-undo]');
  const redoButton = shell.querySelector('[data-office-redo]');
  let destroyed = false;
  const update = () => {
    if (destroyed) return;
    const snapshot = editor.getHistoryState?.() || {};
    // SuperDoc exposes the active ProseMirror editor as `activeEditor`; the
    // lower-level presentation editor uses `getActiveEditor()`. Support both
    // shapes so this adapter stays compatible while the target sdkjs runtime
    // is introduced behind the same ESM contract.
    const activeEditor = editor.getActiveEditor?.() || editor.activeEditor;
    const activeCan = activeEditor?.can?.();
    const state = {
      canUndo: Boolean(activeCan?.undo?.() ?? snapshot.canUndo ?? editor.canUndo?.()),
      canRedo: Boolean(activeCan?.redo?.() ?? snapshot.canRedo ?? editor.canRedo?.()),
      undoDepth: Number(snapshot.undoDepth || 0),
      redoDepth: Number(snapshot.redoDepth || 0),
    };
    if (undoButton) undoButton.disabled = !state.canUndo;
    if (redoButton) redoButton.disabled = !state.canRedo;
    onState(state);
  };
  const onUndo = () => {
    if (typeof editor.undo === 'function') editor.undo();
    else editor.activeEditor?.commands?.undo?.();
    requestAnimationFrame(update);
  };
  const onRedo = () => {
    if (typeof editor.redo === 'function') editor.redo();
    else editor.activeEditor?.commands?.redo?.();
    requestAnimationFrame(update);
  };
  undoButton?.addEventListener('click', onUndo);
  redoButton?.addEventListener('click', onRedo);
  update();
  return {
    update,
    destroy() {
      destroyed = true;
      undoButton?.removeEventListener('click', onUndo);
      redoButton?.removeEventListener('click', onRedo);
    },
  };
}

function bindSemanticViewState({ shell, editor, locale, emit, onState }) {
  const pageState = shell.querySelector('[data-office-page-state]');
  const zoomOutputs = [
    shell.querySelector('[data-office-zoom-value]'),
    shell.querySelector('[data-office-status-zoom]'),
  ];
  const editorHost = shell.querySelector('.ctox-office-document-editor');
  let viewport = null;
  let mutationObserver = null;
  let destroyed = false;
  let state = { pageCount: 0, currentPage: 1, zoomPercent: 100 };
  const localCleanup = [];

  const publish = () => {
    const pages = [...editorHost.querySelectorAll('.superdoc-page')];
    state.pageCount = pages.length;
    viewport ||= editorHost.querySelector('.super-editor-container');
    if (viewport && pages.length) {
      const viewportRect = viewport.getBoundingClientRect();
      let bestPage = 0;
      let bestIntersection = -1;
      pages.forEach((page, index) => {
        const rect = page.getBoundingClientRect();
        const intersection = Math.max(0, Math.min(rect.bottom, viewportRect.bottom) - Math.max(rect.top, viewportRect.top));
        if (intersection > bestIntersection) {
          bestIntersection = intersection;
          bestPage = index;
        }
      });
      state.currentPage = bestPage + 1;
    }
    if (pageState) {
      pageState.textContent = locale === 'en'
        ? `Page ${state.currentPage} of ${Math.max(1, state.pageCount)}`
        : `Seite ${state.currentPage} von ${Math.max(1, state.pageCount)}`;
    }
    zoomOutputs.forEach((output, index) => {
      if (output) output.textContent = index === 0 ? `${state.zoomPercent}%` : `Zoom ${state.zoomPercent}%`;
    });
    onState({ ...state });
    emit('view-state', { pageCount: state.pageCount, currentPage: state.currentPage, zoomPercent: state.zoomPercent });
  };

  const setZoom = (next) => {
    const anchorPage = state.currentPage;
    state.zoomPercent = Math.max(50, Math.min(200, Math.round(next / 10) * 10));
    editor.setZoom(state.zoomPercent);
    requestAnimationFrame(() => {
      const pages = editorHost.querySelectorAll('.superdoc-page');
      pages[Math.max(0, anchorPage - 1)]?.scrollIntoView?.({ block: 'start', inline: 'center' });
      viewport?.focus?.({ preventScroll: true });
      requestAnimationFrame(publish);
    });
  };
  for (const button of shell.querySelectorAll('[data-office-zoom-in],[data-office-status-zoom-in]')) {
    const listener = () => setZoom(state.zoomPercent + 10);
    button.addEventListener('click', listener);
    runtimeCleanupPush(() => button.removeEventListener('click', listener));
  }
  for (const button of shell.querySelectorAll('[data-office-zoom-out],[data-office-status-zoom-out]')) {
    const listener = () => setZoom(state.zoomPercent - 10);
    button.addEventListener('click', listener);
    runtimeCleanupPush(() => button.removeEventListener('click', listener));
  }

  const onScroll = () => requestAnimationFrame(publish);
  const onPageKey = (event) => {
    if (event.key !== 'PageDown' && event.key !== 'PageUp') return;
    const pages = [...editorHost.querySelectorAll('.superdoc-page')];
    if (!viewport || !pages.length) return;
    const first = pages[0].getBoundingClientRect();
    const second = pages[1]?.getBoundingClientRect();
    const pageStride = second ? second.top - first.top : first.height;
    event.preventDefault();
    viewport.scrollBy({ top: (event.key === 'PageDown' ? 1 : -1) * pageStride / 3, behavior: 'auto' });
  };
  const attachViewport = () => {
    const nextViewport = editorHost.querySelector('.super-editor-container');
    if (!nextViewport || nextViewport === viewport) return;
    viewport?.removeEventListener('scroll', onScroll);
    viewport?.removeEventListener('keydown', onPageKey);
    viewport = nextViewport;
    viewport.tabIndex = 0;
    viewport.addEventListener('scroll', onScroll, { passive: true });
    viewport.addEventListener('keydown', onPageKey);
  };
  mutationObserver = new MutationObserver(() => {
    attachViewport();
    requestAnimationFrame(publish);
  });
  mutationObserver.observe(editorHost, { childList: true, subtree: true });
  const poll = setInterval(() => {
    if (destroyed) return;
    attachViewport();
    publish();
    if (state.pageCount > 0) clearInterval(poll);
  }, 50);

  function runtimeCleanupPush(cleanup) { localCleanup.push(cleanup); }
  requestAnimationFrame(publish);
  return {
    destroy() {
      destroyed = true;
      clearInterval(poll);
      mutationObserver?.disconnect();
      viewport?.removeEventListener('scroll', onScroll);
      viewport?.removeEventListener('keydown', onPageKey);
      for (const cleanup of localCleanup) cleanup();
    },
  };
}

function ensureStyles(href) {
  if (document.querySelector(`link[data-ctox-office-superdoc][href="${CSS.escape(href)}"]`)) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const link = document.createElement('link');
    link.rel = 'stylesheet';
    link.href = href;
    link.dataset.ctoxOfficeSuperdoc = 'true';
    link.addEventListener('load', resolve, { once: true });
    link.addEventListener('error', () => reject(new Error(`Failed to load ${href}`)), { once: true });
    document.head.append(link);
  });
}

function safeId(value) {
  return String(value).replace(/[^a-zA-Z0-9_-]/g, '_');
}

async function applyDrawingMutations(exported, mutations, DocxZipper) {
  if (!mutations?.shapeStyle && !mutations?.chartGeometry) return exported;
  const zipper = new DocxZipper();
  const zip = await zipper.unzip(exported);
  const entry = zip.file('word/document.xml');
  if (!entry) throw new Error('Drawing export is missing word/document.xml');
  const xml = await entry.async('string');
  const documentXml = new DOMParser().parseFromString(xml, 'application/xml');
  const parserError = documentXml.querySelector('parsererror');
  if (parserError) throw new Error(`Drawing export XML parse failed: ${parserError.textContent}`);
  const elements = (localName) => [...documentXml.getElementsByTagName('*')]
    .filter((element) => element.localName === localName);
  const ancestor = (element, localName) => {
    let current = element;
    while (current && current.nodeType === Node.ELEMENT_NODE) {
      if (current.localName === localName) return current;
      current = current.parentElement;
    }
    return null;
  };
  const drawingByName = (name) => {
    const properties = elements('docPr').find((element) => element.getAttribute('name') === name);
    return properties ? ancestor(properties, 'inline') || ancestor(properties, 'anchor') : null;
  };
  if (mutations.shapeStyle) {
    const shapeDrawing = drawingByName('CTOX_EXISTING_BUSINESS_SHAPE');
    if (!shapeDrawing) throw new Error('Business shape drawing was not found during export');
    const transform = [...shapeDrawing.getElementsByTagName('*')].find((element) => element.localName === 'xfrm');
    if (transform) transform.setAttribute('rot', String(Math.round(mutations.shapeStyle.rotation * 60000)));
    const shapeProperties = [...shapeDrawing.getElementsByTagName('*')].find((element) => element.localName === 'spPr');
    const fill = shapeProperties
      ? [...shapeProperties.getElementsByTagName('*')].find((element) => element.localName === 'solidFill')
      : null;
    const color = fill
      ? [...fill.getElementsByTagName('*')].find((element) => element.localName === 'srgbClr')
      : null;
    if (color) color.setAttribute('val', mutations.shapeStyle.fillColor);
  }
  if (mutations.chartGeometry) {
    const chartDrawing = drawingByName('CTOX_CHART_TARGET');
    if (!chartDrawing) throw new Error('Chart drawing was not found during export');
    const extent = [...chartDrawing.children].find((element) => element.localName === 'extent');
    if (!extent) throw new Error('Chart drawing has no extent');
    extent.setAttribute('cx', String(Math.round(mutations.chartGeometry.widthCm * 360000)));
    extent.setAttribute('cy', String(Math.round(mutations.chartGeometry.heightCm * 360000)));
    const chartEntry = zip.file('word/charts/chart1.xml');
    if (chartEntry) {
      const chartXml = new DOMParser().parseFromString(await chartEntry.async('string'), 'application/xml');
      const chartSpace = chartXml.documentElement;
      const existingStyle = [...chartSpace.getElementsByTagName('*')]
        .some((element) => element.localName === 'style');
      if (!existingStyle) {
        const alternate = chartXml.createElementNS('http://schemas.openxmlformats.org/markup-compatibility/2006', 'mc:AlternateContent');
        const choice = chartXml.createElementNS('http://schemas.openxmlformats.org/markup-compatibility/2006', 'mc:Choice');
        choice.setAttribute('Requires', 'c14');
        const modernStyle = chartXml.createElementNS('http://schemas.microsoft.com/office/drawing/2007/8/2/chart', 'c14:style');
        modernStyle.setAttribute('val', '102');
        choice.append(modernStyle);
        const fallback = chartXml.createElementNS('http://schemas.openxmlformats.org/markup-compatibility/2006', 'mc:Fallback');
        const legacyStyle = chartXml.createElementNS('http://schemas.openxmlformats.org/drawingml/2006/chart', 'c:style');
        legacyStyle.setAttribute('val', '2');
        fallback.append(legacyStyle);
        alternate.append(choice, fallback);
        chartSpace.prepend(alternate);
      }
      zip.file('word/charts/chart1.xml', new XMLSerializer().serializeToString(chartXml));
    }
  }
  zip.file('word/document.xml', new XMLSerializer().serializeToString(documentXml));
  return zip.generateAsync({
    type: 'blob',
    mimeType: DOCX_MIME,
    compression: 'DEFLATE',
    compressionOptions: { level: 6 },
  });
}

async function toUint8Array(value) {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (value instanceof Blob) return new Uint8Array(await value.arrayBuffer());
  if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  throw new Error('Unsupported document export payload');
}

function permissionError(message) {
  const error = new Error(message);
  error.code = 'permission_denied';
  return error;
}
