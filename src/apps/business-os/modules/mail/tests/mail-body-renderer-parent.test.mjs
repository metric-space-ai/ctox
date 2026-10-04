import test from 'node:test';
import assert from 'node:assert/strict';
import { mountMailBody, selectMessageBody, extractSafeUrl, __mailBodyTestHooks } from '../lib/mail-body-renderer.mjs';

function documentFixture(parsedBody) {
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
      if (tagName === 'template') node.content = parsedBody;
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

function parsedElement(tagName, attributes = {}, childNodes = []) {
  return {
    nodeType: 1, tagName, childNodes,
    attributes: Object.entries(attributes).map(([name, value]) => ({ name, value })),
    getAttribute(name) { return attributes[name] ?? null; },
  };
}

function parsedText(nodeValue) { return { nodeType: 3, nodeValue }; }

test('full text remains visible when HTML is unavailable or has no safe visible content', () => {
  const variants = [
    undefined,
    { childNodes: [
      parsedElement('head', {}, [parsedElement('title', {}, [parsedText('Mail metadata')])]),
      parsedElement('title', {}, [parsedText('Fragment metadata')]),
    ] },
    { childNodes: [
      parsedElement('script', {}, [parsedText('hidden script')]),
      parsedElement('style', {}, [parsedText('hidden CSS')]),
      parsedElement('p', {}, [parsedText('   ')]),
    ] },
  ];
  for (const parsedBody of variants) {
    const doc = documentFixture(parsedBody);
    const host = doc.createElement('div');
    const body = 'Hallo Michael,\n\nVollständige Antwort mit 🙂\n> Originalnachricht';
    const result = mountMailBody(host, { body_html: '<metadata-only>', body_text: body, preview: 'Short preview' });
    assert.equal(result.kind, 'text');
    assert.equal(result.value, body);
    assert.equal(host.dataset.mailBodyKind, 'text');
    const wrapper = host.childNodes[0];
    assert.equal(wrapper.className, 'mail-body-text');
    assert.deepEqual(wrapper.childNodes.filter(node => node.text).map(node => node.text),
      ['Hallo Michael,', 'Vollständige Antwort mit 🙂', '> Originalnachricht']);
    assert.equal(wrapper.childNodes.filter(node => node.tagName === 'br').length, 3);
  }
});

test('HTML copying preserves safe content and drops active subtrees, attributes and image sources', () => {
  // This fixture exercises copying after parsing. Real browser parser and
  // network behavior still require the tenant browser acceptance run.
  const parsedBody = { childNodes: [
    parsedElement('custom-layout', {}, [
      parsedElement('p', { style: 'background:url(https://tracker.test/)', onclick: 'attack()' }, [parsedText('Paragraph')]),
      parsedElement('script', {}, [parsedText('secret-script-content')]),
      parsedElement('a', { href: 'javascript:attack()' }, [
        parsedElement('script', {}, [parsedText('hidden-anchor-script')]),
        parsedText('Unsafe link'),
      ]),
      parsedElement('a', { href: 'https://accounts.google.com/', onclick: 'attack()' }, [parsedText('Google account')]),
      parsedElement('img', { src: 'https://tracker.test/pixel', srcset: 'https://tracker.test/pixel2', alt: 'Logo' }),
    ]),
  ] };
  const doc = documentFixture(parsedBody);
  const host = doc.createElement('div');
  mountMailBody(host, { body_html: '<fixture>' });
  const nodes = host.childNodes[0].childNodes;
  assert.deepEqual(nodes.filter(node => node.tagName).map(node => node.tagName), ['p', 'a', 'a']);
  assert.deepEqual(nodes[0].attributes, {});
  assert.deepEqual(nodes[1].attributes, {});
  assert.deepEqual(nodes[1].childNodes, [{ text: 'Unsafe link' }]);
  assert.deepEqual(nodes[2].attributes, {
    href: 'https://accounts.google.com/', rel: 'noopener noreferrer nofollow',
    target: '_blank', referrerpolicy: 'no-referrer',
  });
  assert.deepEqual(nodes[3], { text: '[Bild blockiert] Logo' });
});

test('deeply nested HTML cannot overflow the JavaScript call stack', () => {
  let body = parsedText('Deep message');
  for (let depth = 0; depth < 20000; depth++) body = parsedElement('custom-layout', {}, [body]);
  const doc = documentFixture({ childNodes: [body] });
  const host = doc.createElement('div');
  const result = mountMailBody(host, { body_html: '<nested-fixture>' });
  assert.equal(result.kind, 'html');
  assert.deepEqual(host.childNodes[0].childNodes, [{ text: 'Deep message' }]);
});
