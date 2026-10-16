// Mutationsbeleg für tests/invariants.test.mjs der App outbound-lead-generation (Strang INV, Owner-Vorgabe E13).
// Gleiche Mechanik wie modules/outbound/handlers/tests/mutation-proof.mjs (bewusst kopiert: je App ein Skript).
// Für jede Invariante wird je eine Handler-Zeile in einer TEMP-KOPIE des handlers/-Ordners per
// String-Ersetzung gebrochen; der zugehörige Invariantentest muss rot werden (kein Syntaxfehler),
// die unveränderte Kopie muss vorher und nachher grün sein. Der Arbeitsbaum wird nie verändert.
// Aufruf: node tests/mutation-proof.mjs   (Exit 0 nur, wenn jede Mutation rot und Baseline grün ist)
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HANDLERS = resolve(dirname(fileURLToPath(import.meta.url)), '..');
// ctox:shared/-Auflösung für die Kindprozesse (absolut, die Kopie liegt im Temp-Ordner).
const BUSINESS_OS = resolve(HANDLERS, '../../..');
const CTOX_SHARED = resolve(BUSINESS_OS, 'v4/tools/node/ctox-shared.mjs');

export const MUTATIONS = [
  { inv: 'INV5', file: '_lib/research-evidence.js',
    from: 'if (evidenceKey !== personKey) return false;',
    to: 'if (false) return false;' },
  { inv: 'INV6', file: '_lib/research-writeback-checks.js',
    from: 'if (!(anyVerified || anyEvidence || anyPersonVerified || anyPersonEvidence)) {',
    to: 'if (false) {' },
  { inv: 'INV6', file: '_lib/research-outcome-patch.js',
    from: "research_status: researchedFieldKeys.length > 0 && unverifiedFieldKeys.length === 0 ? 'completed' : 'needs_review',",
    to: "research_status: researchedFieldKeys.length > 0 ? 'completed' : 'needs_review'," },
  { inv: 'INV6', file: '_lib/research-writeback-sanitize.js',
    from: 'if (unabhaengig < benoetigt) {',
    to: 'if (false) {' },
];

export function runProof(handlersDir, mutations, testFile = 'tests/invariants.test.mjs') {
  const work = mkdtempSync(join(tmpdir(), 'inv-mutation-'));
  const copy = join(work, 'handlers');
  const runTest = (pattern) => {
    const args = ['--import', CTOX_SHARED, '--test', ...(pattern ? ['--test-name-pattern', `^${pattern} `] : []), testFile];
    const r = spawnSync(process.execPath, args, { cwd: copy, encoding: 'utf8', env: { ...process.env, CTOX_BUSINESS_OS_ROOT: BUSINESS_OS } });
    const out = `${r.stdout}\n${r.stderr}`;
    const pass = Number(/ℹ pass (\d+)/.exec(out)?.[1] ?? 0);
    const fail = Number(/ℹ fail (\d+)/.exec(out)?.[1] ?? 0);
    const syntax = /SyntaxError|ReferenceError|ERR_MODULE_NOT_FOUND/.test(out);
    const why = /AssertionError[^:]*: ([^\n]*)/.exec(out)?.[1]?.trim() ?? '';
    return { status: r.status, pass, fail, syntax, why };
  };
  const rows = [];
  let ok = true;
  try {
    cpSync(handlersDir, copy, { recursive: true });
    // Paket-Dateien neben handlers/ (Templates, Skills; Strang SKL-C), damit ctx.asset in der Kopie liest.
    for (const sibling of ['prompts', 'skills']) {
      const from = join(handlersDir, '..', sibling);
      if (existsSync(from)) cpSync(from, join(work, sibling), { recursive: true });
    }
    const baseline = runTest(null);
    rows.push({ inv: 'Baseline', file: '-', mutation: 'unverändert', result: baseline.status === 0 ? `grün (${baseline.pass} pass)` : `ROT (${baseline.fail} fail)` });
    if (baseline.status !== 0) ok = false;
    for (const m of mutations) {
      const path = join(copy, m.file);
      const original = readFileSync(path, 'utf8');
      const hits = original.split(m.from).length - 1;
      if (hits !== 1) {
        rows.push({ inv: m.inv, file: m.file, mutation: m.from, result: `MUSTER ${hits}x gefunden (erwartet 1) – Beleg ungültig` });
        ok = false;
        continue;
      }
      writeFileSync(path, original.replace(m.from, m.to));
      const broken = runTest(m.inv);
      writeFileSync(path, original);
      const restored = runTest(m.inv);
      const red = broken.status !== 0 && broken.fail > 0 && !broken.syntax;
      const green = restored.status === 0 && restored.pass > 0;
      if (!red || !green) ok = false;
      rows.push({
        inv: m.inv, file: m.file, mutation: `${m.from} → ${m.to}`,
        result: `${red ? `rot („${broken.why}“)` : broken.syntax ? 'SYNTAXFEHLER (zählt nicht)' : 'NICHT ROT'} / ${green ? 'zurück grün' : 'ZURÜCK NICHT GRÜN'}`,
      });
    }
    const final = runTest(null);
    rows.push({ inv: 'Wiederhergestellt', file: '-', mutation: 'alle Dateien zurück', result: final.status === 0 ? `grün (${final.pass} pass)` : 'ROT' });
    if (final.status !== 0) ok = false;
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
  console.log('| Invariante | Datei | Mutation | Ergebnis |\n|---|---|---|---|');
  for (const r of rows) console.log(`| ${r.inv} | ${r.file} | \`\` ${r.mutation.replaceAll('|', '\\|')} \`\` | ${r.result} |`);
  console.log(ok ? '\nMUTATIONSBELEG: vollständig' : '\nMUTATIONSBELEG: UNVOLLSTÄNDIG');
  return ok;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = runProof(HANDLERS, MUTATIONS) ? 0 : 1;
}
