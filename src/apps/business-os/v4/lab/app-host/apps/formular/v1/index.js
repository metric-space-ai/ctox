// Demo-App „formular“ v1: Formular (input, textarea, select) + Live-Liste aus Collection `eintraege`.
// Kein snapshot(): die Shell sichert Werte generisch. Versteht einen App-Snapshot von v2 (restored.fields).
export function mount(root, api, { restored } = {}) {
  api.shell.setTitle('Formular');
  root.innerHTML = `
    <div class="f1">
      <form id="form" class="f1__form" autocomplete="off">
        <label>Name <input id="name" name="name"></label>
        <label>Beschreibung <textarea id="beschreibung" name="beschreibung" rows="4"></textarea></label>
        <label>Kategorie <select id="kategorie" name="kategorie"><option value="a">A</option><option value="b">B</option><option value="c">C</option></select></label>
      </form>
      <ul id="liste" class="f1__liste"></ul>
    </div>`;
  const f = restored && restored.fields;
  if (f) { for (const k of ['name', 'beschreibung', 'kategorie']) if (f[k] != null) root.querySelector('#' + k).value = f[k]; }
  const ul = root.querySelector('#liste');
  const q = api.query('eintraege', { selector: { archived: { $ne: true } }, sort: [{ prio: 'desc' }, { titel: 'asc' }] });
  const off = q.subscribe((rows) => {
    ul.replaceChildren(...rows.map((r) => { const li = document.createElement('li'); li.dataset.id = r.id; li.textContent = `${r.titel} · P${r.prio}`; return li; }));
  });
  const btn = document.createElement('button');
  btn.type = 'button'; btn.textContent = 'Neu';
  btn.addEventListener('click', () => api.put('eintraege', { id: 'n' + Date.now(), titel: root.querySelector('#name').value || 'Neu', prio: 9 }));
  api.shell.registerHeader(btn); // §16.19 Nr. 5: Element-Referenz, void
  return { unmount() { off(); api.shell.registerHeader(null); } };
}
