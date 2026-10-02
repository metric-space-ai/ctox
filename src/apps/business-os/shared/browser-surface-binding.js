// Browser live.v1: durable tab_id and Runner active_tab_id are NOT interchangeable.
// These helpers validate evidence; they create no session, lease or authority.
function nonempty(value) {
  return typeof value === 'string' && value.trim().length > 0;
}

function currentRequest(request, surface) {
  return nonempty(request?.sessionId) && nonempty(request?.leaseId)
    && Number.isSafeInteger(request?.epoch) && request.epoch >= 0
    && request.sessionId === surface?.sessionId
    && request.leaseId === surface?.leaseId
    && request.epoch === surface?.epoch;
}

function nativeBinding(response) {
  if (response?.ok !== true) return null;
  const binding = response?.binding;
  if (!binding || !['session_id', 'tab_id', 'runtime_generation', 'active_tab_id']
    .every(key => nonempty(binding[key]))) return null;
  return Object.fromEntries(['session_id', 'tab_id', 'runtime_generation', 'active_tab_id']
    .map(key => [key, binding[key]]));
}

export function browserFrameBinding(response, request, surface) {
  const binding = nativeBinding(response);
  if (!currentRequest(request, surface) || !binding
    || binding.session_id !== request.sessionId
    || !nonempty(response?.screenshot?.base64)
    || response?.nav?.active_tab_id !== binding.active_tab_id) return null;
  if (surface.activeTabId && surface.activeTabId !== binding.active_tab_id) return null;
  if (surface.runtimeGeneration && surface.runtimeGeneration !== binding.runtime_generation) return null;
  if (surface.tabId && surface.tabId !== binding.tab_id) return null;
  return Object.freeze({ ...binding, epoch: request.epoch, leaseId: request.leaseId });
}

export function browserInputBinding(frameBinding, surface) {
  if (!frameBinding || !currentRequest({ sessionId: frameBinding.session_id,
    leaseId: frameBinding.leaseId, epoch: frameBinding.epoch }, surface)
    || !nonempty(frameBinding.runtime_generation) || !nonempty(frameBinding.active_tab_id)
    || !nonempty(frameBinding.tab_id)
    || (surface.runtimeGeneration && surface.runtimeGeneration !== frameBinding.runtime_generation)
    || (surface.tabId && surface.tabId !== frameBinding.tab_id)
    || (surface.activeTabId && surface.activeTabId !== frameBinding.active_tab_id)) return null;
  return { runtime_generation: frameBinding.runtime_generation,
    active_tab_id: frameBinding.active_tab_id };
}

export function browserInputAcknowledgement(response, events, frameBinding, request, surface) {
  const binding = nativeBinding(response);
  const ready = browserInputBinding(frameBinding, surface);
  const matches = currentRequest(request, surface) && ready && binding
    && ['session_id', 'tab_id', 'runtime_generation', 'active_tab_id']
      .every(key => binding[key] === frameBinding[key]);
  if (!matches || !Array.isArray(events) || events.length > 64
    || !Array.isArray(response?.results) || response.results.length > events.length) {
    return { acceptedSeqs: [], complete: false };
  }
  const acceptedSeqs = [];
  const resultsByIndex = new Map();
  for (const result of response.results) {
    const index = result?.index;
    // Missing/duplicate/out-of-range indices cannot prove which submitted
    // event was delivered. Never manufacture positional acknowledgements.
    if (!Number.isSafeInteger(index) || index < 0 || index >= events.length
      || resultsByIndex.has(index)) return { acceptedSeqs: [], complete: false };
    resultsByIndex.set(index, result);
  }
  let complete = response.results.length === events.length;
  for (let index = 0; index < events.length; index += 1) {
    const event = events[index];
    const valid = event?.session_id === binding.session_id && event?.tab_id === binding.tab_id
      && Number.isSafeInteger(event?.seq) && event.seq >= 0
      && resultsByIndex.get(index)?.ok === true;
    if (valid) acceptedSeqs.push(event.seq);
    else complete = false;
  }
  return { acceptedSeqs, complete };
}
