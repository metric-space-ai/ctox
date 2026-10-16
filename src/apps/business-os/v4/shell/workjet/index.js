// Shell v4 – Workjet-Brücke (E24): dieselben Globals wie die V2-Shell, Workjet-Schnittstelle unverändert.
//   globalThis.workjetProjectControl(request, diagnostics?)   → Promise<response>
//   globalThis.workjetComputerControl(request)                → Promise<response>  (.supportsOperationalDetails)
//   globalThis.workjetSessionControl(request)                 → Promise<response>
//   globalThis.workjetSessionEvents  {register, unregister, snapshot}
//   globalThis.workjetBusinessOsDeviceControl(request)        → Promise<response>
// workjetHostBridge / workjetBusinessOsPostMessage stellt der HOST (Workjet-Preload bzw. Mobil-App) bereit; die Shell
// konsumiert sie nur (mobile-host.js, Strang D).
//
// Aufbau: Aktionstabellen je Gruppe (actions/<gruppe>.js, je ein Strang) → Router je Brücke. Nicht freigeschaltete
// Aktionen werfen {code:'unsupported'} mit dem Merkmal, das Workjet je Brücke als „nicht unterstützt“ liest
// (v2-compat.js unsupportedError). Eine Brücke ohne eine einzige freigeschaltete Aktion wird NICHT als Global
// eingehängt: Workjet liest „Global fehlt“ überall als unsupported (Sitzung/Ereignisse kennen kein anderes Merkmal).
import { createV2Compat, unsupportedError } from './v2-compat.js';
import { boundedWorkjetProjectText } from './v2-helpers.js';
import { PROJECT_ACTIONS } from './actions/project.js';
// Strang B
import { SUPERVISOR_ACTIONS } from './actions/supervisor.js';
// Strang C
import { MEETING_ACTIONS } from './actions/meeting.js';
import { CALENDAR_CONNECT_BRIDGE_ACTIONS } from './actions/calendar-connect.js'; // Workjet #784 / Kern #875
// Strang D
import { INSTANCE_ACTIONS } from './actions/instance.js';
import { COMPUTER_ACTIONS } from './actions/computer.js';
import { SESSION_ACTIONS, createWorkjetSessionEvents } from './actions/session.js';
import { DEVICE_ACTIONS } from './actions/device.js';
import { installWorkjetHostBridge } from './host-bridge.js';
// Ende Strang D

// Gemessene Aktionsliste (~/.local/state/shell-v4/analysis/workjet-bridge-actions.md, Workjet eb65947d × V2 e35a116ae).
// Gruppe = Strang (PORT-PLAN.md). Die Liste ist die Abnahmeliste zum Cutover M7: jede Aktion muss freigeschaltet sein.
export const INVENTORY = Object.freeze({
  project: Object.freeze({
    A: ['project.list', 'project.create', 'project.init', 'project.configure', 'project.worker.add', 'project.chat.create',
      'project.kpis.read', 'project.kpis.configure', 'project.kpis.refresh',
      'project.exit_model.read', 'project.exit_model.refresh', 'project.exit_model.submit',
      'project.presentation.read', 'project.presentation.content.read', 'project.presentation.canvas.save'],
    B: ['project.supervisor.bind', 'project.supervisor.turn.submit', 'project.supervisor.turn.input',
      'project.supervisor.turn.cancel', 'project.supervisor.turn.watch', 'project.supervisor.turn.capabilities',
      'project.supervisor.route.read.v1', 'project.supervisor.route.read.v2',
      'project.supervisor.route.capabilities.v1', 'project.supervisor.route.capabilities.v2', 'ctox.plan.step.retry'],
    C: ['project.jour_fixe.meeting.read', 'project.jour_fixe.meeting.start', 'project.jour_fixe.meeting.end',
      'project.jour_fixe.todos.revise', 'project.jour_fixe.todos.confirm', 'project.jour_fixe.comment.add',
      'project.jour_fixe.transcript.append', 'project.jour_fixe.speech', 'project.jour_fixe.narration.read',
      'project.calendar.accounts.read', 'project.calendar.events.read',
      'speech.dictation', 'speech.settings.read', 'speech.settings.configure', 'speech.settings.key',
      'speech.settings.playback', 'speech.settings.voices', 'speech.settings.check', 'speech.settings.check.transcription'],
    D: ['instance.grok.read', 'instance.grok.start', 'instance.grok.poll', 'instance.grok.cancel', 'instance.grok.check',
      'instance.grok.remove',
      'instance.providers.read', 'instance.providers.observe', 'instance.providers.adopt', 'instance.providers.secret',
      'instance.providers.account.enable', 'instance.providers.account.remove', 'instance.providers.models.check',
      'instance.providers.models.select', 'instance.providers.models.exclude'],
    // Workjet #784 / Kern ctox#875 (nach der Messung von 16.10. dazugekommen): Google Kalender verbinden
    E: ['project.calendar.account.connect.start', 'project.calendar.account.connect.complete'],
  }),
  computer: Object.freeze({ D: ['computer.list', 'computer.assign', 'computer.unassign', 'computer.endpoint.upsert', 'computer.endpoint.disable', 'computer.ssh_key.ensure'] }),
  session: Object.freeze({ D: ['session.list', 'session.create', 'session.transfer.start', 'session.transfer.status', 'session.transfer.abort', 'session.transfer.pause_ack'] }),
  device: Object.freeze({ D: ['binding.list', 'binding.revoke', 'invite.create', 'invite.revoke'] }),
});
export const inventoryList = (bridge) => Object.values(INVENTORY[bridge] || {}).flat();

