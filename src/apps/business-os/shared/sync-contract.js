export const RXDB_VERSION = '16.20.0';
export const SYNC_TRANSPORT = 'webrtc';
export const SYNC_TOPIC_PREFIX = 'ctox-business-os';
export const RXDB_NATIVE_PEER_PENDING_REASON = 'src/core/rxdb is not a complete CTOX WebRTC replication peer yet';

export function collectionTopic(syncRoom, collection) {
  if (!syncRoom) throw new Error('Business OS sync requires sync_room');
  if (!collection) throw new Error('Business OS sync requires collection name');
  return `${syncRoom}:${collection}`;
}

export function batchSizeFor(collection) {
  return collection.includes('attachment') || collection.includes('chunk') ? 1 : 10;
}
