import { isMeetingState, isNoShow } from './core/scheduling.js';
import { scoreScorecard, isScorecardComplete } from './core/scorecard.js';

const MOD_BUILD = '20260718-reduce1';
const MODULE_ID = 'interviews';
// Interview coordination has two record families, both plain RxDB collection
// writes (there is no native `ats.interview.*` business command — STT
// transcription is a native/skill effect that writes `transcript_id` directly).
// PRIMARY drives the form + subscription; scorecards are rendered alongside.
const PRIMARY = 'interview_meetings';
const SCORECARDS = 'interview_scorecards';
const TITLE = 'interviews';
const COPY = {
  de: {
    candidatePlaceholder: 'Kandidat-ID', vacancyPlaceholder: 'Vakanz-ID',
    partiesLabel: 'Parteien', partiesPlaceholder: 'Komma-getrennt', startLabel: 'Start', durationPlaceholder: 'Dauer (Min)', locationLabel: 'Ort',
    modeVideo: 'Video', modeOnsite: 'Vor Ort', modePhone: 'Telefon',
    videoLinkPlaceholder: 'Video-Link', createMeeting: 'Termin anlegen', meetings: 'Termine',
    scorecards: 'Scorecards', entriesEmpty: 'Noch keine Einträge.', meetingMissing: 'Termin nicht gefunden.',
    meetingUpdated: 'Termin aktualisiert.', updateFailed: 'Aktualisierung fehlgeschlagen.',
    databaseOffline: 'Offline: Datenbank nicht verfügbar.', candidateRequired: 'Kandidat-ID erforderlich.',
    meetingCreated: 'Termin angelegt.', meeting: 'Termin', status: 'Status', parties: 'Parteien',
    createFailed: 'Anlegen fehlgeschlagen', withoutSchedule: 'ohne Termin', party: 'Partei',
    link: 'Link', transcript: 'Transkript', confirm: 'Bestätigen', attended: 'Stattgefunden',
    noShow: 'No-Show', cancel: 'Absagen', criteria: 'Kriterien', score: 'Score',
    interviewer: 'Interviewer', complete: 'vollständig', open: 'offen', generic: 'allgemein',
    toggleForm: 'Neuer Termin', rowActions: 'Aktionen',
  },
  en: {
    candidatePlaceholder: 'Candidate ID', vacancyPlaceholder: 'Vacancy ID',
    partiesLabel: 'Participants', partiesPlaceholder: 'comma-separated', startLabel: 'Start', durationPlaceholder: 'Duration (min)', locationLabel: 'Location',
    modeVideo: 'Video', modeOnsite: 'On site', modePhone: 'Phone',
    videoLinkPlaceholder: 'Video link', createMeeting: 'Schedule meeting', meetings: 'Meetings',
    scorecards: 'Scorecards', entriesEmpty: 'No entries yet.', meetingMissing: 'Meeting not found.',
    meetingUpdated: 'Meeting updated.', updateFailed: 'Update failed.',
    databaseOffline: 'Offline: database unavailable.', candidateRequired: 'Candidate ID is required.',
    meetingCreated: 'Meeting scheduled.', meeting: 'Meeting', status: 'Status', parties: 'participants',
    createFailed: 'Could not schedule meeting', withoutSchedule: 'not scheduled', party: 'participant',
    link: 'Link', transcript: 'Transcript', confirm: 'Confirm', attended: 'Attended',
    noShow: 'No-show', cancel: 'Cancel', criteria: 'criteria', score: 'Score',
    interviewer: 'Interviewer', complete: 'complete', open: 'open', generic: 'generic',
    toggleForm: 'New meeting', rowActions: 'Actions',
  },
};

