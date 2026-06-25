const BUILD = '20260611-cv-print-builder-v2';
const MODULE_ID = 'cv-print-builder';
const PROFILE_MIME = 'application/vnd.ctox.cv-print-profile+json';
const CHUNK_SIZE = 256000;

const TEMPLATES = [
  { id: 'minimal', label: 'Minimal', description: 'Zeitlos, ruhig, kundentauglich und ohne Anbieteroptik.' },
  { id: 'classic', label: 'Klassisch', description: 'Formaler CV mit chronologischer Einspalten-Struktur.' },
  { id: 'modern', label: 'Modern', description: 'Logo-Kopf, Akzentlinie und kompakte Fakten.' },
];

const VIEW_LABELS = {
  original: 'Original',
  split: 'Beide',
  print: 'Print',
};

export async function mount(ctx) {
  await ensureStyles();
  const html = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());
  ctx.host.innerHTML = html;
  ctx.host.dataset.cvPrintBuilder = 'native';
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();

  const state = {
    ctx,
    host: ctx.host.querySelector('[data-cv-print-builder]'),
    items: [],
    selectedId: '',
    lastSelectedId: '',
    lastSelectedPhase: '',
    viewMode: 'original',
    originalUrls: new Map(),
    disposers: [],
    refreshTimer: null,
    renderSerial: 0,
  };

  bindStaticEvents(state);
  subscribeToCollections(state);
  await refresh(state);

  return () => {
    state.disposers.forEach((dispose) => dispose?.());
    state.originalUrls.forEach((url) => URL.revokeObjectURL(url));
    state.originalUrls.clear();
    clearTimeout(state.refreshTimer);
    ctx.host.replaceChildren();
    delete ctx.host.dataset.cvPrintBuilder;
  };
}

async function ensureStyles() {
  const href = `${new URL('./index.css', import.meta.url).pathname}?v=${BUILD}`;
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = href;
  document.head.append(link);
}

function bindStaticEvents(state) {
  state.host.querySelector('[data-cv-new]')?.addEventListener('click', () => {
    state.host.querySelector('[data-cv-upload]')?.click();
  });
  state.host.querySelector('[data-cv-upload]')?.addEventListener('change', async (event) => {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file) return;
    try {
      await importPdf(state, file);
    } catch (error) {
      notify(state, 'error', 'PDF konnte nicht importiert werden', String(error?.message || error));
    }
  });
  state.host.querySelector('[data-cv-logo-upload]')?.addEventListener('change', async (event) => {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file) return;
    const dataUrl = await fileToDataUrl(file);
    await patchSelectedModel(state, (model) => ({
      ...model,
      print: {
        ...model.print,
        showLogo: true,
        logoDataUrl: dataUrl,
      },
    }));
  });
}

function subscribeToCollections(state) {
  const schedule = () => {
    clearTimeout(state.refreshTimer);
    state.refreshTimer = setTimeout(() => refresh(state), 80);
  };
  ['documents', 'document_versions', 'desktop_files', 'desktop_file_chunks', 'business_chats', 'business_commands', 'ctox_queue_tasks']
    .forEach((name) => {
      const col = getCollection(state.ctx, name);
      const sub = col?.$?.subscribe?.(schedule);
      if (sub?.unsubscribe) state.disposers.push(() => sub.unsubscribe());
    });
}

async function refresh(state) {
  const documents = await findAll(state.ctx, 'documents');
  const versions = await findAll(state.ctx, 'document_versions');
  const versionById = new Map(versions.map((item) => [item.id, item]));
  state.items = documents
    .filter((doc) => doc.document_type === 'cv_print_profile' && !doc.is_deleted)
    .map((doc) => ({
      record: doc,
      version: versionById.get(doc.current_version_id),
      model: versionById.get(doc.current_version_id)?.model_json || null,
    }))
    .filter((item) => item.model)
    .sort((a, b) => (b.record.updated_at_ms || 0) - (a.record.updated_at_ms || 0));

  if (!state.selectedId && state.items[0]) {
    state.selectedId = state.items[0].record.id;
    state.viewMode = defaultViewMode(state.items[0].model);
  }
  if (state.selectedId && !state.items.some((item) => item.record.id === state.selectedId)) {
    state.selectedId = state.items[0]?.record.id || '';
    state.viewMode = state.items[0] ? defaultViewMode(state.items[0].model) : 'original';
  }
  const selected = state.items.find((item) => item.record.id === state.selectedId);
  if (selected) {
    const phase = workflowPhase(selected.model);
    if (state.lastSelectedId !== selected.record.id || state.lastSelectedPhase !== phase) {
      state.viewMode = defaultViewMode(selected.model);
      state.lastSelectedId = selected.record.id;
      state.lastSelectedPhase = phase;
    } else if (!viewAllowed(selected.model, state.viewMode)) {
      state.viewMode = defaultViewMode(selected.model);
    }
  } else {
    state.lastSelectedId = '';
    state.lastSelectedPhase = '';
  }

  render(state);
}

function render(state) {
  renderSidebar(state);
  renderStage(state);
}

