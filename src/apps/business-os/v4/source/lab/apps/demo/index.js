// Demo-App für das Quelltext-Labor (App-API 4.0, mount/unmount).
let label = 'Version eins';
export function mount(root, api) {
  const el = document.createElement('div');
  el.className = 'demo';
  const h = document.createElement('h2');
  h.className = 'demo__label';
  h.textContent = label;
  const list = document.createElement('ul');
  el.append(h, list);
  root.append(el);
  const off = api.query('items', { selector: {} }).subscribe((rows) => {
    list.replaceChildren(...rows.map((r) => { const li = document.createElement('li'); li.textContent = `${r.title} · ${r.count}`; return li; }));
  });
  return { unmount() { off(); el.remove(); } };
}
