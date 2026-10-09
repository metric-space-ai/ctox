const HISTORY_COLLECTIONS = ['ctox_runs', 'ctox_harness_events'];
let observerSequence = 0;

// A native hint only asks for a bounded, authoritative read. It never writes
// a projection into the browser store or grants collection access.
export function subscribeTaskHistoryChanges({ sync, getSelection, onChange, onError = () => {},
  onUnavailable = ({ error }) => onError(error) }) {
  let active = true;
  let sequence = 0;
  const epoch = `${Date.now().toString(36)}:${++observerSequence}`;
  const cleanups = new Set();
  if (typeof sync?.leaseCollection !== 'function') return () => {};
  for (const name of HISTORY_COLLECTIONS) {
    let retired = false;
    let cleanup = () => {};
    const unavailable = (error) => {
      if (!active || retired) return;
      retired = true;
      cleanup();
      onUnavailable({ collection: name, error });
    };
    const report = (error) => {
      if (!active) return;
      if (error?.code === 'COLLECTION_READ_FORBIDDEN') unavailable(error);
      else {
        retired = true;
        cleanup();
        onError(error);
      }
    };
    const readable = () => {
      if (!active || retired) return false;
      // This is the same live role predicate used by leaseCollection. The
      // lease/native peer still enforce authority if it changes after this check.
      if (typeof sync.mayReadCollection !== 'function' || sync.mayReadCollection(name) === true) return true;
      const error = new Error(`${name} is not readable by this Business OS role.`);
      error.code = 'COLLECTION_READ_FORBIDDEN';
      unavailable(error);
      return false;
    };
    Promise.resolve().then(() => readable() && sync.leaseCollection(name, 'ctox-selected-task-history'))
      .then((lease) => {
        if (!lease) return;
        let released = false;
        const release = () => {
          if (released) return;
          released = true;
          void Promise.resolve().then(() => lease.release?.()).catch(report);
        };
        cleanup = release;
        if (!readable()) { release(); return; }
        let master;
        let replacement;
        cleanup = () => {
          const subscriptions = [master, replacement];
          master = replacement = null;
          cleanups.delete(cleanup);
          release();
          for (const subscription of subscriptions) {
            try { subscription?.unsubscribe?.(); } catch (error) { report(error); }
          }
        };
        const bind = (bridge) => {
          master?.unsubscribe?.(); master = null;
          if (!readable() || bridge?.state?.collection?.name !== name) return;
          master = bridge.state.masterChange$?.subscribe?.((hint) => {
            try {
              if (!readable()) return;
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
            } catch (error) { report(error); }
          });
        };
        cleanups.add(cleanup);
        if (typeof lease.subscribeBridge === 'function') replacement = lease.subscribeBridge(bind);
        else bind(lease.bridge);
        // subscribeBridge may synchronously deliver its current bridge and retire
        // this lease before its subscription handle is assigned.
        if (!active || retired) cleanup();
      }).catch(report);
  }
  return () => {
    if (!active) return;
    active = false;
    for (const cleanup of [...cleanups]) cleanup();
    cleanups.clear();
  };
}
