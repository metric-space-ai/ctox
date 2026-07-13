import assert from 'node:assert/strict';
import { normalizeRecord, visibleRecords } from '../core/records.mjs';
import { buildSignatureCommand } from '../core/automation.mjs';
import { ARCHETYPE } from '../core/archetype.mjs';

const record = normalizeRecord({ id: 'one', title: 'One', notes: 'Visible' }, { nowMs: 1781990000000 });
assert.equal(record.id, 'one');
assert.deepEqual(visibleRecords([record], 'vis', 'open'), [record]);
const command = buildSignatureCommand(record, ARCHETYPE);
assert.equal(command.command_type, 'business_os.chat.task');
assert.equal(command.payload.record_snapshot.id, 'one');
