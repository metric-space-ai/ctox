/**
 * Utility constants and functions
 */
import {
  Any,
  AnyObject,
  ArrayOrObject,
  Callback,
  Comparator,
  HashFunction
} from "./types";

/** Represents an error reported by the mingo library. */
export class MingoError extends Error {}

// special value to identify missing items. treated differently from undefined
const MISSING = Symbol("missing");

const CYCLE_FOUND_ERROR = Object.freeze(
  new Error("mingo: cycle detected while processing object/array")
) as Error;

type Constructor = new (...args: Any[]) => Any;

/**
 * Uses the simple hash method as described in Effective Java.
 * @see https://stackoverflow.com/a/113600/1370481
 * @param value The value to hash
 * @returns {number}
 */
const DEFAULT_HASH_FUNCTION: HashFunction = (value: Any): number => {
  const s = stringify(value);
  let hash = 0;
  let i = s.length;
  while (i) hash = ((hash << 5) - hash) ^ s.charCodeAt(--i);
  return hash >>> 0;
};

const isPrimitive = (v: Any): boolean =>
  (typeof v !== "object" && typeof v !== "function") || v === null;

/** Scalar types provided by the JS runtime. Includes primitives, RegExp, and Date */
const isScalar = (v: Any) => isPrimitive(v) || isDate(v) || isRegExp(v);

/** MongoDB sort comparison order. https://www.mongodb.com/docs/manual/reference/bson-type-comparison-order */
const SORT_ORDER: Record<string, number> = {
  undefined: 1,
  null: 2,
  number: 3,
  string: 4,
  symbol: 5,
  object: 6,
  array: 7,
  arraybuffer: 8,
  boolean: 9,
  date: 10,
  regexp: 11,
  function: 12
};

/**
 * Compare function which adheres to MongoDB comparison order.
 *
 * @param a The first value
 * @param b The second value
 * @returns {Number}
 */
export const compare = <T = Any>(a: T, b: T): number => {
  if (a === MISSING) a = undefined;
  if (b === MISSING) b = undefined;
  const [u, v] = [a, b].map(n => SORT_ORDER[typeOf(n)] || 0);
  if (u !== v) return u - v;
  // check for equivalence equality
  if (isEqual(a, b)) return 0;
  if ((a as string) < (b as string)) return -1;
  if ((a as string) > (b as string)) return 1;
  // if we get here we are comparing a type that does not make sense.
  return 0;
};

/**
 * A map implementation that uses value comparison for keys instead of referential identity.
 *
 * IMPORTANT! we assume objects are never modified once the hash is computed and put in the Map.
 * Modifying an object after adding to the Map will cause incorrect behaviour.
 */
export class ValueMap<K, V> extends Map<K, V> {
  // The hash function
  #hashFn = DEFAULT_HASH_FUNCTION;
  // maps the hashcode to key set
  #keyMap = new Map<number, K[]>();
  // returns a tuple of [<masterKey>, <hash>]. Expects an object key.
  #unpack = (key: K): [K, number] => {
    const hash = this.#hashFn(key);
    return [(this.#keyMap.get(hash) || []).find(k => isEqual(k, key)), hash];
  };

  private constructor() {
    super();
  }

  /**
   * Returns a new {@link ValueMap} object.
   * @param fn An optional custom hash function
   */
  static init<K, V>(fn?: HashFunction) {
    const m = new ValueMap<K, V>();
    if (fn) m.#hashFn = fn;
    return m;
  }

  clear(): void {
    super.clear();
    this.#keyMap.clear();
  }

  /**
   * @returns true if an element in the Map existed and has been removed, or false if the element does not exist.
   */
  delete(key: K): boolean {
    if (isPrimitive(key)) return super.delete(key);

    const [masterKey, hash] = this.#unpack(key);
    // nothing deleted
    if (!super.delete(masterKey)) return false;
    // filter out the deleted key
    this.#keyMap.set(
      hash,
      this.#keyMap.get(hash).filter(k => !isEqual(k, masterKey))
    );
    return true;
  }

