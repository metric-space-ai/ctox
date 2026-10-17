// Demo-App „formular“ v3: absichtlich kaputt – mount() kehrt nie zurück.
export function mount(root) {
  root.textContent = '';
  return new Promise(() => {});
}
