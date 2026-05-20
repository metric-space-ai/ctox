export const RXDB_VERSION = '16.20.0';
export const SYNC_TRANSPORT = 'webrtc';
export const SYNC_TOPIC_PREFIX = 'ctox-business-os';
export const RXDB_NATIVE_PEER_PENDING_REASON = 'CTOX native WebRTC peer is starting or unavailable';

export function collectionTopic(syncRoom, collection) {
  if (!syncRoom) throw new Error('Business OS sync requires sync_room');
  if (!collection) throw new Error('Business OS sync requires collection name');
  return `${syncRoom}:${collection}`;
}

export function batchSizeFor(collection) {
  return collection.includes('attachment') || collection.includes('chunk') ? 1 : 10;
}

export function nativeRxdbPeerReady(config, db) {
  return config?.transport === SYNC_TRANSPORT
    && config?.native_rxdb_peer_available === true
    && db?.mode === 'rxdb';
}
