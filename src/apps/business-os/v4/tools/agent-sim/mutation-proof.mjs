#!/usr/bin/env node
// Mutationsbeleg Strang SKL0: agent-task-sim.mjs, check-skills.mjs, shared/prompt-template.js und der §16.30-Teil von
// gen-manifest.mjs. Jede Mutation bricht genau eine Zeile in einer TEMP-SPIEGELUNG des Repos (Zielordner echte Kopie,
// alles andere Symlinks, damit Pfade relativ zur Datei gleich bleiben); der zugehörige Test muss rot werden, ohne
// Syntaxfehler. Die unveränderte Spiegelung muss vorher grün sein. Der Arbeitsbaum wird nie verändert.
// Aufruf: node v4/tools/agent-sim/mutation-proof.mjs   (Exit 0 nur, wenn Baseline grün und jede Mutation rot)
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const BOS = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..'); // src/apps/business-os
const REPO = path.resolve(BOS, '../../..');
const BOS_REL = path.relative(REPO, BOS);

const SUITES = {
  sim: { dir: 'v4/tools/agent-sim', test: 'agent-task-sim.test.mjs', copyAll: true },
  guard: { dir: 'v4/tools/agent-sim', test: 'check-skills.test.mjs', copyAll: true },
  template: { dir: 'shared', test: 'prompt-template.test.mjs', copy: ['prompt-template.js', 'prompt-template.test.mjs'] },
  gen: { dir: 'v4/tools/manifest', test: 'gen-manifest.test.mjs', copyAll: true, pattern: 'Agenten-Paket' },
};

