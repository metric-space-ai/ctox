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

export function isDatabaseClosingError(error) {
  const message = String(error?.message || error || '');
  return /IDBDatabase.*closing|database connection is closing/i.test(message);
}

// The first paint may use an already replicated local layout. Reading it never
// seeds or patches the collection; native authority is reconciled separately.
export async function readLocalDesktopLayout({
  collection,
  defaultLayout,
  documentId = 'layout',
  onDatabaseClosing,
}) {
  if (!collection) return defaultLayout();
  try {
    const query = await collection.findOne(documentId);
    const document = await query.exec();
    return document?.toJSON?.() ?? document ?? defaultLayout();
  } catch (error) {
    if (!isDatabaseClosingError(error)) throw error;
    onDatabaseClosing?.(error);
    return defaultLayout();
  }
}

// One boundary owns restart fallback for every local stage. Native read
// rejection remains unknown authority inside the resolver and never seeds.
export async function ensureDesktopLayoutWithAuthority(options) {
  try {
    return await resolveDesktopLayout(options);
  } catch (error) {
    if (!isDatabaseClosingError(error)) throw error;
    options.onDatabaseClosing?.(error);
    return (options.unknownLayout || options.defaultLayout)();
  }
}

async function resolveDesktopLayout({
  collection,
  defaultLayout,
  unknownLayout = defaultLayout,
  readNativeDocument,
  insertMissingSeed,
  documentId = 'layout',
  now = Date.now,
  isCurrent = () => true,
}) {
  if (typeof readNativeDocument !== 'function') return unknownLayout();

  let authority;
  try {
    authority = await readNativeDocument();
  } catch {
    // Rejection is unknown state, not authoritative absence. Render locally and
    // leave the replicated document untouched.
    return unknownLayout();
  }
  if (!isCurrent()) return unknownLayout();
  if (authority) return authority?.toJSON?.() ?? authority;
  if (authority !== null) {
    // Undefined/false are unknown or malformed outcomes. Only the strict
    // reader normalized null represents confirmed native absence.
    return unknownLayout();
  }

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
