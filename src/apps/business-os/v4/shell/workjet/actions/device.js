// Shell v4 – Workjet-Brücke, Aktionsgruppe D: Gerät (globalThis.workjetBusinessOsDeviceControl). Eigentum Strang D.
// Rumpf wörtlich V2 app.js:7844-7877 (Pin e35a116ae); `state.` → `env.state.`, die Pfeilfunktion am Global wird zur
// benannten Funktion mit `env` (V2: `globalThis.workjetBusinessOsDeviceControl = async (request) => {…}`).
// requestNative (E25) = Kern-Befehl `ctox.workjet.device.v1` (v2-compat). Die Pairing-Übergabe
// (completeWorkjetPairingRedirect, app.js:7932-7964) ist kein Workjet-Aufruf und nicht Teil der Brücke.
//
// v4-Zusatz: fehlt dem Kern die Route (unknown_command → v2-compat {code:'unsupported'}), trägt der Fehler das
// Merkmal, das Workjet für Geräte als „nicht unterstützt“ liest: code `CTOX_WEBRTC_CAPABILITY_MISSING`
// (CtoxGuestManager.ts buildGuestDeviceControlExpression; dort wird nur `code` gelesen).
import { isUnsupported, unsupportedError } from '../v2-compat.js';

export const WORKJET_DEVICE_CAPABILITY_MISSING = 'CTOX_WEBRTC_CAPABILITY_MISSING';

// ref: app.js:7844-7877
const WORKJET_DEVICE_CONTROL_ACTIONS = new Set([
  'invite.create',
  'invite.revoke',
  'binding.list',
  'binding.revoke',
]);

async function workjetBusinessOsDeviceControl(env, request) {
  if (!request || typeof request !== 'object' || Array.isArray(request)) {
    throw new TypeError('Workjet device request must be an object.');
  }
  const action = String(request.action || '');
  if (!WORKJET_DEVICE_CONTROL_ACTIONS.has(action)) {
    throw new TypeError('Workjet device action is unsupported.');
  }
  const allowedKeys = action === 'invite.create'
    ? ['action', 'ttlSeconds', 'displayName']
    : action === 'invite.revoke'
      ? ['action', 'inviteId']
      : action === 'binding.revoke'
        ? ['action', 'bindingId']
        : ['action'];
  if (Object.keys(request).some((key) => !allowedKeys.includes(key))) {
    throw new TypeError('Workjet device request contains an unsupported field.');
  }
  if (typeof env.state.sync?.requestNative !== 'function') { // v4-allow: debt L-42
    const error = new Error('CTOX device control is unavailable.'); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
    error.code = 'ctox_webrtc_unavailable';
    throw error;
  }
  return env.state.sync.requestNative('ctox.workjet.device.v1', request, { // v4-allow: debt L-42
    requiredCapability: 'ctox-workjet-device-control-v1',
    timeoutMs: 10_000,
  });
}

// Kern-Lücke → Geräte-Merkmal „nicht unterstützt“ (siehe Kopf).
async function deviceControl(env, request) {
  try {
    return await workjetBusinessOsDeviceControl(env, request);
  } catch (error) {
    if (isUnsupported(error)) {
      const missing = unsupportedError('device', request?.action, error.detail || error.message);
      missing.code = WORKJET_DEVICE_CAPABILITY_MISSING;
      throw missing;
    }
    throw error;
  }
}

// Freigeschaltete Aktionen (Orakel: Differenz gegen den V2-Rumpf, tests/oracle.test.mjs Block Strang D).
export const DEVICE_ACTIONS = Object.freeze(Object.fromEntries([...WORKJET_DEVICE_CONTROL_ACTIONS].map((action) => [action, deviceControl])));
