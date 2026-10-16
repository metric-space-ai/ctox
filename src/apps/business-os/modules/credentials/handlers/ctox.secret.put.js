// Origin: CTOX
// License: Apache-2.0
// ctox.secret.put – ref: store.rs:17408-17451 + put_credential_command :15353 @ 4794af03d
// Fachlogik (hier): Schlüsselprüfung. Fähigkeit (Kern): Wert kommt aus dem Secret-Intake des Befehls (bzw.
// Payload-`value`, das der Kern vor jeder Persistenz schwärzt – secret_command_safe_command); der Handler reicht
// den Wert NIE weiter und gibt ihn nie zurück. Leerwert-Prüfung und set_credential im Kern → ctx.cap.secret.put.
import { callCap, fail, payloadOf } from './_base.js';
import { credentialName, isValidCredentialKey, scopeArgs } from './_secret.js';

export default async function handle(command, ctx) {
  const name = credentialName(payloadOf(command));
  if (!isValidCredentialKey(name)) fail('invalid', 'invalid credential key: expected UPPER_SNAKE_CASE (A-Z, 0-9, _), max 64 chars');
  // CAP-3 #831: Instanzschlüssel (Katalog) in den Instanz-Schlüsselbund, eigene Schlüssel in den App-Scope.
  await callCap(ctx, 'secret.put', { command_id: command.id, name, value: { secret_intake: true }, ...scopeArgs(name) });
  return { ok: true, name };
}
