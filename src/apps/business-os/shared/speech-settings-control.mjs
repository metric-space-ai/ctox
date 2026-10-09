/** Transient typed speech control: no command projection, HTTP fallback or logging. */
export async function requestSpeechSettings(sync, request, assertCurrent) {
  if (typeof sync?.requestNative !== 'function') throw new Error('Speech settings require a connected CTOX instance.');
  const actions = new Set(['speech.settings.read', 'speech.settings.configure', 'speech.settings.key',
    'speech.settings.voices', 'speech.settings.check', 'speech.settings.playback']);
  if (!request || !actions.has(request.action) || typeof request.commandId !== 'string'
    || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(request.commandId)) {
    throw new Error('Invalid speech settings request.');
  }
  assertCurrent();
  let result;
  try {
    result = await sync.requestNative('ctox.workjet.speech.settings.v1', request, {
      requiredCapability: 'ctox-workjet-speech-settings-v1', timeoutMs: 25000,
    });
  } catch {
    // Native/provider exception bodies must never echo an input secret.
    throw new Error('Speech settings unavailable. Check connection, Owner/Admin access and the installed CTOX version.');
  }
  assertCurrent();
  if (!result || result.action !== request.action || result.commandId !== request.commandId) {
    throw new Error('Speech settings response belongs to another request.');
  }
  return result;
}
