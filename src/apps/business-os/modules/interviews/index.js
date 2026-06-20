import { isMeetingState, isNoShow } from './core/scheduling.js';
import { scoreScorecard, isScorecardComplete } from './core/scorecard.js';

const MOD_BUILD = '20260620-ats9';
const MODULE_ID = 'interviews';
// Interview coordination has two record families, both plain RxDB collection
// writes (there is no native `ats.interview.*` business command — STT
// transcription is a native/skill effect that writes `transcript_id` directly).
// PRIMARY drives the form + subscription; scorecards are rendered alongside.
const PRIMARY = 'interview_meetings';
const SCORECARDS = 'interview_scorecards';
const TITLE = 'interviews';

export async function mount(ctx) {
  await ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.host.dataset.atsModule = MODULE_ID;
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  const root = ctx.host.querySelector('[data-ats-root]');
  const listEl = root?.querySelector('[data-ats-list]');
  const countEl = root?.querySelector('[data-ats-count]');
  const formEl = root?.querySelector('[data-ats-form]');
  const gateEl = root?.querySelector('[data-ats-gate]');
  const titleEl = root?.querySelector('[data-ats-title]');
  const subEl = root?.querySelector('[data-ats-sub]');
  if (titleEl) titleEl.textContent = ctx.manifest?.title || TITLE;
  if (subEl) subEl.textContent = ctx.manifest?.description || '';

  const collection = (name) => { try { return ctx.db?.collection?.(name) || ctx.db?.[name] || null; } catch { return null; } };

  function setGate(html, kind) {
    if (!gateEl) return;
    gateEl.className = 'ats-gate' + (kind ? ' ats-gate--' + kind : '');
    gateEl.innerHTML = html || '';
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
    if (countEl) countEl.textContent = meetings.length + ' Termine · ' + scorecards.length + ' Scorecards';
    const parts = [];
    if (meetings.length) {
      parts.push('<div class="ats-group-head">Termine</div>');
      parts.push(meetings.map((r) => meetingRow(r, now)).join(''));
    }
    if (scorecards.length) {
      parts.push('<div class="ats-group-head">Scorecards</div>');
      parts.push(scorecards.map((r) => scorecardRow(r)).join(''));
    }
    if (listEl) listEl.innerHTML = parts.length ? parts.join('') : '<div class="ats-empty">Noch keine Einträge.</div>';
  }

  // Meeting state transitions are plain RxDB writes — the scheduling engine owns
  // the valid MEETING_STATES, not a native command.
  async function transitionMeeting(meetingId, state, patch) {
    const col = collection(PRIMARY);
    if (!col?.find || !isMeetingState(state)) return;
    setGate('');
    try {
      const doc = await col.findOne(meetingId).exec();
      if (!doc) { setGate('Termin nicht gefunden.', 'block'); return; }
      const now = Date.now();
      await doc.patch(Object.assign({ state, updated_at_ms: now }, patch || {}));
      setGate('<strong>Termin aktualisiert.</strong><div class="ats-result-row">' + esc(meetingId) + ' → ' + esc(state) + '</div>', 'ok');
      await render();
    } catch (e) {
      console.error('[interviews] transition failed:', e);
      setGate('Aktualisierung fehlgeschlagen.', 'offline');
    }
  }

  async function onListClick(event) {
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
    if (!col?.insert) { setGate('Offline: Datenbank nicht verfügbar.', 'offline'); return; }
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const candidate_id = String(f.candidate_id || '').trim();
    if (!candidate_id) { setGate('Kandidat-ID erforderlich.', 'block'); return; }

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
        '<strong>Termin angelegt.</strong>'
        + '<div class="ats-result-row">Termin: ' + esc(record.id) + '</div>'
        + '<div class="ats-result-row">Status: ' + esc(record.state) + (parties.length ? ' · ' + parties.length + ' Parteien' : '') + '</div>',
        'ok'
      );
      try { formEl.reset(); } catch {}
      await render();
    } catch (e) {
      console.error('[interviews] insert failed:', e);
      setGate('Anlegen fehlgeschlagen: ' + esc(e?.message || e), 'block');
    }
  }
  formEl?.addEventListener('submit', onSubmit);

  const subs = [];
  for (const name of [PRIMARY, SCORECARDS]) {
    const col = collection(name);
    if (col?.find) { try { const s = col.find({ selector: {} }).$?.subscribe?.(() => { render().catch(() => {}); }); if (s) subs.push(s); } catch {} }
  }
  await render();

  return () => {
    for (const s of subs) { try { s?.unsubscribe?.(); } catch {} }
    formEl?.removeEventListener('submit', onSubmit);
    listEl?.removeEventListener('click', onListClick);
    ctx.host.replaceChildren();
    delete ctx.host.dataset.atsModule;
  };
}

