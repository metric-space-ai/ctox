// Per Ordner-Upload eingespielte App (Quelltext-Labor, Abnahme 6).
export function mount(root) {
  const el = document.createElement('div');
  el.className = 'zettel';
  el.textContent = 'Zettel-App montiert';
  root.append(el);
  return { unmount() { el.remove(); } };
}
