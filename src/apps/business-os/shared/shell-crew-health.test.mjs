import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const definition = source.slice(source.indexOf('function shellCtoxHealthProblem(status) {'), source.indexOf('async function loadSession()', source.indexOf('function shellCtoxHealthProblem(status) {')));
const problem = new Function('state', 'shellText', definition + '; return shellCtoxHealthProblem;')(
  { lang: 'de', advancedStatusEverHealthy: true }, key => key,
);
test('shell warns about missing model access even when the daemon is running', () => {
  const healthyProcess = { ok: true, source: 'rxdb', ctox_service: { running: true }, runtime_settings: { runtime: { provider: 'openai', chat_model: 'test-model' }, diagnostics: {} } };
  assert.equal(problem(healthyProcess), '');
  assert.match(problem({ ...healthyProcess, runtime_settings: { ...healthyProcess.runtime_settings, diagnostics: { auth_needs_attention: true } } }), /Modellzugang fehlt/);
  assert.match(problem({ ...healthyProcess, runtime_settings: { runtime: { provider: 'openai', chat_model: '' } } }), /kein Modell ausgewählt/);
});
