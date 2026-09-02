// Workjet UI contract helpers shared by the public Business OS shell surfaces.
// The actual colors live in ui-contract/v1/workjet-ui-contract.css; keeping the
// mapping here limited to stable slugs prevents manifests from inventing a
// second category vocabulary or leaking private-module accents into the shell.

export const WORKJET_CATEGORY_IDS = Object.freeze([
  'workspace',
  'collaboration',
  'productivity',
  'development',
  'engineering',
  'knowledge',
  'research',
  'sales',
  'recruiting',
  'finance',
  'operations',
  'governance',
  'security',
  'analytics',
  'system',
  'imported',
]);

const WORKJET_CATEGORY_SET = new Set(WORKJET_CATEGORY_IDS);

const CATEGORY_ALIASES = Object.freeze({
  management: 'operations',
  manage: 'operations',
  recherche: 'research',
  'web-data': 'research',
  'web-and-data': 'research',
  'web-data-research': 'research',
  dev: 'development',
  engineering: 'engineering',
});

const PUBLIC_DISTRIBUTIONS = new Set([
  'public',
  'store',
  'system-module',
  'system',
  'official',
  'packaged',
]);

function slugifyCategory(value) {
  return String(value || '')
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

export function normalizeWorkjetCategory(value, fallback = 'imported') {
  const slug = slugifyCategory(value);
  const candidate = CATEGORY_ALIASES[slug] || slug;
  if (WORKJET_CATEGORY_SET.has(candidate)) return candidate;
  const safeFallback = slugifyCategory(fallback);
  return WORKJET_CATEGORY_SET.has(safeFallback) ? safeFallback : 'imported';
}

export function isPublicWorkjetModule(moduleDef) {
  if (!moduleDef || typeof moduleDef !== 'object' || Array.isArray(moduleDef)) return false;
  const manifest = moduleDef.manifest && typeof moduleDef.manifest === 'object' && !Array.isArray(moduleDef.manifest)
    ? moduleDef.manifest
    : {};
  const isPrivate = (value) => value?.private === true
    || value?.is_private === true
    || value?.customer_id
    || value?.customerId
    || value?.tenant_id
    || value?.tenantId
    || String(value?.visibility || '').trim().toLowerCase() === 'private';
  if (isPrivate(moduleDef) || isPrivate(manifest)) return false;
  if (moduleDef.core === true || manifest.core === true
      || String(moduleDef.source || manifest.source || '').trim().toLowerCase() === 'core') return true;
  const distribution = slugifyCategory(
    moduleDef.store?.distribution
      || moduleDef.distribution
      || moduleDef.store?.visibility
      || manifest.store?.distribution
      || manifest.distribution
      || manifest.store?.visibility
      || '',
  );
  return PUBLIC_DISTRIBUTIONS.has(distribution);
}

export function workjetCategoryForModule(moduleDef, fallback = 'imported') {
  const manifest = moduleDef?.manifest && typeof moduleDef.manifest === 'object' && !Array.isArray(moduleDef.manifest)
    ? moduleDef.manifest
    : {};
  const declared = moduleDef?.category
    || moduleDef?.store?.category
    || moduleDef?.layout?.category
    || manifest.category
    || manifest.store?.category
    || manifest.layout?.category;
  // Die Rubrik ist unabhängig von der Herkunft (APPSTORE-V2 K5): Apps mit
  // server-autoritativem origin behalten ihre deklarierte Rubrik; nur
  // origin:user ohne kanonische Rubrik fällt auf 'imported'. Projektionen
  // alter Daemons ohne origin-Feld laufen weiter über die alte
  // Public-Distribution-Heuristik, damit ein Alt-Binary mit neuer Shell
  // kohärent bleibt.
  const origin = String(moduleDef?.origin || manifest.origin || '').trim().toLowerCase();
  if (origin === 'core' || origin === 'official') {
    return normalizeWorkjetCategory(declared, fallback);
  }
  if (origin === 'user') {
    return normalizeWorkjetCategory(declared, 'imported');
  }
  if (!isPublicWorkjetModule(moduleDef)) return normalizeWorkjetCategory(fallback);
  return normalizeWorkjetCategory(declared, fallback);
}

export function workjetCategoryForTarget(target, fallback = 'imported') {
  if (target?.module) return workjetCategoryForModule(target.module, fallback);
  if (target?.kind === 'module') return workjetCategoryForModule(target, fallback);
  return normalizeWorkjetCategory(
    target?.category || target?.app?.category,
    fallback,
  );
}

export function workjetCategoryStyle(category, fallback = 'imported') {
  const id = normalizeWorkjetCategory(category, fallback);
  const prefix = `--workjet-category-${id}`;
  return Object.freeze({
    id,
    accent: `var(${prefix}-accent)`,
    foreground: `var(${prefix}-accent-foreground)`,
    soft: `var(${prefix}-accent-soft)`,
    border: `var(${prefix}-accent-border)`,
  });
}

export function applyWorkjetCategory(element, category, fallback = 'imported') {
  const style = workjetCategoryStyle(category, fallback);
  if (!element) return style.id;
  element.dataset.workjetCategory = style.id;
  element.style.setProperty('--shell-category-accent', style.accent);
  element.style.setProperty('--shell-category-foreground', style.foreground);
  element.style.setProperty('--shell-category-soft', style.soft);
  element.style.setProperty('--shell-category-border', style.border);
  return style.id;
}
