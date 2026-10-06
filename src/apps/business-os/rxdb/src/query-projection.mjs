// Shared inclusion projection for demand-query rows. Partial rows never enter
// canonical document storage and are not mutation-capable RxDocuments.
export function normalizeQueryProjection(projection, primaryPath = null) {
  if (projection == null) return null;
  if (!Array.isArray(projection)) throw new TypeError('projection must be an array of field paths');
  const fields = new Set();
  for (const field of projection) {
    if (typeof field !== 'string' || !field || field.split('.').some((part) => (
      !part || part === '__proto__' || part === 'constructor' || part === 'prototype'
    ))) throw new TypeError('projection requires safe non-empty field paths');
    fields.add(field);
  }
  if (fields.size && primaryPath) {
    fields.add(primaryPath);
    fields.add('_rev');
    fields.add('_deleted');
  }
  const ordered = [...fields].sort(compareCodePoints);
  const normalized = [];
  for (const field of ordered) {
    if (!normalized.some((parent) => field.startsWith(`${parent}.`))) normalized.push(field);
  }
  return normalized.length ? normalized : null;
}

// Code-point order matches Rust strings, including non-BMP field names.
function compareCodePoints(left, right) {
  const a = Array.from(left, (value) => value.codePointAt(0));
  const b = Array.from(right, (value) => value.codePointAt(0));
  for (let index = 0; index < Math.min(a.length, b.length); index += 1) {
    if (a[index] !== b[index]) return a[index] - b[index];
  }
  return a.length - b.length;
}

export function projectQueryDocument(document, projection) {
  if (!projection?.length) return cloneQueryRow(document);
  const tree = Object.create(null);
  for (const field of projection) {
    let node = tree;
    for (const part of field.split('.')) node = node[part] ||= Object.create(null);
  }
  return projectTree(document, tree);
}

function projectTree(value, tree) {
  if (Object.keys(tree).length === 0) return cloneQueryRow(value);
  if (Array.isArray(value)) return value.map((entry) => projectTree(entry, tree));
  if (value == null || typeof value !== 'object') return null;
  const out = {};
  for (const [field, child] of Object.entries(tree)) {
    if (!Object.hasOwn(value, field)) continue;
    const nested = projectTree(value[field], child);
    if (nested !== null || value[field] === null) out[field] = nested;
  }
  return out;
}

export function cloneQueryRow(row) {
  return row == null ? row : JSON.parse(JSON.stringify(row));
}

export function projectedDocumentWriteError() {
  return Object.assign(new Error('PROJECTED_DOCUMENT_READ_ONLY: hydrate the full document before mutation'), {
    code: 'PROJECTED_DOCUMENT_READ_ONLY',
    retryable: false,
  });
}
