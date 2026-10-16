// E26.5: observational, per-tab, bounded by command creation order.
// No command payloads, durable state or authority are kept in diagnostics.
export function createCommandTimings(changed) {
  const rows = new Map();
  const stages = ['queued', 'sent', 'acked', 'completed'];
  function merge(value, broadcast = false) {
    if (!value || typeof value.command_id !== 'string') return;
    let row = rows.get(value.command_id);
    if (!row) {
      if (!Number.isFinite(value.queued)) return; // late events cannot resurrect an evicted command
      row = { command_id: value.command_id, queued: value.queued, sent: null, acked: null, completed: null };
      rows.set(value.command_id, row);
      if (rows.size > 500) rows.delete(rows.keys().next().value);
    }
    let updated = !row.published;
    for (const stage of stages) {
      if (row[stage] == null && Number.isFinite(value[stage])) { row[stage] = value[stage]; updated = true; }
    }
    if (!updated) return;
    row.published = true;
    const { published, ...snapshot } = row;
    changed(Object.freeze(snapshot), broadcast, value);
  }
  return {
    mark(command_id, stage, at = Date.now()) {
      if (!stages.includes(stage)) throw new TypeError('Unknown command timing stage');
      merge({ command_id, [stage]: at }, true);
    },
    merge,
    snapshot: () => [...rows.values()].map(({ published, ...row }) => ({ ...row })),
  };
}
