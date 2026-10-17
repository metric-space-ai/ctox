// Origin: CTOX
// License: Apache-2.0
// ctox.iot.alarm.update – ref: src/core/iot/commands.rs:1027-1087 alarm_update, alarms.rs:60-75 can_transition_to,
// :267-289 persist_update, :317-330 get_in_realm, :381-411 update_status_in_realm/assign_in_realm
import { fail } from './_base.js';
import { C, alarmRow, canTransition, compact, outcome, parsePayload, parseStatus, sessionRealm, statusWire } from './_iotcore.js';

export default async function handle(command, ctx) {
  const realm = sessionRealm(command);
  const req = parsePayload(command, { alarm_id: 'string', action: 'string', assignee: 'string?', status: 'string?' });
  const action = req.action.toLowerCase();
  let next = null;
  if (action === 'ack' || action === 'acknowledge') next = 'Acknowledged';
  else if (action === 'progress') next = 'InProgress';
  else if (action === 'resolve') next = 'Resolved';
  else if (action === 'close') next = 'Closed';
  else if (action === 'status') {
    if (req.status === null) fail('invalid', 'status action requires `status`');
    next = parseStatus(req.status);
  } else if (action !== 'assign') fail('invalid', `unknown alarm action: ${action}`);
  const row = await ctx.get(C.alarms, req.alarm_id);
  if (!row || row.realm !== realm || !row.data) fail('not_found', `Alarm does not exist: ${req.alarm_id}`);
  const alarm = structuredClone(row.data);
  if (next) {
    if (!canTransition(alarm.status, next)) {
      fail('conflict', `illegal alarm status transition ${statusWire(alarm.status)} -> ${statusWire(next)}`);
    }
    alarm.status = next;
  } else {
    alarm.assignee_id = req.assignee; // null = Zuweisung aufheben
  }
  alarm.last_modified = ctx.now();
  await ctx.put(C.alarms, compact({ ...alarmRow(alarm), id: alarm.id, created_at_ms: row.created_at_ms ?? alarm.created, updated_at_ms: ctx.now() }));
  return outcome({ alarm }, [{ collection: C.alarms, id: alarm.id }]);
}
