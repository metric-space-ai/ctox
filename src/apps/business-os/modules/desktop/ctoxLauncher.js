// Launcher-Verzeichnis des Desktops (App-API 4.0): Einträge aus `business_apps` (Plattform), Öffnen über
// api.shell.navigate. Kein location.hash, keine Desktop-Apps-Liste (DESKTOP_APPS = []).
export function createCtoxLauncher({ modules, currentModuleId, navigate }) {
  const moduleDirectory = new Map(
    (modules || [])
      .filter((mod) => mod && mod.id && mod.id !== currentModuleId)
      .map((mod) => [mod.id, mod])
  );

  function knows(targetId) {
    return moduleDirectory.has(targetId);
  }

  function entries() {
    return Array.from(moduleDirectory.values()).map((mod) => ({ ...mod, kind: 'module' }));
  }

  function kindOf(targetId) {
    return moduleDirectory.has(targetId) ? 'module' : null;
  }

  /** `false` bei unbekanntem Ziel; die Shell meldet unbekannte Ziele selbst in die Zone. */
  function open(targetId, { recordId } = {}) {
    if (!targetId || typeof navigate !== 'function') return false;
    return Boolean(navigate(targetId, recordId ? { record: recordId } : {}));
  }

  function glyphFor(targetId) {
    const mod = moduleDirectory.get(targetId);
    const fallback = MODULE_GLYPHS[targetId] || '◻︎';
    return safeGlyph(mod?.glyph, fallback);
  }

  return { knows, entries, kindOf, open, glyphFor };
}

const MODULE_GLYPHS = {
  ctox: '◆',
  documents: '📄',
  browser: '🌐',
  knowledge: '📚',
  matching: '🔗',
  outbound: '📣',
  research: '🔬',
  'coding-agents': '🤖',
};

function safeGlyph(value, fallback) {
  const glyph = String(value || '').trim();
  if (!glyph || glyph.length > 16) return fallback;
  if (/^(?:data:|https?:)/i.test(glyph) || /[<>{}]/.test(glyph)) return fallback;
  return glyph;
}
