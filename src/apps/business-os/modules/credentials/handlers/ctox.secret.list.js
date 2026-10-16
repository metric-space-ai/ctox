// Origin: CTOX
// License: Apache-2.0
// ctox.secret.list – ref: store.rs:17385-17407 → list_credentials_command :15190 @ 4794af03d
// FAEHIGKEIT/PLATTFORM: Metadaten des verschlüsselten Secret-Stores, niemals Werte. Rechte SecretsManage im Kern.
// CAP-3 #831: secret.list({scope?}) → {scope, secrets:[{name, description, created_at, updated_at}]} je Schlüsselbund.
// Die Antwortform der App bleibt {scope, catalog, extra} (index.js mergeEntries): catalog = Instanzkatalog
// (_secret.js) mit is_set aus dem Instanz-Schlüsselbund, extra = übrige Einträge beider Schlüsselbunde.
import { callCap } from './_base.js';
import { INSTANCE_CREDENTIALS, INSTANCE_SCOPE, isInstanceCredential } from './_secret.js';

const secretsOf = (result) => (Array.isArray(result?.secrets) ? result.secrets.filter((s) => s && typeof s.name === 'string') : []);

export default async function handle(_command, ctx) {
  const instance = secretsOf(await callCap(ctx, 'secret.list', { scope: INSTANCE_SCOPE }));
  const own = secretsOf(await callCap(ctx, 'secret.list', {}));
  const byName = new Map(instance.map((s) => [s.name, s]));
  const catalog = INSTANCE_CREDENTIALS.map(({ name, description }) => ({
    name,
    description,
    is_set: byName.has(name),
    updated_at: byName.get(name)?.updated_at ?? null,
  }));
  const extra = [...instance.filter((s) => !isInstanceCredential(s.name)), ...own]
    .map((s) => ({ name: s.name, description: s.description ?? '', is_set: true, updated_at: s.updated_at ?? null }));
  return { ok: true, scope: INSTANCE_SCOPE, catalog, extra };
}
