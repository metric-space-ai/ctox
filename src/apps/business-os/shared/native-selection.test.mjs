import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const css = await readFile(new URL('../app.css', import.meta.url), 'utf8');

test('Business OS disables page-like text selection on the application canvas', () => {
  assert.match(css, /body\s*\{[\s\S]*?-webkit-user-select:\s*none;[\s\S]*?user-select:\s*none;/);
});

test('real text and explicitly copyable surfaces remain selectable', () => {
  assert.match(
    css,
    /input,[\s\S]*?textarea,[\s\S]*?\[contenteditable\]:not\(\[contenteditable="false"\]\),[\s\S]*?\.ctox-chat-body,[\s\S]*?\[data-ctox-text-selectable\],[\s\S]*?user-select:\s*text;/,
  );
});
