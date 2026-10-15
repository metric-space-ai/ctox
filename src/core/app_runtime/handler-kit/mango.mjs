import { isDeepStrictEqual } from 'node:util';

export class CtoxError extends Error {
  constructor(code, message, path) {
    super(message);
    this.name = 'CtoxError';
    this.code = code;
    if (path !== undefined) this.path = path;
  }
}
export const invalid = (message, path) => { throw new CtoxError('invalid', message, path); };
const plain = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const has = (value, key) => value != null && Object.hasOwn(value, key);
const MISSING = Symbol('missing');
const operators = new Set(['$eq', '$ne', '$gt', '$gte', '$lt', '$lte', '$in', '$nin', '$exists', '$regex', '$options', '$not', '$elemMatch']);
const field = (doc, path) => path.split('.').reduce((value, part) => has(value, part) ? value[part] : MISSING, doc);
const equal = (a, b) => a !== MISSING && isDeepStrictEqual(a, b);
function validateCondition(condition, depth = 0) {
  if (!plain(condition) || !Object.keys(condition).some(key => key.startsWith('$'))) return;
  for (const [op, value] of Object.entries(condition)) {
    if (!operators.has(op)) invalid('Unsupported Mango operator', op);
    if (['$in', '$nin'].includes(op) && !Array.isArray(value)) invalid(op + ' requires an array');
    if (op === '$exists' && typeof value !== 'boolean') invalid('$exists requires a boolean');
    if (op === '$options' && (value !== 'i' || !has(condition, '$regex'))) invalid('Only regex flag i is supported');
    if (op === '$regex') {
      if (typeof value !== 'string') invalid('$regex requires a string');
      try { new RegExp(value, condition.$options ?? ''); } catch { invalid('Invalid regular expression'); }
    }
    if (op === '$elemMatch') {
      if (depth >= 1 || !plain(value)) invalid('$elemMatch supports one level');
      if (Object.keys(value).some(key => key.startsWith('$') && !['$and', '$or', '$not'].includes(key))) validateCondition(value, depth + 1);
      else validateSelector(value, depth + 1);
    }
    if (op === '$not') validateCondition(value, depth);
  }
}
function validateSelector(selector, depth = 0) {
  if (!plain(selector)) invalid('Mango selector must be an object');
  for (const [key, value] of Object.entries(selector)) {
    if (key === '$and' || key === '$or') {
      if (!Array.isArray(value) || value.length === 0) invalid(key + ' requires nonempty selectors');
      value.forEach(item => validateSelector(item, depth));
    } else if (key === '$not') validateSelector(value, depth);
    else if (key.startsWith('$')) invalid('Unsupported Mango selector operator', key);
    else {
      if (!key || key.split('.').some(part => !part || ['__proto__', 'prototype', 'constructor'].includes(part))) invalid('Invalid Mango field', key);
      validateCondition(value, depth);
    }
  }
}
function matchesCondition(actual, condition) {
  if (!plain(condition) || !Object.keys(condition).some(key => key.startsWith('$'))) return equal(actual, condition);
  return Object.entries(condition).every(([op, expected]) => {
    switch (op) {
      case '$eq': return equal(actual, expected);
      case '$ne': return !equal(actual, expected);
      case '$gt': return actual !== MISSING && actual > expected;
      case '$gte': return actual !== MISSING && actual >= expected;
      case '$lt': return actual !== MISSING && actual < expected;
      case '$lte': return actual !== MISSING && actual <= expected;
      case '$in': return expected.some(item => equal(actual, item));
      case '$nin': return !expected.some(item => equal(actual, item));
      case '$exists': return (actual !== MISSING) === expected;
      case '$regex': return typeof actual === 'string' && new RegExp(expected, condition.$options ?? '').test(actual);
      case '$options': return true;
      case '$not': return !matchesCondition(actual, expected);
      case '$elemMatch': return Array.isArray(actual) && actual.some(item =>
        Object.keys(expected).some(key => key.startsWith('$') && !['$and', '$or', '$not'].includes(key))
          ? matchesCondition(item, expected) : matchesSelector(item, expected));
      default: return false;
    }
  });
}
function matchesSelector(doc, selector) {
  return Object.entries(selector).every(([key, expected]) => {
    if (key === '$and') return expected.every(item => matchesSelector(doc, item));
    if (key === '$or') return expected.some(item => matchesSelector(doc, item));
    if (key === '$not') return !matchesSelector(doc, expected);
    return matchesCondition(field(doc, key), expected);
  });
}
const compare = (a, b) => {
  if (isDeepStrictEqual(a, b)) return 0;
  const rank = value => value === MISSING ? 0 : value === null ? 1 : typeof value === 'boolean' ? 2 : typeof value === 'number' ? 3 : typeof value === 'string' ? 4 : Array.isArray(value) ? 5 : 6;
  const ar = rank(a), br = rank(b);
  if (ar !== br) return ar - br;
  if (typeof a === 'number' || typeof a === 'boolean' || typeof a === 'string') return a < b ? -1 : 1;
  return JSON.stringify(a) < JSON.stringify(b) ? -1 : 1;
};

/** Normative v4 subset: missing differs from null; ties sort by id; no implicit limit. */
export function mango(docs, query = { selector: {} }) {
  if (!plain(query) || Object.keys(query).some(key => !['selector', 'sort', 'limit', 'skip'].includes(key))) invalid('Invalid Mango query');
  validateSelector(query.selector ?? {});
  for (const name of ['limit', 'skip']) if (has(query, name) && (!Number.isSafeInteger(query[name]) || query[name] < 0)) invalid('Invalid Mango ' + name);
  const sort = query.sort ?? [];
  if (!Array.isArray(sort)) invalid('Mango sort must be an array');
  for (const item of sort) {
    if (!plain(item) || Object.keys(item).length !== 1 || !['asc', 'desc'].includes(Object.values(item)[0])) invalid('Invalid Mango sort');
    validateSelector({ [Object.keys(item)[0]]: null });
  }
  const result = docs.filter(doc => matchesSelector(doc, query.selector ?? {})).sort((a, b) => {
    for (const item of sort) {
      const [key, direction] = Object.entries(item)[0];
      const order = compare(field(a, key), field(b, key));
      if (order) return direction === 'desc' ? -order : order;
    }
    return compare(a.id, b.id);
  });
  const skip = query.skip ?? 0;
  return structuredClone(query.limit === undefined ? result.slice(skip) : result.slice(skip, skip + query.limit));
}
