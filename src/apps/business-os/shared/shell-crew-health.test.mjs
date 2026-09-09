import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { subscriptionModelUnavailable } from './model-access-health.js';

const source = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const definition = source.slice(source.indexOf('function shellCtoxHealthProblem(status) {'), source.indexOf('async function loadSession()', source.indexOf('function shellCtoxHealthProblem(status) {')));
const problem = new Function('state', 'shellText', 'subscriptionModelUnavailable', definition + '; return shellCtoxHealthProblem;')(
  { lang: 'de', advancedStatusEverHealthy: true }, key => key, subscriptionModelUnavailable,
);
test('shell warns about missing model access even when the daemon is running', () => {
  const healthyProcess = { ok: true, source: 'rxdb', ctox_service: { running: true }, runtime_settings: { runtime: { provider: 'openai', chat_model: 'test-model' }, diagnostics: {} } };
  assert.equal(problem(healthyProcess), '');
  assert.match(problem({ ...healthyProcess, runtime_settings: { ...healthyProcess.runtime_settings, diagnostics: { auth_needs_attention: true } } }), /Modellzugang fehlt/);
  assert.match(problem({ ...healthyProcess, runtime_settings: { runtime: { provider: 'openai', chat_model: '' } } }), /kein Modell ausgewählt/);
  const selected = { runtime: { provider: 'openai', chat_model: 'unsupported-model', available_models_by_provider: { openai: [{ id: 'offered-model' }] } }, auth: { mode: 'subscription' } };
  assert.match(problem({ ...healthyProcess, runtime_settings: selected }), /für diesen Zugang nicht angeboten/);
  selected.runtime.chat_model = 'offered-model';
  assert.equal(problem({ ...healthyProcess, runtime_settings: selected }), '');
  selected.runtime.chat_model = 'custom-api-model';
  selected.auth.mode = 'api_key';
  assert.equal(subscriptionModelUnavailable(selected), false, 'subscription catalog does not restrict API-key models');
  selected.auth.mode = 'subscription';
  selected.runtime.available_models_by_provider = {};
  assert.equal(subscriptionModelUnavailable(selected), false, 'missing catalog is unknown rather than a rejected model');
});
