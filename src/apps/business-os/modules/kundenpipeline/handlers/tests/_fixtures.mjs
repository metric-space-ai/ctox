// Gemeinsame Testdaten fuer die Decision-Hub-Handler.
import { createCtx, loadSchema } from './_ctx.mjs';

export const NOW = 1_760_000_000_000;
export const schemaUrl = new URL('../../collections.schema.json', import.meta.url);

export function vorgang(overrides = {}) {
  return {
    id: 'kpl-v-1',
    title: 'Bitte Export anpassen',
    status: 'zugeordnet',
    kunde_id: 'proj-acme',
    kunde_name: 'ACME GmbH',
    quelle_json: {
      kanal: 'mail',
      absender: 'kunde@acme.example',
      betreff: 'Export anpassen',
      body_clean: 'Bitte die CSV-Spalten umsortieren.',
    },
    // Rust-Ingest (project_inbound_messages) schreibt triage_json/run_json = null; das
    // Schema erlaubt nur object. Die Attrappe prueft Typen, daher fehlen sie hier
    // (Befund siehe V4-HANDLERS.md und schema-befund.test.mjs).
    mails_json: [],
    audit_json: [],
    is_deleted: false,
    created_at_ms: NOW - 1000,
    updated_at_ms: NOW - 1000,
    ...overrides,
  };
}

export async function setup({ seed = {}, caps = {}, now = NOW } = {}) {
  const schema = await loadSchema(schemaUrl);
  return createCtx({
    seed,
    now,
    schema,
    // CAP-3b #844 Ergebnisform {users} (Attrappe nach Vertrag).
    caps: { 'identity.users.list': async () => ({ users: [{ user_id: 'user_owner', display_name: 'Owner', role: 'chef', active: true }, { user_id: 'user_admin', display_name: 'Admin', role: 'admin', active: true }] }), ...caps },
  });
}
