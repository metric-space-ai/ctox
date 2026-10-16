// Demo-App „notizen“ v2 – anderes Layout, weiterhin OHNE snapshot().
export function mount(root, api) {
  api.shell.setTitle('Notizen');
  const lines = Array.from({ length: 60 }, (_, i) => `<p>Verlauf ${i}</p>`).join('');
  root.innerHTML = `
    <div class="n2">
      <div id="verlauf" class="n2__verlauf">${lines}</div>
      <div class="n2__edit">
        <input id="titel" placeholder="Titel">
        <label><input type="checkbox" name="wichtig"> wichtig</label>
        <select name="farbe"><option value="gelb">gelb</option><option value="blau">blau</option><option value="gruen">grün</option><option value="rot">rot</option></select>
        <textarea id="notiz" rows="8"></textarea>
      </div>
    </div>`;
  return { unmount() {} };
}
