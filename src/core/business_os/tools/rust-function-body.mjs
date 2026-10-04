// Classifier brace scanner: Rust lifetimes must not open a character literal.
export function functionBody(text, name, required = true) {
  const pattern = new RegExp(`(?:pub(?:\\([^)]*\\))?\\s+)?(?:super\\s+)?fn\\s+${name}\\s*\\(`);
  const match = pattern.exec(text);
  if (!match) {
    if (required) throw new Error(`cannot locate Rust function ${name}`);
    return '';
  }
  const open = text.indexOf('{', match.index);
  if (open < 0) {
    if (required) throw new Error(`cannot locate body for Rust function ${name}`);
    return '';
  }
  let depth = 0;
  let quote = '';
  let escaped = false;
  for (let index = open; index < text.length; index += 1) {
    const char = text[index];
    if (quote) {
      if (escaped) escaped = false;
      else if (char === '\\') escaped = true;
      else if (char === quote) quote = '';
      continue;
    }
    if (char === '"' || (char === "'" && /^'(?:[^'\\\r\n]|\\(?:[nrt0\\'"]|x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}))'/u.test(text.slice(index)))) {
      quote = char;
      continue;
    }
    if (char === '{') depth += 1;
    else if (char === '}') {
      depth -= 1;
      if (depth === 0) return text.slice(open + 1, index);
    }
  }
  if (required) throw new Error(`unterminated Rust function ${name}`);
  return '';
}
