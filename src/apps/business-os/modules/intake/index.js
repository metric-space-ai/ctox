import { normalizeApplication, applicationDedupeKey } from './core/application.js';

const MOD_BUILD = '20260721-intake-ia';
const MODULE_ID = 'intake';
const PRIMARY = 'applications';
const CAPTURE_COMMAND = 'ats.intake.capture';
// Application statuses that count as "closed" (out of the active funnel). The
// left column's counted band splits Alle / Offen / Abgeschlossen from this.
const CLOSED_STATUSES = new Set(['hired', 'rejected', 'duplicate']);

const COPY = {
  de: {
    kicker: 'Bewerbungseingang', listTitle: 'Bewerbungen', newTitle: 'Neue Bewerbung',
    name: 'Name', email: 'E-Mail', phone: 'Telefon', vacancy: 'Vakanz-ID', channel: 'Kanal',
    capture: 'Erfassen', more: 'Weitere Angaben', entries: 'Einträge', empty: 'Noch keine Einträge.',
    emptyFiltered: 'Keine Bewerbung passt zum Filter.',
    offlineService: 'Offline: Befehlsdienst nicht verfügbar.', nameRequired: 'Name ist erforderlich.',
    offlineSend: 'Offline: Befehl konnte nicht gesendet werden.', blocked: 'Eingang blockiert.',
    captured: 'Bewerbung erfasst.', application: 'Application', dedupeKey: 'Dedupe-Key',
    unnamed: '(ohne Namen)', dedupe: 'Dedupe', vacancyLabel: 'Vakanz', documents: 'Dok.',
    received: 'Empfangen', closeDetail: 'Details schließen', imported: 'Importiert',
    importInvalid: 'Ungültige JSON-Datei.', importEmpty: 'Keine Datensätze in der Datei.',
    statusAll: 'Alle Status', bandAll: 'Alle', bandOpen: 'Offen', bandClosed: 'Abgeschlossen',
    status_new: 'Neu', status_screening: 'Prüfung', status_hired: 'Eingestellt',
    status_rejected: 'Abgelehnt', status_duplicate: 'Duplikat',
  },
  en: {
    kicker: 'Application intake', listTitle: 'Applications', newTitle: 'New application',
    name: 'Name', email: 'Email', phone: 'Phone', vacancy: 'Vacancy ID', channel: 'Channel',
    capture: 'Capture', more: 'More details', entries: 'records', empty: 'No applications yet.',
    emptyFiltered: 'No application matches the filter.',
    offlineService: 'Offline: command service unavailable.', nameRequired: 'Name is required.',
    offlineSend: 'Offline: command could not be sent.', blocked: 'Intake blocked.',
    captured: 'Application captured.', application: 'Application', dedupeKey: 'Dedupe key',
    unnamed: '(unnamed)', dedupe: 'Dedupe', vacancyLabel: 'Vacancy', documents: 'docs',
    received: 'Received', closeDetail: 'Close details', imported: 'Imported',
    importInvalid: 'Invalid JSON file.', importEmpty: 'No records in the file.',
    statusAll: 'All statuses', bandAll: 'All', bandOpen: 'Open', bandClosed: 'Closed',
    status_new: 'New', status_screening: 'Screening', status_hired: 'Hired',
    status_rejected: 'Rejected', status_duplicate: 'Duplicate',
  },
};
let text = COPY.de;
let locale = 'de';

// ---------------------------------------------------------------------------
// Pure record helpers (exported for tests — no DOM, no RxDB).
// ---------------------------------------------------------------------------

// Auto-reveal model (design-guide "Progressive Disclosure", outbound idiom):
// the record detail is shown only when something is selected and the user has
// not collapsed it.
export function shouldRevealRecord(hasSelection, userCollapsed) {
  return Boolean(hasSelection) && !userCollapsed;
}

export function candidateName(r) {
  const candidate = r && typeof r.candidate === 'object' && r.candidate ? r.candidate : {};
  return candidate.name || r?.name || text.unnamed;
}

