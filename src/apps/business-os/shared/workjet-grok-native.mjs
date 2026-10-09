// Origin: CTOX
// License: AGPL-3.0-only
export const GROK_METHOD = 'ctox.workjet.grok.v1';
export const GROK_CAPABILITY = 'ctox-workjet-grok-v1';
const actions = new Set(['instance.grok.read','instance.grok.start','instance.grok.poll','instance.grok.cancel','instance.grok.check','instance.grok.remove']);
export async function requestWorkjetGrok(sync, request) {
  if (!sync || typeof sync.requestNative !== 'function') throw new Error('Grok native control unavailable');
  if (!request || typeof request !== 'object' || Array.isArray(request) || !actions.has(request.action)) throw new Error('Invalid Grok request');
  const allowed = new Set(['action','operationId','loginId','modelId']);
  if (Object.keys(request).some(key => !allowed.has(key))) throw new Error('Invalid Grok fields');
  const operationId = request.operationId || crypto.randomUUID();
  const response = await sync.requestNative(GROK_METHOD, {...request,operationId,version:1}, {requiredCapability:GROK_CAPABILITY,timeoutMs:24000});
  if (!response || response.version !== 1 || response.action !== request.action || response.operationId !== operationId) throw new Error('Grok response correlation failed');
  return response;
}
