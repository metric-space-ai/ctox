// Der Abschlussbericht des Recherche-Agents stand nur im Crew-Chat; in der
// Lead-Ansicht fehlte er (Owner 15.10.2026: "hier müsste der abschlussreport
// mit rein"). Er kommt aus dem Ergebnis des Recherche-Befehls.
export function berichtAusBefehl(befehl) {
  const result = befehl?.result || {};
  const text = [result.outbound_text, result.user_message, befehl?.answer, befehl?.response]
    .find((wert) => typeof wert === 'string' && wert.trim());
  return text ? text.trim() : '';
}

const escapeHtml = (wert) => String(wert)
  .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  .replace(/"/g, '&quot;').replace(/'/g, '&#39;');

// Nur die Auszeichnung, die die Berichte tatsaechlich nutzen: Absaetze,
// Aufzaehlungen mit "- " und **fett**. Alles andere bleibt Text.
export function berichtHtml(text) {
  const inline = (zeile) => escapeHtml(zeile).replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
  const bloecke = [];
  let liste = [];
  const listeSchliessen = () => {
    if (liste.length) bloecke.push(`<ul>${liste.map((punkt) => `<li>${inline(punkt)}</li>`).join('')}</ul>`);
    liste = [];
  };
  for (const roh of String(text || '').split('\n')) {
    const zeile = roh.trim();
    if (/^[-*]\s+/.test(zeile)) { liste.push(zeile.replace(/^[-*]\s+/, '')); continue; }
    listeSchliessen();
    if (zeile) bloecke.push(`<p>${inline(zeile.replace(/^#+\s*/, ''))}</p>`);
  }
  listeSchliessen();
  return bloecke.join('');
}
