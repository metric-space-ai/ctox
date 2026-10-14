const BUILD = '20260830-kundenpipeline-decision-hub-v1';
const MODULE_ID = 'kundenpipeline';
const DECISIONS = 'kundenpipeline_entscheidungen';
const VORGAENGE = 'kundenpipeline_vorgaenge';
const PROJECTS = 'kundenpipeline_projekte';

const state = {
  ctx: null,
  decisions: [],
  vorgaenge: [],
  projekte: [],
  selectedId: '',
  filter: 'open',
  search: '',
  loading: false,
  error: '',
  commandStatus: '',
  commandError: false,
  cleanup: [],
};

function plainRecord(record) {
  return typeof record?.toJSON === 'function' ? record.toJSON() : record;
}

async function readCollection(name) {
  const collection = state.ctx?.db?.collection?.(name);
  if (!collection?.find) return [];
  const result = await collection.find().exec();
  return (Array.isArray(result) ? result : []).map(plainRecord).filter(Boolean);
}

export function normalizeDecision(decision = {}) {
  const question = decision.frage_json?.question || decision.question || '';
  const context = decision.frage_json?.context || decision.context || '';
  const options = Array.isArray(decision.aktionen_json) ? decision.aktionen_json : [];
  const status = String(decision.status || '').toLowerCase();
  const expiresAt = Number(decision.expires_at_ms || 0);
  return {
    ...decision,
    id: String(decision.id || ''),
    title: decision.titel || decision.title || decision.typ || 'Entscheidung',
    question,
    context,
    options,
    status,
    expired: status === 'offen' && expiresAt > 0 && expiresAt <= Date.now(),
  };
}

export function filterDecisions(decisions, { filter = 'open', search = '' } = {}) {
  const needle = String(search || '').trim().toLocaleLowerCase();
  return decisions
    .map(normalizeDecision)
    .filter((decision) => decision.is_deleted !== true)
    .filter((decision) => filter !== 'open' || (decision.status === 'offen' && !decision.expired))
    .filter((decision) => {
      if (!needle) return true;
      const haystack = [
        decision.title,
        decision.typ,
        decision.question,
        decision.context,
        decision.source_json?.authority,
      ].join(' ').toLocaleLowerCase();
      return haystack.includes(needle);
    })
    .sort((a, b) => Number(a.created_at_ms || 0) - Number(b.created_at_ms || 0));
}

export function decisionCommand(decision, optionId, { comment = '', channel = 'desktop' } = {}) {
  const item = normalizeDecision(decision);
  const isAgentDecision = item.typ === 'agent_escalation';
  return {
    module: MODULE_ID,
    command_type: isAgentDecision
      ? 'kundenpipeline.decision.resolve'
      : 'kundenpipeline.decision.answer',
    record_id: item.id,
    payload: isAgentDecision
      ? {
          entscheidung_id: item.id,
          option_id: optionId,
          comment,
          kanal: channel,
        }
      : {
          entscheidung_id: item.id,
          vorgang_id: item.vorgang_id || '',
          wert: optionId,
          kanal: channel,
        },
    client_context: {
      build: BUILD,
      surface: isAgentDecision
        ? 'kundenpipeline.decision.resolve'
        : 'kundenpipeline.decision.answer',
      visible_scope: { app: { module_id: MODULE_ID, app_id: MODULE_ID } },
    },
  };
}

function commandId() {
  return typeof globalThis.crypto?.randomUUID === 'function'
    ? `cmd_${globalThis.crypto.randomUUID()}`
    : `cmd_kundenpipeline_${Date.now()}_${Math.random().toString(36).slice(2)}`;
}

async function loadMarkup() {
  const response = await fetch(new URL('./index.html', import.meta.url));
  return response.text();
}

async function ensureStyles() {
  if (document.querySelector('link[data-kundenpipeline-style]')) return;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.dataset.kundenpipelineStyle = 'true';
  link.href = new URL('./index.css', import.meta.url).href;
  document.head.append(link);
}

function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

function formatDate(value) {
  const timestamp = Number(value || 0);
  if (!timestamp) return '—';
  try {
    return new Intl.DateTimeFormat(state.ctx?.locale === 'en' ? 'en-GB' : 'de-DE', {
      dateStyle: 'medium', timeStyle: 'short',
    }).format(new Date(timestamp));
  } catch {
    return new Date(timestamp).toLocaleString();
  }
}

