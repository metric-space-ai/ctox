// Origin: CTOX
// License: AGPL-3.0-only
export const GROK_METHOD = 'ctox.workjet.grok.v1';
export const GROK_CAPABILITY = 'ctox-workjet-grok-v1';
const actions = new Set(['instance.grok.read','instance.grok.start','instance.grok.poll','instance.grok.cancel','instance.grok.check','instance.grok.remove']);
export async function requestWorkjetGrok(sync, request, assertCurrent = () => {}) {
  if (!sync || typeof sync.requestNative !== 'function') throw new Error('Grok native control unavailable');
  if (!request || typeof request !== 'object' || Array.isArray(request) || !actions.has(request.action)
    || request.version !== 1 || typeof request.operationId !== 'string'
    || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(request.operationId)) throw new Error('Invalid Grok request');
  const allowed = new Set(['version','action','operationId','loginId','modelId']);
  if (Object.keys(request).some(key => !allowed.has(key))) throw new Error('Invalid Grok fields');
  assertCurrent();
  let response;
  try {
    response = await sync.requestNative(GROK_METHOD, request, {requiredCapability:GROK_CAPABILITY,timeoutMs:24000});
  } catch {
    throw new Error('Grok control unavailable. Check connection, Owner/Admin access and the installed CTOX version.');
  }
  assertCurrent();
  if (!response || response.version !== 1 || response.action !== request.action || response.operationId !== request.operationId) throw new Error('Grok response correlation failed');
  return response;
}
