// Shell v4 – Coding Agent als Seitenleiste der Code-Ansicht (Strang AGENT; Bedienkonzept Fassung 4 §3/§3a;
// Kernvertrag §16.44). Wie Copilot in VS Code: rechts neben dem Editor, einklappbar, arbeitet nur auf dem EIGENEN
// Entwurf; veröffentlicht wird erst über „Veröffentlichen“ im Editor-Kopf.
//
//   createAgentPanel({ appId, appTitle, coding, source, zone, canEdit, flushDraft, reloadDraft, openChanges })
//     → { el, setOpen(b), get open, send(prompt), destroy(), get state }
//   coding      Port des Coding-Turn-Vertrags (coding-port.js über die Shell-App-API):
//               { command(type, payload) → Promise<Ergebnis>, turns: {get, subscribe},
//               draftRef(appId) → Promise<{draft_id, rev}|{unsupported}|null>, readPatch(turn) → Promise<string> }
//   source      kernel.source(appId) (Entwurf anlegen per rebase, Patch übernehmen per write/remove, list/read)
//   zone        Zustandszone des Fensters; Codes agent.turn (Ablehnung/Fehler), agent.conflict, agent.patch
//   flushDraft  ausstehende Editor-Eingaben in den Entwurf schreiben (vor jedem Senden)
//   reloadDraft Code-Ansicht aus dem Kern neu aufbauen (nach Turn-Ende, stale_draft_rev, Patch)
//   openChanges(paths) geänderte Dateien als Diff im Editor öffnen
//
// Keine Zustandstexte in der Leiste: laufend = dezenter Indikator, Fehler und Ablehnungen nur in die Zone.

import { applyPatch } from './text-diff.js';