function renderSidebar(state) {
  const list = state.host.querySelector('[data-cv-list]');
  const count = state.host.querySelector('[data-cv-count]');
  count.textContent = `${state.items.length} CV${state.items.length === 1 ? '' : 's'}`;
  list.innerHTML = state.items.map((item) => renderCandidateCard(state, item)).join('');
  list.querySelectorAll('[data-cv-select]').forEach((button) => {
    button.addEventListener('click', (event) => {
      if (event.target.closest('[data-cv-action],[data-cv-view],[data-cv-template],[data-cv-toggle-logo],[data-cv-toggle-anon],[data-cv-upload-logo]')) return;
      const id = button.dataset.cvSelect;
      const item = state.items.find((candidate) => candidate.record.id === id);
      state.selectedId = id;
      state.viewMode = defaultViewMode(item?.model);
      state.lastSelectedId = id;
      state.lastSelectedPhase = workflowPhase(item?.model);
      render(state);
    });
  });
  list.querySelectorAll('[data-cv-action]').forEach((button) => {
    button.addEventListener('click', async () => {
      const item = getSelectedItem(state);
      if (!item) return;
      const phase = workflowPhase(item.model);
      try {
        if (phase === 'uploaded' || phase === 'error') {
          await startParsing(state, item);
        } else if (phase === 'review') {
          await approvePrint(state, item);
        } else if (phase === 'approved') {
          window.print();
        }
      } catch (error) {
        notify(state, 'error', 'Aktion fehlgeschlagen', String(error?.message || error));
      }
    });
  });
  list.querySelectorAll('[data-cv-view]').forEach((button) => {
    button.addEventListener('click', async () => {
      const requested = button.dataset.cvView;
      const item = getSelectedItem(state);
      if (!item || !viewAllowed(item.model, requested)) return;
      state.viewMode = requested;
      await patchSelectedModel(state, (model) => ({
        ...model,
        workflow: {
          ...model.workflow,
          view_mode: requested,
          updated_at_ms: Date.now(),
        },
      }), { quiet: true });
      render(state);
    });
  });
  list.querySelectorAll('[data-cv-template]').forEach((select) => {
    select.addEventListener('change', async () => {
      await patchSelectedModel(state, (model) => ({
        ...model,
        print: {
          ...model.print,
          template: select.value,
        },
      }));
    });
  });
  list.querySelectorAll('[data-cv-toggle-logo]').forEach((button) => {
    button.addEventListener('click', async () => {
      await patchSelectedModel(state, (model) => ({
        ...model,
        print: {
          ...model.print,
          showLogo: !model.print?.showLogo,
        },
      }));
    });
  });
  list.querySelectorAll('[data-cv-toggle-anon]').forEach((button) => {
    button.addEventListener('click', async () => {
      await patchSelectedModel(state, (model) => ({
        ...model,
        print: {
          ...model.print,
          anonymize: !model.print?.anonymize,
        },
      }));
    });
  });
  list.querySelectorAll('[data-cv-upload-logo]').forEach((button) => {
    button.addEventListener('click', () => {
      state.host.querySelector('[data-cv-logo-upload]')?.click();
    });
  });
}

function renderCandidateCard(state, item) {
  const model = item.model;
  const candidate = model.candidate || {};
  const phase = workflowPhase(model);
  const selected = item.record.id === state.selectedId;
  const viewMode = selected ? state.viewMode : defaultViewMode(model);
  const action = actionForPhase(phase);
  const role = candidate.currentRole || 'CV Profil';
  const location = [candidate.location, candidate.availability].filter(Boolean).join(' · ');
  const template = normalizeTemplateId(model.print?.template || 'modern');
  const controlsDisabled = phase === 'uploaded' || phase === 'parsing';
  const approved = phase === 'approved';
  const templateOptions = TEMPLATES.map((entry) => (
    `<option value="${escapeHtml(entry.id)}"${entry.id === template ? ' selected' : ''}>${escapeHtml(entry.label)}</option>`
  )).join('');
  return `
    <article class="cv-card${selected ? ' is-selected' : ''}" data-cv-select="${escapeHtml(item.record.id)}" data-context-record-id="${escapeHtml(item.record.id)}" data-context-record-type="cv_profile" data-context-label="${escapeHtml(item.record.title || item.record.name || item.record.id)}">
      <div class="cv-card-main">
        <div class="cv-avatar">${escapeHtml(initials(candidate.name || item.record.title))}</div>
        <div class="cv-card-info">
          <h3 class="cv-card-name">${escapeHtml(displayCandidateName(model))}</h3>
          <div class="cv-card-line cv-card-role">${escapeHtml(role)}</div>
          <div class="cv-card-file">${escapeHtml(model.source?.filename || item.record.filename || 'cv.pdf')}</div>
          <div class="cv-card-line">${escapeHtml(location || phaseLabel(phase))}</div>
        </div>
        <span class="${phase === 'uploaded' ? 'cv-pdf-pill' : 'cv-status-pill'}">${escapeHtml(statusShort(phase))}</span>
      </div>
      ${selected ? `
        <div class="cv-card-controls">
          <div class="cv-flow-row">
            <button class="cv-icon-btn is-primary" type="button" title="${escapeHtml(action.title)}" aria-label="${escapeHtml(action.title)}" data-cv-action ${phase === 'parsing' ? 'disabled' : ''}>
              ${action.icon}
            </button>
            <div class="cv-segment" role="tablist" aria-label="Ansicht">
              ${Object.entries(VIEW_LABELS).map(([key, label]) => `
                <button type="button" data-cv-view="${key}" class="${viewMode === key ? 'is-active' : ''}" ${viewAllowed(model, key) && !approved ? '' : 'disabled'}>${escapeHtml(label)}</button>
              `).join('')}
            </div>
          </div>
          <div class="cv-template-row">
            <select class="cv-template-select" data-cv-template ${controlsDisabled || approved ? 'disabled' : ''} aria-label="Print Template">
              ${templateOptions}
            </select>
            <button class="cv-small-btn${model.print?.anonymize ? ' is-active' : ''}" type="button" data-cv-toggle-anon title="Anonymisieren" aria-label="Anonymisieren" ${controlsDisabled || approved ? 'disabled' : ''}>${iconEyeOff()}</button>
            <button class="cv-small-btn${model.print?.showLogo ? ' is-active' : ''}" type="button" data-cv-toggle-logo title="Logo anzeigen" aria-label="Logo anzeigen" ${controlsDisabled || approved ? 'disabled' : ''}>${iconImage()}</button>
            <button class="cv-small-btn" type="button" data-cv-upload-logo title="Logo wählen" aria-label="Logo wählen" ${controlsDisabled || approved ? 'disabled' : ''}>${iconUpload()}</button>
          </div>
        </div>
        <div class="cv-card-foot">${escapeHtml(phaseFootnote(model))}</div>
      ` : ''}
    </article>
  `;
}

