// Prüfstand boot-real: minimale zweite App, damit der Starter der Shell etwas zeigt. Kein Produktcode.
export function mount(root) {
  const el = document.createElement('p');
  el.textContent = 'Notizen (Prüfstand)';
  root.appendChild(el);
  return { unmount() { el.remove(); } };
}
