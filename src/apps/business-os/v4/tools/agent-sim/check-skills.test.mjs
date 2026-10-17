// node --test src/apps/business-os/v4/tools/agent-sim/check-skills.test.mjs  (Mutationsbeleg: mutation-proof.mjs)
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { judge, objectKeys, scanSkills } from './check-skills.mjs';

const CLI = path.join(path.dirname(fileURLToPath(import.meta.url)), 'check-skills.mjs');

// Mini-Repo: <tmp>/src/skills (System-Skills) und <tmp>/src/apps/business-os/{modules,customer-modules}.
function miniRepo() {
  const repo = fs.mkdtempSync(path.join(os.tmpdir(), 'check-skills-'));
  const bos = path.join(repo, 'src/apps/business-os');
  const put = (rel, text) => { const f = path.join(repo, rel); fs.mkdirSync(path.dirname(f), { recursive: true }); fs.writeFileSync(f, text); };
  put('src/skills/system/research/core-skill/SKILL.md', '---\nname: core-skill\n---\n');
  put('src/skills/packs/content/doc/SKILL.md', '---\nname: "doc"\n---\n');
  put('src/apps/business-os/modules/alpha/skills/own/SKILL.md', '# own\n');
  put('src/apps/business-os/modules/alpha/index.js', [
    "const ALPHA_SKILL = 'core-skill';",
    "const labels = { skill: 'Skill' };",
    "const keywords = { skill: ['cad', 'sap'] };",
    "// skill: 'kommentar-phantom'",
    "send({ required_skills: flag ? ['doc', 'ui-phantom'] : ['doc'] });",
    "send({ skill: 'beta/fremd' });",
  ].join('\n'));
  put('src/apps/business-os/modules/alpha/handlers/_lib/enqueue.js', [
    'export function suggestedSkill(type) {',
    "  if (type.startsWith('x')) return 'core-skill';",
    "  return 'fn-phantom';",
    '}',
    'export async function a(ctx, p) {',
    "  await ctx.cap.agent.task.enqueue({ title: 't', instruction: 'i', skill: 'alpha/own', payload: p, on_complete: 'alpha.done', on_failed: 'alpha.failed' });",
    "  await ctx.cap?.agent?.task?.enqueue({ title: 't', instruction: 'i', skill: 'alpha/ghost', payload: p, on_complete: 'alpha.done' });",
    "  await ctx.cap.agent.task.enqueue_scrape_repair({ title: 't' });",
    '}',
  ].join('\n'));
  put('src/apps/business-os/modules/alpha/handlers/alpha.run.js', [
    "import { callCap } from './_base.js';",
    'export default async function handle(command, ctx) {',
    "  await callCap(ctx, 'agent.task.enqueue', { title: 't', instruction: 'i', payload: {}, skill: x ?? 'opt-phantom' });",
    '  const task = ctx.cap.agent.task;',
    '  await task.enqueue(args);',
    "  await ctx.cap.agent.task.enqueue({ ...base, on_complete: 'a', on_failed: 'b' });",
    '}',
  ].join('\n'));
  put('src/apps/business-os/modules/alpha/handlers/tests/alpha.test.mjs', "send({ skill: 'test-phantom' });\n");
  put('src/apps/business-os/modules/alpha/vendor/lib.js', "send({ skill: 'vendor-phantom' });\n");
  put('src/apps/business-os/customer-modules/beta/index.js', "send({ skill: 'beta/fremd', suggested_skill: 'core-skill' });\n");
  return { repo, bos };
}

