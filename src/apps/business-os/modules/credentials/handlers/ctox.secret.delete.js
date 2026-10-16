// Origin: CTOX
// License: Apache-2.0
// ctox.secret.delete – ref: store.rs:17452-17485 + delete_credential_command :15368 @ 4794af03d
import { callCap, fail, payloadOf } from './_base.js';
import { credentialName, isValidCredentialKey, scopeArgs } from './_secret.js';

export default async function handle(command, ctx) {
  const name = credentialName(payloadOf(command));
  if (!isValidCredentialKey(name)) fail('invalid', 'invalid credential key');
  await callCap(ctx, 'secret.delete', { name, ...scopeArgs(name) });
  return { ok: true, name };
}
