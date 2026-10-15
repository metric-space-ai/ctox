// Eine Kontaktliste aus dem Browser ist eine Kopie, die aelter sein kann als
// der gespeicherte Lead. Kundeninstanz 15.10.2026: der Recherche-Agent schrieb
// zwei Ansprechpartner um 03:48 zurueck, danach schrieb die App bei der
// Sellify-Ergaenzung ihre aeltere Liste als Ganzes und beide waren weg.
// Darum: mitgegebene Kontakte ersetzen ihre gespeicherten Gegenstuecke (gleiche
// id oder gleicher person_key), gespeicherte Kontakte ohne Gegenstueck bleiben.
// Eine leere Liste ist ein bewusstes Leeren (Neuimport, Recherche-Reset).
export function mergeStoredContacts(stored = [], incoming = []) {
  if (!Array.isArray(incoming)) return stored;
  if (incoming.length === 0 || !Array.isArray(stored) || stored.length === 0) return incoming;
  const key = (value) => String(value || '').trim().toLowerCase();
  const ids = new Set(incoming.map((contact) => key(contact?.id)).filter(Boolean));
  const personKeys = new Set(incoming.map((contact) => key(contact?.person_key)).filter(Boolean));
  const fehlend = stored.filter((contact) => contact
    && !ids.has(key(contact.id))
    && !(key(contact.person_key) && personKeys.has(key(contact.person_key))));
  return fehlend.length ? [...incoming, ...fehlend] : incoming;
}
