// node --test src/apps/business-os/shared/prompt-template.test.mjs  (Mutationsbeleg: v4/tools/agent-sim/mutation-proof.mjs)
import test from 'node:test';
import assert from 'node:assert/strict';
import { fillTemplate, loadPromptTemplate, promptPath, renderPrompt, templateFields } from './prompt-template.js';

test('fillTemplate: Felder, verschachtelt, Leerraum, Zahlen und Wahrheitswerte', () => {
  const out = fillTemplate('Thema {{topic}} / {{ source.label }} / {{n}} / {{on}}', { topic: 'KI', source: { label: 'X' }, n: 3, on: false });
  assert.equal(out, 'Thema KI / X / 3 / false');
});

test('fillTemplate: fehlender Platzhalter ist ein Fehler mit allen Namen, kein leerer Text', () => {
  assert.throws(() => fillTemplate('{{a}} {{b}} {{a}} {{c.d}}', { b: 'x', c: {} }), (err) => {
    assert.equal(err.code, 'invalid');
    assert.deepEqual(err.detail.missing, ['a', 'c.d']);
    return true;
  });
  assert.throws(() => fillTemplate('{{a}}', { a: null }), { code: 'invalid' });
  assert.throws(() => fillTemplate('{{toString}}', {}), { code: 'invalid' }, 'nur eigene Felder, keine Prototyp-Werte');
});

test('fillTemplate: Objekte/Arrays/NaN werden nicht stillschweigend eingesetzt', () => {
  assert.throws(() => fillTemplate('{{o}}', { o: { a: 1 } }), (err) => err.code === 'invalid' && err.detail.bad_type[0] === 'o');
  assert.throws(() => fillTemplate('{{o}}', { o: [1] }), { code: 'invalid' });
  assert.throws(() => fillTemplate('{{o}}', { o: Number.NaN }), { code: 'invalid' });
});

test('fillTemplate: kaputte Klammern sind Fehler, \\{{ ist wörtlich, leerer String ist ein Wert', () => {
  assert.throws(() => fillTemplate('a {{ b', { b: 1 }), /Zeile 1/);
  assert.throws(() => fillTemplate('x\n{{1abc}}', {}), /Zeile 2/);
  assert.throws(() => fillTemplate('{{}}', {}), { code: 'invalid' });
  assert.equal(fillTemplate('JSON: \\{{"a": 1}} {{x}}', { x: 'ok' }), 'JSON: {{"a": 1}} ok');
  assert.equal(fillTemplate('[{{x}}]', { x: '' }), '[]');
  assert.equal(fillTemplate('ohne Platzhalter }} {', {}), 'ohne Platzhalter }} {');
  assert.deepEqual(templateFields('{{a}} {{b.c}} {{a}}'), ['a', 'b.c']);
});

test('promptPath: Name → prompts/<name>.md, Pfade nur unter prompts/, kein .. oder absolut', () => {
  assert.equal(promptPath('issue'), 'prompts/issue.md');
  assert.equal(promptPath('prompts/source-adapter.md'), 'prompts/source-adapter.md');
  assert.equal(promptPath('./prompts/a.md'), 'prompts/a.md');
  for (const bad of ['', '../x.md', 'prompts/../handlers/a.js', '/prompts/a.md', 'skills/a/SKILL.md', 'prompts/a.txt', 'prompts//a.md', 'prompts\\a.md']) {
    assert.throws(() => promptPath(bad), { code: 'invalid' }, bad);
  }
});

test('loadPromptTemplate: liest über ctx.asset (sync oder async), Leser injizierbar, ohne Leser unavailable', async () => {
  const seen = [];
  const ctx = { asset: (p) => { seen.push(p); return 'Hallo {{name}}'; } };
  assert.equal(await loadPromptTemplate(ctx, 'greet'), 'Hallo {{name}}');
  assert.deepEqual(seen, ['prompts/greet.md']);
  const asyncCtx = { asset: async () => 'A {{x}}' };
  assert.equal(await renderPrompt(asyncCtx, 'a', { x: 1 }), 'A 1');
  const injected = await renderPrompt({}, 'b', { x: 'y' }, { read: (p) => `${p}:{{x}}` });
  assert.equal(injected, 'prompts/b.md:y');
  await assert.rejects(loadPromptTemplate({}, 'a'), { code: 'unavailable' });
  await assert.rejects(loadPromptTemplate({ asset: () => undefined }, 'a'), { code: 'invalid' });
  await assert.rejects(renderPrompt(ctx, 'greet', {}), { code: 'invalid' });
});
