// Heritage marker, NOT a dependency: CTOX Sync Engine is a hard fork derived from
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
  // desktop_file_chunks docs serialize to ~22 KB each (16 KiB base64 data +
  // metadata). The native master caps each masterChangesSince answer at
  // 96 KiB anyway (checkpoint-correct truncation), so asking for more per
  // round-trip lets the server fill that cap: a 1.26 MB file drops from
  // ~52 pull round-trips (batchSize 2) to ~26. Safe because the browser
  // pull drain is truncation-aware (loops until an EMPTY answer — see
  // pullFromPeer in the rxdb runtime).
  if (collection === 'desktop_file_chunks') return 6;
  // Knowledge table documents embed row data at the root and in `payload`.
  // Even byte-bounded chunks can be hundreds of KiB, so a regular batch of 20
  // can exceed the framed WebRTC transfer ceiling and strand later documents.
  if (collection === 'knowledge_tables') return 1;
  if (collection.includes('attachment') || collection.includes('chunk')) return 8;
  // Regular business docs are small (≤ ~2 KB); 20 per round-trip halves the
  // initial catch-up round-trips without approaching frame limits (the
  // frame protocol chunks large answers transparently).
  return 20;
}

export function nativeRxdbPeerReady(config, db) {
  return config?.transport === SYNC_TRANSPORT
    && typeof config?.sync_room === 'string'
    && config.sync_room.length > 0
    && Array.isArray(config?.signaling_urls)
    && config.signaling_urls.some((url) => typeof url === 'string' && url.trim().length > 0)
    && db?.mode === 'rxdb';
}
