// Desktop layout defaults may be shown before authority arrives, but they must
// not be published over a replicated layout merely because a local RxDB query
// has not materialized yet. The strict native answer is the only seed gate.

function isConflictError(error) {
  const status = error?.status || error?.parameters?.writeError?.status;
  if (status === 409) return true;
  const code = String(error?.code || error?.rxdb || '').toUpperCase();
  if (code === 'CONFLICT') return true;
  const message = String(error?.message || error || '').toLowerCase();
  return message.includes('conflict') || message.includes('already');
}

export async function ensureDesktopLayoutWithAuthority({
  collection,
  defaultLayout,
  readNativeDocument,
  insertMissingSeed,
  documentId = 'layout',
  now = Date.now,
}) {
  if (typeof readNativeDocument !== 'function') return defaultLayout();

  let authority;
  try {
    authority = await readNativeDocument();
  } catch {
    // Rejection is unknown state, not authoritative absence. Render locally and
    // leave the replicated document untouched.
    return defaultLayout();
  }
  if (authority) return authority?.toJSON?.() ?? authority;

  // Native authority has confirmed absence. Insert once; if another writer wins
  // the race, adopt that row instead of patching defaults over it.
  const seed = {
    id: documentId,
    ...defaultLayout(),
    updated_at_ms: now(),
  };
  if (!collection) return seed;
  let conflict;
  try {
    await insertMissingSeed(collection, seed.id, seed);
  } catch (error) {
    if (!isConflictError(error)) throw error;
    conflict = error;
  }
  const query = await collection.findOne(seed.id);
  const winner = await query.exec();
  if (winner) return winner.toJSON?.() || winner;
  if (conflict) throw conflict;
  return seed;
}