function renderStage(state) {
  const stage = state.host.querySelector('[data-cv-stage]');
  const item = getSelectedItem(state);
  if (!item) {
    stage.innerHTML = `
      <div class="cv-empty-stage">
        <div>
          <strong>Kein CV ausgewählt</strong>
          <div>Links einen PDF-CV anlegen.</div>
        </div>
      </div>
    `;
    return;
  }
  const phase = workflowPhase(item.model);
  if (phase === 'approved') state.viewMode = 'print';
  if (!viewAllowed(item.model, state.viewMode)) state.viewMode = defaultViewMode(item.model);
  const serial = ++state.renderSerial;
  const mode = state.viewMode;
  const views = [];
  if (mode === 'original' || mode === 'split') views.push(renderOriginalPane(state, item));
  if (mode === 'print' || mode === 'split') views.push(renderPrintPane(item));
  stage.innerHTML = `<div class="cv-view-grid${mode === 'split' ? ' is-split' : ''}">${views.join('')}</div>`;
  bindEditablePrintFields(state);
  ensureOriginalUrl(state, item).then(() => {
    if (serial === state.renderSerial) renderOriginalFrame(state, item);
  });
}

function renderOriginalPane(state, item) {
  return `
    <section class="cv-pane" data-cv-original-pane>
      <header class="cv-pane-head">
        <strong>Original PDF</strong>
        <span>${escapeHtml(item.model.source?.filename || item.record.filename || '')}</span>
      </header>
      <div class="cv-pane-body" data-cv-original-body>
        <div class="cv-original-placeholder">PDF wird geladen.</div>
      </div>
    </section>
  `;
}

function renderOriginalFrame(state, item) {
  const body = state.host.querySelector('[data-cv-original-body]');
  if (!body) return;
  const url = state.originalUrls.get(item.model.source?.desktop_file_id);
  if (!url) {
    body.innerHTML = '<div class="cv-original-placeholder">Original-PDF ist noch nicht lokal materialisiert.</div>';
    return;
  }
  body.innerHTML = `<iframe class="cv-original-frame" title="Original PDF" src="${escapeAttr(url)}"></iframe>`;
}

function renderPrintPane(item) {
  const model = item.model;
  const template = normalizeTemplateId(model.print?.template || 'modern');
  return `
    <section class="cv-pane">
      <header class="cv-pane-head">
        <strong>Druckansicht</strong>
        <span>${escapeHtml(templateLabel(template))}${workflowPhase(model) === 'review' ? ' · Korrekturmodus' : ''}</span>
      </header>
      <div class="cv-pane-body">
        <div class="cv-print-wrap">
          ${renderPrintSheet(model)}
        </div>
      </div>
    </section>
  `;
}

function renderPrintSheet(model) {
  const template = normalizeTemplateId(model.print?.template || 'modern');
  if (template === 'minimal') return renderMinimalPrintSheet(model);
  if (template === 'classic') return renderClassicPrintSheet(model);
  return renderModernPrintSheet(model);
}

function renderModernPrintSheet(model) {
  const data = getPrintData(model);
  return `
    <article class="cv-print-sheet cv-template-modern">
      <header class="cv-print-head cv-modern-head">
        <div>
          <div class="cv-print-kicker">Qualifikationsprofil</div>
          <h1 class="cv-print-name cv-editable" ${editableAttr(data.editable, 'candidate.name')}>${escapeHtml(data.name)}</h1>
          <div class="cv-print-role cv-editable" ${editableAttr(data.editable, 'candidate.currentRole')}>${escapeHtml(data.role)}</div>
          <div class="cv-print-meta cv-editable" ${editableAttr(data.editable, 'candidate.location')}>${escapeHtml(data.meta || 'Ort / Verfügbarkeit')}</div>
        </div>
        ${data.logo}
      </header>
      <div class="cv-print-body cv-modern-body">
        <aside>
          <section class="cv-print-section">
            <h3>Kontakt</h3>
            <p>${escapeHtml(data.contact)}</p>
          </section>
          <section class="cv-print-section">
            <h3>Skills</h3>
            <ul class="cv-print-tags">${renderTags(data.skills, 'Skills nachtragen')}</ul>
          </section>
          <section class="cv-print-section">
            <h3>Sprachen</h3>
            <ul class="cv-print-list">${renderSimpleList(data.languages, 'Sprachen nachtragen')}</ul>
          </section>
        </aside>
        <main>
          <section class="cv-print-section">
            <h3>Profil</h3>
            <p class="cv-editable" ${editableAttr(data.editable, 'candidate.summary')}>${escapeHtml(data.summary)}</p>
          </section>
          <section class="cv-print-section">
            <h3>Berufserfahrung</h3>
            <ul class="cv-print-list">${renderTimeline(data.experience, 'Erfahrung nachtragen')}</ul>
          </section>
          <section class="cv-print-section">
            <h3>Ausbildung</h3>
            <ul class="cv-print-list">${renderTimeline(data.education, 'Ausbildung nachtragen')}</ul>
          </section>
        </main>
      </div>
    </article>
  `;
}

