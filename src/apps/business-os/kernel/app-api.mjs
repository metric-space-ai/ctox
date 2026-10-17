// App API 4.0 query and schema primitives derived from the executable contract.
import { CtoxError, isPlainObject, deepEqual } from './values.mjs';
const FIELD_OPS = new Set(['$eq', '$ne', '$gt', '$gte', '$lt', '$lte', '$in', '$nin', '$exists', '$regex', '$options', '$not', '$elemMatch']);
const isOpObject = (c) => isPlainObject(c) && Object.keys(c).length > 0 && Object.keys(c).every((k) => k.startsWith('$'));

function validateSelector(sel, path, inElem) {
  if (!isPlainObject(sel)) throw new CtoxError('invalid', `Selektor muss ein Objekt sein (${path})`, { path });
  for (const [k, v] of Object.entries(sel)) {
    if (k === '$and' || k === '$or') {
      if (!Array.isArray(v) || !v.length) throw new CtoxError('invalid', `${k} braucht ein nichtleeres Array`, { path: `${path}.${k}` });
      v.forEach((s, i) => validateSelector(s, `${path}.${k}[${i}]`, inElem));
    } else if (k === '$not') validateSelector(v, `${path}.$not`, inElem);
    else if (k.startsWith('$')) throw new CtoxError('invalid', `Unbekannter Operator ${k}`, { path: `${path}.${k}` });
    else validateCond(v, `${path}.${k}`, inElem);
  }
}
function validateCond(c, path, inElem) {
  if (c instanceof RegExp) return validateRegex(c, path);
  if (!isOpObject(c)) return;
  for (const [op, arg] of Object.entries(c)) {
    if (!FIELD_OPS.has(op)) throw new CtoxError('invalid', `Unbekannter Operator ${op}`, { path: `${path}.${op}` });
    if ((op === '$in' || op === '$nin') && !Array.isArray(arg)) throw new CtoxError('invalid', `${op} braucht ein Array`, { path: `${path}.${op}` });
    if (op === '$exists' && typeof arg !== 'boolean') throw new CtoxError('invalid', '$exists braucht true|false', { path: `${path}.${op}` });
    if (op === '$regex') {
      if (arg instanceof RegExp) validateRegex(arg, path);
      else if (typeof arg !== 'string') throw new CtoxError('invalid', '$regex braucht string oder RegExp', { path: `${path}.${op}` });
      if (c.$options !== undefined && !/^i?$/.test(c.$options)) throw new CtoxError('invalid', 'Nur Flag i erlaubt', { path: `${path}.$options` });
    }
    if (op === '$options' && c.$regex === undefined) throw new CtoxError('invalid', '$options nur mit $regex', { path: `${path}.${op}` });
    if (op === '$not') validateCond(arg, `${path}.$not`, inElem);
    if (op === '$elemMatch') {
      if (inElem) throw new CtoxError('invalid', '$elemMatch nur eine Ebene', { path: `${path}.${op}` });
      if (!isPlainObject(arg)) throw new CtoxError('invalid', '$elemMatch braucht ein Objekt', { path: `${path}.${op}` });
      if (isOpObject(arg)) validateCond(arg, `${path}.$elemMatch`, true);
      else validateSelector(arg, `${path}.$elemMatch`, true);
    }
  }
}
function validateRegex(re, path) {
  if (re.flags.replace('i', '') !== '') throw new CtoxError('invalid', 'Nur Regex-Flag i erlaubt', { path });
}
function getPath(doc, path) {
  let cur = doc;
  for (const part of path.split('.')) {
    if (cur === null || typeof cur !== 'object' || !Object.prototype.hasOwnProperty.call(cur, part)) return undefined;
    cur = cur[part];
  }
  return cur;
}
function comparable(a, b) {
  return (typeof a === 'number' && typeof b === 'number') || (typeof a === 'string' && typeof b === 'string');
}
function matchSelector(doc, sel) {
  for (const [k, cond] of Object.entries(sel)) {
    if (k === '$and') { if (!cond.every((s) => matchSelector(doc, s))) return false; }
    else if (k === '$or') { if (!cond.some((s) => matchSelector(doc, s))) return false; }
    else if (k === '$not') { if (matchSelector(doc, cond)) return false; }
    else if (!matchCond(getPath(doc, k), cond)) return false;
  }
  return true;
}
function matchCond(v, cond) {
  if (cond instanceof RegExp) return typeof v === 'string' && cond.test(v);
  if (!isOpObject(cond)) return v !== undefined && deepEqual(v, cond);
  for (const [op, arg] of Object.entries(cond)) {
    switch (op) {
      case '$eq': if (v === undefined || !deepEqual(v, arg)) return false; break;
      case '$ne': if (v !== undefined && deepEqual(v, arg)) return false; break;
      case '$gt': if (!(comparable(v, arg) && v > arg)) return false; break;
      case '$gte': if (!(comparable(v, arg) && v >= arg)) return false; break;
      case '$lt': if (!(comparable(v, arg) && v < arg)) return false; break;
      case '$lte': if (!(comparable(v, arg) && v <= arg)) return false; break;
      case '$in': if (v === undefined || !arg.some((a) => deepEqual(v, a))) return false; break;
      case '$nin': if (v !== undefined && arg.some((a) => deepEqual(v, a))) return false; break;
      case '$exists': if ((v !== undefined) !== arg) return false; break;
      case '$regex': {
        const re = arg instanceof RegExp ? arg : new RegExp(arg, cond.$options === 'i' ? 'i' : '');
        if (typeof v !== 'string' || !re.test(v)) return false;
        break;
      }
      case '$options': break;
      case '$not': if (matchCond(v, arg)) return false; break;
      case '$elemMatch':
        if (!Array.isArray(v) || !v.some((el) => (isOpObject(arg) ? matchCond(el, arg) : isPlainObject(el) && matchSelector(el, arg)))) return false;
        break;
      default: return false;
    }
  }
  return true;
}
// Typordnung für sort: fehlend < null < Zahl < String < Boolean < Rest; Gleichstand nach id.
function typeRank(v) {
  if (v === undefined) return 0;
  if (v === null) return 1;
  if (typeof v === 'number') return 2;
  if (typeof v === 'string') return 3;
  if (typeof v === 'boolean') return 4;
  return 5;
}
function compareValues(a, b) {
  const ra = typeRank(a), rb = typeRank(b);
  if (ra !== rb) return ra - rb;
  if (ra === 2 || ra === 3) return a < b ? -1 : a > b ? 1 : 0;
  if (ra === 4) return a === b ? 0 : a ? 1 : -1;
  if (ra === 5) { const sa = JSON.stringify(a), sb = JSON.stringify(b); return sa < sb ? -1 : sa > sb ? 1 : 0; }
  return 0;
}
export function compileQuery(q = {}) {
  if (!isPlainObject(q)) throw new CtoxError('invalid', 'Query muss ein Objekt sein');
  for (const k of Object.keys(q)) if (!['selector', 'sort', 'limit', 'skip'].includes(k)) throw new CtoxError('invalid', `Unbekannter Query-Schlüssel ${k}`, { path: k });
  const selector = q.selector ?? {};
  validateSelector(selector, 'selector', false);
  const sort = q.sort ?? [];
  if (!Array.isArray(sort)) throw new CtoxError('invalid', 'sort muss eine Liste [{feld: asc|desc}] sein', { path: 'sort' });
  const sortSpec = sort.map((s, i) => {
    if (!isPlainObject(s) || Object.keys(s).length !== 1) throw new CtoxError('invalid', 'sort-Eintrag braucht genau ein Feld', { path: `sort[${i}]` });
    const [field, dir] = Object.entries(s)[0];
    if (dir !== 'asc' && dir !== 'desc') throw new CtoxError('invalid', 'Richtung asc|desc', { path: `sort[${i}].${field}` });
    return [field, dir === 'desc' ? -1 : 1];
  });
  for (const k of ['limit', 'skip']) {
    if (q[k] !== undefined && !(Number.isInteger(q[k]) && q[k] >= 0)) throw new CtoxError('invalid', `${k} muss ganzzahlig ≥ 0 sein`, { path: k });
  }
  return { selector, sortSpec, limit: q.limit, skip: q.skip ?? 0 };
}
export function runCompiledQuery(docs, cq) {
  const out = [];
  for (const d of docs) if (matchSelector(d, cq.selector)) out.push(d);
  out.sort((a, b) => {
    for (const [f, dir] of cq.sortSpec) {
      const c = compareValues(getPath(a, f), getPath(b, f));
      if (c) return c * dir;
    }
    return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
  });
  const start = cq.skip;
  return cq.limit === undefined ? out.slice(start) : out.slice(start, start + cq.limit);
}

