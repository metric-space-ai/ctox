// The native checkpoint epoch hashes the collection head. A retained pull
// checkpoint must survive server writes after it was stored (the head moves
// forward) but never a storage-generation, collection or schema change, and
// never a head that moved behind the retained pull position (restore/rewrite).
// Customer tenant, 30.09.2026: every reload re-pulled ~53 MB of leads.
import { replicationWebRtcTestInternals as internals } from '../src/replication-webrtc.mjs';

const { remoteCheckpointStillCovers: covers } = internals;
let failures = 0;
function check(name, condition) {
  if (condition) console.log(`ok   ${name}`);
  else { failures += 1; console.log(`FAIL ${name}`); }
}
const key = (head, { gen = 'yzkddruyan', coll = 'outbound_lead_generation_leads', schema = 'ac4e' } = {}) => `${gen}|${coll}|${schema}|${head}`;
const retained = { validityKey: key('a82b'), pull: { id: 'lead_1', lwt: 1790808101592.85 } };
const remote = (latestLwt) => ({ checkpoint: { epoch: 'x', latestLwt } });

check('identical key covers', covers(retained, key('a82b'), remote(1)));
check('server wrote after persist (head moved forward) covers', covers(retained, key('adad'), remote(1790808200000)));
check('head behind retained pull position does not cover', !covers(retained, key('adad'), remote(1790808000000)));
check('storage generation change does not cover', !covers(retained, key('adad', { gen: 'other' }), remote(1790808200000)));
check('schema change does not cover', !covers(retained, key('adad', { schema: 'ffff' }), remote(1790808200000)));
check('missing remote head does not cover', !covers(retained, key('adad'), {}));
check('missing retained pull does not cover', !covers({ validityKey: key('a82b') }, key('adad'), remote(1790808200000)));
check('legacy three-part keys keep exact equality', !covers({ validityKey: 'e|s|h', pull: { lwt: 1 } }, 'e2|s|h', remote(5)));

if (failures) {
  console.error(`checkpoint-remote-head smoke: ${failures} failure(s)`);
  process.exit(1);
}
console.log('checkpoint-remote-head smoke OK');