export const MUTATIONS = [
  // shared/prompt-template.js
  { suite: 'template', file: 'prompt-template.js', what: 'fehlender Platzhalter wird leer',
    from: 'if (v === undefined || v === null) { if (!missing.includes(t.field)) missing.push(t.field); continue; }', to: 'if (v === undefined || v === null) { continue; }' },
  { suite: 'template', file: 'prompt-template.js', what: 'kaputte {{ werden nicht erkannt',
    from: "      if (!m) {\n", to: '      if (false) {\n' },
  { suite: 'template', file: 'prompt-template.js', what: '.. im Template-Pfad erlaubt',
    from: "parts.some((p) => p === '..' || p === '.' || p === '')", to: 'false' },
  { suite: 'template', file: 'prompt-template.js', what: 'Objekte werden eingesetzt',
    from: 'else if (!badType.includes(t.field)) badType.push(t.field);', to: 'else out += String(v);' },
  { suite: 'template', file: 'prompt-template.js', what: 'ctx.asset mit falschem Pfad',
    from: '(p) => ctx.asset(p)', to: "() => ctx.asset('prompts/x.md')" },
  { suite: 'template', file: 'prompt-template.js', what: 'Escape \\{{ entfernt',
    from: "if (template.startsWith('\\\\{{', i)) { text += '{{'; i += 3; continue; }", to: '' },
  // v4/tools/agent-sim/agent-task-sim.mjs
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'zweite Zustellung nicht verworfen',
    from: 'if (delivered.has(taskId) || TERMINAL.has(t.status)) {', to: 'if (false) {' },
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'Skill nicht im Manifest geprüft',
    from: "if (!decl) throw simError('invalid'", to: "if (false) throw simError('invalid'" },
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'Skill-Datei nicht geprüft',
    from: "if (!readPackage(rel)) throw simError('invalid', `enqueue: Skill-Datei", to: "if (false) throw simError('invalid', `enqueue: Skill-Datei" },
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'fremder Namensraum erlaubt',
    from: "if (ns !== appId) throw simError('invalid'", to: "if (false) throw simError('invalid'" },
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'System-Skill nicht geprüft',
    from: "if (!coreNames().has(skill)) throw", to: 'if (false) throw' },
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'Routing ignoriert Ausgang',
    from: "const type = outcome === 'completed' ? t.on_complete : t.on_failed;", to: 'const type = t.on_complete || t.on_failed;' },
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'Callback-Payload mit Zusatzfeld',
    from: 'const payload = { task_id: taskId, status: outcome,', to: 'const payload = { task_id: taskId, app_id: appId, status: outcome,' },
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'unbekannte enqueue-Felder erlaubt',
    from: "if (unknown.length) throw simError('invalid', `enqueue: unbekannte", to: "if (false) throw simError('invalid', `enqueue: unbekannte" },
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'Callback-Typ ohne Handler erlaubt',
    from: 'if (handlerTypes && !handlerTypes.has(value)) throw', to: 'if (false) throw' },
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'Idempotenz aus',
    from: '          if (prev) {', to: '          if (false) {' },
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'asset liest Testdateien',
    from: 'const buf = isTestPath(rel) ? null : readPackage(rel);', to: 'const buf = readPackage(rel);' },
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'asset ohne 1-MiB-Grenze',
    from: 'if (buf.length > ASSET_MAX_BYTES)', to: 'if (buf.length > ASSET_MAX_BYTES + 1)' },
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'asset UTF-8 nicht streng',
    from: "new TextDecoder('utf-8', { fatal: true })", to: "new TextDecoder('utf-8', { fatal: false })" },
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'asset folgt Symlink nach außen',
    from: 'if (real !== root && !real.startsWith(root + path.sep)) return null;', to: '' },
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'asset erlaubt ..',
    from: "parts.some((s) => s === '' || s === '.' || s === '..')", to: "parts.some((s) => s === '')" },
  { suite: 'sim', file: 'agent-task-sim.mjs', what: 'Trigger ohne watch-Export erlaubt',
    from: "if (!Array.isArray(exported) || !exported.includes('watch'))", to: 'if (false)' },
  // v4/tools/agent-sim/check-skills.mjs
  { suite: 'guard', file: 'check-skills.mjs', what: 'globale Phantome übersehen',
    from: '} else if (!core.has(r.ref)) {', to: '} else if (false) {' },
  { suite: 'guard', file: 'check-skills.mjs', what: 'fremder Namensraum übersehen',
    from: 'if (ns !== app) add(', to: 'if (false) add(' },
  { suite: 'guard', file: 'check-skills.mjs', what: 'on_failed nicht geprüft',
    from: "for (const k of ['on_complete', 'on_failed'])", to: "for (const k of ['on_complete'])" },
  { suite: 'guard', file: 'check-skills.mjs', what: 'Spread gilt als geprüft',
    from: 'if (!shape || shape.spread)', to: 'if (!shape)' },
  { suite: 'guard', file: 'check-skills.mjs', what: 'Ratchet lässt alles durch',
    from: 'fresh: findings.filter((f) => !tolerated.has(f.key)),', to: 'fresh: [],' },
  { suite: 'guard', file: 'check-skills.mjs', what: 'Testdateien mitgezählt',
    from: '        if (isTestPath(rel)) continue;\n', to: '' },
  { suite: 'guard', file: 'check-skills.mjs', what: 'Exit-Code immer 0',
    from: 'return fresh.length ? 1 : 0;', to: 'return 0;' },
  // v4/tools/manifest/gen-manifest.mjs (§16.30-Teil)
  { suite: 'gen', file: 'gen-manifest.mjs', what: 'Skill-Ordner ohne SKILL.md still',
    from: "if (!fs.existsSync(path.join(dir, 'SKILL.md'))) { problems.push", to: "if (false) { problems.push" },
  { suite: 'gen', file: 'gen-manifest.mjs', what: 'undeklarierter Skill erlaubt',
    from: "else if (!declared.has(name)) violations.push", to: 'else if (false) violations.push' },
  { suite: 'gen', file: 'gen-manifest.mjs', what: 'fremder Namensraum erlaubt',
    from: "if (ns !== appId) violations.push", to: 'if (false) violations.push' },
  { suite: 'gen', file: 'gen-manifest.mjs', what: 'skills fehlt im Manifest',
    from: '...(app.agent.skills.length ? { skills: app.agent.skills } : {}),', to: '...({}),' },
  { suite: 'gen', file: 'gen-manifest.mjs', what: 'nur erstes required_skills-Array',
    from: 'for (const arr of m[1].matchAll(/\\[([^\\]]*)\\]/g)) {', to: 'for (const arr of [...m[1].matchAll(/\\[([^\\]]*)\\]/g)].slice(0, 1)) {' },
  { suite: 'gen', file: 'gen-manifest.mjs', what: 'return in Skill-Funktion übersehen',
    from: "for (const r of body.matchAll(/\\breturn\\s+(['\"])([^'\"\\n]+)\\1/g))", to: 'for (const r of [])' },
  { suite: 'gen', file: 'gen-manifest.mjs', what: 'prompt-template.js nicht im Shell-Artefakt',
    from: "  'prompt-template.js',\n", to: '' },
  { suite: 'gen', file: 'gen-manifest.mjs', what: 'Verstöße nicht im _todo',
    from: '      todo.push(v.ref\n', to: '      void (v.ref\n' },
  { suite: 'gen', file: 'gen-manifest.mjs', what: 'Testdatei-Regel für Skills aus',
    from: "      if (isTestPath(rel)) { problems.push({ kind: 'excluded_by_test_rule', name, path: rel }); continue; }\n      skills.push", to: '      skills.push' },
];

