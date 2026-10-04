import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const source = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const helper = source.match(/function replaceModuleHash\(moduleId\) \{[\s\S]*?\n\}/)?.[0];

test('module navigation keeps the public URL despite the shell asset base', () => {
  assert.ok(helper, 'production navigation helper must exist');
  for (const entry of [
    'https://welsch.ctox.dev/#ctox',
    'https://welsch.ctox.dev/?lang=de#ctox',
    'https://example.test/business-os/?lang=en#threads',
  ]) {
    const calls = [];
    const assetBase = 'https://welsch.ctox.dev/business-os/_shell/0.1.46-beta.46/';
    const context = vm.createContext({
      URL,
      location: { href: entry },
      history: { replaceState: (_state, _title, url) => calls.push(new URL(url, assetBase).href) },
    });
    vm.runInContext(`${helper}\nreplaceModuleHash('desktop');`, context);
    const expected = new URL(entry);
    expected.hash = 'desktop';
    assert.deepEqual(calls, [expected.href]);
  }
});

test('module aliases and fallback routes use the same public navigation helper', () => {
  const openModule = source.slice(source.indexOf('async function openModule('), source.indexOf('\nfunction ', source.indexOf('async function openModule(')));
  assert.match(openModule, /replaceModuleHash\(requestedId\)/);
  assert.equal((openModule.match(/replaceModuleHash\(fallbackId\)/g) || []).length, 3);
  assert.doesNotMatch(openModule, /history\.replaceState\([^\n]*`#/);
});
