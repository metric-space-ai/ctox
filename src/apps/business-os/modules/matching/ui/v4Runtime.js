// Laufzeit-Brücke matching ↔ App-API 4.0 (V4-PORT.md).
// Hält die App-API der aktuellen Montage, die Abmeldungen dieser Montage, den Fensterzustand
// (snapshot/restored statt storageScope) und die einzigen Wege für Befehle, Zustand und Dialoge.

let api = null;
let disposers = [];
let viewStore = new Map();

export function bindMatchingApi(nextApi, { restored } = {}) {
  api = nextApi || null;
  disposers = [];
  viewStore = new Map();
  const saved = restored && typeof restored === 'object' && restored.v === 1 ? restored.store : null;
  if (saved && typeof saved === 'object' && !Array.isArray(saved)) {
    for (const [key, value] of Object.entries(saved)) {
      if (typeof value === 'string') viewStore.set(key, value);
    }
  }
}

export function getMatchingApi() {
  return api;
}

/** Abmeldung, die beim unmount der aktuellen Montage läuft. */
export function onMatchingUnmount(fn) {
  if (typeof fn === 'function') disposers.push(fn);
  return fn;
}

export function disposeMatchingRuntime() {
  const list = disposers;
  disposers = [];
  for (const fn of list.reverse()) {
    try { fn(); } catch (error) { console.warn('[matching] unmount cleanup failed', error); }
  }
  api = null;
}

// Fensterzustand je Montage (ersetzt ctx.storageScope; geht über snapshot() in den Remount).
export const matchingStore = {
  get(key) { return viewStore.has(key) ? viewStore.get(key) : null; },
  set(key, value) { viewStore.set(String(key), String(value)); },
};

export function snapshotMatchingStore() {
  return Object.fromEntries(viewStore);
}

export function reportMatchingStatus(code, text, level = 'error', ref) {
  if (!api) return;
  const item = { level, code, text: String(text || code) };
  if (ref) item.ref = ref;
  api.status.report(item);
}

export function clearMatchingStatus(code) {
  api?.status.clear(code);
}

/**
 * Befehl ohne Warten (PORTING R5). Status: failed → Zustandszone, done → Zone leeren.
 * Liefert den CommandHandle (oder null ohne API).
 */
export function sendMatchingCommand(type, payload = {}, { onState } = {}) {
  if (!api) return null;
  const code = `matching.command.${type}`;
  const handle = api.command(type, payload);
  const off = handle.status.subscribe((state) => {
    if (state?.status === 'failed') reportMatchingStatus(code, state.error || type, 'error', handle.id);
    else if (state?.status === 'done') clearMatchingStatus(code);
    try { onState?.(state, handle); } catch (error) { console.warn('[matching] command state handler failed', error); }
    if (state?.status === 'done' || state?.status === 'failed') off?.();
  });
  onMatchingUnmount(() => off?.());
  handle.local.catch((error) => reportMatchingStatus(code, error?.message || String(error), 'error', handle.id));
  return handle;
}

// Dialoge nur innerhalb des App-Fensters (R7). Fehlermeldungen gehen in die Zustandszone (R11).
export async function matchingAlert(text, options = {}) {
  const title = String(options.title || '');
  if (/fehlgeschlagen|failed/i.test(title) || options.level === 'error') {
    reportMatchingStatus(options.code || 'matching.action.failed', title ? `${title}: ${text}` : text, 'error');
    return null;
  }
  if (!api) return null;
  return api.shell.dialog({ kind: 'alert', title: title || 'Matching', text: String(text || ''), confirm_label: options.okLabel || 'OK' });
}

export async function matchingConfirm(text, options = {}) {
  if (!api) return false;
  const answer = await api.shell.dialog({
    kind: 'confirm',
    title: String(options.title || 'Matching'),
    text: String(text || ''),
    confirm_label: options.confirmLabel || options.okLabel || 'OK',
    cancel_label: options.cancelLabel || 'Abbrechen',
    danger: options.danger === true || options.tone === 'danger',
    default: false,
  });
  return answer === true;
}

export async function matchingPrompt(text, options = {}) {
  if (!api) return null;
  const answer = await api.shell.dialog({
    kind: 'prompt',
    title: String(options.title || 'Matching'),
    text: String(text || ''),
    placeholder: options.placeholder || '',
    value: options.defaultValue ?? options.value ?? '',
    required: options.required === true,
    multiline: options.multiline === true,
    confirm_label: options.confirmLabel || options.okLabel || 'OK',
    default: null,
  });
  return typeof answer === 'string' ? answer : null;
}

export function matchingNavigate(appId, params = {}) {
  return api ? api.shell.navigate(appId, params) : false;
}