  /**
   * Returns a specified element from the Map object. If the value that is associated to the provided key is an object, then you will get a reference to that object and any change made to that object will effectively modify it inside the Map.
   * @returns Returns the element associated with the specified key. If no element is associated with the specified key, undefined is returned.
   */
  get(key: K): V | undefined {
    if (isPrimitive(key)) return super.get(key);

    const [masterKey, _] = this.#unpack(key);
    return super.get(masterKey);
  }

  /**
   * @returns boolean indicating whether an element with the specified key exists or not.
   */
  has(key: K): boolean {
    if (isPrimitive(key)) return super.has(key);

    const [masterKey, _] = this.#unpack(key);
    return super.has(masterKey);
  }

  /**
   * Adds a new element with a specified key and value to the Map. If an element with the same key already exists, the element will be updated.
   */
  set(key: K, value: V): this {
    if (isPrimitive(key)) return super.set(key, value);

    const [masterKey, hash] = this.#unpack(key);
    if (super.has(masterKey)) {
      // replace masterKey value
      super.set(masterKey, value);
    } else {
      // add new master key.
      super.set(key, value);
      // cache against hash code.
      const keys = this.#keyMap.get(hash) || [];
      keys.push(key);
      // cache the key
      this.#keyMap.set(hash, keys);
    }
    return this;
  }

  /**
   * @returns the number of elements in the Map.
   */
  get size(): number {
    return super.size;
  }
}

export function assert(condition: boolean, message: string): void {
  if (!condition) throw new MingoError(message);
}

/** Object.prototype.toString.call() representaions of common types.*/
const STRING_REP: Record<string, string> = Object.keys(SORT_ORDER).reduce(
  (memo, k) => {
    memo["[object " + k[0].toUpperCase() + k.substring(1) + "]"] = k;
    return memo;
  },
  {}
);

/**
 * Returns the name of type in lowercase including custom types.
 * @param v Any value
 */
export function typeOf(v: Any): string {
  const s = Object.prototype.toString.call(v) as string;
  // when a custom object has a null constructor/prototype, it is considered a plain object.
  return s === "[object Object]"
    ? v?.constructor?.name?.toLowerCase() || "object"
    : STRING_REP[s] || s.substring(8, s.length - 1).toLowerCase();
}
export const isBoolean = (v: Any): v is boolean => typeof v === "boolean";
export const isString = (v: Any): v is string => typeof v === "string";
export const isSymbol = (v: Any): boolean => typeof v === "symbol";
export const isNumber = (v: Any): v is number =>
  !isNaN(v as number) && typeof v === "number";
export const isNotNaN = (v: Any) =>
  !(isNaN(v as number) && typeof v === "number");
export const isArray = Array.isArray;
export function isObject(v: Any): v is object {
  if (!v) return false;
  const p = Object.getPrototypeOf(v) as Any;
  return (p === Object.prototype || p === null) && typeOf(v) === "object";
}
//  objects, arrays, functions, date, custom object
export const isObjectLike = (v: Any): boolean => !isPrimitive(v);
export const isDate = (v: Any): v is Date => v instanceof Date;
export const isRegExp = (v: Any): v is RegExp => v instanceof RegExp;
export const isFunction = (v: Any): boolean => typeof v === "function";
export const isNil = (v: Any): boolean => v === null || v === undefined;
export const truthy = (arg: Any, strict = true): boolean =>
  !!arg || (strict && arg === "");
export const isEmpty = (x: Any): boolean =>
  isNil(x) ||
  (isString(x) && !x) ||
  (isArray(x) && x.length === 0) ||
  (isObject(x) && Object.keys(x).length === 0);
/** ensure a value is an array or wrapped within one. */
export const ensureArray = <T>(x: T | T[]): T[] => (isArray(x) ? x : [x]);

export const has = (obj: object, prop: string): boolean =>
  !!obj && (Object.prototype.hasOwnProperty.call(obj, prop) as boolean);

const isTypedArray = (v: Any): v is ArrayBuffer =>
  typeof ArrayBuffer !== "undefined" && ArrayBuffer.isView(v);

/**
 * Deep clone an object.
 */
