const VIEW_STATE_PREFIX = "nwt:view-state:";

function asPlainObject(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  return value;
}

function getLocalStorage() {
  try {
    if (typeof window === "undefined" || !window.localStorage) return null;
    return window.localStorage;
  } catch {
    return null;
  }
}

function buildKey(viewName) {
  return `${VIEW_STATE_PREFIX}${String(viewName || "").trim()}`;
}

export function readViewState(viewName, defaults = {}) {
  const safeDefaults = asPlainObject(defaults) || {};
  const storage = getLocalStorage();
  if (!storage) return { ...safeDefaults };

  const key = buildKey(viewName);
  if (!key || key === VIEW_STATE_PREFIX) return { ...safeDefaults };

  try {
    const raw = storage.getItem(key);
    if (!raw) return { ...safeDefaults };
    const parsed = JSON.parse(raw);
    const data = asPlainObject(parsed);
    if (!data) return { ...safeDefaults };
    return { ...safeDefaults, ...data };
  } catch {
    return { ...safeDefaults };
  }
}

export function writeViewState(viewName, nextState) {
  const storage = getLocalStorage();
  if (!storage) return;

  const key = buildKey(viewName);
  if (!key || key === VIEW_STATE_PREFIX) return;

  const safeState = asPlainObject(nextState) || {};
  try {
    storage.setItem(key, JSON.stringify(safeState));
  } catch {}
}

export function patchViewState(viewName, patch, defaults = {}) {
  const safePatch = asPlainObject(patch) || {};
  const merged = {
    ...readViewState(viewName, defaults),
    ...safePatch
  };
  writeViewState(viewName, merged);
  return merged;
}