function renderMinimalPrintSheet(model) {
  const data = getPrintData(model, { minimalLogo: true });
  return `
    <article class="cv-print-sheet cv-template-minimal">
      <header class="cv-minimal-head">
        <div class="cv-minimal-title">
          <h1 class="cv-print-name cv-editable" ${editableAttr(data.editable, 'candidate.name')}>${escapeHtml(data.name)}</h1>
          <div class="cv-print-role cv-editable" ${editableAttr(data.editable, 'candidate.currentRole')}>${escapeHtml(data.role)}</div>
        </div>
        ${data.logo}
      </header>
      <div class="cv-minimal-facts">
        <span>${escapeHtml(data.meta || 'Ort / Verfügbarkeit')}</span>
        <span>${escapeHtml(data.contact)}</span>
        <span>${escapeHtml(data.degree || 'Abschluss nachtragen')}</span>
      </div>
      <main class="cv-minimal-body">
        <section class="cv-print-section cv-minimal-summary">
          <h3>Profil</h3>
          <p class="cv-editable" ${editableAttr(data.editable, 'candidate.summary')}>${escapeHtml(data.summary)}</p>
        </section>
        <section class="cv-print-section">
          <h3>Kernkompetenzen</h3>
          <ul class="cv-print-tags cv-minimal-tags">${renderTags(data.skills, 'Skills nachtragen')}</ul>
        </section>
        <section class="cv-print-section">
          <h3>Beruflicher Verlauf</h3>
          <ul class="cv-print-list cv-minimal-timeline">${renderTimeline(data.experience, 'Erfahrung nachtragen')}</ul>
        </section>
        <div class="cv-minimal-columns">
          <section class="cv-print-section">
            <h3>Ausbildung</h3>
            <ul class="cv-print-list">${renderTimeline(data.education, 'Ausbildung nachtragen')}</ul>
          </section>
          <section class="cv-print-section">
            <h3>Sprachen</h3>
            <ul class="cv-print-list">${renderSimpleList(data.languages, 'Sprachen nachtragen')}</ul>
          </section>
        </div>
      </main>
    </article>
  `;
}

function renderClassicPrintSheet(model) {
  const data = getPrintData(model, { classicLogo: true });
  return `
    <article class="cv-print-sheet cv-template-classic">
      <header class="cv-classic-head">
        ${data.logo}
        <div class="cv-print-kicker">Curriculum Vitae</div>
        <h1 class="cv-print-name cv-editable" ${editableAttr(data.editable, 'candidate.name')}>${escapeHtml(data.name)}</h1>
        <div class="cv-print-role cv-editable" ${editableAttr(data.editable, 'candidate.currentRole')}>${escapeHtml(data.role)}</div>
        <div class="cv-print-meta">${escapeHtml([data.meta, data.contact].filter(Boolean).join(' | '))}</div>
      </header>
      <main class="cv-classic-body">
        <section class="cv-print-section">
          <h3>Profil</h3>
          <p class="cv-editable" ${editableAttr(data.editable, 'candidate.summary')}>${escapeHtml(data.summary)}</p>
        </section>
        <section class="cv-print-section">
          <h3>Berufserfahrung</h3>
          <ul class="cv-print-list cv-classic-timeline">${renderClassicTimeline(data.experience, 'Erfahrung nachtragen')}</ul>
        </section>
        <section class="cv-print-section">
          <h3>Ausbildung</h3>
          <ul class="cv-print-list cv-classic-timeline">${renderClassicTimeline(data.education, 'Ausbildung nachtragen')}</ul>
        </section>
        <section class="cv-print-section cv-classic-skills">
          <h3>Kompetenzen und Sprachen</h3>
          <p>${escapeHtml([...data.skills.map(labelOf), ...data.languages.map(labelOf)].join(' · ') || 'Kompetenzen nachtragen')}</p>
        </section>
      </main>
    </article>
  `;
}

function getPrintData(model, options = {}) {
  const candidate = model.candidate || {};
  const phase = workflowPhase(model);
  const editable = phase === 'review';
  const anonymize = Boolean(model.print?.anonymize);
  const logo = model.print?.showLogo ? renderLogo(model, options) : '';
  const name = anonymize ? anonymizedName(candidate.name) : displayCandidateName(model);
  const role = candidate.currentRole || 'Position / Rolle';
  const meta = [candidate.location, candidate.availability, candidate.highestDegree].filter(Boolean).join(' · ');
  const summary = candidate.summary || 'Kurzprofil nach dem Parsing korrigieren.';
  const skills = normalizeArray(candidate.skills).slice(0, 16);
  const languages = normalizeArray(candidate.languages).slice(0, 8);
  const experience = normalizeTimeline(candidate.additional, 'cv.experience').slice(0, 5);
  const education = normalizeTimeline(candidate.additional, 'cv.education').slice(0, 4);
  const contact = anonymize ? 'Anonymisiert' : contactLine(candidate);
  const degree = candidate.highestDegree || candidate.degree || '';
  return { candidate, editable, anonymize, logo, name, role, meta, summary, skills, languages, experience, education, contact, degree };
}

function bindEditablePrintFields(state) {
  state.host.querySelectorAll('[data-cv-edit-field]').forEach((node) => {
    node.addEventListener('blur', async () => {
      const path = node.dataset.cvEditField;
      const value = node.textContent.trim();
      await patchSelectedModel(state, (model) => setPath({ ...model }, path, value), { quiet: true });
    });
  });
}

