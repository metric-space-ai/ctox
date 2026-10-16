#!/usr/bin/env node
// Scopt App-CSS unter [data-app="<id>"] (Entwurf §6, PORTING.md Regel R-CSS).
// Aufruf: node scope-css.mjs <app-id> <datei.css> [--write] [--check]
//   ohne --write: Ergebnis nach stdout; --check: Exit 1, wenn eine Regel nicht gescoped ist.
// Regeln: jede Selektorliste auf oberster Ebene und in @media/@container/@supports/@layer bekommt das
// Präfix; :root/html/body werden zum Scope selbst; @keyframes/@font-face/@property/@page bleiben unberührt.
// Idempotent: bereits gescopte Selektoren bleiben, wie sie sind.
import { readFileSync, writeFileSync } from 'node:fs';

const PASS_THROUGH_AT = /^@(?:-webkit-)?(keyframes|font-face|property|page|counter-style|font-feature-values)\b/i;
const NESTING_AT = /^@(media|container|supports|layer|document)\b/i;

export function scopeSelector(selector, scope) {
  const sel = selector.trim();
  if (!sel) return sel;
  if (sel.startsWith(scope)) return sel;
  // Shell-Vorfahren (.shell-window…, Fenster-Chrome) liegen in v4 außerhalb von root: der Scope ersetzt sie.
  const replaced = sel
    .replace(/^(?:(?::root|html|body)(?=$|[\s.:[>+~#])|\.shell-window(?:\[[^\]]*\]|[.:][\w-]+)*(?=\s))/, scope);
  if (replaced !== sel) return replaced;
  return `${scope} ${sel}`;
}

function splitSelectorList(prelude) {
  const parts = [];
  let depth = 0, cur = '', quote = '';
  for (const ch of prelude) {
    if (quote) { cur += ch; if (ch === quote) quote = ''; continue; }
    if (ch === '"' || ch === "'") { quote = ch; cur += ch; continue; }
    if (ch === '(' || ch === '[') depth++;
    if (ch === ')' || ch === ']') depth--;
    if (ch === ',' && depth === 0) { parts.push(cur); cur = ''; continue; }
    cur += ch;
  }
  parts.push(cur);
  return parts;
}

export function scopeCss(css, appId) {
  const scope = `[data-app="${appId}"]`;
  let out = '';
  let i = 0;
  const stack = []; // 'nest' | 'rule' | 'pass'
  let prelude = '';
  const unscoped = [];
  while (i < css.length) {
    const ch = css[i];
    if (ch === '/' && css[i + 1] === '*') {
      const end = css.indexOf('*/', i + 2);
      const stop = end < 0 ? css.length : end + 2;
      if (prelude.trim()) prelude += css.slice(i, stop); else out += prelude + css.slice(i, stop), prelude = '';
      i = stop;
      continue;
    }
    if (ch === '"' || ch === "'") {
      const q = ch; let j = i + 1;
      while (j < css.length && css[j] !== q) { if (css[j] === '\\') j++; j++; }
      prelude += css.slice(i, j + 1); i = j + 1; continue;
    }
    const top = stack[stack.length - 1];
    if (ch === '{') {
      const lead = prelude.match(/^\s*/)[0];
      const head = prelude.trim();
      if (top === 'pass' || top === 'rule') {
        out += prelude + '{'; stack.push(top === 'pass' ? 'pass' : 'rule');
      } else if (PASS_THROUGH_AT.test(head)) {
        out += prelude + '{'; stack.push('pass');
      } else if (NESTING_AT.test(head)) {
        out += prelude + '{'; stack.push('nest');
      } else if (head.startsWith('@')) {
        out += prelude + '{'; stack.push('pass');
      } else {
        const scoped = splitSelectorList(head).map((s, idx) => {
          const leading = idx === 0 ? '' : s.match(/^\s*/)[0];
          const r = scopeSelector(s, scope);
          if (!s.trim().startsWith(scope)) unscoped.push(s.trim());
          return idx === 0 ? r : (leading.includes('\n') ? leading + r : ` ${r}`);
        }).join(',');
        out += lead + scoped + ' {'; stack.push('rule');
      }
      prelude = ''; i++; continue;
    }
    if (ch === '}') { out += prelude + '}'; prelude = ''; stack.pop(); i++; continue; }
    if (ch === ';' && (top === undefined || top === 'nest') && prelude.trim().startsWith('@')) {
      out += prelude + ';'; prelude = ''; i++; continue; // @import/@charset/@layer a;
    }
    prelude += ch; i++;
  }
  out += prelude;
  return { css: out, unscoped };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const [appId, file, ...flags] = process.argv.slice(2);
  if (!appId || !file) { console.error('usage: scope-css.mjs <app-id> <file.css> [--write|--check]'); process.exit(64); }
  const src = readFileSync(file, 'utf8');
  const { css } = scopeCss(src, appId);
  if (flags.includes('--check')) {
    const changed = css !== src;
    console.log(changed ? `${file}: nicht vollständig unter [data-app="${appId}"] gescoped` : `${file}: gescoped`);
    process.exit(changed ? 1 : 0);
  }
  if (flags.includes('--write')) writeFileSync(file, css);
  else process.stdout.write(css);
}
