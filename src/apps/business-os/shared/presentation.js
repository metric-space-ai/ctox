// Canonical Business OS app presentation contract with a bounded legacy reader.

const MODES = new Set(['window', 'maximized', 'focus']);

function positiveInt(value, fallback) {
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

function legacyIsWindowed(moduleDef) {
  return moduleDef?.launch_kind === 'desktop-app'
    || moduleDef?.layout?.launch_kind === 'desktop-app'
    || moduleDef?.layout?.shell === 'windowed'
    || moduleDef?.layout?.shell === 'desktop-window';
}

function legacyIsFullWorkspace(moduleDef) {
  return moduleDef?.layout?.shell === 'full-workspace'
    || moduleDef?.layout?.full_workspace === true
    || moduleDef?.layout?.fullFrame === true;
}

export function resolvePresentation(moduleDef = {}) {
  const source = moduleDef.presentation && typeof moduleDef.presentation === 'object'
    ? moduleDef.presentation
    : {};
  const explicitMode = MODES.has(source.default_mode) ? source.default_mode : '';
  const legacyWindowed = legacyIsWindowed(moduleDef);
  const legacyFullWorkspace = legacyIsFullWorkspace(moduleDef);
  const defaultMode = explicitMode || (legacyWindowed ? 'window' : 'workspace');
  const requestedModes = Array.isArray(source.supported_modes)
    ? source.supported_modes.filter((mode) => MODES.has(mode))
    : [];
  const supportedModes = Array.from(new Set([
    ...(defaultMode === 'workspace' ? [] : [defaultMode]),
    ...requestedModes,
  ]));
  if (defaultMode !== 'workspace' && !supportedModes.includes('window')) {
    supportedModes.unshift('window');
  }

  return Object.freeze({
    defaultMode,
    supportedModes: Object.freeze(supportedModes),
    initialSize: Object.freeze({
      width: positiveInt(source.initial_size?.width, positiveInt(moduleDef?.layout?.default_width, 1080)),
      height: positiveInt(source.initial_size?.height, positiveInt(moduleDef?.layout?.default_height, 720)),
    }),
    minimumSize: Object.freeze({
      width: positiveInt(source.minimum_size?.width, positiveInt(moduleDef?.layout?.min_width, 640)),
      height: positiveInt(source.minimum_size?.height, positiveInt(moduleDef?.layout?.min_height, 480)),
    }),
    multiInstance: source.multi_instance === true,
    autoRestore: source.auto_restore === true,
    legacy: Object.freeze({ windowed: legacyWindowed, fullWorkspace: legacyFullWorkspace }),
  });
}

export function launchesInWindow(moduleDef) {
  return resolvePresentation(moduleDef).defaultMode !== 'workspace';
}

export function usesLegacyWorkspace(moduleDef) {
  return resolvePresentation(moduleDef).legacy.fullWorkspace;
}