test('scanSkills: Phantome, Namensräume, enqueue ohne Callback, ungeprüfte Aufrufe', () => {
  const { repo, bos } = miniRepo();
  try {
    const findings = scanSkills(bos, { coreSkills: ['core-skill', 'doc'] });
    const got = findings.map((f) => `${f.kind} ${f.file}:${f.line} ${f.detail}`);
    assert.deepEqual(got.sort(), [
      'enqueue-no-callback modules/alpha/handlers/_lib/enqueue.js:7 on_failed',
      'enqueue-no-callback modules/alpha/handlers/alpha.run.js:3 on_complete',
      'enqueue-no-callback modules/alpha/handlers/alpha.run.js:3 on_failed',
      'enqueue-unchecked modules/alpha/handlers/alpha.run.js:5 kein Objektliteral',
      'enqueue-unchecked modules/alpha/handlers/alpha.run.js:6 spread',
      'foreign-namespace modules/alpha/index.js:6 beta/fremd',
      'phantom-skill customer-modules/beta/index.js:1 beta/fremd',
      'phantom-skill modules/alpha/handlers/_lib/enqueue.js:3 fn-phantom',
      'phantom-skill modules/alpha/handlers/_lib/enqueue.js:7 alpha/ghost',
      'phantom-skill modules/alpha/handlers/alpha.run.js:3 opt-phantom',
      'phantom-skill modules/alpha/index.js:5 ui-phantom',
    ].sort());
    // beta/fremd im eigenen Namensraum ohne Paketdatei ist ein Phantom, kein Fremdzugriff
    assert.ok(!findings.some((f) => f.app === 'beta' && f.kind === 'foreign-namespace'));
    // Schlüssel ohne Zeilennummer
    assert.ok(findings.every((f) => f.key === [f.kind, f.app, f.file, f.detail].join('|')));
  } finally { fs.rmSync(repo, { recursive: true, force: true }); }
});

test('scanSkills: eigener Namensraum mit Paketdatei; beta ohne Datei ist Phantom', () => {
  const { repo, bos } = miniRepo();
  try {
    const findings = scanSkills(bos, { coreSkills: ['core-skill', 'doc'] });
    assert.ok(!findings.some((f) => f.detail === 'alpha/own'));
    assert.ok(findings.some((f) => f.app === 'beta' && f.kind === 'phantom-skill' && f.detail === 'beta/fremd'));
  } finally { fs.rmSync(repo, { recursive: true, force: true }); }
});

test('objectKeys: Kurzschreibweise, Zitate, Verschachtelung, Spread', () => {
  assert.deepEqual(objectKeys("{ a: 1, 'on_failed': x, on_complete, nested: { on_failed: 1 }, s: 'a, on_failed: b' }"), { keys: ['a', 'on_failed', 'on_complete', 'nested', 's'], spread: false });
  assert.equal(objectKeys('args'), null);
  assert.equal(objectKeys('{ ...rest }').spread, true);
});

test('judge: Ratchet neu/erledigt', () => {
  const f = [{ key: 'a' }, { key: 'b' }];
  assert.deepEqual(judge(f, new Set(['a', 'c'])), { fresh: [{ key: 'b' }], resolved: ['c'] });
});

test('CLI: neuer Befund → Exit 1, nach --write-baseline Exit 0, zusätzlicher Befund wieder Exit 1', () => {
  const { repo, bos } = miniRepo();
  const baseline = path.join(repo, 'baseline.json');
  try {
    const run = (...extra) => spawnSync(process.execPath, [CLI, '--root', bos, '--baseline', baseline, ...extra], { encoding: 'utf8' });
    const first = run();
    assert.equal(first.status, 1, first.stdout + first.stderr);
    assert.match(first.stderr, /NEU phantom-skill/);
    assert.match(first.stdout, /Befunde \(phantom-skill 5/);
    assert.equal(run('--write-baseline').status, 0);
    assert.equal(run().status, 0);
    fs.appendFileSync(path.join(bos, 'modules/alpha/index.js'), "\nsend({ skill: 'neues-phantom' });\n");
    const again = run();
    assert.equal(again.status, 1);
    assert.match(again.stderr, /neues-phantom/);
    assert.equal(run('--no-baseline').status, 1);
  } finally { fs.rmSync(repo, { recursive: true, force: true }); }
});
