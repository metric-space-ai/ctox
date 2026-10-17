// Migration ctox/ctox_crew_learnings: Basis main@4794af03d (Schema v0) → Schema v1, neue Generation (§16.4).
// Strang CREWAPP. Im App-Paket: migrations/ctox_crew_learnings.js. Gründe:
//   - Feld „normalized_text“ neu als Pflichtfeld
// Die Lernsätze sind jetzt App-Collection; der Handler prüft UNIQUE(member_id, normalized_text)
// (src/core/crew/mod.rs:114-121). Bisher projizierte Dokumente trugen das Feld nicht; es wird wie
// crew::normalized (mod.rs:441-443) aus dem Text abgeleitet: Leerraum zusammenziehen, klein schreiben.
export default function migrate(doc) {
  if (typeof doc.normalized_text === 'string') return doc;
  const text = typeof doc.text === 'string' ? doc.text : '';
  return { ...doc, normalized_text: text.split(/\s+/u).filter(Boolean).join(' ').toLowerCase() };
}