// Freigeschaltete Aktionen je Brücke. Stränge tragen hier je EINE Zeile pro Gruppendatei ein (siehe PORT-PLAN.md).
const ACTIONS = {
  project: {
    ...PROJECT_ACTIONS,
    ...SUPERVISOR_ACTIONS, // Strang B
    ...MEETING_ACTIONS, // Strang C
    ...CALENDAR_CONNECT_BRIDGE_ACTIONS, // Workjet #784 / Kern #875
    ...INSTANCE_ACTIONS, // Strang D
  },
  computer: { ...COMPUTER_ACTIONS }, // Strang D
  session: { ...SESSION_ACTIONS }, // Strang D
  device: { ...DEVICE_ACTIONS }, // Strang D
};

function assertRequest(request, bridge) {
  if (!request || typeof request !== 'object' || Array.isArray(request)) {
    const what = bridge === 'project' ? 'Workjet project control request must be an object.' : `Workjet ${bridge} control request must be an object.`;
    throw new TypeError(what);
  }
}

/**
 * Baut die Brückenfunktionen über einer V2-Umgebung `env` ({state, actorContext, newId}); `env` darf ein Getter-Objekt
 * sein (Orakel-Tests tauschen state während des Aufrufs).
 */
export function createWorkjetBridges(env, { actions = ACTIONS } = {}) {
  const route = (bridge) => async (request = {}, diagnostics = {}) => {
    assertRequest(request, bridge);
    const action = bridge === 'project'
      ? boundedWorkjetProjectText(request.action, 'action', 64)
      : String(request.action ?? '');
    const handler = Object.hasOwn(actions[bridge] || {}, action) ? actions[bridge][action] : null;
    if (!handler) throw unsupportedError(bridge, action);
    return handler(env, request, diagnostics);
  };
  const workjetProjectControl = route('project');
  const workjetComputerControl = route('computer');
  // V2 setzt das Merkmal immer (app.js:676); Workjet lässt includeOperationalDetails sonst weg.
  workjetComputerControl.supportsOperationalDetails = true;
  const workjetSessionControl = route('session');
  const workjetBusinessOsDeviceControl = route('device');
  const supported = (bridge) => Object.keys(actions[bridge] || {});
  return { workjetProjectControl, workjetComputerControl, workjetSessionControl, workjetBusinessOsDeviceControl, supported };
}

const GLOBALS = ['workjetProjectControl', 'workjetComputerControl', 'workjetSessionControl', 'workjetSessionEvents', 'workjetBusinessOsDeviceControl'];
const BRIDGE_OF = { workjetProjectControl: 'project', workjetComputerControl: 'computer', workjetSessionControl: 'session', workjetSessionEvents: 'session', workjetBusinessOsDeviceControl: 'device' };
let installed = null;

/**
 * Hängt die Globals für diese Shell ein (ein Aufruf je startShell). Ein neuer Aufruf (Shell-Tausch, neuer Kernel)
 * ersetzt den vorherigen und beendet dessen Umgebung, damit laufende V2-Zäune („session changed“) greifen.
 * @returns {{uninstall(): void, compat, bridges, installed: string[]}}
 */
export function installWorkjetBridges({ kernel, shellVer = 'dev', target = globalThis, appId = 'shell', actions = ACTIONS, shell = null, mobileCatalog = null, mobileCatalogChanges = null, doc = globalThis.document } = {}) { // Strang D: shell, mobileCatalog, doc (host-bridge.js)
  installed?.uninstall();
  const compat = createV2Compat({ kernel, shellVer, appId });
  const bridges = createWorkjetBridges(compat, { actions });
  const values = {
    workjetProjectControl: bridges.workjetProjectControl,
    workjetComputerControl: bridges.workjetComputerControl,
    workjetSessionControl: bridges.workjetSessionControl,
    // Ereignisstrom (Strang D): bis zur Portierung kein Global → Workjet registriert 0, kein Fehler.
    workjetSessionEvents: null,
    workjetBusinessOsDeviceControl: bridges.workjetBusinessOsDeviceControl,
  };
  // Strang D – Sitzung kennt bei Workjet nur „Global fehlt“ als „nicht unterstützt“: Sitzungssteuerung und
  // Ereignisstrom hängen nur ein, wenn ALLE Sitzungsaktionen der Inventarliste freigeschaltet sind.
  const sessionComplete = inventoryList('session').every((a) => bridges.supported('session').includes(a));
  if (!sessionComplete) values.workjetSessionControl = null;
  else values.workjetSessionEvents = createWorkjetSessionEvents(Object.create(compat, { hostBridge: { get: () => target.workjetHostBridge } }));
  // Ende Strang D
  const names = [];
  for (const name of GLOBALS) {
    const value = values[name];
    if (!value || bridges.supported(BRIDGE_OF[name]).length === 0) continue;
    target[name] = value;
    names.push(name);
  }
  // Strang D – Hostbrücke für den Workjet-Mobil-Host (nur mit <html data-workjet-mobile-host="true">).
  const hostBridge = installWorkjetHostBridge({ target, doc, shell, catalog: mobileCatalog, catalogChanges: mobileCatalogChanges });
  // Ende Strang D
  const handle = {
    compat,
    bridges,
    installed: names,
    hostBridge, // Strang D
    uninstall() {
      for (const name of names) if (target[name] === values[name]) delete target[name];
      values.workjetSessionEvents?.unregister?.(); // Strang D
      hostBridge.uninstall(); // Strang D
      compat.dispose();
      if (installed === handle) installed = null;
    },
  };
  installed = handle;
  return handle;
}
