// REGRESSION: runtime-installed Business OS app collections are not in the
// static Business OS schema-hash registry. Their browser hash must still match
// the native rxdb-rs hash, which is computed after RxDB default schema filling.

import { schemaHash, schemaHashSource } from '../dist/ctox-rxdb-js.mjs';

const runtimeModuleSchema = {
  additionalProperties: true,
  primaryKey: 'id',
  properties: {
    assignee: { type: 'string' },
    created_at_ms: { type: 'number' },
    deleted_at_ms: { type: 'number' },
    due_date: { type: 'string' },
    due_date_ms: { type: 'number' },
    equipment_name: { type: 'string' },
    id: { maxLength: 180, type: 'string' },
    is_deleted: { type: 'boolean' },
    is_returned: { type: 'boolean' },
    notes: { type: 'string' },
    overdue_notice_at_ms: { type: 'number' },
    status: { type: 'string' },
    updated_at_ms: { type: 'number' },
  },
  required: [
    'id',
    'equipment_name',
    'assignee',
    'due_date',
    'status',
    'is_deleted',
    'is_returned',
    'created_at_ms',
    'updated_at_ms',
  ],
  type: 'object',
  version: 1,
};

const collection = 'proof_office_checkout_20260624_001_items';
const expectedNativeHash = 'f4a83b2945c457f886c70d27b47c18921ced30fff141dd4c993ded5c7364a059';
const actual = await schemaHash(runtimeModuleSchema, collection);
if (actual !== expectedNativeHash) {
  throw new Error(`runtime module schema hash mismatch: expected ${expectedNativeHash}, got ${actual}`);
}
if (schemaHashSource(collection) !== 'canonical-json-schema-sha256-v1') {
  throw new Error('runtime module schema hash source must remain the custom canonical source label');
}

console.log('ctox-rxdb runtime module schema hash smoke OK');
