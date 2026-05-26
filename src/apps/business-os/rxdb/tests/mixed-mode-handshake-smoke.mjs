import {
  CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
  CTOX_RXDB_PROTOCOL,
  V1_5_QUERY_FETCH_CAPABILITY,
  assertCompatibleProtocol,
  buildProtocolPayload,
  remoteSupportsQueryFetch,
} from '../dist/ctox-rxdb-js.mjs';

// V1.5 browser builds its own protocol payload exactly the way replicateWebRTC does.
const localBrowserPayload = buildProtocolPayload({
  collectionName: 'business_records',
  schemaVersion: 1,
  schemaHash: 'aaaa',
  schemaHashSource: 'rxdb-rs-schema-hash-v1',
  peerSessionId: 'browser:test',
  peerGeneration: 1,
  checkpoint: { source: 'browser', state: 'advertised', epoch: 'browser:test' },
  role: 'browser',
  capabilities: [
    'ctox-rxdb-browser-v1',
    'ctox-file-chunks-v1',
    'ctox-schema-hash-v1',
    'ctox-peer-session-v1',
    'ctox-checkpoint-epoch-v1',
    V1_5_QUERY_FETCH_CAPABILITY,
  ],
});

// A V1 native peer that knows nothing about V1.5.
const remoteV1Native = {
  protocol: CTOX_RXDB_PROTOCOL,
  capabilities: [
    'ctox-rxdb-native-v1',
    'ctox-file-chunks-v1',
    'ctox-replication-handshake-v1',
    'ctox-schema-hash-v1',
    'ctox-peer-session-v1',
    'ctox-checkpoint-epoch-v1',
  ],
  collection: { name: 'business_records', schemaVersion: 1, schemaHash: 'aaaa' },
  peerSession: { role: 'ctox_instance', sessionId: 'native:test' },
};

assertCompatibleProtocol(localBrowserPayload, remoteV1Native, {
  requiredCapabilities: CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
});

assert(!remoteSupportsQueryFetch(remoteV1Native), 'V1 native peer must report as non-V1.5');

// A V1.5 native peer answers the handshake — capability detection must light up.
const remoteV15Native = {
  ...remoteV1Native,
  capabilities: [...remoteV1Native.capabilities, V1_5_QUERY_FETCH_CAPABILITY],
};
assertCompatibleProtocol(localBrowserPayload, remoteV15Native, {
  requiredCapabilities: CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
});
assert(remoteSupportsQueryFetch(remoteV15Native), 'V1.5 native peer must light query-fetch capability');

console.log('ctox-rxdb-js mixed-mode handshake smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