function typeLabel(type) {
  return ({
    agent_escalation: 'Owner-Entscheidung',
    zuordnung: 'Zuordnung',
    triage: 'Triage',
    mailfreigabe: 'Mailfreigabe',
  })[type] || type || 'Entscheidung';
}

function statusLabel(decision) {
  if (decision.expired) return 'Abgelaufen';
  return ({ offen: 'Offen', entschieden: 'Entschieden', abgelehnt: 'Abgelehnt' })[decision.status]
    || decision.status || 'Unbekannt';
}

function selectedDecision() {
  return state.decisions.map(normalizeDecision).find((decision) => decision.id === state.selectedId) || null;
}

function selectedVorgang(decision) {
  if (!decision?.vorgang_id) return null;
  return state.vorgaenge.find((item) => item.id === decision.vorgang_id) || null;
}

function readiness(name) {
  return state.ctx?.sync?.collectionReadiness?.call(state.ctx.sync, name) || null;
}

function isLoadingEmpty() {
  return state.loading || [DECISIONS, VORGAENGE].some((name) => readiness(name)?.ready === false);
}

function renderList(root) {
  const visible = filterDecisions(state.decisions, { filter: state.filter, search: state.search });
  root.querySelector('[data-count="open"]').textContent = String(filterDecisions(state.decisions).length);
  root.querySelector('[data-count="all"]').textContent = String(state.decisions.filter((item) => item.is_deleted !== true).length);
  const list = root.querySelector('[data-decision-list]');
  if (!visible.length) {
    list.innerHTML = isLoadingEmpty()
      ? '<div class="ctox-syncing kundenpipeline-empty" role="status" aria-busy="true"></div>'
      : '<div class="kundenpipeline-empty">Keine passenden Entscheidungen.</div>';
  } else {
    list.innerHTML = visible.map((decision) => `
      <button type="button" class="kundenpipeline-decision-card${decision.id === state.selectedId ? ' is-selected' : ''}" data-decision-id="${escapeHtml(decision.id)}" aria-pressed="${decision.id === state.selectedId}">
        <span class="kundenpipeline-card-meta"><span class="kundenpipeline-type">${escapeHtml(typeLabel(decision.typ))}</span><span class="kundenpipeline-status ${decision.status === 'offen' && !decision.expired ? 'is-open' : 'is-resolved'}">${escapeHtml(statusLabel(decision))}</span></span>
        <span class="kundenpipeline-card-title">${escapeHtml(decision.title)}</span>
        <span class="kundenpipeline-card-foot"><span>${escapeHtml(decision.question || decision.vorgang_id || 'Kein Kontext')}</span><span>${escapeHtml(formatDate(decision.created_at_ms))}</span></span>
      </button>`).join('');
  }
  root.querySelector('[data-list-status]').textContent = `${visible.length} von ${state.decisions.length} Entscheidungen`;
}