// Spiegel: entlang des Pfads zum Zielordner echte Ordner, Geschwister als Symlink; im Zielordner echte Kopien.
function mirror(suite) {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'skl0-mut-'));
  const targetRel = path.join(BOS_REL, suite.dir);
  const segs = targetRel.split(path.sep);
  let real = REPO;
  let fake = tmp;
  for (const seg of segs) {
    for (const name of fs.readdirSync(real)) if (name !== seg) fs.symlinkSync(path.join(real, name), path.join(fake, name));
    real = path.join(real, seg);
    fake = path.join(fake, seg);
    fs.mkdirSync(fake);
  }
  for (const name of fs.readdirSync(real)) {
    const copy = suite.copyAll || suite.copy.includes(name);
    if (copy) fs.cpSync(path.join(real, name), path.join(fake, name), { recursive: true, verbatimSymlinks: true });
    else fs.symlinkSync(path.join(real, name), path.join(fake, name));
  }
  return { tmp, dir: fake };
}

function runSuite(suite, dir) {
  const args = ['--test'];
  if (suite.pattern) args.push('--test-name-pattern', suite.pattern);
  args.push(path.join(dir, suite.test));
  return spawnSync(process.execPath, args, { encoding: 'utf8', cwd: dir });
}

function main() {
  let ok = true;
  for (const [name, suite] of Object.entries(SUITES)) {
    const { tmp, dir } = mirror(suite);
    try {
      const r = runSuite(suite, dir);
      const green = r.status === 0;
      console.log(`${green ? 'grün' : 'ROT '}  Baseline ${name} (${suite.test}${suite.pattern ? ` /${suite.pattern}/` : ''})`);
      if (!green) { ok = false; console.log(r.stdout.slice(-2000)); }
    } finally { fs.rmSync(tmp, { recursive: true, force: true }); }
  }
  for (const m of MUTATIONS) {
    const suite = SUITES[m.suite];
    const { tmp, dir } = mirror(suite);
    try {
      const file = path.join(dir, m.file);
      const src = fs.readFileSync(file, 'utf8');
      const count = src.split(m.from).length - 1;
      if (count !== 1) { ok = false; console.log(`FEHLT  ${m.suite}/${m.file}: Muster ${count}× statt 1× – ${m.what}`); continue; }
      fs.writeFileSync(file, src.replace(m.from, m.to));
      const r = runSuite(suite, dir);
      const syntax = /SyntaxError/.test(r.stdout + r.stderr);
      const red = r.status !== 0 && !syntax;
      if (!red) ok = false;
      console.log(`${red ? 'rot ' : syntax ? 'SYNT' : 'GRÜN'}  ${m.suite.padEnd(8)} ${m.what}`);
    } finally { fs.rmSync(tmp, { recursive: true, force: true }); }
  }
  console.log(ok ? `mutation-proof: OK – ${MUTATIONS.length} Mutationen rot, Baselines grün` : 'mutation-proof: FEHLER');
  return ok ? 0 : 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exit(main());
