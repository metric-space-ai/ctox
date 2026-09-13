import { mount } from './index.js';
import { CREDENTIAL_REVEAL_METHOD } from '../../shared/native-request-privacy.mjs';

const login = Object.freeze({ username: 'crew@example.invalid', password: 'Synthetic-copy-fixture-42!' });
const records = Object.freeze({
  SYNTHETIC_LOGIN: JSON.stringify(login),
  SYNTHETIC_API_KEY: 'synthetic-api-key-not-a-provider-key',
});
const baseline = JSON.stringify(records);
let writes = 0, reads = 0, legacyReads = 0, deny = false;
const legacy = new URL(location.href).searchParams.get('legacy') === '1';
const evidence = document.querySelector('#evidence');
const dispose = await mount({
  host: document.querySelector('#app'), locale: 'en',
  session: { user: { id: 'synthetic-ui-fixture', role: 'chef' } },
  commandBus: { dispatch: async command => {
    if (command.command_type !== 'ctox.secret.list') { writes += 1; throw Error('fixture mutations forbidden'); }
    return { result: { catalog: [], extra: Object.keys(records).map(name => ({ name, is_set: true })) } };
  } },
  sync: { startCollection: async () => {},
    requestNative: async () => { legacyReads += 1; throw Error('legacy relay must never receive a reveal'); },
    ...(!legacy ? { requestPrivateNative: async (method, params) => {
    reads += 1;
    if (deny || method !== CREDENTIAL_REVEAL_METHOD || !Object.hasOwn(records, params.name)) throw Error('synthetic denial');
    return { schema: 'ctox.credential-reveal.v1', name: params.name, value: records[params.name] };
    } } : {}),
  },
});
evidence.textContent = 'Fixture ready. Stored values are synthetic; native authorization is tested separately.';
document.querySelector('#verify').addEventListener('click', () => {
  evidence.textContent = JSON.stringify(records) === baseline && writes === 0 && legacyReads === 0
    ? `PASS: stored fixture unchanged; zero mutations; ${reads} explicit private reads; zero legacy requests.` : 'FAIL: fixture changed or legacy relay called';
});
document.querySelector('#clipboard').addEventListener('click', async () => {
  try {
    const copied = await navigator.clipboard.readText();
    const label = copied === login.username ? 'username' : copied === login.password ? 'password'
      : copied === records.SYNTHETIC_API_KEY ? 'API key' : '';
    evidence.textContent = label ? `PASS: clipboard contains the exact synthetic ${label}.` : 'FAIL: clipboard does not match fixture';
  } catch { evidence.textContent = 'Clipboard read unavailable; not a passing clipboard proof.'; }
});
document.querySelector('#deny').addEventListener('click', () => { deny = true; evidence.textContent = 'Next native reveal will be denied.'; });
window.addEventListener('pagehide', dispose, { once: true });
