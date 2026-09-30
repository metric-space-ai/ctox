// A retained pull checkpoint must survive ordinary pulls and local writes
// after it was stored (the newest local row moves forward), but never a schema
// change or a local eviction. Exact equality re-pulled whole eager collections
// on every page load of a busy tenant (customer tenant, 30.09.2026: ~35 MB, 60-75 s).
import { replicationWebRtcTestInternals as internals } from '../src/replication-webrtc.mjs';

const { localCheckpointValidityKey: key, localCheckpointStillCovers: covers } = internals;
let failures = 0;
function check(name, condition) {
  if (condition) console.log(`ok   ${name}`);
  else { failures += 1; console.log(`FAIL ${name}`); }
}

const schema = 'ac4e33e45b546e1220acd4e7cd899000117f02b2f4635145113d3f987e58e4be';
const at = (lwt, extra = {}) => key({
  epoch: `browser:outbound_lead_generation_leads:${lwt}:1d2bdf24e8740ebd`,
  schemaHash: schema,
  ...extra,
});
const stored = at(1790778099978);

check('identical key covers', covers(stored, stored));
check('newer local head (pull or write after persist) covers', covers(stored, at(1790778102996)));
check('fractional lwt keeps working', covers(at('1790778099978.5'), at(1790778099979)));
check('older local head (rows lost) does not cover', !covers(stored, at(1790778000000)));
check('schema change does not cover', !covers(stored, key({
  epoch: 'browser:outbound_lead_generation_leads:1790778102996:91f3a6ead8e0bffe',
  schemaHash: 'ffff',
})));
check('eviction after persist does not cover', !covers(stored, at(1790778102996, { evictionGeneration: 1 })));
check('same eviction generation covers', covers(at(1790778099978, { evictionGeneration: 2 }), at(1790778102996, { evictionGeneration: 2 })));
check('emptied local store does not cover', !covers(stored, key({ epoch: 'browser:outbound_lead_generation_leads:empty', schemaHash: schema })));
check('other collection does not cover', !covers(stored, key({ epoch: 'browser:business_chats:1790778102996:91f3a6ead8e0bffe', schemaHash: schema })));
check('non-browser keys keep exact equality', !covers('local-epoch-1|x', 'local-epoch-2|x') && covers('local-epoch-1|x', 'local-epoch-1|x'));
check('missing keys never cover', !covers('', stored) && !covers(stored, ''));
check('no eviction keeps the pre-existing key format', stored === `browser:outbound_lead_generation_leads:1790778099978:1d2bdf24e8740ebd|${schema}`);
const firstStore = '11111111-1111-4111-8111-111111111111';
const replacementStore = '22222222-2222-4222-8222-222222222222';
check('same persisted store identity resumes', covers(at(1000, { localStoreGeneration: firstStore }), at(2000, { localStoreGeneration: firstStore })));
check('recreated store with newer head cannot resume', !covers(at(1000, { localStoreGeneration: firstStore }), at(2000, { localStoreGeneration: replacementStore })));
check('legacy checkpoint requires a first generation-confirming pull', !covers(stored, at(1790778102996, { localStoreGeneration: firstStore })));
check('malformed generation is not a checkpoint', key({ epoch: 'browser:fixture:1000:abc', localStoreGeneration: 'bad' }) === '');

if (failures) {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}
console.log('checkpoint-local-cover smoke passed');