export const cloneDeep = <T>(v: T, refs?: Set<Any>): T => {
  // if (structuredClone) return structuredClone(v);
  if (isNil(v) || isBoolean(v) || isNumber(v) || isString(v)) return v;
  if (isDate(v)) return new Date(v) as T;
  if (isRegExp(v)) return new RegExp(v) as T;
  if (isTypedArray(v)) {
    const ctor = v.constructor as Constructor;
    return new ctor(v) as T;
  }
  if (!(refs instanceof Set)) refs = new Set();
  if (refs.has(v)) throw CYCLE_FOUND_ERROR;
  refs.add(v);
  try {
    if (isArray(v)) {
      const arr = new Array<Any>(v.length);
      for (let i = 0; i < v.length; i++) arr[i] = cloneDeep(v[i], refs);
      return arr as T;
    }
    if (isObject(v)) {
      const obj: AnyObject = {};
      for (const k of Object.keys(v)) obj[k] = cloneDeep(v[k], refs);
      return obj as T;
    }
  } finally {
    refs.delete(v);
  }

  // custom-type. will be treated as immutable so return as is.
  return v;
};

const isMissing = (v: Any): boolean => v === MISSING;

/**
 * Deep merge objects or arrays. When the inputs have unmergeable types, the right hand value is returned.
 * If inputs are arrays, elements in the same position are merged together.
 * Remaining elements are appended to the target object.
 *
 * @param target Target object to merge into.
 * @param input  Source object to merge from.
 * @private
 */
export function merge(target: Any, input: Any): Any {
  // take care of missing inputs
  if (isMissing(target) || isNil(target)) return input;
  if (isMissing(input) || isNil(input)) return target;
  if (isPrimitive(target) || isPrimitive(input)) return input;
  if (isArray(target) && isArray(input)) {
    assert(
      target.length === input.length,
      "arrays must be of equal length to merge."
    );
  }
  for (const k of Object.keys(input as AnyObject)) {
    target[k] = merge(target[k], input[k]);
  }
  return target;
}

/**
 * Returns the intersection of multiple arrays.
 *
 * @param  {Array} input An array of arrays from which to find intersection.
 * @param  {Function} hashFunction Custom function to hash values, default the hashCode method
 * @return {Array} Array of intersecting values.
 */
export function intersection(
  input: Any[][],
  hashFunction: HashFunction = DEFAULT_HASH_FUNCTION
): Any[] {
  const vmaps = [ValueMap.init(hashFunction), ValueMap.init(hashFunction)];
  if (input.length === 0) return [];
  if (input.some(arr => arr.length === 0)) return [];
  if (input.length === 1) return [...input];
  // start with last array to ensure stableness.
  input[input.length - 1].forEach(v => vmaps[0].set(v, true));
  // process collection backwards.
  for (let i = input.length - 2; i > -1; i--) {
    input[i].forEach(v => {
      if (vmaps[0].has(v)) vmaps[1].set(v, true);
    });
    if (vmaps[1].size === 0) return [];
    vmaps.reverse();
    vmaps[1].clear();
  }

  return Array.from(vmaps[0].keys());
}

/**
 * Flatten the array
 *
 * @param xs The array to flatten
 * @param depth The number of nested lists to iterate. @default 1
 */
export function flatten(xs: Any[], depth = 1): Any[] {
  const arr = new Array<Any>();
  function flatten2(ys: Any[], n: number) {
    for (let i = 0, len = ys.length; i < len; i++) {
      if (isArray(ys[i]) && (n > 0 || n < 0)) {
        flatten2(ys[i] as Any[], Math.max(-1, n - 1));
      } else {
        arr.push(ys[i]);
      }
    }
  }
  flatten2(xs, depth);
  return arr;
}

/** Returns a map all members of the obect for generating a custom data representation. */
function getMembersOf(o: Any): AnyObject {
  const props = {} as AnyObject;
  while (o) {
    // Get all properties of the current object and add if not already included
    for (const k of Object.getOwnPropertyNames(o))
      if (!(k in props)) props[k] = o[k];
    // Move to the prototype of the current object
    o = Object.getPrototypeOf(o);
  }
  return props;
}

