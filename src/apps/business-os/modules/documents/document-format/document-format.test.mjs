import assert from 'node:assert/strict';
import test from 'node:test';

import { materializeDocxMergeFields, materializeDocxTextReplacements } from '../../../vendor/document-format.mjs';

function complexField(instruction, resultText) {
  return {
    type: 'reference',
    name: 'w:noBreakHyphen',
    field: {
      kind: 'complex',
      instruction,
      instructionType: 'UNKNOWN',
      resultText,
      displayText: resultText,
      supported: false,
      complete: true,
      structure: { nodeName: 'sd:unsupportedField', fieldType: 'UNKNOWN', reason: 'fixture' },
      rawNodes: [
        run({ name: 'w:fldChar', attributes: { 'w:fldCharType': 'begin' } }),
        run({ name: 'w:instrText', children: [{ name: '#text', text: ` ${instruction} ` }] }),
        run({ name: 'w:fldChar', attributes: { 'w:fldCharType': 'separate' } }),
        run({ name: 'w:t', children: [{ name: '#text', text: resultText }] }),
        run({ name: 'w:fldChar', attributes: { 'w:fldCharType': 'end' } }),
      ],
    },
  };
}

function run(content) {
  return {
    name: 'w:r',
    children: [
      { name: 'w:rPr', children: [{ name: 'w:b' }] },
      content,
    ],
  };
}

function textIn(node) {
  if (Array.isArray(node)) return node.map(textIn).join('');
  if (!node || typeof node !== 'object') return '';
  return node.name === '#text' ? node.text ?? '' : textIn(node.children ?? []);
}

test('materializes MERGEFIELD values in every document story and preserves non-merge fields', () => {
  const bodyField = complexField('MERGEFIELD Name', '«Name»');
  const headerField = complexField(' MERGEFIELD "E Mail" \\* MERGEFORMAT ', '«E Mail»');
  const footerPage = complexField('PAGE', '1');
  const document = {
    type: 'document',
    body: { type: 'body', blocks: [{ type: 'paragraph', runs: [bodyField] }] },
    headers: { header1: { type: 'header', blocks: [{ type: 'paragraph', runs: [headerField] }] } },
    footers: { footer1: { type: 'footer', blocks: [{ type: 'paragraph', runs: [footerPage] }] } },
  };

  const report = materializeDocxMergeFields(document, { name: 'WITTENSTEIN SE', 'e mail': 'kontakt@example.test' });

  assert.deepEqual(report, { mergedFields: ['E Mail', 'Name'], missingFields: [] });
  assert.equal(textIn(bodyField.field.rawNodes), 'WITTENSTEIN SE');
  assert.equal(textIn(headerField.field.rawNodes), 'kontakt@example.test');
  assert.equal(bodyField.field.rawNodes.length, 1);
  assert.equal(bodyField.field.rawNodes[0].children[0].name, 'w:rPr');
  assert.equal(footerPage.field.instruction, 'PAGE');
  assert.equal(footerPage.field.rawNodes.length, 5);
});

test('reports missing merge values without changing the field result', () => {
  const field = complexField('MERGEFIELD salu', '«salu»');
  const document = {
    type: 'document',
    body: { type: 'body', blocks: [{ type: 'paragraph', runs: [field] }] },
  };

  const report = materializeDocxMergeFields(document, {});

  assert.deepEqual(report, { mergedFields: [], missingFields: ['salu'] });
  assert.equal(textIn(field.field.rawNodes), ' MERGEFIELD salu «salu»');
});

test('materializes legacy guillemet placeholders in all document stories', () => {
  const document = {
    type: 'document',
    body: { type: 'body', blocks: [{ type: 'paragraph', runs: [{ type: 'text', text: 'E-Mail: «oema»' }] }] },
    sections: [{ header: { blocks: [{ type: 'paragraph', runs: [{ type: 'text', text: '«ONAM»' }] }] } }],
  };
  const report = materializeDocxMergeFields(document, { oema: 'kontakt@example.test', onam: 'Beispiel GmbH' });

  assert.deepEqual(report, { mergedFields: ['oema', 'ONAM'], missingFields: [] });
  assert.equal(document.body.blocks[0].runs[0].text, 'E-Mail: kontakt@example.test');
  assert.equal(document.sections[0].header.blocks[0].runs[0].text, 'Beispiel GmbH');
});

test('materializes explicitly configured literal template text', () => {
  const document = {
    type: 'document',
    body: { type: 'body', blocks: [{ type: 'paragraph', runs: [{ type: 'text', text: 'Schreiben vom DATUM ANSCHREIBEN' }] }] },
  };
  const report = materializeDocxTextReplacements(document, { 'DATUM ANSCHREIBEN': '15.07.2026' });

  assert.deepEqual(report, { replacedText: ['DATUM ANSCHREIBEN'], missingTextReplacements: [] });
  assert.equal(document.body.blocks[0].runs[0].text, 'Schreiben vom 15.07.2026');
});
