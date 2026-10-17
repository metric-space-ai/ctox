import assert from 'node:assert/strict';
import { readFile, readdir, stat } from 'node:fs/promises';
import test from 'node:test';

import {
  EASY_EMAIL_UPSTREAM,
  createEasyEmailEditor,
  createEmailDocument,
} from '../index.mjs';

const bundleUrl = new URL('../bundle/', import.meta.url);

test('exposes the real pinned upstream bridge contract', () => {
  assert.equal(typeof createEasyEmailEditor, 'function');
  assert.match(createEasyEmailEditor.toString(), /setLogicPreview/);
  assert.equal(EASY_EMAIL_UPSTREAM.commit, '16bb02926a20af20dc6dc473c72619f4a0b4f64b');
  assert.equal(EASY_EMAIL_UPSTREAM.version, '4.17.1');
});

test('creates the upstream page/wrapper/section/column/text document model', () => {
  const document = createEmailDocument({ subject: 'Sales', content: '<p>Hallo</p>' });
  assert.equal(document.subject, 'Sales');
  assert.equal(document.content.type, 'page');
  assert.equal(document.content.children[0].type, 'wrapper');
  assert.equal(document.content.children[0].children[0].type, 'section');
  assert.equal(document.content.children[0].children[0].children[0].type, 'column');
  assert.equal(document.content.children[0].children[0].children[0].children[0].type, 'text');
});

test('bundle records the exact upstream pin and has a local HTML entry', async () => {
  const build = JSON.parse(await readFile(new URL('../bundle/BUILD.json', import.meta.url), 'utf8'));
  assert.equal(build.upstream_commit, EASY_EMAIL_UPSTREAM.commit);
  assert.equal(build.entry, 'frame.html');
  const html = await readFile(new URL('../bundle/frame.html', import.meta.url), 'utf8');
  assert.match(html, /<script[^>]+src="\.\/assets\//);
  assert.doesNotMatch(html, /https?:\/\//);
});

test('generated JavaScript has no bare or remote runtime imports', async () => {
  const assets = new URL('../bundle/assets/', import.meta.url);
  const files = (await readdir(assets)).filter((name) => name.endsWith('.js'));
  assert.ok(files.length >= 1);
  for (const file of files) {
    const source = await readFile(new URL(file, assets), 'utf8');
    assert.doesNotMatch(source, /\bfrom\s*["'](?:https?:|react(?:\/|["'])|easy-email-|@)/);
    assert.doesNotMatch(source, /\bimport\s*["'](?:https?:|react(?:\/|["'])|easy-email-|@)/);
  }
});

test('bundle contains substantial upstream React/MJML output', async () => {
  const assets = new URL('../bundle/assets/', import.meta.url);
  const files = await readdir(assets);
  const sizes = await Promise.all(files.map(async (name) => (await stat(new URL(name, assets))).size));
  assert.ok(sizes.reduce((sum, value) => sum + value, 0) > 1_000_000);
  const scripts = await Promise.all(
    files.filter((name) => name.endsWith('.js')).map((name) => readFile(new URL(name, assets), 'utf8')),
  );
  const source = scripts.join('\n');
  assert.match(source, /MJML v3 syntax|mjmlConfig|<mjml>/i);
  assert.match(source, /icon-undo/);
  assert.match(source, /icon-redo/);
  assert.match(source, /BlockLayerManager/);
  assert.match(source, /data-tree-idx/);
});