export async function mount(ctx) {
  await ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.host.dataset.atsModule = MODULE_ID;
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  const root = ctx.host.querySelector('[data-ats-root]');
  const locale = String(ctx.locale || document.documentElement.lang || 'de').toLowerCase().startsWith('en') ? 'en' : 'de';
  const copy = COPY[locale];
  const t = (key) => copy[key] || COPY.de[key] || key;
  root?.setAttribute('lang', locale);
  root?.querySelectorAll('[data-i18n]').forEach((node) => { node.textContent = t(node.dataset.i18n); });
  root?.querySelectorAll('[data-i18n-placeholder]').forEach((node) => { node.placeholder = t(node.dataset.i18nPlaceholder); });
  root?.querySelectorAll('[data-i18n-title]').forEach((node) => { node.title = t(node.dataset.i18nTitle); node.setAttribute('aria-label', t(node.dataset.i18nTitle)); });
  const listEl = root?.querySelector('[data-ats-list]');
  const countEl = root?.querySelector('[data-ats-count]');
  const formEl = root?.querySelector('[data-ats-form]');
  const toggleFormEl = root?.querySelector('[data-toggle-form]');
  const gateEl = root?.querySelector('[data-ats-gate]');
  const titleEl = root?.querySelector('[data-ats-title]');
  const subEl = root?.querySelector('[data-ats-sub]');
  if (titleEl) titleEl.textContent = ctx.manifest?.title || TITLE;
  if (subEl) subEl.textContent = ctx.manifest?.description || '';

  const collection = (name) => { try { return ctx.db?.collection?.(name) || null; } catch { return null; } };

  // Gate results render in the kit callout; kinds map onto its state variants.
  const GATE_KINDS = { ok: 'is-success', block: 'is-danger', offline: 'is-warning' };
  function setGate(html, kind) {
    if (!gateEl) return;
    gateEl.className = 'ctox-callout' + (GATE_KINDS[kind] ? ' ' + GATE_KINDS[kind] : '');
    gateEl.innerHTML = html || '';
    gateEl.hidden = !html;
  }

  async function loadRows(name) {
    const col = collection(name);
    if (!col?.find) return [];
    try {
      const docs = await col.find({ selector: {} }).exec();
      return docs.map((d) => (typeof d.toJSON === 'function' ? d.toJSON() : d)).filter((r) => !r._deleted);
    } catch (e) { console.error('[interviews] load failed:', name, e); return []; }
  }

  async function render() {
    const now = Date.now();
    const [meetings, scorecards] = await Promise.all([loadRows(PRIMARY), loadRows(SCORECARDS)]);
    if (countEl) countEl.textContent = meetings.length + ' ' + t('meetings') + ' · ' + scorecards.length + ' ' + t('scorecards');
    const parts = [];
    if (meetings.length) {
      parts.push('<div class="ats-group-head">' + esc(t('meetings')) + '</div>');
      parts.push(meetings.map((r) => meetingRow(r, now, t, locale)).join(''));
    }
    if (scorecards.length) {
      parts.push('<div class="ats-group-head">' + esc(t('scorecards')) + '</div>');
      parts.push(scorecards.map((r) => scorecardRow(r, t)).join(''));
    }
    if (listEl) listEl.innerHTML = parts.length ? parts.join('') : '<div class="ctox-empty">' + esc(t('entriesEmpty')) + '</div>';
  }

  // Meeting state transitions are plain RxDB writes — the scheduling engine owns
  // the valid MEETING_STATES, not a native command.
  async function transitionMeeting(meetingId, state, patch) {
    const col = collection(PRIMARY);
    if (!col?.find || !isMeetingState(state)) return;
    setGate('');
    try {
      const doc = await col.findOne(meetingId).exec();
      if (!doc) { setGate(t('meetingMissing'), 'block'); return; }
      const now = Date.now();
      await doc.patch(Object.assign({ state, updated_at_ms: now }, patch || {}));
      setGate('<strong>' + esc(t('meetingUpdated')) + '</strong><div class="ats-result-row">' + esc(meetingId) + ' → ' + esc(state) + '</div>', 'ok');
      await render();
    } catch (e) {
      console.error('[interviews] transition failed:', e);
      setGate(t('updateFailed'), 'offline');
    }
  }

  async function onListClick(event) {
    // The per-row "…" toggle reveals that record's action cluster on demand;
    // rows default to badge + body only.
    const toggle = event.target?.closest?.('[data-meeting-actions-toggle]');
    if (toggle) {
      const row = toggle.closest('.ats-item');
      const open = row?.classList.toggle('is-actions-open') || false;
      toggle.setAttribute('aria-expanded', open ? 'true' : 'false');
      return;
    }
    const btn = event.target?.closest?.('[data-meeting-action]');
    if (!btn) return;
    const meetingId = btn.getAttribute('data-meeting-id');
    const action = btn.getAttribute('data-meeting-action');
    if (!meetingId || !action) return;
    if (action === 'confirmed') return transitionMeeting(meetingId, 'confirmed');
    if (action === 'completed') return transitionMeeting(meetingId, 'completed', { attended: true });
    if (action === 'no_show') return transitionMeeting(meetingId, 'no_show', { attended: false });
    if (action === 'cancelled') return transitionMeeting(meetingId, 'cancelled');
  }
  listEl?.addEventListener('click', onListClick);

  async function onSubmit(event) {
    event.preventDefault();
    setGate('');
    const col = collection(PRIMARY);
    if (!col?.insert) { setGate(t('databaseOffline'), 'offline'); return; }
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const candidate_id = String(f.candidate_id || '').trim();
    if (!candidate_id) { setGate(t('candidateRequired'), 'block'); return; }

    const now = Date.now();
    const startMs = f.start ? Date.parse(f.start) : NaN;
    const durationMin = f.duration_min === '' || f.duration_min == null ? null : Number(f.duration_min);
    const start = Number.isFinite(startMs) ? startMs : null;
    const end = start != null && Number.isFinite(durationMin) ? start + durationMin * 60_000 : null;
    const parties = String(f.parties || '')
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean)
      .map((name) => ({ name }));

    const record = {
      id: 'imeet_' + now + '_' + Math.round(now % 1e6),
      candidate_id,
      vacancy_id: String(f.vacancy_id || '').trim() || null,
      parties,
      start,
      end,
      location_mode: String(f.location_mode || 'video').trim() || null,
      video_link: String(f.video_link || '').trim() || null,
      state: 'proposed',
      attended: false,
      created_at_ms: now,
      updated_at_ms: now,
      _deleted: false,
    };

    try {
      await col.insert(record);
      setGate(
        '<strong>' + esc(t('meetingCreated')) + '</strong>'
        + '<div class="ats-result-row">' + esc(t('meeting')) + ': ' + esc(record.id) + '</div>'
        + '<div class="ats-result-row">' + esc(t('status')) + ': ' + esc(record.state) + (parties.length ? ' · ' + parties.length + ' ' + esc(t('parties')) : '') + '</div>',
        'ok'
      );
      try { formEl.reset(); } catch {}
      await render();
    } catch (e) {
      console.error('[interviews] insert failed:', e);
      setGate(t('createFailed') + ': ' + esc(e?.message || e), 'block');
    }
  }
  formEl?.addEventListener('submit', onSubmit);

  // Create form is hidden by default (is-form-hidden on root); the header
  // toggle reveals it on demand, consent's data-toggle-rights idiom.
  function onToggleForm() {
    const hidden = root?.classList.toggle('is-form-hidden');
    toggleFormEl?.setAttribute('aria-pressed', hidden ? 'false' : 'true');
    if (!hidden) formEl?.querySelector('input')?.focus?.();
  }
  toggleFormEl?.addEventListener('click', onToggleForm);

  const subs = [];
  for (const name of [PRIMARY, SCORECARDS]) {
    const col = collection(name);
    if (col?.find) { try { const s = col.find({ selector: {} }).$?.subscribe?.(() => { render().catch(() => {}); }); if (s) subs.push(s); } catch {} }
  }
  await render();

  return () => {
    for (const s of subs) { try { s?.unsubscribe?.(); } catch {} }
    formEl?.removeEventListener('submit', onSubmit);
    toggleFormEl?.removeEventListener('click', onToggleForm);
    listEl?.removeEventListener('click', onListClick);
    ctx.host.replaceChildren();
    delete ctx.host.dataset.atsModule;
  };
}