export function statusLabel(status) {
  const key = 'status_' + String(status || 'new');
  return text[key] || String(status || 'new');
}

// Which counted band a record belongs to.
export function bandOf(status) {
  return CLOSED_STATUSES.has(String(status || 'new')) ? 'closed' : 'open';
}

export function countsFor(rows) {
  const list = Array.isArray(rows) ? rows : [];
  let open = 0;
  let closed = 0;
  for (const r of list) {
    if (bandOf(r.status) === 'closed') closed += 1; else open += 1;
  }
  return { all: list.length, open, closed };
}

// Apply the current grammar state (band + status filter + search) to the rows.
export function filterRows(rows, { band = 'all', status = 'all', search = '' } = {}) {
  const needle = String(search || '').trim().toLowerCase();
  return (Array.isArray(rows) ? rows : []).filter((r) => {
    const st = String(r.status || 'new');
    if (band === 'open' && bandOf(st) !== 'open') return false;
    if (band === 'closed' && bandOf(st) !== 'closed') return false;
    if (status && status !== 'all' && st !== status) return false;
    if (needle) {
      const candidate = r && typeof r.candidate === 'object' && r.candidate ? r.candidate : {};
      const hay = [candidateName(r), r.channel, r.vacancy_id, candidate.email, candidate.phone, r.email, r.phone, st, r.id]
        .filter(Boolean).join(' ').toLowerCase();
      if (!hay.includes(needle)) return false;
    }
    return true;
  });
}

// A shard is a pure selector: title + ONE muted meta line. No inline expansion
// inside the selection list (design-guide "Canonical Column Grammar").
export function applicationRow(r, opts = {}) {
  const view = opts.view === 'list' ? 'list' : 'cards';
  const selected = Boolean(opts.selected);
  const name = candidateName(r);
  const status = String(r.status || 'new');
  const channel = r.channel || '—';
  const id = r.id || '';
  const ts = Number(r.received_at_ms || r.created_at_ms || 0);
  const badge = '<span class="' + ('ctox-badge ' + statusBadgeClass(status)).trim()
    + '" data-status="' + esc(status) + '">' + esc(statusLabel(status)) + '</span>';
  const attrs = ' class="ctox-list-item intake-row intake-row--' + view + (selected ? ' is-selected' : '') + '"'
    + ' role="button" tabindex="0" aria-selected="' + (selected ? 'true' : 'false') + '"'
    + ' data-context-record-id="' + esc(id) + '"'
    + ' data-context-record-type="application"'
    + ' data-context-label="' + esc(name || id) + '"';
  if (view === 'list') {
    return '<div' + attrs + '><span class="intake-row-title">' + esc(name) + '</span>' + badge + '</div>';
  }
  const metaBits = [esc(text.kicker), esc(statusLabel(status)), esc(channel)];
  if (ts) metaBits.push(esc(fmtDate(ts)));
  return '<div' + attrs + '>'
    + '<div class="intake-row-head"><span class="intake-row-title">' + esc(name) + '</span>' + badge + '</div>'
    + '<div class="intake-row-meta">' + metaBits.join(' · ') + '</div>'
    + '</div>';
}

// The list body markup (shards or compact rows), or the empty state.
export function renderRecordList(rows, opts = {}) {
  const list = Array.isArray(rows) ? rows : [];
  if (!list.length) {
    return '<div class="ctox-empty"><strong>' + esc(opts.emptyText || text.empty) + '</strong></div>';
  }
  return list.map((r) => applicationRow(r, { view: opts.view, selected: r.id && r.id === opts.selectedId })).join('');
}

