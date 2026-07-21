const VIEW_STATE_PREFIX = 'matching:view-state:';
let storageScope = null;

function asPlainObject(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  return value;
}

function buildKey(viewName) {
  return `${VIEW_STATE_PREFIX}${String(viewName || '').trim()}`;
}

export function setViewStateStorageScope(nextStorageScope) {
  storageScope = nextStorageScope?.get && nextStorageScope?.set ? nextStorageScope : null;
}

export function readViewState(viewName, defaults = {}) {
  const safeDefaults = asPlainObject(defaults) || {};
  const key = buildKey(viewName);
  if (!storageScope || !key || key === VIEW_STATE_PREFIX) return { ...safeDefaults };
  try {
    const raw = storageScope.get(key);
    if (!raw) return { ...safeDefaults };
    const data = asPlainObject(JSON.parse(raw));
    return data ? { ...safeDefaults, ...data } : { ...safeDefaults };
  } catch {
    return { ...safeDefaults };
  }
}

export function writeViewState(viewName, nextState) {
  const key = buildKey(viewName);
  if (!storageScope || !key || key === VIEW_STATE_PREFIX) return;
  const safeState = asPlainObject(nextState) || {};
  try { storageScope.set(key, JSON.stringify(safeState)); } catch {}
}

export function patchViewState(viewName, patch, defaults = {}) {
  const safePatch = asPlainObject(patch) || {};
  const merged = { ...readViewState(viewName, defaults), ...safePatch };
  writeViewState(viewName, merged);
  return merged;
}
