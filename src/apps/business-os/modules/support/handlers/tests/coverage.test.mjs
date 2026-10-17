import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { SUPPORT_AGENT_CALLBACK_TYPES, SUPPORT_COMMAND_TYPES } from '../../support-commands.mjs';
import { SUPPORT_COMMAND_PERMISSIONS } from '../_shared.js';
import { loadHandler } from './_ctx.mjs';

const handlersDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');

test('every support command type has a handler file with a default export', async () => {
  assert.equal(SUPPORT_COMMAND_TYPES.length, 35);
  assert.equal(SUPPORT_AGENT_CALLBACK_TYPES.length, 2);
  for (const type of [...SUPPORT_COMMAND_TYPES, ...SUPPORT_AGENT_CALLBACK_TYPES]) {
    assert.ok(existsSync(resolve(handlersDir, `${type}.js`)), `${type}.js exists`);
    assert.equal(typeof (await loadHandler(type)), 'function', `${type} default export`);
  }
});

test('permission table covers exactly the 35 command types and the 2 agent callbacks', () => {
  assert.deepEqual(Object.keys(SUPPORT_COMMAND_PERMISSIONS).sort(), [...SUPPORT_COMMAND_TYPES, ...SUPPORT_AGENT_CALLBACK_TYPES].sort());
});

test('handlers use no file system, network or process APIs', () => {
  const forbidden = /\bfrom\s+['"](node:|fs|path|http|https|net|child_process|os)|\brequire\(|\bprocess\.|\bfetch\(|globalThis\.crypto|Date\.now\(|Math\.random\(/;
  for (const type of [...SUPPORT_COMMAND_TYPES, ...SUPPORT_AGENT_CALLBACK_TYPES, 'support.intake.project', '_shared', '_suggestion-decision', '_agent-task']) {
    const source = readFileSync(resolve(handlersDir, `${type}.js`), 'utf8');
    assert.ok(!forbidden.test(source), `${type}.js stays inside the ctx contract`);
    for (const match of source.matchAll(/from\s+['"]([^'"]+)['"]/g)) {
      // erlaubt: handler-lokale Helfer und der reine Shell-Helfer ctox:shared/prompt-template.js (§16.36 Nr. 3)
      assert.ok(match[1].startsWith('./_') || match[1] === 'ctox:shared/prompt-template.js', `${type}.js imports only handler-local helpers (${match[1]})`);
    }
  }
});

test('V4-HANDLERS.md lists every command type', () => {
  const doc = readFileSync(resolve(handlersDir, '..', 'V4-HANDLERS.md'), 'utf8');
  for (const type of [...SUPPORT_COMMAND_TYPES, ...SUPPORT_AGENT_CALLBACK_TYPES]) assert.ok(doc.includes(`\`${type}\``), `${type} documented`);
});
