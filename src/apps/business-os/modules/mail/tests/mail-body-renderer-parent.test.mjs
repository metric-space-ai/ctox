import test from 'node:test';
import assert from 'node:assert/strict';
import { mountMailBody, selectMessageBody, extractSafeUrl, __mailBodyTestHooks } from '../lib/mail-body-renderer.mjs';

function documentFixture() {
  const doc = {
    createElement(tagName) {
      const node = {
        tagName, ownerDocument: doc, childNodes: [], dataset: {}, attributes: {},
        classList: { add() {}, remove() {} },
        get firstChild() { return this.childNodes[0] || null; },
        appendChild(child) { this.childNodes.push(child); return child; },
        removeChild(child) { this.childNodes.splice(this.childNodes.indexOf(child), 1); },
        setAttribute(name, value) { this.attributes[name] = value; },
      };
      return node;
    },
    createTextNode(value) { return { text: value }; },
  };
  return doc;
}

test('plain mail with multiple links terminates and preserves quotes and line breaks', () => {
  // matchAll rejects a non-global scanner immediately. This guards the former
  // infinite exec loop without letting an old renderer hang the test process.
  const matches = [...'https://one.example https://two.example'.matchAll(__mailBodyTestHooks.URL_REGEX)];
  assert.equal(matches.length, 2);
  const doc = documentFixture();
  const host = doc.createElement('div');
  const result = mountMailBody(host, { body_text: 'Hello\n\n> Quote\nhttps://one.example https://two.example' });
  assert.equal(result.kind, 'text');
  const nodes = host.childNodes[0].childNodes;
  assert.equal(nodes.filter(node => node.tagName === 'br').length, 3);
  assert.ok(nodes.some(node => node.text === '> Quote'));
  const links = nodes.filter(node => node.tagName === 'a');
  assert.deepEqual(links.map(node => node.attributes.href), ['https://one.example', 'https://two.example']);
  assert.ok(links.every(node => node.attributes.rel.includes('noreferrer')));
});

test('full HTML alternative wins over its generated text preview', () => {
  const html = '<p>First paragraph</p><p>Second paragraph</p>';
  const selection = selectMessageBody({ body_html: html, body_text: 'Flattened alternative', preview: 'Short preview' });
  assert.equal(selection.kind, 'html');
  assert.equal(selection.value, html);
  assert.equal(selectMessageBody({ body_text: 'First\n\nSecond' }).value, 'First\n\nSecond');
});

test('unsafe link schemes and control characters cannot become clickable links', () => {
  for (const url of ['javascript:alert(1)', 'data:text/html,attack', 'file:///etc/passwd', '//tracking.example/', 'java\nscript:alert(1)', 'https://good.example\u0000bad']) {
    assert.equal(extractSafeUrl(url), null, url);
  }
  assert.equal(extractSafeUrl('https://account.google.com/'), 'https://account.google.com/');
  assert.equal(extractSafeUrl('mailto:team@example.test'), 'mailto:team@example.test');
});
