import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const appRoot = resolve(scriptDir, '..');
const repoRoot = resolve(appRoot, '../../..');

const scannedRoots = [
  join(appRoot, 'app.js'),
  join(appRoot, 'index.html'),
  join(appRoot, 'shared'),
  join(appRoot, 'modules'),
  join(repoRoot, 'src/core/business_os/store.rs'),
];

const excludedSegments = new Set(['vendor', 'output', 'installed-modules']);
const forbidden = [
  { name: 'frontend-rxdb-http-pull', pattern: /\/rxdb\/pull/ },
  { name: 'frontend-command-http-post', pattern: /\/commands[`'")]/ },
  { name: 'native-http-command-bridge', pattern: /recordNativeCommand|pullNativeCollection|native-http-pull/ },
  { name: 'sync-config-http-bridge-enabled', pattern: /http_bridge_available:\s*true/ },
  { name: 'native-http-bridge-reason', pattern: /native HTTP bridge/ },
];

const offenders = [];
for (const file of expandFiles(scannedRoots)) {
  const rel = relative(repoRoot, file);
  const content = readFileSync(file, 'utf8');
  for (const rule of forbidden) {
    if (rule.pattern.test(content)) offenders.push(`${rel}: ${rule.name}`);
  }
}

if (offenders.length) {
  console.error(`RxDB-only contract failed:\n${offenders.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log('RxDB-only contract OK');

function expandFiles(paths) {
  const files = [];
  for (const path of paths) {
    collect(path, files);
  }
  return files;
}

function collect(path, files) {
  const stat = statSync(path, { throwIfNoEntry: false });
  if (!stat) return;
  if (stat.isFile()) {
    if (/\.(js|mjs|html|rs)$/.test(path)) files.push(path);
    return;
  }
  if (!stat.isDirectory()) return;
  const name = path.split(/[\\/]/).pop();
  if (excludedSegments.has(name)) return;
  for (const entry of readdirSync(path)) collect(join(path, entry), files);
}
