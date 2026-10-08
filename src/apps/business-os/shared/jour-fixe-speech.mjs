// Origin: CTOX
// License: AGPL-3.0-only
import { JOUR_FIXE_SPEECH_CAPABILITY, JOUR_FIXE_SPEECH_METHOD } from './jour-fixe-speech-contract.mjs';

/** Shared browser/desktop guest ingress; no Owner transcript append or HTTP. */
export async function requestJourFixeSpeech(sync, nativeInstanceId, request) {
  if (!sync?.requestNative || !nativeInstanceId) throw new Error('Speech transport unavailable.');
  const allowed = new Set(['action', 'projectId', 'meetingId', 'deckRevision', 'op', 'requestId', 'streamId', 'sequence', 'pcmBase64', 'afterSequence']);
  if (!request || Object.keys(request).some(key => !allowed.has(key))
    || request.action !== 'project.jour_fixe.speech') throw new TypeError('Invalid speech request.');
  const { action, projectId, meetingId, deckRevision, ...operation } = request;
  if (![projectId, meetingId].every(v => typeof v === 'string' && v.trim() === v && v.length > 0 && v.length <= 256)
    || !Number.isSafeInteger(deckRevision) || deckRevision < 1) throw new TypeError('Invalid speech scope.');
  const response = await sync.requestNative(JOUR_FIXE_SPEECH_METHOD, {
    ...operation, scope: { instanceId: nativeInstanceId, projectId, meetingId, deckRevision },
  }, { requiredCapability: JOUR_FIXE_SPEECH_CAPABILITY, timeoutMs: operation.op === 'open' ? 20_000 : 5_000 });
  return { action, projectId, meetingId, deckRevision, op: operation.op, ...response };
}
