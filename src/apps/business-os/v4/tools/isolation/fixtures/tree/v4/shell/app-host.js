// Kommentar mit 'beta' zaehlt nicht.
export function host(appId) {
  if (appId === 'beta') return 1;
  const url = '/~a/alpha@1.0.0/index.js';
  const col = 'beta_items';
  const generic = 'ctox:shared/util.js';
  const mime = 'text/beta';
  const ok = 'alpha'; // isolation-allow: Fixture-Ausnahme
  return [url, col, generic, mime, ok];
}
