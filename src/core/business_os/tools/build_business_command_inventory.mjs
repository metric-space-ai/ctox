import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const toolDir = path.dirname(fileURLToPath(import.meta.url));
const businessOsDir = path.resolve(toolDir, '..');
const repoRoot = path.resolve(businessOsDir, '../../..');
const storePath = path.join(businessOsDir, 'store.rs');
const peerPath = path.join(businessOsDir, 'rxdb_peer.rs');
const browserRoot = path.join(repoRoot, 'src/apps/business-os');
const outputPath = path.join(businessOsDir, 'business_command_inventory.json');
const source = fs.readFileSync(storePath, 'utf8');
const peerSource = fs.readFileSync(peerPath, 'utf8');
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
const controlClassifierBody = functionBody(source, 'is_rxdb_control_command_type');
const predicateNames = [...new Set(
  [...controlClassifierBody.matchAll(/(?:[A-Za-z_][\w]*::)*([A-Za-z_][\w]*)\(command_type\)/g)]
    .map((match) => match[1])
    .filter((name) => name !== 'contains'),
)];
const rustSources = walk(path.join(repoRoot, 'src/core'))
  .filter((file) => file.endsWith('.rs'))
  .map((file) => fs.readFileSync(file, 'utf8'));
const predicateBodies = predicateNames.flatMap((name) => {
  for (const rustSource of rustSources) {
    const body = functionBody(rustSource, name, false);
    if (body) return [body];
  }
  return [];
});
const predicateControlTypes = [...new Set(
  predicateBodies.flatMap((body) => [...body.matchAll(/"([a-z][a-z0-9_-]*(?:\.[a-z0-9_:-]+)+)"/g)].map((match) => match[1])),
)].sort();
const controlPrefixes = [...new Set(
  [controlClassifierBody, ...predicateBodies]
    .flatMap((body) => [...body.matchAll(/\.starts_with\("([^"]+)"\)/g)].map((match) => match[1])),
)].sort();
const browserRuntimeTypes = [...new Set(
  [...functionBody(peerSource, 'is_browser_runtime_command').matchAll(/"([a-z][a-z0-9_-]*(?:\.[a-z0-9_:-]+)+)"/g)]
    .map((match) => match[1]),
)].sort();
const browserLiteralTypes = [...new Set(
  walk(browserRoot)
    .filter((file) => /\.(?:js|mjs)$/.test(file))
    .filter((file) => !excluded(path.relative(browserRoot, file)))
    .flatMap((file) => {
      const text = fs.readFileSync(file, 'utf8');
      const constants = new Map(
        [...text.matchAll(/\bconst\s+([A-Z][A-Z0-9_]*)\s*=\s*['"]([a-z][a-z0-9_-]*(?:\.[a-z0-9_:-]+)+)['"]/g)]
          .map((match) => [match[1], match[2]]),
      );
      const propertyPattern = /\b(command_type|commandType|type)\s*:\s*(?:['"]([a-z][a-z0-9_-]*(?:\.[a-z0-9_:-]+)+)['"]|([A-Z][A-Z0-9_]*))/g;
      return [...text.matchAll(propertyPattern)]
        .filter((match) => match[1] !== 'type' || legacyTypeLooksLikeCommand(text, match.index))
        .map((match) => match[2] || constants.get(match[3]) || '')
        .filter(Boolean);
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
  predicate_control_types: predicateControlTypes,
  control_prefixes: controlPrefixes,
  browser_runtime_types: browserRuntimeTypes,
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
    predicateControlTypes: predicateControlTypes.length,
    browserRuntimeTypes: browserRuntimeTypes.length,
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

function legacyTypeLooksLikeCommand(text, index) {
  const before = text.slice(Math.max(0, index - 320), index);
  const after = text.slice(index, Math.min(text.length, index + 900));
  const context = `${before}${after}`;
  return /\bmodule\s*:/.test(context) && /\bpayload\s*(?::|,|\})/.test(context);
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

function functionBody(text, name, required = true) {
  const pattern = new RegExp(`(?:pub(?:\\([^)]*\\))?\\s+)?(?:super\\s+)?fn\\s+${name}\\s*\\(`);
  const match = pattern.exec(text);
  if (!match) {
    if (required) throw new Error(`cannot locate Rust function ${name}`);
    return '';
  }
  const open = text.indexOf('{', match.index);
  if (open < 0) {
    if (required) throw new Error(`cannot locate body for Rust function ${name}`);
    return '';
  }
  let depth = 0;
  let quote = '';
  let escaped = false;
  for (let index = open; index < text.length; index += 1) {
    const char = text[index];
    if (quote) {
      if (escaped) escaped = false;
      else if (char === '\\') escaped = true;
      else if (char === quote) quote = '';
      continue;
    }
    if (char === '"' || char === "'") {
      quote = char;
      continue;
    }
    if (char === '{') depth += 1;
    else if (char === '}') {
      depth -= 1;
      if (depth === 0) return text.slice(open + 1, index);
    }
  }
  if (required) throw new Error(`unterminated Rust function ${name}`);
  return '';
}
