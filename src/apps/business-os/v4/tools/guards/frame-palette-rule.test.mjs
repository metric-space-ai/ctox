// node --test src/apps/business-os/v4/tools/guards/frame-palette-rule.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { findKey, scanText, scan, ratchet, loadBaseline, writeBaseline, main, DEFAULT_ROOT, DEFAULT_BASELINE } from './frame-palette-rule.mjs';

function tree(files) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'frame-palette-'));
  for (const [rel, value] of Object.entries(files)) {
    const p = path.join(root, rel);
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, typeof value === 'string' ? value : JSON.stringify(value));
  }
  return root;
}

test('findKey/scanText: jede Tiefe, kein Treffer ohne Schluessel, kaputtes JSON zaehlt Textvorkommen', () => {
  assert.deepEqual(findKey({ layout: { frame_palette: { start: '#000' } } }), ['layout.frame_palette']);
  assert.deepEqual(findKey({ a: [{ frame_palette: 1 }], frame_palette: 2 }), ['a.0.frame_palette', 'frame_palette']);
  assert.deepEqual(findKey({ layout: { icon_asset: 'assets/icon.jpg', note: 'frame_palette' } }), []);
  assert.deepEqual(scanText('{"layout": {"frame_palette": {}'), ['(kein JSON)']);
});

test('scan: module.json und v4-manifest.json unter modules/, customer-modules/, desktop-apps/; andere Dateien nicht', () => {
  const root = tree({
    'modules/a/module.json': { layout: { frame_palette: { start: '#111' } } },
    'modules/b/module.json': { layout: { icon_asset: 'assets/icon.jpg' } },
    'modules/b/v4-manifest.json': { _format: 'x', frame_palette: {} },
    'customer-modules/c/module.json': { layout: { frame_palette: {} } },
    'modules/d/other.json': { frame_palette: {} },
    'shared/module.json': { frame_palette: {} },
  });
  try {
    const r = scan(root);
    assert.deepEqual(Object.keys(r.files).sort(), ['customer-modules/c/module.json', 'modules/a/module.json', 'modules/b/v4-manifest.json']);
    assert.equal(r.total, 3);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('Ratchet: neue Datei oder mehr Treffer = Befund (Exit 1), weniger = senken; Baseline steigt nur mit --force', () => {
  const root = tree({ 'modules/a/module.json': { layout: { frame_palette: {} } } });
  const base = path.join(root, 'baseline.json');
  try {
    assert.equal(main(['--root', root, '--baseline', base, '--check']), 1, 'ohne Baseline ist jeder Treffer neu');
    assert.throws(() => writeBaseline(base, scan(root)), /--force/);
    writeBaseline(base, scan(root), { force: true });
    assert.deepEqual(loadBaseline(base), { 'modules/a/module.json': 1 });
    assert.equal(main(['--root', root, '--baseline', base, '--check']), 0);
    fs.mkdirSync(path.join(root, 'modules/b'), { recursive: true });
    fs.writeFileSync(path.join(root, 'modules/b/v4-manifest.json'), JSON.stringify({ frame_palette: { start: '#fff' } }));
    assert.equal(main(['--root', root, '--baseline', base, '--check']), 1, 'neue Palette ist ein Befund');
    fs.writeFileSync(path.join(root, 'modules/a/module.json'), '{}');
    fs.rmSync(path.join(root, 'modules/b/v4-manifest.json'));
    const rat = ratchet(scan(root), loadBaseline(base));
    assert.deepEqual(rat.lowered, [{ file: 'modules/a/module.json', count: 0, allowed: 1 }]);
    writeBaseline(base, scan(root));
    assert.deepEqual(loadBaseline(base), {});
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('heutiger Baum: nichts ueber der Baseline (Altbestand nur senken)', () => {
  const rat = ratchet(scan(DEFAULT_ROOT), loadBaseline(DEFAULT_BASELINE));
  assert.deepEqual(rat.raised, []);
});
