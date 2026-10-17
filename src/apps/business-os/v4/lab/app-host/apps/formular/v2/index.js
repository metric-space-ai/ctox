// Demo-App „formular“ v2: zusätzliches Feld `prio`, anderes Layout (zwei Spalten), MIT snapshot().
export function mount(root, api, { restored } = {}) {
  api.shell.setTitle('Formular');
  root.innerHTML = `
    <div class="f2">
      <ul id="liste" class="f2__liste"></ul>
      <form id="form" class="f2__form" autocomplete="off">
        <label>Name <input id="name" name="name"></label>
        <label>Priorität <input id="prio" name="prio" type="number" min="0" max="9" value="3"></label>
        <label>Kategorie <select id="kategorie" name="kategorie"><option value="a">A</option><option value="b">B</option><option value="c">C</option></select></label>
        <label class="f2__wide">Beschreibung <textarea id="beschreibung" name="beschreibung" rows="6"></textarea></label>
      </form>
    </div>`;
  const $ = (id) => root.querySelector('#' + id);
  const f = restored && restored.fields;
  if (f) { for (const k of ['name', 'beschreibung', 'kategorie', 'prio']) if (f[k] != null) $(k).value = f[k]; }
  const ul = $('liste');
  const q = api.query('eintraege', { selector: { archived: { $ne: true } }, sort: [{ prio: 'desc' }, { titel: 'asc' }] });
  const off = q.subscribe((rows) => {
    ul.replaceChildren(...rows.map((r) => { const li = document.createElement('li'); li.dataset.id = r.id; li.textContent = `P${r.prio} — ${r.titel}`; return li; }));
  });
  const btn = document.createElement('button');
  btn.type = 'button'; btn.textContent = 'Speichern';
  btn.addEventListener('click', () => api.put('eintraege', { id: 'n' + Date.now(), titel: $('name').value || 'Neu', prio: Number($('prio').value) || 0 }));
  api.shell.registerHeader(btn); // §16.19 Nr. 5: Element-Referenz, void
  return {
    unmount() { off(); api.shell.registerHeader(null); },
    snapshot() {
      return { schema: 'formular/2', fields: { name: $('name').value, beschreibung: $('beschreibung').value, kategorie: $('kategorie').value, prio: $('prio').value } };
    },
  };
}
