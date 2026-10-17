// Statischer Import-Abschluss der Shell v4 (M7-Slot): welche Dateien lädt der Browser beim Boot, ausgehend von den
// Einstiegen? Pfade sind relativ zur Shell-Wurzel (src/apps/business-os), wie im Slot-Archiv.
//
// Abbildung der URLs auf Shell-Pfade (Daemon und v4/sw.js):
//   /business-os/<p>  → <p>          (v4/index.html lädt /business-os/v4/boot.js)
//   /kernel/<p>       → kernel/<p>   (v4/boot.js importiert /kernel/boot.mjs)
//   /sw.js            → v4/sw.js
//   ctox:shared/<p>   → shared/<p>   (Umschreibung im Service Worker, eine Wurzel je Shell-Version)
//   ./ ../            → relativ zur importierenden Datei
// Nicht verfolgt: ctox:lib/… (Bibliotheksbindung), http(s):, data:, blob:, nackte Paketnamen, sonstige absolute Pfade
// (/api/…). Gelesen werden statische import/export-from, import('literal'), import 'x' und
// new URL('literal', import.meta.url) in .js/.mjs, <script src>
// und <link href> in .html, @import und url(...) in .css.
//
// Der Slot-Bauer nutzt den Abschluss zweimal: über den Quellbaum (diese Dateien sind Laufzeitinhalt, gleich wie sie
// heißen – kernel/cache.mjs fiel sonst unter die Namensregel für Build-/Cache-Artefakte) und über die fertige Nutzlast
// (fehlt eine erreichbare Datei im Archiv, bricht der Bau ab).
import { posix as pathPosix } from 'node:path';

export const SHELL_BOOT_ENTRIES = Object.freeze(['v4/index.html', 'kernel/boot.mjs', 'v4/boot.js', 'v4/frame/index.js']);

const JS_FROM = /(?:^|[\s;}])(?:import|export)\s[^;'"`]*?\bfrom\s*(['"])([^'"\r\n]+)\1/g;
const JS_BARE = /(?:^|[\s;}])import\s*(['"])([^'"\r\n]+)\1/g;
const JS_DYNAMIC = /\bimport\(\s*(['"])([^'"\r\n]+)\1\s*\)/g;
const JS_META_URL = /\bnew\s+URL\(\s*(['"])([^'"\r\n]+)\1\s*,\s*import\.meta\.url\s*\)/g;
const HTML_SRC = /<(?:script|link)\b[^>]*?\b(?:src|href)\s*=\s*(['"])([^'"]+)\1/gi;
const CSS_IMPORT = /@import\s+(['"])([^'"]+)\1/g;
const CSS_URL = /url\(\s*(['"]?)([^'")\s]+)\1\s*\)/g;

function stripJsComments(text) {
  // Block- und Zeilenkommentare entfernen, Zeichenketten unangetastet lassen (URLs mit // in Strings bleiben).
  let out = '';
  for (let i = 0; i < text.length; i += 1) {
    const c = text[i];
    if (c === '"' || c === "'" || c === '`') {
      const q = c; out += c; i += 1;
      while (i < text.length && text[i] !== q) { if (text[i] === '\\') { out += text[i]; i += 1; } out += text[i] ?? ''; i += 1; }
      out += text[i] ?? '';
    } else if (c === '/' && text[i + 1] === '*') {
      const end = text.indexOf('*/', i + 2); i = end < 0 ? text.length : end + 1; out += ' ';
    } else if (c === '/' && text[i + 1] === '/' && (i === 0 || /[\s;{}(),]/.test(text[i - 1]))) {
      const end = text.indexOf('\n', i); i = end < 0 ? text.length : end - 1; out += ' ';
    } else out += c;
  }
  return out;
}

export function specifiersOf(path, text) {
  const ext = pathPosix.extname(path).toLowerCase();
  const found = [];
  const collect = (re, src) => { for (const m of src.matchAll(re)) found.push(m[2]); };
  if (ext === '.js' || ext === '.mjs') {
    const src = stripJsComments(text);
    collect(JS_FROM, src); collect(JS_BARE, src); collect(JS_DYNAMIC, src); collect(JS_META_URL, src);
  } else if (ext === '.html') {
    collect(HTML_SRC, text.replace(/<!--[\s\S]*?-->/g, ' '));
  } else if (ext === '.css') {
    const src = text.replace(/\/\*[\s\S]*?\*\//g, ' ');
    collect(CSS_IMPORT, src); collect(CSS_URL, src);
  }
  return found;
}

// Liefert den Shell-Pfad eines Bezeichners oder null (nicht verfolgt).
export function resolveSpecifier(fromPath, specifier) {
  let s = String(specifier).trim().replace(/[?#].*$/, '');
  if (!s || /^(?:[a-z][a-z0-9+.-]*:)?\/\//i.test(s) || /^(?:data|blob|https?|mailto|tel):/i.test(s)) return null;
  if (s.startsWith('ctox:shared/')) return pathPosix.normalize(`shared/${s.slice('ctox:shared/'.length)}`);
  if (/^[a-z][a-z0-9+.-]*:/i.test(s)) return null;
  if (s.startsWith('/business-os/')) return pathPosix.normalize(s.slice('/business-os/'.length));
  if (s.startsWith('/kernel/')) return pathPosix.normalize(s.slice(1));
  if (s === '/sw.js') return 'v4/sw.js';
  if (s.startsWith('/')) return null;
  if (!s.startsWith('./') && !s.startsWith('../')) return null;
  const resolved = pathPosix.normalize(pathPosix.join(pathPosix.dirname(fromPath), s));
  return resolved.startsWith('../') ? null : resolved;
}

// readText(path) → Text oder null (Datei fehlt). Ergebnis: erreichbare Pfade und fehlende Kanten {from, specifier, path}.
export async function importClosure(readText, entries = SHELL_BOOT_ENTRIES) {
  const reachable = new Set();
  const missing = [];
  const queue = [];
  for (const entry of entries) {
    const text = await readText(entry);
    if (text == null) { missing.push({ from: '(Einstieg)', specifier: entry, path: entry }); continue; }
    reachable.add(entry); queue.push([entry, text]);
  }
  while (queue.length) {
    const [from, text] = queue.shift();
    for (const specifier of specifiersOf(from, text)) {
      const path = resolveSpecifier(from, specifier);
      if (!path || reachable.has(path)) continue;
      const next = await readText(path);
      if (next == null) { missing.push({ from, specifier, path }); continue; }
      reachable.add(path);
      if (/\.(?:m?js|html|css)$/i.test(path)) queue.push([path, next]);
    }
  }
  return { reachable, missing };
}

export function describeMissing(missing) {
  return missing.slice(0, 12).map((m) => `${m.from} → ${m.specifier} (${m.path})`).join('; ');
}
