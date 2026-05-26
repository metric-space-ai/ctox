export function createCtoxLauncher({ modules, apps, currentModuleId, openApp }) {
  const moduleDirectory = new Map(
    (modules || [])
      .filter((mod) => mod && mod.id && mod.id !== currentModuleId)
      .map((mod) => [mod.id, mod])
  );
  const appDirectory = new Map(
    (apps || [])
      .filter((app) => app && app.id)
      .map((app) => [app.id, app])
  );

  function knows(targetId) {
    return moduleDirectory.has(targetId) || appDirectory.has(targetId);
  }

  function entries() {
    return [
      ...Array.from(moduleDirectory.values()).map((mod) => ({ ...mod, kind: 'module' })),
      ...Array.from(appDirectory.values()).map((app) => ({ ...app, kind: 'app' })),
    ];
  }

  function kindOf(targetId) {
    if (appDirectory.has(targetId)) return 'app';
    if (moduleDirectory.has(targetId)) return 'module';
    return null;
  }

  function open(targetId, { recordId } = {}) {
    if (!targetId) return false;
    if (appDirectory.has(targetId)) {
      openApp?.(targetId);
      return true;
    }
    if (!moduleDirectory.has(targetId)) return false;
    const hash = recordId
      ? `#${encodeURIComponent(targetId)}?record=${encodeURIComponent(recordId)}`
      : `#${encodeURIComponent(targetId)}`;
    if (location.hash === hash) {
      window.dispatchEvent(new HashChangeEvent('hashchange'));
    } else {
      location.hash = hash;
    }
    return true;
  }

  function glyphFor(targetId) {
    const app = appDirectory.get(targetId);
    if (app?.glyph) return app.glyph;
    return MODULE_GLYPHS[targetId] || '◻︎';
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
