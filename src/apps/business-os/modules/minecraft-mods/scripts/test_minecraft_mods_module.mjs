import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const moduleRoot = resolve(scriptDir, '..');
const modulesRoot = resolve(moduleRoot, '..');

const manifest = JSON.parse(readFileSync(join(moduleRoot, 'module.json'), 'utf8'));
const registry = JSON.parse(readFileSync(join(modulesRoot, 'registry.json'), 'utf8'));
const schema = readFileSync(join(moduleRoot, 'schema.js'), 'utf8');
const index = readFileSync(join(moduleRoot, 'index.js'), 'utf8');

assert(manifest.id === 'minecraft-mods', 'module id must be minecraft-mods');
assert(manifest.store?.installable === true, 'minecraft-mods must be App Store installable');
assert(manifest.source_path === undefined || manifest.source_path !== 'templates/business-basic', 'module must not use business-basic legacy path');
assert(registry.modules.some((mod) => mod.id === 'minecraft-mods' && mod.core === false), 'registry must include minecraft-mods as non-core');

for (const collection of [
  'minecraft_mod_projects',
  'minecraft_mod_artifacts',
  'minecraft_mod_installations',
  'minecraft_mod_merge_sets',
  'business_commands'
]) {
  assert(manifest.collections.includes(collection), `manifest missing ${collection}`);
  assert(schema.includes(collection), `schema missing ${collection}`);
}

for (const command of ['minecraft.mods.build', 'minecraft.mods.install', 'minecraft.mods.merge', 'minecraft.mods.inspect']) {
  assert(index.includes(command) || index.includes('minecraft.mods.${action}'), `index missing ${command}`);
}

for (const forbidden of ['next.js', 'postgres', '/api/business-os', 'fallbackDb']) {
  assert(!index.toLowerCase().includes(forbidden), `index contains forbidden legacy marker ${forbidden}`);
}

console.log('minecraft-mods module contract OK');

function assert(condition, message) {
  if (!condition) {
    console.error(message);
    process.exit(1);
  }
}