const ACTIVE = new Set(['queued', 'running']);
function h(tag, cls, text) {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined) n.textContent = text;
  return n;
}
const newTurnId = () => `turn-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

export function createAgentPanel({ appId, appTitle, coding, source, zone, canEdit = () => true, flushDraft, reloadDraft, openChanges, activeVer }) {
  const sink = zone ?? { report() {}, clear() {} };
  const st = { turns: [], draftId: null, sending: false, open: true, seen: new Map(), applied: new Set(), closed: false, log: [] };

  const el = h('aside', 'v4-agent');
  el.dataset.v4Agent = appId;
  el.setAttribute('aria-label', 'Coding Agent');
  const head = h('div', 'v4-agent__head');
  const title = h('span', 'v4-agent__title', 'Agent');
  const collapse = h('button', 'v4-agent__icon', '›');
  collapse.type = 'button';
  collapse.dataset.agentCollapse = '';
  collapse.title = 'Agent ausblenden';
  collapse.setAttribute('aria-label', 'Agent ausblenden');
  head.append(title, collapse);
  const list = h('div', 'v4-agent__turns');
  list.setAttribute('role', 'log');
  list.setAttribute('aria-live', 'polite');
  const empty = h('p', 'v4-agent__empty', `Beschreibe, was sich an ${appTitle} ändern soll. Der Agent arbeitet im Entwurf; du siehst die Änderungen als Diff und veröffentlichst selbst.`);
  const form = h('form', 'v4-agent__form');
  const input = h('textarea', 'v4-agent__input');
  input.rows = 3;
  input.dataset.agentInput = '';
  input.placeholder = `Was soll der Agent an ${appTitle} ändern?`;
  input.setAttribute('aria-label', input.placeholder);
  const acts = h('div', 'v4-agent__acts');
  // Signatur-Steuerung der Seitenleiste (shared/base.css, genau einmal wie in research/outbound/iot/creator):
  // EIN Knopf, „Senden“ im Leerlauf, „Abbrechen“ solange ein Turn läuft (aria-busy, is-running).
  const runBtn = h('button', 'ctox-button ctox-run-control v4-agent__run', 'Senden');
  runBtn.type = 'button';
  runBtn.dataset.agentRun = 'send';
  acts.append(h('span', 'v4-src__spacer'), runBtn);
  form.append(input, acts);
  el.append(head, list, form);

  // Eingeklappt: schmale Lasche am rechten Rand (wie die sekundäre Seitenleiste in VS Code).
  const tab = h('button', 'v4-agent-tab', 'Agent');
  tab.type = 'button';
  tab.dataset.agentToggle = '';
  tab.title = 'Agent einblenden';
  tab.setAttribute('aria-label', 'Agent einblenden');

  const report = (code, text, extra = {}) => sink.report({ level: 'error', code, text, source: 'shell', ...extra });
  const running = () => st.turns.find((t) => ACTIVE.has(t.state)) ?? null;

  function setOpen(open) {
    st.open = !!open;
    el.hidden = !st.open;
    tab.hidden = st.open;
    el.closest('.v4-src')?.classList.toggle('has-agent', st.open);
    if (st.open) requestAnimationFrame(() => { if (!st.closed && canEdit()) input.focus({ preventScroll: true }); });
  }
  collapse.addEventListener('click', () => setOpen(false));
  tab.addEventListener('click', () => setOpen(true));

  function renderTurn(t) {
    const card = h('article', 'v4-agent__turn');
    card.dataset.turnId = t.id;
    card.dataset.state = t.state;
    // CodingTurn.prompt (Stub 51b1e33); messages kommen erst am Turn-Ende.
    card.append(h('div', 'v4-agent__prompt', t.prompt ?? t.messages?.find((m) => m.role === 'user')?.text ?? ''));
    if (ACTIVE.has(t.state)) {
      const dots = h('div', 'v4-agent__dots');
      dots.setAttribute('aria-label', 'Agent arbeitet');
      dots.append(h('i'), h('i'), h('i'));
      card.append(dots);
    }
    for (const m of t.messages ?? []) {
      if (m.role !== 'assistant' || !m.text) continue;
      card.append(h('div', 'v4-agent__reply', m.text));
    }
    if ((t.changed_files ?? []).length && ['done', 'conflict'].includes(t.state)) {
      const files = h('ul', 'v4-agent__files');
      for (const p of t.changed_files) {
        const li = h('li');
        const b = h('button', 'v4-agent__file', p);
        b.type = 'button';
        b.dataset.agentFile = p;
        b.title = t.state === 'done' ? `${p} als Diff öffnen` : p;
        b.disabled = t.state !== 'done';
        b.addEventListener('click', () => openChanges?.([p]));
        li.append(b);
        files.append(li);
      }
      card.append(files);
    }
    if (t.state === 'conflict' && t.patch?.file_id && !st.applied.has(t.id)) {
      const apply = h('button', 'v4-src__btn v4-agent__apply', 'Änderungen übernehmen');
      apply.type = 'button';
      apply.dataset.agentApply = t.id;
      apply.addEventListener('click', () => applyConflictPatch(t));
      card.append(apply);
    }
    return card;
  }
  function render() {
    list.replaceChildren();
    const mine = st.turns.filter((t) => t.draft_id === st.draftId).sort((a, b) => (a.created_at_ms ?? 0) - (b.created_at_ms ?? 0));
    if (!mine.length) list.append(empty);
    for (const t of mine) list.append(renderTurn(t));
    const r = mine.find((t) => ACTIVE.has(t.state));
    runBtn.dataset.agentRun = r ? 'cancel' : 'send';
    runBtn.textContent = r ? 'Abbrechen' : 'Senden';
    runBtn.classList.toggle('is-running', !!r);
    if (r) runBtn.setAttribute('aria-busy', 'true'); else runBtn.removeAttribute('aria-busy');
    runBtn.disabled = r ? false : (st.sending || !canEdit());
    input.disabled = !canEdit();
    el.dataset.running = r ? r.id : '';
    list.scrollTop = list.scrollHeight;
  }

  // Übergänge eines Turns (nur einmal je Turn und Zustand).
  async function onTransition(t, prev) {
    if (t.draft_id !== st.draftId) return;
    if (t.state === 'done') {
      sink.clear('agent.turn');
      await reloadDraft?.();
      if (t.changed_files?.length) await openChanges?.(t.changed_files);
    } else if (t.state === 'conflict') {
      sink.report({
        level: 'warn', code: 'agent.conflict', source: 'shell',
        text: 'Agent-Änderungen nicht eingespielt: dein Entwurf hat sich währenddessen geändert. Die Änderungen liegen als Patch bereit.',
        action: { label: 'Änderungen übernehmen', run: () => applyConflictPatch(t) },
      });
    } else if (t.state === 'failed') {
      report('agent.turn', `Agent-Lauf fehlgeschlagen (${t.error?.code ?? 'failed'}: ${t.error?.reason ?? '–'})${t.error?.detail ? ` – ${t.error.detail}` : ''}.`);
    }
    void prev;
  }
  const offTurns = coding.turns.subscribe((rows) => {
    if (st.closed) return;
    st.turns = Array.isArray(rows) ? rows : [];
    for (const t of st.turns) {
      const prev = st.seen.get(t.id);
      st.seen.set(t.id, t.state);
      // Beim ersten Sehen schon terminale Turns (Verlauf) lösen nichts aus.
      if (prev !== undefined && prev !== t.state && ACTIVE.has(prev)) onTransition(t, prev);
    }
    render();
  });

  async function ensureDraft() {
    let ref = await coding.draftRef(appId);
    if (ref) return ref;
    // Kern verlangt einen bestehenden Entwurf (draft_not_found): leerer Entwurf auf der aktiven Version. Revision erst
    // nach der Core-Bestätigung verwenden (eine frühere, gelöschte Zeile kann noch eine alte Revision tragen).
    await source.draft.rebase(activeVer?.() ?? null);
    ref = await coding.draftRef(appId);
    return ref?.unsupported ? ref : freshRef(ref?.rev ?? null);
  }
  async function dispatch(type, payload, meta = {}, opts = null) {
    const r = await coding.command(type, payload, opts ?? undefined);
    st.log.push({ type, payload: { ...payload }, status: r?.status ?? null, code: r?.code ?? null, reason: r?.reason ?? null, ...meta });
    return r;
  }
  // Nach stale_draft_rev: warten, bis der Kern die neue Entwurfsrevision bestätigt hat (eigene Eingaben liegen im
  // selben Ausgang VOR dem Befehl), höchstens 2 s.
  async function freshRef(oldRev) {
    const t0 = Date.now();
    for (;;) {
      const ref = await coding.draftRef(appId);
      if (!ref || ref.unsupported || (ref.rev && ref.rev !== oldRev) || Date.now() - t0 > 2000) return ref;
      await new Promise((r) => setTimeout(r, 50));
    }
  }

  // opts.record_id: Bezug des Befehls (Meldung, Strang LOOP); die Antwort trägt dann auch draft_id.
  async function send(prompt, opts = null) {
    const text = String(prompt ?? input.value).trim();
    const cmdOpts = opts?.record_id ? { record_id: String(opts.record_id) } : null;
    if (!text || st.sending || !canEdit()) return null;
    st.sending = true;
    render();
    try {
      // Eigene Eingaben zuerst bestätigen lassen: base_draft_rev = result.rev der letzten Schreibung (Kern ≥ 8489b4e).
      // Ohne result (alte Kerne) gilt draft.info().rev; stale_draft_rev fängt dann die Wiederholung unten ab.
      const confirmed = await flushDraft?.();
      if (confirmed?.rejected) { input.value = text; report('agent.turn', `Entwurf nicht gespeichert (${confirmed.rejected.code ?? 'rejected'}) – Auftrag nicht gesendet.`); return null; }
      let ref = await ensureDraft();
      let revSource = 'info';
      if (ref && !ref.unsupported && confirmed?.rev) { ref = { ...ref, rev: confirmed.rev }; revSource = 'write'; }
      if (ref?.unsupported) { input.value = text; report('agent.turn', 'Der Coding Agent ist im Kern dieser Instanz noch nicht verfügbar (§16.44).'); return null; } // v4-allow: zone (report = sink.report der Zustandszone)
      if (!ref?.rev) { input.value = text; report('agent.turn', 'Kein bestätigter Entwurf für den Agenten verfügbar.'); return null; }
      st.draftId = ref.draft_id;
      const payload = (r0) => ({ app_id: appId, draft_id: r0.draft_id, base_draft_rev: r0.rev, prompt: text, turn_id: newTurnId() });
      let r = await dispatch('ctox.coding.turn', payload(ref), { revSource }, cmdOpts);
      if (r?.reason === 'stale_draft_rev') {
        // Eigene Eingaben waren noch unterwegs bzw. ein anderer Tab hat geändert: still neu laden, auf die bestätigte
        // Revision warten und genau einmal erneut senden.
        await reloadDraft?.();
        const next = await freshRef(ref.rev);
        if (next?.rev && next.rev !== ref.rev) { ref = next; r = await dispatch('ctox.coding.turn', payload(ref), { revSource: 'retry' }, cmdOpts); }
      }
      if (r?.status === 'running' || r?.status === 'completed') {
        sink.clear('agent.turn');
        input.value = '';
        st.seen.set(r.turn_id, st.seen.get(r.turn_id) ?? 'queued');
        return { ...r, draft_id: ref.draft_id };
      }
      if (r?.reason === 'stale_draft_rev') {
        // Auch der zweite Versuch kam zu spät: still neu laden und erneut anbieten (Prompt im Feld, kein Zoneneintrag).
        await reloadDraft?.();
        input.value = text;
        input.focus();
        el.dataset.reoffered = String(Number(el.dataset.reoffered || 0) + 1);
        return r;
      }
      if (r?.reason === 'turn_running') {
        input.value = text;
        sink.report({ level: 'warn', code: 'agent.turn', source: 'shell', text: 'Der Agent arbeitet noch an diesem Entwurf. Warte das Ergebnis ab oder brich den Lauf ab.' });
        return r;
      }
      input.value = text;
      report('agent.turn', `Agent-Auftrag abgelehnt (${r?.code ?? 'rejected'}: ${r?.reason ?? '–'}).`);
      return r;
    } catch (e) {
      input.value = text;
      report('agent.turn', `Agent-Auftrag nicht gesendet: ${e?.message ?? e}`);
      return null;
    } finally {
      st.sending = false;
      render();
    }
  }
  async function cancel() {
    const r = running();
    if (!r) return null;
    try {
      const res = await dispatch('ctox.coding.cancel', { turn_id: r.id });
      if (res?.status === 'rejected') report('agent.turn', `Abbrechen abgelehnt (${res.code}: ${res.reason}).`);
      return res;
    } catch (e) {
      report('agent.turn', `Abbrechen nicht möglich: ${e?.message ?? e}`);
      return null;
    }
  }
  async function applyConflictPatch(t) {
    try {
      await flushDraft?.();
      const patch = await coding.readPatch(t);
      const files = {};
      const present = new Set(await source.list());
      for (const p of new Set([...(t.changed_files ?? [])])) files[p] = present.has(p) ? await source.read(p) : null;
      const next = applyPatch(files, patch);
      for (const [p, txt] of Object.entries(next)) { if (txt === null) await source.draft.remove(p); else await source.draft.write(p, txt); }
      st.applied.add(t.id);
      sink.clear('agent.conflict');
      sink.clear('agent.patch');
      await reloadDraft?.();
      await openChanges?.(Object.keys(next));
      render();
      return Object.keys(next);
    } catch (e) {
      report('agent.patch', e?.name === 'PatchConflict'
        ? `Agent-Änderungen passen nicht mehr zu deinem Entwurf (${e.message}) – nicht übernommen, dein Entwurf ist unverändert.`
        : `Patch nicht übernommen: ${e?.message ?? e}. Dein Entwurf ist unverändert.`);
      return null;
    }
  }

  /** Lauf zeigen (Verknüpfung an der Meldung): Seitenleiste öffnen, Karte des Turns hervorheben. */
  function focusTurn(turnId) {
    setOpen(true);
    const card = [...list.querySelectorAll('.v4-agent__turn')].find((c) => c.dataset.turnId === turnId);
    for (const c of list.querySelectorAll('.v4-agent__turn.is-focused')) c.classList.remove('is-focused');
    if (!card) return false;
    card.classList.add('is-focused');
    card.scrollIntoView?.({ block: 'nearest' });
    return true;
  }

  form.addEventListener('submit', (e) => { e.preventDefault(); send(); });
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); send(); }
    // Esc in der Eingabe soll die Code-Ansicht nicht verlassen, solange Text drin steht.
    if (e.key === 'Escape' && input.value) { e.preventDefault(); input.value = ''; }
  });
  runBtn.addEventListener('click', () => { if (runBtn.dataset.agentRun === 'cancel') cancel(); else send(); });

  // Entwurfs-Kennung für den Verlauf (ohne Entwurf: Verlauf leer bis zum ersten Auftrag).
  coding.draftRef(appId).then((ref) => { if (!st.closed && ref) { st.draftId = ref.draft_id; render(); } }).catch(() => {});
  render();

  return {
    el, tab, setOpen, send, cancel, applyConflictPatch, focusTurn,
    get open() { return st.open; },
    get state() {
      return {
        draftId: st.draftId, running: running()?.id ?? null, log: st.log.map((x) => ({ ...x, payload: { ...x.payload } })),
        turns: st.turns.filter((t) => t.draft_id === st.draftId).map((t) => ({ id: t.id, state: t.state, changed_files: t.changed_files ?? [], patch: t.patch ?? null })),
      };
    },
    destroy() { st.closed = true; offTurns?.(); el.remove(); tab.remove(); },
  };
}
