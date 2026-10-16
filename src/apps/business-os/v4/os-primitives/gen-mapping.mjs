#!/usr/bin/env node
// Origin: CTOX
// License: Apache-2.0
//
// Generates the body of MAPPING.md from the live v4 manifests + catalog.mjs so the
// table always covers every capabilities_required exactly once. Deterministic.
//   node gen-mapping.mjs            print the generated markdown to stdout
//   node gen-mapping.mjs --check    exit 1 if MAPPING.md is stale (between markers)

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { classify, PRIMITIVE_ORDER } from './catalog.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const MANIFESTS = path.resolve(HERE, '../manifests');
const MAPPING = path.join(HERE, 'MAPPING.md');
const BEGIN = '<!-- BEGIN GENERATED (gen-mapping.mjs) -->';
const END = '<!-- END GENERATED -->';

const byCap = new Map();
for (const f of fs.readdirSync(MANIFESTS).sort()) {
  if (!f.endsWith('.manifest.json')) continue;
  const m = JSON.parse(fs.readFileSync(path.join(MANIFESTS, f), 'utf8'));
  const ev = m._capability_evidence || {};
  for (const cap of m.capabilities_required || []) {
    if (!byCap.has(cap)) byCap.set(cap, { apps: new Set(), files: new Set() });
    byCap.get(cap).apps.add(m.app_id);
    for (const h of ev[cap] || []) byCap.get(cap).files.add(`${m._source_dir?.split('/').pop() || m.app_id}/${h}`);
  }
}

const caps = [...byCap.keys()].sort();
const rows = caps.map((cap) => ({ cap, ...byCap.get(cap), c: classify(cap) }));

const os = rows.filter((r) => r.c.kind === 'os');
const app = rows.filter((r) => r.c.kind === 'app');
const plat = rows.filter((r) => r.c.kind === 'platform');
const unknown = rows.filter((r) => r.c.kind === 'unknown');

const esc = (s) => String(s).replace(/\|/g, '\\|');

let out = '';
out += `${BEGIN}\n`;
out += `_Generiert aus ${caps.length} eindeutigen \`capabilities_required\` über ${new Set(rows.flatMap((r) => [...r.apps])).size} Manifeste. Nicht von Hand ändern; \`node gen-mapping.mjs\`._\n\n`;
out += `**Zählung:** OS-Primitiv ${os.length} · App-Logik ${app.length} · Plattform ${plat.length} · UNBEKANNT ${unknown.length}.\n\n`;

out += `### (a) Bereits ein OS-Primitiv\n\n`;
out += `| Capability | Primitiv | Apps |\n|---|---|---|\n`;
for (const r of os) out += `| \`${esc(r.cap)}\` | \`${r.c.primitive}\` | ${[...r.apps].sort().join(', ')} |\n`;

out += `\n### (b) App-Logik (muss aus dem Core in App-Handler)\n\n`;
out += `| Capability | App | Fachlogik | Umsetzung in der App | Aufwand | Handler |\n|---|---|---|---|---|---|\n`;
for (const r of app) out += `| \`${esc(r.cap)}\` | ${esc(r.c.app)} | ${esc(r.c.logic || '')} | ${esc(r.c.via || '')} | ${r.c.effort || ''} | ${esc([...r.files].sort().join('; ') || (r.c.files || []).join('; '))} |\n`;

out += `\n### (c) Plattform (ctox-platform, nicht App)\n\n`;
out += `| Capability | Begründung | Apps |\n|---|---|---|\n`;
for (const r of plat) out += `| \`${esc(r.cap)}\` | ${esc(r.c.why || '')} | ${[...r.apps].sort().join(', ')} |\n`;

if (unknown.length) {
  out += `\n### UNBEKANNT (Triage nötig — Guard schlägt an)\n\n`;
  for (const r of unknown) out += `- \`${esc(r.cap)}\` <- ${[...r.apps].sort().join(', ')}\n`;
}
out += `${END}\n`;

if (process.argv.includes('--check')) {
  const cur = fs.readFileSync(MAPPING, 'utf8');
  const a = cur.indexOf(BEGIN), b = cur.indexOf(END);
  const block = a >= 0 && b >= 0 ? cur.slice(a, b + END.length) + '\n' : '';
  if (block.trim() !== out.trim()) { console.error('MAPPING.md stale — run: node gen-mapping.mjs > /tmp/x && splice'); process.exit(1); }
  console.log('MAPPING.md up to date'); process.exit(0);
}
process.stdout.write(out);
