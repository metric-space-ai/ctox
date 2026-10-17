const BUILD = '20260830-kundenpipeline-decision-hub-v1'; // Kennung im client_context (keine Cache-Buster-Rolle)
const MODULE_ID = 'kundenpipeline';
const DECISIONS = 'kundenpipeline_entscheidungen';
const VORGAENGE = 'kundenpipeline_vorgaenge';
const PROJECTS = 'kundenpipeline_projekte';

const state = {
  api: null,
  root: null,
  decisions: [],
  vorgaenge: [],
  projekte: [],
  selectedId: '',
  filter: 'open',
  search: '',
  commands: new Map(),
};

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

async function loadMarkup() {
  const response = await fetch(new URL('./index.html', import.meta.url));
  return response.text();
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
    const locale = String(state.api?.shell.locale.get() || '').toLowerCase().startsWith('en') ? 'en-GB' : 'de-DE';
    return new Intl.DateTimeFormat(locale, {
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

function renderList(root) {
  const visible = filterDecisions(state.decisions, { filter: state.filter, search: state.search });
  root.querySelector('[data-count="open"]').textContent = String(filterDecisions(state.decisions).length);
  root.querySelector('[data-count="all"]').textContent = String(state.decisions.filter((item) => item.is_deleted !== true).length);
  const list = root.querySelector('[data-decision-list]');
  if (!visible.length) {
    // Leere Liste rendert nichts (PORTING.md R11): kein Lade-, Synchronisierungs- oder Leertext.
    list.innerHTML = '';
  } else {
    list.innerHTML = visible.map((decision) => `
      <button type="button" class="kundenpipeline-decision-card${decision.id === state.selectedId ? ' is-selected' : ''}" data-decision-id="${escapeHtml(decision.id)}" aria-pressed="${decision.id === state.selectedId}">
        <span class="kundenpipeline-card-meta"><span class="kundenpipeline-type">${escapeHtml(typeLabel(decision.typ))}</span><span class="kundenpipeline-status ${decision.status === 'offen' && !decision.expired ? 'is-open' : 'is-resolved'}">${escapeHtml(statusLabel(decision))}</span></span>
        <span class="kundenpipeline-card-title">${escapeHtml(decision.title)}</span>
        <span class="kundenpipeline-card-foot"><span>${escapeHtml(decision.question || '')}</span><span>${escapeHtml(formatDate(decision.created_at_ms))}</span></span>
      </button>`).join('');
  }
}

function renderDetail(root) {
  const decision = selectedDecision();
  const body = root.querySelector('[data-detail-body]');
  const actionBody = root.querySelector('[data-action-body]');
  if (!decision) {
    // Ohne Auswahl bleiben Detail und Aktionen leer (R11).
    root.querySelector('[data-detail-kicker]').textContent = '';
    root.querySelector('[data-detail-title]').textContent = '';
    body.innerHTML = '';
    actionBody.innerHTML = '';
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
        ${frage ? `<h3 class="kundenpipeline-question">${escapeHtml(frage)}</h3>` : ''}
        ${decision.context ? `<p class="kundenpipeline-copy">${escapeHtml(decision.context)}</p>` : ''}
      </section>
      <dl class="kundenpipeline-fact-grid">
        <div class="kundenpipeline-fact"><dt>Vorgang</dt><dd>${escapeHtml(vorgang?.title || '—')}</dd></div>
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
        { wert: 'annehmen', label: 'Annehmen', description: 'Entscheidung übernehmen und Folgeaktion ausführen' },
        { wert: 'ablehnen', label: 'Ablehnen', description: 'Entscheidung ohne Folgeaktion schließen' },
      ];
  const canResolve = decision.status === 'offen' && !decision.expired;
  actionBody.innerHTML = `
    <section class="kundenpipeline-section">
      <div class="kundenpipeline-action-list">
        ${canResolve && options.length ? options.map((option) => `
          <button type="button" class="kundenpipeline-action" data-decision-option="${escapeHtml(option.wert || option.id || '')}"${option.description ? ` title="${escapeHtml(option.description)}"` : ''}>
            <strong>${escapeHtml(option.label || option.wert || option.id || 'Option')}</strong>
          </button>`).join('') : ''}
      </div>
    </section>`;
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

// Ein Live-Abo je Collection ersetzt die 10-s-Neuladeschleife und den Aktualisieren-Knopf.
function applyRows(root) {
  if (!state.selectedId || !state.decisions.some((item) => item.id === state.selectedId)) {
    state.selectedId = filterDecisions(state.decisions)[0]?.id || state.decisions[0]?.id || '';
  }
  render(root);
}

// Befehl über api.command, nie warten (R5). Status live in die Zustandszone: failed → Fehler, done → räumen.
function dispatchDecision(root, decision, optionId) {
  const api = state.api;
  if (!api) return;
  const envelope = decisionCommand(decision, optionId);
  const code = `kundenpipeline.command.${envelope.command_type}`;
  let handle;
  try {
    handle = api.command(envelope.command_type, {
      ...envelope.payload,
      record_id: envelope.record_id,
      client_context: envelope.client_context,
    });
  } catch (error) {
    api.status.report({ level: 'error', code, text: `Aktion fehlgeschlagen: ${error?.message || error}` });
    return;
  }
  handle.local.catch((error) => api.status.report({ level: 'error', code, text: `Aktion fehlgeschlagen: ${error?.message || error}`, ref: handle.id }));
  const off = handle.status.subscribe((st) => {
    if (st?.status === 'failed') {
      api.status.report({ level: 'error', code, text: st.error ? `Die Aktion wurde abgelehnt: ${st.error}` : 'Die Aktion wurde abgelehnt.', ref: handle.id });
    } else if (st?.status === 'done') {
      api.status.clear(code);
    } else return;
    off?.();
    state.commands.delete(handle.id);
  });
  state.commands.set(handle.id, off);
}

function wire(root) {
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

const COLLECTIONS = Object.freeze([['decisions', DECISIONS], ['vorgaenge', VORGAENGE], ['projekte', PROJECTS]]);

export async function mount(host, api, { restored } = {}) {
  const disposers = [];
  const commands = new Map();
  Object.assign(state, { api, root: host, decisions: [], vorgaenge: [], projekte: [], selectedId: '', filter: 'open', search: '', commands });
  if (!host.dataset.app) host.dataset.app = api.app?.id || MODULE_ID;
  // CSS lädt die Shell aus dem Manifest (R10); Markup relativ ohne Cache-Buster.
  host.innerHTML = await loadMarkup();
  api.shell.setTitle('Decision Hub');
  const root = host.querySelector('[data-kundenpipeline-root]');
  if (!root) return { unmount() { host.replaceChildren(); } };
  if (restored && typeof restored === 'object' && restored.v === 1) {
    if (typeof restored.selectedId === 'string') state.selectedId = restored.selectedId;
    if (restored.filter === 'open' || restored.filter === 'all') state.filter = restored.filter;
    if (typeof restored.search === 'string') state.search = restored.search;
    const search = root.querySelector('[data-search]');
    if (search) search.value = state.search;
    if (restored.actionsVisible === true) root.classList.remove('is-actions-hidden');
  }
  wire(root);
  render(root);
  let pendingScroll = restored?.v === 1 && Number.isFinite(restored.listScrollTop) ? restored.listScrollTop : null;
  for (const [key, name] of COLLECTIONS) {
    disposers.push(api.query(name, {}).subscribe((rows) => {
      state[key] = (Array.isArray(rows) ? rows : []).filter((item) => item.is_deleted !== true);
      applyRows(root);
      if (pendingScroll != null && state.decisions.length) {
        const list = root.querySelector('[data-decision-list]');
        if (list) list.scrollTop = pendingScroll;
        pendingScroll = null;
      }
    }));
  }
  return {
    unmount() {
      for (const off of disposers.splice(0)) { try { off(); } catch {} }
      for (const off of commands.values()) { try { off(); } catch {} }
      commands.clear();
      host.replaceChildren();
      if (state.api === api) { state.api = null; state.root = null; }
    },
    snapshot() {
      return {
        v: 1,
        selectedId: state.selectedId,
        filter: state.filter,
        search: root.querySelector('[data-search]')?.value || state.search,
        actionsVisible: !root.classList.contains('is-actions-hidden'),
        listScrollTop: root.querySelector('[data-decision-list]')?.scrollTop || 0,
      };
    },
  };
}

export const __kundenpipelineTestHooks = {
  normalizeDecision,
  filterDecisions,
  decisionCommand,
};
