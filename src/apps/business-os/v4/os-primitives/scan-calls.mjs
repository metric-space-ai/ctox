// Origin: CTOX
// License: Apache-2.0
//
// Aufrufprüfung: findet jeden Capability-Aufruf in den Handlern und vergleicht Pfad und
// Argumentform mit dem Kernvertrag (core-contract.mjs). Erkannt werden
//   ctx.cap.<pfad>(arg, …)            direkter Aufruf
//   ctx.cap.<a>['<b>'](arg)           Indexzugriff (z. B. legacy)
//   callCap(ctx, '<pfad>', arg, …)    Helfer aus handlers/_base.js
// Kommentare werden vor dem Scan ausgeblendet (Zeilennummern bleiben erhalten).
// Nur das erste Argument wird geprüft; ist es kein Objektliteral, gilt der Aufruf als
// UNGEPRUEFT (nicht statisch prüfbar).
//
// Klassen:
//   OK            Pfad im Vertrag, Argumentform passt
//   ABWEICHUNG    Pfad in einer CAP-1/2-Familie, aber nicht im Vertrag; oder unbekannte/fehlende
//                 Felder; oder Aktions-Alias ohne/mit unbekannter action
//   HUELLE        CAP-1/Alias-Route mit flachen Argumenten statt {payload, record_id?, action?}
//                 (Doku sagt flach, Decoder verlangt Hülle – offene Vertragsentscheidung)
//   OHNE_VERTRAG  Familie hat noch keinen Kernvertrag (kein CAP-1/2)
//   UNGEPRUEFT    Pfad im Vertrag, Argument nicht statisch auswertbar

import fs from 'node:fs';
import path from 'node:path';
import {
  ROUTE_BY_PATH, COVERED_FAMILIES, RENAMES, REMOVED, FLAT_ARGS_SOURCE, BLOB_REF_FIELDS, BLOB_REF_KEYS, BLOB_REF_SOURCE,
  OPEN_IN_CORE,
} from './core-contract.mjs';

const CAP1_KEYS = new Set(['payload', 'record_id', 'action']);

/** Kommentare durch Leerzeichen ersetzen, Zeilenumbrüche und Strings erhalten. */
function stripComments(src) {
  let out = '';
  let i = 0;
  const n = src.length;
  while (i < n) {
    const c = src[i];
    const d = src[i + 1];
    if (c === '/' && d === '/') {
      while (i < n && src[i] !== '\n') { out += ' '; i++; }
    } else if (c === '/' && d === '*') {
      out += '  '; i += 2;
      while (i < n && !(src[i] === '*' && src[i + 1] === '/')) { out += src[i] === '\n' ? '\n' : ' '; i++; }
      out += '  '; i += 2;
    } else if (c === '"' || c === "'" || c === '`') {
      const q = c; out += c; i++;
      while (i < n && src[i] !== q) {
        if (src[i] === '\\') { out += src[i] + (src[i + 1] ?? ''); i += 2; continue; }
        out += src[i]; i++;
      }
      out += src[i] ?? ''; i++;
    } else { out += c; i++; }
  }
  return out;
}

/** Ab `start` (Position nach '(') das erste Argument bis Komma/Klammer auf Ebene 0. */
function firstArg(src, start) {
  let depth = 0;
  let i = start;
  let quote = null;
  for (; i < src.length; i++) {
    const c = src[i];
    if (quote) {
      if (c === '\\') { i++; continue; }
      if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === "'" || c === '`') { quote = c; continue; }
    if (c === '(' || c === '{' || c === '[') depth++;
    else if (c === ')' || c === '}' || c === ']') {
      if (depth === 0) break;
      depth--;
    } else if (c === ',' && depth === 0) break;
  }
  return src.slice(start, i).trim();
}

