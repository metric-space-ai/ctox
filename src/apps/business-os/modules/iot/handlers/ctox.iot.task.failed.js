// Origin: CTOX
// License: Apache-2.0
// ctox.iot.task.failed – on_failed-Rückruf der IoT-Alarm-Aufträge (Kern §16.30 Nr. 3; Abbruch = status 'cancelled'),
// kein UI-Sender. Hält Fehlschlag/Abbruch am Alarm fest, Alarmstatus bleibt (Logik in _agenttask.js).
import { callbackOf, failAlarmTask } from './_agenttask.js';

export default async function handle(command, ctx) {
  return failAlarmTask(ctx, callbackOf(command));
}