type Stringer = { toString(): string };
function hasCustomString(o: Any): o is Stringer {
  while (o) {
    if (Object.getOwnPropertyNames(o).includes("toString"))
      return o["toString"] !== Object.prototype.toString;
    o = Object.getPrototypeOf(o);
  }
  return false;
}

/**
 * Determine whether two values are the same or strictly equivalent.
 * Checking whether values are the same only applies to built in objects.
 * For user-defined objects this checks for only referential equality so
 * two different instances with the same values are not equal.
 *
 * @param a The first value
 * @param b The second value
 * @return True if values are equivalent, false otherwise.
 */
export function isEqual(a: Any, b: Any): boolean {
  // strictly equal must be equal. matches referentially equal values.
  if (a === b || Object.is(a, b)) return true;
  if (a === null || b === null) return false;
  if (typeof a !== typeof b) return false;
  if (typeof a !== "object") return false;
  if (a.constructor !== b.constructor) return false;
  // common objects
  if (a instanceof Date) return +a === +(b as Date);
  if (a instanceof RegExp) return a.toString() === (b as RegExp).toString();
  const ctor = a.constructor;
  if (ctor === Array || ctor === Object) {
    const aKeys = Object.keys(a).sort();
    const bKeys = Object.keys(b).sort();
    if (aKeys.length !== bKeys.length) return false;
    for (let i = 0, k = aKeys[i]; i < aKeys.length; k = aKeys[++i]) {
      if (k !== bKeys[i] || !isEqual(a[k], b[k])) return false;
    }
    return true;
  }
  // toString() compare all supported types including custom ones.
  return (
    hasCustomString(a) &&
    (a as Stringer).toString() === (b as Stringer).toString()
  );
}

/**
 * Return a new unique version of the collection
 * @param  {Array} input The input collection
 * @return {Array}
 */
export function unique(
  input: Any[],
  hashFunction: HashFunction = DEFAULT_HASH_FUNCTION
): Any[] {
  const m = ValueMap.init(hashFunction);
  input.forEach(v => m.set(v, true));
  return Array.from(m.keys());
}

/**
 * Encode value to string using a simple non-colliding stable scheme.
 * Handles user-defined types by processing keys on first non-empty prototype.
 * If a user-defined type provides a "toString" function, it is used.
 *
 * @param value The value to convert to a string representation.
 * @returns {string}
 */
export const stringify = (v: Any, refs?: Set<Any>): string => {
  if (v === null) return "null";
  if (v === undefined) return "undefined";
  if (isString(v) || isNumber(v) || isBoolean(v)) return JSON.stringify(v);
  if (isDate(v)) return v.toISOString();
  if (isRegExp(v) || isSymbol(v) || isFunction(v))
    return (v as Stringer).toString();
  if (!(refs instanceof Set)) refs = new Set();
  if (refs.has(v)) throw CYCLE_FOUND_ERROR;
  try {
    refs.add(v);
    if (isArray(v)) return "[" + v.map(s => stringify(s, refs)).join(",") + "]";
    if (isObject(v)) {
      const keys = Object.keys(v).sort();
      return "{" + keys.map(k => `${k}:${stringify(v[k], refs)}`).join() + "}";
    }
    // use toString representation of custom-type
    const s = hasCustomString(v)
      ? v.toString()
      : stringify(getMembersOf(v), refs);
    return typeOf(v) + "(" + s + ")";
  } finally {
    refs.delete(v);
  }
};

/**
 * Generate hash code.
 * This selected function is the result of benchmarking various hash functions.
 * This version performs well and can hash 10^6 documents in ~3s with on average 100 collisions.
 *
 * @param value
 * @returns {number|null}
 */
export function hashCode(value: Any, hashFunction?: HashFunction): number {
  if (isNil(value)) return null;
  hashFunction = hashFunction || DEFAULT_HASH_FUNCTION;
  return hashFunction(value);
}

/**
 * Groups the collection into sets by the returned key
 *
 * @param collection
 * @param keyFn {Function} to compute the group key of an item in the collection
 * @returns {Map<Any, Any[]>}
 */
