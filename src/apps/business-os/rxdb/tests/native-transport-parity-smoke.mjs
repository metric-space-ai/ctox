// The native host and its two standalone test crates must exercise the same
// WebRTC implementation, including the local ICE patch. Previously Sync tests
// passed with 0.20.5 while the shipped daemon used 0.20.0-alpha.1.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../../../..');
const roots = ['', 'src/core/rxdb', 'src/core/sync'];
const read = path => readFileSync(resolve(root, path), 'utf8');
const value = (block, key) => block.match(new RegExp('^' + key + ' = "([^"]+)"', 'm'))?.[1];
const transport = name => name === 'webrtc' || name === 'rtc' || name?.startsWith('rtc-');
const locks = roots.map(base => {
  const manifest = read(base ? base + '/Cargo.toml' : 'Cargo.toml');
  const patch = manifest.split(/^\[patch\.crates-io\]\s*$/m)[1]?.split(/^\[/m)[0];
  const icePath = patch?.match(/^rtc-ice = \{[^\n]*path = "([^"]+)"/m)?.[1];
  const packages = read(base ? base + '/Cargo.lock' : 'Cargo.lock')
    .split(/^\[\[package\]\]\s*$/m).slice(1)
    .filter(block => transport(value(block, 'name')))
    .map(block => {
      const name = value(block, 'name');
      const version = value(block, 'version');
      let source = value(block, 'source');
      if (!source) {
        assert.equal(name, 'rtc-ice', 'unrecognized local transport package');
        assert.ok(icePath, base + ': local ICE package has no declared patch');
        source = resolve(root, base, icePath);
      }
      return [name, { version, source, checksum: value(block, 'checksum') ?? null }];
    }).sort(([a], [b]) => a.localeCompare(b));
  assert.ok(packages.some(([name]) => name === 'webrtc'), base + ': WebRTC package missing');
  assert.equal(new Set(packages.map(([name]) => name)).size, packages.length,
    base + ': multiple versions of a transport package');
  return { base: base || '.', packages: Object.fromEntries(packages) };
});

for (const lock of locks.slice(1)) {
  assert.deepEqual(lock.packages, locks[0].packages,
    lock.base + ': standalone tests use a different native transport than the CTOX binary');
}
const rxdbManifest = read('src/core/rxdb/Cargo.toml');
for (const dependency of ['webrtc', 'ice']) {
  const line = rxdbManifest.match(new RegExp('^' + dependency + ' = (.+)$', 'm'))?.[1];
  assert.ok(line && /"=[^"]+"/.test(line),
    dependency + ': transport dependencies require exact pins, not compatible ranges');
}
console.log('native transport dependency and patch parity OK (' +
  Object.keys(locks[0].packages).length + ' packages)');
