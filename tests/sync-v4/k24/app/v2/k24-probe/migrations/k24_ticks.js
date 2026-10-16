// Migration k24_ticks v1 → neue Generation (Architektur §16.4, Format A9 migrate_fn): neues Pflichtfeld weight.
// Ausgeführt vom Migrationsrunner der App-Laufzeit je Dokument; Rückgabe = Dokument der neuen Generation.
export default function migrate(doc) {
  return { ...doc, weight: typeof doc.weight === 'number' ? doc.weight : 1 };
}