// Read-only detail card for the selected record.
export function recordDetailHtml(r) {
  const name = candidateName(r);
  const status = String(r.status || 'new');
  const candidate = r && typeof r.candidate === 'object' && r.candidate ? r.candidate : {};
  const rows = [];
  const contact = [candidate.email || r.email, candidate.phone || r.phone].filter(Boolean).map(esc).join(' · ');
  rows.push(field(text.channel, esc(r.channel || '—')));
  if (contact) rows.push(field(text.email, contact));
  if (r.vacancy_id) rows.push(field(text.vacancyLabel, esc(r.vacancy_id)));
  const docs = Array.isArray(r.documents) ? r.documents.length : 0;
  if (docs) rows.push(field(text.documents, String(docs)));
  const ts = Number(r.received_at_ms || r.created_at_ms || 0);
  if (ts) rows.push(field(text.received, esc(fmtDate(ts))));
  if (r.id) rows.push(field('ID', '<span class="ats-tag">' + esc(r.id) + '</span>'));
  if (r.dedupe_key) rows.push(field(text.dedupe, esc(r.dedupe_key)));
  return '<header class="intake-detail-head">'
    + '<div class="intake-detail-titles">'
    + '<span class="' + ('ctox-badge ' + statusBadgeClass(status)).trim() + '" data-status="' + esc(status) + '">' + esc(statusLabel(status)) + '</span>'
    + '<strong class="intake-detail-name">' + esc(name) + '</strong>'
    + '</div>'
    + '<button type="button" class="ctox-pane-icon" data-action="collapse-detail" aria-label="' + esc(text.closeDetail) + '" title="' + esc(text.closeDetail) + '"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"/></svg></button>'
    + '</header>'
    + '<dl class="ctox-fields ctox-fields--stacked">' + rows.join('') + '</dl>';
}

function field(label, valueHtml) {
  return '<dt>' + esc(label) + '</dt><dd>' + valueHtml + '</dd>';
}

// Reuse the intake normalizer for imported JSON, preserving id/status/timestamps
// from an exported record (round-trip friendly). Required fields: id, channel,
// updated_at_ms.
export function prepareImport(raw, nowMs, salt = '') {
  const norm = normalizeApplication(raw);
  const now = Number(nowMs) || 0;
  const id = norm.id || (raw && typeof raw === 'object' && raw.id) || ('app_' + now + '_' + salt);
  const dedupe_key = (raw && typeof raw === 'object' && raw.dedupe_key) || applicationDedupeKey(norm) || '';
  const status = raw && typeof raw === 'object' && typeof raw.status === 'string' && raw.status ? raw.status : norm.status;
  const received = norm.received_at_ms || Number(raw && raw.received_at_ms) || now;
  const created = Number(raw && raw.created_at_ms) || received;
  return { ...norm, id: String(id), dedupe_key, status, received_at_ms: received, created_at_ms: created, updated_at_ms: now };
}

// ---------------------------------------------------------------------------
// Mount
// ---------------------------------------------------------------------------

