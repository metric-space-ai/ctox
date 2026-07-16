const RESTORABLE_OWNER_PREFIX = 'desktop-app:';

export function buildWorkspaceSessionSnapshot(windows, activeModuleId, now = Date.now()) {
  const entries = (Array.isArray(windows) ? windows : [])
    .filter((entry) => String(entry?.ownerId || '').startsWith(RESTORABLE_OWNER_PREFIX))
    .map((entry) => ({
      ownerId: String(entry.ownerId),
      state: ['normal', 'minimized', 'maximized'].includes(entry.state) ? entry.state : 'normal',
      appMode: ['window', 'maximized', 'focus'].includes(entry.appMode) ? entry.appMode : 'window',
      focused: entry.isFocused === true,
    }));
  return {
    version: 1,
    updatedAtMs: Number(now) || Date.now(),
    activeModuleId: String(activeModuleId || '').trim(),
    windows: entries,
  };
}

export function normalizeWorkspaceSessionSnapshot(value) {
  if (!value || typeof value !== 'object' || Number(value.version) !== 1) return null;
  const seen = new Set();
  const windows = [];
  for (const entry of Array.isArray(value.windows) ? value.windows : []) {
    const ownerId = String(entry?.ownerId || '').trim();
    if (!ownerId.startsWith(RESTORABLE_OWNER_PREFIX) || seen.has(ownerId)) continue;
    seen.add(ownerId);
    windows.push({
      ownerId,
      state: ['normal', 'minimized', 'maximized'].includes(entry.state) ? entry.state : 'normal',
      appMode: ['window', 'maximized', 'focus'].includes(entry.appMode) ? entry.appMode : 'window',
      focused: entry.focused === true,
    });
  }
  return {
    version: 1,
    updatedAtMs: Number(value.updatedAtMs || 0),
    activeModuleId: String(value.activeModuleId || '').trim(),
    windows,
  };
}
