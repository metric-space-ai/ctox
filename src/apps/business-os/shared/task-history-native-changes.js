const HISTORY_COLLECTIONS = ['ctox_runs', 'ctox_harness_events'];
let observerSequence = 0;

// A native hint only asks for a bounded, authoritative read. It never writes
// a projection into the browser store or grants collection access.
export function subscribeTaskHistoryChanges({ sync, getSelection, onChange, onError = () => {} }) {
  let active = true;
  let sequence = 0;
  const epoch = `${Date.now().toString(36)}:${++observerSequence}`;
  const cleanups = new Set();
  if (typeof sync?.leaseCollection !== 'function') return () => {};
  for (const name of HISTORY_COLLECTIONS) {
    Promise.resolve().then(() => active && sync.leaseCollection(name, 'ctox-selected-task-history'))
      .then((lease) => {
        if (!lease) return;
        const release = () => Promise.resolve().then(() => lease.release?.()).catch((error) => { if (active) onError(error); });
        if (!active) { void release(); return; }
        let master;
        const bind = (bridge) => {
          master?.unsubscribe?.(); master = null;
          if (!active || bridge?.state?.collection?.name !== name) return;
          master = bridge.state.masterChange$?.subscribe?.((hint) => {
            if (!active) return;
            const selection = getSelection();
            if (!selection?.taskId) return;
            const documents = hint?.documents ?? hint?.result?.documents;
            if (Array.isArray(documents)) {
              if (!documents.length) return;
              const rows = documents.map((row) => row?.documentData ?? row?.document ?? row);
              const identified = rows.filter((row) => row?.task_id || row?.command_id);
              if (identified.length && !identified.some((row) => row.task_id === selection.taskId
                || (selection.commandId && row.command_id === selection.commandId))) return;
            }
            onChange({ ...selection, revision: `ctox-task-history:${epoch}:${++sequence}` });
          });
        };
        let replacement;
        const cleanup = () => {
          master?.unsubscribe?.(); replacement?.unsubscribe?.();
          void release();
        };
        cleanups.add(cleanup);
        if (typeof lease.subscribeBridge === 'function') replacement = lease.subscribeBridge(bind);
        else bind(lease.bridge);
      }).catch((error) => { if (active) onError(error); });
  }
  return () => {
    if (!active) return;
    active = false;
    for (const cleanup of cleanups) cleanup();
    cleanups.clear();
  };
}
