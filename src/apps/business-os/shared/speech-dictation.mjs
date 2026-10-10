// Origin: CTOX
// License: AGPL-3.0-only
export const DICTATION_METHOD = 'ctox.workjet.speech.dictation.v1';
export const DICTATION_CAPABILITY = 'ctox-workjet-speech-dictation-v1';
const uuid = value => typeof value === 'string'
  && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value);
const integer = value => Number.isSafeInteger(value) && value >= 0;
const states = new Set(['open', 'finishing', 'finished', 'canceled', 'failed']);
const errors = new Set(['missing_credential', 'configuration_unavailable', 'backend_unavailable',
  'timeout', 'credentials_rejected', 'access_denied', 'rate_limit', 'quota', 'provider_rejected',
  'backpressure', 'invalid_response', 'transport', 'retired', 'canceled',
  'invalid_sequence_or_audio', 'invalid_finish', 'missing_final']);
function validate(request) {
  if (!request || Array.isArray(request) || request.action !== 'speech.dictation' || !uuid(request.commandId)) {
    throw new TypeError('Invalid dictation request.');
  }
  const fields = ['action', 'commandId', 'op'];
  let valid = false;
  if (request.op === 'open') valid = true;
  else if (['write', 'read', 'finish', 'cancel'].includes(request.op) && uuid(request.streamId)) {
    fields.push('streamId');
    if (request.op === 'write') {
      fields.push('sequence', 'pcmBase64');
      valid = integer(request.sequence) && request.sequence > 0
        && typeof request.pcmBase64 === 'string' && request.pcmBase64.length >= 4
        && request.pcmBase64.length <= 4268 && /^[A-Za-z0-9+/]+={0,2}$/.test(request.pcmBase64);
    } else if (request.op === 'read') {
      fields.push('afterSequence'); valid = integer(request.afterSequence);
    } else valid = true;
  }
  if (!valid || Object.keys(request).some(key => !fields.includes(key))) throw new TypeError('Invalid dictation request.');
}
function result(response, request) {
  const allowed = ['action', 'commandId', 'op', 'streamId', 'state', 'events', 'text', 'error'];
  if (!response || Array.isArray(response) || Object.keys(response).some(key => !allowed.includes(key))
    || response.action !== request.action || response.commandId !== request.commandId || response.op !== request.op
    || !uuid(response.streamId) || request.op !== 'open' && response.streamId !== request.streamId
    || !states.has(response.state) || !Array.isArray(response.events) || response.events.length > 1
    || response.events.some(event => !event || Object.keys(event).some(key => !['sequence', 'text'].includes(key))
      || !integer(event.sequence) || event.sequence < 1 || typeof event.text !== 'string' || event.text.length > 8192)
    || !(response.error === null || errors.has(response.error))
    || !(response.text === null || typeof response.text === 'string' && response.text.length <= 32768)
    || response.state === 'finished' && (typeof response.text !== 'string' || !response.text.trim() || response.error !== null)
    || response.state !== 'finished' && response.text !== null
    || response.state === 'failed' && response.error === null
    || ['failed', 'canceled'].includes(response.state) && response.events.length > 0) {
    throw new Error('Invalid or mismatched dictation response.');
  }
  // Allowlist only. These are transient draft strings, never a meeting receipt.
  return Object.fromEntries(allowed.map(key => [key, response[key]]));
}
/** General draft-only consumer. The selected instance comes from the trusted host. */
export async function requestDictation(sync, nativeInstanceId, request, assertCurrent) {
  validate(request);
  if (typeof sync?.requestNative !== 'function' || typeof nativeInstanceId !== 'string' || !nativeInstanceId) {
    throw new Error('Dictation requires a connected CTOX instance. Open Speech settings.');
  }
  assertCurrent();
  const { action, ...operation } = request;
  const response = await sync.requestNative(DICTATION_METHOD, {
    ...operation, scope: { instanceId: nativeInstanceId },
  }, { requiredCapability: DICTATION_CAPABILITY, timeoutMs: request.op === 'open' ? 20_000 : 5_000 });
  assertCurrent();
  return result(response, request);
}
