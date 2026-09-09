// Real Workjet client against the native host; only fixture coordination uses stdio.
import assert from "node:assert/strict";
import { generateKeyPairSync } from "node:crypto";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";
import { exerciseHandoff } from "./workjet_handoff_ipc.mjs";
const [clientPath, endpoint, encodedSpec, encodedHandoff] = process.argv.slice(2);
const { requestSyncAuthority } = await import(pathToFileURL(clientPath).href);
const spec = JSON.parse(encodedSpec);
await exerciseHandoff(requestSyncAuthority, endpoint, JSON.parse(encodedHandoff));
let sequence = 0;
const request = (operation, requestId = `node-client-${++sequence}`) =>
  requestSyncAuthority(endpoint, { version: 1, requestId, operation });
const hello = await request({ type: "hello" });
assert.equal(hello.result.type, "ready");
assert.equal(hello.result.nodeId, 1);
assert.equal(hello.result.scopeId, spec.scopeId);
// Use the actual Workjet schemas and framed Unix-socket client for current
// membership, not a copied codec or an in-process dispatch shortcut.
const { publicKey } = generateKeyPairSync("ed25519");
const worker = {
  nodeId: 4,
  identity: `ed25519:${Buffer.from(publicKey.export({ format: "jwk" }).x, "base64url").toString("hex")}`,
  dataReplica: true,
  revoked: false,
};
const membership = { type: "workerMembership", nodeId: worker.nodeId };
const currentMember = () => request(membership, "same-membership-read");
assert.deepEqual((await currentMember()).result, {
  type: "workerMembership", nodeId: worker.nodeId, worker: null,
});
const admission = { type: "admitWorker", worker };
assert.deepEqual((await request(admission, "worker-admission")).result, {
  type: "workerApplied", worker,
});
assert.deepEqual((await currentMember()).result, {
  type: "workerMembership", nodeId: worker.nodeId, worker,
});
const revokedWorker = { ...worker, revoked: true };
assert.deepEqual((await request({ type: "revokeWorker", nodeId: worker.nodeId })).result, {
  type: "workerApplied", worker: revokedWorker,
});
assert.deepEqual((await request(admission, "worker-admission")).result, {
  type: "workerReplayed", worker,
});
assert.deepEqual((await currentMember()).result, {
  type: "workerMembership", nodeId: worker.nodeId, worker: revokedWorker,
});
const created = await request({ type: "create", spec });
assert.equal(created.result.type, "applied");
const ownership = created.result.ownership;
const validate = { type: "validate", jobId: spec.jobId, ownership };
assert.equal((await request(validate)).result.type, "authorized");
const begin = { type: "beginEffect", jobId: spec.jobId, ownership, effectId: "fixture-effect" };
assert.equal((await request(begin, "same-command")).result.type, "applied");
assert.equal((await request(begin, "same-command")).result.type, "replayed");
const commands = createInterface({ input: process.stdin })[Symbol.asyncIterator]();
process.stdout.write("authorized\n");
assert.equal((await commands.next()).value, "partition");
assert.equal((await request(validate)).result.type, "unavailable");
assert.equal((await currentMember()).result.type, "unavailable");
process.stdout.write("revoked\n");
assert.equal((await commands.next()).value, "host-stopped");
await assert.rejects(request(validate));
await assert.rejects(currentMember());
process.stdout.write("disconnected\n");
process.stdin.destroy();
