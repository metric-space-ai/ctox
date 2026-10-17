import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { deterministicId, kurz, sha256Hex, wrapText } from '../_lib/hub.js';

test('sha256Hex entspricht node:crypto (ASCII, Umlaute, Emoji, NUL, >64 Byte)', () => {
  for (const text of ['', 'abc', 'Größe ändern', '🚀 x', 'a\0b\0c', 'x'.repeat(1000)]) {
    assert.equal(sha256Hex(text), createHash('sha256').update(text, 'utf8').digest('hex'));
  }
});

test('deterministicId: FNV-1a-64 mit dem Rust-Startwert 1469598103934665603 (NICHT der Standard 14695981039346656037)', () => {
  // decision_hub.rs :1376 nutzt einen um eine Ziffer gekuerzten Startwert. Der Port bleibt
  // treu, sonst aendern sich alle bestehenden Vorgangs-/Entscheidungs-IDs.
  assert.equal(deterministicId('p', ''), `p--${(1469598103934665603n).toString(16).padStart(16, '0')}`);
  const expectA = (((1469598103934665603n ^ 0x61n) * 1099511628211n) & 0xffffffffffffffffn).toString(16).padStart(16, '0');
  assert.equal(deterministicId('p', 'a'), `p-a-${expectA}`);
  const id = deterministicId('kpl-v', '<msg/1@x>');
  assert.match(id, /^kpl-v-msg1x-[0-9a-f]{16}$/);
});

test('wrapText bricht bei Breite, behaelt Leerabsaetze, entfernt Leerzeilen am Ende', () => {
  assert.deepEqual(wrapText('eins zwei drei', 9), ['eins zwei', 'drei']);
  assert.deepEqual(wrapText('a\n\nb\n\n', 10), ['a', '', 'b']);
});

test('kurz zaehlt Zeichen, nicht UTF-16-Einheiten', () => {
  assert.equal(kurz('äöü', 3), 'äöü');
  assert.equal(kurz('abcdef', 4), 'abc…');
});
