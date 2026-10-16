import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd } from './_ctx.mjs';
import handle from '../outbound.research_source.auth_assist.js';

test('auth assist requests browser session with credential ref', async () => {
  const requests = [];
  const t = createTestCtx({ caps: {
    'secrets.exists': async () => ({ exists: false }),
    'browser.auth_assist.request': async (args) => { requests.push(args); return { request_id: 'ar1' }; },
  }, data: { outbound_research_adapters: { a1: { id: 'a1', source_id: 'dnb', requires_credential: true, credential_secret_name: 'dnb-login' } } } });
  const result = await handle(cmd('outbound.research_source.auth_assist', { adapter_id: 'a1', source_id: 'dnb', requires_credential: true, credential_secret_name: 'dnb-login', url: 'https://login.dnb.com' }, { id: 'cmd-a' }), t.ctx);
  assert.deepEqual(result.auth_assist, { request_id: 'ar1' });
  assert.equal(requests[0].credential_ref, 'ctox-secret://credentials/dnb-login');
  assert.equal(requests[0].requesting_task_id, 'cmd-a');
  assert.equal(requests[0].target_url, 'https://login.dnb.com');
  // CAP-3b #844: keine Felder außerhalb von AuthAssistRequest (target_url = Alias von target_url_override).
  assert.deepEqual(Object.keys(requests[0]).filter((k) => !['source_id', 'target_url', 'credential_ref', 'login_hint',
    'preferred_auth_assist', 'requesting_task_id', 'source_module', 'command_path'].includes(k)), []);
  const doc = t.doc('outbound_research_adapters', 'a1');
  assert.equal(doc.status, 'auth_requested');
  assert.equal(doc.auth_status, 'browser_session_requested');
  assert.equal(doc.payload.credential_ref.status, 'auth_requested');
});
