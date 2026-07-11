import {
  decryptRecoveryArtifact,
  encryptRecoveryArtifact,
  sha256Json,
} from '../src/recovery-crypto.mjs';

const content = {
  schema: 'ctox.browser-recovery.v2',
  databaseName: 'ctox-test',
  instanceId: 'instance-a',
  pendingBatches: [{ batchId: 'batch-1', rows: [{ id: 'doc-1', value: 42 }] }],
};
const envelope = await encryptRecoveryArtifact(content, 'correct horse battery staple');
assert(envelope.schema === 'ctox.browser-recovery.crypto.v1', 'export must use the typed crypto envelope');
assert(envelope.kdf.iterations === 600_000, 'export must retain the hardened PBKDF2 cost');
assert(JSON.stringify(envelope).includes('correct horse') === false, 'passphrase must never enter the artifact');
assert(JSON.stringify(envelope).includes('doc-1') === false, 'recovery content must not remain plaintext');
assert(JSON.stringify(await decryptRecoveryArtifact(envelope, 'correct horse battery staple')) === JSON.stringify(content),
  'encrypted recovery content must round-trip');

await assertRejects(
  () => decryptRecoveryArtifact(envelope, 'incorrect passphrase'),
  'recovery_integrity_failed',
);
const tampered = structuredClone(envelope);
tampered.ciphertextBase64 = `${tampered.ciphertextBase64.slice(0, -2)}AA`;
await assertRejects(
  () => decryptRecoveryArtifact(tampered, 'correct horse battery staple'),
  'recovery_integrity_failed',
);
assert(
  await sha256Json({ b: 2, a: 1 }) === await sha256Json({ a: 1, b: 2 }),
  'integrity hashes must use canonical object ordering',
);

console.log('ctox-rxdb recovery crypto smoke OK');

async function assertRejects(run, code) {
  try {
    await run();
  } catch (error) {
    assert(error?.code === code, `expected ${code}, got ${error?.code}`);
    return;
  }
  throw new Error(`expected ${code} rejection`);
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
