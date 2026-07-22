import assert from 'node:assert/strict';
import test from 'node:test';

import JSZip from '../../../vendor/jszip/jszip.mjs';
import {
  materializeDocxMergeFields,
  materializeDocxTextReplacements,
  mergeDocxFields,
} from '../../../vendor/document-format.mjs';

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

test('round-trips merged values through DOCX body, header, and footer parts', async () => {
  const input = await createMailMergeFixture();
  const output = await mergeDocxFields(input, {
    salu: 'Sehr geehrte Frau Beispiel',
    atem: 'kontakt@example.test',
  });
  const zip = await JSZip.loadAsync(output.bytes);
  const documentXml = await zip.file('word/document.xml').async('string');
  const headerXml = await zip.file('word/header1.xml').async('string');
  const footerXml = await zip.file('word/footer1.xml').async('string');

  assert.deepEqual(output.mergedFields, ['atem', 'salu']);
  assert.match(documentXml, /Sehr geehrte Frau Beispiel/);
  assert.match(headerXml, /kontakt@example\.test/);
  assert.match(footerXml, /kontakt@example\.test/);
  for (const xml of [documentXml, headerXml, footerXml]) {
    assert.doesNotMatch(xml, /MERGEFIELD|«[^»]+»/);
  }
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

async function createMailMergeFixture() {
  const zip = new JSZip();
  zip.file('[Content_Types].xml', `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>
  <Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>
</Types>`);
  zip.file('_rels/.rels', `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>`);
  zip.file('word/_rels/document.xml.rels', `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/>
</Relationships>`);
  zip.file('word/document.xml', wordDocumentXml(complexMergeField('salu')));
  zip.file('word/header1.xml', storyXml('hdr', complexMergeField('atem')));
  zip.file('word/footer1.xml', storyXml('ftr', '<w:r><w:t>«atem»</w:t></w:r>'));
  return zip.generateAsync({ type: 'uint8array', compression: 'DEFLATE' });
}

function wordDocumentXml(content) {
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <w:body><w:p>${content}</w:p><w:sectPr><w:headerReference w:type="default" r:id="rId1"/><w:footerReference w:type="default" r:id="rId2"/></w:sectPr></w:body>
</w:document>`;
}

function storyXml(kind, content) {
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:${kind} xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p>${content}</w:p></w:${kind}>`;
}

function complexMergeField(name) {
  return `<w:r><w:fldChar w:fldCharType="begin"/></w:r>
<w:r><w:instrText xml:space="preserve"> MERGEFIELD ${name} </w:instrText></w:r>
<w:r><w:fldChar w:fldCharType="separate"/></w:r>
<w:r><w:t>«${name}»</w:t></w:r>
<w:r><w:fldChar w:fldCharType="end"/></w:r>`;
}
