const MOD_BUILD = '20260706-kit1';
const MODULE_ID = 'submissions';
const PRIMARY = 'submissions';
const TITLE = 'submissions';
const PRESENT_COMMAND = 'ats.submission.present';

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

  let rowsCache = [];
  const collection = () => { try { return ctx.db?.collection?.(PRIMARY) || null; } catch { return null; } };

  function setGate(html, kind) {
    if (!gateEl) return;
    gateEl.className = 'ats-gate' + (kind ? ' ats-gate--' + kind : '');
    gateEl.innerHTML = html || '';
  }

  async function render() {
    const col = collection();
    let rows = [];
    if (col?.find) {
      try { const docs = await col.find({ selector: {} }).exec(); rows = docs.map((d) => (typeof d.toJSON === 'function' ? d.toJSON() : d)).filter((r) => !r._deleted); }
      catch (e) { console.error('[submissions] load failed:', e); }
    }
    rows.sort((a, b) => Number(b?.sent_at_ms || b?.created_at_ms || 0) - Number(a?.sent_at_ms || a?.created_at_ms || 0));
    rowsCache = rows;
    if (countEl) countEl.textContent = rows.length + ' Einträge';
    if (listEl) listEl.innerHTML = rows.length ? rows.map((r) => submissionRow(r)).join('') : '<div class="ctox-empty">Noch keine Einträge.</div>';
  }

  // The submissions module has exactly one native command (ats.submission.present);
  // there is no native withdraw/feedback handler, so the only per-row action is a
  // local, non-dispatching id copy — we never fabricate a server command.
  async function onListClick(event) {
    const copyBtn = event.target?.closest?.('[data-copy-id]');
    if (copyBtn) {
      const id = copyBtn.getAttribute('data-copy-id') || '';
      if (id) { try { await navigator.clipboard?.writeText?.(id); } catch {} setGate('<strong>ID kopiert:</strong> <span class="ats-result-row">' + esc(id) + '</span>', 'ok'); }
    }
  }
  listEl?.addEventListener('click', onListClick);

  async function onSubmit(event) {
    event.preventDefault();
    setGate('');
    const dispatch = ctx.commandBus?.dispatch;
    if (typeof dispatch !== 'function') {
      setGate('Offline: Befehlsdienst nicht verfügbar.', 'offline');
      return;
    }
    const data = new FormData(formEl);
    const f = Object.fromEntries(data.entries());
    const candidate_id = String(f.candidate_id || '').trim();
    const client_account_id = String(f.client_account_id || '').trim();
    if (!candidate_id || !client_account_id) {
      setGate('<strong>Blockiert.</strong><div class="ats-result-row">Kandidat-ID und Kunden-Account-ID sind erforderlich.</div>', 'block');
      return;
    }
    const payload = {
      candidate_id,
      client_account_id,
      vacancy_id: String(f.vacancy_id || '').trim() || null,
      client_contact_id: String(f.client_contact_id || '').trim() || null,
    };

    let outcome;
    try {
      outcome = await ctx.commandBus?.dispatch?.({
        module: MODULE_ID,
        type: PRESENT_COMMAND,
        command_type: PRESENT_COMMAND,
        payload,
      });
    } catch (e) {
      console.error('[submissions] present dispatch failed:', e);
      setGate('Offline: Befehl konnte nicht gesendet werden.', 'offline');
      return;
    }

    const result = outcome?.result ?? outcome ?? {};
    const decision = result?.gate || result?.decision || null;
    const blockers = result?.blockers || decision?.blockers || result?.errors || null;
    const blocked = result?.ok === false
      || result?.allowed === false
      || result?.status === 'blocked'
      || decision?.decision === 'block'
      || (Array.isArray(blockers) && blockers.length > 0);

    if (blocked) {
      const items = (Array.isArray(blockers) ? blockers : [blockers])
        .filter(Boolean)
        .map((b) => '<li>' + esc(blockerText(b)) + '</li>')
        .join('');
      setGate('<strong>Blockiert.</strong>' + (items ? '<ul class="ats-blockers">' + items + '</ul>' : ''), 'block');
      return;
    }

    const submissionId = result?.submission_id ?? result?.data?.submission_id ?? null;
    setGate(
      '<strong>Vorgestellt.</strong>'
      + '<div class="ats-result-row">Submission: ' + esc(submissionId ?? '—') + '</div>',
      'ok'
    );
    try { formEl.reset(); } catch {}
    await render();
  }
  formEl?.addEventListener('submit', onSubmit);

  let sub = null;
  const col = collection();
  if (col?.find) { try { sub = col.find({ selector: {} }).$?.subscribe?.(() => { render().catch(() => {}); }); } catch {} }
  await render();

  return () => { try { sub?.unsubscribe?.(); } catch {} formEl?.removeEventListener('submit', onSubmit); listEl?.removeEventListener('click', onListClick); ctx.host.replaceChildren(); delete ctx.host.dataset.atsModule; };
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

function blockerText(b) {
  if (b == null) return 'unknown';
  if (typeof b === 'string') return b;
  const reason = b.reason || b.message || 'unknown';
  if (b.conflicting_submission_id) return reason + ' (Konflikt: ' + b.conflicting_submission_id + ')';
  return reason;
}

function fmtTime(ms) {
  const n = Number(ms);
  if (!Number.isFinite(n) || n <= 0) return '';
  try { return new Date(n).toLocaleString(); } catch { return ''; }
}

// Status → kit badge state (base.css .ctox-badge modifiers).
function badgeStateClass(status) {
  switch (status) {
    case 'sent':
    case 'hired':
      return ' is-success';
    case 'withdrawn':
      return ' is-warning';
    case 'rejected':
      return ' is-danger';
    default:
      return '';
  }
}

function submissionRow(r) {
  const status = String(r?.status || 'sent');
  const sentAt = fmtTime(r?.sent_at_ms || r?.created_at_ms);
  const feedbackOutcome = r?.feedback && typeof r.feedback === 'object' ? r.feedback.outcome : '';
  const meta = [];
  meta.push('Submission: ' + esc(r?.id || '—'));
  if (r?.vacancy_id) meta.push('Vakanz: ' + esc(r.vacancy_id));
  if (r?.client_contact_id) meta.push('Kontakt: ' + esc(r.client_contact_id));
  if (r?.consent_id) meta.push('Consent: ' + esc(r.consent_id));
  if (feedbackOutcome) meta.push('Feedback: ' + esc(feedbackOutcome));
  if (sentAt) meta.push('Gesendet: ' + esc(sentAt));
  const main = esc(r?.candidate_id || '—') + ' &rarr; ' + esc(r?.client_account_id || '—');
  const action = r?.id
    ? '<div class="ats-actions"><button type="button" class="ctox-button" data-copy-id="' + esc(r.id) + '">ID kopieren</button></div>'
    : '';
  const ctxLabel = r?.candidate_id || r?.id || '';
  return ''
    + '<div class="ats-item ats-item--rich"'
    + ' data-context-record-id="' + esc(r?.id || '') + '"'
    + ' data-context-record-type="submission"'
    + ' data-context-label="' + esc(ctxLabel) + '">'
    + '<div class="ats-item-main">'
    + '<span class="ctox-badge' + badgeStateClass(status) + '">' + esc(status) + '</span>'
    + '<span class="ats-item-title">' + main + '</span>'
    + '<div class="ats-item-meta">' + meta.join(' · ') + '</div>'
    + '</div>'
    + action
    + '</div>';
}

function esc(v) { return String(v == null ? '' : v).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