function renderDetail(root) {
  const decision = selectedDecision();
  const body = root.querySelector('[data-detail-body]');
  const actionBody = root.querySelector('[data-action-body]');
  if (!decision) {
    root.querySelector('[data-detail-kicker]').textContent = 'Keine Auswahl';
    root.querySelector('[data-detail-title]').textContent = 'Entscheidung auswählen';
    body.innerHTML = '<div class="kundenpipeline-empty">Wähle eine offene Entscheidung aus.</div>';
    actionBody.innerHTML = '<div class="kundenpipeline-empty">Wähle eine Entscheidung, um die verfügbaren Optionen zu sehen.</div>';
    return;
  }
  const vorgang = selectedVorgang(decision);
  root.querySelector('[data-detail-kicker]').textContent = typeLabel(decision.typ);
  root.querySelector('[data-detail-title]').textContent = decision.title;
  const source = decision.source_json || {};
  const frage = decision.question || (Array.isArray(decision.zeilen_json) ? decision.zeilen_json.join('\n') : '');
  const mails = Array.isArray(vorgang?.mails_json) ? vorgang.mails_json : [];
  const audit = Array.isArray(vorgang?.audit_json) ? vorgang.audit_json.slice(-8).reverse() : [];
  body.innerHTML = `
    <div class="kundenpipeline-detail-body">
      <section class="kundenpipeline-section">
        <span class="kundenpipeline-status ${decision.status === 'offen' && !decision.expired ? 'is-open' : 'is-resolved'}">${escapeHtml(statusLabel(decision))}</span>
        <h3 class="kundenpipeline-question">${escapeHtml(frage || 'Keine Frage hinterlegt.')}</h3>
        ${decision.context ? `<p class="kundenpipeline-copy">${escapeHtml(decision.context)}</p>` : ''}
      </section>
      <dl class="kundenpipeline-fact-grid">
        <div class="kundenpipeline-fact"><dt>Vorgang</dt><dd>${escapeHtml(vorgang?.title || decision.vorgang_id || 'Agent-Eskalation')}</dd></div>
        <div class="kundenpipeline-fact"><dt>Kunde</dt><dd>${escapeHtml(vorgang?.kunde_name || '—')}</dd></div>
        <div class="kundenpipeline-fact"><dt>Quelle</dt><dd>${escapeHtml(source.authority || vorgang?.quelle_json?.kanal || 'CTOX')}</dd></div>
        <div class="kundenpipeline-fact"><dt>Aktualisiert</dt><dd>${escapeHtml(formatDate(decision.updated_at_ms))}</dd></div>
      </dl>
      ${vorgang?.quelle_json?.body_clean ? `<section class="kundenpipeline-section"><h3>Eingang</h3><p class="kundenpipeline-mail">${escapeHtml(vorgang.quelle_json.body_clean)}</p></section>` : ''}
      ${mails.length ? `<section class="kundenpipeline-section"><h3>Mail-Verlauf</h3>${mails.slice(-4).reverse().map((mail) => `<p class="kundenpipeline-mail"><strong>${escapeHtml(mail.betreff || 'Mail')}</strong><br>${escapeHtml(mail.body || '')}</p>`).join('')}</section>` : ''}
      ${audit.length ? `<section class="kundenpipeline-section"><h3>Audit</h3>${audit.map((entry) => `<p class="kundenpipeline-audit-entry">${escapeHtml(formatDate(entry.zeit_ms || entry.at_ms))} · ${escapeHtml(entry.aktion || entry.action || '')} · ${escapeHtml(entry.akteur || entry.actor || '')}</p>`).join('')}</section>` : ''}
    </div>`;
  renderActions(root, decision, vorgang);
}

function renderActions(root, decision, vorgang) {
  const actionBody = root.querySelector('[data-action-body]');
  const options = decision.typ === 'agent_escalation'
    ? decision.options
    : [
        { wert: 'annehmen', label: 'Annehmen', description: 'Die Entscheidung übernehmen und die serverseitige Folgeaktion ausführen.' },
        { wert: 'ablehnen', label: 'Ablehnen', description: 'Die Entscheidung schließen, ohne eine Folgeaktion auszuführen.' },
      ];
  const canResolve = decision.status === 'offen' && !decision.expired;
  actionBody.innerHTML = `
    <section class="kundenpipeline-section">
      <h3>${decision.typ === 'agent_escalation' ? 'Option wählen' : 'Entscheidung treffen'}</h3>
      <p class="kundenpipeline-copy">${escapeHtml(vorgang?.kunde_name ? `Vorgang für ${vorgang.kunde_name}` : 'Der Command wird serverseitig policy-geprüft.')}</p>
      <div class="kundenpipeline-action-list">
        ${canResolve && options.length ? options.map((option) => `
          <button type="button" class="kundenpipeline-action" data-decision-option="${escapeHtml(option.wert || option.id || '')}">
            <strong>${escapeHtml(option.label || option.wert || option.id || 'Option')}</strong>
            ${option.description ? `<span>${escapeHtml(option.description)}</span>` : ''}
          </button>`).join('') : '<p class="kundenpipeline-empty">Für diese Entscheidung sind keine Aktionen mehr verfügbar.</p>'}
      </div>
    </section>
    <div class="kundenpipeline-command-status${state.commandError ? ' is-error' : ''}" role="status" aria-live="polite">${escapeHtml(state.commandStatus || '')}</div>`;
}

function render(root) {
  root.querySelectorAll('[data-filter]').forEach((button) => {
    const active = button.dataset.filter === state.filter;
    button.classList.toggle('is-active', active);
    button.setAttribute('aria-selected', String(active));
  });
  renderList(root);
  renderDetail(root);
}

async function refresh(root) {
  state.loading = state.decisions.length === 0;
  state.error = '';
  render(root);
  try {
    const [decisions, vorgaenge, projekte] = await Promise.all([
      readCollection(DECISIONS),
      readCollection(VORGAENGE),
      readCollection(PROJECTS),
    ]);
    state.decisions = decisions.filter((item) => item.is_deleted !== true);
    state.vorgaenge = vorgaenge.filter((item) => item.is_deleted !== true);
    state.projekte = projekte.filter((item) => item.is_deleted !== true);
    if (!state.selectedId || !state.decisions.some((item) => item.id === state.selectedId)) {
      state.selectedId = filterDecisions(state.decisions)[0]?.id || state.decisions[0]?.id || '';
    }
  } catch (error) {
    console.warn('[kundenpipeline] load failed', error);
    state.error = '';
  } finally {
    state.loading = false;
    render(root);
  }
}

