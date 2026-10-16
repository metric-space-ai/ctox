// Origin: CTOX
// License: Apache-2.0
// Gemeinsame Helfer der v4-Handler (Strang H8, Shell v4). Datei ist in jeder App identisch kopiert, weil
// App-Artefakte getrennt ausgeliefert werden (/~a/<app_id>@<ver>/…) und nicht über App-Grenzen importieren.

/** Fehler nach §16.10/§16.16: throw {code, message}. */
export function fail(code, message, extra = {}) {
  const error = new Error(message);
  error.code = code;
  Object.assign(error, extra);
  throw error;
}

export const isObject = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
export const payloadOf = (command) => (isObject(command?.payload) ? command.payload : {});

/** ref: store.rs:26959 first_string_field – erster nicht-leerer, getrimmter String. */
export function firstStringField(value, keys) {
  if (!isObject(value)) return undefined;
  for (const key of keys) {
    const item = value[key];
    if (typeof item === 'string' && item.trim() !== '') return item.trim();
  }
  return undefined;
}

/** ref: mission/tickets/cases.rs payload_string / required_payload_string */
export const payloadString = (payload, key) => firstStringField(payload, [key]);
export function requiredString(payload, key, message = `${key} is required`) {
  const value = payloadString(payload, key);
  if (value === undefined) fail('invalid', message);
  return value;
}

export const asBool = (value) => (typeof value === 'boolean' ? value : undefined);
export const asI64 = (value) => (typeof value === 'number' && Number.isInteger(value) ? value : undefined);
export const asF64 = (value) => (typeof value === 'number' && Number.isFinite(value) ? value : undefined);
export const asU64 = (value) => (typeof value === 'number' && Number.isInteger(value) && value >= 0 ? value : undefined);

/** Generische Capability holen; fehlt sie (nicht deklariert/nicht gebaut) → unavailable. */
export function capability(ctx, path) {
  let cursor = ctx?.cap;
  for (const part of path.split('.')) cursor = cursor?.[part];
  if (typeof cursor !== 'function') fail('unavailable', `capability ctx.cap.${path} is not available`);
  return cursor;
}

export const callCap = (ctx, path, ...args) => capability(ctx, path)(...args);