export function groupBy(
  collection: Any[],
  keyFn: Callback<Any>,
  hashFunction: HashFunction = DEFAULT_HASH_FUNCTION
): Map<Any, Any[]> {
  if (collection.length < 1) return new Map();

  // map of hash to collided values
  const lookup = new Map<number, Any[]>();
  // map of raw key values to objects.
  const result = new Map<Any, Any[]>();

  for (let i = 0; i < collection.length; i++) {
    const obj = collection[i];
    const key = keyFn(obj, i);
    const hash = hashCode(key, hashFunction);

    if (hash === null) {
      if (result.has(null)) {
        result.get(null).push(obj);
      } else {
        result.set(null, [obj]);
      }
    } else {
      // find if we can match a hash for which the value is equivalent.
      // this is used to deal with collisions.
      const existingKey = lookup.has(hash)
        ? lookup.get(hash).find(k => isEqual(k, key))
        : null;

      // collision detected or first time seeing key
      if (isNil(existingKey)) {
        // collision detected or first entry so we create a new group.
        result.set(key, [obj]);
        // upload the lookup with the collided key
        if (lookup.has(hash)) {
          lookup.get(hash).push(key);
        } else {
          lookup.set(hash, [key]);
        }
      } else {
        // key exists
        result.get(existingKey).push(obj);
      }
    }
  }

  return result;
}

// max elements to push.
// See argument limit https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Function/apply
const MAX_ARRAY_PUSH = 50000;

/**
 * Merge elements into the dest
 *
 * @param {*} target The target object
 * @param {*} rest The array of elements to merge into dest
 * @private
 */
export function into(
  target: ArrayOrObject,
  ...rest: ArrayOrObject[]
): ArrayOrObject {
  if (isArray(target)) {
    for (const arr of rest as Any[][]) {
      // push arrary in batches to handle large inputs
      let i = Math.ceil(arr.length / MAX_ARRAY_PUSH);
      let begin = 0;
      while (i-- > 0) {
        Array.prototype.push.apply(
          target,
          arr.slice(begin, begin + MAX_ARRAY_PUSH)
        );
        begin += MAX_ARRAY_PUSH;
      }
    }
    return target;
  } else {
    // merge objects. same behaviour as Object.assign
    return rest.filter(isObjectLike).reduce((acc, item) => {
      Object.assign(acc, item);
      return acc;
    }, target);
  }
}

/**
 * Retrieve the value of a given key on an object
 * @param obj
 * @param key
 * @returns {*}
 * @private
 */
function getValue(obj: ArrayOrObject, key: string | number): Any {
  return isObjectLike(obj) ? obj[key] : undefined;
}

/**
 * Unwrap a single element array to specified depth
 * @param {Array} arr
 * @param {Number} depth
 * @private
 */
function unwrap(arr: Any[], depth: number): Any[] {
  if (depth < 1) return arr;
  while (depth-- && arr.length === 1) arr = arr[0] as Any[];
  return arr;
}

/** Options to resolve() and resolveGraph() functions */
interface ResolveOptions {
  /** Unwrap the final array value.  */
  unwrapArray?: boolean;
  /** Replace "undefined" values with special MISSING symbol value. */
  preserveMissing?: boolean;
  /** Preserve values for untouched keys of objects. */
  preserveKeys?: boolean;
  /** Preserve untouched indexes in arrays. */
  preserveIndex?: boolean;
}

/**
 * Resolve the value of the field (dot separated) on the given object
 * @param obj {AnyObject} the object context
 * @param selector {String} dot separated path to field
 * @returns {*}
 */
