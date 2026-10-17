/* One module graph per page: all database and sync consumers share this promise. */
export const RXDB_BUNDLE_URL = "../rxdb/dist/ctox-rxdb-js.mjs?v=20261017-shell-v2-v4-train16m";
let runtimePromise;
export function loadRxdbRuntime() {
  return runtimePromise ??= import(RXDB_BUNDLE_URL).catch((error) => {
    // Retry a transient import failure using the same canonical bundle URL.
    runtimePromise = undefined;
    throw error;
  });
}
