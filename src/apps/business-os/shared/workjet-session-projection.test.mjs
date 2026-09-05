import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const appSource = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
function functionSource(name) {
  const match = appSource.indexOf(`function ${name}(`);
  assert.ok(match >= 0, `${name} exists`);
  const start = appSource.lastIndexOf('\n', match) + 1;
  const end = appSource.indexOf('\n}\n', match) + 3;
  assert.ok(end > match, `${name} has a complete top-level body`);
  return appSource.slice(start, end);
}
const runtimeSource = [
  'waitForProjectedWorkjetSession', 'boundedWorkjetSessionResult',
  'boundedWorkjetSessionText', 'boundedOptionalWorkjetSessionText',
  'waitForSyncBridgeReady',
].map(functionSource).join('\n');

for (const sessionId of ['session-nested', null]) {
  test(`session projection pulls nested replication state before ${sessionId ? 'ID lookup' : 'working-copy lookup'}`, async () => {
    let pulled = false;
    const session = {
      id: 'session-nested', owner_user_id: 'owner-1', project_id: 'project-1',
      working_copy_id: 'copy-1', computer_id: 'computer-1',
      run_status: 'idle', fence_epoch: 0, updated_at_ms: 1,
    };
    const context = vm.createContext({
      state: { db: { collection: () => ({
        findOne: () => ({ exec: async () => pulled ? session : null }),
        find: () => ({ exec: async () => pulled ? [session] : [] }),
      }) } },
      window: { setTimeout }, setTimeout, clearTimeout, Date,
    });
    vm.runInContext(runtimeSource, context);
    context.bridge = { state: { async awaitInSync() { pulled = true; } } };
    context.sessionId = sessionId;
    const result = await vm.runInContext(
      "waitForProjectedWorkjetSession(sessionId, 'owner-1', bridge, 200, {projectId:'project-1',workingCopyId:'copy-1'})", context,
    );
    assert.equal(result.id, session.id);
    assert.equal(result.workingCopyId, session.working_copy_id);
    assert.equal(pulled, true);
  });
}