export function resolve(
  obj: ArrayOrObject,
  selector: string,
  options?: Pick<ResolveOptions, "unwrapArray">
): Any {
  let depth = 0;
  function resolve2(o: ArrayOrObject, path: string[]): Any {
    let value: Any = o;
    for (let i = 0; i < path.length; i++) {
      const field = path[i];
      const isText = /^\d+$/.exec(field) === null;
      // using instanceof to aid typescript compiler
      if (isText && isArray(value)) {
        // On the first iteration, we check if we received a stop flag.
        // If so, we stop to prevent iterating over a nested array value
        // on consecutive object keys in the selector.
        if (i === 0 && depth > 0) break;
        depth += 1;
        // only look at the rest of the path
        const subpath = path.slice(i);
        value = value.reduce<Any[]>((acc: Any[], item: ArrayOrObject) => {
          const v = resolve2(item, subpath);
          if (v !== undefined) acc.push(v);
          return acc;
        }, []);
        break;
      } else {
        value = getValue(value as ArrayOrObject, field);
      }
      if (value === undefined) break;
    }
    return value;
  }

  const res = isScalar(obj) ? obj : resolve2(obj, selector.split("."));
  return isArray(res) && options?.unwrapArray ? unwrap(res, depth) : res;
}

/**
 * Returns the full object to the resolved value given by the selector.
 *
 * @param obj {AnyObject} the object context
 * @param selector {String} dot separated path to field
 */
export function resolveGraph(
  obj: ArrayOrObject,
  selector: string,
  options?: ResolveOptions
): ArrayOrObject | undefined {
  const sep = selector.indexOf(".");
  const key = sep == -1 ? selector : selector.substring(0, sep);
  const next = selector.substring(sep + 1);
  const hasNext = sep != -1;

  if (isArray(obj)) {
    // obj is an array
    const isIndex = /^\d+$/.test(key);
    const arr = isIndex && options?.preserveIndex ? [...obj] : [];
    if (isIndex) {
      const index = parseInt(key);
      let value = getValue(obj, index) as ArrayOrObject;
      if (hasNext) {
        value = resolveGraph(value, next, options);
      }
      if (options?.preserveIndex) {
        arr[index] = value;
      } else {
        arr.push(value);
      }
    } else {
      for (const item of obj) {
        const value = resolveGraph(item as ArrayOrObject, selector, options);
        if (options?.preserveMissing) {
          arr.push(value == undefined ? MISSING : value);
        } else if (value != undefined || options?.preserveIndex) {
          arr.push(value);
        }
      }
    }
    return arr;
  }

  const res = options?.preserveKeys ? { ...obj } : {};
  let value = getValue(obj, key);
  if (hasNext) {
    value = resolveGraph(value as ArrayOrObject, next, options);
  }
  if (value === undefined) return undefined;
  res[key] = value;
  return res;
}

/**
 * Filter out all MISSING values from the object in-place
 *
 * @param obj The object to filter
 * @private
 */
export function filterMissing(obj: ArrayOrObject): void {
  if (isArray(obj)) {
    for (let i = obj.length - 1; i >= 0; i--) {
      if (obj[i] === MISSING) {
        obj.splice(i, 1);
      } else {
        filterMissing(obj[i] as ArrayOrObject);
      }
    }
  } else if (isObject(obj)) {
    for (const k in obj) {
      if (has(obj, k)) {
        filterMissing(obj[k] as ArrayOrObject);
      }
    }
  }
}

/** Options passed to the walk function. */
export interface WalkOptions {
  buildGraph?: boolean;
  descendArray?: boolean;
}

const NUMBER_RE = /^\d+$/;

/**
 * Walk the object graph and execute the given transform function
 *
 * @param  {AnyObject|Array} obj   The object to traverse.
 * @param  {String} selector    The selector to navigate.
 * @param  {Callback} fn Callback to execute for value at the end the traversal.
 * @param  {WalkOptions} options The opetions to use for the function.
 * @return {*}
 */
