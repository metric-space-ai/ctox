import assert from 'node:assert/strict';
import test from 'node:test';

function makeElement(tagName) {
  const children = [];
  return {
    tagName,
    children,
    className: '',
    dataset: {},
    style: {},
    attributes: new Map(),
    append(...nodes) { children.push(...nodes); },
    appendChild(node) { children.push(node); return node; },
    addEventListener() {},
    classList: { add() {}, remove() {} },
    querySelector() { return null; },
    setAttribute(name, value) { this.attributes.set(name, value); },
    getBoundingClientRect() { return { left: 0, top: 0, width: 44, height: 44 }; },
  };
}

test('Business reporter keeps desktop idle free of RAF animation timers', async () => {
  const previousDocument = globalThis.document;
  const previousWindow = globalThis.window;
  const previousDesktopBridge = globalThis.ctoxBusinessOsDesktop;
  const previousSetTimeout = globalThis.setTimeout;
  const previousClearTimeout = globalThis.clearTimeout;
  const previousRequestAnimationFrame = globalThis.requestAnimationFrame;
  try {
    let timeoutCount = 0;
    let rafCount = 0;
    const documentStub = {
      body: makeElement('body'),
      head: makeElement('head'),
      documentElement: { lang: 'de' },
      getElementById() { return null; },
      querySelector() { return null; },
      createElement: makeElement,
    };
    globalThis.document = documentStub;
    globalThis.window = {
      innerWidth: 1440,
      innerHeight: 900,
      addEventListener() {
        throw new Error('desktop idle animation must not install activity listeners');
      },
    };
    globalThis.ctoxBusinessOsDesktop = { openSwitcher() {} };
    globalThis.setTimeout = () => { timeoutCount += 1; return 1; };
    globalThis.clearTimeout = () => {};
    globalThis.requestAnimationFrame = () => { rafCount += 1; return 1; };

    const { initBusinessReporter } = await import(`./business-reporter.js?test=${Date.now()}`);
    initBusinessReporter({
      session: { authenticated: true },
      getActiveModule: () => ({ id: 'ctox', title: 'CTOX' }),
      commandBus: {},
    });

    assert.equal(documentStub.body.children.length, 1);
    assert.equal(timeoutCount, 0);
    assert.equal(rafCount, 0);
  } finally {
    globalThis.document = previousDocument;
    globalThis.window = previousWindow;
    globalThis.ctoxBusinessOsDesktop = previousDesktopBridge;
    globalThis.setTimeout = previousSetTimeout;
    globalThis.clearTimeout = previousClearTimeout;
    globalThis.requestAnimationFrame = previousRequestAnimationFrame;
  }
});
