const WORKSPACE_BRANDING_STYLE_ID = 'ctox-workspace-branding-style';

export const WORKSPACE_BRANDING_COLLECTION = 'business_workspace_branding';
export const WORKSPACE_BRANDING_DOCUMENT_ID = 'workspace-branding';

export const BRANDING_TOKEN_TO_CSS_VAR = Object.freeze({
  bg: '--bg',
  surface: '--surface',
  surface_2: '--surface-2',
  line: '--line',
  text: '--text',
  text_strong: '--text-strong',
  muted: '--muted',
  accent: '--accent',
  accent_soft: '--accent-soft',
  accent_foreground: '--accent-foreground',
  danger: '--danger',
  warning: '--warning',
  success: '--success',
  focus_ring: '--focus-ring',
});

const SAFE_COLOR_RE = /^(#[0-9a-f]{3,8}|rgba?\([0-9a-zA-Z%.,\s/+-]+\)|hsla?\([0-9a-zA-Z%.,\s/+-]+\)|oklch\([0-9a-zA-Z%.,\s/+-]+\)|oklab\([0-9a-zA-Z%.,\s/+-]+\))$/i;

export function normalizeWorkspaceBranding(document = null) {
  const source = document?.toJSON?.() || document || {};
  if (
    !source
    || Object.keys(source).length === 0
    || source._deleted === true
    || source.is_deleted === true
    || source.custom === false
  ) {
    return defaultWorkspaceBranding();
  }
  return {
    id: WORKSPACE_BRANDING_DOCUMENT_ID,
    name: cleanBrandingName(source.name) || 'Workspace Branding',
    custom: true,
    light: normalizeTokenSet(source.light),
    dark: normalizeTokenSet(source.dark),
    module_accents: normalizeModuleAccents(source.module_accents),
    updated_at_ms: Number(source.updated_at_ms || 0),
  };
}

export function defaultWorkspaceBranding() {
  return {
    id: WORKSPACE_BRANDING_DOCUMENT_ID,
    name: 'CTOX Default',
    custom: false,
    light: {},
    dark: {},
    module_accents: {},
    updated_at_ms: 0,
  };
}

export function workspaceBrandingStyleText(branding = null) {
  const normalized = normalizeWorkspaceBranding(branding);
  if (!normalized.custom) return '';
  const light = tokenSetToCss(normalized.light);
  const dark = tokenSetToCss(normalized.dark);
  const blocks = [];
  if (light) {
    blocks.push(`:root[data-workspace-branding="custom"] {\n${light}\n}`);
  }
  if (dark) {
    blocks.push(`:root[data-workspace-branding="custom"][data-theme="dark"] {\n${dark}\n}`);
  }
  return blocks.join('\n\n');
}

export function applyWorkspaceBranding(document = null) {
  const branding = normalizeWorkspaceBranding(document);
  let style = globalThis.document?.getElementById?.(WORKSPACE_BRANDING_STYLE_ID) || null;
  const css = workspaceBrandingStyleText(branding);
  if (!css) {
    style?.remove?.();
    globalThis.document?.documentElement?.removeAttribute?.('data-workspace-branding');
    return branding;
  }
  if (!style) {
    style = globalThis.document.createElement('style');
    style.id = WORKSPACE_BRANDING_STYLE_ID;
    style.dataset.workspaceBranding = 'true';
    globalThis.document.head.appendChild(style);
  }
  style.textContent = css;
  globalThis.document.documentElement.dataset.workspaceBranding = 'custom';
  return branding;
}

export function brandingForPreferencePayload(branding = null) {
  const normalized = normalizeWorkspaceBranding(branding);
  return {
    name: normalized.name,
    custom: normalized.custom,
    light: { ...normalized.light },
    dark: { ...normalized.dark },
    module_accents: { ...normalized.module_accents },
    updated_at_ms: normalized.updated_at_ms,
  };
}

export function normalizeBrandingImportPayload(raw) {
  const payload = typeof raw === 'string' ? JSON.parse(raw) : raw;
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) {
    throw new Error('Branding JSON muss ein Objekt sein.');
  }
  if (!payload.light || typeof payload.light !== 'object' || Array.isArray(payload.light)) {
    throw new Error('Branding JSON braucht ein light Objekt.');
  }
  if (!payload.dark || typeof payload.dark !== 'object' || Array.isArray(payload.dark)) {
    throw new Error('Branding JSON braucht ein dark Objekt.');
  }
  return {
    name: cleanBrandingName(payload.name) || 'Workspace Branding',
    light: normalizeTokenSet(payload.light, { strict: true, label: 'light' }),
    dark: normalizeTokenSet(payload.dark, { strict: true, label: 'dark' }),
    module_accents: normalizeModuleAccents(payload.module_accents, { strict: true }),
  };
}

export function brandingExportJson(branding = null) {
  const normalized = normalizeWorkspaceBranding(branding);
  return JSON.stringify({
    name: normalized.name,
    light: normalized.light,
    dark: normalized.dark,
    module_accents: normalized.module_accents,
  }, null, 2);
}

function tokenSetToCss(tokens = {}) {
  return Object.entries(tokens)
    .map(([token, value]) => {
      const cssVar = BRANDING_TOKEN_TO_CSS_VAR[token];
      if (!cssVar || !isSafeBrandingColor(value)) return '';
      return `  ${cssVar}: ${value};`;
    })
    .filter(Boolean)
    .join('\n');
}

function normalizeTokenSet(value = {}, options = {}) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    if (options.strict) throw new Error(`Branding ${options.label || 'tokens'} muss ein Objekt sein.`);
    return {};
  }
  const out = {};
  for (const [token, raw] of Object.entries(value)) {
    if (!Object.prototype.hasOwnProperty.call(BRANDING_TOKEN_TO_CSS_VAR, token)) {
      if (options.strict) throw new Error(`Unbekannter Branding Token: ${token}`);
      continue;
    }
    const color = String(raw || '').trim();
    if (!isSafeBrandingColor(color)) {
      if (options.strict) throw new Error(`Unsicherer Branding Wert fuer ${token}.`);
      continue;
    }
    out[token] = color;
  }
  return out;
}

function normalizeModuleAccents(value = {}, options = {}) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    if (options.strict && value != null) throw new Error('module_accents muss ein Objekt sein.');
    return {};
  }
  const out = {};
  for (const [moduleId, raw] of Object.entries(value)) {
    const id = String(moduleId || '')
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9._-]+/g, '-')
      .replace(/^-+|-+$/g, '');
    if (!id) continue;
    const color = String(raw || '').trim();
    if (isSafeBrandingColor(color)) {
      out[id] = color;
    } else if (options.strict) {
      throw new Error(`Unsicherer Modul-Akzent fuer ${moduleId}.`);
    }
  }
  return out;
}

function isSafeBrandingColor(value) {
  const text = String(value || '').trim();
  if (!text || text.length > 96) return false;
  if (/[;{}<>]|url\s*\(|var\s*\(|attr\s*\(|calc\s*\(|import/i.test(text)) return false;
  return SAFE_COLOR_RE.test(text);
}

function cleanBrandingName(value) {
  return String(value || '').trim().replace(/\s+/g, ' ').slice(0, 80);
}