// Maps a meeting/scorecard state onto the kit badge states
// (completed/success, no_show/danger, waiting/warning, confirmed/info,
// cancelled/neutral).
function statusBadgeClass(state) {
  if (state === 'completed') return 'is-success';
  if (state === 'no_show') return 'is-danger';
  if (state === 'proposed' || state === 'rescheduled') return 'is-warning';
  if (state === 'confirmed') return 'is-info';
  return '';
}

function badgeSpan(state, label) {
  const cls = ('ctox-badge ' + statusBadgeClass(state)).trim();
  return '<span class="' + cls + '" data-status="' + esc(state) + '">' + esc(label == null ? state : label) + '</span>';
}

function meetingRow(r, nowMs, t, locale) {
  let state = String(r.state || 'proposed');
  if (isNoShow(r, nowMs) && state !== 'no_show' && state !== 'cancelled') state = 'no_show';
  const badge = isMeetingState(state) ? state : 'proposed';
  const cand = esc(r.candidate_id || '—');
  const vac = r.vacancy_id ? ' → ' + esc(r.vacancy_id) : '';
  const partyCount = Array.isArray(r.parties) ? r.parties.length : 0;
  const when = r.start ? fmtTime(r.start, locale) : t('withoutSchedule');
  const metaBits = [
    esc(r.id || ''),
    when,
    esc(r.location_mode || 'video'),
    partyCount + ' ' + (partyCount === 1 ? t('party') : t('parties')),
  ];
  if (r.video_link) metaBits.push(t('link'));
  if (r.transcript_id) metaBits.push(t('transcript') + ': ' + esc(r.transcript_id));
  const actions = [];
  if (state === 'proposed' || state === 'rescheduled') actions.push(actionBtn(r.id, 'confirmed', t('confirm')));
  if (state !== 'completed' && state !== 'cancelled') {
    actions.push(actionBtn(r.id, 'completed', t('attended')));
    actions.push(actionBtn(r.id, 'no_show', t('noShow')));
  }
  if (state !== 'cancelled' && state !== 'completed') actions.push(actionBtn(r.id, 'cancelled', t('cancel')));
  return ''
    + '<div class="ats-item ats-item--rich" data-context-record-id="' + esc(r.id || '') + '" data-context-record-type="interview_meeting" data-context-label="' + cand + '">'
    + '<div class="ats-item-body">'
    + '<div class="ats-item-main">' + badgeSpan(badge) + ' ' + cand + vac + '</div>'
    + '<div class="ats-item-meta">' + metaBits.map(esc).join(' · ') + '</div>'
    + '</div>'
    + (actions.length
      ? '<div class="ats-item-trail">' + actionsToggleBtn(r.id, t('rowActions')) + '<div class="ats-actions">' + actions.join('') + '</div></div>'
      : '')
    + '</div>';
}

