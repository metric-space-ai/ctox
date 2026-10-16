// Origin: CTOX
// License: Apache-2.0
// ref: store.rs:6213-6226 source_sanitize_slug @ 4794af03d – klein, [a-z0-9] behalten, Rest zu einem '-', Ränder ab.
export function sourceSanitizeSlug(value) {
  let out = '';
  let lastDash = false;
  for (const ch of String(value ?? '').trim().toLowerCase()) {
    if (/^[a-z0-9]$/.test(ch)) {
      out += ch;
      lastDash = false;
    } else if (!lastDash) {
      out += '-';
      lastDash = true;
    }
  }
  return out.replace(/^-+|-+$/g, '');
}
