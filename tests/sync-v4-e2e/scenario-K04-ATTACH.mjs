import { checkpointDelivered } from './criteria.mjs';
export default {
  id: 'K04-ATTACH', title: 'An ATTACH write behind the checkpoint is not lost',
  coverage: 'Controller-requested ATTACH/checkpoint mechanism, distinct from the complete K04 incident sequence.',
  steps: ['Complete initial sync and record the real replication checkpoint', 'ATTACH writer inserts with older lastWriteTime', 'Compare read-only native row with local browser row', 'Move the same row forward as a positive transport control'],
  criterion: { deliveryMs: 10000, missingRows: 0, positiveControlRequired: true },
  async run(h) {
    const page = await h.page(); await h.warm(page);
    const checkpoint = await h.checkpoint(page);
    if (!Number.isFinite(checkpoint?.latestLwt)) throw Error('No confirmed local checkpoint; fault precondition absent');
    const write = await h.inject('behind');
    if (write.lwt >= checkpoint.latestLwt) throw Error('Injected timestamp did not land behind checkpoint');
    const behindPull = await h.checkpointPull(page);
    const observation = await h.awaitLocal(page, write.id, 10000);
    const native = await h.truth();
    const forward = await h.inject('forward');
    const forwardPull = await h.checkpointPull(page);
    const control = await h.awaitLocal(page, forward.id, 10000, 'forward');
    const controlNative = (await h.truth()).rows.find(x => x.id === forward.id);
    control.nativeConfirmed = controlNative?.phase === 'forward' && controlNative?.revision === forward.revision && controlNative?.lwt === forward.lwt;
    return { measured: { checkpoint, injected: write, behindPull, forwardPull, observation, nativeRow: native.rows.find(x => x.id === write.id), positiveControl: control, controlNative,
      injectionScope: 'SQL ATTACH equivalent to old private writer; not a public CLI invocation' },
      pass: checkpointDelivered(observation, control, native.rows.find(x => x.id === write.id), write) };
  },
};
