// Fehlerform des Handler-Vertrags: throw {code, message}.
// Codes: invalid|forbidden|not_found|conflict|revoked|expired|quota|unavailable
export function fail(code, message) {
  const error = { code, message };
  return error;
}

export function invalid(message) {
  return fail('invalid', message);
}

// Rust `.chars().count()` zählt Unicode-Skalarwerte, nicht UTF-16-Einheiten.
export function charCount(value) {
  return [...value].length;
}

export function isPlainObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
