// Unter 760px stapelt das Layout Kampagnen, Leadliste und Details untereinander.
// Die Details lagen dann unterhalb des sichtbaren Bereichs; ein Klick auf einen
// Lead zeigte scheinbar nichts an (Kundeninstanz, 15.10.2026, Fenster 751px).
// Nur im gestapelten Layout scrollen, nebeneinander bleibt alles stehen.
export function revealDetailIfStacked({ leads, detail } = {}) {
  if (!leads || !detail || typeof detail.scrollIntoView !== 'function') return false;
  const leadsBox = leads.getBoundingClientRect();
  const detailBox = detail.getBoundingClientRect();
  if (detailBox.top < leadsBox.bottom - 1) return false;
  detail.scrollIntoView({ block: 'start', behavior: 'smooth' });
  return true;
}
