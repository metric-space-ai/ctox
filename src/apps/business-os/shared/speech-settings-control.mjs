/** Transient typed speech control: no command projection, HTTP fallback or logging. */
export async function requestSpeechSettings(sync, request, assertCurrent) {
  if (typeof sync?.requestNative !== 'function') throw new Error('Speech settings require a connected CTOX instance.');
  const actions = new Set(['speech.settings.read', 'speech.settings.configure', 'speech.settings.key',
    'speech.settings.voices', 'speech.settings.check', 'speech.settings.check.transcription', 'speech.settings.playback']);
  if (!request || !actions.has(request.action) || typeof request.commandId !== 'string'
    || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(request.commandId)) {
    throw new Error('Invalid speech settings request.');
  }
  assertCurrent();
  let result;
  try {
    result = await sync.requestNative('ctox.workjet.speech.settings.v1', request, {
      requiredCapability: 'ctox-workjet-speech-settings-v1', timeoutMs: request.action === 'speech.settings.check.transcription' ? 30000 : 25000,
    });
  } catch (error) {
    // Native/provider exception bodies must never echo an input secret:
    // only fixed messages and codes leave this boundary.
    throw speechSettingsFailure(error);
  }
  assertCurrent();
  if (!result || result.action !== request.action || result.commandId !== request.commandId) {
    throw new Error('Speech settings response belongs to another request.');
  }
  return result;
}

function fixedError(message, code) {
  const error = new Error(message);
  error.code = code;
  return error;
}

/** Maps a native failure to a fixed, secret-free message and code Workjet can explain. */
export function speechSettingsFailure(error) {
  const code = typeof error?.code === 'string' ? error.code : '';
  const message = error instanceof Error ? error.message : '';
  if (code === 'CTOX_WEBRTC_CAPABILITY_MISSING') {
    return fixedError('Speech settings require a newer CTOX core.', 'SPEECH_CORE_OUTDATED');
  }
  if (code === 'PEER_UNAVAILABLE' || message === 'Native WebRTC peer is not connected') {
    return fixedError('Native WebRTC peer is not connected', 'PEER_UNAVAILABLE');
  }
  if (code === 'REQUEST_TIMEOUT' || /^Native request [a-zA-Z0-9._-]+ exceeded /u.test(message)) {
    return fixedError('Native request ctox.workjet.speech.settings.v1 exceeded its deadline.', 'REQUEST_TIMEOUT');
  }
  if (/Owner\/Admin/u.test(message)) {
    return fixedError('Speech settings require Owner/Admin.', 'SPEECH_ACCESS_DENIED');
  }
  return fixedError('Speech settings unavailable. Check connection, Owner/Admin access and the installed CTOX version.', 'SPEECH_SETTINGS_FAILED');
}
