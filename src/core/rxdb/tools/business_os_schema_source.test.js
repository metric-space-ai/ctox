const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../../../..');
test('packaged creators retain foreign exports with the canonical owner schema', async () => {
  const { readPackagedModuleDefinitions } = await import('./business_os_schema_source.mjs');
  const canonical = JSON.parse(fs.readFileSync(path.join(root, 'src/core/business_os/business_os_schema_contract.json'), 'utf8'));
  const ctox = await readPackagedModuleDefinitions(root, 'ctox');
  const reports = await readPackagedModuleDefinitions(root, 'reports');
  const conversations = await readPackagedModuleDefinitions(root, 'conversations');
  assert.deepEqual(ctox.business_commands.schema, canonical.business_commands);
  assert.deepEqual(reports.communication_accounts.schema, canonical.communication_accounts);
  assert.deepEqual(ctox.communication_accounts.schema, canonical.communication_accounts);
  assert.equal(conversations.communication_accounts.schema.primaryKey, 'id');
  assert.deepEqual(conversations.outbound_messages.schema, canonical.outbound_messages);
});