async function dispatchDecision(root, decision, optionId) {
  if (!state.ctx?.commandBus?.dispatch) {
    state.commandError = true;
    console.warn('[kundenpipeline] command bus unavailable');
    state.commandStatus = 'Aktion hat nicht geklappt.';
    render(root);
    return;
  }
  state.commandError = false;
  state.commandStatus = 'Wird an CTOX übergeben …';
  render(root);
  try {
    const outcome = await state.ctx.commandBus.dispatch({
      ...decisionCommand(decision, optionId),
      id: commandId(),
    });
    state.commandStatus = outcome?.status === 'rejected' || outcome?.ok === false
      ? 'Die Aktion wurde abgelehnt.'
      : 'Entscheidung gespeichert.';
    state.commandError = outcome?.status === 'rejected' || outcome?.ok === false;
    await refresh(root);
  } catch (error) {
    state.commandError = true;
    console.warn('[kundenpipeline] action failed', error);
    state.commandStatus = 'Aktion hat nicht geklappt.';
    render(root);
  }
}

function wire(root) {
  root.querySelector('[data-refresh]')?.addEventListener('click', () => refresh(root));
  root.querySelector('[data-search]')?.addEventListener('input', (event) => {
    state.search = event.target.value;
    render(root);
  });
  root.querySelectorAll('[data-filter]').forEach((button) => button.addEventListener('click', () => {
    state.filter = button.dataset.filter || 'open';
    const first = filterDecisions(state.decisions, { filter: state.filter, search: state.search })[0];
    if (first) state.selectedId = first.id;
    render(root);
  }));
  root.querySelector('[data-decision-list]')?.addEventListener('click', (event) => {
    const button = event.target.closest('[data-decision-id]');
    if (!button) return;
    state.selectedId = button.dataset.decisionId || '';
    state.commandStatus = '';
    state.commandError = false;
    render(root);
  });
  root.querySelector('[data-action-body]')?.addEventListener('click', (event) => {
    const button = event.target.closest('[data-decision-option]');
    const decision = selectedDecision();
    if (!button || !decision) return;
    dispatchDecision(root, decision, button.dataset.decisionOption);
  });
  root.querySelector('[data-toggle-actions]')?.addEventListener('click', () => {
    const visible = root.classList.toggle('is-actions-hidden') === false;
    const toggle = root.querySelector('[data-toggle-actions]');
    toggle?.setAttribute('aria-pressed', String(visible));
    toggle?.setAttribute('aria-label', visible ? 'Aktionen ausblenden' : 'Aktionen einblenden');
    toggle?.setAttribute('title', visible ? 'Aktionen ausblenden' : 'Aktionen einblenden');
  });
}

export async function mount(ctx = {}) {
  state.ctx = ctx;
  state.decisions = [];
  state.vorgaenge = [];
  state.projekte = [];
  state.selectedId = '';
  state.filter = 'open';
  state.search = '';
  state.commandStatus = '';
  state.commandError = false;
  state.error = '';
  state.cleanup.splice(0).forEach((cleanup) => cleanup?.());
  await ensureStyles();
  ctx.host.innerHTML = await loadMarkup();
  ctx.left?.replaceChildren?.();
  ctx.right?.replaceChildren?.();
  const root = ctx.host.querySelector('[data-kundenpipeline-root]');
  if (!root) return () => {};
  wire(root);
  const readinessUnsubs = [DECISIONS, VORGAENGE, PROJECTS]
    .map((name) => {
      try {
        return ctx.sync?.subscribeCollectionReadiness?.(name, () => render(root));
      } catch {
        return null;
      }
    })
    .filter((unsubscribe) => typeof unsubscribe === 'function');
  state.cleanup.push(() => readinessUnsubs.forEach((unsubscribe) => unsubscribe()));
  const timer = window.setInterval(() => refresh(root), 10000);
  state.cleanup.push(() => window.clearInterval(timer));
  await refresh(root);
  return () => {
    state.cleanup.splice(0).forEach((cleanup) => cleanup?.());
    root.replaceChildren();
  };
}

export const __kundenpipelineTestHooks = {
  normalizeDecision,
  filterDecisions,
  decisionCommand,
};
