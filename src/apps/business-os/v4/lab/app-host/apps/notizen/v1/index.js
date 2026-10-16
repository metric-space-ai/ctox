// Demo-App „notizen“ v1 – OHNE snapshot(): Prüfling für die generische Sicherung der Shell.
export function mount(root, api) {
  api.shell.setTitle('Notizen');
  const lines = Array.from({ length: 60 }, (_, i) => `<p>Verlauf ${i}</p>`).join('');
  root.innerHTML = `
    <div class="n1">
      <input id="titel" placeholder="Titel">
      <select name="farbe"><option value="gelb">gelb</option><option value="blau">blau</option><option value="gruen">grün</option></select>
      <label><input type="checkbox" name="wichtig"> wichtig</label>
      <textarea id="notiz" rows="5"></textarea>
      <div id="verlauf" class="n1__verlauf">${lines}</div>
    </div>`;
  return { unmount() {} };
}
