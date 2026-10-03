import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { runInNewContext } from 'node:vm';

const source = await readFile(new URL('../app.js', import.meta.url), 'utf8');
const bootstrap = source.match(/async function bootstrap\(\)[\s\S]*?\n\}/)?.[0];
const diagnostics = source.match(/async function reportPreservedLocalReplicas\(syncConfig\)[\s\S]*?\n\}/)?.[0];
assert.ok(bootstrap && diagnostics);
assert.doesNotMatch(bootstrap, /await\s+(?:traceShellPhase\([^\n]*report|reportPreservedLocalReplicas)/);
assert.match(bootstrap, /markBootTiming\('firstModuleMountedMs'\)[\s\S]*void reportPreservedLocalReplicas\(syncConfig\)/);
// Keep the existing data-preservation guards: changing scheduling is not
// permission to delete a primary, journal, or superseded generation.
assert.doesNotMatch(source, /\bresetBusinessDb\s*\(|\bindexedDB\.deleteDatabase\s*\(/);

const calls = [];
let releaseEnumeration;
const enumerate = new Promise(resolve => { releaseEnumeration = resolve; });
const run = runInNewContext(`${diagnostics}\nreportPreservedLocalReplicas`, {
  traceShellPhase: (_name, action) => action(),
  reportLegacySharedBusinessDb: async () => { calls.push('legacy'); await enumerate; },
  reportSupersededBusinessDbGenerations: async () => { calls.push('superseded'); },
  console: { warn: () => assert.fail('unexpected diagnostic error') },
});
const pending = run({ instance_id: 'fixture' });
assert.deepEqual(calls, ['legacy']);
releaseEnumeration();
await pending;
assert.deepEqual(calls, ['legacy', 'superseded']);
let recovered = false;
const warnings = [];
const failing = runInNewContext(`${diagnostics}\nreportPreservedLocalReplicas`, {
  traceShellPhase: (_name, action) => action(),
  reportLegacySharedBusinessDb: async () => { throw new Error('enumeration unavailable'); },
  reportSupersededBusinessDbGenerations: async () => { recovered = true; },
  console: { warn: message => warnings.push(message) },
});
await failing({});
assert.equal(recovered, true, 'one failing diagnostic cannot suppress the remaining inventory');
assert.equal(warnings.length, 1);
console.log('startup inventory scheduling/preservation regression PASS');
