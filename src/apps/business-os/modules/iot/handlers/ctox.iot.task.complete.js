// Origin: CTOX
// License: Apache-2.0
// ctox.iot.task.complete – on_complete-Rückruf der IoT-Alarm-Aufträge (Kern §16.30 Nr. 3), kein UI-Sender.
// Legt das Urteil des Skills iot/operations am Alarm ab und quittiert bzw. löst ihn (Logik in _agenttask.js).
import { callbackOf, completeAlarmTask } from './_agenttask.js';

export default async function handle(command, ctx) {
  return completeAlarmTask(ctx, callbackOf(command));
}