async function importPdf(state, file) {
  if (!/^application\/pdf$/i.test(file.type) && !/\.pdf$/i.test(file.name)) {
    throw new Error('Bitte eine PDF-Datei auswählen.');
  }
  requireCollections(state.ctx, ['documents', 'document_versions', 'desktop_files', 'desktop_file_chunks']);
  const bytes = new Uint8Array(await file.arrayBuffer());
  const sha = await sha256Hex(bytes);
  const now = Date.now();
  const documentId = `cv_${crypto.randomUUID()}`;
  const versionId = `${documentId}_v1`;
  const fileId = `desktop_file_${documentId}`;
  const generationId = `${fileId}_g1`;
  const base64 = uint8ToBase64(bytes);
  const title = titleFromFilename(file.name);
  const model = createInitialModel({
    documentId,
    versionId,
    fileId,
    generationId,
    filename: file.name,
    title,
    sha,
    size: file.size,
    now,
  });

  await saveDesktopFile(state.ctx, {
    fileId,
    generationId,
    documentId,
    filename: file.name,
    mimeType: file.type || 'application/pdf',
    size: file.size,
    sha,
    base64,
    now,
  });

  await getCollection(state.ctx, 'document_versions').insert({
    id: versionId,
    document_id: documentId,
    version: 1,
    source_kind: 'cv_pdf_upload',
    blob_id: fileId,
    model_json: model,
    diagnostics: [],
    created_at_ms: now,
    updated_at_ms: now,
  });

  await getCollection(state.ctx, 'documents').insert({
    id: documentId,
    title,
    filename: file.name,
    mime_type: PROFILE_MIME,
    status: 'uploaded',
    document_type: 'cv_print_profile',
    owner_id: '',
    current_version_id: versionId,
    source_sha256: sha,
    page_count: 0,
    diagnostics_count: 0,
    linked_records: [
      { collection: 'desktop_files', id: fileId, role: 'source_pdf' },
    ],
    display_cache: {
      candidate_name: title,
      phase: 'uploaded',
      template: 'minimal',
    },
    index_text: `${title} ${file.name}`,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  });

  state.originalUrls.set(fileId, URL.createObjectURL(file));
  state.selectedId = documentId;
  state.viewMode = 'original';
  await refresh(state);
}

async function saveDesktopFile(ctx, input) {
  await getCollection(ctx, 'desktop_files').insert({
    id: input.fileId,
    parent_id: '',
    path: '',
    local_path: '',
    virtual_path: `/cv-print-builder/${input.filename}`,
    name: input.filename,
    kind: 'file',
    mime_type: input.mimeType,
    extension: 'pdf',
    size_bytes: input.size,
    owner_id: '',
    source: MODULE_ID,
    linked_collection: 'documents',
    linked_record_id: input.documentId,
    content_ref: '',
    content_state: 'available',
    content_hash: input.sha,
    content_hash_scheme: 'sha256',
    content_generation_id: input.generationId,
    mtime_ms: input.now,
    content_synced_at_ms: input.now,
    sort_index: input.now,
    is_deleted: false,
    created_at_ms: input.now,
    updated_at_ms: input.now,
  });
  const total = Math.ceil(input.base64.length / CHUNK_SIZE) || 1;
  for (let idx = 0; idx < total; idx += 1) {
    const data = input.base64.slice(idx * CHUNK_SIZE, (idx + 1) * CHUNK_SIZE);
    await getCollection(ctx, 'desktop_file_chunks').insert({
      id: `${input.fileId}_${idx}`,
      file_id: input.fileId,
      generation_id: input.generationId,
      content_hash: input.sha,
      content_hash_scheme: 'sha256',
      idx,
      total,
      encoding: 'base64',
      data,
      chunk_hash: '',
      chunk_hash_scheme: '',
      size_bytes: data.length,
      created_at_ms: Date.now(),
    });
  }
}

async function startParsing(state, item) {
  if (!state.ctx.commandBus?.dispatch) {
    throw new Error('CTOX commandBus ist nicht verfügbar. CV Parsing muss als Business-OS Chat-Task gestartet werden.');
  }
  requireCollections(state.ctx, ['business_chats', 'documents', 'document_versions']);
  const now = Date.now();
  const chatId = `chat_cv_${item.record.id}`;
  const prompt = buildParserPrompt(item);
  await upsertBusinessChat(state.ctx, {
    id: chatId,
    title: `CV Parsing · ${displayCandidateName(item.model)}`,
    message: prompt,
    now,
  });
  const result = await state.ctx.commandBus.dispatch({
    module: MODULE_ID,
    type: 'business_os.chat.task',
    record_id: item.record.id,
    inbound_channel: MODULE_ID,
    payload: {
      title: `CV strukturieren: ${item.model.source?.filename || item.record.filename}`,
      instruction: prompt,
      prompt,
      chat_id: chatId,
      message_id: `msg_${crypto.randomUUID()}`,
      conversation: [],
      attachments: [
        {
          kind: 'desktop_file',
          file_id: item.model.source?.desktop_file_id,
          name: item.model.source?.filename || item.record.filename,
          mime_type: 'application/pdf',
        },
      ],
      outbound_channel: 'business_os_chat',
      response_channel: 'business_os_chat',
      inbound_channel: MODULE_ID,
      source_module: MODULE_ID,
      skill: 'ctox-cv-print-parser',
      skill_id: 'ctox-cv-print-parser',
      source_file_id: item.model.source?.desktop_file_id,
      document_id: item.record.id,
      version_id: item.version.id,
      writeback_contract: {
        command_type: 'ctox.cv_print.apply_parse',
        target_collection: 'document_versions',
        document_id: item.record.id,
        expected_model_schema: 'ctox.cv_print_profile.v1',
      },
    },
    client_context: {
      surface: 'business-os-cv-print-builder',
      action: 'parse_cv_pdf',
      module: MODULE_ID,
      document_id: item.record.id,
      version_id: item.version.id,
      desktop_file_id: item.model.source?.desktop_file_id,
      chat_id: chatId,
    },
  });

  await patchItemModel(state, item, (model) => ({
    ...model,
    workflow: {
      ...model.workflow,
      phase: 'parsing',
      chat_id: chatId,
      command_id: result?.command_id || result?.id || '',
      task_id: result?.task_id || result?.tracking_id || '',
      task_status: result?.status || 'pending_sync',
      updated_at_ms: now,
    },
  }), {
    documentStatus: 'parsing',
    displayPhase: 'parsing',
  });
  state.viewMode = 'original';
  await refresh(state);
}

