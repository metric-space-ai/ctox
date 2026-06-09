// Heritage marker, NOT a dependency: CTOX DB is a hard fork derived from
// RxDB 16.20.0 concepts (see docs/ctox-rxdb.md). The runtime imports no
// upstream rxdb code — do not "update" this version or add npm rxdb.
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
  if (collection === 'desktop_file_chunks') return 2;
  return collection.includes('attachment') || collection.includes('chunk') ? 4 : 10;
}

export function nativeRxdbPeerReady(config, db) {
  return config?.transport === SYNC_TRANSPORT
    && typeof config?.sync_room === 'string'
    && config.sync_room.length > 0
    && Array.isArray(config?.signaling_urls)
    && config.signaling_urls.some((url) => typeof url === 'string' && url.trim().length > 0)
    && db?.mode === 'rxdb';
}
