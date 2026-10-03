'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const { CATALOG_MODULES, prepareUiCatalogFixture } = require('./business_os_ui_catalog_fixture');
const source = path.resolve(__dirname, '../../../..');
const sourceApp = path.join(source, 'src/apps/business-os');
const sourceTemplates = path.join(sourceApp, 'template-store');

function freshRoot(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ctox-ui-catalog-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  return root;
}

test('catalog prerequisites use native template inputs without changing source or installed state', (t) => {
  const root = freshRoot(t);
  const app = path.join(root, 'src/apps/business-os');
  fs.mkdirSync(app, { recursive: true });
  const templates = path.join(app, 'template-store');
  fs.symlinkSync(sourceTemplates, templates, 'dir');
  const originalNames = fs.readdirSync(sourceTemplates).sort();
  const originalMatching = fs.readFileSync(path.join(sourceTemplates, 'matching/template.json'));
  const fixture = prepareUiCatalogFixture(source, root);
  assert.equal(fixture.installs.length, 17);
  assert.deepEqual(fixture.installs.map((item) => item.moduleId), [...CATALOG_MODULES]);
  assert.equal(fs.lstatSync(templates).isSymbolicLink(), false);
  for (const item of fixture.installs) {
    const template = JSON.parse(fs.readFileSync(path.join(templates, item.templateId, 'template.json')));
    assert.equal(template.source_module, item.moduleId);
    assert.equal(template.id, item.templateId);
    assert.equal(template.starter_archetype, undefined);
  }
  assert.deepEqual(fs.readdirSync(sourceTemplates).sort(), originalNames);
  assert.deepEqual(fs.readFileSync(path.join(sourceTemplates, 'matching/template.json')), originalMatching);
  assert.equal(fs.existsSync(path.join(root, 'runtime/business-os/installed-modules')), false);
  assert.throws(() => prepareUiCatalogFixture(source, root), /fresh smoke template-store symlink/);
});

test('source root and a smoke app symlink escaping its root are rejected before writes', (t) => {
  assert.throws(() => prepareUiCatalogFixture(source, source), /separate smoke root/);
  const root = freshRoot(t);
  fs.mkdirSync(path.join(root, 'src/apps'), { recursive: true });
  fs.symlinkSync(sourceApp, path.join(root, 'src/apps/business-os'), 'dir');
  assert.throws(() => prepareUiCatalogFixture(source, root), /app root escapes/);
});

test('an existing real template directory is never repurposed as a fresh fixture', (t) => {
  const root = freshRoot(t);
  const templates = path.join(root, 'src/apps/business-os/template-store');
  fs.mkdirSync(templates, { recursive: true });
  fs.writeFileSync(path.join(templates, 'keep'), 'untouched');
  assert.throws(() => prepareUiCatalogFixture(source, root), /fresh smoke template-store symlink/);
  assert.deepEqual(fs.readdirSync(templates), ['keep']);
  assert.equal(fs.readFileSync(path.join(templates, 'keep'), 'utf8'), 'untouched');
});