export async function mount(ctx) {
  locale = ctx.locale === 'en' ? 'en' : 'de';
  text = COPY[locale];
  await ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.host.dataset.atsModule = MODULE_ID;
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();

  const root = ctx.host.querySelector('[data-ats-root]');
  const rail = root?.querySelector('.intake-rail');
  const listEl = root?.querySelector('[data-ats-list]');
  const formEl = root?.querySelector('[data-ats-form]');
  const gateEl = root?.querySelector('[data-ats-gate]');
  const titleEl = root?.querySelector('[data-ats-title]');
  const subEl = root?.querySelector('[data-ats-sub]');
  const detailEl = root?.querySelector('[data-ats-detail]');
  if (subEl) subEl.textContent = ctx.manifest?.description || '';
  applyStaticCopy(root);

  let rowsCache = [];
  let selectedId = null;
  let userCollapsed = false;
  const collection = () => { try { return ctx.db?.collection?.(PRIMARY) || null; } catch { return null; } };

  // Gate callout → kit .ctox-callout variants (base.css).
  const GATE_VARIANTS = { ok: ' is-success', block: ' is-danger', offline: ' is-warning' };
  function setGate(html, kind) {
    if (!gateEl) return;
    gateEl.className = 'ctox-callout' + (GATE_VARIANTS[kind] || '');
    gateEl.innerHTML = html || '';
  }

  // Read the current SHELL-wired grammar state straight from the pane DOM. The
  // shell owns the chrome behaviour; the module only reads the resulting state.
  function readGrammar() {
    return {
      search: (rail?.querySelector('[data-pg-search]')?.value || '').trim().toLowerCase(),
      view: rail?.querySelector('[data-pg-view][aria-pressed="true"]')?.dataset.pgView || 'cards',
      band: rail?.querySelector('[data-pg-band][aria-selected="true"]')?.dataset.pgBand || 'all',
      status: rail?.querySelector('[data-pg-filter][data-pg-name="status"]')?.value || 'all',
    };
  }

  // Counts/footer via the shell handle when it has wired the pane, else plain
  // textContent (the shell wires asynchronously after mount).
  function writeCounts(counts) {
    const pg = rail?.__ctoxPaneGrammar;
    if (pg && typeof pg.setCounts === 'function') { pg.setCounts(counts); return; }
    for (const [key, value] of Object.entries(counts)) {
      const node = rail?.querySelector('[data-pg-count="' + key + '"]');
      if (node) node.textContent = ' (' + value + ')';
    }
  }
  function writeFooter(str) {
    const pg = rail?.__ctoxPaneGrammar;
    if (pg && typeof pg.setFooter === 'function') { pg.setFooter(str); return; }
    const node = rail?.querySelector('[data-pg-footer]');
    if (node) node.textContent = str || '';
  }

  function renderDetail() {
    if (!detailEl) return;
    const rec = selectedId ? rowsCache.find((r) => r.id === selectedId) : null;
    const show = shouldRevealRecord(Boolean(rec), userCollapsed);
    detailEl.hidden = !show;
    detailEl.innerHTML = show ? recordDetailHtml(rec) : '';
    if (show) {
      detailEl.setAttribute('data-context-record-id', rec.id || '');
      detailEl.setAttribute('data-context-record-type', 'application');
      detailEl.setAttribute('data-context-label', candidateName(rec));
    }
    if (titleEl) titleEl.textContent = rec ? candidateName(rec) : text.newTitle;
  }

  function render() {
    const g = readGrammar();
    const filtered = filterRows(rowsCache, g);
    if (listEl) {
      const emptyText = rowsCache.length ? text.emptyFiltered : text.empty;
      listEl.innerHTML = renderRecordList(filtered, { view: g.view, selectedId, emptyText });
    }
    writeCounts(countsFor(rowsCache));
    const scope = g.band === 'all' ? text.bandAll : g.band === 'open' ? text.bandOpen : text.bandClosed;
    writeFooter(filtered.length + ' ' + text.entries + ' · ' + scope);
    renderDetail();
  }

  async function load() {
    const col = collection();
    let rows = [];
    if (col?.find) {
      try {
        const docs = await col.find({ selector: {} }).exec();
        rows = docs.map((d) => (typeof d.toJSON === 'function' ? d.toJSON() : d)).filter((r) => !r._deleted);
      } catch (e) { console.error('[intake] load failed:', e); }
    }
    rows.sort((a, b) => (Number(b.received_at_ms || b.created_at_ms || 0) - Number(a.received_at_ms || a.created_at_ms || 0)));
    rowsCache = rows;
  }

  function fillForm(rec) {
    if (!formEl) return;
    const candidate = rec && typeof rec.candidate === 'object' && rec.candidate ? rec.candidate : {};
    setField('name', candidate.name || rec.name || '');
    setField('email', candidate.email || rec.email || '');
    setField('phone', candidate.phone || rec.phone || '');
    setField('vacancy_id', rec.vacancy_id || '');
    setField('channel', rec.channel || 'email');
  }
  function setField(name, value) {
    const el = formEl?.querySelector('[name="' + name + '"]');
    if (el) el.value = value == null ? '' : String(value);
  }

  function selectRecord(id) {
    selectedId = id || null;
    userCollapsed = false;
    const rec = selectedId ? rowsCache.find((r) => r.id === selectedId) : null;
    if (rec) fillForm(rec);
    // Selection is an in-place class flip — a list rebuild resets the
    // operator's scroll (design-guide: re-renders never move the operator).
    applyListSelection();
    renderDetail();
  }

  function applyListSelection() {
    listEl?.querySelectorAll('[data-context-record-id]').forEach((row) => {
      const on = (row.getAttribute('data-context-record-id') || '') === String(selectedId || '');
      row.classList.toggle('is-selected', on);
      row.setAttribute('aria-selected', String(on));
    });
  }

  function startNew() {
    selectedId = null;
    userCollapsed = false;
    try { formEl?.reset(); } catch {}
    setGate('');
    render();
    formEl?.querySelector('[name="name"]')?.focus?.();
  }

  function exportRecords() {
    const filtered = filterRows(rowsCache, readGrammar());
    let url = '';
    try {
      const blob = new Blob([JSON.stringify(filtered, null, 2)], { type: 'application/json' });
      url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'intake-applications.json';
      a.rel = 'noopener';
      root?.appendChild(a);
      a.click();
      a.remove();
    } catch (e) {
      console.error('[intake] export failed:', e);
    } finally {
      if (url) setTimeout(() => { try { URL.revokeObjectURL(url); } catch {} }, 4000);
    }
  }

  function importRecords() {
    const col = collection();
    if (!col?.upsert) { setGate(text.offlineService, 'offline'); return; }
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'application/json,.json';
    input.addEventListener('change', async () => {
      const file = input.files && input.files[0];
      if (!file) return;
      let parsed;
      try { parsed = JSON.parse(await file.text()); } catch { setGate(text.importInvalid, 'block'); return; }
      const items = Array.isArray(parsed) ? parsed : (parsed && typeof parsed === 'object' ? [parsed] : []);
      if (!items.length) { setGate(text.importEmpty, 'block'); return; }
      const nowMs = Date.now();
      let count = 0;
      for (const raw of items) {
        try { await col.upsert(prepareImport(raw, nowMs, String(count) + Math.random().toString(36).slice(2, 6))); count += 1; }
        catch (e) { console.error('[intake] import failed:', e); }
      }
      setGate('<strong>' + esc(text.imported) + '</strong>: ' + count, 'ok');
      await load();
      render();
    });
    input.click();
  }

  async function onListClick(event) {
    const row = event.target?.closest?.('[data-context-record-id]');
    if (!row || !listEl.contains(row)) return;
    selectRecord(row.getAttribute('data-context-record-id'));
  }
  function onListKey(event) {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    const row = event.target?.closest?.('[data-context-record-id]');
    if (!row || !listEl.contains(row)) return;
    event.preventDefault();
    selectRecord(row.getAttribute('data-context-record-id'));
  }

  function onAction(event) {
    const btn = event.target?.closest?.('[data-action]');
    if (!btn) return;
    const action = btn.dataset.action;
    if (action === 'new') startNew();
    else if (action === 'import') importRecords();
    else if (action === 'export') exportRecords();
    else if (action === 'collapse-detail') { userCollapsed = true; renderDetail(); }
  }

  async function onSubmit(event) {
    event.preventDefault();
    setGate('');
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') {
      setGate(text.offlineService, 'offline');
      return;
    }
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const name = String(f.name == null ? '' : f.name).trim();
    if (!name) { setGate(text.nameRequired, 'block'); return; }
    const email = String(f.email == null ? '' : f.email).trim();
    const phone = String(f.phone == null ? '' : f.phone).trim();
    const vacancy_id = String(f.vacancy_id == null ? '' : f.vacancy_id).trim();
    const channel = String(f.channel == null ? '' : f.channel).trim() || 'email';
    const payload = {
      name,
      email: email || null,
      phone: phone || null,
      vacancy_id: vacancy_id || null,
      channel,
    };

    const submitBtn = formEl?.querySelector('button[type="submit"]');
    if (submitBtn) submitBtn.disabled = true;
    let result;
    try {
      result = await dispatch({
        module: MODULE_ID,
        command_type: CAPTURE_COMMAND,
        payload,
      });
    } catch (e) {
      console.error('[intake] dispatch failed:', e);
      setGate(text.offlineSend, 'offline');
      return;
    } finally {
      if (submitBtn) submitBtn.disabled = false;
    }

    const decision = result?.gate || result?.decision || null;
    const blockers = result?.blockers || decision?.blockers || result?.errors || null;
    const blocked = result?.ok === false || result?.status === 'blocked' || result?.allowed === false
      || decision?.status === 'blocked' || decision?.decision === 'block'
      || (Array.isArray(blockers) && blockers.length > 0);

    if (blocked) {
      const items = (Array.isArray(blockers) ? blockers : [blockers])
        .filter(Boolean)
        .map((b) => '<li>' + esc(typeof b === 'string' ? b : (b?.message || b?.reason || JSON.stringify(b))) + '</li>')
        .join('');
      setGate('<strong>' + esc(text.blocked) + '</strong>' + (items ? '<ul class="ats-blockers">' + items + '</ul>' : ''), 'block');
      return;
    }

    const appId = result?.application_id ?? result?.data?.application_id ?? null;
    const dedupeKey = result?.dedupe_key ?? result?.data?.dedupe_key ?? null;
    setGate(
      '<strong>' + esc(text.captured) + '</strong>'
      + '<div class="ats-result-row">' + esc(text.application) + ': ' + esc(appId ?? '—') + '</div>'
      + '<div class="ats-result-row">' + esc(text.dedupeKey) + ': ' + esc(dedupeKey ?? '—') + '</div>',
      'ok',
    );
    selectedId = null;
    try { formEl.reset(); } catch {}
    await load();
    render();
  }

  listEl?.addEventListener('click', onListClick);
  listEl?.addEventListener('keydown', onListKey);
  root?.addEventListener('click', onAction);
  formEl?.addEventListener('submit', onSubmit);
  // Re-render when the shell reports a grammar change (search / view / tray /
  // band). The event bubbles from the wired pane.
  const onGrammarChange = () => { render(); };
  rail?.addEventListener('ctox-pane-grammar-change', onGrammarChange);

  let sub = null;
  const col = collection();
  if (col?.find) {
    try { sub = col.find({ selector: {} }).$?.subscribe?.(() => { load().then(render).catch(() => {}); }); } catch {}
  }
  await load();
  render();

  return () => {
    try { sub?.unsubscribe?.(); } catch {}
    listEl?.removeEventListener('click', onListClick);
    listEl?.removeEventListener('keydown', onListKey);
    root?.removeEventListener('click', onAction);
    formEl?.removeEventListener('submit', onSubmit);
    rail?.removeEventListener('ctox-pane-grammar-change', onGrammarChange);
    ctx.host.replaceChildren();
    delete ctx.host.dataset.atsModule;
  };
}

