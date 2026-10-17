// K24-Probe – UI (App-API 4.0, §10.2a): mount(root, api, {restored}) → {unmount, snapshot}.
// Eine Eingabe, zwei Wege: „Tick“ = api.command('k24.tick.add') (Handler schreibt), „Direkt“ = api.put (eigene
// Collection). Liste = api.query live. Fehler nur über api.status.report (keine Zustandstexte im Inhalt).
// snapshot() trägt die nicht abgeschickte Eingabe über einen Remount (§16.10).

const COLLECTION = 'k24_ticks';
const APP_SCHEMA = 1;
const CODE_COMMAND = 'k24.command.tick_add';
const CODE_PUT = 'k24.put.tick';
const CODE_AGENT = 'k24.command.agent_start';
const RUNS = 'k24_runs';

function newTick(label) {
  return { id: `ui:${crypto.randomUUID()}`, label, source: 'ui', created_at_ms: Date.now() };
}
function describe(row) {
  return [row.label, row.source, row.weight !== undefined ? `w${row.weight}` : null].filter(Boolean).join(' · ');
}

export async function mount(root, api, { restored } = {}) {
  const offs = new Set();
  root.dataset.k24Schema = String(APP_SCHEMA);
  root.innerHTML = `
    <div class="k24">
      <form class="k24-form" data-k24-form>
        <input class="k24-input" data-k24-input name="label" autocomplete="off" maxlength="200" aria-label="Tick">
        <button type="submit" class="k24-btn" data-k24-command>Tick</button>
        <button type="button" class="k24-btn" data-k24-put>Direkt</button>
        <button type="button" class="k24-btn" data-k24-agent>Agent</button>
      </form>
      <ul class="k24-list" data-k24-list></ul>
      <ul class="k24-list" data-k24-runs></ul>
    </div>`;
  const form = root.querySelector('[data-k24-form]');
  const input = root.querySelector('[data-k24-input]');
  const list = root.querySelector('[data-k24-list]');
  const runsList = root.querySelector('[data-k24-runs]');

  if (restored && typeof restored === 'object' && typeof restored.draft === 'string') {
    input.value = restored.draft;
    if (Array.isArray(restored.sel) && restored.sel.length === 2) input.setSelectionRange(restored.sel[0], restored.sel[1]);
    if (restored.focus) input.focus();
  }

  const query = api.query(COLLECTION, { sort: [{ created_at_ms: 'desc' }] });
  offs.add(query.subscribe((rows) => {
    list.replaceChildren(...rows.map((row) => {
      const li = document.createElement('li');
      li.dataset.k24Tick = row.id;
      li.dataset.k24Source = row.source;
      if (row.weight !== undefined) li.dataset.k24Weight = String(row.weight);
      li.textContent = describe(row);
      return li;
    }));
  }));

  offs.add(api.query(RUNS, { sort: [{ created_at_ms: 'desc' }] }).subscribe((rows) => {
    runsList.replaceChildren(...rows.map((run) => {
      const li = document.createElement('li');
      li.dataset.k24Run = run.id;
      li.dataset.k24RunStatus = run.status;
      li.dataset.k24Callbacks = String(run.callback_count ?? 0);
      li.textContent = [run.label, run.result_text].filter(Boolean).join(' – ');
      return li;
    }));
  }));

  // Knopf „Agent“: Befehl → eigener Handler → agent.task.enqueue mit mitgebrachtem Skill (§16.30).
  root.querySelector('[data-k24-agent]').addEventListener('click', () => {
    const label = input.value.trim();
    if (!label) return;
    const h = api.command('k24.agent.start', { label });
    input.value = '';
    h.local.catch((e) => api.status.report({ level: 'error', code: CODE_AGENT, text: `Agentenlauf nicht angelegt (${e?.code ?? 'error'})`, ref: h.id }));
    let off = null;
    let finished = false;
    const stop = () => { finished = true; if (off) { off(); offs.delete(off); } };
    off = h.status.subscribe((s) => {
      if (s.status === 'failed') { api.status.report({ level: 'error', code: CODE_AGENT, text: `Agentenlauf nicht gestartet: ${s.error ?? 'unbekannt'}`, ref: h.id }); stop(); }
      else if (s.status === 'done') { api.status.clear(CODE_AGENT); stop(); }
    });
    if (finished) off(); else offs.add(off);
  });

  form.addEventListener('submit', (event) => {
    event.preventDefault();
    const label = input.value.trim();
    if (!label) return;
    const h = api.command('k24.tick.add', { label });
    input.value = '';
    h.local.catch((e) => api.status.report({ level: 'error', code: CODE_COMMAND, text: `Tick nicht angelegt (${e?.code ?? 'error'})`, ref: h.id }));
    let off = null;
    let finished = false;
    const stop = () => { finished = true; if (off) { off(); offs.delete(off); } };
    off = h.status.subscribe((s) => {
      if (s.status === 'failed') { api.status.report({ level: 'error', code: CODE_COMMAND, text: `Tick fehlgeschlagen: ${s.error ?? 'unbekannt'}`, ref: h.id }); stop(); }
      else if (s.status === 'done') { api.status.clear(CODE_COMMAND); stop(); }
    });
    if (finished) off(); else offs.add(off);
  });

  root.querySelector('[data-k24-put]').addEventListener('click', () => {
    const label = input.value.trim();
    if (!label) return;
    const h = api.put(COLLECTION, newTick(label));
    input.value = '';
    h.local.then(() => api.status.clear(CODE_PUT), (e) => api.status.report({ level: 'error', code: CODE_PUT, text: `Tick nicht gespeichert (${e?.code ?? 'error'})` }));
  });

  return {
    snapshot() {
      return { v: 1, draft: input.value, sel: [input.selectionStart ?? 0, input.selectionEnd ?? 0], focus: document.activeElement === input };
    },
    unmount() {
      for (const off of offs) off();
      offs.clear();
      root.replaceChildren();
    },
  };
}
