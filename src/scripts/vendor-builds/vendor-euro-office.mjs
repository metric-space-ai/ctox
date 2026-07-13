import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, '../../..');
const officeRoot = path.join(repoRoot, 'src/apps/business-os/office-engine');
const pin = JSON.parse(fs.readFileSync(path.join(officeRoot, 'upstream/euro-office-v9.3.1.json'), 'utf8'));
const sourceArg = process.argv.find((value) => value.startsWith('--source='));
if (!sourceArg) {
  throw new Error('Pass --source=/absolute/path/to/euro-office. The vendor step never clones or fetches automatically.');
}
const sourceRoot = path.resolve(sourceArg.slice('--source='.length));
const webAppsRoot = path.join(sourceRoot, 'web-apps');
const sdkjsRoot = path.join(sourceRoot, 'sdkjs');
const coreFontsRoot = path.join(sourceRoot, 'core-fonts');
verifyCheckout(webAppsRoot, pin.submodules['web-apps']);
verifyCheckout(sdkjsRoot, pin.submodules.sdkjs);
verifyCheckout(coreFontsRoot, pin.submodules['core-fonts']);

const destinationArg = process.argv.find((value) => value.startsWith('--destination='));
const destination = destinationArg
  ? path.resolve(destinationArg.slice('--destination='.length))
  : path.join(repoRoot, 'runtime/vendor-sources/euro-office/document-closure-audit');
fs.rmSync(destination, { recursive: true, force: true });
fs.mkdirSync(destination, { recursive: true });
const copied = [];

for (const relative of [
  'LICENSE.txt', '3DPARTY.md', 'build/documenteditor.json',
  'apps/documenteditor/main', 'apps/common/main',
  'vendor/jquery', 'vendor/underscore', 'vendor/backbone', 'vendor/requirejs-text',
  'vendor/perfect-scrollbar', 'vendor/xregexp',
]) copyTree(webAppsRoot, path.join(destination, 'web-apps'), relative, copied);

for (const relative of ['LICENSE.txt', '3DPARTY.md', 'configs/word.json']) {
  copyTree(sdkjsRoot, path.join(destination, 'sdkjs'), relative, copied);
}
const wordConfig = JSON.parse(fs.readFileSync(path.join(sdkjsRoot, 'configs/word.json'), 'utf8'));
const sdkFiles = [...new Set(collectStrings(wordConfig.sdk).filter((value) => value.endsWith('.js')))];
const excludedConfigFiles = [];
for (const relative of sdkFiles) {
  if (/^(slide|visio|pdf|cell)\//.test(relative)) {
    excludedConfigFiles.push(relative);
    continue;
  }
  copyTree(sdkjsRoot, path.join(destination, 'sdkjs'), relative, copied);
}

const forbidden = copied.filter((entry) => /(^|\/)(presentationeditor|spreadsheeteditor|pdfeditor|visioeditor|mobile|adminpanel|wopi)(\/|$)/i.test(entry.path));
if (forbidden.length) throw new Error(`Forbidden Euro-Office surfaces entered the document closure:\n${forbidden.map((entry) => entry.path).join('\n')}`);
copied.sort((left, right) => left.path.localeCompare(right.path));
const inventory = {
  schema_version: 'ctox-euro-office-source-closure-v1',
  release: pin.release,
  documentserver_commit: pin.commit_sha,
  submodules: { 'web-apps': pin.submodules['web-apps'], sdkjs: pin.submodules.sdkjs, 'core-fonts': pin.submodules['core-fonts'] },
  policy: 'document-only-bootstrap',
  copied_files: copied,
  excluded_cross_editor_config_files: excludedConfigFiles.sort(),
};
fs.writeFileSync(path.join(destination, 'source-inventory.json'), `${JSON.stringify(inventory, null, 2)}\n`);
console.log(`Vendored ${copied.length} pinned Euro-Office source files; excluded ${excludedConfigFiles.length} cross-editor sdkjs entries.`);

function verifyCheckout(root, expectedCommit) {
  if (!fs.statSync(root, { throwIfNoEntry: false })?.isDirectory()) throw new Error(`Missing checkout: ${root}`);
  const actual = execFileSync('git', ['-C', root, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
  if (actual !== expectedCommit) throw new Error(`Pinned checkout mismatch at ${root}: expected ${expectedCommit}, got ${actual}`);
}

function collectStrings(value) {
  if (typeof value === 'string') return [value];
  if (Array.isArray(value)) return value.flatMap(collectStrings);
  if (value && typeof value === 'object') return Object.values(value).flatMap(collectStrings);
  return [];
}

function copyTree(sourceBase, targetBase, relative, inventory) {
  const source = path.join(sourceBase, relative);
  const stat = fs.statSync(source, { throwIfNoEntry: false });
  if (!stat) throw new Error(`Pinned source closure entry is missing: ${source}`);
  if (stat.isDirectory()) {
    for (const entry of fs.readdirSync(source, { withFileTypes: true })) {
      copyTree(sourceBase, targetBase, path.join(relative, entry.name), inventory);
    }
    return;
  }
  const bytes = fs.readFileSync(source);
  const target = path.join(targetBase, relative);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.copyFileSync(source, target);
  inventory.push({
    path: path.relative(destination, target).split(path.sep).join('/'),
    bytes: bytes.length,
    sha256: crypto.createHash('sha256').update(bytes).digest('hex'),
  });
}
