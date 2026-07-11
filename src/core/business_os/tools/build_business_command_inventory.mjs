import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const toolDir = path.dirname(fileURLToPath(import.meta.url));
const businessOsDir = path.resolve(toolDir, '..');
const repoRoot = path.resolve(businessOsDir, '../../..');
const storePath = path.join(businessOsDir, 'store.rs');
const browserRoot = path.join(repoRoot, 'src/apps/business-os');
const outputPath = path.join(businessOsDir, 'business_command_inventory.json');
const source = fs.readFileSync(storePath, 'utf8');
const functionStart = source.indexOf('pub fn accept_rxdb_business_command_with_origin');
const matchStart = source.indexOf('match command.command_type.as_str()', functionStart);
const fallback = source.indexOf('        _ => {}', matchStart);
if (functionStart < 0 || matchStart < 0 || fallback < 0) {
  throw new Error('cannot locate authoritative Business OS command classifier');
}
const classifier = source.slice(matchStart, fallback);
const exactControlTypes = [...classifier.matchAll(/^\s*((?:"[^"]+"\s*(?:\|\s*)?)+)=>\s*\{/gm)]
  .flatMap((match) => [...match[1].matchAll(/"([^"]+)"/g)].map((value) => value[1]))
  .sort();
const dispatchPredicates = [...classifier.matchAll(/^\s*command_type\s+if\s+(.+?)\s*=>\s*\{/gm)]
  .map((match) => match[1].trim())
  .sort();
const queueRedirectPredicates = dispatchPredicates.filter((predicate) => predicate.includes('ctox.report.'));
const controlPredicates = dispatchPredicates.filter((predicate) => !queueRedirectPredicates.includes(predicate));
const browserLiteralTypes = [...new Set(
  walk(browserRoot)
    .filter((file) => /\.(?:js|mjs)$/.test(file))
    .filter((file) => !excluded(path.relative(browserRoot, file)))
    .flatMap((file) => {
      const text = fs.readFileSync(file, 'utf8');
      return [...text.matchAll(/(?:command_type|commandType|type)\s*:\s*['"]([a-z][a-z0-9_-]*(?:\.[a-z0-9_:-]+)+)['"]/g)]
        .map((match) => match[1]);
    }),
)].sort();

const inventory = {
  schema: 'ctox.business_os.command_type_inventory.v1',
  authoritative_router: 'src/core/business_os/store.rs::accept_rxdb_business_command_with_origin',
  classification: {
    control: 'an exact_control_type or control_predicate match',
    queue: 'all remaining command types fall through to record_command',
  },
  exact_control_types: exactControlTypes,
  control_predicates: controlPredicates,
  queue_redirect_predicates: queueRedirectPredicates,
  browser_literal_types: browserLiteralTypes,
};
const rendered = `${JSON.stringify(inventory, null, 2)}\n`;
if (process.argv.includes('--check')) {
  const existing = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, 'utf8') : '';
  if (existing !== rendered) {
    throw new Error('business_command_inventory.json drifted; regenerate with build_business_command_inventory.mjs');
  }
  console.log('Business OS command type inventory OK', {
    exactControlTypes: exactControlTypes.length,
    controlPredicates: controlPredicates.length,
    browserLiteralTypes: browserLiteralTypes.length,
  });
} else {
  fs.writeFileSync(outputPath, rendered);
  console.log(`wrote ${outputPath}`);
}

function excluded(relative) {
  const file = relative.split(path.sep).join('/');
  return file.startsWith('rxdb/dist/')
    || file.startsWith('rxdb/src/')
    || file.startsWith('rxdb/tests/')
    || file.startsWith('scripts/')
    || file.endsWith('.test.js')
    || file.endsWith('.test.mjs')
    || file.endsWith('/test.mjs')
    || file.endsWith('/schema.js');
}

function walk(directory) {
  const files = [];
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...walk(target));
    else files.push(target);
  }
  return files;
}
