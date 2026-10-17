// node --test src/apps/business-os/v4/tools/port/scan.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { scanApp, scanModules, isVendorPath } from './scan.mjs';

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'v4-scan-'));
  const app = join(root, 'demo');
  mkdirSync(join(app, 'vendor', 'lib'), { recursive: true });
  mkdirSync(join(app, 'ui', 'vendor'), { recursive: true });
  writeFileSync(join(app, 'module.json'), '{"id":"demo"}');
  writeFileSync(join(app, 'index.js'), "export function mount(ctx) { ctx.db.collection('x'); }\n");
  // Fremdcode im App-Vendor: wuerde R3/R5/R12 treffen, darf aber nicht zaehlen.
  const foreign = "const commandBus = 1; setInterval(() => {}, 5); db.collection('y').findOne();\n";
  writeFileSync(join(app, 'vendor', 'lexical.mjs'), foreign);
  writeFileSync(join(app, 'vendor', 'lib', 'deep.js'), foreign);
  writeFileSync(join(app, 'vendor', 'lib', 'schema.js'), 'export default {};\n');
  writeFileSync(join(app, 'vendor', 'theme.css'), 'body { color: red }\n');
  // vendor/ nur auf App-Ebene ausgenommen, nicht tiefer.
  writeFileSync(join(app, 'ui', 'vendor', 'own.js'), 'setInterval(() => {}, 5);\n');
  return { root, app };
}

test('isVendorPath: nur vendor/ auf App-Ebene', () => {
  assert.equal(isVendorPath('vendor'), true);
  assert.equal(isVendorPath('vendor/lexical.mjs'), true);
  assert.equal(isVendorPath('vendor/lib/deep.js'), true);
  assert.equal(isVendorPath('ui/vendor/own.js'), false);
  assert.equal(isVendorPath('vendorish.js'), false);
});

test('scanApp: modules/<id>/vendor/** wird nicht gescannt (Code, CSS, schema.js)', () => {
  const { root, app } = fixture();
  try {
    const r = scanApp(app, 'demo');
    assert.equal(r.findings.filter((f) => f.file.startsWith('vendor')).length, 0);
    assert.ok(r.findings.some((f) => f.file === 'index.js' && f.rule === 'R3-db'));
    assert.ok(r.findings.some((f) => f.file === join('ui', 'vendor', 'own.js') && f.rule === 'R12-timer'));
    assert.equal(r.findings.filter((f) => f.rule === 'R17-schema').length, 0);
    assert.equal(r.cssUnscoped, 0);
    assert.equal(scanModules(root).length, 1);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
