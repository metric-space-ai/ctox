import assert from 'node:assert/strict';
import test from 'node:test';

import {
  applyWorkjetCategory,
  isPublicWorkjetModule,
  normalizeWorkjetCategory,
  workjetCategoryForModule,
  workjetCategoryForTarget,
  workjetCategoryStyle,
  WORKJET_CATEGORY_IDS,
} from './workjet-theme.js';

test('normalizes the complete Workjet category vocabulary and safe aliases', () => {
  assert.deepEqual(WORKJET_CATEGORY_IDS, [
    'workspace',
    'collaboration',
    'productivity',
    'development',
    'engineering',
    'knowledge',
    'research',
    'sales',
    'recruiting',
    'finance',
    'operations',
    'governance',
    'security',
    'analytics',
    'system',
    'imported',
  ]);
  assert.equal(normalizeWorkjetCategory('Recherche'), 'research');
  assert.equal(normalizeWorkjetCategory('Management'), 'operations');
  assert.equal(normalizeWorkjetCategory('Engineering'), 'engineering');
  assert.equal(normalizeWorkjetCategory('customer-private', 'workspace'), 'workspace');
  assert.equal(normalizeWorkjetCategory('customer-private'), 'imported');
});

test('only public/core module manifests can provide a category accent', () => {
  assert.equal(workjetCategoryForModule({ core: true, category: 'Security' }), 'security');
  assert.equal(workjetCategoryForModule({
    manifest: { source: 'core', category: 'Engineering' },
  }), 'engineering');
  assert.equal(workjetCategoryForModule({
    store: { distribution: 'store', category: 'Research' },
  }), 'research');
  assert.equal(workjetCategoryForModule({
    source: 'runtime',
    store: { distribution: 'runtime' },
    category: 'Sales',
  }), 'imported');
  assert.equal(workjetCategoryForModule({
    core: true,
    category: 'Security',
    customer_id: 'customer-42',
  }), 'imported');
  assert.equal(isPublicWorkjetModule({ source: 'runtime', visibility: 'private' }), false);
});

test('targets and rendered elements use the same canonical category refs', () => {
  assert.equal(workjetCategoryForTarget({
    kind: 'module',
    core: true,
    category: 'Workspace',
  }), 'workspace');
  assert.equal(workjetCategoryForTarget({ kind: 'app', category: 'Development' }), 'development');

  const style = workjetCategoryStyle('Security');
  assert.equal(style.id, 'security');
  assert.equal(style.accent, 'var(--workjet-category-security-accent)');
  assert.equal(style.soft, 'var(--workjet-category-security-accent-soft)');

  const properties = {};
  const element = {
    dataset: {},
    style: { setProperty(name, value) { properties[name] = value; } },
  };
  assert.equal(applyWorkjetCategory(element, 'Security'), 'security');
  assert.equal(element.dataset.workjetCategory, 'security');
  assert.equal(properties['--shell-category-accent'], 'var(--workjet-category-security-accent)');
  assert.equal(properties['--shell-category-border'], 'var(--workjet-category-security-accent-border)');
});

// APPSTORE-V2 P6 Teil 2: Die Rubrik ist unabhaengig von der Herkunft. Vorher
// entschied PUBLIC_DISTRIBUTIONS ueber die Rubrik — weil dieses Set weder
// 'catalog-module' noch 'ctox-runtime-installed-module' kannte, verloren ALLE
// Katalog-Apps und jede installierte App im Startmenue ihre deklarierte
// Rubrik und landeten unter 'imported', waehrend der Store die echte zeigte.
test('the rubric follows the declared category, not the app origin', () => {
  // Katalog-App: origin official, Distribution steht NICHT in der alten
  // Public-Liste — trotzdem behaelt sie ihre Rubrik.
  assert.equal(workjetCategoryForModule({
    id: 'buchhaltung',
    origin: 'official',
    category: 'finance',
    store: { distribution: 'catalog-module' },
  }), 'finance');

  // Laufzeit-installierte App, ebenfalls ausserhalb der alten Public-Liste.
  assert.equal(workjetCategoryForModule({
    id: 'kundenpipeline',
    origin: 'official',
    category: 'operations',
    store: { distribution: 'ctox-runtime-installed-module' },
  }), 'operations');

  // Eigene App des Nutzers: behaelt ihre Rubrik, wenn sie eine kanonische hat.
  assert.equal(workjetCategoryForModule({
    id: 'meine-app',
    origin: 'user',
    category: 'sales',
  }), 'sales');

  // Nur eine Nutzer-App OHNE kanonische Rubrik faellt auf 'imported'.
  assert.equal(workjetCategoryForModule({
    id: 'meine-app',
    origin: 'user',
    category: 'voellig-erfunden',
  }), 'imported');

  // Rueckwaertskompatibilitaet: Projektionen alter Daemons ohne origin-Feld
  // laufen weiter ueber die alte Heuristik (Alt-Binary + neue Shell).
  assert.equal(workjetCategoryForModule({
    id: 'legacy',
    category: 'finance',
    store: { distribution: 'catalog-module' },
  }), 'imported');
});