function buildParserPrompt(item) {
  const source = item.model.source || {};
  return [
    'Du bist der CTOX Business-OS Skill `ctox-cv-print-parser`.',
    '',
    'Ziel: Lies das angehängte CV-PDF über den CTOX PDF Stack und die Business-OS',
    'RxDB/WebRTC-Datenebene und strukturiere es in ein einheitliches Druckprofil.',
    '',
    'Strikte Ausgabe-Regeln:',
    '- Antworte ausschließlich mit einem einzigen JSON-Objekt (kein Markdown, kein Fließtext).',
    '- Das JSON ist das vollständige `model_json` mit `schema = "ctox.cv_print_profile.v1"`.',
    '- Setze `workflow.phase = "review"`.',
    '- Fülle `candidate` (name, firstName, lastName, currentRole, location, email, …) und',
    '  `candidate.additional` mit den Keys `cv.education`, `cv.experience`, `cv.skills`, `cv.meta`.',
    '- Notiere unsichere/fehlende Felder in `workflow.diagnostics` (Liste aus {level, message}).',
    '- Keine HTTP-Fallbacks, keine externen Services.',
    '',
    'Eingaben:',
    `- document_id: ${item.record.id}`,
    `- version_id: ${item.version.id}`,
    `- desktop_file_id: ${source.desktop_file_id || ''}`,
    `- filename: ${source.filename || item.record.filename || ''}`,
    '',
    'Die neue `document_versions`-Version und der `documents`-Patch werden vom nativen',
    'Writeback `ctox.cv_print.apply_parse` aus deinem JSON erzeugt — gib nur das JSON zurück.',
  ].join('\n');
}

async function approvePrint(state, item) {
  await patchItemModel(state, item, (model) => ({
    ...model,
    workflow: {
      ...model.workflow,
      phase: 'approved',
      view_mode: 'print',
      approved_at_ms: Date.now(),
      updated_at_ms: Date.now(),
    },
  }), {
    documentStatus: 'approved',
    displayPhase: 'approved',
  });
  state.viewMode = 'print';
  await refresh(state);
}

async function upsertBusinessChat(ctx, input) {
  const col = getCollection(ctx, 'business_chats');
  const existing = await col.findOne(input.id).exec();
  const message = {
    id: `msg_${crypto.randomUUID()}`,
    role: 'user',
    content: input.message,
    createdAt: input.now,
    source_module: MODULE_ID,
  };
  if (existing?.incrementalPatch) {
    const json = existing.toJSON();
    await existing.incrementalPatch({
      title: input.title,
      open: true,
      minimized: false,
      messages: [...(json.messages || []), message],
      draft: '',
      updated_at_ms: input.now,
    });
    return;
  }
  await col.insert({
    id: input.id,
    title: input.title,
    open: true,
    minimized: false,
    owner_user_id: '',
    lastTrackingId: '',
    messages: [message],
    draft: '',
    createdAt: input.now,
    updated_at_ms: input.now,
  });
}

async function ensureOriginalUrl(state, item) {
  const fileId = item.model.source?.desktop_file_id;
  if (!fileId || state.originalUrls.has(fileId)) return;
  const chunks = await findAll(state.ctx, 'desktop_file_chunks');
  const fileChunks = chunks
    .filter((chunk) => chunk.file_id === fileId)
    .sort((a, b) => (a.idx || 0) - (b.idx || 0));
  if (!fileChunks.length) return;
  const base64 = fileChunks.map((chunk) => chunk.data || '').join('');
  const bytes = base64ToUint8(base64);
  const blob = new Blob([bytes], { type: 'application/pdf' });
  state.originalUrls.set(fileId, URL.createObjectURL(blob));
}

async function patchSelectedModel(state, updater, options = {}) {
  const item = getSelectedItem(state);
  if (!item) return;
  await patchItemModel(state, item, updater, options);
  if (!options.quiet) await refresh(state);
}

async function patchItemModel(state, item, updater, options = {}) {
  const now = Date.now();
  const current = structuredClone(item.model);
  const next = updater(current);
  next.workflow = {
    ...next.workflow,
    updated_at_ms: now,
  };
  const versionDoc = await getCollection(state.ctx, 'document_versions').findOne(item.version.id).exec();
  if (!versionDoc?.incrementalPatch) throw new Error('CV Version konnte nicht aktualisiert werden.');
  await versionDoc.incrementalPatch({
    model_json: next,
    diagnostics: next.workflow?.diagnostics || item.version.diagnostics || [],
    updated_at_ms: now,
  });
  const documentDoc = await getCollection(state.ctx, 'documents').findOne(item.record.id).exec();
  if (documentDoc?.incrementalPatch) {
    await documentDoc.incrementalPatch({
      title: displayCandidateName(next),
      status: options.documentStatus || documentDoc.toJSON().status || workflowPhase(next),
      diagnostics_count: (next.workflow?.diagnostics || []).length,
      display_cache: {
        ...(documentDoc.toJSON().display_cache || {}),
        candidate_name: displayCandidateName(next),
        phase: options.displayPhase || workflowPhase(next),
        template: normalizeTemplateId(next.print?.template || 'minimal'),
      },
      index_text: buildIndexText(next),
      updated_at_ms: now,
    });
  }
}

