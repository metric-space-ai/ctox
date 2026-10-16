// Shared helpers for the outbound-lead-generation v4 command handlers.
//
// Each helper is a faithful port of a Rust helper from the native snapshot
// (origin/main 4794af03d, src/core/business_os/). `ref:` anchors name the
// original so behavior can be re-checked line by line. No Node builtins: the
// app runtime gives handlers only `ctx` (§16.16).

/** Throw a contract error `{code, message}` (codes §16.10). */
export function fail(code, message) {
  const error = new Error(message);
  error.code = code;
  throw error;
}

export function isObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

/** serde `Value::get(key).is_some()` — key present on an object, even with null. */
export function has(value, key) {
  return isObject(value) && Object.prototype.hasOwnProperty.call(value, key);
}

/** ref: store.rs:18292 outbound_string — trimmed, non-empty string at path, else null. */
export function str(value, path) {
  let cursor = value;
  for (const key of path) {
    if (!has(cursor, key)) return null;
    cursor = cursor[key];
  }
  if (typeof cursor !== 'string') return null;
  const trimmed = cursor.trim();
  return trimmed ? trimmed : null;
}

/** ref: store.rs:18304 outbound_first_string */
export function firstString(values) {
  for (const value of values) {
    if (typeof value === 'string' && value !== '') return value;
  }
  return null;
}

/** ref: store.rs:18312 outbound_required_string */
export function requiredString(value, path) {
  const found = str(value, path);
  if (found == null) fail('invalid', `${path.join('.')} is required`);
  return found;
}

/** ref: store.rs:18285 outbound_object_payload — shallow object copy or {}. */
export function objectPayload(payload) {
  return isObject(payload) ? JSON.parse(JSON.stringify(payload)) : {};
}

/** serde `Value::as_u64` — non-negative integral JSON number, else null. */
export function asU64(value) {
  return typeof value === 'number' && Number.isInteger(value) && value >= 0 ? value : null;
}

/** serde `Value::as_i64` */
export function asI64(value) {
  return typeof value === 'number' && Number.isInteger(value) ? value : null;
}

/** Rust `str::len()` counts UTF-8 bytes, not characters. */
export function utf8Len(text) {
  let bytes = 0;
  for (const ch of String(text)) {
    const cp = ch.codePointAt(0);
    bytes += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
  }
  return bytes;
}

/** Rust `.chars().take(n)` — code points, not UTF-16 units. */
export function takeChars(text, n) {
  return Array.from(String(text)).slice(0, n).join('');
}

/** Wrap a capability failure like Rust `.context(msg)` (anyhow chain "msg: cause"). */
export async function withContext(message, fn) {
  try {
    return await fn();
  } catch (error) {
    const wrapped = new Error(`${message}: ${error?.message || error}`);
    wrapped.code = error?.code || 'unavailable';
    throw wrapped;
  }
}
