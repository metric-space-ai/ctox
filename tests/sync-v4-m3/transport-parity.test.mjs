// Extend parity evidence to the M3 fixture and the explicit P4-b local SCTP patches.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
const root = path.resolve(import.meta.dirname, '../..');
const value = (block, key) => block.match(new RegExp('^' + key + ' = "([^"]+)"', 'm'))?.[1];
test('daemon, RxDB, sync and M3 use identical native transport packages', () => {
 const graphs = ['', 'src/core/rxdb', 'src/core/sync', 'tests/sync-v4-m3/native'].map(base => {
  const manifest = fs.readFileSync(path.join(root, base, 'Cargo.toml'), 'utf8');
  const patches = manifest.split(/^\[patch\.crates-io\]\s*$/m)[1]?.split(/^\[/m)[0] || '';
  return Object.fromEntries(fs.readFileSync(path.join(root, base, 'Cargo.lock'), 'utf8')
   .split(/^\[\[package\]\]\s*$/m).slice(1)
   .filter(block => /^(webrtc|rtc(?:-|$))/.test(value(block, 'name')))
   .map(block => {
    const name = value(block, 'name'), version = value(block, 'version');
    let source = value(block, 'source');
    if(!source) {
     assert.ok(['rtc-ice','rtc','webrtc'].includes(name), 'unknown local transport package');
     const local = patches.match(new RegExp('^' + name + ' = \\{[^\\n]*path = "([^"]+)"', 'm'))?.[1];
     assert.ok(local, base+': undeclared local transport patch '+name);
     source = fs.realpathSync(path.join(root, base, local));
    }
    return [name, {version, source, checksum:value(block,'checksum') || null}];
   }).sort(([a],[b])=>a.localeCompare(b)));
 });
 for(const graph of graphs.slice(1)) assert.deepEqual(graph, graphs[0]);
});
