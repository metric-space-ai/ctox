// Wächter für die eingecheckte Stub-Kopie (fixtures/app-api-stub, genutzt von layout.browser.mjs): sie darf nicht
// still veralten oder verändert werden.
//   (a) immer, auch in CI: sha256 der Kopie = `sha256:` in PIN.md
//   (b) nur wenn das Stub-Repo lokal liegt (~/.local/state/ctox-sync-v4/stub, sonst CTOX_APP_API_STUB_REPO):
//       Kopie byte-gleich zu `git show <pin-commit>:ctox-app-api-stub.mjs`; sonst sichtbar übersprungen.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';

const DIR = new URL('./fixtures/app-api-stub/', import.meta.url);
const copy = readFileSync(new URL('ctox-app-api-stub.mjs', DIR));
const pin = readFileSync(new URL('PIN.md', DIR), 'utf8');
const field = (name) => pin.match(new RegExp(`^${name}: ([0-9a-f]+)$`, 'm'))?.[1];
const STUB_REPO = process.env.CTOX_APP_API_STUB_REPO || join(homedir(), '.local/state/ctox-sync-v4/stub');

test('Stub-Kopie: sha256 gleich PIN.md', () => {
  const expected = field('sha256');
  assert.match(expected || '', /^[0-9a-f]{64}$/, 'PIN.md braucht eine Zeile "sha256: <64 hex>"');
  assert.equal(createHash('sha256').update(copy).digest('hex'), expected);
});

test('Stub-Kopie: byte-gleich zum gepinnten Commit im Stub-Repo', (t) => {
  const commit = field('pin-commit');
  assert.match(commit || '', /^[0-9a-f]{40}$/, 'PIN.md braucht eine Zeile "pin-commit: <40 hex>"');
  if (!existsSync(join(STUB_REPO, '.git'))) {
    const note = `Stub-Repo ${STUB_REPO} nicht vorhanden – Vergleich mit ${commit} übersprungen (nur sha256 geprüft)`;
    console.log(`HINWEIS: ${note}`);
    t.skip(note);
    return;
  }
  const pinned = execFileSync('git', ['-C', STUB_REPO, 'show', `${commit}:ctox-app-api-stub.mjs`], { maxBuffer: 16 * 1024 * 1024 });
  assert.ok(Buffer.compare(copy, pinned) === 0, `Kopie weicht von ${commit}:ctox-app-api-stub.mjs ab`);
});
