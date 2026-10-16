// G10 Kontrast (Strang DARK): reine Funktionen des Kontrast-Kriteriums.
//   node --test v4/tools/geometry/contrast.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { contrastRatio, minRatio, MIN } from './contrast.mjs';

const hex = (h) => ({ r: parseInt(h.slice(1, 3), 16), g: parseInt(h.slice(3, 5), 16), b: parseInt(h.slice(5, 7), 16) });
const r2 = (v) => Math.round(v * 100) / 100;

test('Kontrast nach WCAG: Schwarz/Weiß 21, gleiche Farbe 1, symmetrisch', () => {
  assert.equal(r2(contrastRatio(hex('#000000'), hex('#ffffff'))), 21);
  assert.equal(r2(contrastRatio(hex('#777777'), hex('#777777'))), 1);
  assert.equal(contrastRatio(hex('#27272a'), hex('#ffffff')), contrastRatio(hex('#ffffff'), hex('#27272a')));
});

test('gemessene FIX8/DARK-Befunde: Kopfzeile auf fester Shell-v2-Fläche 1,16, Symbol 2,24, Token-Paar --muted/.ctox-well 4,43', () => {
  assert.equal(r2(contrastRatio(hex('#27272a'), hex('#141b24'))), 1.16);
  assert.equal(r2(contrastRatio(hex('#52525b'), hex('#141b24'))), 2.24);
  assert.equal(r2(contrastRatio(hex('#71717b'), hex('#f5f5f5'))), 4.43);
  // nach dem Fix: Text auf der Theme-Fläche
  assert.ok(contrastRatio(hex('#27272a'), hex('#fcfcfc')) >= MIN.body);
});

test('Schwellen: Fließtext 4,5, große Schrift (≥ 24px oder ≥ 18,66px fett) 3, Symbole 3', () => {
  assert.equal(minRatio({ fontSize: 13, fontWeight: 400 }), 4.5);
  assert.equal(minRatio({ fontSize: 20, fontWeight: 400 }), 4.5);
  assert.equal(minRatio({ fontSize: 20, fontWeight: 700 }), 3);
  assert.equal(minRatio({ fontSize: 24, fontWeight: 400 }), 3);
  assert.equal(minRatio({ symbol: true }), 3);
});
