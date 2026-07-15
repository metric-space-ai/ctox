import assert from 'node:assert/strict';
import { CtoxResizer } from './resizer.js';

class ClassList {
  values = new Set();
  add(value) { this.values.add(value); }
  remove(value) { this.values.delete(value); }
  toggle(value, active) { active ? this.add(value) : this.remove(value); }
}

class ElementStub {
  constructor() {
    this.attributes = new Map();
    this.classList = new ClassList();
    this.listeners = new Map();
    this.style = {
      values: new Map(),
      setProperty: (key, value) => this.style.values.set(key, value),
      getPropertyValue: (key) => this.style.values.get(key) || '',
    };
  }
  addEventListener(type, listener) { this.listeners.set(type, listener); }
  removeEventListener(type) { this.listeners.delete(type); }
  hasAttribute(name) { return this.attributes.has(name); }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  getAttribute(name) { return this.attributes.get(name) || null; }
}

const body = new ElementStub();
globalThis.document = { body };
globalThis.window = {
  addEventListener() {},
  removeEventListener() {},
  getComputedStyle: (element) => element.style,
};
globalThis.requestAnimationFrame = (callback) => { callback(); return 1; };
globalThis.cancelAnimationFrame = () => {};

function fixture({ orientation, side, cssVar, initial }) {
  const handle = new ElementStub();
  const container = new ElementStub();
  container.style.setProperty(cssVar, `${initial}px`);
  container.querySelector = () => null;
  const resizer = new CtoxResizer({
    resizerEl: handle,
    containerEl: container,
    cssVar,
    orientation,
    side,
    minWidth: 100,
    maxWidth: 500,
  });
  return { handle, container, resizer };
}

const vertical = fixture({ orientation: 'vertical', side: 'left', cssVar: '--left', initial: 240 });
vertical.resizer.onKeyDown({ key: 'ArrowRight', preventDefault() {} });
assert.equal(vertical.container.style.getPropertyValue('--left'), '264px');
assert.equal(vertical.handle.getAttribute('aria-orientation'), 'vertical');

const horizontal = fixture({ orientation: 'horizontal', side: 'bottom', cssVar: '--bottom', initial: 220 });
horizontal.resizer.onKeyDown({ key: 'ArrowUp', preventDefault() {} });
assert.equal(horizontal.container.style.getPropertyValue('--bottom'), '244px');
assert.equal(horizontal.handle.getAttribute('aria-orientation'), 'horizontal');
horizontal.resizer.onPointerDown({ clientX: 0, clientY: 300, preventDefault() {} });
horizontal.resizer.onPointerMove({ clientX: 0, clientY: 260 });
assert.equal(horizontal.container.style.getPropertyValue('--bottom'), '284px');
horizontal.resizer.onPointerUp();
assert.equal(body.classList.values.has('is-resizing-horizontal'), false);

// Releasing the pointer before the scheduled animation frame must still
// commit the final drag position. Under a busy browser this is the common path.
let queuedFrame = null;
globalThis.requestAnimationFrame = (callback) => { queuedFrame = callback; return 2; };
const delayed = fixture({ orientation: 'vertical', side: 'left', cssVar: '--delayed', initial: 200 });
delayed.resizer.onPointerDown({ clientX: 100, clientY: 0, preventDefault() {} });
delayed.resizer.onPointerMove({ clientX: 148, clientY: 0 });
assert.equal(delayed.container.style.getPropertyValue('--delayed'), '200px');
delayed.resizer.onPointerUp();
assert.equal(delayed.container.style.getPropertyValue('--delayed'), '248px');
assert.equal(queuedFrame !== null, true);

vertical.resizer.destroy();
horizontal.resizer.destroy();
delayed.resizer.destroy();
console.log('Business OS resizer vertical/horizontal test OK');