function createInitialModel(input) {
  return {
    schema: 'ctox.cv_print_profile.v1',
    source: {
      desktop_file_id: input.fileId,
      generation_id: input.generationId,
      filename: input.filename,
      mime_type: 'application/pdf',
      size_bytes: input.size,
      sha256: input.sha,
      imported_at_ms: input.now,
    },
    candidate: {
      id: input.documentId,
      name: input.title,
      firstName: '',
      lastName: '',
      currentRole: '',
      desiredPosition: '',
      location: '',
      availability: '',
      email: '',
      phone: '',
      highestDegree: '',
      degree: '',
      nationality: '',
      birthDate: '',
      languages: [],
      skills: [],
      softSkills: [],
      summary: '',
      additional: [
        { key: 'cv.education', label: 'Ausbildung (CV)', value: [] },
        { key: 'cv.experience', label: 'Berufserfahrung (CV)', value: [] },
        { key: 'cv.skills', label: 'Skills (CV)', value: {} },
        { key: 'cv.meta', label: 'Stammdaten (CV)', value: {} },
      ],
    },
    print: {
      template: 'minimal',
      anonymize: false,
      showLogo: true,
      logoDataUrl: '',
      preset: 'standard',
      overrides: {},
    },
    workflow: {
      phase: 'uploaded',
      view_mode: 'original',
      chat_id: '',
      command_id: '',
      task_id: '',
      task_status: '',
      diagnostics: [],
      created_at_ms: input.now,
      updated_at_ms: input.now,
    },
  };
}

function getCollection(ctx, name) {
  const collection = ctx?.db?.collection?.(name);
  if (!collection) throw new Error(`Business-OS Collection fehlt: ${name}`);
  return collection;
}

async function findAll(ctx, name) {
  const collection = ctx?.db?.collection?.(name);
  if (!collection?.find) return [];
  const docs = await collection.find().exec();
  return docs.map((doc) => doc?.toJSON ? doc.toJSON() : doc);
}

function requireCollections(ctx, names) {
  names.forEach((name) => getCollection(ctx, name));
}

function getSelectedItem(state) {
  return state.items.find((item) => item.record.id === state.selectedId) || null;
}

function workflowPhase(model) {
  return model?.workflow?.phase || 'uploaded';
}

function defaultViewMode(model) {
  const phase = workflowPhase(model);
  if (phase === 'review') return model.workflow?.view_mode || 'split';
  if (phase === 'approved') return 'print';
  return 'original';
}

function viewAllowed(model, view) {
  const phase = workflowPhase(model);
  if (phase === 'approved') return view === 'print';
  if (view === 'original') return true;
  return phase === 'review';
}

function actionForPhase(phase) {
  if (phase === 'review') return { title: 'Print freigeben', icon: iconCheck() };
  if (phase === 'approved') return { title: 'Drucken', icon: iconPrinter() };
  return { title: 'Parsing starten', icon: iconPlay() };
}

function phaseLabel(phase) {
  return ({
    uploaded: 'PDF geladen',
    parsing: 'Parsing läuft',
    review: 'Korrektur',
    approved: 'Freigegeben',
    error: 'Fehler',
  })[phase] || phase;
}

function statusShort(phase) {
  return ({
    uploaded: 'PDF',
    parsing: 'Parsing',
    review: 'Korrektur',
    approved: 'Print',
    error: 'Fehler',
  })[phase] || 'CV';
}

function phaseFootnote(model) {
  const phase = workflowPhase(model);
  if (phase === 'uploaded') return 'PDF geladen. Als Nächstes Parsing starten.';
  if (phase === 'parsing') return `Task läuft${model.workflow?.task_id ? ` · ${model.workflow.task_id}` : ''}`;
  if (phase === 'review') return `${templateLabel(model.print?.template)} · Korrektur und Template prüfen.`;
  if (phase === 'approved') return `${templateLabel(model.print?.template)} · Druckansicht freigegeben.`;
  return 'Status prüfen.';
}

function templateLabel(id) {
  return TEMPLATES.find((entry) => entry.id === normalizeTemplateId(id))?.label || 'Modern';
}

function normalizeTemplateId(id) {
  if (id === 'neutral') return 'minimal';
  if (id === 'klassisch') return 'classic';
  if (id === 'modern' || id === 'minimal' || id === 'classic') return id;
  return 'modern';
}

function displayCandidateName(model) {
  const candidate = model?.candidate || {};
  return candidate.name || [candidate.firstName, candidate.lastName].filter(Boolean).join(' ') || model?.source?.filename?.replace(/\.pdf$/i, '') || 'Neuer CV';
}

