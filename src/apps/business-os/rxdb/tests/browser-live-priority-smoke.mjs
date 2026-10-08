import assert from 'node:assert/strict';

import { webrtcNativeTestInternals } from '../src/webrtc-native.mjs';
import { readFileSync } from 'node:fs';

const { classifySendPriority } = webrtcNativeTestInternals;

assert.equal(
  classifySendPriority({ id: 'live-1', method: 'ctox.browser.live.v1', params: [{}] }, '{}'),
  'high',
  'Browser frames and input must interleave with an in-flight bulk transfer',
);
const source = readFileSync(new URL('../src/webrtc-native.mjs', import.meta.url), 'utf8');
assert.match(
  source,
  /method === 'ctox\.browser\.live\.v1'[\s\S]*?sendImmediateControlFrame/,
  'Browser live RPCs must bypass the bulk send queue instead of treating enqueue as delivery',
);
assert.match(
  source,
  /method === CTOX_OUTBOUND_SELLIFY_LOOKUP_METHOD[\s\S]*?sendImmediateControlFrame/,
  'the typed Sellify read must bypass the bulk send queue on the proven primary channel',
);
assert.match(
  source,
  /method === 'ctox\.workjet\.jour_fixe\.speech\.v1'[\s\S]*?sendImmediateControlFrame/,
  'meeting PCM and partial reads must bypass the bulk queue on the authenticated primary channel',
);
assert.match(source, /async sendImmediateControlFrame\(/);
assert.match(source, /async requestAuxiliary\(/);
assert.match(
  source,
  /forceInitiatorPeers\.add\(peerId\)[\s\S]*?aux-channel-not-open-/,
  'an unavailable Browser live channel must actively take over reconnect initiation',
);
assert.doesNotMatch(
  source.match(/async requestAuxiliary[\s\S]*?async sendImmediateControlFrame/)?.[0] || '',
  /removeConnection/,
  'a Browser live timeout must never tear down the shared Business OS peer',
);
assert.match(
  source,
  /auxChannelRegistrations\.size > 0[\s\S]*?forceInitiatorPeers\.add/,
  'a failed shared peer with Browser live registered must reconnect from the browser side',
);

const replicationSource = readFileSync(new URL('../src/replication-webrtc.mjs', import.meta.url), 'utf8');
assert.match(replicationSource, /openAuxChannel\('', CTOX_BROWSER_LIVE_CHANNEL/);
assert.match(replicationSource, /requestAuxiliary\([\s\S]*?CTOX_BROWSER_LIVE_CHANNEL/);
assert.match(replicationSource, /CTOX_OUTBOUND_SELLIFY_LOOKUP_METHOD = 'ctox\.outbound\.sellify_lookup\.v1'/);
assert.doesNotMatch(
  replicationSource,
  /String\(method \|\| ''\) === CTOX_OUTBOUND_SELLIFY_LOOKUP_METHOD[\s\S]*?requestAuxiliary/,
  'Sellify must not use the production auxiliary channel that drops browser-to-native frames',
);
assert.match(replicationSource, /The production TURN path can report[\s\S]*?this\.peer\.request\(/);
assert.equal(
  classifySendPriority({ id: 'ordinary-1', method: 'app.ordinary', params: [{}] }, '{}'),
  'normal',
  'ordinary auxiliary work keeps the fair normal lane',
);

console.log('ctox-rxdb Browser live send priority smoke OK');