/** Schlüssel der obersten Ebene eines Objektliterals; null wenn kein Literal. */
function topKeys(text) {
  if (!text.startsWith('{')) return null;
  const body = text.slice(1, text.lastIndexOf('}'));
  const keys = [];
  let spread = false;
  let depth = 0;
  let quote = null;
  let token = '';
  const values = {};
  let currentKey = null;
  let valueBuf = '';
  const flush = () => {
    const t = token.trim();
    if (currentKey !== null) {
      values[currentKey] = valueBuf.trim();
    } else if (t.startsWith('...')) {
      spread = true;
    } else if (t) {
      const m = t.match(/^([A-Za-z_$][\w$]*)/); // Kurzschreibweise { a, b }
      if (m) { keys.push(m[1]); values[m[1]] = m[1]; }
    }
    token = ''; currentKey = null; valueBuf = '';
  };
  for (let i = 0; i < body.length; i++) {
    const c = body[i];
    if (quote) {
      if (c === '\\') { (currentKey !== null ? (valueBuf += c + body[i + 1]) : (token += c + body[i + 1])); i++; continue; }
      if (c === quote) quote = null;
      if (currentKey !== null) valueBuf += c; else token += c;
      continue;
    }
    if (c === '"' || c === "'" || c === '`') { quote = c; if (currentKey !== null) valueBuf += c; else token += c; continue; }
    if (c === '(' || c === '{' || c === '[') depth++;
    if (c === ')' || c === '}' || c === ']') depth--;
    if (depth === 0 && c === ',') { flush(); continue; }
    if (depth === 0 && c === ':' && currentKey === null) {
      const k = token.trim().replace(/^['"]|['"]$/g, '');
      if (/^[\w$.-]+$/.test(k)) { currentKey = k; keys.push(k); token = ''; continue; }
    }
    if (currentKey !== null) valueBuf += c; else token += c;
  }
  flush();
  return { keys, spread, values };
}

const lineOf = (src, idx) => src.slice(0, idx).split('\n').length;

/** Literalwert eines Feldes ('…'/"…"/`…` ohne ${} bzw. null); undefined = nicht statisch auswertbar. */
function literalOf(text) {
  const t = String(text ?? '').trim();
  if (t === 'null') return null;
  const m = t.match(/^(['"`])([^'"`$\\]*)\1$/);
  return m ? m[2] : undefined;
}

/** route.values: {feld: [erlaubte Literale]} – nur prüfbar, wenn der Aufruf ein Literal übergibt. */
function badValues(route, parsed) {
  if (!route.values || !parsed) return [];
  const bad = [];
  for (const [field, allowed] of Object.entries(route.values)) {
    if (!parsed.keys.includes(field)) continue;
    const lit = literalOf(parsed.values[field]);
    if (lit === undefined) continue;
    if (!allowed.includes(lit)) bad.push(`${field}=${lit === null ? 'null' : `'${lit}'`} (erlaubt: ${allowed.map((v) => (v === null ? 'null' : v)).join('|')})`);
  }
  return bad;
}

function familyCovered(p) {
  return COVERED_FAMILIES.some((f) => p.startsWith(f));
}

function judge(capPath, argText) {
  const route = ROUTE_BY_PATH.get(capPath);
  const parsed = topKeys(argText);
  if (!route) {
    if (capPath.includes('${')) return { cls: 'OHNE_VERTRAG', soll: 'dynamischer Pfad; Familie ohne Kernvertrag' };
    if (REMOVED[capPath]) return { cls: 'ABWEICHUNG', soll: REMOVED[capPath] };
    if (familyCovered(capPath)) {
      return { cls: 'ABWEICHUNG', soll: RENAMES[capPath] ? `Pfad ${RENAMES[capPath]}` : 'Pfad nicht im Kernvertrag (CAP-1/2/3 kennt ihn nicht)' };
    }
    if (OPEN_IN_CORE?.[capPath]) return { cls: 'OHNE_VERTRAG', soll: OPEN_IN_CORE[capPath] };
    return { cls: 'OHNE_VERTRAG', soll: 'kein Kernvertrag (Familie nicht in CAP-1/2/3)' };
  }
  if (route.kind === 'cap2' || route.kind === 'flat') {
    if (route.fields === '*') {
      const bad = badValues(route, parsed);
      return bad.length
        ? { cls: 'ABWEICHUNG', soll: `native Payload — Wert unzulässig: ${bad.join('; ')} (${route.src})` }
        : { cls: 'OK', soll: `native Payload (${route.src})` };
    }
    // Argumentloser Aufruf (z. B. ctx.cap.secret.scope()): der Kern setzt {} ein (cap3.rs wire_args).
    if (argText === '' && !route.required.length) return { cls: 'OK', soll: `argumentlos (${route.src})` };
    if (!parsed) return { cls: 'UNGEPRUEFT', soll: `{${route.allowed.join(', ')}} (${route.src})` };
    const unknown = parsed.keys.filter((k) => !route.allowed.includes(k));
    const missing = parsed.spread ? [] : route.required.filter((r) => (Array.isArray(r) ? !r.some((x) => parsed.keys.includes(x)) : !parsed.keys.includes(r)))
      .map((r) => (Array.isArray(r) ? r.join('|') : r));
    // §16.32: BlobRef-Felder als Objektliteral müssen genau {ref} sein (keine transfer_id/sha256/size).
    const badRef = BLOB_REF_FIELDS.filter((f) => parsed.keys.includes(f) && route.allowed.includes(f))
      .filter((f) => {
        const v = String(parsed.values[f] ?? '').trim();
        if (!v.startsWith('{')) return false;
        const inner = topKeys(v);
        return !inner || inner.spread || inner.keys.length !== BLOB_REF_KEYS.length
          || !BLOB_REF_KEYS.every((k) => inner.keys.includes(k));
      });
    const badVal = badValues(route, parsed);
    if (!unknown.length && !missing.length && !badRef.length && !badVal.length) return { cls: 'OK', soll: route.src };
    const parts = [];
    if (badVal.length) parts.push(`Wert unzulässig: ${badVal.join('; ')}`);
    if (badRef.length) parts.push(`BlobRef-Form {ref} (${BLOB_REF_SOURCE}) verletzt: ${badRef.join(', ')}`);
    if (unknown.length) parts.push(`unbekannt: ${unknown.join(', ')}`);
    if (missing.length) parts.push(`fehlt: ${missing.join(', ')}`);
    return { cls: 'ABWEICHUNG', soll: `{${route.allowed.join(', ')}} — ${parts.join('; ')} (${route.src})` };
  }
  // cap1 / alias
  const shape = route.kind === 'alias'
    ? `{action: ${route.actions.join('|')}, payload, record_id?}`
    : '{payload, record_id?}';
  if (!parsed) return { cls: 'UNGEPRUEFT', soll: `${shape} (${route.src})` };
  if (route.kind === 'alias') {
    if (!parsed.keys.includes('action') && !parsed.spread) {
      return { cls: 'ABWEICHUNG', soll: `${shape} — action fehlt; Kern bietet kein generisches ${capPath}, nur Legacy-Befehle (${route.src})` };
    }
    const av = parsed.values.action;
    const lit = av && av.match(/^['"`]([^'"`$]+)['"`]$/);
    if (lit && !route.actions.includes(lit[1])) {
      return { cls: 'ABWEICHUNG', soll: `${shape} — action '${lit[1]}' unbekannt (${route.src})` };
    }
  }
  const extra = parsed.keys.filter((k) => !CAP1_KEYS.has(k));
  if (!extra.length) return { cls: 'OK', soll: route.src };
  if (parsed.keys.includes('payload')) {
    return { cls: 'ABWEICHUNG', soll: `${shape} — zusätzlich: ${extra.join(', ')} (${route.src})` };
  }
  // §16.30 Nr. 5: flache Argumente sind Vertrag (wire_args hüllt auch CAP-1 intern).
  if (FLAT_ARGS_SOURCE) return { cls: 'OK', soll: `${route.src}; flach (${FLAT_ARGS_SOURCE})` };
  return { cls: 'HUELLE', soll: `${shape} — flach übergeben: ${extra.join(', ')} (${route.src})` };
}

// Direkt (auch optional verkettet), Indexzugriff, callCap-Helfer.
const CALL_RE = /ctx\??\.cap((?:\??\.[A-Za-z_$][\w$]*)+)((?:\[\s*['"][^'"]+['"]\s*\])?)\s*\(|callCap\(\s*ctx\s*,\s*(['"`])([^'"`]+)\3\s*(,|\))/g;
// Array-Pfad-Helfer: cap(ctx, ['a', 'b'])(arg)  (threads/_lib.js)
const ARRAY_RE = /\bcap\(\s*ctx\s*,\s*\[([^\]]*)\]\s*\)\s*\(/g;
// Alias: const x = ctx.cap?.a?.b;  …  x.fn(arg)  (support)
const ALIAS_RE = /(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*ctx\??\.cap((?:\??\.[A-Za-z_$][\w$]*)+)\s*;/g;

/** Scannt die Handler aller Apps unter den gegebenen Wurzeln. */
export function scanCalls(bosRoot, roots = ['modules', 'customer-modules']) {
  const rows = [];
  for (const root of roots) {
    const base = path.join(bosRoot, root);
    if (!fs.existsSync(base)) continue;
    for (const app of fs.readdirSync(base).sort()) {
      const hdir = path.join(base, app, 'handlers');
      if (!fs.existsSync(hdir) || !fs.statSync(hdir).isDirectory()) continue;
      const stack = [hdir];
      while (stack.length) {
        const dir = stack.pop();
        for (const name of fs.readdirSync(dir).sort()) {
          const full = path.join(dir, name);
          const st = fs.statSync(full);
          if (st.isDirectory()) { if (name !== 'tests' && name !== 'node_modules') stack.push(full); continue; }
          // Handler-Tests liegen in handlers/tests/ und heißen *.test.mjs. Ein Befehlstyp darf
          // auf ".test" enden (outbound.research_source.test.js ist ein echter Handler).
          if (!/\.(m?js)$/.test(name) || /\.test\.mjs$/.test(name)) continue;
          const src = stripComments(fs.readFileSync(full, 'utf8'));
          const rel = path.relative(bosRoot, full);
          const add = (capPath, argText, idx, via) => {
            const { cls, soll } = judge(capPath, argText);
            const flat = argText.replace(/\s+/g, ' ');
            const ist = `${capPath}(${flat.length > 90 ? flat.slice(0, 87) + '…' : flat})${via ? ` [${via}]` : ''}`;
            rows.push({ file: rel, line: lineOf(src, idx), app, root, path: capPath, ist, soll, cls, via: via || 'direkt' });
          };
          const clean = (p) => p.replace(/\?/g, '').replace(/^\./, '');
          let m;
          CALL_RE.lastIndex = 0;
          while ((m = CALL_RE.exec(src))) {
            if (m[1]) {
              let capPath = clean(m[1]);
              if (m[2]) capPath += '.' + m[2].replace(/^\[\s*['"]|['"]\s*\]$/g, '');
              add(capPath, firstArg(src, CALL_RE.lastIndex), m.index);
            } else {
              add(m[4], m[5] === ',' ? firstArg(src, CALL_RE.lastIndex) : '', m.index, 'callCap');
            }
          }
          ARRAY_RE.lastIndex = 0;
          while ((m = ARRAY_RE.exec(src))) {
            const parts = [...m[1].matchAll(/['"]([^'"]+)['"]/g)].map((x) => x[1]);
            add(parts.join('.'), firstArg(src, ARRAY_RE.lastIndex), m.index, 'cap(ctx,[…])');
          }
          ALIAS_RE.lastIndex = 0;
          while ((m = ALIAS_RE.exec(src))) {
            const [, name, rawPrefix] = m;
            const prefix = clean(rawPrefix);
            const useRe = new RegExp(`\\b${name.replace(/\$/g, '\\$')}\\??\\.([A-Za-z_$][\\w$]*)\\s*\\(`, 'g');
            let u;
            while ((u = useRe.exec(src))) add(`${prefix}.${u[1]}`, firstArg(src, useRe.lastIndex), u.index, `Alias ${name}`);
          }
        }
      }
    }
  }
  rows.sort((a, b) => (a.file === b.file ? a.line - b.line : a.file < b.file ? -1 : 1));
  return rows;
}

export function renderCallsMarkdown(rows, source, snapshot = '') {
  const by = (c) => rows.filter((r) => r.cls === c);
  const esc = (s) => String(s).replace(/\|/g, '\\|');
  const counts = ['ABWEICHUNG', 'HUELLE', 'UNGEPRUEFT', 'OK', 'OHNE_VERTRAG'].map((c) => `${c} ${by(c).length}`).join(' · ');
  let out = `<!-- BEGIN GENERATED (check-caps.mjs --calls-md) -->\n`;
  out += `_Stand: ${snapshot || 'unbekannt'}. Kernvertrag: ${source}. ${rows.length} Capability-Aufrufe in \`modules/*/handlers\` und \`customer-modules/*/handlers\` (ohne Tests)._\n\n`;
  out += `**Zählung:** ${counts}\n\n`;
  for (const cls of ['ABWEICHUNG', 'HUELLE', 'UNGEPRUEFT', 'OHNE_VERTRAG', 'OK']) {
    const list = by(cls);
    if (!list.length) continue;
    out += `### ${cls} (${list.length})\n\n| Datei:Zeile | Ist | Soll |\n|---|---|---|\n`;
    for (const r of list) out += `| \`${esc(r.file)}:${r.line}\` | \`${esc(r.ist)}\` | ${esc(r.soll)} |\n`;
    out += '\n';
  }
  out += `<!-- END GENERATED -->\n`;
  return out;
}