// Maps an application status onto the kit badge states
// (hired/success, rejected/danger, in-flight/warning, new/info).
function statusBadgeClass(status) {
  if (status === 'hired') return 'is-success';
  if (status === 'rejected') return 'is-danger';
  if (status === 'screening' || status === 'duplicate') return 'is-warning';
  if (status === 'new') return 'is-info';
  return '';
}

function fmtDate(ms) {
  try { return new Date(ms).toLocaleString(locale === 'en' ? 'en' : 'de'); } catch { return ''; }
}

function applyStaticCopy(root) {
  root.querySelectorAll('[data-copy]').forEach((node) => { node.textContent = text[node.dataset.copy] || node.textContent; });
}

async function ensureStyles() {
  const href = new URL('./index.css', import.meta.url).pathname + '?v=' + MOD_BUILD;
  if (document.querySelector('link[href="' + href + '"]')) return;
  const link = document.createElement('link'); link.rel = 'stylesheet'; link.href = href; document.head.append(link);
}
async function loadMarkup() {
  const url = new URL('./index.html', import.meta.url).pathname + '?v=' + MOD_BUILD;
  const html = await fetch(url).then((r) => r.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((n) => n.remove());
  return doc.body.innerHTML;
}
function esc(v) { return String(v == null ? '' : v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