function titleFromFilename(filename) {
  return String(filename || 'Neuer CV')
    .replace(/\.[^.]+$/, '')
    .replace(/[_-]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function initials(name) {
  const parts = String(name || 'CV').trim().split(/\s+/).filter(Boolean);
  return (parts[0]?.[0] || 'C') + (parts[1]?.[0] || parts[0]?.[1] || 'V');
}

function anonymizedName(name) {
  const parts = String(name || '').trim().split(/\s+/).filter(Boolean);
  if (!parts.length) return 'Kandidat';
  return `${parts[0]} ${parts[1] ? parts[1][0] + '.' : ''}`.trim();
}

function contactLine(candidate) {
  return [candidate.email, candidate.phone].filter(Boolean).join(' · ') || 'Kontakt nachtragen';
}

function renderLogo(model, options = {}) {
  const logo = model.print?.logoDataUrl;
  if (logo) return `<div class="cv-print-logo"><img alt="Logo" src="${escapeAttr(logo)}"></div>`;
  if (options.minimalLogo) return '';
  if (options.classicLogo) return '<div class="cv-print-logo cv-print-logo-classic">CV</div>';
  return '<div class="cv-print-logo">ctox</div>';
}

function normalizeArray(value) {
  if (!Array.isArray(value)) return [];
  return value.filter((item) => item !== null && item !== undefined && String(labelOf(item)).trim());
}

function normalizeTimeline(additional, key) {
  const entry = Array.isArray(additional) ? additional.find((item) => item?.key === key) : null;
  return Array.isArray(entry?.value) ? entry.value : [];
}

function renderTimeline(items, fallback) {
  if (!items.length) return `<li>${escapeHtml(fallback)}</li>`;
  return items.map((item) => {
    const title = item.title || item.position || item.degree || item.school || item.company || item.name || labelOf(item);
    const meta = [item.company, item.institution, item.location, item.start, item.end || item.until].filter(Boolean).join(' · ');
    const description = item.description || item.summary || '';
    return `<li><strong>${escapeHtml(title)}</strong>${meta ? `<span>${escapeHtml(meta)}</span>` : ''}${description ? `<p>${escapeHtml(description)}</p>` : ''}</li>`;
  }).join('');
}

function renderClassicTimeline(items, fallback) {
  if (!items.length) return `<li>${escapeHtml(fallback)}</li>`;
  return items.map((item) => {
    const title = item.title || item.position || item.degree || item.school || item.company || item.name || labelOf(item);
    const period = [item.start, item.end || item.until].filter(Boolean).join(' - ');
    const place = [item.company, item.institution, item.location].filter(Boolean).join(' · ');
    const description = item.description || item.summary || '';
    return `
      <li>
        <span class="cv-classic-period">${escapeHtml(period || 'Zeitraum')}</span>
        <span>
          <strong>${escapeHtml(title)}</strong>
          ${place ? `<em>${escapeHtml(place)}</em>` : ''}
          ${description ? `<p>${escapeHtml(description)}</p>` : ''}
        </span>
      </li>
    `;
  }).join('');
}

function renderTags(items, fallback) {
  return items.map((item) => `<li>${escapeHtml(labelOf(item))}</li>`).join('') || `<li>${escapeHtml(fallback)}</li>`;
}

function renderSimpleList(items, fallback) {
  return items.map((item) => `<li>${escapeHtml(labelOf(item))}</li>`).join('') || `<li>${escapeHtml(fallback)}</li>`;
}

function labelOf(item) {
  if (typeof item === 'string') return item;
  return item?.label || item?.name || item?.title || item?.skill || item?.language || JSON.stringify(item);
}

function editableAttr(editable, path) {
  return editable ? `contenteditable="true" spellcheck="false" data-cv-edit-field="${escapeAttr(path)}"` : '';
}

function setPath(model, path, value) {
  const parts = String(path).split('.');
  let target = model;
  for (const part of parts.slice(0, -1)) {
    target[part] = { ...(target[part] || {}) };
    target = target[part];
  }
  target[parts.at(-1)] = value;
  return model;
}

function buildIndexText(model) {
  const candidate = model.candidate || {};
  return [
    displayCandidateName(model),
    candidate.currentRole,
    candidate.location,
    candidate.availability,
    candidate.summary,
    ...normalizeArray(candidate.skills).map(labelOf),
    ...normalizeArray(candidate.languages).map(labelOf),
  ].filter(Boolean).join(' ').slice(0, 20000);
}

async function sha256Hex(bytes) {
  const hash = await crypto.subtle.digest('SHA-256', bytes);
  return Array.from(new Uint8Array(hash)).map((value) => value.toString(16).padStart(2, '0')).join('');
}

function uint8ToBase64(bytes) {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(binary);
}

function base64ToUint8(base64) {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

function fileToDataUrl(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result || ''));
    reader.onerror = () => reject(reader.error || new Error('Datei konnte nicht gelesen werden.'));
    reader.readAsDataURL(file);
  });
}

function notify(state, type, title, message) {
  state.ctx.notifications?.show?.({ type, title, message, time: 8000 });
  if (!state.ctx.notifications?.show) console[type === 'error' ? 'error' : 'log'](`[${MODULE_ID}] ${title}: ${message}`);
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

function escapeAttr(value) {
  return escapeHtml(value).replace(/`/g, '&#096;');
}

function iconPlay() {
  return '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5v14l11-7z"/></svg>';
}

function iconCheck() {
  return '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m5 12 4 4L19 6"/></svg>';
}

function iconPrinter() {
  return '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 8V4h10v4"/><path d="M7 17H5a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2h-2"/><path d="M7 14h10v6H7z"/></svg>';
}

function iconUpload() {
  return '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 16V4"/><path d="m7 9 5-5 5 5"/><path d="M5 20h14"/></svg>';
}

function iconImage() {
  return '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="5" width="16" height="14" rx="2"/><path d="m8 15 3-3 2 2 2-3 3 4"/><circle cx="9" cy="9" r="1"/></svg>';
}

function iconEyeOff() {
  return '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m3 3 18 18"/><path d="M10.6 10.6a2 2 0 0 0 2.8 2.8"/><path d="M9.5 5.3A10.4 10.4 0 0 1 12 5c5 0 8.5 4.5 9.5 7a12 12 0 0 1-3 4.1"/><path d="M6.2 6.8A12 12 0 0 0 2.5 12c1 2.5 4.5 7 9.5 7a10 10 0 0 0 4-.8"/></svg>';
}
