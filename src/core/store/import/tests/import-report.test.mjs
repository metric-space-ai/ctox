import test from 'node:test';
import assert from 'node:assert/strict';
import { inventory, accounting } from '../../../../../scripts/sync-v4/import-report.mjs';

test('actual release drops legacy sync knobs and retains migrations and platform ownership', async () => {
  const root = process.env.IMPORT_INPUT_ROOT;
  assert.ok(root, 'IMPORT_INPUT_ROOT must identify the copied release inputs');
  const result = await inventory(`${root}/v4`,`${root}/ctox-platform.manifest.json`);
  assert.ok(result.collections.includes('app_drafts'));
  assert.ok(result.collections.includes('app_catalog'));
  assert.ok(result.collections.includes('mail_server_state'));
  assert.equal(result.migrations.communication_accounts.from_ver,'main@4794af03d');
  const remaining = JSON.stringify([result.manifests,result.schemas]);
  assert.doesNotMatch(remaining,/"(?:syncProfile|conflictStrategy)":/);
  assert.ok(result.removed_sync_settings.length, 'input proves removed fields existed');
  assert.equal(accounting({collections:{lead:{source_rows:2,target_documents:1}}}).lead.complete,false);
});