function scorecardRow(r, t) {
  // Recompute the weighted score from the engine so the row reflects the real
  // criteria/ratings, not just a stored `overall`.
  let computed = null;
  try { computed = scoreScorecard(r, r.ratings); } catch { computed = null; }
  const overall = computed ? computed.overall : (Number.isFinite(Number(r.overall)) ? Number(r.overall) : 0);
  const complete = computed ? computed.complete : isScorecardComplete(r, r.ratings);
  const badge = complete ? 'completed' : 'proposed';
  const cand = esc(r.candidate_id || '—');
  const vac = r.vacancy_id ? ' → ' + esc(r.vacancy_id) : '';
  const critCount = Array.isArray(r.criteria) ? r.criteria.length : 0;
  const metaBits = [
    esc(r.id || ''),
    esc(r.role_template || t('generic')),
    critCount + ' ' + t('criteria'),
    t('score') + ' ' + overall + '/100',
  ];
  if (r.interviewer) metaBits.push(t('interviewer') + ': ' + esc(r.interviewer));
  return ''
    + '<div class="ats-item ats-item--rich" data-context-record-id="' + esc(r.id || '') + '" data-context-record-type="interview_scorecard" data-context-label="' + cand + '">'
    + '<div class="ats-item-body">'
    + '<div class="ats-item-main">' + badgeSpan(badge, complete ? t('complete') : t('open')) + ' ' + cand + vac + '</div>'
    + '<div class="ats-item-meta">' + metaBits.map(esc).join(' · ') + '</div>'
    + '</div>'
    + '<div class="ats-score">' + overall + '</div>'
    + '</div>';
}

function actionBtn(meetingId, action, label) {
  return '<button type="button" class="ctox-button" data-meeting-action="' + esc(action) + '" data-meeting-id="' + esc(meetingId || '') + '">' + esc(label) + '</button>';
}

// Per-row "…" disclosure that reveals the record's state-transition actions.
function actionsToggleBtn(meetingId, label) {
  return '<button type="button" class="ctox-pane-icon" data-meeting-actions-toggle="' + esc(meetingId || '') + '" title="' + esc(label) + '" aria-label="' + esc(label) + '" aria-expanded="false">'
    + '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><circle cx="5" cy="12" r="1.7"/><circle cx="12" cy="12" r="1.7"/><circle cx="19" cy="12" r="1.7"/></svg>'
    + '</button>';
}

function fmtTime(ms, locale) {
  const n = Number(ms);
  if (!Number.isFinite(n)) return 'ohne Termin';
  try { return new Date(n).toLocaleString(locale === 'en' ? 'en-US' : 'de-DE', { dateStyle: 'short', timeStyle: 'short' }); }
  catch { return new Date(n).toISOString(); }
}

async function ensureStyles() {
  const href = new URL('./index.css', import.meta.url).pathname + '?v=' + MOD_BUILD;
  if (document.querySelector('link[href="' + href + '"]')) return;
  const link = document.createElement('link'); link.rel = 'stylesheet'; link.href = href; document.head.append(link);
}
async function loadMarkup() {
  const html = await fetch(new URL('./index.html', import.meta.url)).then((r) => r.text());
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, link[rel="stylesheet"]').forEach((n) => n.remove());
  return doc.body.innerHTML;
}
function esc(v) { return String(v == null ? '' : v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;'); }