// ---------------------------------------------------------------------------------------------------------------
// Lokale Schemaprüfung (§16.10): JSON-Werte + Teilmenge JSON-Schema (type, required, properties, items, enum,
// additionalProperties:false) aus dem Manifest der schreibenden App-Version.
// ---------------------------------------------------------------------------------------------------------------
export function normalizeJson(v, path, ancestors = new Set()) {
  if(v === null || typeof v === 'string' || typeof v === 'boolean')return v;
  if(typeof v === 'number') {
    if(!Number.isFinite(v))throw new CtoxError('invalid','Non-finite number',{path});
    return v;
  }
  if(!Array.isArray(v) && !isPlainObject(v))throw new CtoxError('invalid','Expected a JSON value',{path});
  if(ancestors.has(v))throw new CtoxError('invalid','Cyclic JSON value',{path});
  ancestors.add(v);
  try {
    if(Array.isArray(v))return v.map((x,i)=>normalizeJson(x===undefined?null:x,`${path}[${i}]`,ancestors));
    const out={};
    for(const [key,value] of Object.entries(v))if(value!==undefined)Object.defineProperty(out,key,{value:normalizeJson(value,path?`${path}.${key}`:key,ancestors),enumerable:true,writable:true,configurable:true});
    return out;
  } finally {ancestors.delete(v);}
}
function jsonType(v) {
  if (v === null) return 'null';
  if (Array.isArray(v)) return 'array';
  if (typeof v === 'number') return Number.isInteger(v) ? 'integer' : 'number';
  return typeof v;
}
export function validateSchema(v, schema, path) {
  if (!schema || typeof schema !== 'object') return;
  if (schema.type) {
    const types = Array.isArray(schema.type) ? schema.type : [schema.type];
    const t = jsonType(v);
    if (!types.includes(t) && !(t === 'integer' && types.includes('number'))) throw new CtoxError('invalid', `Typ ${t}, erwartet ${types.join('|')}`, { path });
  }
  if (schema.enum && !schema.enum.some((e) => deepEqual(e, v))) throw new CtoxError('invalid', 'Wert nicht in enum', { path });
  if (isPlainObject(v)) {
    for (const r of schema.required ?? []) if (v[r] === undefined) throw new CtoxError('invalid', `Pflichtfeld ${r} fehlt`, { path: path ? `${path}.${r}` : r });
    for (const [k, x] of Object.entries(v)) {
      const sub = schema.properties?.[k];
      if (sub) validateSchema(x, sub, path ? `${path}.${k}` : k);
      else if (schema.additionalProperties === false) throw new CtoxError('invalid', `Feld ${k} nicht erlaubt`, { path: path ? `${path}.${k}` : k });
    }
  }
  if (Array.isArray(v) && schema.items) v.forEach((x, i) => validateSchema(x, schema.items, `${path}[${i}]`));
}
