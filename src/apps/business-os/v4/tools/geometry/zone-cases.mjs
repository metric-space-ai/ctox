// Shell v4 Geometrie-Labor – Fehlerfälle mit erwarteter Zustandszone (Strang VIS4).
//
// Der Regelfall des Labors (fixtures.mjs) bildet einen GÜLTIGEN Zustand ab: keine Fehlerfelder, gesunde Status zuerst.
// Eine App, die in einem echten Fehlerzustand ist, soll ihn in der Zustandszone des Fensters melden und nirgends
// sonst. Diese Fälle prüfen genau das: Fixture gezielt in den Fehlerzustand setzen, dann muss die Zone mit dem
// erwarteten Code sichtbar sein und der Inhalt bleibt frei von Zustandstexten (G3 ohne Inhaltstreffer).
//
//   { app, name, expect: '<code>', patch(seed) → seed }   expect = Code des Zonen-Eintrags (Quelle app)
export const ZONE_CASES = [
  {
    app: 'news',
    name: 'ausgabe-fehlgeschlagen',
    expect: 'news.issue.failed',
    patch: (seed) => ({ ...seed, news_issues: (seed.news_issues || []).map((r) => ({ ...r, status: 'failed', error: 'Quelle nicht erreichbar' })) }),
  },
  {
    app: 'documents',
    name: 'version-fehlt',
    expect: 'documents.version.missing',
    patch: (seed) => ({ ...seed, document_versions: [], documents: (seed.documents || []).map((r) => ({ ...r, current_version_id: '' })) }),
  },
  {
    // DOCX ohne native Office-Vorbereitung (Kern-Stub): Editor-Fehler gehört in die Zone, nicht in den Inhalt.
    app: 'documents',
    name: 'docx-ohne-kern',
    expect: 'documents.editor',
    patch: (seed) => ({ ...seed, documents: (seed.documents || []).map((r) => ({ ...r, document_type: 'word_document', mime_type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', filename: String(r.filename || 'Dokument').replace(/\.[a-z]+$/i, '') + '.docx' })) }),
  },
  {
    app: 'browser',
    name: 'sitzung-fehlgeschlagen',
    expect: 'browser.frame',
    patch: (seed) => ({ ...seed, browser_sessions: (seed.browser_sessions || []).map((r) => ({ ...r, status: 'failed', runtime_status: 'failed', error: 'Browser-Prozess beendet' })) }),
  },
];

// Bewertung eines gemessenen Falls (measureInPage-Ergebnis): Zone mit erwartetem Code sichtbar, Inhalt ohne
// Zustandstexte. Exportiert für geometry-v4.test.mjs.
export function judgeZoneCase(c, r) {
  const fails = [];
  const zoneShown = (r.G3 || []).some((m) => m.startsWith('Zustandszone sichtbar'));
  const entries = r.info?.zoneEntries || [];
  if (!entries.includes(`app:${c.expect}`)) fails.push(`Zonen-Eintrag app:${c.expect} fehlt (${entries.join(', ') || 'leer'})`);
  if (!zoneShown) fails.push('Zustandszone nicht sichtbar');
  const content = (r.G3 || []).filter((m) => !m.startsWith('Zustandszone sichtbar'));
  if (content.length) fails.push(...content.map((m) => `Inhalt: ${m}`));
  if (r.failedToRun) fails.push(...(r.G1 || []));
  return { ok: fails.length === 0, fails };
}
