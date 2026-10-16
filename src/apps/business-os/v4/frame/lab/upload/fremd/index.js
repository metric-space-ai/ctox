// Per „Neue App“ hochgeladene Fremd-App (Rahmen-Labor): Icon ohne xmlns, Kategorie klein geschrieben.
export function mount(root) {
  const el = document.createElement('div');
  el.textContent = 'Fremde App montiert';
  root.append(el);
  return { unmount() { el.remove(); } };
}
