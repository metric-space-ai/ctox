// E45: migration declarations and parity cases come from the canonical v4 source.
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const ports = new Set(['communication_accounts', 'communication_messages', 'communication_threads', 'ctox_crew_learnings', 'accounting_dunning_runs']);
export async function buildNativeMigrations(documents, root) {
  const strategies = {};
  const corpus = [];
  for (const { raw } of documents) {
    for (const [name, definition] of Object.entries(raw.collections)) {
      const schema = definition.schema || definition;
      const chain = structuredClone(raw.migration_strategies?.[name] || {});
      const a9 = raw.migrations?.[name];
      if (a9?.from_ver) {
        const version = a9._schema_version;
        if (version && version.to !== schema.version) throw new Error(`Migration target differs from schema: ${name}`);
        if (a9.migrate_fn) {
          if (!ports.has(name) || !version) throw new Error(`No reviewed native migration port for ${name}`);
          const path = a9._file;
          if (typeof path !== 'string' || !path.startsWith('v4/schemas/migrations/') || path.includes('..')) throw new Error(`Invalid migration path: ${name}`);
          const url = new URL(path.slice('v4/schemas/'.length), root);
          const source = await readFile(url, 'utf8');
          const operation = { op: 'v4_function', collection: name, source_sha256: createHash('sha256').update(source).digest('hex') };
          chain[version.to] = { operations: [operation] };
          const { default: migrate } = await import(url.href);
          const inputs = name.startsWith('communication_')
            ? [{ [name === 'communication_accounts' ? 'account_key' : name === 'communication_messages' ? 'message_key' : 'thread_key']: 'native-key' }, { id: 'native-key', account_key: 'native-key', message_key: 'native-key', thread_key: 'native-key' }]
            : name === 'ctox_crew_learnings'
              ? [{ text: '  A\u00a0B\ufeffC\u0085D  ' }, { text: 'Other', normalized_text: 'owner value' }]
              : [{ invoices_total: 7 }, { invoices_total: ['invoice-1'] }, {}];
          for (const body of inputs) {
            const input = { ...(name.startsWith('communication_') ? {} : { id: 'retained-id' }), ...body, _rev: '3-history', _deleted: true, extra: { retained: 'unchanged' } };
            corpus.push({ collection: name, operation, input, expected: migrate(structuredClone(input)) });
          }
        } else if (Array.isArray(a9.steps) && a9.steps.length === 0 && version) {
          chain[version.to] = { operations: [] };
        } else if (schema.version > 0 && version) {
          throw new Error(`Unsupported native A9 migration for ${name}`);
        }
      }
      if (schema.version > 0) {
        for (let step = 1; step <= schema.version; step++) {
          if (!Object.hasOwn(chain, step)) throw new Error(`Missing native migration ${name}.${step}`);
        }
        strategies[name] = chain;
      }
    }
  }
  return { strategies: Object.fromEntries(Object.entries(strategies).sort(([a], [b]) => a.localeCompare(b))), corpus };
}