function meetingRow(r, nowMs) {
  let state = String(r.state || 'proposed');
  if (isNoShow(r, nowMs) && state !== 'no_show' && state !== 'cancelled') state = 'no_show';
  const badge = isMeetingState(state) ? state : 'proposed';
  const cand = esc(r.candidate_id || '—');
  const vac = r.vacancy_id ? ' → ' + esc(r.vacancy_id) : '';
  const partyCount = Array.isArray(r.parties) ? r.parties.length : 0;
  const when = r.start ? fmtTime(r.start) : 'ohne Termin';
  const metaBits = [
    esc(r.id || ''),
    when,
    esc(r.location_mode || 'video'),
    partyCount + (partyCount === 1 ? ' Partei' : ' Parteien'),
  ];
  if (r.video_link) metaBits.push('Link');
  if (r.transcript_id) metaBits.push('Transkript: ' + esc(r.transcript_id));
  const actions = [];
  if (state === 'proposed' || state === 'rescheduled') actions.push(actionBtn(r.id, 'confirmed', 'Bestätigen'));
  if (state !== 'completed' && state !== 'cancelled') {
    actions.push(actionBtn(r.id, 'completed', 'Stattgefunden'));
    actions.push(actionBtn(r.id, 'no_show', 'No-Show'));
  }
  if (state !== 'cancelled' && state !== 'completed') actions.push(actionBtn(r.id, 'cancelled', 'Absagen'));
  return ''
    + '<div class="ats-item ats-item--rich">'
    + '<div class="ats-item-body">'
    + '<div class="ats-item-main"><span class="ats-badge ats-badge--' + esc(badge) + '">' + esc(badge) + '</span> ' + cand + vac + '</div>'
    + '<div class="ats-item-meta">' + metaBits.map(esc).join(' · ') + '</div>'
    + '</div>'
    + (actions.length ? '<div class="ats-actions">' + actions.join('') + '</div>' : '')
    + '</div>';
}

function scorecardRow(r) {
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
    esc(r.role_template || 'generic'),
    critCount + ' Kriterien',
    'Score ' + overall + '/100',
  ];
  if (r.interviewer) metaBits.push('Interviewer: ' + esc(r.interviewer));
  return ''
    + '<div class="ats-item ats-item--rich">'
    + '<div class="ats-item-body">'
    + '<div class="ats-item-main"><span class="ats-badge ats-badge--' + esc(badge) + '">' + (complete ? 'vollständig' : 'offen') + '</span> ' + cand + vac + '</div>'
    + '<div class="ats-item-meta">' + metaBits.map(esc).join(' · ') + '</div>'
    + '</div>'
    + '<div class="ats-score">' + overall + '</div>'
    + '</div>';
}

function actionBtn(meetingId, action, label) {
  return '<button type="button" class="ats-action" data-meeting-action="' + esc(action) + '" data-meeting-id="' + esc(meetingId || '') + '">' + esc(label) + '</button>';
}

function fmtTime(ms) {
  const n = Number(ms);
  if (!Number.isFinite(n)) return 'ohne Termin';
  try { return new Date(n).toLocaleString('de-DE', { dateStyle: 'short', timeStyle: 'short' }); }
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
function esc(v) { return String(v == null ? '' : v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
