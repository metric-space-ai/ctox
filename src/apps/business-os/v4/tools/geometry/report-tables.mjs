#!/usr/bin/env node
// Markdown-Tabellen aus out/<variante>/results.json (für REPORT.md).
//   node v4/tools/geometry/report-tables.mjs [frame|shared-css] [--detail]
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const variant = process.argv[2] && !process.argv[2].startsWith('--') ? process.argv[2] : 'frame';
const detail = process.argv.includes('--detail');
const r = JSON.parse(readFileSync(join(HERE, 'out', variant, 'results.json'), 'utf8'));
const crit = ['G1', 'G2', 'G3', 'G4', 'G5', 'G6'];
const apps = [...new Set(r.results.map((x) => x.app))];
const cell = (x) => (x ? (x.ok ? 'OK' : x.fails.join(' ')) : '–');
const lines = [];
lines.push(`Variante \`${r.variant}\`, ${r.at.slice(0, 16)}Z, Breiten ${r.widths.join('/')}, ${r.results.filter((x) => x.ok).length}/${r.results.length} Läufe OK.`);
lines.push('');
lines.push(`| App | ${r.widths.map((w) => `${w}`).join(' | ')} | Titel | Tokens fehlen | Seitenfehler |`);
lines.push(`|---|${r.widths.map(() => '---').join('|')}|---|---:|---:|`);
for (const a of apps) {
  const rows = r.widths.map((w) => r.results.find((x) => x.app === a && x.width === w));
  const first = rows.find(Boolean);
  const errs = new Set(rows.flatMap((x) => x?.errors || []));
  lines.push(`| ${a} | ${rows.map(cell).join(' | ')} | ${first?.info?.title || '–'} | ${first?.info?.undefinedTokenCount ?? '–'} | ${errs.size} |`);
}
lines.push('');
const count = Object.fromEntries(crit.map((k) => [k, r.results.filter((x) => x[k]?.length).length]));
lines.push(`Fehlschläge je Kriterium (Läufe): ${crit.map((k) => `${k} ${count[k]}`).join(' · ')}`);
if (detail) {
  lines.push('');
  for (const a of apps) {
    const rows = r.results.filter((x) => x.app === a);
    const msgs = [];
    for (const x of rows) for (const k of crit) for (const m of x[k] || []) msgs.push(`- ${x.width} ${k}: ${m}`);
    const errs = [...new Set(rows.flatMap((x) => x.errors || []))];
    if (!msgs.length && !errs.length) continue;
    lines.push(`#### ${a}`);
    lines.push(...msgs.slice(0, 14));
    if (errs.length) lines.push(`- Seitenfehler: ${errs.slice(0, 4).join(' | ')}`);
    lines.push('');
  }
}
console.log(lines.join('\n'));
