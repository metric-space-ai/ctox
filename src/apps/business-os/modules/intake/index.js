const MOD_BUILD = '20260620-ats9';
const MODULE_ID = 'intake';
const PRIMARY = 'applications';
const TITLE = 'intake';
const CAPTURE_COMMAND = 'ats.intake.capture';

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
  const collection = () => { try { return ctx.db?.collection?.(PRIMARY) || ctx.db?.[PRIMARY] || null; } catch { return null; } };

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
      catch (e) { console.error('[intake] load failed:', e); }
    }
    rows.sort((a, b) => (Number(b.received_at_ms || b.created_at_ms || 0) - Number(a.received_at_ms || a.created_at_ms || 0)));
    rowsCache = rows;
    if (countEl) countEl.textContent = rows.length + ' Einträge';
    if (listEl) listEl.innerHTML = rows.length ? rows.map((r) => applicationRow(r)).join('') : '<div class="ats-empty">Noch keine Einträge.</div>';
  }

  // Delegated row-action handler. The intake capture engine exposes no
  // per-record native command, so rows carry no command buttons today; the
  // handler stays wired (and is removed in cleanup) for forward-compat with
  // secondary actions added to applicationRow().
  async function onListClick(event) {
    const btn = event.target?.closest?.('[data-action]');
    if (!btn) return;
    // No native row command yet — nothing to dispatch.
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
    const name = String(f.name == null ? '' : f.name).trim();
    if (!name) { setGate('Name ist erforderlich.', 'block'); return; }
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
        type: CAPTURE_COMMAND,
        command_type: CAPTURE_COMMAND,
        payload,
      });
    } catch (e) {
      console.error('[intake] dispatch failed:', e);
      setGate('Offline: Befehl konnte nicht gesendet werden.', 'offline');
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
      setGate('<strong>Eingang blockiert.</strong>' + (items ? '<ul class="ats-blockers">' + items + '</ul>' : ''), 'block');
      return;
    }

    const appId = result?.application_id ?? result?.data?.application_id ?? null;
    const dedupeKey = result?.dedupe_key ?? result?.data?.dedupe_key ?? null;
    setGate(
      '<strong>Bewerbung erfasst.</strong>'
      + '<div class="ats-result-row">Application: ' + esc(appId ?? '—') + '</div>'
      + '<div class="ats-result-row">Dedupe-Key: ' + esc(dedupeKey ?? '—') + '</div>',
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

  return () => {
    try { sub?.unsubscribe?.(); } catch {}
    formEl?.removeEventListener('submit', onSubmit);
    listEl?.removeEventListener('click', onListClick);
    ctx.host.replaceChildren();
    delete ctx.host.dataset.atsModule;
  };
}

function applicationRow(r) {
  const candidate = r && typeof r.candidate === 'object' && r.candidate ? r.candidate : {};
  const name = candidate.name || r.name || '(ohne Namen)';
  const email = candidate.email || r.email || '';
  const phone = candidate.phone || r.phone || '';
  const status = String(r.status || 'new');
  const channel = r.channel || '—';
  const id = r.id || '';
  const dedupe = r.dedupe_key || '';
  const vacancy = r.vacancy_id || '';
  const docs = Array.isArray(r.documents) ? r.documents.length : 0;
  const ts = Number(r.received_at_ms || r.created_at_ms || 0);

  const contact = [email, phone].filter(Boolean).map(esc).join(' · ');
  const metaParts = [];
  if (id) metaParts.push('<span class="ats-tag">' + esc(id) + '</span>');
  if (dedupe) metaParts.push('Dedupe: ' + esc(dedupe));
  if (vacancy) metaParts.push('Vakanz: ' + esc(vacancy));
  if (docs) metaParts.push(docs + ' Dok.');
  if (ts) metaParts.push(esc(fmtDate(ts)));

  const label = candidate.name || r.name || id;

  return '<div class="ats-item ats-item--rich"'
    + ' data-context-record-id="' + esc(id) + '"'
    + ' data-context-record-type="application"'
    + ' data-context-label="' + esc(label) + '">'
    + '<div class="ats-item-body">'
    + '<div class="ats-item-main">'
    + '<span class="ats-badge ats-badge--' + esc(status) + '">' + esc(status) + '</span>'
    + '<span class="ats-channel">' + esc(channel) + '</span>'
    + '<span class="ats-name">' + esc(name) + '</span>'
    + '</div>'
    + (contact ? '<div class="ats-item-sub">' + contact + '</div>' : '')
    + '<div class="ats-item-meta">' + metaParts.join(' · ') + '</div>'
    + '</div>'
    + '</div>';
}

function fmtDate(ms) {
  try { return new Date(ms).toISOString().slice(0, 16).replace('T', ' '); } catch { return ''; }
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