export function walk(
  obj: ArrayOrObject,
  selector: string,
  fn: Callback<void>,
  options?: WalkOptions
): void {
  const names = selector.split(".");
  const key = names[0];
  const next = names.slice(1).join(".");

  if (names.length === 1) {
    if (isObject(obj) || (isArray(obj) && NUMBER_RE.test(key))) {
      fn(obj, key);
    }
  } else {
    // force the rest of the graph while traversing
    if (options?.buildGraph && isNil(obj[key])) {
      obj[key] = {};
    }

    // get the next item
    const item = obj[key] as ArrayOrObject;
    // nothing more to do
    if (!item) return;
    // we peek to see if next key is an array index.
    const isNextArrayIndex = !!(names.length > 1 && NUMBER_RE.test(names[1]));
    // if we have an array value but the next key is not an index and the 'descendArray' option is set,
    // we walk each item in the array separately. This allows for handling traversing keys for objects
    // nested within an array.
    //
    // Eg: Given { array: [ {k:1}, {k:2}, {k:3} ] }
    //  - individual objecs can be traversed with "array.k"
    //  - a specific object can be traversed with "array.1"
    if (isArray(item) && options?.descendArray && !isNextArrayIndex) {
      item.forEach(((e: ArrayOrObject) =>
        walk(e, next, fn, options)) as Callback<void>);
    } else {
      walk(item, next, fn, options);
    }
  }
}

/**
 * Set the value of the given object field
 *
 * @param obj {AnyObject|Array} the object context
 * @param selector {String} path to field
 * @param value {*} the value to set. if it is function, it is invoked with the old value and must return the new value.
 */
export function setValue(
  obj: ArrayOrObject,
  selector: string,
  value: Any
): void {
  walk(
    obj,
    selector,
    ((item: AnyObject, key: string) => {
      item[key] = isFunction(value) ? (value as Callback)(item[key]) : value;
    }) as Callback<void>,
    { buildGraph: true }
  );
}

/**
 * Removes an element from the container.
 * If the selector resolves to an array and the leaf is a non-numeric key,
 * the remove operation will be performed on objects of the array.
 *
 * @param obj {ArrayOrObject} object or array
 * @param selector {String} dot separated path to element to remove
 */
export function removeValue(
  obj: ArrayOrObject,
  selector: string,
  options?: Pick<WalkOptions, "descendArray">
): void {
  walk(
    obj,
    selector,
    ((item: Any, key: string) => {
      if (isArray(item)) {
        if (/^\d+$/.test(key)) {
          item.splice(parseInt(key), 1);
        } else if (options && options.descendArray) {
          for (const elem of item) {
            if (isObject(elem)) {
              delete (elem as AnyObject)[key];
            }
          }
        }
      } else if (isObject(item)) {
        delete item[key];
      }
    }) as Callback<void>,
    options
  );
}

const OPERATOR_NAME_PATTERN = /^\$[a-zA-Z0-9_]+$/;
/**
 * Check whether the given name passes for an operator. We assume AnyVal field name starting with '$' is an operator.
 * This is cheap and safe to do since keys beginning with '$' should be reserved for internal use.
 * @param {String} name
 */
export function isOperator(name: string): boolean {
  return OPERATOR_NAME_PATTERN.test(name);
}

/**
 * Simplify expression for easy evaluation with query operators map
 * @param expr
 * @returns {*}
 */
export function normalize(expr: Any): Any {
  if (isScalar(expr)) {
    return isRegExp(expr) ? { $regex: expr } : { $eq: expr };
  }

  // normalize object expression. using ObjectLike handles custom types
  if (isObjectLike(expr)) {
    // no valid query operator found, so we do simple comparison
    if (!Object.keys(expr as AnyObject).some(isOperator)) return { $eq: expr };
    // ensure valid regex
    if (has(expr as AnyObject, "$regex")) {
      const newExpr = { ...(expr as AnyObject) };
      newExpr["$regex"] = new RegExp(
        expr["$regex"] as string,
        expr["$options"] as string
      );
      delete newExpr["$options"];
      return newExpr;
    }
  }

  return expr;
}

/**
 * Find the insert index for the given key in a sorted array.
 *
 * @param sorted The sorted array to search
 * @param item The search key
 * @param comparator Optional custom compare function
 */
export function findInsertIndex(
  sorted: Any[],
  item: Any,
  comparator: Comparator = compare
): number {
  // uses binary search
  let lo = 0;
  let hi = sorted.length - 1;
  while (lo <= hi) {
    const mid = Math.round(lo + (hi - lo) / 2);
    if (comparator(item, sorted[mid]) < 0) {
      hi = mid - 1;
    } else if (comparator(item, sorted[mid]) > 0) {
      lo = mid + 1;
    } else {
      return mid;
    }
  }
  return lo;
}
